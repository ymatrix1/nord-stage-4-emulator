#!/usr/bin/env python3
"""Screen session: boot the host firmware once with panels, EEPROMs and keys, then press keys.

`arm_panel.py` boots to the main loop and only watches. This module keeps the booted machine as a
`Session` (emulator, `PanelLink`, `KeyMatrix`, `I2cBus`) that can be saved to one file, restored in
seconds and driven: `press` holds keys for a number of 1 ms ticks, releases them, and returns the
frames the panels received plus every string the text widgets were given.

The strings come from the text widget's "set text" routine (`0xC007E3AC`; the hook sits at
`0xC007E43C`, after the routine has applied the widget's upper-case flag `[widget + 0x88]` to the
copy in `[widget + 0x54]`), so they are the exact bytes the OLED draws, case included.

    uv run --with unicorn python tools/arm_screens.py boot target/fw_unpacked/bank0_CODE.bin \
        target/fw_unpacked/bank0_SRAM.bin --out target/oled/state.bin
    uv run --with unicorn python tools/arm_screens.py press target/fw_unpacked/bank0_CODE.bin \
        target/fw_unpacked/bank0_SRAM.bin --state target/oled/state.bin --key 0x01,0,3 \
        --out target/oled/press
    uv run --with unicorn python tools/arm_screens.py sweep ... --state target/oled/state.bin

States, frames and texts are firmware output: keep them under `target/` (git-ignored).
Evidence and limits: `docs/research/arm-oled-render.md` sections 6 to 9.
"""

from __future__ import annotations

import argparse
import json
import pickle
import sys
import zlib
from dataclasses import dataclass, field
from pathlib import Path

import arm_i2c as ai
import arm_keys as ak
import arm_oled as ao
import arm_panel as ap

# VERIFIED (disassembly `0xC007E3AC..0xC007E43C`): the text widget's set-text routine copies the
# string to `[widget + 0x54]` (0x40 bytes), upper-cases it when `[widget + 0x88]` is set, then
# lays it out. The hook address is the instruction after the upper-case loop; `r4` is the widget.
TEXT_SET_DONE = 0xC007E43C
WIDGET_TEXT, WIDGET_FONT, WIDGET_UPPER = 0x54, 0x2C, 0x88
TEXT_BYTES = 0x40
LATCH = 0x60000025
# ASSUMED: one tick is 1 ms of virtual time (`arm_steady.INSNS_PER_TICK`).
TICK_INSNS = 200_000
CONTROLLER_MODE_HORIZONTAL = 0  # the firmware's init stream sends `20 00`
# VERIFIED (fifth pass, research s16): the modifier word the key handler keeps (`0xC007B590` clears
# and `0xC007B79C` sets bits; `0xC007B0BC` copies it into the flags word of every encoder event).
# Bit 0 is set after a Shift press and release and stays set; the menu handler `0xC014EEC8` then
# moves the page / slot selection with the Program dial. With bit 0 clear the dial edits the value
# of the selected item (`0xC012DEC0` -> settings adjust `0xC00DFA00`).
MODIFIER_WORD = 0xC05846AC


@dataclass(frozen=True)
class TextEvent:
    """One string given to a text widget: widget address, font object, upper-case flag, text."""

    widget: int
    font: int
    upper: bool
    text: str


@dataclass
class Capture:
    """What the panels and text widgets saw during one `Session.press`."""

    frames: tuple[list[bytes], list[bytes]]
    texts: list[TextEvent] = field(default_factory=list)

    def final(self, display: int) -> bytes | None:
        return self.frames[display][-1] if self.frames[display] else None

    def strings(self) -> list[str]:
        """Distinct strings in the order they were set."""
        seen: list[str] = []
        for t in self.texts:
            if t.text not in seen:
                seen.append(t.text)
        return seen


def distinct(frames: list[bytes]) -> list[bytes]:
    """`frames` without repeats, order kept."""
    out: list[bytes] = []
    for f in frames:
        if f not in out:
            out.append(f)
    return out


