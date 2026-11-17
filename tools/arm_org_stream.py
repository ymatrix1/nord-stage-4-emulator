#!/usr/bin/env python3
"""Capture the real ARM -> ORG Core2 message stream and the ARM time per tick (D119 system run).

Boots the full machine (`arm_screens.Session`) with a factory program in the Store record
(`arm_program_load.build_image`), attaches the board-present flag and the UART2 MIDI model
(`arm_keydown`), then runs phases of 1 ms ARM ticks and logs, per tick, the wall time and every
DSP message the firmware sent:

* `boot`  the messages sent before the main loop starts (not ticks);
* `load`  800 ticks after the boot (the program recall plus the initial push to every DSP);
* `idle`  300 ticks with nothing pressed;
* `key1`  one note-on, 40 ticks, note-off, 40 ticks;
* `key8`  eight note-ons (C4 to C5 white keys), 40 ticks, note-offs, 40 ticks.

The ORG Core2 messages are written as `sharc-sys --msg-script` files (`PASS W1 W2`, W1 = DM word,
use `--msg-base 0`). The ARM tick is mapped to SHARC passes with `passes_per_tick()`: one pass is
2 audio frames at `FRAME_RATE`, one tick is 1 ms.

    C=target/fw_unpacked
    uv run --with unicorn python tools/arm_org_stream.py $C/bank0_CODE.bin $C/bank0_SRAM.bin \
        --eeprom target/oled/eeprom.bin "factory_programs/Program/Bank A/B3 Chords n Solo.ns4p" \
        --out target/org_system

Evidence and tables: `docs/research/org-system-run.md`. A run takes about 5 minutes of CPU.
"""

from __future__ import annotations

import argparse
import json
import statistics
import sys
import time
from pathlib import Path

# ASSUMED: the LP24 path of the SY DSP runs at this audio frame rate (docs/research/dsp-frame-rate
# notes, `tools/dsp_frame_rate.py`); one ORG Core2 pass handles 2 frames. Other paths: see the note.
FRAME_RATE = 43_402.34375
FRAMES_PER_PASS = 2
TICK_SECONDS = 1e-3
# VERIFIED (arm_steady.INSNS_PER_TICK): ARM instructions per injected timer tick.
TICK_INSNS = 200_000
WARMUP = 16
LOAD_TICKS, IDLE_TICKS, KEY_TICKS = 800, 300, 40
# Eight white keys C4..C5 (MIDI 60, 62, 64, 65, 67, 69, 71, 72).
KEYS8 = (60, 62, 64, 65, 67, 69, 71, 72)
CHIP, CORE = "ORG", 2
# Script targets written per phase: (file prefix, chip, core). PNO core 1 is where the notes land;
# every other path-2 receiver is written too, for the other-cores table of the note.
TARGETS = (
    ("org-sys", "ORG", 2),
    ("pno-sys", "PNO", 1),
    ("org-sys", "ORG", 1),
    ("pno-sys", "PNO", 2),
    ("fx-sys", "FX", 1),
    ("fx-sys", "FX", 2),
)


def pass_period_s(frame_rate: float = FRAME_RATE) -> float:
    """Audio time of one pass in seconds (2 frames)."""
    return FRAMES_PER_PASS / frame_rate


def passes_per_tick(frame_rate: float = FRAME_RATE) -> float:
    """SHARC passes that fit in one 1 ms ARM tick (about 21.7)."""
    return TICK_SECONDS / pass_period_s(frame_rate)


def tick_to_pass(tick: int, start: int = 0, frame_rate: float = FRAME_RATE) -> int:
    """Pass number at which a message sent in `tick` reaches the DSP (start of that tick)."""
    return start + int(tick * passes_per_tick(frame_rate))


def select(messages, chip: str = CHIP, core: int = CORE) -> list:
    """`(tick, Message)` rows that reach one DSP core (path 2) in send order."""
    return [(t, m) for t, m in messages if m.chip == chip and m.path == 2 and m.core == core]


def script_text(rows, start_pass: int = 0, frame_rate: float = FRAME_RATE) -> str:
    """`PASS W1 W2` lines (W1 = DM word) for `rows` of `(tick, Message)`."""
    lines = [
        f"{tick_to_pass(t, start_pass, frame_rate)} {m.dm_word:#x} {m.word2:#010x}"
        f"  # tick {t} {m.word1:#010x} pc {m.pc:#x}"
        for t, m in rows
    ]
    return "\n".join(lines) + ("\n" if lines else "")


