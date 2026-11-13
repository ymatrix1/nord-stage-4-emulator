#!/usr/bin/env python3
"""Reference models of FX DSP routines, checked word for word against the `sharc-fx` oracle.

Each model is a transliteration of one routine of the FX image (`docs/research/fx-type-map.md`
s7) over a flat memory dict: SIMD pair semantics (PEx = word at the address, PEy = the next
word, `MODE1` BDCST1 copies the PEx word to PEy for every load through `I1`), 40-bit registers
rounded to a 32-bit mantissa, stores keep the upper 32 bits. The arithmetic helpers come from
`tools/fx_biquad_check.py`.

`compressor_0x120630` is the compressor gain computer (module list `0xC0441558`, run 6 of
`docs/research/fx-chain-install.md`): per block of 4 stereo frames it measures the stereo-linked
peak, keeps a release-smoothed level, turns it into an estimate of `log2`, subtracts a threshold,
smooths the excess with an attack/release pair, scales it to a gain-reduction fraction, turns that
into a gain by interpolating two float bit patterns and multiplies the 4 frames in place.

    cargo build -p nord-sharc --release
    python tools/fx_routine_models.py check compressor SRAM/core2_sram.bin --scenarios 300
    python tools/fx_routine_models.py golden compressor      # Rust golden vector, no image
"""

from __future__ import annotations

import argparse
import math
import random
import subprocess
import sys
from fractions import Fraction
from pathlib import Path

if __package__ in (None, ""):
    sys.path.insert(0, str(Path(__file__).resolve().parent))
import fx_biquad_check as fb

#: VERIFIED: docs/research/fx-type-map.md s7 (`check compressor`): routine entry / last pc.
COMPRESSOR_PC, COMPRESSOR_END = 0x120630, 0x1206AF
#: Offsets from `I1` of the 4-frame block context. Loads through `I1` are broadcast to both
#: elements (BDCST1); loads and stores through `I4 = I1` are pairs `(word, word + 1)`.
#: VERIFIED against the oracle (900 random scenarios, 0 differing words); the roles are
#: EMPIRICAL (read from the data flow).
FIELDS = {
    "m3": -0x13,  # DAG M3: offset of frame 2 from I0 (4 in the pass)
    "m4": -0x12,  # DAG M4 (through R4): offset of frame 3 from I0 (6)
    "level": -0x11,  # pair: smoothed level state (written back)
    "excess": -0x0F,  # pair: smoothed excess state (written back)
    "mant_scale": -0x0D,  # int: exponent shift of the MANT term (-31 gives log2 = e + frac)
    "fix_scale": -0x0C,  # int: FIX exponent shift (31 gives a Q31 fraction)
    "log_gain": -0x0B,  # float: multiplier of the log2 estimate
    "table0": -0x0A,  # word: float bit pattern at fraction 0
    "smooth": -0x09,  # float: coefficient of the makeup smoother
    "attack": -0x20,  # pair: coefficient while the excess rises
    "release": -0x1E,  # pair: coefficient of the level release and of the excess fall
    "makeup_target": -0x1C,  # pair
    "makeup": -0x1A,  # pair: smoothed makeup gain (written back)
    "threshold": -0x19,  # pair (the left word overlaps the right makeup word)
    "slope": -0x17,  # pair: reduction per log2 unit above the threshold
    "reduction_scale": -0x15,  # float: sign / scale of the reduction before FIX
    "table1": -0x14,  # word: float bit pattern at fraction 1
}
LO_OFFSET, HI_OFFSET = -0x20, -0x07  # words read or written relative to I1 (inclusive)

ext40, store32 = fb.ext40, fb.store32


def f_of(word: int) -> Fraction:
    """Float32 memory word to an exact value (a zero exponent field flushes to zero)."""
    return Fraction(0) if (word >> 23) & 0xFF == 0 else fb.from_bits(word)


def signed32(x: int) -> int:
    x &= 0xFFFFFFFF
    return x - (1 << 32) if x & 0x80000000 else x