def split_regions(blob: bytes) -> tuple[list[tuple[int, int, bytes]], dict]:
    """Decompress a `arm_steady.snapshot` blob once: `(base, size, raw)` regions and the state."""
    regions, state = pickle.loads(zlib.decompress(blob))  # noqa: S301 - our own state file
    return [(b, s, zlib.decompress(c)) for b, s, _perms, c in regions], state


class Session:
    """A booted machine plus the models around it (see the module docstring)."""

    def __init__(self, code: Path, sram: Path) -> None:
        import arm_boot_full as bf

        self.keys = ak.KeyMatrix()
        self.emu = bf.prepare(code, sram, display_stubs=False)
        self.link = ap.install_panel_link(self.emu, self.keys)
        self.bus = ai.install_i2c(self.emu)
        self.texts: list[TextEvent] = []
        self.emu.uc.hook_add(
            self.emu._u.UC_HOOK_CODE, self._on_text, begin=TEXT_SET_DONE, end=TEXT_SET_DONE
        )
        self._saved: tuple[list[tuple[int, int, bytes]], dict, list[bytes]] | None = None

    # ----- text hook -------------------------------------------------------------------------
    def _on_text(self, _uc, _address, _size, _user) -> None:  # noqa: ANN001
        e = self.emu
        w = e.reg(4)
        raw = e.read_bytes(e.read32(w + WIDGET_TEXT), TEXT_BYTES).split(b"\0")[0]
        self.texts.append(
            TextEvent(w, e.read32(w + WIDGET_FONT), bool(e.read_bytes(w + WIDGET_UPPER, 1)[0]),
                      raw.decode("latin-1"))
        )  # fmt: skip

    def load_eeproms(self, blob: bytes) -> None:
        """Preload the four EEPROM images (one 256 KB file) before `boot`."""
        n = ai.EEPROM_SIZE
        for i, eeprom in enumerate(self.bus.eeproms):
            eeprom.data[:] = blob[i * n : (i + 1) * n]

    # ----- boot, save, restore ---------------------------------------------------------------
    def boot(self, max_insns: int = 1_500_000_000) -> int:
        """Run to the main loop (I2C blank storage is formatted on the way); returns the pc."""
        import arm_boot as ab
        import arm_emu as ae
        import arm_steady as st

        self.emu.set_reg(13, ae.STACK_TOP - 0x1000)
        self.emu.set_reg(14, ae.SENTINEL)
        pc, total = ab.APP_THUNK, 0
        while total < max_insns:
            pc, ran, err = st.run_until(self.emu, st.MAIN_LOOP, 50_000_000, pc=pc)
            total += ran
            if pc == st.MAIN_LOOP or err:
                break
            if ap.in_wait_loop(pc):
                st.inject_irq(self.emu)
        st.enable_real_timer(self.emu)
        return pc

    def state_bytes(self) -> bytes:
        import arm_steady as st

        return pickle.dumps((st.snapshot(self.emu), [bytes(e.data) for e in self.bus.eeproms]))

    def load_state(self, data: bytes) -> None:
        """Restore a `state_bytes` blob; later `reset()` calls return to it quickly."""
        blob, eeproms = pickle.loads(data)  # noqa: S301 - our own state file
        regions, state = split_regions(blob)
        self._saved = (regions, state, eeproms)
        self.reset()

    def reset(self) -> None:
        import arm_emu as ae
        import arm_steady as st

        assert self._saved is not None, "load_state first"
        regions, state, eeproms = self._saved
        for base, size, raw in regions:
            try:
                self.emu.uc.mem_write(base, raw)
            except Exception:  # page not mapped in this emulator yet  # noqa: BLE001
                for page in range(base, base + size, ae.PAGE):
                    try:
                        self.emu.uc.mem_map(page, ae.PAGE)
                    except Exception:  # noqa: BLE001, S110
                        pass
                self.emu.uc.mem_write(base, raw)
        for n in range(16):
            self.emu.set_reg(n, state["regs"][n])
        if hasattr(self.emu, "tick_state"):
            self.emu.tick_state.update(state["tick"] if isinstance(state["tick"], dict) else {})
        if hasattr(self.emu, "heap_top"):
            self.emu.heap_top[0] = state["heap"]
        for eeprom, data in zip(self.bus.eeproms, eeproms, strict=True):
            eeprom.data[:] = data
        self.sync_panels()
        self.keys.release_all()
        st.enable_real_timer(self.emu)

    def sync_panels(self) -> None:
        """Make the controller models agree with the restored firmware state.

        The firmware keeps a shadow of the latch byte and skips redundant latch and command
        stores, so a new `PanelLink` has to start from the stored latch and from the controllers'
        horizontal addressing mode (init stream `20 00`), with page and column at 0.
        """
        self.link.latch = self.emu.read_bytes(LATCH, 1)[0]
        for panel in self.link.panels:
            panel.mode = CONTROLLER_MODE_HORIZONTAL
            panel.page = panel.col = 0
            panel._burst = 0  # noqa: SLF001

    # ----- driving ---------------------------------------------------------------------------
    def run(self, ticks: int) -> None:
        import arm_steady as st

        st.run_ticking(self.emu, ticks * TICK_INSNS)

    def press(
        self, keys: list[tuple[int, int, int]], hold: int = 40, after: int = 40, before: int = 20
    ) -> Capture:
        """Hold `keys` for `hold` ticks, release, run `after` more ticks; report what changed."""
        self.run(before)
        n = [len(self.link.frames(0)), len(self.link.frames(1))]
        t0 = len(self.texts)
        for k in keys:
            self.keys.press(*k)
        self.run(hold)
        for k in keys:
            self.keys.release(*k)
        self.run(after)
        frames = (self.link.frames(0)[n[0] :], self.link.frames(1)[n[1] :])
        return Capture(frames, self.texts[t0:])

    def turn(
        self,
        pair: int,
        detents: int,
        hold_keys: list[tuple[int, int, int]] | None = None,
        dwell: int = 30,
        after: int = 40,
    ) -> Capture:
        """Turn encoder `pair` (see `arm_keys.ENCODER_PAIRS`) by `detents`, with `hold_keys` down.

        Each Gray state is held `dwell` ticks so the scan (4 reads per sweep) sees every edge.
        """
        n = [len(self.link.frames(0)), len(self.link.frames(1))]
        t0 = len(self.texts)
        for k in hold_keys or []:
            self.keys.press(*k)
        self.run(20)
        for state in ak.encoder_steps(pair, detents):
            for k in [k for k in self.keys.held if k[0] == ak.ENCODER_STROBE]:
                self.keys.release(*k)
            for k in state:
                self.keys.press(*k)
            self.run(dwell)
        self.run(after)
        for k in hold_keys or []:
            self.keys.release(*k)
        self.run(after)
        frames = (self.link.frames(0)[n[0] :], self.link.frames(1)[n[1] :])
        return Capture(frames, self.texts[t0:])


