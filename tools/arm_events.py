#!/usr/bin/env python3
"""Panel events of the host firmware: from the scan, through the event queue, to the widgets.

Fourth pass answer to "nothing reads the encoder counters": they are read, by the firmware's UI
event loop. The chain (all addresses VERIFIED by hooks in `Session`, research note s15):

1. The scan decoder `0xC0082A9C` adds +1 / -1 per Gray step to the counter of encoder `index`
   and calls `0xC007AEBC(obj, index, entry, ..)` (acceleration / repeat limiter).
2. That calls the listener `[obj + 8]` (vtable slot 0 = `0xC007B0BC`), which **posts an event** on
   the per-task ring queue `table[0xC05613F4 + [0x8000C000] * 16]` (index 3 for the UI task): a
   64-byte entry `{tag, source id, encoder index, delta}`. A second post with the same tag, source
   and index is merged into the first by adding the deltas (`0xC007B170`), so a slow turn is one
   event with a small delta.
3. The UI task polls the ring in `0xC00A0240..0xC00A03FC`, and calls every subscriber registered
   for the tag (`blx` at `0xC00A035C`). Encoder events carry the tag `0xC053F9CC`, key events
   `0xC053F9D0` (`{tag, 0, key id, 1 / 0}`).
4. The focused screen routes the event (`0xC018933C`: widget tree, then the key table of the
   screen) to a value widget `0xC0137CC4`, which compares the encoder index with the two ids
   `[widget + 0x44]` / `[widget + 0x48]` and calls the step method of the bound parameter
   (`vtable[6]`, `0xC00EEEB0`) with the delta.

Who gets which pair (fifth pass, `docs/research/arm-oled-render.md` s16): encoder index 6
(pair 1) is the **Program dial**; with the modifier word cleared (`arm_screens.edit`) it edits
the value of the selected menu item, with bit 0 set it pages (menus, preset lists, the program
list `Import into A:11`). Index 7 (pair 0) is the **PIANO SELECT** dial: the widget above steps
the piano list whose index vector `[view + 0x34]` is empty while no piano library is loaded
(that was the "empty value container", not a menu parameter). Indices 15, 14, 13 (pairs 2 to 4)
are the three **Synth dials**, active on the Synth waveform page only (`arm_synth_dials.py`).

    uv run --with unicorn python tools/arm_events.py turn target/fw_unpacked/bank0_CODE.bin \
        target/fw_unpacked/bank0_SRAM.bin --state target/oled/state.bin --pair 1 --detents 3

Evidence and limits: `docs/research/arm-oled-render.md` section 15.
"""

from __future__ import annotations

import argparse
import sys
from dataclasses import dataclass
from pathlib import Path

# VERIFIED (hooks during `Session.turn`, research note s15): event tags, queue table and the
# dispatcher call site.
TAG_ENCODER = 0xC053F9CC
TAG_KEY = 0xC053F9D0
QUEUE_TABLE = 0xC05613F4
TASK_INDEX_ADDRESS = 0x8000C000
DISPATCH_CALL = 0xC00A035C
EVENT_BYTES = 64
# VERIFIED: value widget handler and the parameter step method behind it.
VALUE_WIDGET = 0xC0137CC4
PARAM_STEP = 0xC00EEEB0
# VERIFIED (`arm_keys.encoder_steps`, event ids seen at the dispatcher): encoder pair -> index.
ENCODER_PAIR_OF_ID: dict[int, int] = {7: 0, 6: 1, 15: 2, 14: 3, 13: 4}
ID_OF_ENCODER_PAIR: dict[int, int] = {v: k for k, v in ENCODER_PAIR_OF_ID.items()}


# VERIFIED (key-id sweep of research note s16.7: Program 1 events rewritten to each id 0..255 with
# modifier word 0 and 1, `tools/arm_events.py key`): ids seen at the dispatcher and the screens that
# open. `(id, flags)`; `flags` is the modifier word the event carries (1 after any Shift release).
KEY_IDS: dict[str, tuple[int, int]] = {
    "shift": (36, 0),  # event id of the Shift key itself
    "page": (118, 0),
    "store": (145, 0),
    "program1": (146, 0),
    "program2": (147, 0),
    "master_clock": (16, 0),  # popup `Master Clock Tempo` / `120 bpm`
    "transpose": (17, 0),  # popup `Transpose` / `+1 semi`
    "undo": (21, 1),  # dialog `UNDO` / `Nothing to Undo` / `Ok` (flags 0: nothing happens)
}


def patch_key_entry(read32, write32, entry: int, ident_from: int, ident: int, flags: int) -> bool:  # noqa: ANN001
    """Rewrite the key event at `entry` from `ident_from` to `ident` and `flags`."""
    if read32(entry) != TAG_KEY or read32(entry + 8) != ident_from:
        return False
    write32(entry + 8, ident)
    write32(entry + 4, flags)
    return True


