#!/usr/bin/env python3
"""SAMPLES note-on in the booted host firmware: run the voice update `0xC01E00AC` end to end.

Evidence and tables: `docs/research/synth-sample-kernel-rule.md` section 12. After `arm_sample_probe`
has loaded and bound a sample (layer table index 0), a MIDI note-on posts two events to the handler
queue `0x9062E330` (class `0xC02FDB40`). Nothing in the harness drains that queue, so the voice
update (slot 1) never ran. This tool plays the part of the consumer and of one DSP status bit:

* `pump` pops the queue head-first, `[handler + 8]` head index, `[handler + 4]` count, items at
  `[handler + 0x10 + 4 i]` (ring of `0x200`), and calls the event class thunk `[[event]]`
  (`0xC01DB3EC` slot 2, `0xC01DB690` slot 0, `0xC01DBA54` slot 1) as `thunk(event, handler)`,
  which is what the drain loop `0xC01DEFEC` does with `blx`.
* `DspAck`: the voice update waits (`0xC01E0694`, `0xC01E06A4`) for the DSP to set bit `[r5 + 8]` in
  the status word `[r6 + 4 [r5 + 4]]` (`r6 = 0xC05B0454`) after it sent the prepare message; the
  harness has no DSP, so the bit is set before the test, which takes the claim branch
  `0xC01E09C8` (ASSUMED meaning: "SY voice ready").

    uv run --with unicorn python tools/arm_sy_voice.py target/fw_unpacked/bank0_CODE.bin \
        target/fw_unpacked/bank0_SRAM.bin --cache target/keydown.snap \
        --ns4b ".reference/Nord Stage 4 Factory Restore RevK v1.66.ns4b" \
        --member "Samp Lib/Accordion_Harm/Carina Accordion 1_PS 4.1.nsmp4" --notes 48 59 60 72 96

Nothing from the backup is written to disk.
"""

from __future__ import annotations

import argparse
import sys
from dataclasses import dataclass, field
from pathlib import Path

import arm_edit_capture as aec
import arm_emu as ae
import arm_keydown as kd
import arm_piano_load as pl
import arm_steady as ast
import arm_sy_kernel_rule as kr
import arm_sy_pitch_law as law
import arm_zone_capture as zc

# VERIFIED (run, section 12): the handler instance and its class; the event queue is the handler
# itself (`[+4]` count, `[+8]` head, `[+0xC]` tail, items from `+0x10`, `0x200` slots).
HANDLER = 0x9062E330
HANDLER_VTABLE = 0xC02FDB40
QUEUE_COUNT, QUEUE_HEAD, QUEUE_ITEMS, QUEUE_SIZE = 4, 8, 0x10, 0x200
VOICE_UPDATE = 0xC01E00AC
# VERIFIED (listing): the wait for the DSP status bit, and the two instructions after the voice
# KEY and TRANSPOSE stores (`str r3, [r4, #8]` at `0xC01E0A00`, `str r1, [r4, #0x18]` at `0xC01E0A10`).
ACK_PCS = (0xC01E0694, 0xC01E06A4)
KEY_STORE_DONE, TRANSPOSE_STORE_DONE = 0xC01E0A04, 0xC01E0A14
# VERIFIED (listing `0xC01E14AC..0xC01E15A4`): voice object fields the pitch block reads.
PITCH_FIELDS = {
    "key": 0x08,
    "transpose": 0x18,
    "cents_a": 0x14,
    "cents_b": 0x1C,
    "fine": 0x74,
    "layer": 0x34,
}
LAYER_FIELDS = {"tune": 0x10, "fixed_key": 0x60, "mode": 0x6C}
# VERIFIED (run): messages of one SAMPLES note-on (DM word addresses of SY voice 0).
STATE_WORD, STATE_RESET, STATE_PREPARE = 0x92414, 0x1C0794, 0x1C07CD
KERNEL_WORD = 0x9243B
PITCH_WORD = 0x921A6  # EMPIRICAL: pair block word of voice 0 in this run (table `0xC05482F8`)
GAIN_WORDS = (0x92420, 0x92421)
# VERIFIED (run, `capture_note` for keys 48, 59, 60, 72, 96): record words written before prepare,
# offsets from `0x92400` (`+0x0A, +0x0B` ... `+0x13`); roles are EMPIRICAL (section 12).
FIELD_BASE, FIELD_RANGE = 0x92400, range(0x0A, 0x14)


