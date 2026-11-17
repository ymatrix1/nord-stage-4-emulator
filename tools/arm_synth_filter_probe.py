#!/usr/bin/env python3
"""Synth filter links on the real firmware: a loaded synth program, watches and edit sweeps.

Boots the full machine (`arm_screens.Session`) with a factory program in the Store record
(`arm_program_load.build_image`), so the firmware recalls the program itself, then:

1. **load**: runs the recall with a write watch on the strip objects given by `--strips` (offsets
   `+0x60..+0x90`: `+0x74` = the type selector, `+0x7C` = the sender index) and a code hook on the
   word-1 sender `0xC01CCF84`; logs every write (pc, lr, offset, value) and the final DSP words of
   the ORG core 2 strips and common blocks. `--strips` is found by a first run without it: the
   discovery step dispatches `kFltEnable` on the three synth layer stores and prints the strip
   pointer the sender receives;
2. **sweep**: from the settled state, dispatches one edit id value by value (`--sweep NAME LO HI`,
   Q16.16 cells) through the layer store and logs the DSP words and the watched fields after each;
3. **voice**: optionally sends a note-on through the UART2 model and logs the messages.

    C=target/fw_unpacked
    uv run --with unicorn python tools/arm_synth_filter_probe.py \
        $C/bank0_CODE.bin $C/bank0_SRAM.bin \
        --eeprom target/oled/eeprom.bin "factory_programs/Program/Bank A/Crunch Synth.ns4p" \
        --out target/synth_filter/crunch.json --sweep Edt_Syn_kFltType 0 5

Evidence: `docs/research/synth-filter-links.md`. A run takes about 6 minutes of CPU per program.
"""

from __future__ import annotations

import argparse
import csv
import json
import sys
from pathlib import Path

# VERIFIED (docs/research/synth-voice-control.md s2): synth layer A / B / C use ORG core 2 strips
# `DM(0xE0000 + 62 k + w)` with k = (0, 6), (2, 8), (4, 10), commons `DM(0xE02E8 + 32 L + w)`.
STRIP_BASE, STRIP_STRIDE = 0xE0000, 62
COMMON_BASE, COMMON_STRIDE = 0xE02E8, 32
LAYER_STRIPS = ((0, 6), (2, 8), (4, 10))
# VERIFIED (arm_zones.layer_bases docs): dispatcher client indices of the synth layers A, B, C.
SYNTH_LAYER_INDEX = (4, 5, 6)
# VERIFIED (docs/research/synth-filter-type-map.md s1): the type selector `[strip + 0x74]` and the
# sender index `[strip + 0x7C]` with `strip = layer + 0x68`; the word-1 sender `0xC01CCF84`.
SELECTOR_OFFSET, DRIVE_OFFSET, SENDER_INDEX_OFFSET = 0x74, 0x78, 0x7C
WATCH_BEGIN, WATCH_END = 0x60, 0x90
WORD1_SENDER = 0xC01CCF84
# VERIFIED (disassembly): the `kFltType` / `kFltEnable` case of the strip apply function.
TYPE_CASE = 0xC0017B4C
LOAD_TICKS, IDLE_TICKS, SWEEP_TICKS, VOICE_TICKS = 800, 300, 40, 60
TICK_INSNS = 200_000
ORG_CORE2 = ("ORG", 2)


def strip_word(dm_word: int) -> tuple[str, int, int] | None:
    """`("strip", k, w)` or `("common", L, w)` for a DM word of the ORG core 2 strip region."""
    if STRIP_BASE <= dm_word < COMMON_BASE:
        k, w = divmod(dm_word - STRIP_BASE, STRIP_STRIDE)
        return ("strip", k, w)
    if COMMON_BASE <= dm_word < COMMON_BASE + 3 * COMMON_STRIDE:
        layer, w = divmod(dm_word - COMMON_BASE, COMMON_STRIDE)
        return ("common", layer, w)
    return None


def last_words(rows) -> dict[int, int]:  # noqa: ANN001 - rows of `(tick, Message)` or Message
    """Last value written per ORG core 2 DM word (path 2) in send order."""
    out: dict[int, int] = {}
    for item in rows:
        m = item[1] if isinstance(item, tuple) else item
        if m.path == 2 and (m.chip, m.core) == ORG_CORE2:
            out[m.dm_word] = m.word2
    return out


def layer_words(words: dict[int, int], layer: int) -> dict[str, int]:
    """`{"s<k>w<w>" | "c<w>": value}` for the two strips and the common block of synth `layer`."""
    out: dict[str, int] = {}
    for k in LAYER_STRIPS[layer]:
        for w in range(STRIP_STRIDE):
            v = words.get(STRIP_BASE + STRIP_STRIDE * k + w)
            if v is not None:
                out[f"s{k}w{w}"] = v
    for w in range(COMMON_STRIDE):
        v = words.get(COMMON_BASE + COMMON_STRIDE * layer + w)
        if v is not None:
            out[f"c{w}"] = v
    return out


