#!/usr/bin/env python3
"""Check the FX biquad cascade routine `0x120fdb` of the oracle against a reference model.

Used by docs/research/fx-dsp-map.md section 6. The FX DSP image routine `0x120fdb` (FX Core1 and
Core2, identical code) runs a cascade of three transposed direct form II biquads over four stereo
frames in SIMD (left in PEx, right in PEy), coefficients `b0 b1 b2 a1 a2` read from PM at `I9`
(`0x9437a`), states at `I1 + 0x12 + 4k` (`s1L s1R s2L s2R`), output in place at `I1 + 0xA`.
This script runs it through `sharc-fx` (the oracle) on scripted inputs and states and compares the
result word for word with

    y  = b0 x + s1
    s1 = (b1 x + s2) - a1 y
    s2 = b2 x - a2 y

computed in exact rational arithmetic with every operation rounded to the 40-bit register format
(32-bit mantissa, round to nearest even, `MODE1.RND32 = 0`) in the order of the listing, and every
store to memory keeping the upper 32 bits of the register (truncation toward zero).

    cargo build -p nord-sharc --release
    python tools/fx_biquad_check.py SRAM/core2_sram.bin

`--golden` needs no firmware image: it prints the reference model's results for a fixed input as
Rust source (`crates/nord-core/src/fx/tests_biquad_golden.rs`; derived values only).
"""

from __future__ import annotations

import argparse
import math
import struct
import subprocess
import sys
from fractions import Fraction
from pathlib import Path

FRAMES = 4
STAGES = 3
IN_ADDR, FRAME_ADDR, COEF_ADDR = 0xB1000, 0xB2000, 0x9437A
OUT_ADDR = FRAME_ADDR + 0xA
STATE_ADDR = FRAME_ADDR + 0x12
#: A state word may differ by this many float32 ulp (see `compare`): the stored state is the
#: difference of two nearly equal terms, so a 1-ulp wobble of a 40-bit register shows up there.
STATE_ULP_TOLERANCE = 8


def bits(x: float) -> int:
    return struct.unpack("<I", struct.pack("<f", x))[0]


def from_bits(w: int) -> Fraction:
    return Fraction(struct.unpack("<f", struct.pack("<I", w))[0])


def word(q: Fraction) -> int:
    """Float32 bit pattern of a value that is exactly representable (a stored word)."""
    return bits(float(q))


def round_mantissa(q: Fraction, width: int, nearest: bool) -> Fraction:
    """`q` kept to `width` significant bits: round to nearest even, or truncate toward zero."""
    if q == 0:
        return q
    a = abs(q)
    e = a.numerator.bit_length() - a.denominator.bit_length()
    if Fraction(2) ** e > a:
        e -= 1
    if Fraction(2) ** (e + 1) <= a:
        e += 1
    unit = Fraction(2) ** (e - (width - 1))
    k = a / unit
    m = math.floor(k)
    if nearest:
        rem = k - m
        if rem > Fraction(1, 2) or (rem == Fraction(1, 2) and m % 2 == 1):
            m += 1
    return (m * unit) if q > 0 else -(m * unit)


def ext40(q: Fraction) -> Fraction:
    """A register value: 32-bit mantissa (24 of the IEEE single plus the 8 extension bits)."""
    return round_mantissa(q, 32, True)


def store32(q: Fraction) -> Fraction:
    """What a store of a 40-bit register to a 32-bit memory word keeps (upper 32 bits)."""
    return round_mantissa(q, 24, False)


def biquad_step(
    x: Fraction, coef: tuple[Fraction, ...], state: list[Fraction], rnd=ext40
) -> Fraction:
    """One sample of the firmware's transposed form II (state = [s1, s2]); every operation is
    rounded with `rnd`. Returns the register value `y` (the caller stores it with `store32`)."""
    b0, b1, b2, a1, a2 = coef
    y = rnd(rnd(b0 * x) + state[0])
    t = rnd(rnd(b1 * x) + state[1])
    state[0] = rnd(t - rnd(a1 * y))
    state[1] = rnd(rnd(b2 * x) - rnd(a2 * y))
    return y