@dataclass
class VoiceMessages:
    """The DSP messages of one SAMPLES note-on, split by role."""

    kernel_base: int | None = None
    pitch_bits: int | None = None
    gains: tuple[int, ...] = ()
    fields: dict[int, int] = field(default_factory=dict)
    reset: bool = False
    prepared: bool = False

    @property
    def kernel(self) -> int | None:
        """Kernel number of the record write `0x9243B`, `None` without a write."""
        return None if self.kernel_base is None else kr.kernel_index(self.kernel_base)


def decode_messages(pairs: list[tuple[int, int]]) -> VoiceMessages:
    """Sort `(dm_word, value)` pairs into kernel base, pitch word, gains and record fields."""
    out = VoiceMessages()
    gains: list[int] = []
    for word, value in pairs:
        if word == KERNEL_WORD:
            out.kernel_base = value
        elif word == PITCH_WORD:
            out.pitch_bits = value
        elif word in GAIN_WORDS:
            gains.append(value)
        elif word == STATE_WORD and value == STATE_RESET:
            out.reset = True
        elif word == STATE_WORD and value == STATE_PREPARE:
            out.prepared = True
        elif word - FIELD_BASE in FIELD_RANGE:
            out.fields[word - FIELD_BASE] = value
    out.gains = tuple(gains)
    return out


def queued_events(emu: ae.ArmEmu, handler: int = HANDLER) -> list[int]:
    """Event pointers in the handler queue, head first."""
    head = emu.read32(handler + QUEUE_HEAD)
    return [
        emu.read32(handler + QUEUE_ITEMS + 4 * ((head + i) % QUEUE_SIZE))
        for i in range(emu.read32(handler + QUEUE_COUNT))
    ]


def pop_event(emu: ae.ArmEmu, handler: int = HANDLER) -> int:
    """Remove and return the head event of the queue (the bookkeeping of `0xC01DEFEC`)."""
    event = queued_events(emu, handler)[0]
    emu.write32(handler + QUEUE_HEAD, (emu.read32(handler + QUEUE_HEAD) + 1) % QUEUE_SIZE)
    emu.write32(handler + QUEUE_COUNT, emu.read32(handler + QUEUE_COUNT) - 1)
    return event


def pump(emu: ae.ArmEmu, handler: int = HANDLER, max_insns: int = 300_000) -> list[int]:
    """Deliver every queued event: `[[event]]` is the thunk that calls the handler slot."""
    delivered: list[int] = []
    while emu.read32(handler + QUEUE_COUNT):
        event = pop_event(emu, handler)
        thunk = emu.read32(emu.read32(event))
        ast.call_preserving(emu, thunk, event, handler, max_insns=max_insns)
        delivered.append(event)
    return delivered


def ack_address(r5: int, r6: int, read32) -> tuple[int, int]:  # noqa: ANN001
    """`(status word address, mask)` the voice update waits on: `r6 + 4 [r5 + 4]`, `[r5 + 8]`."""
    return r6 + 4 * read32(r5 + 4), read32(r5 + 8)


@dataclass
class VoiceCapture:
    """What one note-on left: the stored voice fields, the pitch inputs and the DSP messages."""

    note: int
    key_store: int | None = None
    transpose_store: int | None = None
    pitch_inputs: dict[str, int] = field(default_factory=dict)
    layer_inputs: dict[str, int] = field(default_factory=dict)
    pitch_bits: int | None = None
    messages: VoiceMessages = field(default_factory=VoiceMessages)
    delivered: int = 0


def _signed(value: int) -> int:
    return value - (1 << 32) if value >= 1 << 31 else value


def pitch_from_inputs(pitch_emu: ae.ArmEmu, cap: VoiceCapture) -> int:
    """Run the real pitch instructions (`arm_sy_pitch_law`) on the captured inputs."""
    p, lay = cap.pitch_inputs, cap.layer_inputs
    return law.run_block(
        pitch_emu,
        p["key"],
        transpose=_signed(p["transpose"]),
        cents=_signed(p["cents_a"]) + _signed(p["cents_b"]),
        fine=_signed(p["fine"]),
        tune=_signed(lay["tune"]),
        fixed_key=None if lay["mode"] == 1 else lay["fixed_key"],
    )


