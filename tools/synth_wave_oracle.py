#!/usr/bin/env python3
"""Runs the Core2 oscillator stage `0x1212D3` in `sharc-org` and compares it with the model.

The stage is installed as the stage-A routine of slot 0 (`DM(0xC10B8)`); the pass calls it for
channel 0 with `I2 = 0xC1F96`, `I0 = 0xC1E02`, `I4 = 0xC1E8A` (scratch) and the parameter block
`P = DM(0xC1F76) = 0xC169A`. The pass rewrites the pitch word (`DM(-29, I2)`) and the shape words
(`DM(17, I2)`, `DM(19, I2)`) before the stage runs, so they are read back after the pass and fed to
the model; everything else is poked before pass 0 (phases, `P` ratios, weights, output gain).

    python tools/synth_wave_oracle.py --image SRAM_ORG/core2_sram.bin

prints the number of output words that differ over random trials (320 of 320 equal in the recorded
run, `docs/research/synth-wave-models.md`). Needs the local ORG core 2 image.
"""

from __future__ import annotations

import argparse
import csv
import random
import subprocess
import sys
import tempfile
from pathlib import Path

if __package__ in (None, ""):
    sys.path.insert(0, str(Path(__file__).resolve().parent))
import synth_wave_model as m

# VERIFIED (`docs/research/org-stage-map.md` s2-s3, listing of the pass): stage-A call frame.
A_ENTRY = 0xC10B8
RECORD = 0xC1F96  # I2 of channel 0
PARAM = 0xC169A  # P = DM(-32, I2)
STAGE = 0x1212D3
STAGE_SAW = 0x1215FB
STAGE_PULSE = 0x121B99
PULSE_WORD = 0xC15AE  # DM(-8, I1) of channel 0, read by both lanes (I1 is not duplicated in SIMD)
DEVS = ("0x282c0380=1", "0x3108d008=5")


def fbits(x: float) -> int:
    return m.f2u(x)


MODELS = {STAGE: m.stage, STAGE_SAW: m.stage_saw, STAGE_PULSE: m.stage_pulse}


def pokes(lanes: list[m.Lane]) -> dict[int, int]:
    """DM address -> word for the inputs that survive the pass (lane y one word above lane x)."""
    d: dict[int, int] = {}
    for i, ln in enumerate(lanes):
        d[RECORD - 21 + i] = ln.phase1
        d[RECORD - 19 + i] = ln.phase2
        d[PARAM - 32 + i] = ln.ratio1
        d[PARAM - 30 + i] = ln.ratio2
        for off, w in zip((-14, -16, -12, -10), ln.weights, strict=True):
            d[PARAM + off + i] = w
        d[PARAM + 16 + i] = ln.out_gain
        d[PULSE_WORD] = lanes[0].pulse
    return d


def random_lanes(rng: random.Random) -> list[m.Lane]:
    """Two lanes with random phases, ratios (2^-2 .. 2^5.3), weights, gain and pulse word."""
    lanes = [
        m.Lane(
            pitch=0,
            phase1=rng.getrandbits(32),
            phase2=rng.getrandbits(32),
            shape_a=0,
            shape_b=0,
            ratio1=fbits(2 ** rng.uniform(-2, 5.3)),
            ratio2=fbits(2 ** rng.uniform(-2, 5.3)),
            weights=tuple(fbits(rng.uniform(-0.5, 0.5)) for _ in range(4)),
            out_gain=fbits(rng.uniform(0.5, 1.5)),
            pulse=0,
        )
        for _ in range(2)
    ]
    pulse = rng.getrandbits(32)
    for ln in lanes:
        ln.pulse = pulse
    return lanes


def run_pass(
    sharc_org: str, image: str, lanes: list[m.Lane], stage: int = STAGE
) -> dict[int, int]:  # pragma: no cover
    """One pass in the oracle; returns every DM word of the frame region after the pass."""
    lines = [f"0 {A_ENTRY:#x} {stage:#x}"] + [f"0 {a:#x} {v:#x}" for a, v in pokes(lanes).items()]
    with tempfile.TemporaryDirectory() as tmp:
        sp, cp = Path(tmp, "m.txt"), Path(tmp, "o.csv")
        sp.write_text("\n".join(lines) + "\n")
        cmd = [sharc_org, image]
        for dev in DEVS:
            cmd += ["--dev", dev]
        cmd += ["--passes", "1", "--msg-base", "0", "--msg-script", str(sp)]
        cmd += ["--buf", f"{RECORD - 38:#x}:64:f32", "--buf", f"{PULSE_WORD - 8:#x}:24:f32"]
        cmd += ["--csv", str(cp)]
        r = subprocess.run(cmd, capture_output=True, text=True, check=False)
        if "fault" in r.stdout or not cp.exists():
            raise RuntimeError(r.stdout[-800:])
        rows = list(csv.DictReader(cp.open()))
    return {int(row["address"], 16): int(row["raw"], 16) for row in rows}


def compare(
    sharc_org: str, image: str, trials: int, stage: int = STAGE
) -> tuple[int, int]:  # pragma: no cover
    """`(equal words, total words)` of model against oracle over random trials."""
    equal = total = 0
    for seed in range(1, trials + 1):
        lanes = random_lanes(random.Random(seed))
        got = run_pass(sharc_org, image, lanes, stage)
        for i, ln in enumerate(lanes):
            ln.pitch = got[RECORD - 29 + i]
            ln.shape_a = got[RECORD + 17 + i]
            ln.shape_b = got[RECORD + 19 + i]
        model = MODELS[stage]
        want, _, _ = model(lanes)
        for i in (0, 1):
            for k in range(4):
                total += 1
                equal += want[i][k] == got[RECORD - 13 + 2 * k + i]
    return equal, total


def main(argv: list[str] | None = None) -> int:  # pragma: no cover - thin CLI
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--image", required=True)
    ap.add_argument("--sharc-org", default="target/release/sharc-org")
    ap.add_argument("--trials", type=int, default=40)
    ap.add_argument(
        "--stage", type=lambda v: int(v, 0), default=STAGE, help="0x1212D3, 0x1215FB or 0x121B99"
    )
    ns = ap.parse_args(argv)
    equal, total = compare(ns.sharc_org, ns.image, ns.trials, ns.stage)
    print(f"{equal} of {total} output words equal")
    return 0 if equal == total else 1


if __name__ == "__main__":
    sys.exit(main())
