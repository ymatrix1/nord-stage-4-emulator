#!/usr/bin/env python3
"""End-to-end check of the Rust firmware strip chain against the SHARC oracle (`sharc-org`).

The Rust example `synth_strip_e2e` prints what the opt-in `SynthChain::FirmwareStrip` derives for
a panel (strip word, cutoff coefficient `R`, table position, envelope records, envelope levels
and filtered saw samples). This script feeds the same words to the ORG core 2 pass of the real
firmware image and compares:

1. envelope: records poked into the slot-0 chain, controller gate on at pass 0 and a release
   start at the release pass (`tools/synth_adr_oracle.py`), level word after every pass;
2. cutoff: the oracle `R` after the strip word 3 (and its common block target) settled, against
   the Rust `R`;
3. filter: stage `0x12566E` over the Rust saw with the Rust `R` and table position word
   (`tools/synth_stage_oracle.py`) against the Rust filter output.

Usage (see `docs/research/synth-strip-path.md`, needs the local ORG image and `sharc-org`):

    cargo run --release -p nord-core --example synth_strip_e2e > strip.json
    python tools/synth_strip_e2e.py strip.json
"""

from __future__ import annotations

import json
import struct
import sys
import tempfile
from pathlib import Path
from typing import Any, Sequence

import synth_adr_oracle as adr
import synth_stage_oracle as stage

CONTROLLER_ON = 0x01000100  # gate on, scenario A of the research note
CONTROLLER_RELEASE = 0x00000200  # sign 0: start the release record, scenario B


def bits_to_float(b: int) -> float:
    """Float of a float32 bit pattern."""
    return struct.unpack("<f", struct.pack("<I", b & 0xFFFFFFFF))[0]


def max_abs_error(a: Sequence[float], b: Sequence[float]) -> float:
    """Largest absolute difference of two equally long series."""
    if len(a) != len(b):
        raise ValueError(f"length {len(a)} != {len(b)}")
    return max((abs(x - y) for x, y in zip(a, b)), default=0.0)


def word_mismatches(oracle: Sequence[int], rust: Sequence[int]) -> list[int]:
    """Indices where the two word series differ."""
    return [i for i, (o, r) in enumerate(zip(oracle, rust)) if o != r]


def envelope_records(spec: dict[str, Any]) -> dict[int, tuple[float, float, float, int]]:
    """Oracle record table `{address: (a, b, thr, link)}` from the Rust record words."""
    rec = spec["records"]

    def f(name: str) -> tuple[float, float, float]:
        a, b, thr = (bits_to_float(w) for w in rec[name])
        return a, b, thr

    (aa, ab, at), (da, db, dt), (ra, rb, rt) = f("attack"), f("decay"), f("release")
    return {
        adr.ATTACK: (aa, ab, at, adr.DECAY),
        adr.DECAY: (da, db, dt, adr.DECAY),
        adr.RELEASE: (ra, rb, rt, adr.RELEASE),
    }


def check_envelope(spec: dict[str, Any], org: str, image: str) -> dict[str, Any]:
    """Level words of the oracle chain against the Rust levels (oracle pass p = step p).

    Scenario A holds the gate (attack, decay to the sustain level); scenario B starts the release
    record at `release_at`. 15 passes each: the pass installer rewrites the records every 16th.
    """
    out: dict[str, Any] = {}
    for name, key, ctrl in (
        ("A", "levels_a", {0: CONTROLLER_ON}),
        ("B", "levels_b", {0: CONTROLLER_ON, spec["release_at"]: CONTROLLER_RELEASE}),
    ):
        levels = spec[key]
        frames = adr.run_slot0(org, image, len(levels) + 1, ctrl, envelope_records(spec))
        oracle = [frames[p][adr.STATE_Y] for p in range(1, len(levels) + 1)]
        bad = word_mismatches(oracle, levels)
        out[name] = {"steps": len(levels), "mismatches": len(bad), "first_bad": bad[:3]}
    return out


def check_cutoff(spec: dict[str, Any], org: str, image: str) -> dict[str, Any]:
    """`R` the oracle derives once the strip word has settled, against the Rust `R`.

    The strip word chases the per-layer common block word (`w <- 0.99609 w + 0.00488 c`), so the
    target is written to both (common block word 1, `DM(0xE02E9)`) and read after 40 passes.
    """
    word = spec["cutoff_word"] & 0xFFFFFFFF
    rows = adr.run_script(
        org,
        image,
        40,
        msgs=[(0, 3, word), (0, 0x2E9, word)],
        bufs=[f"{adr.R_WORDS:#x}:2:f32"],
    )
    r_oracle = adr.u2f(rows[(39, 0)][0])
    r_rust = bits_to_float(spec["r_bits"])
    return {
        "word": hex(word),
        "r_oracle": r_oracle,
        "r_rust": r_rust,
        "abs_err": abs(r_oracle - r_rust),
    }


def check_filter(spec: dict[str, Any], org: str, image: str) -> dict[str, Any]:
    """Stage `0x12566E` over the Rust saw, both lanes at the Rust `R` and `W28`."""
    xs = [bits_to_float(w) for w in spec["saw"]]
    r, w28 = spec["r_bits"], spec["w28"]
    frame = {-30: (r, r), -28: (w28, w28), -10: (0, 0), -8: (0, 0), -6: (0, 0)}
    frame |= {-4: (0, 0), -2: (0, 0), 0: (0, 0)}
    with tempfile.TemporaryDirectory() as tmp:
        patched = Path(tmp, "core2_patched.bin")  # the three frame-rewriting stores become NOPs
        patched.write_bytes(stage.patch_controls(Path(image).read_bytes()))
        out = stage.run_stage(org, str(patched), 0x12566E, frame, xs, list(xs))
    rust = [bits_to_float(w) for w in spec["filtered"]]
    err = max_abs_error(out["x"], rust)
    peak = max(abs(v) for v in rust)
    return {"samples": len(xs), "max_abs_err": err, "peak": peak, "rel_err": err / peak}


def main(argv: Sequence[str] | None = None) -> int:
    args = list(sys.argv[1:] if argv is None else argv)
    if len(args) != 1:
        print(__doc__)
        return 2
    spec = json.loads(Path(args[0]).read_text(encoding="utf-8"))
    org, image = adr.default_paths()
    report = {
        "envelope": check_envelope(spec, org, image),
        "cutoff": check_cutoff(spec, org, image),
        "filter": check_filter(spec, org, image),
    }
    print(json.dumps(report, indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
