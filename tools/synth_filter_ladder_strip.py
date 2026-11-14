#!/usr/bin/env python3
"""End-to-end check of the Ladder24 cutoff law: strip word 3 -> pass control -> stage `0x12549D`.

The word-for-word oracle run (`tools/synth_filter_ladder_oracle.py`) patches the frame-rewriting
control stores out and pokes the cutoff position `DM(-30)` by hand. This tool leaves the pass
control block of the image in place instead: it installs the Ladder24 stage as the B stage of
channel 0 (`tools/synth_stage_oracle.py`), writes the strip-0 cutoff word `DM(0xE0003)` (Q31, the
word the ARM sends), runs a few passes and reads back, per lane,

* the frame word `DM(-30)` that the control block wrote (the table value `R` of
  `docs/research/synth-adr-and-strip-law.md` s4, the same value every other filter stage reads),
* the resonance word `DM(-28)` and the four coefficient words `DM(-18, -16, -14, -12)` that the
  stage stored in the same pass.

The stage coefficients equal `tools/synth_filter_ladder.py::coefficients(R, q)` word for word, so
the cutoff position of the Ladder24 stage **is `R`**: no separate law for kFltType 2.

    python tools/synth_filter_ladder_strip.py --image SRAM_ORG/core2_sram.bin
    python tools/synth_filter_ladder_strip.py --image ... --record cases.json

Needs the local ORG core 2 image and `cargo build --release -p nord-sharc`.
Result: `docs/research/synth-stage-models-10.md` s1.
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

if __package__ in (None, ""):
    sys.path.insert(0, str(Path(__file__).resolve().parent))
import synth_filter_ladder as lad
import synth_stage_oracle as o

STRIP0_WORD3 = 0xE0003  # VERIFIED: strip 0 word 3, synth-adr-and-strip-law.md s4
PASS = 5  # frame read after pass 5: the strip smoother has left the first passes behind
WORDS = (0x00000000, 0x08000000, 0x10000000, 0x20000000, 0x30000000, 0x40000000, 0x50000000)
COEF_OFFS = (-18, -16, -14, -12)


def model_coefs(w30: int, w28: int) -> list[int]:
    """Coefficient words the stage computes from the frame words `DM(-30)` and `DM(-28)`."""
    return list(lad.coefficients(w30, w28))


def oracle_case(sharc_org: str, image: str, word: int) -> dict:  # pragma: no cover - firmware
    """One strip word: per lane `r` (`DM(-30)`), `q` (`DM(-28)`) and the coefficient words."""
    res = o.run_stage(
        sharc_org,
        image,
        lad.STAGE,
        {},
        [0.5] * (4 * (PASS + 1)),
        extra={STRIP0_WORD3: word},
        watch_frame=(-32, 64),
    )
    fr = res["frames"][PASS]
    return {
        "word": word,
        "lanes": [
            {
                "r": fr[-30][lane],
                "q": fr[-28][lane],
                "coefs": [fr[off][lane] for off in COEF_OFFS],
            }
            for lane in (0, 1)
        ],
    }


def mismatches(cases: list[dict]) -> int:
    """Number of lane coefficient sets that differ from the model run on the recorded `R`."""
    bad = 0
    for case in cases:
        r_x = case["lanes"][0]["r"]  # the cutoff word of lane x serves both lanes
        for lane in case["lanes"]:
            bad += model_coefs(r_x, lane["q"]) != lane["coefs"]
    return bad


def main(argv: list[str] | None = None) -> int:  # pragma: no cover - thin CLI
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--image", required=True)
    ap.add_argument("--sharc-org", default="target/release/sharc-org")
    ap.add_argument("--record", type=Path, help="write the recorded cases to this file")
    ns = ap.parse_args(argv)
    cases = [oracle_case(ns.sharc_org, ns.image, w) for w in WORDS]
    bad = mismatches(cases)
    print(f"{len(cases) * 2 - bad} of {len(cases) * 2} lane coefficient sets equal the model")
    if ns.record:
        doc = {"stage": "0x12549d", "pass": PASS, "cases": cases}
        ns.record.write_text(json.dumps(doc) + "\n", encoding="utf-8")
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
