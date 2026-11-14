#!/usr/bin/env python3
"""Recover the 29 factory arpeggiator / gate pattern presets from the host firmware.

The preset dial `Edt_Syn_kArpPtnPreset` (edit id `0x114`, range 0..28) is applied by the Glue object
`GlueArpPtnPreset` (vtable method at `0xC010FDAC`, case `[obj+0xEC] == 2`). The method reads the
dial value `v` (`0xC00247E0`, stubbed here), indexes a table of 20-byte structs in `bank0_CODE.bin`
and derives three 32-bit words (16 x 2 bit) that it writes to the pattern fields: `0x118` Dur,
`0x119` Pan, `0x117` Acc (always 0) and `0x116` Length (steps - 1).

    struct(v), v = 1..28, at 0xC02BA860 + 20 * (v - 1):
      +0  name pointer      +4  flag (byte)      +8  steps (8 or 16)
      +12 string A pointer  +16 string B pointer (flag 1 only, else an empty string)

Strings use `S` (short), `L` (long), anything else (`_`) = rest, one character per step. The
firmware turns them into the gate code of each step (`E q Q .`) and, with a B string, a pan word,
so this tool runs the real routine on the ARM emulator (`tools/arm_emu.py`, `unicorn`) instead of
re-implementing the merge. Value 0 is not a table entry: the firmware restores the saved custom
pattern for it (`tools/ns4_arp_gate.py dial`).

    uv run --with unicorn python tools/ns4_arp_presets.py table target/fw_unpacked/bank0_CODE.bin
    uv run --with unicorn python tools/ns4_arp_presets.py rust  target/fw_unpacked/bank0_CODE.bin
    python tools/ns4_arp_presets.py check "<backup>.ns4b" docs/research/arp-preset-table.csv

Evidence and limits: `docs/research/synth-arp-patterns.md`.
"""

from __future__ import annotations

import argparse
import csv
import struct
import sys
from pathlib import Path

LOAD_BASE = 0xC0000000
STRUCT_BASE = 0xC02BA860
STRUCT_SIZE = 20
PRESET_COUNT = 28  # table entries; value 0 is the default pattern
APPLY_ENTRY = 0xC010FF50  # "preset" case of the Glue update method
APPLY_DONE = 0xC010FE00  # words are in r8 (Dur), r6 (Pan), r7 (Acc), `[sp + 4]` = steps - 1
READ_DIAL = 0xC00247E0
FAKE_OBJ = 0x80040000
GATE_CHARS = "EqQ."
PAN_CHARS = "CLR."


def digits(word: int, chars: str, count: int) -> str:
    """Spells the first `count` 2-bit steps of `word` (step 0 = least significant pair)."""
    return "".join(chars[(word >> (2 * i)) & 3] for i in range(count))


def word_from(text: str, chars: str) -> int:
    """Inverse of `digits`: packs a step string into a 2-bit-per-step word."""
    return sum(chars.index(c) << (2 * i) for i, c in enumerate(text))


def cstring(code: bytes, address: int) -> str:
    """NUL-terminated string at `address` in the code image."""
    start = address - LOAD_BASE
    return code[start : code.index(b"\0", start)].decode("latin1")


def read_struct(code: bytes, v: int) -> dict:
    """Raw table entry of preset `v` (1..28): name, flag, steps and the S/L/_ strings."""
    base = STRUCT_BASE - LOAD_BASE + STRUCT_SIZE * (v - 1)
    name, flag, steps, a, b = struct.unpack_from("<IIIII", code, base)
    return {
        "name": cstring(code, name),
        "flag": flag & 0xFF,
        "steps": steps,
        "a": cstring(code, a),
        "b": cstring(code, b),
    }