def play_note(rig: kd.Rig, note: int, velocity: int = 100, ticks: int = 40) -> VoiceCapture:
    """Send a note-on, run `ticks`, deliver the queue with the DSP bit set, return the capture."""
    emu = rig.emu
    cap = VoiceCapture(note)

    def ack(_uc, _a, _s, _u) -> None:  # noqa: ANN001
        address, mask = ack_address(emu.reg(5), emu.reg(6), emu.read32)
        emu.write32(address, emu.read32(address) | mask)

    def key_done(_uc, _a, _s, _u) -> None:  # noqa: ANN001
        cap.key_store = emu.reg(3)

    def transpose_done(_uc, _a, _s, _u) -> None:  # noqa: ANN001
        cap.transpose_store = emu.reg(1)

    def pitch_in(_uc, _a, _s, _u) -> None:  # noqa: ANN001
        obj = emu.reg(6)
        cap.pitch_inputs = {k: emu.read32(obj + o) for k, o in PITCH_FIELDS.items()}
        layer = cap.pitch_inputs["layer"]
        cap.layer_inputs = {k: emu.read32(layer + o) for k, o in LAYER_FIELDS.items()}

    def pitch_out(_uc, _a, _s, _u) -> None:  # noqa: ANN001
        cap.pitch_bits = emu.reg(0)

    hooks = [(pc, ack) for pc in ACK_PCS]
    hooks += [(KEY_STORE_DONE, key_done), (TRANSPOSE_STORE_DONE, transpose_done)]
    hooks += [(law.BLOCK_START, pitch_in), (law.BLOCK_END, pitch_out)]
    handles = [emu.uc.hook_add(emu._u.UC_HOOK_CODE, f, begin=pc, end=pc) for pc, f in hooks]
    try:
        rig.send(kd.note_on(note, velocity))
        sent = [(m.dm_word, m.word2) for _t, m in rig.tick(ticks)]
        emu.clear_messages()
        cap.delivered = len(pump(emu))
        sent += [(m.dm_word, m.word2) for m in emu.messages]
        cap.messages = decode_messages(sent)
    finally:
        for handle in handles:
            emu.uc.hook_del(handle)
    return cap


def boot_sample_rig(code: Path, sram: Path | None, cache: Path | None, data: bytes) -> tuple:
    """Settled rig with synth layer A (table index 0) switched to SAMPLES and bound to `data`."""
    import arm_sample_probe as sp

    setup = zc.prepare(code, sram, cache)
    setup.keep({4})  # zone layer 4 = synth layer A; its synth object is table index 0
    emu = setup.emu
    sp.serve_sample(emu, sp.sample_device_image(data, sp.sample_name(data)))
    if sp.load_sample(emu) != 0:
        raise SystemExit("sample loader failed")
    store = setup.stores[4]
    aec.apply_value(emu, store.address, sp.EDIT_SMP_ID, 1)
    aec.apply_value(emu, store.address, sp.EDIT_MODE, 0)
    sp.bind_layer(emu, 0)
    setup.rig.tick(2)
    setup.freeze()
    return setup, store


def main(argv: list[str] | None = None) -> int:  # pragma: no cover - thin CLI
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    ap.add_argument("image", type=Path)
    ap.add_argument("sram", type=Path, nargs="?")
    ap.add_argument("--cache", type=Path)
    ap.add_argument("--ns4b", type=Path, required=True)
    ap.add_argument("--member", required=True)
    ap.add_argument("--notes", type=int, nargs="+", default=[60])
    ns = ap.parse_args(argv)
    data = pl.read_npno(ns.ns4b, ns.member, 0x400000)
    setup, _store = boot_sample_rig(ns.image, ns.sram, ns.cache, data)
    pitch_emu = law.load_static_init(ns.image, ns.sram)
    for note in ns.notes:
        setup.reset()
        cap = play_note(setup.rig, note)
        again = pitch_from_inputs(pitch_emu, cap) if cap.pitch_inputs else None
        flt = law.float_from_bits(cap.pitch_bits or 0)
        print(
            f"note {note}: key store {cap.key_store} transpose store {cap.transpose_store} "
            f"kernel {cap.messages.kernel} pitch {cap.pitch_bits and hex(cap.pitch_bits)} "
            f"({flt:.6f}) law {again == cap.pitch_bits} inputs {cap.pitch_inputs} "
            f"layer {cap.layer_inputs} fields "
            f"{ {hex(k): hex(v) for k, v in cap.messages.fields.items()} }"
        )
    return 0


if __name__ == "__main__":  # pragma: no cover
    sys.exit(main())
