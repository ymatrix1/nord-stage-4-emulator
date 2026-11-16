#!/usr/bin/env python3
"""Train a LoKr or LoRA adapter on Transkun 2.0 from a ``tools/synth_dataset.py`` dataset.

    C:/tmp/ns4-ft-venv/Scripts/python.exe tools/synth_train.py C:/tmp/ns4-ft-data \
        --arm lokr --steps 8000 --out C:/tmp/ns4-ft-runs/lokr1

Plan and targets: ``docs/research/transkun-synth-finetune-plan.md`` (sections 7.2 and 7.3).
Loss, as upstream ``transkun.train``: ``-log_prob(audio, notes).sum(-1).mean()`` (semi-CRF
intervals + velocity + refined onset/offset + presence). The optimizer is AdamW with a OneCycle
schedule (5 % warm-up, ``div_factor`` 20, ``final_div_factor`` 2, as upstream); gradient norms
are clipped at the 0.8 quantile of the last 100 norms, as upstream's adaptive clipping.
ASSUMED: AdamW instead of upstream's AdaBelief (same state size), weight decay 1e-4 as upstream.
``--schedule wsd`` (``synth_train_sched.py``) keeps the rate constant and cools down only at the
end, so a run can be extended: ``--resume <snapshot> --steps <step + N> --cooldown N``.

Every ``--nll-every`` steps: validation NLL on fixed held-out segments. Every ``--eval-every``
steps: NLL plus onset F1 / velocity MAE on ``--eval-items`` held-out items (``synth_eval``) and
the checkpoints ``last`` and ``best`` (by NLL); every ``--snapshot-every`` steps also ``step_N``.
Output, never committed: adapter folders, ``log.csv``, ``evals.jsonl``.
"""

from __future__ import annotations

import argparse
import collections
import csv
import json
import sys
import time
from collections.abc import Sequence
from pathlib import Path
from typing import Any

import numpy as np
import synth_adapter
import synth_eval
import synth_train_data as std
import synth_train_sched as sched_mod
import synth_train_state as sts

GRAD_WINDOW = 100
CLIP_QUANTILE = 0.8


def to_notes(batch: list[list[dict[str, Any]]]) -> list[list[Any]]:
    from transkun.Data import Note

    return [
        [
            Note(n["start"], n["end"], n["pitch"], n["velocity"], n["has_onset"], n["has_offset"])
            for n in notes
        ]
        for notes in batch
    ]


def clip_value(history: collections.deque) -> float:
    """Adaptive gradient-clip threshold: quantile of recent norms, infinite while warming up."""
    if len(history) < 10:
        return float("inf")
    return float(np.quantile(np.array(history), CLIP_QUANTILE))


def nll(model: Any, audio: np.ndarray, notes: list[list[dict[str, Any]]], device: str) -> Any:
    import torch

    x = torch.from_numpy(np.ascontiguousarray(audio)).to(device)
    return -model.log_prob(x, to_notes(notes)).sum(-1).mean()


def validation_nll(model: Any, segments: list, device: str) -> float:
    import torch

    model.eval()
    total = 0.0
    with torch.no_grad():
        for audio, notes in segments:
            total += float(nll(model, audio[None, :, None], [notes], device))
    model.train()
    return total / max(1, len(segments))


def val_scores(model: Any, root: Path, entries: list, device: str, tol: float) -> dict[str, Any]:
    import torch

    model.eval()
    est = {}
    with torch.no_grad():
        for e in entries:
            est[e["id"]] = synth_eval.transcribe_file(model, root / e["audio"], device)
    model.train()
    return synth_eval.score(entries, est, tol)["all"]


def log_record(ctx: dict, rec: dict) -> None:
    with (ctx["out"] / "evals.jsonl").open("a", encoding="utf-8") as f:
        f.write(json.dumps(rec) + chr(10))
    print("EVAL", rec, flush=True)