def emulate_preset(emu, dial: list[int], v: int) -> dict:
    """Runs the firmware routine for dial value `v`; returns steps and the gate / pan words.

    `dial` is the one-element list that the stub of `READ_DIAL` returns (installed once).
    """
    from arm_emu import SENTINEL, STACK_TOP

    dial[0] = v
    emu.write32(FAKE_OBJ + 0xEC, 2)
    emu.set_reg(4, 2)
    emu.set_reg(5, FAKE_OBJ)
    emu.set_reg(13, STACK_TOP - 0x1000)
    emu.set_reg(14, SENTINEL)
    emu.run(APPLY_ENTRY, APPLY_DONE, 200_000)
    sp = emu.reg(13)
    return {
        "steps": emu.read32(sp + 4) + 1,
        "gate": emu.reg(8),
        "pan": emu.reg(6),
        "acc": emu.reg(7),
    }


def recover(code_path: Path, sram_path: Path | None = None) -> list[dict]:
    """The 28 table presets: struct fields plus the emulated gate / pan strings."""
    import arm_emu

    emu = arm_emu.load_firmware(code_path, sram_path)
    code = code_path.read_bytes()
    dial = [0]
    emu.add_stub(READ_DIAL, lambda _e: dial[0])
    rows = []
    for v in range(1, PRESET_COUNT + 1):
        raw = read_struct(code, v)
        out = emulate_preset(emu, dial, v)
        n = out["steps"]
        rows.append(
            {
                "preset": v,
                "steps": n,
                "gate": digits(out["gate"], GATE_CHARS, n),
                "pan": digits(out["pan"], PAN_CHARS, n),
                "accent_word": out["acc"],
                "name": raw["name"],
                "flag": raw["flag"],
                "a": raw["a"],
                "b": raw["b"],
            }
        )
    return rows


def read_csv(path: Path) -> list[dict]:
    """`preset,steps,gate,pan` rows of the committed table."""
    with path.open(newline="", encoding="utf-8") as f:
        return [
            {
                "preset": int(r["preset"]),
                "steps": int(r["steps"]),
                "gate": r["gate"],
                "pan": r["pan"],
            }
            for r in csv.DictReader(line for line in f if not line.startswith("#"))
        ]


def corpus_matches(rows: list[dict], patterns: list[tuple[str, str, dict]]) -> list[dict]:
    """Presets that a factory program holds unedited (same steps, gate and pan, no accent)."""
    found = []
    for r in rows:
        hits = [
            f"{name}/{layer}"
            for name, layer, p in patterns
            if p["steps"] == r["steps"]
            and p["gate"] == r["gate"]
            and p["pan"] == r["pan"]
            and set(p["accent"]) == {"."}
        ]
        gate_only = [
            f"{name}/{layer}"
            for name, layer, p in patterns
            if p["steps"] == r["steps"] and p["gate"] == r["gate"]
        ]
        found.append({"preset": r["preset"], "exact": hits, "gate_only": gate_only})
    return found


def rust_rows(rows: list[dict]) -> str:
    """Rust source lines `preset("gate", "pan"),` for the engine table."""
    return "".join(f'    preset("{r["gate"]}", "{r["pan"]}"),\n' for r in rows)


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)
    for cmd in ("table", "rust"):
        p = sub.add_parser(cmd)
        p.add_argument("code", type=Path, help="bank0_CODE.bin")
        p.add_argument("--sram", type=Path, default=None)
    p = sub.add_parser("check")
    p.add_argument("backup", type=Path, help=".ns4b backup or directory of .ns4y files")
    p.add_argument("csv", type=Path, help="docs/research/arp-preset-table.csv")
    ns = ap.parse_args(argv)
    if ns.cmd == "check":
        import ns4_arp_patterns
        import ns4_bitscan

        patterns = ns4_arp_patterns.scan(ns4_bitscan.load_presets(ns.backup))
        for m in corpus_matches(read_csv(ns.csv), patterns):
            if m["gate_only"]:
                print(m["preset"], "exact:", m["exact"], "gate-only:", len(m["gate_only"]))
        return 0
    rows = recover(ns.code, ns.sram)
    if ns.cmd == "rust":
        sys.stdout.write(rust_rows(rows))
        return 0
    print("preset,steps,gate,pan,flag,name")
    for r in rows:
        print(f"{r['preset']},{r['steps']},{r['gate']},{r['pan']},{r['flag']},{r['name']}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
