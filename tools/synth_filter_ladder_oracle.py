#!/usr/bin/env python3
"""Runs the Core2 filter stage `0x12549D` (Ladder24) in `sharc-org` and compares it with the model.

The stage is the B stage of channel 0 (`tools/synth_stage_oracle.py`: frame `I3 = 0xC2942`, the
three frame rewriting control stores patched out). Per trial a random frame is poked (cutoff
position `-30`, resonance word `-28`, the coefficient words of the previous call `-18..-12`, the
states `-10..2`), three passes of four samples are run and compared word for word: the output
samples, the states and the coefficient words stored for the next call, both lanes.

    python tools/synth_filter_ladder_oracle.py --image SRAM_ORG/core2_sram.bin

    python tools/synth_filter_ladder_oracle.py --image ... --record cases.json   # recorded cases

Needs the local ORG core 2 image and `cargo build --release -p nord-sharc`. Result and law:
`docs/research/synth-stage-models-9.md`.
"""

from __future__ import annotations

import argparse
import json
import random
import sys
import tempfile
from pathlib import Path

if __package__ in (None, ""):
    sys.path.insert(0, str(Path(__file__).resolve().parent))
import synth_filter_ladder as lad
import synth_stage_oracle as o

STATE_OFFS = (-10, -8, -6, -4, -2, 0, 2)
COEF_OFFS = (-18, -16, -14, -12)
PASSES = 3


def random_case(seed: int) -> dict:
    """Random frame words and input samples of one trial (own generator per seed)."""
    rng = random.Random(seed)
    amp = rng.choice([0.3, 1.0, 4.0])
    f2u = o.f2u
    return {
        "w30": [f2u(rng.uniform(0.0, 0.995)), f2u(rng.uniform(0.0, 1.0))],
        "w28": [(rng.randrange(0, 127) << 24) | rng.randrange(0, 1 << 24) for _ in range(2)],
        "coefs": [
            [
                f2u(rng.uniform(-0.2, 0.9)),
                f2u(rng.uniform(0, 0.4)),
                f2u(rng.uniform(-1, 0)),
                f2u(rng.uniform(0.0, 3)),
            ]
            for _ in range(2)
        ],
        "states": [[f2u(rng.uniform(-amp, amp)) for _ in STATE_OFFS] for _ in range(2)],
        "xs": [[f2u(rng.uniform(-amp, amp)) for _ in range(4 * PASSES)] for _ in range(2)],
    }


def model_run(case: dict) -> dict:
    """Model result for a case: outputs, final states and final coefficient words per lane."""
    out = []
    for lane in (0, 1):
        ln = lad.Lane()
        ln.sect = list(case["states"][lane][:4])
        ln.y, ln.t, ln.fb = case["states"][lane][4:7]
        cf = tuple(case["coefs"][lane])
        words: list[int] = []
        for p in range(PASSES):
            words += lad.process(ln, cf, case["xs"][lane][4 * p : 4 * p + 4])
            # the cutoff position of lane x serves both lanes
            cf = lad.coefficients(case["w30"][0], case["w28"][lane])
        out.append(
            {"out": words, "states": [*ln.sect, ln.y, ln.t, ln.fb], "coefs": list(cf)},
        )
    return {"lanes": out}


def oracle_run(sharc_org: str, image: str, case: dict) -> dict:  # pragma: no cover - needs firmware
    """Oracle result of a case, same shape as `model_run`."""
    patched = Path(tempfile.gettempdir(), "core2_ladder_patched.bin")
    patched.write_bytes(o.patch_controls(Path(image).read_bytes()))
    frame = {-30: tuple(case["w30"]), -28: tuple(case["w28"])}
    for k, off in enumerate(COEF_OFFS):
        frame[off] = (case["coefs"][0][k], case["coefs"][1][k])
    for k, off in enumerate(STATE_OFFS):
        frame[off] = (case["states"][0][k], case["states"][1][k])
    xs = [o.u2f(w) for w in case["xs"][0]]
    ys = [o.u2f(w) for w in case["xs"][1]]
    res = o.run_stage(sharc_org, str(patched), lad.STAGE, frame, xs, ys, watch_frame=(-32, 64))
    fr = res["frames"][-1]
    lanes = []
    for lane, series in ((0, res["x"]), (1, res["y"])):
        lanes.append(
            {
                "out": [o.f2u(v) for v in series],
                "states": [fr[off][lane] for off in STATE_OFFS],
                "coefs": [fr[off][lane] for off in COEF_OFFS],
            }
        )
    return {"lanes": lanes}


def compare(sharc_org: str, image: str, trials: int) -> tuple[int, int]:  # pragma: no cover
    """`(equal, total)` words of model against oracle."""
    equal = total = 0
    for seed in range(1, trials + 1):
        case = random_case(seed)
        want, got = model_run(case), oracle_run(sharc_org, image, case)
        for lane in (0, 1):
            for key in ("out", "states", "coefs"):
                for a, b in zip(want["lanes"][lane][key], got["lanes"][lane][key], strict=True):
                    total += 1
                    equal += a == b
    return equal, total


def record(sharc_org: str, image: str, seeds: list[int], path: Path) -> None:  # pragma: no cover
    """Writes recorded oracle cases (inputs and oracle words) for the firmware-free tests."""
    cases = []
    for seed in seeds:
        case = random_case(seed)
        case["oracle"] = oracle_run(sharc_org, image, case)
        cases.append(case)
    path.write_text(json.dumps({"stage": "0x12549d", "cases": cases}) + "\n", encoding="utf-8")


def main(argv: list[str] | None = None) -> int:  # pragma: no cover - thin CLI
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--image", required=True)
    ap.add_argument("--sharc-org", default="target/release/sharc-org")
    ap.add_argument("--trials", type=int, default=40)
    ap.add_argument("--record", type=Path, help="write recorded cases (seeds 1..6) to this file")
    ns = ap.parse_args(argv)
    if ns.record:
        record(ns.sharc_org, ns.image, [1, 2, 3, 4, 5, 6], ns.record)
        return 0
    equal, total = compare(ns.sharc_org, ns.image, ns.trials)
    print(f"{equal} of {total} words equal")
    return 0 if equal == total else 1


if __name__ == "__main__":
    sys.exit(main())