def reference_cascade(
    frames: list[tuple[Fraction, Fraction]],
    coef: tuple[Fraction, ...],
    states: list[Fraction],
) -> list[tuple[Fraction, Fraction]]:
    """`len(states) // 4` stages over stereo frames; each stage reads and writes memory words.
    `states` is [s1L, s1R, s2L, s2R] per stage, loaded and stored as 32-bit words once per stage
    and block, and updated in place."""
    out = list(frames)
    for k in range(len(states) // 4):
        st = states[4 * k : 4 * k + 4]
        sl, sr = [st[0], st[2]], [st[1], st[3]]
        out = [
            (store32(biquad_step(left, coef, sl)), store32(biquad_step(right, coef, sr)))
            for (left, right) in out
        ]
        states[4 * k : 4 * k + 4] = [store32(v) for v in (sl[0], sr[0], sl[1], sr[1])]
    return out


def parse_dumps(text: str) -> list[dict[int, int]]:
    """`call N:` blocks of `sharc-fx` output into one {address: word} dict per call."""
    calls: list[dict[int, int]] = []
    for line in text.splitlines():
        if line.startswith("call "):
            calls.append({})
        elif line.startswith("dump ") and calls:
            _, a, w, *_ = line.split()
            calls[-1][int(a, 16)] = int(w, 16)
    return calls


def find_binary(explicit: str | None) -> Path:
    if explicit:
        return Path(explicit)
    root = Path(__file__).resolve().parents[1]
    for name in ("sharc-fx.exe", "sharc-fx"):
        p = root / "target" / "release" / name
        if p.exists():
            return p
    raise SystemExit("sharc-fx not built: cargo build -p nord-sharc --release")


def fill(addr: int, words: list[Fraction], call: int | None = None) -> list[str]:
    prefix = "" if call is None else f"{call}:"
    return ["--fill", f"{prefix}{addr:#x}=" + ",".join(f"{word(w):#x}" for w in words)]


def oracle_args(
    image: str, blocks: list[list[tuple[Fraction, Fraction]]], states: list[Fraction]
) -> list[str]:
    """`sharc-fx` command line for one pass context plus one routine call per input block."""
    a = [
        image,
        "--dev", "0x28300b00=1",
        "--dev", "0x3108d008=5",
        "--trigger", "0xC02C2",
        "--stop-at", "0x12139b",
        "--max", "200000",
        "--call", "0x120fdb",
        "--until", "0x1210b0",
        "--calls", str(len(blocks)),
        "--set", f"I0={IN_ADDR:#x}",
        "--set", f"I1={FRAME_ADDR:#x}",
        "--set", f"I9={COEF_ADDR:#x}",
        "--set", "M0=0", "--set", "M2=2", "--set", "M9=1",
        "--dump", f"{OUT_ADDR:#x}:{2 * FRAMES}",
        "--dump", f"{STATE_ADDR:#x}:{4 * STAGES}",
        "--dump", f"{COEF_ADDR:#x}:5",
    ]  # fmt: skip
    a += fill(STATE_ADDR, states)
    for n, block in enumerate(blocks):
        a += fill(IN_ADDR, [v for frame in block for v in frame], n)
    return a


def ulp_distance(a: int, b: int) -> int:
    """Distance in float32 ulp between two words of the same sign."""
    return abs(a - b) if (a >> 31) == (b >> 31) else 1 << 30


def compare(
    calls: list[dict[int, int]],
    blocks: list[list[tuple[Fraction, Fraction]]],
    states: list[Fraction],
) -> dict[str, int]:
    """Output words must be identical; state words are counted and their worst ulp error kept."""
    coef = tuple(from_bits(calls[0][COEF_ADDR + k]) for k in range(5))
    st = list(states)
    r = {"out_same": 0, "out_total": 0, "st_same": 0, "st_total": 0, "st_worst_ulp": 0}
    for dump, block in zip(calls, blocks):
        want = reference_cascade(block, coef, st)
        for k, v in enumerate(v for frame in want for v in frame):
            r["out_total"] += 1
            r["out_same"] += word(v) == dump[OUT_ADDR + k]
        for k, v in enumerate(st):
            r["st_total"] += 1
            d = ulp_distance(word(v), dump[STATE_ADDR + k])
            r["st_same"] += d == 0
            r["st_worst_ulp"] = max(r["st_worst_ulp"], d)
    return r


def scenario() -> tuple[list[list[tuple[Fraction, Fraction]]], list[Fraction]]:
    """Two input blocks (impulse-like, then a mixed block) and non-zero initial states."""

    def q(x: float) -> Fraction:
        return from_bits(bits(x))  # the float32 word the oracle will read

    raw = [
        [(1.0, 0.5), (0.0, 0.25), (0.0, 0.0), (0.0, -1.0)],
        [(0.3, -0.7), (-0.2, 0.9), (0.05, 0.0), (0.6, 0.4)],
    ]
    blocks = [[(q(a), q(b)) for a, b in blk] for blk in raw]
    states = [q(0.01 * (k + 1) * (-1) ** k) for k in range(4 * STAGES)]
    return blocks, states


#: Golden vector shape: GOLDEN_BLOCKS blocks of FRAMES stereo frames, STAGES stages, `COEF_WORDS`.
GOLDEN_BLOCKS = 16
#: Butterworth low pass at fs/4 as read from PM 0x9437A (b0 b1 b2 a1 a2 as float32 words).
COEF_WORDS = (0x3E95F619, 0x3F15F619, 0x3E95F612, 0, 0x3E2FB0D5)


def golden_vectors() -> dict[str, list[int]]:
    """Float32 words of the reference cascade for the fixed input the Rust test regenerates.

    Input n (0-based) is `((37 n + 11) % 64) / 64 - 0.5` left and `((53 n + 5) % 61) / 64 - 0.5`
    right (exact in float32); initial states are those of `scenario`.
    """
    n_frames = GOLDEN_BLOCKS * FRAMES
    frames = [
        (
            Fraction((37 * n + 11) % 64, 64) - Fraction(1, 2),
            Fraction((53 * n + 5) % 61, 64) - Fraction(1, 2),
        )
        for n in range(n_frames)
    ]
    states = scenario()[1]
    coef = tuple(from_bits(w) for w in COEF_WORDS)
    out: list[tuple[Fraction, Fraction]] = []
    for b in range(GOLDEN_BLOCKS):
        out += reference_cascade(frames[b * FRAMES : (b + 1) * FRAMES], coef, states)
    return {
        "left": [word(o[0]) for o in out],
        "right": [word(o[1]) for o in out],
        "states": [word(v) for v in states],
    }


#: Master IIR (PM 0x945F2..0x94605): 4 sets of `b0 b1 b2 a1 a2` float32 words, read from the FX
#: Core2 flat SRAM image (`python tools/fx_dm_tables.py` style dump, identical on Core1).
MASTER_WORDS = (
    (0x3F0570C0, 0x3E8659B2, 0x3F0570C0, 0xBE3FF6D3, 0x3F414CF5),
    (0x3F27B1AA, 0x3F05CCF3, 0x3F27B1AA, 0xBD44728B, 0x3ECC3FA1),
    (0x3EF6EF68, 0x3F277272, 0x3EF6EF68, 0x3DD7E720, 0x3E223B71),
    (0x3E5FF7D4, 0x3ED59E20, 0x3E5FF7D4, 0x3E54046B, 0x3CEE34FC),
)
MASTER_BLOCKS = 8


def master_golden_vectors() -> dict[str, list[int]]:
    """Reference model of the 4-section master IIR: the sets in order, blocks of FRAMES frames,
    states zero, the same input as `golden_vectors`. Each section reads and writes memory words
    (`store32`) like one stage of `0x120FDB` (the master IIR's own store points are ASSUMED)."""
    frames = [
        (
            Fraction((37 * n + 11) % 64, 64) - Fraction(1, 2),
            Fraction((53 * n + 5) % 61, 64) - Fraction(1, 2),
        )
        for n in range(MASTER_BLOCKS * FRAMES)
    ]
    sets = [tuple(from_bits(w) for w in ws) for ws in MASTER_WORDS]
    states = [[Fraction(0)] * 4 for _ in sets]
    out: list[tuple[Fraction, Fraction]] = []
    for b in range(MASTER_BLOCKS):
        blk = frames[b * FRAMES : (b + 1) * FRAMES]
        for coef, st in zip(sets, states):
            blk = reference_cascade(blk, coef, st)
        out += blk
    return {
        "left": [word(o[0]) for o in out],
        "right": [word(o[1]) for o in out],
        "states": [word(v) for st in states for v in st],
    }


def golden_rust(master: bool = False) -> str:
    """`golden_vectors` (or `master_golden_vectors`) as Rust `const` arrays of `u32` words."""
    lines = []
    vectors = master_golden_vectors() if master else golden_vectors()
    prefix = "MASTER_" if master else ""
    for name, words in vectors.items():
        lines.append(f"pub(super) const {prefix}GOLDEN_{name.upper()}: [u32; {len(words)}] = [")
        lines += [
            "    " + ", ".join(f"{w:#010x}" for w in words[i : i + 6]) + ","
            for i in range(0, len(words), 6)
        ]
        lines.append("];")
    return "\n".join(lines)


def main(argv: list[str] | None = None) -> int:
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    p.add_argument(
        "image", nargs="?", help="FX Core1 or Core2 flat SRAM image (tools/ns4_dsp_ldr.py --sram)"
    )
    p.add_argument("--bin", help="path of sharc-fx (default target/release)")
    p.add_argument("--golden", action="store_true", help="print golden vectors as Rust, no image")
    p.add_argument(
        "--golden-master", action="store_true", help="print the master IIR golden vectors as Rust"
    )
    args = p.parse_args(argv)
    if args.golden or args.golden_master:
        print(golden_rust(master=args.golden_master))
        return 0
    if not args.image:
        p.error("image is required unless --golden is given")
    blocks, states = scenario()
    cmd = [str(find_binary(args.bin))] + oracle_args(args.image, blocks, states)
    run = subprocess.run(cmd, capture_output=True, text=True, check=False)
    if run.returncode != 0:
        print(run.stderr.strip() or run.stdout, file=sys.stderr)
        return 1
    calls = parse_dumps(run.stdout)
    coef = [float(from_bits(calls[0][COEF_ADDR + k])) for k in range(5)]
    print("coefficients b0 b1 b2 a1 a2 =", " ".join(f"{c:.9g}" for c in coef))
    r = compare(calls, blocks, states)
    print(
        f"output words bit-identical {r['out_same']}/{r['out_total']}; "
        f"state words bit-identical {r['st_same']}/{r['st_total']}, worst {r['st_worst_ulp']} ulp"
    )
    ok = r["out_same"] == r["out_total"] and r["st_worst_ulp"] <= STATE_ULP_TOLERANCE
    return 0 if ok else 2


if __name__ == "__main__":
    sys.exit(main())
