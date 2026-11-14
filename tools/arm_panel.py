#!/usr/bin/env python3
"""Run the host firmware with its OLED panels attached and capture what it draws (local only).

The ARM harness (`tools/arm_boot_full.py`, `tools/arm_steady.py`) never saw a display because the
board bring-up `0xC024B484` skips the display setup unless both panel boards answer. They answer
through EMIFA CS2: the detection routine `0xC024D648` writes `1` then `2` to `0x60000015` and reads
bit 7 of `0x60000010` (panel 0) / `0x60000020` (panel 1). `install_panel_link` returns `0x80` for
those two reads inside the routine, records every store to `0x6000....` and feeds it to
`arm_oled.PanelLink`, which rebuilds the controller RAM. Two more things are needed:

* the boot blocks in `0xC0075D0C` (post a request, spin until it is served by another context), so
  the timer interrupt is injected while the program counter sits in that loop (`WAIT_LOOP`);
* `TEXT_MEASURE` / `EVENT_INSERT` run unstubbed (`arm_boot_full.prepare(display_stubs=False)`):
  the display objects and fonts they read now exist.

    uv run --with unicorn python tools/arm_panel.py capture target/fw_unpacked/bank0_CODE.bin \
        target/fw_unpacked/bank0_SRAM.bin --out target/oled --steady 150000000
    uv run --with unicorn python tools/arm_panel.py oracle target/fw_unpacked/bank0_CODE.bin
    python tools/arm_panel.py fonts target/fw_unpacked/bank0_CODE.bin

Frames and fonts are copyrighted firmware output: write them under `target/` (git-ignored) only.
Key presses, the saved machine and the widget-text log live in `tools/arm_screens.py`.
Evidence and limits: `docs/research/arm-oled-render.md`.
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

import arm_oled as ao
from arm_keys import KeyMatrix

# VERIFIED (disassembly `0xC024D648..0xC024D70C`): panel presence read. Panel 0 answers on
# `0x60000010`, panel 1 on `0x60000020` (bit 7 = present, bit 6 selects the second answer value).
DETECT_RANGE = (0xC024D648, 0xC024D710)
PANEL_STATUS = (0x60000010, 0x60000020)
PRESENT = 0x80
CS2_BASE, CS2_SIZE = 0x60000000, 0x1000
# VERIFIED (run): `0xC0075D0C` posts a request (`0xC00A05D4`) and spins on `[req + 0x1C]` until the
# interrupt-time context serves it; without an injected timer interrupt the boot never returns.
WAIT_LOOP = (0xC0075D38, 0xC0075D50)
# Scratch buffers for the oracle, inside the emulator's 16 MB RAM window past the firmware's BSS.
TEXT_BUF, PLANE_BUF, MASK_BUF = 0xC0F10000, 0xC0F20000, 0xC0F21000


def panel_answer(pc: int, address: int) -> int:
    """Value a read of the panel status registers returns: present inside the detection routine."""
    in_detect = DETECT_RANGE[0] <= pc < DETECT_RANGE[1]
    return PRESENT if in_detect and address in PANEL_STATUS else 0


def in_wait_loop(pc: int) -> bool:
    return WAIT_LOOP[0] <= pc <= WAIT_LOOP[1]


def install_panel_link(emu, keys: KeyMatrix | None = None) -> ao.PanelLink:  # noqa: ANN001
    """Attach both panels: detection answers, store capture, controller model.

    With `keys`, the panel status ports answer the key matrix outside the detection routine
    (idle high, held keys low) and the strobe stores are fed to it; without it they answer 0.
    """
    link = ao.PanelLink()
    try:
        emu.uc.mem_map(CS2_BASE, CS2_SIZE)
    except Exception:  # already mapped by the poll breaker or an earlier call  # noqa: BLE001, S110
        pass

    def on_read(uc, _access, address, _size, _value, _user) -> None:  # noqa: ANN001
        base = address & ~3
        if base not in PANEL_STATUS:
            return
        block = panel_block(emu.reg(15), base, keys)
        uc.mem_write(base, block)

    def on_write(_uc, _access, address, _size, value, _user) -> None:  # noqa: ANN001
        link.write(address, value)
        if keys is not None:
            keys.write(address, value)

    emu.uc.hook_add(emu._u.UC_HOOK_MEM_READ, on_read, begin=CS2_BASE, end=CS2_BASE + CS2_SIZE - 1)
    emu.uc.hook_add(emu._u.UC_HOOK_MEM_WRITE, on_write, begin=CS2_BASE, end=CS2_BASE + CS2_SIZE - 1)
    return link


def panel_block(pc: int, base: int, keys: KeyMatrix | None) -> bytes:
    """The four bytes of the panel status port at `base` for a read executed at `pc`."""
    answer = panel_answer(pc, base)
    if answer or keys is None:
        return bytes([answer, 0, 0, 0])
    first = keys.read(base)
    return bytes([0 if first is None else first, 0xFF, 0xFF, 0xFF])


def boot_with_panels(emu, max_insns: int = 1_500_000_000) -> tuple[int, int, int]:  # noqa: ANN001
    """Run the application thunk to the main loop, serving the wait loop with timer interrupts.

    Returns `(pc, instructions, interrupts injected)`.
    """
    import arm_boot as ab
    import arm_emu as ae
    import arm_steady as st

    emu.set_reg(13, ae.STACK_TOP - 0x1000)
    emu.set_reg(14, ae.SENTINEL)
    pc, total, irqs = ab.APP_THUNK, 0, 0
    while total < max_insns:
        pc, ran, err = st.run_until(emu, st.MAIN_LOOP, 50_000_000, pc=pc)
        total += ran
        if pc == st.MAIN_LOOP or err:
            break
        if in_wait_loop(pc):
            st.inject_irq(emu)
            irqs += 1
    return pc, total, irqs


def oracle_emulator(code: Path):  # noqa: ANN201 - ArmEmu
    """Firmware image only (no boot): enough to run the pure draw / measure routines."""
    import arm_emu as ae

    return ae.load_firmware(code, None)


def firmware_draw(
    emu, font: int, text: str, x: int = 0, y: int = 0, limit: int = ao.WIDTH
) -> bytes:  # noqa: ANN001
    """Render `text` through the firmware's own `0xC004064C` into a cleared 128 x 64 buffer."""
    emu.write_bytes(TEXT_BUF, text.encode("latin-1") + b"\0")
    emu.write_bytes(PLANE_BUF, bytes(ao.FRAME_BYTES))
    emu.write_bytes(MASK_BUF, b"\xff" * ao.FRAME_BYTES)
    emu.call(ao.DRAW_TEXT, TEXT_BUF, font, PLANE_BUF, MASK_BUF, x, y, limit)
    return bytes(emu.read_bytes(PLANE_BUF, ao.FRAME_BYTES))


