#!/usr/bin/env python3
"""Pair-register machine for the FX routine models (`tools/fx_model_*.py`).

The FX routines run in SIMD mode (`MODE1` `PEYEN`): every data register is a pair, `PEx` (the
`R` file, left channel) and `PEy` (the `S` file, right channel). A model is a transliteration of
the routine, one `Machine.step` call per executed instruction, so that the data flow of the
firmware is kept verbatim while the arithmetic is exact (`Fraction`) with the 40-bit rounding of
`tools/fx_biquad_check.py`. Semantics follow the `nord-sharc` oracle (`crates/nord-sharc/src/core`):

* a compute reads the registers as they were before the instruction; the registers, the stores
  and the DAG updates of one `step` all take effect together (parallel dual moves);
* a `DM`/`PM` access through `Ii` is a pair `(word, word + 1)`, except a load through `I1` which
  `BDCST1` broadcasts to both elements (`Machine.ldb`);
* `Sn = Rm` moves `Rm` of PEx into `Sn` and `Sm` into `Rn` (the pair swapped), `Rn <-> Sm`
  exchanges only `PEx.Rn` with `PEy.Sm`;
* an integer or shifter operation reads the upper 32 bits of a register, a store keeps them.

A register holds either a 32-bit word (`int`: loaded or produced by an integer / shifter
operation) or an exact extended-precision value (`Fraction`: produced by a float operation).
`Machine.step` can compare the machine with a `sharc-fx --trace` of the real routine before every
instruction (`Machine.trace`), which is how a mismatch is localised to one instruction.
"""

from __future__ import annotations

import math
import sys
from fractions import Fraction
from pathlib import Path

if __package__ in (None, ""):
    sys.path.insert(0, str(Path(__file__).resolve().parent))
import fx_biquad_check as fb

ext40, store32 = fb.ext40, fb.store32
M32 = 0xFFFFFFFF
#: Number of words in a `sharc-fx --trace` row after `trace PC`: R, S, I, M, L and B files.
TRACE_FILES = ("r", "s", "i", "m", "l", "b")


def signed32(x: int) -> int:
    x &= M32
    return x - (1 << 32) if x & 0x80000000 else x


def fl(v) -> Fraction:
    """A register or memory value read as a float (a zero exponent field flushes to zero)."""
    if v is None:
        raise ValueError("register read before it was written")
    if isinstance(v, int):
        return Fraction(0) if (v >> 23) & 0xFF == 0 else fb.from_bits(v)
    return v


def wd(v) -> int:
    """A register or memory value read as a 32-bit word (a float keeps its upper 32 bits)."""
    if v is None:
        raise ValueError("register read before it was written")
    if isinstance(v, int):
        return v & M32
    if v == 0:
        return 0
    return fb.word(store32(v))


def lift(fn):
    """Scalar function of two or one values lifted to registers pairs."""
    return lambda *ops: [fn(*(o[c] for o in ops)) for c in (0, 1)]


def exp_of(q: Fraction) -> int:
    """Unbiased binary exponent of a non-zero value."""
    a = abs(q)
    e = a.numerator.bit_length() - a.denominator.bit_length()
    if Fraction(2) ** e > a:
        e -= 1
    if Fraction(2) ** (e + 1) <= a:
        e += 1
    return e


def recips_value(a) -> Fraction:
    """`Fn = RECIPS Fx`: sign of `a`, exponent `-e - 1`, 8-bit seed fraction of the 7 MSBs."""
    q = fl(a)
    if q == 0:
        return Fraction(2) ** 200  # infinity stand-in (AV set); never reached in the scenarios
    e = exp_of(q)
    if e > 125:
        return Fraction(0)
    idx = int((abs(q) / Fraction(2) ** e - 1) * 128)
    seed = RECIPS_SEED[idx]
    return (1 if q > 0 else -1) * Fraction(2) ** (-e - 1) * (1 + Fraction(seed, 256))