def diff_words(base: dict[str, int], now: dict[str, int]) -> dict[str, tuple[int | None, int]]:
    """Words of `now` that differ from `base` as `{name: (old, new)}`."""
    return {k: (base.get(k), v) for k, v in sorted(now.items()) if base.get(k) != v}


def param_ids(csv_path: Path) -> dict[str, tuple[int, int, int]]:
    """`{name: (id, min, max)}` of the edit-parameter table."""
    with csv_path.open(encoding="utf-8") as fh:
        return {
            r["name"]: (int(r["id"], 0), int(r["min"]), int(r["max"])) for r in csv.DictReader(fh)
        }


def parse_point(text: str) -> list[tuple[str, int]]:
    """`"Edt_Syn_kFltFreq=80,Edt_Syn_kFltEnvAmt=127"` -> `[(name, value), ...]` in order."""
    out = []
    for item in text.split(","):
        name, _, value = item.partition("=")
        if not name or not value:
            raise ValueError(f"bad point item {item!r}, want NAME=VALUE")
        out.append((name.strip(), int(value)))
    return out


def q16(value: int) -> int:
    """Q16.16 cell word of an integer UI value."""
    return (value << 16) & 0xFFFFFFFF


class Watch:  # pragma: no cover - needs the emulator
    """Write watch over the filter fields of strip objects and a hook on the word-1 sender."""

    def __init__(self, emu, strips: list[int]) -> None:  # noqa: ANN001
        self.emu, self.strips = emu, strips
        self.writes: list[dict] = []
        self.sender_calls: list[dict] = []
        for i, strip in enumerate(strips):
            emu.uc.hook_add(
                emu._u.UC_HOOK_MEM_WRITE, self._on_write, user_data=i,
                begin=strip + WATCH_BEGIN, end=strip + WATCH_END,
            )  # fmt: skip
        emu.uc.hook_add(emu._u.UC_HOOK_CODE, self._on_sender, begin=WORD1_SENDER, end=WORD1_SENDER)
        self.apply_hits: list[dict] = []
        emu.uc.hook_add(emu._u.UC_HOOK_CODE, self._on_apply, begin=TYPE_CASE, end=TYPE_CASE)

    def _on_apply(self, _uc, _address, _size, _user) -> None:  # noqa: ANN001
        e = self.emu
        r2 = e.reg(2)
        self.apply_hits.append(
            {"r0": e.reg(0), "r2": r2, "word": e.read32(r2), "r4": e.reg(4), "lr": e.reg(14)}
        )

    def _on_write(self, _uc, _access, address, size, value, i) -> None:  # noqa: ANN001
        e = self.emu
        self.writes.append(
            {"strip": i, "off": address - self.strips[i], "size": size,
             "value": value & 0xFFFFFFFF, "pc": e.reg(15), "lr": e.reg(14),
             "r0_r3": [e.reg(n) for n in range(4)]}
        )  # fmt: skip

    def _on_sender(self, _uc, _address, _size, _user) -> None:  # noqa: ANN001
        r0 = self.emu.reg(0)
        self.sender_calls.append(
            {"strip": r0, "lr": self.emu.reg(14),
             "selector": self.emu.read32(r0 + SELECTOR_OFFSET),
             "index": self.emu.read32(r0 + SENDER_INDEX_OFFSET)}
        )  # fmt: skip

    def clear(self) -> None:
        self.writes, self.sender_calls, self.apply_hits = [], [], []

    def fields(self) -> list[dict[str, int]]:
        return [
            {"selector": self.emu.read32(s + SELECTOR_OFFSET),
             "drive": self.emu.read32(s + DRIVE_OFFSET),
             "index": self.emu.read32(s + SENDER_INDEX_OFFSET)}
            for s in self.strips
        ]  # fmt: skip


def run_ticks(emu, ticks: int) -> list:  # noqa: ANN001  # pragma: no cover - emulator
    import arm_steady as ast

    rows = []
    for n in range(ticks):
        emu.clear_messages()
        ast.run_ticking(emu, TICK_INSNS, per_tick=TICK_INSNS)
        rows += [(n, m) for m in emu.messages]
    return rows


def words_json(words: dict[str, int]) -> dict[str, str]:
    return {k: f"{v:#010x}" for k, v in words.items()}