def logb_word(q: Fraction) -> int:
    """`Rn = LOGB Fx` as a 32-bit word: the unbiased exponent (zero gives `0xFF800000`)."""
    if q == 0:
        return 0xFF800000
    a = abs(q)
    e = a.numerator.bit_length() - a.denominator.bit_length()
    if Fraction(2) ** e > a:
        e -= 1
    if Fraction(2) ** (e + 1) <= a:
        e += 1
    return e & 0xFFFFFFFF


def mant_word(q: Fraction) -> int:
    """`Rn = MANT Fx`: the mantissa with its hidden bit as an unsigned 1.31 integer."""
    if q == 0:
        return 0
    m = abs(q) / Fraction(2) ** signed32(logb_word(q)) * (1 << 31)
    assert m.denominator == 1
    return int(m)


def float_by(i: int, scale: int) -> Fraction:
    """`Fn = FLOAT Rx BY scale` for the signed integer `i` (exact below 2^32)."""
    return ext40(Fraction(i) * Fraction(2) ** scale)


def fix_by_word(q: Fraction, scale: int) -> int:
    """`Rn = FIX Fx BY scale`: round to nearest even; out of range gives all ones."""
    if q == 0:
        return 0
    v = q * Fraction(2) ** scale
    fl = v.numerator // v.denominator
    rem = v - fl
    if rem > Fraction(1, 2) or (rem == Fraction(1, 2) and fl % 2 == 1):
        fl += 1
    return 0xFFFFFFFF if not -(1 << 31) <= fl < (1 << 31) else fl & 0xFFFFFFFF


def ssf_word(a: int, b: int) -> int:
    """`Rn = Rx * Ry (SSF)`: signed fractional product, upper 32 bits of `2 a b` (floor)."""
    return ((signed32(a) * signed32(b) * 2) >> 32) & 0xFFFFFFFF