def timing_stats(seconds: list[float], warmup: int = WARMUP) -> dict[str, float]:
    """Mean / worst / median of `seconds` after dropping `warmup` leading samples."""
    kept = seconds[warmup:]
    if not kept:
        raise ValueError("no samples left after warm-up")
    return {
        "n": len(kept),
        "mean": statistics.fmean(kept),
        "median": statistics.median(kept),
        "worst": max(kept),
    }


def per_pass(tick_seconds: float, frame_rate: float = FRAME_RATE) -> float:
    """ARM wall time attributed to one pass interval (tick time / passes per tick)."""
    return tick_seconds / passes_per_tick(frame_rate)


def count_by_target(messages) -> dict[str, int]:
    """`{"ORG core 2": n, ...}` message counts by receiving DSP core."""
    out: dict[str, int] = {}
    for _t, m in messages:
        key = f"{m.chip} core {m.core}" if m.path == 2 else f"{m.chip} path 1"
        out[key] = out.get(key, 0) + 1
    return out


def run_phase(rig, ticks: int, action=None):
    """Run `ticks` ticks, `action()` before the first one; returns `(rows, seconds)`."""
    import arm_steady as ast

    rows, seconds = [], []
    for n in range(ticks):
        if n == 0 and action:
            action()
        rig.emu.clear_messages()
        t0 = time.perf_counter()
        ast.run_ticking(rig.emu, TICK_INSNS, per_tick=TICK_INSNS)
        seconds.append(time.perf_counter() - t0)
        rows += [(n, m) for m in rig.emu.messages]
    return rows, seconds


def capture(code: Path, sram: Path, eeprom: Path, program: Path):  # pragma: no cover - firmware
    """Run all phases; returns `{phase: (rows, seconds)}`."""
    import arm_keydown as kd
    import arm_program_load as apl
    import arm_screens as sc

    session = sc.Session(code, sram)
    session.load_eeproms(
        apl.build_image(eeprom.read_bytes(), program.read_bytes(), program.stem[:23])
    )
    kd.install_board_present(session.emu)
    pc = session.boot()
    rig = kd.Rig(session.emu, kd.Uart2().install(session.emu))
    session.emu.install_dsp_ports()
    print(f"booted, pc {pc:#x}", flush=True)
    out = {}
    # messages sent while the firmware booted (gains such as DM(0xB44C0) = 1.0, boot tables)
    out["boot"] = ([(0, m) for m in session.emu.messages], [])
    out["load"] = run_phase(rig, LOAD_TICKS)
    out["idle"] = run_phase(rig, IDLE_TICKS)

    def send_on(keys):
        return lambda: [rig.send(kd.note_on(k, 100)) for k in keys]

    def send_off(keys):
        return lambda: [rig.send(kd.note_off(k)) for k in keys]

    for name, keys in (("key1", KEYS8[:1]), ("key8", KEYS8)):
        out[name] = run_phase(rig, KEY_TICKS, send_on(keys))
        out[name + "_off"] = run_phase(rig, KEY_TICKS, send_off(keys))
    return out


def write_outputs(phases, out: Path, program: str) -> None:  # pragma: no cover - firmware
    out.mkdir(parents=True, exist_ok=True)
    summary: dict[str, object] = {"program": program, "frame_rate": FRAME_RATE, "phases": {}}
    for name, (rows, seconds) in phases.items():
        for prefix, chip, core in TARGETS:
            text = script_text(select(rows, chip, core))
            (out / f"{prefix}-{name}-core{core}.script").write_text(text, "utf-8")
        entry = {
            "ticks": len(seconds),
            "messages": count_by_target(rows),
            "arm_tick_seconds": timing_stats(seconds) if len(seconds) > WARMUP else None,
        }
        summary["phases"][name] = entry
        (out / f"tick-seconds-{name}.json").write_text(json.dumps(seconds), "utf-8")
    (out / "arm-stream-summary.json").write_text(json.dumps(summary, indent=1), "utf-8")
    print(json.dumps(summary, indent=1))


def main(argv: list[str] | None = None) -> int:  # pragma: no cover - needs the local firmware
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    ap.add_argument("image", type=Path)
    ap.add_argument("sram", type=Path)
    ap.add_argument("--eeprom", type=Path, required=True)
    ap.add_argument("program", type=Path)
    ap.add_argument("--out", type=Path, required=True)
    ns = ap.parse_args(argv)
    sys.path.insert(0, str(Path(__file__).resolve().parent))
    phases = capture(ns.image, ns.sram, ns.eeprom, ns.program)
    write_outputs(phases, ns.out, ns.program.stem)
    return 0


if __name__ == "__main__":
    sys.exit(main())
