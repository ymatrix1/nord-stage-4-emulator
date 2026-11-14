#!/usr/bin/env python3
"""Which layers sound for a decoded program, checked against the ARM zone enable bytes.

`docs/research/keydown-zones.md` section 2.2 found that a note reaches a layer only when its *zone
enable byte* is non-zero, and that the byte comes from the program section table. This tool decodes
the section and layer enables of `.ns4p` files (scene I and II), writes them into the section table
of the booted firmware, lets the layer enable handlers recompute the zone bytes and compares:

    zone byte of layer i  ==  section enable  AND  bit i of the section layer mask

(`docs/research/engine-routing.md` section 2). The decode side needs no firmware; the run needs
`unicorn` and the unpacked firmware:

    uv run --with unicorn python tools/arm_program_routing.py \
        target/fw_unpacked/bank0_CODE.bin target/fw_unpacked/bank0_SRAM.bin \
        --cache target/keydown.snap "factory_programs/Program/Bank A/B3 Chords n Solo.ns4p"
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

HEADER = 48
# Program payload layout (docs/specs/cbin-and-presets.md section 4): global 45 B, organ 135 B,
# piano 147 B, synth 493 B.
GLOBAL, ORGAN, PIANO, SYNTH = 0, 45, 180, 327
SECTION_ENABLE_I, SECTION_ENABLE_II = 284, 287
# Layer enable bits of a section block: (offset, width) of scene I and scene II.
LAYER_ENABLE = {
    "organ": (ORGAN, (9, 2), (12, 2)),
    "piano": (PIANO, (9, 2), (12, 2)),
    "synth": (SYNTH, (10, 3), (15, 3)),
}
# Section enable bit of `SectionEnable_*`: bit 2 organ, bit 1 piano, bit 0 synth.
SECTION_BIT = {"organ": 2, "piano": 1, "synth": 0}
LAYERS = {"organ": (0, 1), "piano": (2, 3), "synth": (4, 5, 6)}


def read_bits(data: bytes, pos: int, width: int) -> int:
    """MSB-first bit field of `width` bits at bit `pos`."""
    value = 0
    for i in range(width):
        bit = (data[(pos + i) // 8] >> (7 - (pos + i) % 8)) & 1
        value = (value << 1) | bit
    return value


def scene_views(payload: bytes) -> dict[int, dict[str, tuple[int, int]]]:
    """`{scene: {section: (enable, layer mask)}}` for scene 0 (I) and 1 (II).

    The layer mask has bit 0 = layer A (B, C above it), the layout of the section table view."""
    views: dict[int, dict[str, tuple[int, int]]] = {0: {}, 1: {}}
    for scene, enable_pos in ((0, SECTION_ENABLE_I), (1, SECTION_ENABLE_II)):
        sections = read_bits(payload, GLOBAL * 8 + enable_pos, 3)
        for name, (base, scene_i, scene_ii) in LAYER_ENABLE.items():
            pos, width = scene_i if scene == 0 else scene_ii
            mask = read_bits(payload, base * 8 + pos, width)
            views[scene][name] = ((sections >> SECTION_BIT[name]) & 1, mask)
    return views


def expected_zone_bytes(enable: int, mask: int, layers: int) -> list[int]:
    """Zone enable bytes of the first `layers` layers of a section (the engine rule)."""
    return [int(bool(enable) and bool((mask >> i) & 1)) for i in range(layers)]


def load_payload(path: Path) -> bytes:
    data = path.read_bytes()
    if len(data) < HEADER + 820:
        raise SystemExit(f"{path}: not a program (868 bytes expected)")
    return data[HEADER : HEADER + 820]


def run_program(emu, stores, bases, payload: bytes) -> list[tuple[int, str, list[int], list[int]]]:  # noqa: ANN001
    """Writes the section views of both scenes, refreshes the zones, returns comparison rows."""
    import arm_edit_capture as aec
    import arm_zones as az

    rows = []
    for scene, sections in scene_views(payload).items():
        for name, (enable, mask) in sections.items():
            view = az.setup_object(emu) + az.SECTION_VIEW + az.SECTION_STRIDE * az.SECTIONS[name]
            for table in (az.SECTION_VIEW, az.SECTION_SCENE_A):
                emu.write32(view + table - az.SECTION_VIEW, enable)
                emu.write32(view + table - az.SECTION_VIEW + 4, mask)
                emu.write32(view + table - az.SECTION_VIEW + 8, 0)
            layers = LAYERS[name]
            for layer in layers:
                aec.apply_value(
                    emu, stores[layer].address, az.LAYER_ENABLE_ID[layer], 1, window=400_000
                )
            got = [int(az.zone_enabled(emu, bases[layer])) for layer in layers]
            rows.append((scene, name, expected_zone_bytes(enable, mask, len(layers)), got))
    return rows


def main(argv: list[str] | None = None) -> int:  # pragma: no cover - thin CLI
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    ap.add_argument("image", type=Path)
    ap.add_argument("sram", type=Path)
    ap.add_argument("programs", type=Path, nargs="+")
    ap.add_argument("--cache", type=Path)
    ns = ap.parse_args(argv)
    sys.path.insert(0, str(Path(__file__).resolve().parent))
    import arm_keydown as kd
    import arm_zones as az

    emu = kd.build_rig(ns.image, ns.sram, ns.cache).emu
    stores, bases = az.first_stores(emu), az.layer_bases(emu)
    bad = 0
    for path in ns.programs:
        for scene, name, want, got in run_program(emu, stores, bases, load_payload(path)):
            ok = want == got
            bad += not ok
            roman = "I" if scene == 0 else "II"
            verdict = "ok" if ok else "MISMATCH"
            print(f"{path.stem:28s} scene {roman:2s} {name:5s} want {want} arm {got} {verdict}")
    print("mismatches:", bad)
    return int(bad > 0)


if __name__ == "__main__":
    sys.exit(main())