def compressor_0x120630(mem: dict[int, int], i0: int, i1: int) -> dict[str, list]:
    """Run routine `0x120630` on `mem` (address -> 32-bit word), updating it in place.

    `i0` is the 8-word block (4 stereo frames, left words first of each pair), `i1` the context.
    Returns the intermediate values `gain` (per element), `r5` (the interpolation fraction word
    per element) and `gt` (attack branch taken per element).
    """

    def pair(addr: int) -> list[int]:  # I4 load: PEx = [addr], PEy = [addr + 1]
        return [mem[addr], mem[addr + 1]]

    def bcast(addr: int) -> list[int]:  # I1 load with BDCST1
        return [mem[addr], mem[addr]]

    def put(addr: int, words: list[int]) -> None:
        mem[addr], mem[addr + 1] = words

    def fl(words: list[int]) -> list[Fraction]:
        return [f_of(w) for w in words]

    def swap(v: list) -> list:  # `Rn = Sm`: the other element's register
        return [v[1], v[0]]

    m3, m4 = signed32(mem[i1 + FIELDS["m3"]]), signed32(mem[i1 + FIELDS["m4"]])
    offsets = (0, 2, m3, m4)
    frames = [pair(i0 + off) for off in offsets]  # all four loads precede the stores
    peak = [max(abs(f_of(fr[c])) for fr in frames) for c in range(2)]
    level_in, rel = pair(i1 + FIELDS["level"]), pair(i1 + FIELDS["release"])
    decayed = [ext40(f_of(level_in[c]) * f_of(rel[c])) for c in range(2)]
    linked = [max(peak[c], swap(peak)[c]) for c in range(2)]  # stereo-linked peak
    keep = [ext40(f_of(level_in[c]) - decayed[c]) for c in range(2)]
    follow = [ext40(ext40(linked[c] * f_of(rel[c])) + keep[c]) for c in range(2)]
    target, cur = pair(i1 + FIELDS["makeup_target"]), pair(i1 + FIELDS["makeup"])
    k = bcast(i1 + FIELDS["smooth"])
    diff = [ext40(f_of(cur[c]) - f_of(target[c])) for c in range(2)]
    level = [max(linked[c], follow[c]) for c in range(2)]
    diff = [ext40(diff[c] * f_of(k[c])) for c in range(2)]
    exp1 = [(signed32(logb_word(level[c])) + 1) & 0xFFFFFFFF for c in range(2)]
    makeup = [ext40(f_of(target[c]) + diff[c]) for c in range(2)]
    put(i1 + FIELDS["level"], [fb.word(store32(v)) for v in level])
    put(i1 + FIELDS["makeup"], [fb.word(store32(v)) for v in makeup])
    ms = bcast(i1 + FIELDS["mant_scale"])
    thr = pair(i1 + FIELDS["threshold"])  # read after the makeup store (overlap)
    atk = pair(i1 + FIELDS["attack"])
    log2 = [
        ext40(
            float_by(signed32(exp1[c]), 0)
            + float_by(signed32(mant_word(level[c])), signed32(ms[c]))
        )
        for c in range(2)
    ]
    lg = bcast(i1 + FIELDS["log_gain"])
    ex_in = pair(i1 + FIELDS["excess"])
    scaled = [ext40(log2[c] * f_of(lg[c])) for c in range(2)]
    over = [max(scaled[c], f_of(thr[c])) - f_of(thr[c]) for c in range(2)]
    over = [ext40(v) for v in over]
    gt = [over[c] > f_of(ex_in[c]) for c in range(2)]
    coef = [atk[c] if gt[c] else rel[c] for c in range(2)]
    keep_ex = [ext40(f_of(ex_in[c]) - ext40(f_of(ex_in[c]) * f_of(coef[c]))) for c in range(2)]
    over_c = [ext40(over[c] * f_of(coef[c])) for c in range(2)]
    excess = [ext40(over_c[c] + keep_ex[c]) for c in range(2)]
    slope = pair(i1 + FIELDS["slope"])
    red = [ext40(excess[c] * f_of(slope[c])) for c in range(2)]
    red = [max(red[c], swap(red)[c]) for c in range(2)]  # stereo-linked reduction
    gs = bcast(i1 + FIELDS["reduction_scale"])
    red = [ext40(red[c] * f_of(gs[c])) for c in range(2)]
    fx = bcast(i1 + FIELDS["fix_scale"])
    frac = [fix_by_word(red[c], signed32(fx[c])) for c in range(2)]
    put(i1 + FIELDS["excess"], [fb.word(store32(v)) for v in excess])
    t0, t1 = bcast(i1 + FIELDS["table0"]), bcast(i1 + FIELDS["table1"])
    hi = [ssf_word(t0[c], frac[c]) for c in range(2)]
    lo = [ssf_word(t1[c], frac[c]) for c in range(2)]
    gword = [((t0[c] - hi[c]) + lo[c]) & 0xFFFFFFFF for c in range(2)]
    makeup_now = bcast(i1 + FIELDS["makeup"])
    gain = [ext40(f_of(makeup_now[c]) * f_of(gword[c])) for c in range(2)]
    for off, fr in zip(offsets, frames, strict=True):
        y = [fb.word(store32(ext40(f_of(fr[c]) * gain[c]))) for c in range(2)]
        for c in range(2):  # a zero product keeps the XOR of the operand signs
            if y[c] == 0 and (fr[c] >> 31) ^ (gain[c] < 0):
                y[c] = 0x80000000
        put(i0 + off, y)
    return {"gain": gain, "r5": frac, "gt": gt}


def random_scenario(rng: random.Random) -> tuple[list[int], dict[int, int]]:
    """Random input block (8 words) and context words, finite and in plausible ranges."""

    def lu(lo: float, hi: float) -> float:
        return math.exp(rng.uniform(math.log(lo), math.log(hi)))

    inp = []
    for _ in range(8):
        v = 0.0 if rng.random() < 0.1 else lu(1e-3, 8.0)
        inp.append(fb.bits(v * rng.choice([-1, 1])))
    ctx = {o: fb.bits(lu(0.01, 3.0)) for o in range(LO_OFFSET, HI_OFFSET + 1)}
    ctx[FIELDS["m3"]] = rng.choice([4, 4, 2, 6])
    ctx[FIELDS["m4"]] = rng.choice([6, 6, 4, 2])
    ctx[FIELDS["mant_scale"]] = (-31 + rng.choice([0, 0, 0, 1, -1])) & 0xFFFFFFFF
    ctx[FIELDS["fix_scale"]] = rng.choice([20, 24, 28, 22])
    ctx[FIELDS["threshold"]] = fb.bits(rng.uniform(0.0, 4.0))
    ctx[FIELDS["threshold"] + 1] = fb.bits(rng.uniform(0.0, 4.0))
    ctx[FIELDS["table0"]] = 0x3F800000
    ctx[FIELDS["table1"]] = fb.bits(rng.uniform(0.2, 1.0))
    ctx[FIELDS["reduction_scale"]] = fb.bits(-lu(0.1, 3))
    for o in (-0x20, -0x1F, -0x1E, -0x1D, -0x9, -0x8):
        ctx[o] = fb.bits(rng.uniform(0.0, 1.0))
    return inp, ctx


