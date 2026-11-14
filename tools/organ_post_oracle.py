#!/usr/bin/env python3
"""Run the PNO Core1 post stage (0x12272D) and output stage (0x1229B3) in the SHARC oracle.

Harness: `sharc-chain` (crates/nord-sharc/src/bin/sharc_chain.rs). Models: `organ_post_ref.py`.
Method and results: docs/research/organ-post-stage.md.

    python tools/organ_post_oracle.py verify SRAM/core1_sram.bin --exe target/release/sharc-chain
    python tools/organ_post_oracle.py render SRAM/core1_sram.bin --exe target/release/sharc-chain \
        --notes 60,64,67 --depth 0x240B700 --frames 65536 --out chain_vib
    python tools/organ_post_oracle.py report chain_vib.f32.npy --notes 60,64,67
"""

from __future__ import annotations

import argparse
import os
import subprocess
import sys
import tempfile
from pathlib import Path

import numpy as np
import organ_oracle_audio as oa
import organ_post_ref as pr

DM_RANGES = ((0xE0000, 0x3000), (0xB0000, 0x2000))
POST_STEP = "0x12272D:0x1229B2"
OUT_STEP = "0x1229B3:0x122A70"
BANK_STEP = "0x122620:0x12268C"
MIX_STEP = "0x12268F:0x12272A"
MIX_OUT, FINAL_OUT = 0xB0220, 0xB0200
WHEEL0_MIDI = 24  # ASSUMED: note oscillator 0 = C1, like wheel 0 of the tonewheel bank
SAVED_I0 = (0xE210E, 0xE210F)  # the post stage saves I0 here (not modelled)


def run_chain(
    exe: str,
    image: str,
    pokes: dict[int, int],
    steps: list[str],
    ranges: tuple[tuple[int, int], ...],
    calls: int = 1,
) -> np.ndarray:
    """Run `sharc-chain` and return the captured windows (one row of words per call)."""
    with tempfile.TemporaryDirectory() as tmp:
        script, out = Path(tmp) / "s.txt", Path(tmp) / "o.bin"
        script.write_text("\n".join(f"0 {a:#x} {v:#x}" for a, v in pokes.items()), encoding="utf-8")
        cmd = [os.path.abspath(exe), image, "--calls", str(calls), "--script", str(script)]
        for s in steps:
            cmd += ["--step", s]
        for a, n in ranges:
            cmd += ["--range", f"{a:#x}:{n}"]
        subprocess.run([*cmd, "--out", str(out)], check=True, capture_output=True)
        words = np.fromfile(out, dtype="<u4")
    return words.reshape(calls, -1)


def snapshot(words: np.ndarray, ranges: tuple[tuple[int, int], ...]) -> pr.Mem:
    """A `Mem` from one row of captured windows."""
    m, pos = pr.Mem(), 0
    for base, n in ranges:
        for i in range(n):
            m[base + i] = int(words[pos + i])
        pos += n
    return m


def compare(model: pr.Mem, oracle: pr.Mem) -> list[int]:
    """Addresses of the captured windows where model and oracle differ."""
    return [
        a
        for base, n in DM_RANGES
        for a in range(base, base + n)
        if model[a] != oracle[a] and a not in SAVED_I0
    ]


def verify_scenario(exe: str, image: str, seed: int, stage: str) -> list[int]:
    """Differences between the Python model and the oracle on one random scenario."""
    if stage == "post":
        pokes, steps, ret = pr.synthetic_inputs(seed), [POST_STEP], None
    else:
        pokes = pr.output_inputs(seed)
        steps = [f"{OUT_STEP}:I0={FINAL_OUT:#x},I1={pr.OUT_RECORD:#x}"]
        ret = (FINAL_OUT, pr.OUT_RECORD)
    pre = snapshot(run_chain(exe, image, pokes, [], DM_RANGES)[0], DM_RANGES)
    want = snapshot(run_chain(exe, image, pokes, steps, DM_RANGES)[0], DM_RANGES)
    pre.update(pokes)  # words outside the captured windows (0x95xxx tables)
    if ret:
        pr.output_stage(pre, *ret)
    else:
        pr.post_stage(pre)
    return compare(pre, want)


def cmd_verify(a: argparse.Namespace) -> int:
    bad = 0
    for stage in ("post", "output"):
        for seed in range(a.seeds):
            diffs = verify_scenario(a.exe, a.image, seed, stage)
            print(f"{stage} stage seed {seed}: {len(diffs)} differing words {diffs[:4]}")
            bad += bool(diffs)
    print("OK: bit-exact" if not bad else f"FAILED: {bad} scenarios differ")
    return 1 if bad else 0


