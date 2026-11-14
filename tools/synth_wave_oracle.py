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
import synth_stage_noise as noise
import synth_stage_shape_sine as shape_sine
import synth_stage_shape_sine2 as shape_sine2
import synth_stage_shape_sine3 as shape_sine3
import synth_stage_shapes as shapes
import synth_stage_super1 as super1
import synth_stage_unison as unison
import synth_wave_model as m

# VERIFIED (`docs/research/org-stage-map.md` s2-s3, listing of the pass): stage-A call frame.
A_ENTRY = 0xC10B8
RECORD = 0xC1F96  # I2 of channel 0
PARAM = 0xC169A  # P = DM(-32, I2)
STAGE = 0x1212D3
STAGE_SAW = 0x1215FB
STAGE_PULSE = 0x121B99
STAGE_SINE = 0x120CD0
STAGE_TRIANGLE = 0x121183
STAGE_SHAPE_SAW = 0x12169D
STAGE_NOISE = 0x121FD8
STAGE_SHAPE_SQUARE = 0x121AB5
STAGE_SUPER = 0x121D6D
STAGE_SHAPE_SINE0 = 0x123427
STAGE_SHAPE_SINE5 = 0x122F1F
STAGE_SHAPE_SINE2 = 0x1232A5
STAGE_SHAPE_SINE1 = 0x1235C9
STAGE_SHAPE_SINE4 = 0x120DAB
STAGE_SHAPE_SINE3 = 0x12377D
STAGE_MULTI = 0x121C54
STAGE_SUPER1 = 0x121E9D
UNISON_STAGES = (STAGE_SUPER, STAGE_MULTI, STAGE_SUPER1)
PHASE_TABLE = 0xC2336  # voice phase words, lane x at even offsets, lane y one above
NOISE_STATE = 0xB2EF9  # DM state word of lane x (lane y one above)
PULSE_WORD = 0xC15AE  # DM(-8, I1) of channel 0, read by both lanes (I1 is not duplicated in SIMD)
EXTRA_BUF: list[str] = []
# VERIFIED (listing `0x241E4E..0x241EA6`, dump after the pass): the SHAPE SINE 4 output high
# pass reads its state words and coefficient at `DM(-32 / -30 / -28, I7)`,
# `I7 = DM(-9, I3) = 0xC22F6`.
HP_WORDS = (0xC22D6, 0xC22D8, 0xC22DA)
HP_IDLE_COEF = 0x3F7F9DB2
HP_STAGES = (STAGE_SHAPE_SINE4,)
DEVS = ("0x282c0380=1", "0x3108d008=5")


def fbits(x: float) -> int:
    return m.f2u(x)


MODELS = {
    STAGE: m.stage,
    STAGE_SAW: m.stage_saw,
    STAGE_PULSE: m.stage_pulse,
    STAGE_SINE: m.stage_sine,
    STAGE_TRIANGLE: m.stage_triangle,
    STAGE_SHAPE_SAW: m.stage_shape_saw,
    STAGE_NOISE: noise.stage_noise,
    STAGE_SHAPE_SQUARE: shapes.stage_shape_square,
    STAGE_SUPER: unison.stage_super,
    STAGE_SHAPE_SINE0: shape_sine.stage_shape_sine,
    STAGE_SHAPE_SINE5: shape_sine.stage_shape_sine_5,
    STAGE_SHAPE_SINE2: shape_sine.stage_shape_sine_2,
    STAGE_SHAPE_SINE1: shape_sine.stage_shape_sine_1,
    STAGE_SHAPE_SINE4: shape_sine2.stage_shape_sine_4,
    STAGE_SHAPE_SINE3: shape_sine3.stage_shape_sine_3,
    STAGE_MULTI: unison.stage_multi,
    STAGE_SUPER1: super1.stage_super1,
}


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
        d[NOISE_STATE + i] = ln.state
        if ln.hp:
            for off, w in zip(HP_WORDS, ln.hp, strict=True):
                d[off + i] = w
        d[PULSE_WORD] = lanes[0].pulse
        for j, (rho, ph) in enumerate(zip(ln.ratios, ln.phases, strict=False)):
            d[PARAM - 32 + 2 * j + i] = rho
            d[PHASE_TABLE + 2 * j + i] = ph
    for k, c in enumerate(lanes[0].octaves):
        d[unison.MULTI_OCTAVES_ADDR + k] = c
    for j, grp in enumerate(lanes[0].groups):
        for k, w in enumerate(grp):
            d[PARAM - 16 + 4 * j + k] = w
    return d


def random_lanes(rng: random.Random, unison: bool = False) -> list[m.Lane]:
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
            state=rng.getrandbits(32),
        )
        for _ in range(2)
    ]
    pulse = rng.getrandbits(32)
    octaves = tuple(fbits(2 ** rng.uniform(-1, 1.0)) for _ in range(3))
    groups = tuple(tuple(fbits(rng.uniform(-0.3, 0.3)) for _ in range(4)) for _ in range(8))
    for ln in lanes:
        ln.pulse = pulse
        if not unison:
            continue
        ln.ratios = tuple(fbits(2 ** rng.uniform(-2, 3.0)) for _ in range(8))
        ln.phases = tuple(rng.getrandbits(32) for _ in range(8))
        ln.groups = groups
        ln.octaves = octaves
    return lanes


def random_hp(rng: random.Random, lanes: list[m.Lane], stage: int) -> None:
    """Random output high pass states for the stages that end in one (idle coefficient)."""
    if stage in HP_STAGES:
        for ln in lanes:
            ln.hp = (fbits(rng.uniform(-1, 1)), fbits(rng.uniform(-1, 1)), HP_IDLE_COEF)


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
        cmd += ["--buf", f"{NOISE_STATE - 2:#x}:8:f32"]
        cmd += ["--buf", f"{PARAM - 32:#x}:64:f32", "--buf", f"{PHASE_TABLE:#x}:16:f32"]
        cmd += EXTRA_BUF
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
        rng = random.Random(seed)
        lanes = random_lanes(rng, stage in UNISON_STAGES)
        random_hp(rng, lanes, stage)
        got = run_pass(sharc_org, image, lanes, stage)
        for i, ln in enumerate(lanes):
            ln.pitch = got[RECORD - 29 + i]
            ln.shape_a = got[RECORD + 17 + i]
            ln.shape_b = got[RECORD + 19 + i]
            if stage == STAGE_SHAPE_SAW:
                ln.shape_a = got[RECORD - 5 + i]
            ln.spread = got[RECORD - 5 + i]
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
