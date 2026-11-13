#!/usr/bin/env python3
"""Golden vectors of the FX routine models for the Rust ports (`crates/nord-core/src/fx/`).

    python tools/fx_golden.py resonant > crates/nord-core/src/fx/tests_resonant_golden.rs

Each routine is called `CALLS` times on a seeded synthetic scenario with the state carried
between calls (new input frames per call). The output is Rust `const` arrays of 32-bit words:
the context before the first call, the inputs and outputs of every call and the context after
the last. Only derived values appear (the models are checked word for word against the
`sharc-fx` oracle, `tools/fx_models.py check`).
"""

from __future__ import annotations

import argparse
import random
import sys
from collections.abc import Callable
from dataclasses import dataclass, field
from pathlib import Path

if __package__ in (None, ""):
    sys.path.insert(0, str(Path(__file__).resolve().parent))
import fx_biquad_check as fb
import fx_models
import fx_oracle as fo
from fx_pe import M32

CALLS = 3
DELAY_READ = 0xB4080  # BUF_A + 0x80: read line of the delay golden scenario
#: smoothing coefficient 0.9908, clip bound 1.0 and input pole 0.3 of the resonant filters
RESONANT_FIXED = {-0x11: fb.bits(0.9908), -0xE: fb.bits(1.0), -0x14: fb.bits(0.3)}


@dataclass
class Spec:
    """Where a routine's words live: the context, the frame block, per-call extras, tables."""

    routine: str
    prefix: str
    ctx: tuple[int, int]  # first and last offset from CTX
    frames: int  # words at IN_ADDR per call
    tables: dict[str, tuple[int, int]] = field(default_factory=dict)  # name -> address range
    per_call: dict[int, tuple[int, int]] = field(default_factory=dict)  # addr -> random int range
    fixed: dict[int, int] = field(default_factory=dict)  # scenario overrides (offsets from CTX)
    absolute: dict[int, int] = field(default_factory=dict)  # scenario overrides (addresses)
    initial: dict[str, tuple[int, int]] = field(default_factory=dict)  # ranges before the calls
    final: dict[str, tuple[int, int]] = field(default_factory=dict)  # ranges after the calls
    setup: Callable | None = None  # setup(case, rng): extra scenario edits
    consts: dict[str, int] = field(default_factory=dict)  # plain `usize` constants


def delay_setup(case: dict[int, int], rng: random.Random) -> None:
    """Move the read line to `BUF_A + 0x80` so that one Rust slice holds both lines."""
    import fx_model_delay as dm

    case[0xC03D5] = DELAY_READ
    for k in range(0x60):
        case[DELAY_READ + k] = case.pop(dm.BUF_B + k)


def specs() -> dict[str, Spec]:
    import fx_model_allpass as ap
    import fx_model_delay as dm
    import fx_model_lfo as lf

    return {
        "resonant": Spec("resonant", "RES2", (-0x20, -1), 4, fixed=RESONANT_FIXED),
        "resonant4": Spec("resonant4", "RES4", (-0x20, -1), 8, fixed=RESONANT_FIXED),
        "allpass": Spec(
            "allpass",
            "AP",
            (-0x20, 4),
            8,
            per_call={ap.PHASE_PTR: (0, M32), ap.PHASE_PTR + 1: (0, M32)},
            fixed={4: 2},
        ),  # fmt: skip
        "delay": Spec(
            "delay",
            "DLY",
            (-0x20, 0x25),
            4,
            tables={"shaper": (dm.TABLE - 24, dm.TABLE + 24)},
            initial={
                "line": (dm.BUF_A, DELAY_READ + 0x5F),
                "sends": (dm.OUT_ADDR, dm.OUT_ADDR + 3),
            },
            final={
                "line_out": (dm.BUF_A, DELAY_READ + 0x5F),
                "sends_out": (dm.OUT_ADDR, dm.OUT_ADDR + 3),
            },
            fixed={
                -0xC: fb.bits(5.0),
                0xE: fb.bits(0.9908),
                -0x1E: 24,
                -0x1D: (-24) & M32,
                -0x1C: 24 << 6,
                0x13: 0,
                0x10: 2,
                0x11: 3,
                0x12: 4,
            },
            setup=delay_setup,
            consts={"READ_BASE": 0x80},
        ),  # fmt: skip
        "lfo": Spec(
            "lfo",
            "LFO",
            (-0x20, 0x15),
            8,
            tables={
                "table_a": (lf.TAB_A, lf.TAB_A + 64),
                "table_b": (lf.TAB_B, lf.TAB_B + 64),
                "table_c": (lf.TAB_C - 80, lf.TAB_C + 80),
            },
            fixed={
                0x15: 2,
                -2: 30,
                -1: (-31) & M32,
                -0x20: fb.bits(0.3),
                -0x1F: fb.bits(0.4),
                0: fb.bits(10.0),
                1: fb.bits(0.2),
            },
        ),  # fmt: skip
    }


