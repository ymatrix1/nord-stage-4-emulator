#!/usr/bin/env python3
"""Call the ARM strip apply / ring consumer `0xC01CE7D8` after SAMPLES load, bind and note-on.

Disassembly of `bank0_CODE.bin` shows a single caller of `0xC01CE7D8`: `BL 0xC01CE7D8` at
`0xC0019410` (reached from `0xC00171D8: add r0, r0, #0x68; beq 0xC0019404`), which passes `r0` =
the synth strip object. This tool boots the SAMPLES rig as `tools/arm_sy_slot_probe.py`, captures
the strip object pointer, runs `0xC01CE7D8(strip)` via `call_preserving`, logs every SY/SMP port
message it sends, and reports how many fall in `DM 0x93500..0x935A0`.

    uv run --with unicorn python tools/arm_sy_ring_consumer.py \\
        target/fw_unpacked/bank0_CODE.bin target/fw_unpacked/bank0_SRAM.bin \\
        --cache target/keydown.snap \\
        --ns4b ".reference/Nord Stage 4 Factory Restore RevK v1.66.ns4b" \\
        --member "Samp Lib/Accordion_Harm/Carina Accordion 1_PS 4.1.nsmp4"

Nothing from the backup is written to disk.
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

# VERIFIED (disassembly `0xC01CE7D8`, caller `0xC0019410`, and `tools/synth_p_writer.py`):
# `0xC01CE7D8(strip)` is the strip apply routine; `0xC01CC85C` is the word-0 sender (`r0` = strip).
APPLY = 0xC01CE7D8
SENDER = 0xC01CC85C
WINDOW = (0x93500, 0x935A0)
SY_CHIPS = ("SY", "SMP")


def in_window(dm_word: int) -> bool:
    """True when a DM word address falls in `0x93500..0x935A0`."""
    return WINDOW[0] <= dm_word < WINDOW[1]


def is_sy_or_smp(chip: str) -> bool:
    """True when a port message targets the SY or SMP DSP."""
    return chip in SY_CHIPS


def filter_sy_smp_messages(messages) -> list[tuple[str, int, int, int]]:  # noqa: ANN001
    """Return `(chip, core, dm_word, word2)` for every SY or SMP message in `messages`."""
    return [(m.chip, m.core, m.dm_word, m.word2) for m in messages if is_sy_or_smp(m.chip)]


def count_window_hits(rows: list[tuple[str, int, int, int]]) -> list[tuple[str, int, int, int]]:
    """Return the subset of `(chip, core, dm_word, word2)` rows whose DM address is in `WINDOW`."""
    return [row for row in rows if in_window(row[2])]


def format_message(chip: str, core: int, dm_word: int, word2: int) -> str:
    """Format one SY/SMP port message for console output."""
    return f"  {chip}{core} {dm_word:#07x} <- {word2:#010x}"


def probe(  # pragma: no cover - requires booted firmware and unicorn
    code: Path,
    sram: Path | None,
    cache: Path | None,
    ns4b: Path,
    member: str,
    note: int = 60,
) -> int:
    """Boot the SAMPLES rig, play `note`, call `0xC01CE7D8(strip)`, and log SY/SMP messages."""
    import arm_edit_capture as aec
    import arm_emu as ae
    import arm_piano_load as pl
    import arm_sample_probe as sp
    import arm_steady as ast
    import arm_sy_voice as sv
    import arm_zone_capture as zc
    import unicorn

    data = pl.read_npno(ns4b, member, 0x400000)
    setup = zc.prepare(code, sram, cache)
    emu = setup.emu
    if not any(lo <= aec.SCRATCH <= hi for lo, hi, _ in emu.uc.mem_regions()):
        emu.uc.mem_map(aec.SCRATCH, ae.PAGE)

    strips: list[int] = []
    apply_calls: list[int] = []
    emu.uc.hook_add(
        unicorn.UC_HOOK_CODE,
        lambda uc, *_: strips.append(uc.reg_read(unicorn.arm_const.UC_ARM_REG_R0)),
        begin=SENDER,
        end=SENDER,
    )
    emu.uc.hook_add(
        unicorn.UC_HOOK_CODE,
        lambda uc, *_: apply_calls.append(uc.reg_read(unicorn.arm_const.UC_ARM_REG_R0)),
        begin=APPLY,
        end=APPLY,
    )

    setup.keep({4})
    sp.serve_sample(emu, sp.sample_device_image(data, sp.sample_name(data)))
    if sp.load_sample(emu) != 0:
        raise SystemExit("sample loader failed")
    store = setup.stores[4]
    aec.apply_value(emu, store.address, sp.EDIT_SMP_ID, 1)
    aec.apply_value(emu, store.address, sp.EDIT_MODE, 0)
    sp.bind_layer(emu, 0)
    setup.rig.tick(2)
    setup.freeze()
    setup.reset()
    sv.play_note(setup.rig, note, ticks=60)

    print(f"during load/bind/note-on: SENDER hits={len(strips)}, APPLY hits={len(apply_calls)}")
    if not strips and not apply_calls:
        # Discover strip pointer via a transient wave edit on a scratch snapshot, then restore.
        snap = ast.snapshot(emu)
        cell = aec.value_cell(emu, store.address, 0x127)
        emu.write32(cell, 3 << 16)
        aec.apply_value(emu, store.address, 0x127, 3 << 16, 2000)
        ast.restore(emu, snap)

    strip = apply_calls[-1] if apply_calls else (strips[-1] if strips else 0)
    if not strip:
        raise SystemExit("could not locate strip object pointer")
    print(f"calling 0x{APPLY:08X}(strip=0x{strip:08X})")

    emu.install_dsp_ports()
    emu.clear_messages()
    ret = ast.call_preserving(emu, APPLY, strip)
    rows = filter_sy_smp_messages(emu.messages)
    hits = count_window_hits(rows)

    print(
        f"0x{APPLY:08X} returned {ret:#x}: {len(emu.messages)} total messages, "
        f"{len(rows)} SY/SMP messages, {len(hits)} in {WINDOW[0]:#x}..{WINDOW[1]:#x}"
    )
    for row in rows:
        print(format_message(*row))
    return 0


def main(argv: list[str] | None = None) -> int:  # pragma: no cover - thin CLI
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    ap.add_argument("image", type=Path)
    ap.add_argument("sram", type=Path, nargs="?")
    ap.add_argument("--cache", type=Path)
    ap.add_argument("--ns4b", type=Path, required=True)
    ap.add_argument("--member", required=True)
    ap.add_argument("--note", type=int, default=60, help="MIDI note for the note-on before apply")
    ns = ap.parse_args(argv)
    return probe(ns.image, ns.sram, ns.cache, ns.ns4b, ns.member, ns.note)


if __name__ == "__main__":  # pragma: no cover
    sys.exit(main())
