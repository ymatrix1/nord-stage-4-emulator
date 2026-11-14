#!/usr/bin/env python3
"""Oracle harness of the generated Mod 2 models (`tools/fx_model_mod2_gen.py`).

The routines are generated from the ADI listing by `tools/fx_listing_gen.py`; this module only
describes how to call them: the memory map of a scenario (context around `I1`, delay ring, frame
block) and the registers the pass leaves set. `tools/fx_models.py check|trace NAME` runs the
scenarios through `sharc-fx` and compares every written word (`tools/fx_oracle.py`).
`docs/research/fx-mod2-types.md` has the routines and their context words.
"""

from __future__ import annotations

import random
import sys
from fractions import Fraction
from collections.abc import Callable
from dataclasses import dataclass, field
from pathlib import Path

if __package__ in (None, ""):
    sys.path.insert(0, str(Path(__file__).resolve().parent))
import fx_biquad_check as fb
import fx_leslie_ops as ops
import fx_model_mod2_gen as gen
import fx_oracle as fo

IN_ADDR, CTX, BUF = 0xB1000, 0xB2040, 0xB4000
RING_WORDS = 0x410
#: Registers the pass leaves set that the routines read before writing (`M1`, `M0`, `M2`).
REGS = {"m0": 0, "m1": 1, "m2": 2}


@dataclass
class Spec:
    """One routine: where it is, how a scenario fills its memory."""

    name: str
    pc: int
    end: int
    fn: Callable
    #: context words `CTX + lo .. CTX + hi` filled with random floats
    window: tuple[int, int] = (-0x20, 0x26)
    #: fixup(rng, mem, count) sets the integer / pointer words of the context
    fixup: Callable = lambda rng, mem, count: None
    ring: bool = True
    #: context offset of the block count word
    blocks: int = 0
    #: range of the random float words (a reciprocal in the routine needs them away from -1)
    span: tuple[float, float] = (-1.0, 1.0)
    sets: dict[str, int] = field(default_factory=dict)


def rnd_float(rng: random.Random, span: tuple[float, float] = (-1.0, 1.0)) -> int:
    return fb.bits(rng.uniform(*span))


def scenario(spec: Spec, rng: random.Random) -> dict[int, int]:
    """Context, ring and 1 to 3 blocks of input frames."""
    count = rng.randrange(1, 4)
    mem = {CTX + o: rnd_float(rng, spec.span) for o in range(spec.window[0], spec.window[1] + 1)}
    mem.update({a: rnd_float(rng) for a in range(IN_ADDR, IN_ADDR + 4 * count)})
    if spec.ring:
        mem.update({a: rnd_float(rng) for a in range(BUF, BUF + RING_WORDS)})
    spec.fixup(rng, mem, count)
    return mem


def finite_scenario(spec: Spec, rng: random.Random) -> dict[int, int]:
    """A scenario whose float state stays moderate. The models do not represent overflow (the
    hardware saturates to infinity): a random context of a feedback routine blows up now and
    then, the context of a real instance never does."""
    while True:
        case = scenario(spec, rng)
        try:
            m = run(spec, fo.Missing(case))
        except OverflowError:
            continue
        floats = [v for v in (*m.x, *m.y) if isinstance(v, Fraction)]
        if all(abs(v) < 2**64 for v in floats):
            return case


def routine(spec: Spec):
    """The harness description of a routine (`tools/fx_oracle.py`)."""
    return fo.Routine(
        spec.name,
        spec.pc,
        spec.end,
        {"I0": IN_ADDR, "I1": CTX, "M0": 0, "M2": 2, "M9": 1, **spec.sets},
        lambda mem, trace=None: run(spec, mem, trace),
        lambda rng: finite_scenario(spec, rng),
    )