IN_ADDR, CTX_ADDR = 0xB1000, 0xB2040


def oracle_args(image: str, inp: list[int], ctx: dict[int, int]) -> list[str]:
    """`sharc-fx` command line: pass context, one call of the compressor, dump block + context."""
    words = [ctx[o] for o in range(LO_OFFSET, HI_OFFSET + 1)]
    n = HI_OFFSET - LO_OFFSET + 1
    return [
        image, "--dev", "0x28300b00=1", "--dev", "0x3108d008=5", "--trigger", "0xC02C2",
        "--stop-at", "0x12139b", "--max", "200000",
        "--call", hex(COMPRESSOR_PC), "--until", hex(COMPRESSOR_END), "--calls", "1",
        "--set", f"I0={IN_ADDR:#x}", "--set", f"I1={CTX_ADDR:#x}",
        "--set", "M0=0", "--set", "M2=2", "--set", "M9=1",
        "--fill", f"{IN_ADDR:#x}=" + ",".join(f"{w:#x}" for w in inp),
        "--fill", f"{CTX_ADDR + LO_OFFSET:#x}=" + ",".join(f"{w:#x}" for w in words),
        "--dump", f"{IN_ADDR:#x}:8", "--dump", f"{CTX_ADDR + LO_OFFSET:#x}:{n}",
    ]  # fmt: skip


def run_model(inp: list[int], ctx: dict[int, int]) -> dict[int, int]:
    """The model on the same scenario; returns `address -> word` for the dumped ranges."""
    mem = {IN_ADDR + k: w for k, w in enumerate(inp)}
    mem.update({CTX_ADDR + o: w for o, w in ctx.items()})
    compressor_0x120630(mem, IN_ADDR, CTX_ADDR)
    return mem


def check_compressor(image: str, binary: Path, scenarios: int, seed: int) -> tuple[int, int]:
    """`(scenarios with a differing word, scenarios)` of model versus oracle."""
    rng = random.Random(seed)
    bad = 0
    for _ in range(scenarios):
        inp, ctx = random_scenario(rng)
        run = subprocess.run(
            [str(binary), *oracle_args(image, inp, ctx)], capture_output=True, text=True, check=True
        )
        got = fb.parse_dumps(run.stdout)[0]
        want = run_model(inp, ctx)
        bad += any(got[a] != want[a] for a in got)
    return bad, scenarios


def golden_scenario() -> tuple[list[list[int]], dict[int, int]]:
    """Fixed 6-block scenario for the Rust golden test (no RNG: integer formulas only)."""
    blocks = []
    for b in range(6):
        blk = []
        for n in range(8):
            v = ((37 * (8 * b + n) + 11) % 64) / 64.0 - 0.5
            blk.append(fb.bits(v * (1.0 + 3.0 * (b % 3))))
        blocks.append(blk)
    ctx = {o: 0 for o in range(LO_OFFSET, HI_OFFSET + 1)}
    values = {
        "m3": 4, "m4": 6, "level": 0x3C23D70A, "excess": 0,
        "mant_scale": (-31) & 0xFFFFFFFF, "fix_scale": 31, "log_gain": fb.bits(1.0),
        "table0": 0x3F800000, "smooth": fb.bits(0.9908), "attack": fb.bits(0.3),
        "release": fb.bits(0.02), "makeup_target": fb.bits(1.0), "makeup": fb.bits(1.0),
        "threshold": fb.bits(-3.0), "slope": fb.bits(0.5), "reduction_scale": fb.bits(0.4),
        "table1": 0x3F000000,
    }  # fmt: skip
    for name, v in values.items():
        ctx[FIELDS[name]] = v
    # pair fields repeat their word for the right element; `threshold` shares `makeup + 1`
    for name in ("level", "excess", "attack", "release", "makeup_target", "makeup", "slope"):
        ctx[FIELDS[name] + 1] = values[name]
    ctx[FIELDS["threshold"] + 1] = values["threshold"]
    return blocks, ctx


