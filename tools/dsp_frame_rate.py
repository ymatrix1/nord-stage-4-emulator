#!/usr/bin/env python3
"""Checks that settle the DSP frame rate (43,402.34 Hz), see docs/research/dsp-frame-rate.md.

    python tools/dsp_frame_rate.py literal target/fw_unpacked/bank0_CODE.bin
    python tools/dsp_frame_rate.py bank    target/fw_unpacked/bank0_CODE.bin
    python tools/dsp_frame_rate.py delay
    python tools/dsp_frame_rate.py sy
    python tools/dsp_frame_rate.py b3

`literal` finds the ARM sample-rate float words (`fs`, `fs / 2`, `2 fs`), `bank` evaluates the 92
wheel ladder of the Pipe bank at the ARM's default Fine Tune word, `delay` checks the FX delay
words of `docs/research/data/fx-delay-time.csv`, `sy` the SY saw fade constants and `b3` the B3
generator at both fine tune words. Nothing but derived numbers is printed or stored.
"""

from __future__ import annotations

import argparse
import math
import struct
import sys
from pathlib import Path

LOAD_BASE = 0xC0000000
FS = 44_444_000 / 1024  # 43,402.34375, the ARM literal 0x47298A58
C1 = 440.0 * 2 ** (-45 / 12)
C4 = 440.0 * 2 ** (-9 / 12)

# Fine Tune words: the image's default and what the ARM writes at index 0x1F3 (table << 8).
UNITY_TUNE = 0x40000000
ARM_TUNE = 0x3FA02A00
B3_IMAGE_TUNE = 0x3F000000

# B3 generator phase ladder, DM 0x95172..0x9517D of the PNO image (also in b3_wheels.rs).
PHASE_INC = (
    0x31AD21,
    0x34A01A,
    0x37BD05,
    0x3B0C45,
    0x3E8DDA,
    0x424A27,
    0x4638C9,
    0x4A6A87,
    0x4ED6FD,
    0x53868F,
    0x58793D,
    0x5DBFCE,
)

# (display ms, ARM delay word in samples): the exact rows of docs/research/data/fx-delay-time.csv
# (kDlyRateClkDiv 16383, 12288, 3072, 2048, 512) against the DELAY_TIME_MS table, plus the
# constant c[0x16] = 10850 = 250 ms.
DELAY_ROWS = (
    (20.0, 868.0),
    (150.0, 6510.0),
    (625.0, 27126.0),
    (750.0, 32551.0),
    (1250.0, 54252.0),
    (250.0, 10850.0),
)

# SY saw fade words (image 0x12501F prologue): A, B, C.
SY_WORDS = (0x3EB0F34D, 0x3DEBEF11, 0x408AE2EA)

# ARM float words: fs, fs / 2, 2 fs.
ARM_LITERALS = {0x47298A58: "fs", 0x46A98A58: "fs/2", 0x47A98A58: "2 fs"}

TABLE_A = 0xC0443EA8  # detuned pair table, 184 words, value << 8 written to the bank
TABLE_B = 0xC0444188  # exact table


def cents(a: float, b: float) -> float:
    return 1200.0 * math.log2(a / b)


def f32(word: int) -> float:
    return struct.unpack("<f", struct.pack("<I", word))[0]


def fracmul(a: int, b: int) -> int:
    """SHARC `ssfr` multiply of two Q31 words with round to nearest (a, b unsigned words)."""
    sa = a - (1 << 32) if a >= 1 << 31 else a
    sb = b - (1 << 32) if b >= 1 << 31 else b
    return (sa * sb * 2 + (1 << 31)) >> 32


def find_literals(code: bytes) -> list[tuple[int, str]]:
    """Offsets of the ARM sample-rate float words in a code image."""
    out = []
    for off in range(0, len(code) - 3, 4):
        w = struct.unpack_from("<I", code, off)[0]
        if w in ARM_LITERALS:
            out.append((off, ARM_LITERALS[w]))
    return out


def delay_samples(ms: float, fs: float = FS) -> int:
    """Delay word of the ARM: the sample count truncated."""
    return math.floor(ms * fs / 1000.0)


def delay_check(fs: float = FS) -> list[tuple[float, float, int]]:
    """(ms, ARM words, model) rows."""
    return [(ms, words, delay_samples(ms, fs)) for ms, words in DELAY_ROWS]


