#!/usr/bin/env python3
"""Verify value-scale tables used by the engine against the OS v1.66 firmware image.

Covers two things:

* the LFO-to-pitch depth curve (128 `f32`, semitones) stored in ``bank0_CODE.bin`` at
  ``0x2ca2c0`` and embedded as ``LFO_PITCH_SEMITONES`` in ``crates/nord-core/src/synth_tables.rs``;
* the 128-step *display string* tables (``"0.5 ms"``, ``"748 ms"``, ``"32 kHz"`` ...) that the
  ARM firmware keeps as pointer arrays at ``0x53abe4..0x53b3e4`` and that our
  ``ENV_*_TABLE``, ``FILTER_CUTOFF_HZ_TABLE`` and ``LFO_RATE_HZ_TABLE`` must reproduce within the
  rounding of the displayed digits.

    python tools/ns4_fw_tables.py target/fw_unpacked/bank0_CODE.bin
"""

from __future__ import annotations

import argparse
import math
import re
import struct
import sys
from pathlib import Path

LFO_PITCH_OFFSET = 0x2CA2C0
TABLE_LEN = 128
LOAD_BASE = 0xC0000000
DEFAULT_RS = Path(__file__).resolve().parents[1] / "crates/nord-core/src/synth_tables.rs"
PRESETS_RS = Path(__file__).resolve().parents[1] / "crates/nord-core/src/cbin/presets.rs"

# (label, pointer-table offset in bank0_CODE.bin, Rust table name, "Inf" means >= 44 s)
DISPLAY_TABLES = (
    ("env attack", 0x53ABE4, "ENV_ATTACK_MS_TABLE"),
    ("env decay", 0x53ADE4, "ENV_DECAY_MS_TABLE"),
    ("env release", 0x53AFE4, "ENV_RELEASE_MS_TABLE"),
    ("filter cutoff", 0x53B1E4, "FILTER_CUTOFF_HZ_TABLE"),
    ("lfo rate", 0x53B3E4, "LFO_RATE_HZ_TABLE"),
)
UNIT_FACTOR = {"ms": 1.0, "s": 1000.0, "Hz": 1.0, "kHz": 1000.0}


def read_f32_table(image: bytes, offset: int, length: int = TABLE_LEN) -> list[float]:
    if offset + 4 * length > len(image):
        raise ValueError(f"table at 0x{offset:x} (+{length} f32) is outside the image")
    return list(struct.unpack_from(f"<{length}f", image, offset))


def parse_rust_table(source: str, name: str) -> list[float]:
    """Values of ``pub const NAME: [f32; N] = [ ... ];``."""
    alias = re.search(rf"pub const {name}: \[f32; \d+\] = ([A-Z0-9_]+);", source)
    if alias:
        return parse_rust_table(source, alias.group(1))
    m = re.search(rf"pub const {name}: \[f32; (\d+)\] = \[(.*?)\];", source, re.S)
    if not m:
        raise ValueError(f"{name} not found")
    values = [float(x) for x in re.findall(r"-?\d+(?:\.\d+)?(?:e-?\d+)?", m.group(2))]
    if len(values) != int(m.group(1)):
        raise ValueError(f"{name}: expected {m.group(1)} values, found {len(values)}")
    return values


def display_string(image: bytes, pointer: int) -> str:
    """The NUL-terminated (max 8 byte) string at the absolute address `pointer`."""
    off = pointer - LOAD_BASE
    if not 0 <= off < len(image) - 8:
        raise ValueError(f"pointer 0x{pointer:08x} is outside the image")
    raw = image[off : off + 8].split(b"\0")[0]
    return raw.decode("latin1")


def parse_display(text: str) -> tuple[float, float]:
    """`(value, rounding tolerance)` of a display string, in the table's unit (ms or Hz).

    ``"748 ms"`` -> ``(748.0, 0.5)``, ``"0.03 Hz"`` -> ``(0.03, 0.005)``, ``"Inf"`` -> ``(inf, 0)``.
    """
    if text == "Inf":
        return math.inf, 0.0
    m = re.fullmatch(r"(\d+(?:\.(\d+))?) ?(ms|s|Hz|kHz)", text)
    if not m:
        raise ValueError(f"unparseable display string {text!r}")
    decimals = len(m.group(2) or "")
    factor = UNIT_FACTOR[m.group(3)]
    return float(m.group(1)) * factor, 0.5 * 10.0 ** (-decimals) * factor


def check_display_table(
    image: bytes, offset: int, ours: list[float]
) -> list[tuple[int, str, float]]:
    """Entries of the 128-pointer display table at `offset` that disagree with `ours`."""
    bad: list[tuple[int, str, float]] = []
    for i in range(TABLE_LEN):
        (ptr,) = struct.unpack_from("<I", image, offset + 4 * i)
        text = display_string(image, ptr)
        value, tol = parse_display(text)
        ok = ours[i] >= 44_000.0 if math.isinf(value) else abs(value - ours[i]) <= tol * 1.01
        if not ok:
            bad.append((i, text, ours[i]))
    return bad


def check_display_tables(
    image: bytes, presets_source: str
) -> dict[str, list[tuple[int, str, float]]]:
    return {
        label: check_display_table(image, off, parse_rust_table(presets_source, name))
        for label, off, name in DISPLAY_TABLES
    }


def max_abs_error(a: list[float], b: list[float]) -> float:
    if len(a) != len(b):
        raise ValueError("table length mismatch")
    return max(abs(x - y) for x, y in zip(a, b, strict=True))


def check_lfo_pitch(image: bytes, rust_source: str) -> tuple[float, bool]:
    """`(max abs error in semitones, firmware table is monotonic 0..48)`."""
    fw = read_f32_table(image, LFO_PITCH_OFFSET)
    ours = parse_rust_table(rust_source, "LFO_PITCH_SEMITONES")
    monotonic = all(x <= y for x, y in zip(fw, fw[1:])) and fw[0] == 0.0 and fw[-1] == 48.0
    return max_abs_error(fw, ours), monotonic


def main(argv: list[str] | None = None) -> int:
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    p.add_argument("firmware", type=Path, help="bank0_CODE.bin")
    p.add_argument("--rust", type=Path, default=DEFAULT_RS)
    p.add_argument("--presets-rs", type=Path, default=PRESETS_RS)
    p.add_argument("--lfo-only", action="store_true", help="skip the display-string table checks")
    args = p.parse_args(argv)
    image = args.firmware.read_bytes()
    err, mono = check_lfo_pitch(image, args.rust.read_text(encoding="utf-8"))
    print(
        f"LFO_PITCH_SEMITONES vs firmware @0x{LFO_PITCH_OFFSET:x}: "
        f"max err {err:.2e}, monotonic 0..48: {mono}"
    )
    bad = 0
    if args.lfo_only:
        return 0 if mono and err < 1e-5 else 1
    tables = check_display_tables(image, args.presets_rs.read_text(encoding="utf-8"))
    for label, mismatches in tables.items():
        print(
            f"{label}: {TABLE_LEN - len(mismatches)}/{TABLE_LEN} entries match the display strings"
        )
        for i, text, mine in mismatches[:5]:
            print(f"  step {i}: firmware {text!r} vs ours {mine}")
        bad += len(mismatches)
    return 0 if mono and err < 1e-5 and not bad else 1


if __name__ == "__main__":
    sys.exit(main())