def golden_vectors() -> dict[str, list[int]]:
    """Output words (blocks in order) and the final context words of `golden_scenario`."""
    blocks, ctx = golden_scenario()
    mem = {CTX_ADDR + o: w for o, w in ctx.items()}
    out: list[int] = []
    for blk in blocks:
        mem.update({IN_ADDR + k: w for k, w in enumerate(blk)})
        compressor_0x120630(mem, IN_ADDR, CTX_ADDR)
        out += [mem[IN_ADDR + k] for k in range(8)]
    final = [mem[CTX_ADDR + o] for o in range(LO_OFFSET, HI_OFFSET + 1)]
    return {"input": [w for blk in blocks for w in blk], "output": out, "context": final}


def golden_rust() -> str:
    """`golden_vectors` and the initial context as Rust `const` arrays of `u32` words."""
    vectors = golden_vectors()
    vectors["context_in"] = [golden_scenario()[1][o] for o in range(LO_OFFSET, HI_OFFSET + 1)]
    lines = []
    for name, words in vectors.items():
        lines.append(f"pub(super) const CMP_GOLDEN_{name.upper()}: [u32; {len(words)}] = [")
        lines += [
            "    " + ", ".join(f"{w:#010x}" for w in words[i : i + 6]) + ","
            for i in range(0, len(words), 6)
        ]
        lines.append("];")
    return "\n".join(lines)


def check_golden(image: str, binary: Path) -> bool:
    """The 6-block golden scenario through the oracle (state carried between calls)."""
    blocks, ctx = golden_scenario()
    args = oracle_args(image, blocks[0], ctx)
    args[args.index("--calls") + 1] = str(len(blocks))
    for n, blk in enumerate(blocks):
        args += ["--fill", f"{n}:{IN_ADDR:#x}=" + ",".join(f"{w:#x}" for w in blk)]
    run = subprocess.run([str(binary), *args], capture_output=True, text=True, check=True)
    calls = fb.parse_dumps(run.stdout)
    want = golden_vectors()
    got_out = [c[IN_ADDR + k] for c in calls for k in range(8)]
    got_ctx = [calls[-1][CTX_ADDR + o] for o in range(LO_OFFSET, HI_OFFSET + 1)]
    return got_out == want["output"] and got_ctx == want["context"]


def find_binary(explicit: str | None) -> Path:
    if explicit:
        return Path(explicit)
    root = Path(__file__).resolve().parents[1]
    for name in ("sharc-fx.exe", "sharc-fx"):
        p = root / "target" / "release" / name
        if p.exists():
            return p
    raise SystemExit("sharc-fx not built: cargo build -p nord-sharc --release")


def main(argv: list[str] | None = None) -> int:
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    sub = p.add_subparsers(dest="cmd", required=True)
    ck = sub.add_parser("check", help="model versus oracle on random scenarios")
    ck.add_argument("routine", choices=["compressor"])
    ck.add_argument("image", help="FX Core2 flat SRAM image (tools/ns4_dsp_ldr.py --sram)")
    ck.add_argument("--scenarios", type=int, default=300)
    ck.add_argument("--seed", type=int, default=1)
    ck.add_argument("--bin")
    gd = sub.add_parser("golden", help="print the Rust golden vector (no image needed)")
    gd.add_argument("routine", choices=["compressor"])
    args = p.parse_args(argv)
    if args.cmd == "golden":
        print(golden_rust())
        return 0
    binary = find_binary(args.bin)
    bad, n = check_compressor(args.image, binary, args.scenarios, args.seed)
    print(f"compressor 0x120630: {n - bad}/{n} scenarios bit-identical (outputs and context words)")
    golden = check_golden(args.image, binary)
    print(f"golden scenario (6 blocks, state carried): {'identical' if golden else 'DIFFERENT'}")
    return 0 if bad == 0 and golden else 2


if __name__ == "__main__":
    sys.exit(main())