def sy_implied(words: tuple[int, int, int] = SY_WORDS) -> dict[str, float]:
    """Frame rate implied by the SY fade words: B = 5 kHz / fs, A = 15 kHz / fs, C = 1 / (2 B)."""
    a, b, c = (f32(w) for w in words)
    return {"from_B": 5000.0 / b, "from_A": 15000.0 / a, "from_C": 1e4 * c}


def b3_step(note: int, tune: int) -> int:
    """Phase step of a B3 generator note at a fine tune word (units of 2^-32 cycle per call)."""
    base = (PHASE_INC[note % 12] * tune + (1 << 30)) >> 31
    return (base << (note // 12)) & 0xFFFFFFFF


def b3_check(note: int = 60, frames_per_call: int = 2) -> dict[str, dict[str, float]]:
    """Frame rate that makes `note` sound at its 12-TET pitch (C4 for 60), per fine tune word."""
    f_note = 440.0 * 2 ** ((note - 69) / 12)
    out = {}
    for name, tune in (("image", B3_IMAGE_TUNE), ("arm", ARM_TUNE), ("unity", UNITY_TUNE)):
        step = b3_step(note, tune)
        implied = f_note * frames_per_call * 2**32 / step
        at_fs = step / 2**32 * FS / frames_per_call
        out[name] = {"implied_fs": implied, "hz_at_FS": at_fs, "cents_at_FS": cents(at_fs, f_note)}
    return out


def read_table(code: bytes, addr: int, count: int = 184) -> list[int]:
    off = addr - LOAD_BASE
    return list(struct.unpack_from(f"<{count}I", code, off))


def bank_ladder(table: list[int], tune: int = ARM_TUNE, fs: float = FS) -> dict[str, float]:
    """Deviation in cents of the 92 wheels (pair mean of `v << 8` words) from 12-TET from C1."""
    devs = []
    for k in range(len(table) // 2):
        cyc = sum(fracmul(v << 8, tune) for v in table[2 * k : 2 * k + 2]) / 2 / 2**32
        devs.append(cents(cyc * fs, C1 * 2 ** (k / 12)))
    mean = sum(devs) / len(devs)
    sd = math.sqrt(sum((d - mean) ** 2 for d in devs) / len(devs))
    return {
        "mean": mean,
        "sd": sd,
        "min": min(devs),
        "max": max(devs),
        "wheel45_hz": 440.0 * 2 ** (mean / 1200),
    }


def main(argv: list[str] | None = None) -> int:
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    sub = p.add_subparsers(dest="cmd", required=True)
    for name in ("literal", "bank"):
        sub.add_parser(name).add_argument("code", type=Path, help="bank0_CODE.bin")
    for name in ("delay", "sy", "b3"):
        sub.add_parser(name)
    a = p.parse_args(argv)
    if a.cmd == "literal":
        for off, what in find_literals(a.code.read_bytes()):
            print(f"{what:5s} pool {LOAD_BASE + off:#x}")
    elif a.cmd == "bank":
        code = a.code.read_bytes()
        for name, addr in (("table A (detuned pair)", TABLE_A), ("table B (exact)", TABLE_B)):
            for tune, tn in ((ARM_TUNE, "ARM 0x3FA02A00"), (UNITY_TUNE, "unity 0x40000000")):
                r = bank_ladder(read_table(code, addr), tune)
                m, sd = r["mean"], r["sd"]
                print(f"{name:22s} {tn:17s} mean {m:+.2f} sd {sd:.2f} cent from C1 12-TET")
    elif a.cmd == "delay":
        for ms, words, model in delay_check():
            verdict = "ok" if words == model else "DIFF"
            print(f"{ms:7.1f} ms  ARM {words:8.0f}  model {model:8d}  {verdict}")
    elif a.cmd == "sy":
        for k, v in sy_implied().items():
            print(f"{k}: {v:.3f} Hz")
    elif a.cmd == "b3":
        for k, v in b3_check().items():
            fs_i, hz, c = v["implied_fs"], v["hz_at_FS"], v["cents_at_FS"]
            print(f"{k:6s} implied fs {fs_i:.1f} Hz, note 60 at FS {hz:.3f} Hz ({c:+.2f} cent)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