def quick_nll(step: int, model: Any, ctx: dict) -> None:
    """Validation NLL only (about 15 s), logged as ``kind: nll``."""
    v = validation_nll(model, ctx["val_segments"], ctx["device"])
    log_record(
        ctx,
        {
            "step": step,
            "kind": "nll",
            "val_nll": round(v, 2),
            "seconds": round(time.time() - ctx["t0"]),
        },
    )


def run_eval(
    step: int, model: Any, peft_model: Any, ctx: dict, best: float, snapshot: bool = False
) -> float:
    """Validation NLL + onset F1 / velocity MAE, checkpoint ``last`` and ``best``; returns best.

    ``snapshot`` also keeps the adapter as ``step_N`` (4 MB), so any step can be scored later.
    """
    v = validation_nll(model, ctx["val_segments"], ctx["device"])
    scores = val_scores(model, ctx["root"], ctx["f1_entries"], ctx["device"], 0.05)
    rec = {
        "step": step,
        "val_nll": round(v, 2),
        "seconds": round(time.time() - ctx["t0"]),
        **scores,
    }
    log_record(ctx, {**rec, "kind": "full"})
    new_best = min(best, v)
    folders = [ctx["out"] / "last"] + ([ctx["out"] / f"step_{step}"] if snapshot else [])
    for folder in folders:
        synth_adapter.save_adapter(peft_model, folder)
        sts.save_state(folder, step=step, best=new_best, **ctx["state_fn"]())
    if v < best:
        synth_adapter.save_adapter(peft_model, ctx["out"] / "best")
    return new_best


def make_schedule(opt: Any, args: argparse.Namespace) -> Any:
    """OneCycle (fixed length, the plan's default) or the extendable WSD schedule."""
    if args.schedule == "wsd":
        return sched_mod.WsdSchedule(opt, args.lr, args.steps, args.warmup, args.cooldown)
    import torch

    return torch.optim.lr_scheduler.OneCycleLR(
        opt,
        args.lr,
        total_steps=args.steps,
        pct_start=0.05,
        div_factor=20,
        final_div_factor=2,
        cycle_momentum=False,
    )