def capture(ns) -> dict:  # noqa: ANN001  # pragma: no cover - needs the local firmware
    import arm_keydown as kd
    import arm_program_load as apl
    import arm_screens as sc
    import arm_steady as ast
    import arm_zones as az

    program = ns.program.read_bytes()
    session = sc.Session(ns.image, ns.sram)
    session.load_eeproms(apl.build_image(ns.eeprom.read_bytes(), program, ns.program.stem[:23]))
    kd.install_board_present(session.emu)
    emu = session.emu
    # the recall of the stored program happens during the boot: watch from the first instruction
    strips = [int(x, 0) for x in ns.strips]
    watch = Watch(emu, strips)
    pc = session.boot()
    out: dict = {"program": ns.program.stem, "boot_pc": pc, "strips": strips}
    out["boot"] = {"writes": watch.writes, "sender_calls": watch.sender_calls,
                   "fields": watch.fields()}  # fmt: skip
    watch.clear()
    rig = kd.Rig(emu, kd.Uart2().install(emu))
    emu.install_dsp_ports()

    load_rows = run_ticks(emu, LOAD_TICKS)
    out["load"] = {
        "writes": watch.writes,
        "sender_calls": watch.sender_calls,
        "fields": watch.fields(),
        "org2_words": {str(i): words_json(layer_words(last_words(load_rows), i)) for i in range(3)},
    }
    ns.out.parent.mkdir(parents=True, exist_ok=True)
    ns.out.with_suffix(".load.json").write_text(json.dumps(out, indent=1), "utf-8")
    run_ticks(emu, IDLE_TICKS)
    watch.clear()
    settled = ast.snapshot(emu)
    store = az.first_stores(emu)[SYNTH_LAYER_INDEX[0]]
    base_words = layer_words(last_words(load_rows), 0)
    out["discovered"] = {}
    import arm_edit_capture as aec

    if not any(b <= aec.SCRATCH <= e for b, e, _ in emu.uc.mem_regions()):
        emu.uc.mem_map(aec.SCRATCH, 0x1000)
    enable_id = param_ids(ns.params)["Edt_Syn_kFltEnable"][0]
    for layer, st in az.first_stores(emu).items():
        if layer in SYNTH_LAYER_INDEX:
            watch.clear()
            aec.apply_value(emu, st.address, enable_id, q16(1), window=SWEEP_TICKS * TICK_INSNS)
            out["discovered"][str(layer)] = [hex(c["strip"]) for c in watch.sender_calls[:2]]
    ast.restore(emu, settled)
    emu.install_dsp_ports()
    out["sweeps"] = []
    for spec in ns.sweep:
        name, lo, hi = spec[0], int(spec[1]), int(spec[2])
        eid, _mn, _mx = param_ids(ns.params)[name]
        rows = []
        for v in range(lo, hi + 1):
            ast.restore(emu, settled)
            emu.install_dsp_ports()
            watch.clear()
            import arm_edit_capture as aec

            if not any(b <= aec.SCRATCH <= e for b, e, _ in emu.uc.mem_regions()):
                emu.uc.mem_map(aec.SCRATCH, 0x1000)
            during, later, err = aec.apply_value(
                emu, store.address, eid, q16(v), window=SWEEP_TICKS * TICK_INSNS
            )
            words = layer_words(last_words(during + later), 0)
            rows.append(
                {"value": v, "error": err, "fields": watch.fields()[0],
                 "writes": watch.writes, "sender_calls": watch.sender_calls,
                 "apply_hits": watch.apply_hits, "words": words_json(words), "changed": words_json(
                     {k: x for k, x in words.items() if base_words.get(k) != x})}
            )  # fmt: skip
        out["sweeps"].append({"id": name, "rows": rows})
    ids = param_ids(ns.params)
    out["points"] = []
    for text in ns.point:
        sets = parse_point(text)
        ast.restore(emu, settled)
        emu.install_dsp_ports()
        watch.clear()
        rows = []
        for name, value in sets:
            during, later, _err = aec.apply_value(
                emu, store.address, ids[name][0], q16(value), window=SWEEP_TICKS * TICK_INSNS
            )
            rows += during + later
        words = layer_words(last_words(rows), 0)
        out["points"].append(
            {"point": text, "words": words_json(words),
             "changed": words_json({k: x for k, x in words.items() if base_words.get(k) != x})}
        )  # fmt: skip
    ast.restore(emu, settled)
    emu.install_dsp_ports()
    if ns.voice:
        watch.clear()
        rig.send(kd.note_on(ns.voice, 100))
        rows = run_ticks(emu, VOICE_TICKS)
        out["voice"] = {
            "note": ns.voice,
            "messages": [
                {"tick": t, "chip": m.chip, "core": m.core, "path": m.path,
                 "dm": f"{m.dm_word:#x}", "value": f"{m.word2:#010x}"}
                for t, m in rows
            ],
        }  # fmt: skip
    return out


def main(argv: list[str] | None = None) -> int:  # pragma: no cover - needs the local firmware
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    ap.add_argument("image", type=Path)
    ap.add_argument("sram", type=Path)
    ap.add_argument("--eeprom", type=Path, required=True)
    ap.add_argument("program", type=Path)
    ap.add_argument("--out", type=Path, required=True)
    ap.add_argument("--params", type=Path, default=Path("docs/research/arm-edit-params.csv"))
    ap.add_argument("--sweep", nargs=3, action="append", default=[], metavar=("NAME", "LO", "HI"))
    ap.add_argument("--point", action="append", default=[], help="NAME=V[,NAME=V...] edit point")
    ap.add_argument("--strips", nargs="*", default=[], help="strip object addresses to watch")
    ap.add_argument("--voice", type=int, default=0, help="MIDI note to send after the sweeps")
    ns = ap.parse_args(argv)
    sys.path.insert(0, str(Path(__file__).resolve().parent))
    result = capture(ns)
    ns.out.parent.mkdir(parents=True, exist_ok=True)
    ns.out.write_text(json.dumps(result, indent=1), "utf-8")
    print(f"wrote {ns.out}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