def words_rust(name: str, words: list[int]) -> str:
    items = ", ".join(f"{w:#010x}" for w in words)
    one = f"pub(super) const {name}: [u32; {len(words)}] = [{items}];"
    if len(one) <= 100:  # rustfmt keeps a short array on one line
        return one
    lines = [f"pub(super) const {name}: [u32; {len(words)}] = ["]
    lines += [
        "    " + ", ".join(f"{w:#010x}" for w in words[i : i + 8]) + ","
        for i in range(0, len(words), 8)
    ]
    return "\n".join(lines + ["];"])


def vectors(spec: Spec, seed: int = 20260102) -> dict[str, list[int]]:
    """Run the model `CALLS` times and collect the words for the Rust constants."""
    rt = fx_models.routines()[spec.routine]
    mod = fx_models.MODULES[spec.routine]
    rng = random.Random(seed)
    case = rt.scenario(rng)
    for off, w in spec.fixed.items():
        case[mod.CTX + off] = w
    case.update(spec.absolute)
    if spec.setup:
        spec.setup(case, rng)
    mem = fo.Missing({**rt.static, **case})
    ctx_range = range(mod.CTX + spec.ctx[0], mod.CTX + spec.ctx[1] + 1)
    out: dict[str, list[int]] = {"ctx_in": [mem[a] for a in ctx_range]}
    for name, (lo, hi) in spec.initial.items():
        out[name] = [mem.get(a, 0) for a in range(lo, hi + 1)]
    out |= {"frames_in": [], "frames_out": [], "extra": []}
    for _ in range(CALLS):
        for a in range(mod.IN_ADDR, mod.IN_ADDR + spec.frames):
            mem[a] = fb.bits(rng.uniform(-1.0, 1.0))
        for a, (lo, hi) in spec.per_call.items():
            mem[a] = rng.randrange(lo, hi + 1)
            out["extra"].append(mem[a])
        out["frames_in"] += [mem[a] for a in range(mod.IN_ADDR, mod.IN_ADDR + spec.frames)]
        rt.model(mem, None)
        out["frames_out"] += [mem[a] for a in range(mod.IN_ADDR, mod.IN_ADDR + spec.frames)]
    out["ctx_out"] = [mem[a] for a in ctx_range]
    for name, (lo, hi) in spec.tables.items():
        out[name] = [mem[a] for a in range(lo, hi + 1)]
    for name, (lo, hi) in spec.final.items():
        out[name] = [mem.get(a, 0) for a in range(lo, hi + 1)]
    if not out["extra"]:
        del out["extra"]
    return out


def rust(names: list[str]) -> str:
    """One Rust file with the constants of every routine in `names`."""
    table = specs()
    lines = [
        f"//! Firmware parity of the {', '.join(names)} routine model(s): golden vectors.",
        "//!",
        f"//! Derived values only (context, input and output float32 / integer words of {CALLS}",
        "//! calls with the state carried), generated by",
        f"//! `python tools/fx_golden.py {' '.join(names)} > <this file>` from the models that",
        "//! match the `sharc-fx` run of the firmware routines word for word",
        "//! (`docs/research/fx-routine-models.md`).",
        "",
    ]
    checks = []
    for name in names:
        spec = table[name]
        vec = vectors(spec)
        lines += [words_rust(f"{spec.prefix}_{k.upper()}", v) for k, v in vec.items()]
        lines += [
            f"pub(super) const {spec.prefix}_{k}: usize = {v:#x};" for k, v in spec.consts.items()
        ]
        p = spec.prefix
        checks += [
            f"    assert_eq!({p}_CTX_IN.len(), {len(vec['ctx_in'])});",
            f"    assert_eq!({p}_CTX_OUT.len(), {p}_CTX_IN.len());",
            f"    assert_eq!({p}_FRAMES_IN.len(), {len(vec['frames_in'])});",
            f"    assert_eq!({p}_FRAMES_OUT.len(), {p}_FRAMES_IN.len());",
            f"    assert!({p}_FRAMES_IN != {p}_FRAMES_OUT);",
        ]
    lines += ["", "#[test]", "fn golden_vectors_have_the_documented_shapes() {", *checks, "}"]
    return "\n".join(lines) + "\n"


def main(argv: list[str] | None = None) -> int:
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    p.add_argument("routine", nargs="+", choices=sorted(specs()))
    args = p.parse_args(argv)
    print(rust(args.routine), end="")
    return 0


if __name__ == "__main__":
    sys.exit(main())