def edit(session: Session, pair: int, detents: int, **kwargs) -> Capture:  # noqa: ANN003
    """Turn encoder `pair` as an edit: the modifier word is cleared first (see `MODIFIER_WORD`).

    Without that, a menu opened with Shift + Program n keeps bit 0 set after Shift is released and
    the Program dial pages instead of changing the value (research note s16.1).
    """
    session.emu.write32(MODIFIER_WORD, 0)
    return session.turn(pair, detents, **kwargs)


def parse_key(text: str) -> tuple[int, int, int]:
    """`strobe,port,bit` (numbers may be hex) to a key tuple."""
    strobe, port, bit = (int(x, 0) for x in text.split(","))
    return strobe, port, bit


def save_capture(cap: Capture, out: Path, name: str, scale: int = 2) -> None:
    """Write the last frame of each display (PNG and ASCII) and the strings (JSON)."""
    out.mkdir(parents=True, exist_ok=True)
    for i in (0, 1):
        frame = cap.final(i)
        if frame is not None:
            (out / f"{name}_d{i}.png").write_bytes(ao.to_png(frame, scale=scale))
            (out / f"{name}_d{i}.txt").write_text(ao.to_ascii(frame))
    rows = [{"font": hex(t.font), "upper": t.upper, "text": t.text} for t in cap.texts]
    (out / f"{name}.json").write_text(json.dumps(rows, indent=1))