def train(args: argparse.Namespace) -> None:
    import torch

    torch.manual_seed(args.seed)
    device = "cuda"
    root, out = Path(args.root), Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    model = synth_adapter.load_base(device)
    peft_model = synth_adapter.attach(model, args.arm, args.r, args.factor)
    n_train = synth_adapter.count_trainable(peft_model)
    print(f"arm={args.arm} trainable parameters: {n_train}", flush=True)
    train_s = std.SegmentSampler(root, std.load_entries(root, "train"), args.seg, args.seed)
    val_entries = std.load_entries(root, "val")
    val_segments = std.SegmentSampler(root, val_entries, args.seg, 0).fixed(args.val_segments)
    f1_entries = val_entries[:: max(1, len(val_entries) // args.eval_items)][: args.eval_items]
    params = [p for p in peft_model.parameters() if p.requires_grad]
    opt = torch.optim.AdamW(params, lr=args.lr, weight_decay=1e-4)
    sched = make_schedule(opt, args)
    history: collections.deque = collections.deque(maxlen=GRAD_WINDOW)
    start, best, elapsed = 0, float("inf"), 0.0
    if args.resume:
        src = Path(args.resume)
        synth_adapter.restore_adapter(peft_model, src)
        state = sts.load_state(src)
        start, best, elapsed = sts.apply_state(
            state,
            total_steps=args.steps,
            opt=opt,
            sched=sched,
            history=history,
            sampler_rng=train_s.rng,
            schedule=args.schedule,
        )
        print(f"resumed from {src} at step {start}", flush=True)
    t0 = time.time() - elapsed
    log_path = out / "log.csv"
    fresh = not log_path.exists() or log_path.stat().st_size == 0
    logf = log_path.open("a" if args.resume else "w", newline="")
    log = csv.writer(logf)
    if fresh or not args.resume:
        log.writerow(["step", "loss", "grad_norm", "clip", "lr", "seconds"])
    ctx = {
        "root": root,
        "out": out,
        "val_segments": val_segments,
        "f1_entries": f1_entries,
        "device": device,
        "t0": t0,
        "state_fn": lambda: {
            "total_steps": args.steps,
            "schedule": args.schedule,
            "opt": opt,
            "sched": sched,
            "history": history,
            "elapsed": time.time() - t0,
            "sampler_rng": train_s.rng,
        },
    }
    if not args.resume:
        best = run_eval(0, model, peft_model, ctx, best)  # baseline of the untrained adapter
    model.train()
    for step in range(start + 1, args.steps + 1):
        audio, notes = train_s.batch(args.bs)
        opt.zero_grad(set_to_none=True)
        with torch.autocast("cuda", dtype=torch.bfloat16, enabled=args.bf16):
            loss = nll(model, audio, notes, device)
        loss.backward()
        clip = clip_value(history)
        gnorm = float(torch.nn.utils.clip_grad_norm_(params, clip))
        history.append(gnorm)
        opt.step()
        sched.step()
        if not np.isfinite(float(loss)):
            raise RuntimeError(f"non-finite loss at step {step}")
        if step % args.log_every == 0 or step == 1:
            row = [
                step,
                round(float(loss), 2),
                round(gnorm, 2),
                round(clip, 2),
                f"{sched.get_last_lr()[0]:.3g}",
                round(time.time() - t0),
            ]
            log.writerow(row)
            logf.flush()
            print(*row, flush=True)
        if step % args.nll_every == 0 and step % args.eval_every != 0:
            quick_nll(step, model, ctx)
            model.train()
        if step % args.eval_every == 0 or step == args.steps:
            snap = step % args.snapshot_every == 0 or step == args.steps
            best = run_eval(step, model, peft_model, ctx, best, snap)


def build_parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    p.add_argument("root", help="Dataset folder (dataset.json, audio/).")
    p.add_argument("--arm", choices=synth_adapter.ARMS, default="lokr")
    p.add_argument("--r", type=int)
    p.add_argument("--factor", type=int, help="LoKr decompose_factor (default 4).")
    p.add_argument("--steps", type=int, default=8000)
    p.add_argument("--bs", type=int, default=4)
    p.add_argument("--seg", type=float, default=std.SEG_S)
    p.add_argument("--lr", type=float, default=2e-4, help="Peak rate.")
    p.add_argument(
        "--schedule",
        choices=("onecycle", "wsd"),
        default="onecycle",
        help="onecycle: fixed length. wsd: warm-up, constant, cool-down; can be extended.",
    )
    p.add_argument("--warmup", type=int, default=150, help="wsd warm-up steps.")
    p.add_argument(
        "--cooldown", type=int, default=0, help="wsd: decay over the last N steps (0 = never)."
    )
    p.add_argument("--seed", type=int, default=1)
    p.add_argument("--bf16", action="store_true")
    p.add_argument("--log-every", type=int, default=20)
    p.add_argument("--nll-every", type=int, default=250, help="Quick validation NLL.")
    p.add_argument("--eval-every", type=int, default=500, help="Full eval with F1 / velocity.")
    p.add_argument("--snapshot-every", type=int, default=500, help="Keep step_N adapters.")
    p.add_argument("--eval-items", type=int, default=12, help="Held-out items for F1 per eval.")
    p.add_argument("--val-segments", type=int, default=24, help="Held-out segments for NLL.")
    p.add_argument("--resume", help="Adapter folder with train_state.pt (last or step_N).")
    p.add_argument("--out", required=True, help="Run folder (outside the repo).")
    return p


def main(argv: Sequence[str] | None = None) -> int:
    train(build_parser().parse_args(argv))
    return 0


if __name__ == "__main__":
    sys.exit(main())
