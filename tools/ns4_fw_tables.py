#!/usr/bin/env python3
"""Verify value-scale tables used by the engine against the OS v1.66 firmware image.

Currently covers the LFO-to-pitch depth curve (128 `f32`, semitones) stored in
``bank0_CODE.bin`` at ``0x2ca2c0`` and embedded as ``LFO_PITCH_SEMITONES`` in
``crates/nord-core/src/synth_tables.rs``.

    python tools/ns4_fw_tables.py target/fw_unpacked/bank0_CODE.bin
"""

from __future__ import annotations

import argparse
import re
import struct
import sys
from pathlib import Path

LFO_PITCH_OFFSET = 0x2CA2C0
TABLE_LEN = 128
DEFAULT_RS = Path(__file__).resolve().parents[1] / "crates/nord-core/src/synth_tables.rs"


def read_f32_table(image: bytes, offset: int, length: int = TABLE_LEN) -> list[float]:
    if offset + 4 * length > len(image):
        raise ValueError(f"table at 0x{offset:x} (+{length} f32) is outside the image")
    return list(struct.unpack_from(f"<{length}f", image, offset))


def parse_rust_table(source: str, name: str) -> list[float]:
    """Values of ``pub const NAME: [f32; N] = [ ... ];``."""
    m = re.search(rf"pub const {name}: \[f32; (\d+)\] = \[(.*?)\];", source, re.S)
    if not m:
        raise ValueError(f"{name} not found")
    values = [float(x) for x in re.findall(r"-?\d+(?:\.\d+)?(?:e-?\d+)?", m.group(2))]
    if len(values) != int(m.group(1)):
        raise ValueError(f"{name}: expected {m.group(1)} values, found {len(values)}")
    return values


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
    args = p.parse_args(argv)
    err, mono = check_lfo_pitch(args.firmware.read_bytes(), args.rust.read_text(encoding="utf-8"))
    print(
        f"LFO_PITCH_SEMITONES vs firmware @0x{LFO_PITCH_OFFSET:x}: "
        f"max err {err:.2e}, monotonic 0..48: {mono}"
    )
    return 0 if mono and err < 1e-5 else 1


if __name__ == "__main__":
    sys.exit(main())
