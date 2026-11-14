#!/usr/bin/env python3
"""Records oracle cases of the further oscillator stages into the golden JSON.

    python tools/synth_stage_record.py --image SRAM_ORG/core2_sram.bin --stage 0x121D6D

Runs six random one-pass trials of the stage in `sharc-org` and stores the lane words (pitch and
shape words as read back after the pass) and the oracle output words under the stage key of
`docs/research/data/synth-stage-oracle-cases-rest.json` (one case per line; the first six
stages stay in `synth-stage-oracle-cases.json`); `tools/tests/test_synth_stage_rest.py` replays
them through the Python models without the firmware.
"""

from __future__ import annotations

import argparse
import dataclasses
import json
import random
import sys
from pathlib import Path

if __package__ in (None, ""):
    sys.path.insert(0, str(Path(__file__).resolve().parent))
import synth_stage_super2 as super2
import synth_wave_model as m
import synth_wave_oracle as o

CASES = (
    Path(__file__).resolve().parents[1] / "docs/research/data/synth-stage-oracle-cases-rest.json"
)
FIRST_SEED = 101
COUNT = 6


def lane_json(ln: m.Lane) -> dict:
    """Lane words as JSON (tuples become lists)."""
    return json.loads(json.dumps(dataclasses.asdict(ln)))


def lane_from_json(d: dict) -> m.Lane:
    """Inverse of `lane_json` (lists become tuples)."""
    fields = {
        k: (tuple(map(tuple, v)) if k == "groups" else tuple(v)) if isinstance(v, list) else v
        for k, v in d.items()
    }
    return m.Lane(**fields)


def dump(data: dict) -> str:
    """JSON with one case per line (keeps the golden file short)."""
    items = []
    for name, cases in sorted(data["stages"].items()):
        body = ",\n".join("  " + json.dumps(c, separators=(",", ":")) for c in cases)
        items.append(f' "{name}": [\n{body}\n ]')
    return '{"stages": {\n' + ",\n".join(items) + "\n}}\n"


def record(
    sharc_org: str, image: str, stage: int, count: int = COUNT
) -> list[dict]:  # pragma: no cover - needs the firmware image
    """`count` oracle cases of one stage."""
    cases = []
    for seed in range(FIRST_SEED, FIRST_SEED + count):
        rng = random.Random(seed)
        lanes = o.random_lanes(rng, stage in o.UNISON_STAGES)
        if stage == o.STAGE_SUPER24:
            super2.random_state(seed, lanes)
        o.random_hp(rng, lanes, stage)
        o.random_misc(seed, lanes, stage)
        o.random_s5(seed, lanes, stage)
        o.random_pitch(seed, lanes, stage)
        got = o.run_pass(sharc_org, image, lanes, stage)
        for i, ln in enumerate(lanes):
            ln.pitch = got[o.RECORD - 29 + i]
            ln.shape_a = got[o.RECORD + 17 + i]
            ln.shape_b = got[o.RECORD + 19 + i]
            ln.spread = got[o.RECORD - 5 + i]
        out = [[got[o.RECORD - 13 + 2 * k + i] for k in range(4)] for i in (0, 1)]
        cases.append({"lanes": [lane_json(ln) for ln in lanes], "oracle_out": out})
    return cases


def main(argv: list[str] | None = None) -> int:  # pragma: no cover - thin CLI
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--image", required=True)
    ap.add_argument("--sharc-org", default="target/release/sharc-org")
    ap.add_argument("--stage", type=lambda v: int(v, 0), required=True)
    ns = ap.parse_args(argv)
    data = json.loads(CASES.read_text()) if CASES.exists() else {"stages": {}}
    data["stages"][f"{ns.stage:#x}"] = record(ns.sharc_org, ns.image, ns.stage)
    CASES.write_text(dump(data))
    return 0


if __name__ == "__main__":
    sys.exit(main())