#: VERIFIED: `RECIPS` seed fraction by the 7 MSBs of the input fraction, simulator measurement,
#: `crates/nord-sharc/src/core/float_seeds.rs` (`RECIPS_SEED`, 5,644 inputs, max err 0).
RECIPS_SEED = [
    0xFF, 0xFC, 0xF8, 0xF4, 0xF0, 0xEC, 0xE8, 0xE4, 0xE0, 0xDE, 0xDA, 0xD6, 0xD4, 0xD0, 0xCC, 0xCA,
    0xC6, 0xC2, 0xC0, 0xBC, 0xBA, 0xB6, 0xB4, 0xB2, 0xAE, 0xAC, 0xA8, 0xA6, 0xA4, 0xA0, 0x9E, 0x9C,
    0x98, 0x96, 0x94, 0x92, 0x8E, 0x8C, 0x8A, 0x88, 0x86, 0x82, 0x80, 0x7E, 0x7C, 0x7A, 0x78, 0x76,
    0x74, 0x72, 0x70, 0x6E, 0x6C, 0x6A, 0x68, 0x66, 0x64, 0x62, 0x60, 0x5E, 0x5C, 0x5A, 0x58, 0x56,
    0x54, 0x52, 0x50, 0x50, 0x4E, 0x4C, 0x4A, 0x48, 0x46, 0x46, 0x44, 0x42, 0x40, 0x3E, 0x3E, 0x3C,
    0x3A, 0x38, 0x38, 0x36, 0x34, 0x32, 0x32, 0x30, 0x2E, 0x2E, 0x2C, 0x2A, 0x28, 0x28, 0x26, 0x24,
    0x24, 0x22, 0x20, 0x20, 0x1E, 0x1E, 0x1C, 0x1A, 0x1A, 0x18, 0x18, 0x16, 0x14, 0x14, 0x12, 0x12,
    0x10, 0x0E, 0x0E, 0x0C, 0x0C, 0x0A, 0x0A, 0x08, 0x08, 0x06, 0x06, 0x04, 0x04, 0x02, 0x02, 0x00,
]  # fmt: skip


def fix_word(q: Fraction, scale: int) -> int:
    """`Rn = FIX Fx BY scale`: round to nearest even; out of range gives all ones."""
    if q == 0:
        return 0
    v = q * Fraction(2) ** scale
    fl_ = v.numerator // v.denominator
    rem = v - fl_
    if rem > Fraction(1, 2) or (rem == Fraction(1, 2) and fl_ % 2 == 1):
        fl_ += 1
    return M32 if not -(1 << 31) <= fl_ < (1 << 31) else fl_ & M32


def float_by(i: int, scale: int) -> Fraction:
    """`Fn = FLOAT Rx BY scale` for the signed integer `i`."""
    return ext40(Fraction(i) * Fraction(2) ** scale)


def lshift(x: int, s: int) -> int:
    s = signed32(s)
    if s >= 0:
        return (x << s) & M32 if s < 32 else 0
    return x >> -s if -s < 32 else 0


def extract(x: int, pos: int, length: int, signed: bool) -> int:
    v = (x & M32) >> pos if pos < 32 else 0
    v &= (1 << length) - 1 if length < 32 else M32
    if signed and length and pos + length <= 32 and (v >> (length - 1)) & 1:
        v |= ~((1 << length) - 1) & M32
    return v


def deposit(x: int, pos: int, length: int) -> int:
    field = (x & M32) & ((1 << length) - 1 if length < 32 else M32)
    return (field << pos) & M32


def fpack(x: int) -> int:
    """`FPACK`: IEEE single to the 16-bit short float (`shifter.rs::fpack`, census verified)."""
    sign = (x >> 31) << 15
    e, frac = (x >> 23) & 0xFF, x & 0x7FFFFF

    def rnd(value: int, drop: int) -> int:
        kept, rest, half = value >> drop, value & ((1 << drop) - 1), 1 << (drop - 1)
        return kept + (1 if rest > half or (rest == half and kept & 1) else 0)

    if e > 135:
        return sign | 0x7FFF
    if e > 120:
        mag = ((((e >> 7) & 1) << 3) | (e & 7)) << 11
        mag += rnd(frac, 12)
    elif e > 109:
        mag = rnd(0x800000 | frac, 133 - e)
    else:
        mag = 0
    return sign | (0x7FFF if mag > 0x7FFF else mag)


