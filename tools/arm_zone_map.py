#!/usr/bin/env python3
"""Key range of a layer from its `Edt_*_kKbZone` code and the split state (firmware port).

`docs/research/keydown-zones.md` s8 left the map from the 12 zone codes to the key range open: the
range setters (vtable slots 2 and 3, `0xC00C843C` / `0xC00C83FC`) have no static callers. They are
called by `0xC010ABA4` (the zone-range update of a layer store, reached by the `kKbZone` and
`Edt_Com_kSplit*` edit ids), which asks two pure functions for the lower and upper bound:

* `0xC01B0DC4(cfg, code, sel, out)` lower bound, `0xC01B0C40(cfg, code, sel, out)` upper bound,
  `cfg` = the split state object `[0xC059B334]` (flags `+0xD0..+0xD3`, positions `+0xD4..+0xDC`,
  width settings `+0xE0..+0xE8`, lowest / highest key `+0xEC` / `+0xF0`);
* `0xC01B0A44(cfg, k)` resolves the position setting of point `k` against its active neighbours.

Both bound functions look the code up in the byte pair table `0xC02CB230` (index
`(code + 12 * mask) * 2 + (sel == 1)`, `mask` = bit 0 Low, bit 1 Mid, bit 2 High point active) and
switch on the case (0..10). This tool holds the same logic as a pure model, checks it against the
ARM functions (`--check`) and prints a digest that the Rust table (`engine/zone_table.rs`) pins.

    uv run --with unicorn python tools/arm_zone_map.py --check target/fw_unpacked/bank0_CODE.bin \
        target/fw_unpacked/bank0_SRAM.bin --cache target/keydown.snap

Evidence: `docs/research/zone-key-ranges.md`.
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

# VERIFIED (ARM data read, `--check`): bytes of the case table `0xC02CB230`, `[mask][code][sel]`.
# `mask` bit 0 = Low point active, bit 1 = Mid, bit 2 = High (split enabled AND point flag). Both
# `sel` columns are equal except for code 11, where `sel == 1` gives case 10 (empty range).
CASE_TABLE: tuple[tuple[tuple[int, int], ...], ...] = (
    tuple((9, 9) for _ in range(11)) + ((9, 10),),
    ((0, 0),) * 5 + ((9, 9),) + ((8, 8),) * 5 + ((8, 10),),
    ((4, 4),) * 5 + ((9, 9),) + ((6, 6),) * 5 + ((6, 10),),
    (
        (0, 0),
        (1, 1),
        (4, 4),
        (4, 4),
        (4, 4),
        (9, 9),
        (8, 8),
        (8, 8),
        (8, 8),
        (1, 1),
        (6, 6),
        (6, 10),
    ),
    ((7, 7),) * 5 + ((9, 9),) + ((3, 3),) * 5 + ((3, 10),),
    (
        (0, 0),
        (5, 5),
        (7, 7),
        (7, 7),
        (7, 7),
        (9, 9),
        (8, 8),
        (8, 8),
        (8, 8),
        (5, 5),
        (3, 3),
        (3, 10),
    ),
    (
        (4, 4),
        (2, 2),
        (7, 7),
        (7, 7),
        (7, 7),
        (9, 9),
        (6, 6),
        (6, 6),
        (6, 6),
        (2, 2),
        (3, 3),
        (3, 10),
    ),
    (
        (0, 0),
        (1, 1),
        (4, 4),
        (5, 5),
        (7, 7),
        (9, 9),
        (8, 8),
        (5, 5),
        (6, 6),
        (2, 2),
        (3, 3),
        (3, 10),
    ),
)
# VERIFIED (jump tables `0xC01B0E20` lower / `0xC01B0C9C` upper): per case, the bound kind. Lower:
# `min` (lowest key), `P0..P2` (first key of point 0..2) or `none`; upper: `max`, `P0..P2` (last key
# below point 0..2, i.e. note - 1) or `none` (range 0..1).
LOWER_KIND = ("min", "P0", "P1", "P2", "min", "P0", "P1", "min", "P0", "min", "none")
UPPER_KIND = ("P0", "P1", "P2", "max", "P1", "P2", "max", "P2", "max", "max", "none")
# VERIFIED (ARM data `0xC02D8370`, `0xC02D839C`, `0xC02D8368`): split note per position setting,
# crossfade half width in keys per width setting, lowest / highest key of the split state.
SPLIT_NOTES = (36, 41, 48, 53, 60, 65, 72, 77, 84, 89, 96)
FADE_KEYS = (0, 6, 12)
LOWEST_KEY, HIGHEST_KEY = 21, 108
# VERIFIED (ARM data `0xC0569018` / `0xC0569048`, 16.16 gains, `--check`): the crossfade curves of
# width settings 1 (12 entries) and 2 (24 entries); setting 0 is the single entry 0x10000.
FADE_12 = (
    0x72B,
    0xC8B,
    0x15C2,
    0x24DD,
    0x3BE7,
    0x5B22,
    0x8000,
    0xB810,
    0xDE35,
    0xF1A9,
    0xFA1C,
    0xFDB2,
)
FADE_24 = (
    0x560, 0x76C, 0x9FB, 0xD91, 0x126E, 0x1893, 0x20C4, 0x2B02, 0x37CE, 0x46E9, 0x5851, 0x6BC6,
    0x8000, 0x9FBE, 0xBBE7, 0xD22D, 0xE24D, 0xED4F, 0xF439, 0xF8D4, 0xFBA5, 0xFD70, 0xFE76, 0xFEF9,
)  # fmt: skip
UNITY = 0x10000


def mask_of(enabled: bool, flags: tuple[bool, bool, bool]) -> int:
    """Case table row: split enabled AND point flag, bit `i` = point `i`."""
    return sum(1 << i for i, f in enumerate(flags) if enabled and f)


def split_index(flags: tuple[bool, bool, bool], pos: tuple[int, int, int], k: int) -> int:
    """Port of `0xC01B0A44`: position setting of point `k` limited by its active neighbours."""
    act = [i for i in range(3) if flags[i]]
    ip = pos[k]
    lo, hi = 0, 10
    if len(act) >= 2 and flags[k]:
        if len(act) == 2:
            a, b = act
            if k == a:
                hi = max(pos[b] - 1, 0)
            else:
                lo = min(pos[a] + 1, 10)
        elif k == 0:
            hi = 0 if pos[1] - 1 < 0 else min(pos[1] - 1, 8)
        elif k == 1:
            lo = 1 if pos[0] + 1 <= 0 else min(pos[0] + 1, 9)
            hi = 1 if pos[2] - 1 <= 0 else min(pos[2] - 1, 9)
        else:
            lo = 2 if pos[1] + 1 <= 1 else min(pos[1] + 1, 10)
    return lo if ip < lo else min(ip, hi)


def bound(kind: str, upper: bool, flags, pos, widths) -> tuple[int, int, int]:
    """`(key, fade width setting, curve length)` of one bound kind (`0xC01B0*` out struct)."""
    if kind == "min":
        return LOWEST_KEY, 0, 1
    if kind == "max":
        return HIGHEST_KEY, 0, 1
    if kind == "none":
        return (1 if upper else 0), 0, 1
    k = int(kind[1])
    note = SPLIT_NOTES[split_index(flags, pos, k)] - (1 if upper else 0)
    w = widths[k]
    return note, w, (1, 12, 24)[w]


def zone_range(code, enabled, flags, pos, widths, sel=0):
    """Lower and upper `(key, width setting, curve length)` of a layer: the setter arguments."""
    case = CASE_TABLE[mask_of(enabled, flags)][code][1 if sel == 1 else 0]
    return (
        bound(LOWER_KIND[case], False, flags, pos, widths),
        bound(UPPER_KIND[case], True, flags, pos, widths),
    )


def fade(width: int, index: int) -> int:
    """16.16 gain entry `index` of the curve of width setting `width`."""
    if width == 0:
        return UNITY
    return (FADE_12 if width == 1 else FADE_24)[index]


def bound_gain(rng: tuple[int, int, int], upper: bool, note: int) -> int:
    """Port of `0xC00C84B8` (16.16 gain of one bound struct at `note`)."""
    key, w, _n = rng
    half = FADE_KEYS[w]
    if half == 0:
        inside = note <= key if upper else note >= key
        return UNITY if inside else 0
    if upper:
        if note < key + 1 - half:
            return UNITY
        return 0 if note > key + half else fade(w, key + half - note)
    if note < key - half:
        return 0
    return UNITY if note > key + half - 1 else fade(w, note - (key - half))


def note_gain(lower, upper, note: int) -> int:
    """Gain handed to the layer key object (`0xC00C859C`): the smaller of the two bound gains,
    or 0 when the note is outside `[lower key - width, upper key + width]`."""
    lo = lower[0] - FADE_KEYS[lower[1]]
    hi = upper[0] + FADE_KEYS[upper[1]]
    if note < lo or note > hi:
        return 0
    return min(bound_gain(lower, False, note), bound_gain(upper, True, note))


def digest() -> int:
    """FNV-1a 64 over the model outputs; pinned by `engine/zone_table.rs`.

    Stream: the case table, then for every mask / code / sel, 18 position triples and
    the three width settings, the lower and upper `(key, width)` and the gain of 8 notes."""
    h = 0xCBF29CE484222325

    def eat(v: int) -> None:
        nonlocal h
        h = ((h ^ (v & 0xFFFFFFFF)) * 0x100000001B3) & 0xFFFFFFFFFFFFFFFF

    for row in CASE_TABLE:
        for pair in row:
            eat(pair[0] * 16 + pair[1])
    triples = [(a, b, c) for a in (0, 5, 10) for b in (0, 5, 10) for c in (0, 8)]
    for m in range(8):
        flags = (bool(m & 1), bool(m & 2), bool(m & 4))
        for code in range(12):
            for sel in (0, 1):
                for pos in triples:
                    for w in range(3):
                        lo, up = zone_range(code, True, flags, pos, (w, w, w), sel)
                        for v in (*lo[:2], *up[:2]):
                            eat(v)
                        for note in (21, 40, 53, 59, 60, 65, 66, 108):
                            eat(note_gain(lo, up, note))
    return h


def check(emu) -> int:  # pragma: no cover - needs the firmware
    """Compares the model with the ARM functions; returns the number of mismatches."""
    import arm_edit_capture as aec
    import arm_steady as ast

    if not any(b <= aec.SCRATCH <= e for b, e, _ in emu.uc.mem_regions()):
        emu.uc.mem_map(aec.SCRATCH, 0x1000)
    cfg = emu.read32(0xC059B334)
    out = aec.SCRATCH + 0x100
    bad = 0

    def put(flags, pos, widths) -> None:
        emu.write_bytes(cfg + 0xD0, bytes([1, *map(int, flags)]))
        for i in range(3):
            emu.write32(cfg + 0xD4 + 4 * i, pos[i])
            emu.write32(cfg + 0xE0 + 4 * i, widths[i])

    # tables
    data = [(emu.read32(0xC02D8370 + 4 * i)) for i in range(11)]
    bad += data != list(SPLIT_NOTES)
    bad += [emu.read32(0xC02D839C + 4 * i) for i in range(3)] != list(FADE_KEYS)
    bad += [emu.read32(0xC0569018 + 4 * i) for i in range(12)] != list(FADE_12)
    bad += [emu.read32(0xC0569048 + 4 * i) for i in range(24)] != list(FADE_24)
    for m in range(8):
        for c in range(12):
            for s in range(2):
                raw = emu.read32(0xC02CB230 + ((c + 12 * m) * 8) + 4 * s)
                bad += raw != CASE_TABLE[m][c][s]
    # split_index, exhaustive
    for m in range(8):
        flags = (bool(m & 1), bool(m & 2), bool(m & 4))
        for pos in ((a, b, c) for a in range(11) for b in range(11) for c in range(11)):
            put(flags, pos, (0, 0, 0))
            for k in range(3):
                got = ast.call_preserving(emu, 0xC01B0A44, cfg, k)
                bad += got != split_index(flags, pos, k)
    # bound functions
    triples = [(1, 4, 7), (0, 0, 0), (10, 10, 10), (3, 3, 3), (2, 9, 5), (6, 1, 8), (4, 4, 4)]
    for m in range(8):
        flags = (bool(m & 1), bool(m & 2), bool(m & 4))
        for pos in triples:
            for w in range(3):
                put(flags, pos, (w, w, w))
                for code in range(12):
                    for sel in (0, 1):
                        ast.call_preserving(emu, 0xC01B0DC4, cfg, code, sel, out)
                        lo = [emu.read32(out + 4 * i) for i in range(4)]
                        ast.call_preserving(emu, 0xC01B0C40, cfg, code, sel, out + 0x20)
                        up = [emu.read32(out + 0x20 + 4 * i) for i in range(4)]
                        want = zone_range(code, True, flags, pos, (w, w, w), sel)
                        got = ((lo[0], lo[3]), (up[0], up[3]))
                        mine = tuple((r[0], r[2]) for r in want)
                        if got != mine:
                            bad += 1
                            print("range", m, pos, w, code, sel, got, mine)
    print("mismatches:", bad)
    return bad


def main(argv: list[str] | None = None) -> int:  # pragma: no cover - thin CLI
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    ap.add_argument("--check", type=Path, nargs="*", help="CODE SRAM images to check against")
    ap.add_argument("--cache", type=Path)
    ns = ap.parse_args(argv)
    print(f"digest {digest():#018x}")
    if ns.check:
        sys.path.insert(0, str(Path(__file__).resolve().parent))
        import arm_keydown as kd

        rig = kd.build_rig(ns.check[0], ns.check[1] if len(ns.check) > 1 else None, ns.cache)
        return int(check(rig.emu) > 0)
    return 0


if __name__ == "__main__":
    sys.exit(main())