def press_as(session, key, ident: int, flags: int, hold: int = 60, after: int = 60):  # noqa: ANN001, ANN201 - needs unicorn
    """Press the matrix key `key` but deliver its events as key id `ident` with `flags`.

    The matrix has no Solo / Undo key, so the router is reached by rewriting the event of a key
    that exists (its id is read from the first event seen) just before the dispatcher call.
    """
    emu = session.emu
    own: list[int] = []

    def on_dispatch(_uc, _addr, _size, _user) -> None:  # noqa: ANN001
        entry = emu.read32(emu.reg(13) + 4)
        if emu.read32(entry) == TAG_KEY:
            if not own:
                own.append(emu.read32(entry + 8))
            patch_key_entry(emu.read32, emu.write32, entry, own[0], ident, flags)

    handle = emu.uc.hook_add(
        emu._u.UC_HOOK_CODE, on_dispatch, begin=DISPATCH_CALL, end=DISPATCH_CALL
    )
    try:
        return session.press([key], hold=hold, after=after)
    finally:
        emu.uc.hook_del(handle)


@dataclass(frozen=True)
class Event:
    """One dispatched event: its tag, the flags word, the id (key or encoder index) and value."""

    tag: int
    flags: int
    ident: int
    value: int

    @property
    def kind(self) -> str:
        return {TAG_ENCODER: "encoder", TAG_KEY: "key"}.get(self.tag, "other")

    @property
    def delta(self) -> int:
        """Signed value of an encoder event (the queue stores it as a 32-bit word)."""
        return self.value - (1 << 32) if self.value >= 1 << 31 else self.value


def decode_event(words: tuple[int, int, int, int]) -> Event:
    """`Event` from the first four words of a queue entry."""
    return Event(*words)


def merge_delta(pending: int, delta: int) -> int:
    """Delta after the queue merged a repeated encoder event (`0xC007B170`: plain addition)."""
    return pending + delta


def queue_depth(head: int, tail: int) -> int:
    """Entries waiting in a ring whose head / tail counters only grow (`tail - head`)."""
    return tail - head


def ring_slot(index: int, mask: int) -> int:
    """Slot of counter value `index` in a ring of `mask + 1` entries."""
    return index & mask


def encoder_pair_for(ident: int) -> int | None:
    """Encoder pair (`arm_keys.ENCODER_PAIRS` index) whose events carry `ident`."""
    return ENCODER_PAIR_OF_ID.get(ident)


def trace_turn(session, pair: int, detents: int):  # noqa: ANN001, ANN201 - needs unicorn
    """Turn encoder `pair` and return `(capture, [Event, ...])` dispatched during it."""
    emu, events = session.emu, []

    def on_dispatch(_uc, _addr, _size, _user) -> None:  # noqa: ANN001
        entry = emu.read32(emu.reg(13) + 4)
        events.append(decode_event(tuple(emu.read32(entry + 4 * i) for i in range(4))))

    handle = emu.uc.hook_add(
        emu._u.UC_HOOK_CODE, on_dispatch, begin=DISPATCH_CALL, end=DISPATCH_CALL
    )
    try:
        cap = session.turn(pair, detents)
    finally:
        emu.uc.hook_del(handle)
    return cap, events


def main(argv: list[str] | None = None) -> int:  # pragma: no cover - needs the local firmware
    import arm_screens as sc

    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    ap.add_argument("cmd", choices=["turn", "key"])
    ap.add_argument("image", type=Path)
    ap.add_argument("sram", type=Path)
    ap.add_argument("--state", type=Path, default=Path("target/oled/state.bin"))
    ap.add_argument("--pair", type=int, default=1)
    ap.add_argument("--detents", type=int, default=3)
    ap.add_argument("--out", type=Path, default=Path("target/oled/turn"))
    ap.add_argument("--name", choices=sorted(KEY_IDS), default="undo", help="cmd key: the id")
    ns = ap.parse_args(argv)
    session = sc.Session(ns.image, ns.sram)
    session.load_state(ns.state.read_bytes())
    if ns.cmd == "key":
        ident, flags = KEY_IDS[ns.name]
        cap = press_as(session, (0x10, 0, 2), ident, flags)
        print("strings:", cap.strings())
        sc.save_capture(cap, ns.out, f"key_{ns.name}")
        return 0
    cap, events = trace_turn(session, ns.pair, ns.detents)
    for ev in events:
        print(f"{ev.kind} id={ev.ident} flags={ev.flags} value={ev.delta}")
    print("strings:", cap.strings())
    sc.save_capture(cap, ns.out, f"pair{ns.pair}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