def chain_pokes(notes: list[int], depth: int, fine: int = 0x40000000) -> dict[int, int]:
    """Pokes of a whole pass: tonewheel bank + mix (chord weights) and post + output stages.

    The output-stage record is ASSUMED (the ARM builds it): identity sections (b0 = 1), gains
    1 and 0, final gain 1 / 0.090075, weight 1 on the oscillator of each note (note - 24).
    """
    pokes = {
        a: v
        for _, a, v in oa.build_pokes(
            oa.chord_weights(notes, oa.drawbar_digits("888000000")), False
        )
    }
    pokes[pr.DEPTH], pokes[pr.OFFSET] = depth, fine
    for i in range(pr.NOTES + 12):
        pokes[pr.OUT_WEIGHTS + i] = 0
    for n in notes:
        pokes[pr.OUT_WEIGHTS + n - WHEEL0_MIDI] = oa.f2u(1.0)
    g = pr.OUT_PARAMS
    pokes[pr.OUT_RECORD - 0x1F], pokes[pr.OUT_RECORD - 0x1E] = pr.OUT_WEIGHTS, g
    pokes.update({g: oa.ONE, g + 1: 0, g + 4: oa.ONE, g + 5: oa.ONE, g + 6: 0, g + 7: 0})
    pokes[g + 2], pokes[g + 3], pokes[g + 8] = pr.OUT_STATES[1], pr.OUT_STATES[2], pr.OUT_STATES[0]
    pokes[g + 0xB] = oa.f2u(1 / 0.090075)
    for s in pr.OUT_STATES:
        pokes.update({s + o: 0 for o in range(8)})
        pokes[s + 6] = oa.ONE
    return pokes


def cmd_render(a: argparse.Namespace) -> int:
    notes = [int(n) for n in a.notes.split(",")]
    pokes = chain_pokes(notes, int(a.depth, 0))
    steps = [
        BANK_STEP,
        f"{MIX_STEP}:I1=0xB473C,I0={MIX_OUT:#x}",
        POST_STEP,
        f"{OUT_STEP}:I0={FINAL_OUT:#x},I1={pr.OUT_RECORD:#x}",
    ]
    calls = a.frames // 2
    rows = run_chain(a.exe, a.image, pokes, steps, ((MIX_OUT, 4), (FINAL_OUT, 4)), calls)
    f = rows.astype("<u4").view("<f4")
    mix, final = f[:, :4].reshape(-1, 2), f[:, 4:].reshape(-1, 2)
    np.save(f"{a.out}.f32.npy", final)
    np.save(f"{a.out}.mix.f32.npy", mix)
    oa.write_wav_f32(Path(f"{a.out}.wav"), final / max(1e-9, float(np.abs(final).max())), a.rate)
    print(
        f"final lanes peak {np.abs(final).max():.4f}, rms {final.std():.4f}; mix lanes peak "
        f"{np.abs(mix).max():.3f}"
    )
    return 0


def osc_cycles(index: int) -> float:
    """Cycles per frame of note oscillator `index` at mod = 0: rate / 4 (m = 1/2)."""
    return pr.RATE_BASE * 2 ** (index / 12) / 2**31 / 4


def cmd_report(a: argparse.Namespace) -> int:
    out = np.load(a.npy)[a.skip :, 0].astype(np.float64)
    notes = [int(n) for n in a.notes.split(",")]
    lfo = pr.LFO_STEP / 2**32
    print(f"LFO {lfo:.6f} cycles/frame; frames analysed {len(out)}")
    print("note  cycles/frame  carrier  -1st sideband  +1st sideband  (relative to carrier)")
    for n in notes:
        c = osc_cycles(n - WHEEL0_MIDI)
        amp = oa.amplitude(out, c)
        lo, hi = oa.amplitude(out, c - lfo), oa.amplitude(out, c + lfo)
        print(f"{n:4d}  {c:.6f}  {amp:.4f}  {lo / max(amp, 1e-12):.4f}  {hi / max(amp, 1e-12):.4f}")
    return 0


def main(argv: list[str] | None = None) -> int:
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    sub = p.add_subparsers(dest="cmd", required=True)
    v = sub.add_parser("verify")
    v.add_argument("image")
    v.add_argument("--exe", required=True)
    v.add_argument("--seeds", type=int, default=4)
    v.set_defaults(fn=cmd_verify)
    r = sub.add_parser("render")
    r.add_argument("image")
    r.add_argument("--exe", required=True)
    r.add_argument("--notes", default="60,64,67")
    r.add_argument("--depth", default="0")
    r.add_argument("--frames", type=int, default=65536)
    r.add_argument("--rate", type=int, default=48000)
    r.add_argument("--out", required=True)
    r.set_defaults(fn=cmd_render)
    q = sub.add_parser("report")
    q.add_argument("npy")
    q.add_argument("--notes", default="60,64,67")
    q.add_argument("--skip", type=int, default=4096)
    q.set_defaults(fn=cmd_report)
    args = p.parse_args(argv)
    return args.fn(args)


if __name__ == "__main__":
    sys.exit(main())