def funpack(x: int) -> int:
    """`FUNPACK`: the 16-bit short float in the low half of `x` to an IEEE single."""
    sign, exp, frac = ((x >> 15) & 1) << 31, (x >> 11) & 0xF, x & 0x7FF
    if exp:
        msb = exp >> 3
        e8 = (msb << 7) | (((1 - msb) * 0xF) << 3) | (exp & 7)
        return sign | (e8 << 23) | (frac << 12)
    if frac == 0:
        return sign
    lead = 11 - frac.bit_length()
    return sign | ((120 - lead) << 23) | (((frac << (lead + 1)) & 0x7FF) << 12)


def fclip_value(a: Fraction, b: Fraction) -> Fraction:
    """`Fn = CLIP Fx BY Fy`: `a` limited to `+-|b|`."""
    return a if abs(a) <= abs(b) else (abs(b) if a > 0 else -abs(b))


fadd = lift(lambda a, b: ext40(fl(a) + fl(b)))
fsub = lift(lambda a, b: ext40(fl(a) - fl(b)))
fmul = lift(lambda a, b: ext40(fl(a) * fl(b)))
favg = lift(lambda a, b: ext40((fl(a) + fl(b)) / 2))
fmax = lift(lambda a, b: max(fl(a), fl(b)))
fmin = lift(lambda a, b: min(fl(a), fl(b)))
fneg = lift(lambda a: -fl(a))
fabs = lift(lambda a: abs(fl(a)))
fclip = lift(lambda a, b: fclip_value(fl(a), fl(b)))
frecips = lift(recips_value)
ffloat = lift(lambda i, s: float_by(signed32(wd(i)), signed32(wd(s))))
ffix = lift(lambda a, s: fix_word(fl(a), signed32(wd(s))))
iadd = lift(lambda a, b: (wd(a) + wd(b)) & M32)
ishl = lift(lambda a, s: lshift(wd(a), wd(s)))
fpack_p = lift(lambda a: fpack(wd(a)))
funpack_p = lift(lambda a: funpack(wd(a)))
fext_p = lift(lambda a, pos, ln, se: extract(wd(a), pos, ln, se))
fext_r = lift(lambda a, r: extract(wd(a), wd(r) & 63, (wd(r) >> 6) & 63, False))
fdep_r = lift(lambda a, r: deposit(wd(a), wd(r) & 63, (wd(r) >> 6) & 63))
words = lift(wd)
fdep_p = lift(lambda a, pos, ln: deposit(wd(a), pos, ln))
fpass = lift(lambda a: fl(a))
fdep_or = lift(lambda old, a, pos, ln: wd(old) | deposit(wd(a), pos, ln))


def swap(p):
    """`Rn = Sm`: the pair with the elements exchanged."""
    return [p[1], p[0]]


def imm(word: int):
    """Immediate load: both elements."""
    return [word, word]


def comp(a, b):
    """`COMP(Fx, Fy)`: per element `(AN, AZ)` as two lists."""
    return (
        [fl(a[c]) < fl(b[c]) for c in (0, 1)],
        [fl(a[c]) == fl(b[c]) for c in (0, 1)],
    )


def cond_ge(flags, new, old):
    """`IF GE Rn = Rm`: per element the new value where the last `COMP` was not less."""
    an, _ = flags
    return [old[c] if an[c] else new[c] for c in (0, 1)]


def dag_next(i: int, m: int, length: int, base: int) -> int:
    """`Ii` after `Ii += Mm` with circular buffer `Bi` / `Li` (`core/dag.rs::dag_next`, CBUFEN).

    VERIFIED: the wrap is one buffer length: a non-negative modify reaching `B + L` or beyond
    subtracts `L`, a negative one falling below `B` adds `L`; `L = 0` is a plain add."""
    nxt = (i + m) & M32
    if length == 0:
        return nxt
    if signed32(m) >= 0:
        return (nxt - length) & M32 if nxt >= (base + length) & M32 else nxt
    return (nxt + length) & M32 if nxt < base else nxt


class Missing(dict):
    """Memory that reports the first read of an unset address through `reads`."""

    def __init__(self, *a, default: int | None = None, **k):
        super().__init__(*a, **k)
        self.reads: list[int] = []
        self.default = default

    def __missing__(self, key):
        self.reads.append(key)
        if self.default is not None:
            return self.default
        raise KeyError(f"read of unset address {key:#x}")