def cmd_boot(code: Path, sram: Path, out: Path, eeprom: Path | None = None) -> int:
    session = Session(code, sram)
    if eeprom is not None:
        session.load_eeproms(eeprom.read_bytes())
    pc = session.boot()
    print(f"boot: pc={pc:#x}, I2C transactions {len(session.bus.log)}")
    session.run(100)  # let the first screens settle
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_bytes(session.state_bytes())
    for i in (0, 1):
        frames = distinct(session.link.frames(i))
        print(f"display {i}: {len(session.link.frames(i))} frames, {len(frames)} distinct")
        if frames:
            (out.parent / f"boot_d{i}.png").write_bytes(ao.to_png(frames[-1], scale=3))
    return 0 if pc else 1


def cmd_press(code: Path, sram: Path, state: Path, keys: list[str], out: Path, hold: int) -> int:
    session = Session(code, sram)
    session.load_state(state.read_bytes())
    cap = session.press([parse_key(k) for k in keys], hold=hold)
    save_capture(cap, out, "press")
    print("strings:", cap.strings())
    return 0


SHIFT_KEY = (0x02, 1, 4)  # EMPIRICAL (sweep): Shift + Program n opens the menus


def cmd_sweep(
    code: Path, sram: Path, state: Path, out: Path, hold: int, shift: bool = False
) -> int:
    session = Session(code, sram)
    session.load_state(state.read_bytes())
    summary = {}
    for key in ak.all_keys():
        if shift and key == SHIFT_KEY:
            continue
        session.reset()
        cap = session.press([key, SHIFT_KEY] if shift else [key], hold=hold)
        name = "%02x_%d_%d" % key
        summary[name] = cap.strings()
        if cap.texts or cap.frames[0] or cap.frames[1]:
            save_capture(cap, out, name)
        print(name, cap.strings()[:8], flush=True)
    (out / "summary.json").write_text(json.dumps(summary, indent=1))
    return 0


def main(argv: list[str] | None = None) -> int:  # pragma: no cover - needs the local firmware
    ap_ = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawTextHelpFormatter
    )
    sub = ap_.add_subparsers(dest="cmd", required=True)
    for name in ("boot", "press", "sweep"):
        p = sub.add_parser(name)
        p.add_argument("image", type=Path)
        p.add_argument("sram", type=Path)
        p.add_argument("--out", type=Path, default=Path("target/oled"))
        if name != "boot":
            p.add_argument("--state", type=Path, default=Path("target/oled/state.bin"))
            p.add_argument("--hold", type=int, default=40, help="ticks the keys stay down")
        if name == "sweep":
            p.add_argument("--shift", action="store_true", help="hold Shift with every key")
        if name == "boot":
            p.add_argument("--eeprom", type=Path, help="initial 256 KB EEPROM image (arm_store.py)")
        if name == "press":
            p.add_argument("--key", action="append", required=True, help="strobe,port,bit")
    ns = ap_.parse_args(argv)
    if ns.cmd == "boot":
        return cmd_boot(ns.image, ns.sram, ns.out, ns.eeprom)
    if ns.cmd == "press":
        return cmd_press(ns.image, ns.sram, ns.state, ns.key, ns.out, ns.hold)
    return cmd_sweep(ns.image, ns.sram, ns.state, ns.out, ns.hold, ns.shift)


if __name__ == "__main__":
    sys.exit(main())