def run(spec: Spec, mem: dict[int, int], trace=None) -> ops.LMachine:
    m = ops.LMachine(mem, {**REGS, "i0": IN_ADDR, "i1": CTX}, trace)
    try:
        spec.fn(m)
    except IndexError:  # the oracle stops before the final `rts`: one step past the trace
        if m.pcs[-1] != spec.end:
            raise
        m.pcs.pop()
    return m


def fix_vibe(rng: random.Random, mem: dict[int, int], count: int) -> None:
    """`0x123C84`: phase `c[-0x19] += c[-0x17]` through the self pointer `c[-0x16]`; ring base
    `c[-0x1F]`, running pointer `c[-0x1E]` (a pair address inside the 0x400 ring), delay
    `c[-0x20]` in words; `c[0x14]` blocks."""
    mem.update({
        CTX - 0x16: CTX - 0x19,
        CTX - 0x19: rng.randrange(1 << 32),
        CTX - 0x17: rng.randrange(1 << 24),
        CTX - 0x20: rng.randrange(0, 0x400),
        CTX - 0x1F: BUF,
        CTX - 0x1E: BUF + 2 * rng.randrange(0, 0x200),
        CTX + 0x14: count,
    })  # fmt: skip


def fix_flang(rng: random.Random, mem: dict[int, int], count: int) -> None:
    """`0x12348D`: phase word `c[-0x13] += c[-0x14]` read through the pointer `c[-0x11]`; ring
    pointer `c[-0x1A]`, base `c[-0x19]` (0x400 words); `c[2]` blocks."""
    mem.update({
        CTX - 0x11: CTX - 0x13,
        CTX - 0x13: rng.randrange(1 << 32),
        CTX - 0x14: rng.randrange(1 << 24),
        CTX - 0x1A: BUF + 2 * rng.randrange(0, 0x200),
        CTX - 0x19: BUF,
        CTX + 2: count,
    })  # fmt: skip


def fix_phas1(rng: random.Random, mem: dict[int, int], count: int) -> None:
    """`0x1238FC`: phase word `c[-0x1F] += c[-0x20]` read through the pointer `c[-0x1D]`;
    `c[4]` blocks. One scenario in six zeroes the phase so that the `IF NE` branch is taken."""
    zero = rng.randrange(6) == 0
    mem.update({
        CTX - 0x1D: CTX - 0x1F,
        CTX - 0x1F: 0 if zero else rng.randrange(1 << 32),
        CTX - 0x20: 0 if zero else rng.randrange(1 << 24),
        CTX + 4: count,
    })  # fmt: skip


def fix_phas0(rng: random.Random, mem: dict[int, int], count: int) -> None:
    """`0x12379B`: `c[-0x17] += c[-0x20]` (phase, integer overflow flags select the words kept),
    `c[-0x1F]` points at the word the sum is taken with, `c[0x1A]` at a table read at
    `trunc(f0)`; `c[0x1B]` blocks."""
    mem.update({
        CTX - 0x1F: BUF + 0x300,
        CTX - 0x20: rng.randrange(1 << 32),
        CTX - 0x17: rng.randrange(1 << 32),
        CTX + 0x1A: BUF + 0x100,
        CTX + 0x1B: count,
    })  # fmt: skip


SPECS: dict[str, Spec] = {
    "vibe": Spec("vibe", 0x123C84, 0x123DB3, gen.vibe, fixup=fix_vibe, blocks=0x14),
    "flang": Spec("flang", 0x12348D, 0x123538, gen.flang, fixup=fix_flang, blocks=2),
    "phas0": Spec("phas0", 0x12379B, 0x1238FB, gen.phas0, fixup=fix_phas0, blocks=0x1B, span=(0.0, 0.2)),
    "phas1": Spec("phas1", 0x1238FC, 0x1239CF, gen.phas1, fixup=fix_phas1, blocks=4, ring=False),
}


class Kind:
    """Registry adapter for `tools/fx_models.py`."""

    def __init__(self, name: str):
        self.name = name

    def routine(self):
        return routine(SPECS[self.name])