class Machine:
    """Registers, DAG registers and flat data memory (`address -> word`)."""

    def __init__(self, mem: dict[int, int], ir: dict[str, int], trace: list | None = None):
        self.mem = mem
        self.x: list = [None] * 16
        self.y: list = [None] * 16
        self.ir = dict(ir)
        self.trace = trace
        self._row = 0
        self.pcs: list[int] = []
        self.written: set[int] = set()

    # --- reads -----------------------------------------------------------------------------
    def r(self, n: int) -> list:
        return [self.x[n], self.y[n]]

    def ld(self, addr: int) -> list:
        """`Rn = DM(addr)` in SIMD mode: PEx at `addr`, PEy at `addr + 1`."""
        return [self.mem[addr], self.mem[addr + 1]]

    def ldb(self, addr: int) -> list:
        """Load through `I1` (`BDCST1`): the PEx word in both elements."""
        return [self.mem[addr], self.mem[addr]]

    def a(self, name: str, off: int = 0) -> int:
        return (self.ir[name] + off) & M32

    def mod(self, i: int, m: int) -> int:
        """`Ii` modified by `Mm` honouring `Li` / `Bi` (post-modify, pre-modify, `MODIFY`)."""
        return dag_next(
            self.ir[f"i{i}"], self.ir[f"m{m}"], self.ir.get(f"l{i}", 0), self.ir.get(f"b{i}", 0)
        )

    # --- one instruction ---------------------------------------------------------------------
    def step(self, pc, w=None, wx=None, wy=None, st=None, d=None, swaps=()):
        """Check the trace, then commit the effects of one instruction.

        `w` maps register numbers to pairs; `wx` / `wy` map to single elements; `st` lists
        `(address, pair)` stores (PEx at the address, PEy at the next); `d` updates DAG registers.
        """
        self.pcs.append(pc)
        if self.trace is not None:
            self._check(pc)
        for n, p in (w or {}).items():
            self.x[n], self.y[n] = p
        for n, v in (wx or {}).items():
            self.x[n] = v
        for n, v in (wy or {}).items():
            self.y[n] = v
        for addr, p in st or ():
            self.mem[addr & M32] = wd(p[0])
            self.mem[(addr + 1) & M32] = wd(p[1])
            self.written.update((addr & M32, (addr + 1) & M32))
        for k, v in (d or {}).items():
            self.ir[k] = v & M32
            if k[0] == "b":  # a write to `Bn` also loads `In` (circular buffer base)
                self.ir["i" + k[1:]] = v & M32

    def _check(self, pc: int) -> None:
        row = self.trace[self._row]
        self._row += 1
        if row["pc"] != pc:
            raise AssertionError(
                f"trace row {self._row - 1}: oracle pc {row['pc']:#x}, model {pc:#x}"
            )
        for name, files in (("x", row["r"]), ("y", row["s"])):
            regs = getattr(self, name)
            for n, v in enumerate(files):
                if regs[n] is not None and wd(regs[n]) != v and (wd(regs[n]) | v) & M32 >> 1:
                    raise AssertionError(
                        f"before {pc:#x}: {'RS'[name == 'y']}{n} model {wd(regs[n]):#010x} "
                        f"oracle {v:#010x}"
                    )
        for k, v in self.ir.items():
            kind, n = k[0], int(k[1:])
            if kind in row and row[kind][n] != v:
                raise AssertionError(f"before {pc:#x}: {k} model {v:#x} oracle {row[kind][n]:#x}")


def parse_trace(text: str, call: int = 0) -> list[dict]:
    """The `trace` rows of call number `call` of a `sharc-fx --trace` output."""
    calls: list[list[dict]] = [[]]
    for line in text.splitlines():
        if line.startswith("trace "):
            f = line.split()
            vals = [int(t, 16) for t in f[2:]]
            row = {"pc": int(f[1], 16)}
            for k, name in enumerate(TRACE_FILES):
                row[name] = vals[16 * k : 16 * k + 16]
            calls[-1].append(row)
        elif line.startswith("call "):
            calls.append([])
    return calls[call]


def log2_exact(q: Fraction) -> float:
    return math.log2(float(q))
