#!/usr/bin/env python3
"""Replay the Vox model (`Edt_Org_kModel = 1`) through the post stage and the real output record.

`docs/research/organ-model-paths.md`: the ARM installs `0x12272D` (generator slot) and `0x1229B3`
(processor slot, record `I1`) for the Vox model. The record, its section coefficients and the
state blocks are image data; the ARM burst of the install writes only the two gain targets
`DM(G + 0) = 1.0`, `DM(G + 1) = 0` and the vibrato depth `DM(0xE20E3)`. The 96 note and bucket
weights are not written by any ARM message that was captured, so the key to oscillator map is
ASSUMED (`note - 24`, weight 1), as in `organ_post_oracle.py`.

    python tools/organ_vox_replay.py SRAM/core1_sram.bin --exe target/release/sharc-chain \
        --notes 60,64,67 --depth 0x240B700 --frames 16384 --out vox

Runs `sharc-chain` through `organ_post_oracle.run_chain`.
"""

from __future__ import annotations

import argparse
import sys

import numpy as np
import organ_oracle_audio as oa
import organ_post_oracle as po
import organ_post_ref as pr

# VERIFIED (image dump `sharc-chain SRAM/core1_sram.bin --dump 0xE2D22:30` and the ARM slot buffer
# messages of `arm_model_paths.py`): the four output-stage records of the PNO image. Record `i`
# (processor slot `4 + i`) has `I1 = RECORD0_I1 + 7 i`, weights `PM(WEIGHTS0 + 96 i)` and parameter
# block `G = PARAMS0 + 12 i`. Organ layer A installs record 0, layer B record 2.
RECORD0_I1 = 0xE2D42
RECORD_STRIDE = 7
WEIGHTS0, WEIGHT_STRIDE = 0x95E46, 0x60
PARAMS0, PARAMS_STRIDE = 0x95FC6, 0xC
VOX_DEPTHS = (0xE46400, 0x1782D00, 0x240B700)  # VERIFIED (ARM): Vox vibrato types 1..3 << 8
WHEEL0_MIDI = po.WHEEL0_MIDI


def record_addresses(index: int) -> tuple[int, int, int]:
    """`(I1, weights, G)` of output-stage record `index` (0..3)."""
    return (
        RECORD0_I1 + RECORD_STRIDE * index,
        WEIGHTS0 + WEIGHT_STRIDE * index,
        PARAMS0 + PARAMS_STRIDE * index,
    )


def vox_pokes(notes: list[int], depth: int, index: int = 0, weights: bool = True) -> dict[int, int]:
    """Pokes on top of the image: the ARM install burst plus the ASSUMED chord weights."""
    _, w0, g = record_addresses(index)
    pokes = {g: oa.ONE, g + 1: 0, pr.DEPTH: depth}  # VERIFIED ARM words (install burst)
    if weights:
        for n in notes:
            pokes[w0 + n - WHEEL0_MIDI] = oa.f2u(1.0)  # ASSUMED key to oscillator map
    return pokes


def replay_steps(index: int = 0) -> list[str]:
    i1, _, _ = record_addresses(index)
    return [po.POST_STEP, f"{po.OUT_STEP}:I0={po.FINAL_OUT:#x},I1={i1:#x}"]


def lane_stats(final: np.ndarray, skip: int = 0) -> dict[str, float]:
    """Peak and rms of the settled part of the final lanes (`frames x 2`)."""
    x = final[skip:].astype(np.float64)
    return {
        "peak": float(np.abs(x).max()) if x.size else 0.0,
        "rms": float(np.sqrt((x**2).mean())) if x.size else 0.0,
        "lane_diff": float(np.abs(x[:, 0] - x[:, 1]).max()) if x.size else 0.0,
    }


def render(exe: str, image: str, pokes: dict[int, int], index: int, frames: int) -> np.ndarray:
    rows = po.run_chain(exe, image, pokes, replay_steps(index), ((po.FINAL_OUT, 4),), frames // 2)
    return rows.astype("<u4").view("<f4").reshape(-1, 2)


def main(argv: list[str] | None = None) -> int:  # pragma: no cover - thin CLI
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    ap.add_argument("image")
    ap.add_argument("--exe", required=True)
    ap.add_argument("--notes", default="60,64,67")
    ap.add_argument("--depth", default="0")
    ap.add_argument("--record", type=int, default=0)
    ap.add_argument("--frames", type=int, default=16384)
    ap.add_argument("--skip", type=int, default=2048)
    ap.add_argument("--out")
    ns = ap.parse_args(argv)
    notes = [int(n) for n in ns.notes.split(",")]
    for label, with_weights in (("ARM burst only", False), ("ARM burst + chord weights", True)):
        pokes = vox_pokes(notes, int(ns.depth, 0), ns.record, with_weights)
        final = render(ns.exe, ns.image, pokes, ns.record, ns.frames)
        s = lane_stats(final, ns.skip)
        print(f"{label}: peak {s['peak']:.5f} rms {s['rms']:.5f} lane diff {s['lane_diff']:.2e}")
        if ns.out and with_weights:
            np.save(f"{ns.out}.f32.npy", final)
    return 0


if __name__ == "__main__":
    sys.exit(main())