def firmware_width(emu, font: int, text: str) -> int:  # noqa: ANN001
    emu.write_bytes(TEXT_BUF, text.encode("latin-1") + b"\0")
    return emu.call(ao.TEXT_MEASURE, TEXT_BUF, font) & 0xFFFFFFFF


def cmd_fonts(code: Path) -> int:
    image = code.read_bytes()
    print("idx addr        height first last  widest glyph")
    for i, f in enumerate(ao.font_table(image)):
        print(
            f"{i:3d} {f.addr:#x}  {f.height:5d} {f.first:#04x}  {f.last:#04x}  "
            f"{max(map(len, f.glyphs))}"
        )
    return 0


def cmd_oracle(code: Path) -> int:
    """Compare the reference rasteriser and width with the firmware routines on every font."""
    image = code.read_bytes()
    emu = oracle_emulator(code)
    sample = "Nord Stage 4 A:11 OS v1.66 0123456789 Hello, World!"
    bad = 0
    for f in [*ao.font_table(image), ao.parse_font(image, ao.CRASH_FONT)]:
        text = "".join(c for c in sample if f.has(ord(c)))
        mine = bytearray(ao.FRAME_BYTES)
        ao.draw_text(mine, f, text, 0, 0)
        theirs = firmware_draw(emu, f.addr, text, 0, 0)
        w_fw, w_me = firmware_width(emu, f.addr, text), ao.text_width(f, text)
        ok = bytes(mine) == theirs and w_fw == w_me
        bad += not ok
        print(
            f"{f.addr:#x} h={f.height:2d} width fw={w_fw:3d} ref={w_me:3d} "
            f"pixels {'==' if bytes(mine) == theirs else '!='}"
        )
    print("oracle:", "all equal" if not bad else f"{bad} fonts differ")
    return int(bool(bad))


def cmd_capture(code: Path, sram: Path, out: Path, steady: int, stubs: bool) -> int:
    import arm_boot_full as bf
    import arm_i2c as ai
    import arm_steady as st

    emu = bf.prepare(code, sram, display_stubs=stubs)
    link = install_panel_link(emu, KeyMatrix())  # idle matrix: no key held
    ai.install_i2c(emu)  # program memory: without it the Program screen shows `Write error`
    pc, n, irqs = boot_with_panels(emu)
    print(f"boot: pc={pc:#x} after {n} instructions, {irqs} timer interrupts in the wait loop")
    if pc == st.MAIN_LOOP and steady:
        st.enable_real_timer(emu)
        pc, err = st.run_ticking(emu, steady)
        print(f"steady: {steady} instructions, pc={pc:#x} {err!r}")
    out.mkdir(parents=True, exist_ok=True)
    for i in (0, 1):
        frames = link.frames(i, unique=True)
        print(f"display {i}: {len(link.frames(i))} frames, {len(frames)} distinct")
        for k, fr in enumerate(frames):
            (out / f"d{i}_{k:03d}.png").write_bytes(ao.to_png(fr, scale=3))
    return 0


def main(argv: list[str] | None = None) -> int:  # pragma: no cover - needs the local firmware
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)
    p = sub.add_parser("fonts")
    p.add_argument("image", type=Path)
    p = sub.add_parser("oracle")
    p.add_argument("image", type=Path)
    p = sub.add_parser("capture")
    p.add_argument("image", type=Path)
    p.add_argument("sram", type=Path)
    p.add_argument("--out", type=Path, default=Path("target/oled"))
    p.add_argument("--steady", type=int, default=0, help="instructions to run after the main loop")
    p.add_argument(
        "--display-stubs",
        action="store_true",
        help="keep the old TEXT_MEASURE / EVENT_INSERT no-ops",
    )
    ns = ap.parse_args(argv)
    if ns.cmd == "fonts":
        return cmd_fonts(ns.image)
    if ns.cmd == "oracle":
        return cmd_oracle(ns.image)
    return cmd_capture(ns.image, ns.sram, ns.out, ns.steady, ns.display_stubs)


if __name__ == "__main__":
    sys.exit(main())
