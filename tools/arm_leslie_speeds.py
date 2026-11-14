#!/usr/bin/env python3
"""Leslie rotor speeds of the ARM firmware and the DSP speed words they become.

The rotor state block of the organ part (`0x9115D2B0` in the settled harness, doubles) holds, per
section, the current and target speed, the acceleration steps and the slow and fast speed in Hz
(`+0x48 / +0x50` the horn section, `+0xB0 / +0xB8` the rotor section; `docs/research/
arm-part-activation.md`). The periodic update `0xC02096E4` / `0xC0209728` converts the current speed
to a float and calls `0xC020D68C`, which sends `int(hz * 8388607 / 11025) << 8` to the DSP row
(`0xB2F26` horn, `0xB2F25` rotor); `0xB2FA1` follows the horn word.

    uv run --with unicorn python tools/arm_leslie_speeds.py target/fw_unpacked/bank0_CODE.bin \\
        target/fw_unpacked/bank0_SRAM.bin --cache settled.snap
"""

from __future__ import annotations

import argparse
import struct
import sys
from pathlib import Path

#: VERIFIED (run): the ARM law of `0xC020D68C`, `word = int(hz * 8388607 / 11025) << 8`; the
#: values `0x26000` (0.8 Hz), `0x22B00` (0.73 Hz) and the ramp step words `0x28E00`, `0x23300`
#: (0.86, 0.74 Hz) are reproduced by it.
SCALE = 8388607.0 / 11025.0
#: EMPIRICAL: `0xB2FA1 = int(HORN_FOLLOW * horn_hz * SCALE) << 8` fits the three words observed
#: (`0x15D00` at 0.80 Hz, `0x17800` at 0.86 Hz): the factor lies in [0.57456, 0.57502].
HORN_FOLLOW = 0.5748


def speed_word(hz: float) -> int:
    """The DSP word for a rotor speed of `hz` (nominal, `fs / 4 = 11025 Hz`)."""
    return int(hz * SCALE) << 8


def words(horn_hz: float, rotor_hz: float) -> dict[int, int]:
    """`{DM address: word}` of the three speed words for the current horn and rotor speeds."""
    return {
        0xB2F25: speed_word(rotor_hz),
        0xB2F26: speed_word(horn_hz),
        0xB2FA1: speed_word(horn_hz * HORN_FOLLOW),
    }


def hz_of_word(word: int, fs: float = 44_444_000 / 1024) -> float:
    """The speed a word means at the real DSP frame rate (one call = 2 frames, `2^32` phase)."""
    return (word >> 8) * 256 / 2**32 * fs / 2


def read_block(read_bytes, base: int = 0x9115D2B0) -> dict[str, float]:
    """The doubles of the rotor state block from a memory reader `read_bytes(address, n)`."""
    names = {
        0x08: "horn_now", 0x10: "horn_target", 0x18: "horn_up", 0x28: "horn_down",
        0x48: "horn_slow", 0x50: "horn_fast",
        0x70: "rotor_now", 0x78: "rotor_target", 0x80: "rotor_up", 0x90: "rotor_down",
        0xB0: "rotor_slow", 0xB8: "rotor_fast",
    }  # fmt: skip
    return {n: struct.unpack("<d", read_bytes(base + o, 8))[0] for o, n in names.items()}


def main(argv: list[str] | None = None) -> int:  # pragma: no cover - firmware harness
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    ap.add_argument("image", type=Path)
    ap.add_argument("sram", type=Path)
    ap.add_argument("--cache", type=Path, required=True)
    ns = ap.parse_args(argv)
    sys.path.insert(0, str(Path(__file__).resolve().parent))
    import fx_clients as fc

    emu, _ = fc.fx_snapshot(ns.image, ns.sram, ns.cache)
    blk = read_block(emu.read_bytes)
    print(blk)
    for label, h, r in (
        ("slow", blk["horn_slow"], blk["rotor_slow"]),
        ("fast", blk["horn_fast"], blk["rotor_fast"]),
    ):
        print(label, {hex(a): hex(w) for a, w in words(h, r).items()})
    return 0


if __name__ == "__main__":
    sys.exit(main())
