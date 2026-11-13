#!/usr/bin/env python3
"""Census of the ARM to DSP traffic the harness stubs or drops, and the full boot message log.

The booted harness (`arm_boot_full`, `arm_steady`) replaces the SPI image download, the SPI block
download and the SPI master transfer by no-ops, and `arm_steady.settle` / `clear_messages` throw
away every DSP message sent before the main loop (the engine constructor `0xC002187C` alone sends
15,289 words). This tool

* wraps those stubs and logs each call (`Transfer`: stub name, caller, arguments, phase);
* hooks the bulk table sender `0xC001C6A0(dm_word, src_ptr, count)` and fingerprints its source
  table (`BulkCall`: destination, source region, SHA-256 of the source words, never the words);
* keeps the raw port log across the whole boot (`init`, `boot`, `settle N` phases) and groups it
  once, so no message is lost at a phase boundary;
* reduces the log to `final_state` (last value per DSP word), fingerprints it and writes
  `sharc-pass` / `sharc-org` / `sharc-fx` replay scripts.

    uv run --with unicorn python tools/arm_bulk_tables.py capture \
        target/fw_unpacked/bank0_CODE.bin target/fw_unpacked/bank0_SRAM.bin --out target/bulk

Only fingerprints and counts are meant for the repository (`docs/research/arm-bulk-tables.md`);
the log (`boot_log.jsonl`) and the scripts hold firmware-derived words and stay under `target/`.
Evidence and limits: `docs/research/arm-bulk-tables.md`.
"""

from __future__ import annotations

import argparse
import bisect
import hashlib
import json
import struct
import sys
from collections import Counter
from dataclasses import dataclass, field
from pathlib import Path

import arm_emu as ae

# VERIFIED (disassembly, docs/research/arm-bulk-tables.md s2): the bulk table sender
# `(dm_word, src_ptr, count)`: for each source int it forms a float word and stores
# `(0x28000000 + 4 * (dm_word + i), [pad 0], word)` to the PNO comm port. Two call sites, both in
# the engine constructor `0xC002187C`.
BULK_SENDER = 0xC001C6A0
# VERIFIED (run, docs/research/arm-bulk-tables.md s2): the port object the real boot builds
# (`[0xC055B150]`) holds the path 2 ports of comm 0..2 at `+0x40 + 4 c`, and the path 1 comm
# objects `[0xC053F8A4 + 4 (c + 1)]` hold the ports of comm 3, 4 at `+0x10`. Window of the
# EMIFA CS3 stores the boot sends before the harness installs its fake ports.
REAL_PORTS = {
    0x6200401C: (2, 0),
    0x62004014: (2, 1),
    0x62004018: (2, 2),
    0x6200000C: (1, 3),
    0x62000010: (1, 4),
}
REAL_WINDOW = (0x62000000, 0x62004100)
# VERIFIED (arm_boot.py, arm_boot_full.py): the routines the harness replaces.
DSP_DOWNLOAD = 0xC0241C58
SPI_DOWNLOAD_BLOCK = 0xC0240548
SPI_TRANSFER = 0xC0066E64
FILE_READ = 0xC0094EA0
STUB_NAMES = {
    DSP_DOWNLOAD: "dsp_image_download",
    SPI_DOWNLOAD_BLOCK: "spi_download_block",
    SPI_TRANSFER: "spi_transfer",
}
# VERIFIED (tools/arm_emu.py): code image size is the file size; the RAM init block follows the
# same layout the harness maps.
SRAM_WINDOW = 0x100000


def region_of(address: int, code_size: int, sram_size: int) -> str:
    """Name the memory region of an ARM address as the harness maps it."""
    if ae.LOAD_BASE <= address < ae.LOAD_BASE + code_size:
        return "bank0_CODE"
    if ae.LOAD_BASE + code_size <= address < ae.LOAD_BASE + ae.RAM_SIZE:
        return "bss_runtime"
    if ae.SRAM_BASE <= address < ae.SRAM_BASE + sram_size:
        return "bank0_SRAM"
    if ae.SRAM_BASE + sram_size <= address < ae.SRAM_BASE + SRAM_WINDOW:
        return "sram_runtime"
    if 0x90000000 <= address < 0x98000000:
        return "heap_runtime"
    if any(lo <= address < hi for lo, hi in ae.DEVICE_WINDOWS):
        return "device"
    return "other"


def fingerprint(words: list[int]) -> str:
    """SHA-256 (first 16 hex digits) of little-endian u32 words."""
    masked = [w & 0xFFFFFFFF for w in words]
    return hashlib.sha256(struct.pack(f"<{len(masked)}I", *masked)).hexdigest()[:16]


@dataclass(frozen=True)
class Transfer:
    """One call of a stubbed transfer routine."""

    name: str
    lr: int
    args: tuple[int, ...]
    phase: str
    obj: tuple[int, ...] = ()  # first words of the object in r0 (pointers and sizes, no payload)


@dataclass(frozen=True)
class BulkCall:
    """One call of the bulk table sender: destination DM word, source table, size, fingerprint."""

    dm_word: int
    src: int
    count: int
    region: str
    sha: str
    lr: int


@dataclass(frozen=True)
class PhasedMessage:
    message: ae.Message
    phase: str


@dataclass
class BootLog:
    """Everything one instrumented boot saw."""

    transfers: list[Transfer] = field(default_factory=list)
    bulk: list[BulkCall] = field(default_factory=list)
    file_reads: Counter[int] = field(default_factory=Counter)
    messages: list[PhasedMessage] = field(default_factory=list)
    phases: list[tuple[str, int]] = field(default_factory=list)  # (name, raw index at its end)
    null_calls: int = 0
    poll_hits: int = 0
    auto_mapped: int = 0
    instructions: int = 0


def group_indexed(
    writes: list[tuple[int, int, int]], pad: tuple[int, ...] = (1, 1, 1, 0, 0)
) -> list[tuple[ae.Message, int]]:
    """`ae.group_port_writes` that also returns the raw index of each message's last store."""
    pending: dict[int, list[tuple[int, int]]] = {}
    out: list[tuple[ae.Message, int]] = []
    for i, (port, value, pc) in enumerate(writes):
        if ae.PORT2_BASE <= port < ae.PORT2_BASE + 0x10 * ae.COMM_COUNT:
            path, comm = 2, (port - ae.PORT2_BASE) // 0x10
            need = 3 if pad[comm] else 2
        elif ae.PORT1_BASE <= port < ae.PORT1_BASE + 0x10 * ae.COMM_COUNT:
            path, comm, need = 1, (port - ae.PORT1_BASE) // 0x10, 2
        else:
            continue
        buf = pending.setdefault(port, [])
        buf.append((value, pc))
        if len(buf) == need:
            out.append((ae.Message(path, comm, buf[0][0], buf[-1][0], buf[-1][1]), i))
            buf.clear()
    return out


def phase_of(index: int, phases: list[tuple[str, int]]) -> str:
    """Phase name of raw store `index` (phases are `(name, end index)`, ascending)."""
    ends = [e for _, e in phases]
    k = bisect.bisect_right(ends, index)
    return phases[min(k, len(phases) - 1)][0]


def phase_messages(
    writes: list[tuple[int, int, int]],
    phases: list[tuple[str, int]],
    pad: tuple[int, ...] = (1, 1, 1, 0, 0),
) -> list[PhasedMessage]:
    return [PhasedMessage(m, phase_of(i, phases)) for m, i in group_indexed(writes, pad)]


StateKey = tuple[str, int, int, int]  # (chip, core, path, dm word)


def is_dsp_word(m: ae.Message) -> bool:
    """True when the message addresses a DSP data word (path 2 inside `0x28000000..0x29000000`)."""
    return m.path == 1 or 0x28000000 <= m.word1 < 0x29000000


def is_l2_word(m: ae.Message) -> bool:
    """True for a path 2 store to the DSP's L2 window (byte address `0x20000000..0x28000000`)."""
    return m.path == 2 and 0x20000000 <= m.word1 < 0x28000000


def l2_fingerprint(messages: list[ae.Message], chip: str) -> tuple[int, str]:
    """`(word count, sha)` of the ordered `(byte address, value)` L2 stores to one chip."""
    flat = [x for m in messages if m.chip == chip and is_l2_word(m) for x in (m.word1, m.word2)]
    return len(flat) // 2, fingerprint(flat) if flat else ""


def final_state(messages: list[ae.Message]) -> dict[StateKey, int]:
    """Last value written to each DSP word (messages that carry no DSP address are skipped)."""
    return {(m.chip, m.core, m.path, m.dm_word): m.word2 for m in messages if is_dsp_word(m)}


def state_fingerprint(state: dict[StateKey, int], chip: str, core: int) -> tuple[int, str]:
    """`(word count, sha)` of the `(dm, value)` pairs of one DSP core, ordered by address."""
    items = sorted((k[3], v) for k, v in state.items() if k[0] == chip and k[1] == core)
    flat = [x for pair in items for x in pair]
    return len(items), fingerprint(flat) if flat else ""


def summarise(messages: list[PhasedMessage]) -> list[dict[str, object]]:
    """One row per (phase kind, chip, core, path): messages, distinct words, zero values."""
    rows: dict[tuple[str, str, int, int], list[ae.Message]] = {}
    for pm in messages:
        m = pm.message
        kind = "settle" if pm.phase.startswith("settle") else pm.phase
        rows.setdefault((kind, m.chip, m.core, m.path), []).append(m)
    out = []
    for (kind, chip, core, path), ms in sorted(rows.items()):
        out.append(
            {
                "phase": kind,
                "chip": chip,
                "core": core,
                "path": path,
                "messages": len(ms),
                "distinct_words": len({m.dm_word for m in ms}),
                "zero_values": sum(1 for m in ms if m.word2 == 0),
                "l2_words": sum(1 for m in ms if is_l2_word(m)),
                "other_words": sum(1 for m in ms if not is_dsp_word(m) and not is_l2_word(m)),
            }
        )
    return out


def bulk_rows(calls: list[BulkCall]) -> list[dict[str, object]]:
    return [
        {
            "dm_word": f"{c.dm_word:#x}",
            "count": c.count,
            "src": f"{c.src:#010x}",
            "region": c.region,
            "sha": c.sha,
            "lr": f"{c.lr:#010x}",
        }
        for c in calls
    ]


def transfer_rows(transfers: list[Transfer]) -> list[dict[str, object]]:
    """Group the stub calls by `(name, caller)`."""
    groups: Counter[tuple[str, int, str]] = Counter()
    first: dict[tuple[str, int, str], Transfer] = {}
    for t in transfers:
        key = (t.name, t.lr, t.phase.split()[0])
        groups[key] += 1
        first.setdefault(key, t)
    return [
        {
            "stub": name,
            "caller_lr": f"{lr:#010x}",
            "phase": phase,
            "calls": n,
            "first_args": [f"{a:#x}" for a in first[(name, lr, phase)].args],
            "first_obj": [f"{w:#x}" for w in first[(name, lr, phase)].obj],
        }
        for (name, lr, phase), n in sorted(groups.items())
    ]


def replay_script(messages: list[ae.Message], chip: str, core: int) -> str:
    """`sharc-pass` / `sharc-fx` style lines `0 DM VALUE` for one core's path 2 messages."""
    return "".join(
        f"0 {m.dm_word:#x} {m.word2:#010x}\n"
        for m in messages
        if m.chip == chip and m.path == 2 and m.core == core and is_dsp_word(m)
    )


def messages_jsonl(messages: list[PhasedMessage]) -> str:
    return "".join(json.dumps({"phase": p.phase, **p.message.to_dict()}) + "\n" for p in messages)


# ARM side (needs `unicorn` and the firmware).
# -----------------------------------------------------------------------------------------------


def instrument(
    emu: ae.ArmEmu, log: BootLog, phase: list[str], code_size: int, sram_size: int
) -> None:  # pragma: no cover - needs the firmware
    """Wrap the stubs and hook the bulk sender and the file read."""
    for addr, name in STUB_NAMES.items():
        inner = emu.stubs[addr]

        def wrapped(e: ae.ArmEmu, inner=inner, name=name) -> int | None:  # noqa: ANN001
            args = tuple(e.reg(i) for i in range(4))
            try:
                obj = tuple(e.read32(args[0] + 4 * i) for i in range(8))
            except Exception:  # r0 is not a mapped object
                obj = ()
            log.transfers.append(Transfer(name, e.reg(14), args, phase[0], obj))
            return inner(e)

        emu.stubs[addr] = wrapped

    def on_bulk(_uc, _a, _s, _u) -> None:  # noqa: ANN001
        dm, src, count = emu.reg(0), emu.reg(1), emu.reg(2)
        words = list(struct.unpack(f"<{count}I", emu.read_bytes(src, 4 * count)))
        log.bulk.append(
            BulkCall(
                dm,
                src,
                count,
                region_of(src, code_size, sram_size),
                fingerprint(words),
                emu.reg(14),
            )
        )

    def on_file(_uc, _a, _s, _u) -> None:  # noqa: ANN001
        log.file_reads[emu.reg(14)] += 1

    emu.uc.hook_add(emu._u.UC_HOOK_CODE, on_bulk, begin=BULK_SENDER, end=BULK_SENDER)
    emu.uc.hook_add(emu._u.UC_HOOK_CODE, on_file, begin=FILE_READ, end=FILE_READ)


def real_port_capture(emu: ae.ArmEmu) -> None:  # pragma: no cover - needs the firmware
    """Log the boot's stores to the real EMIFA ports in the fake-port format of `ArmEmu`."""

    def hook(_uc, _access, address, _size, value, _user) -> None:  # noqa: ANN001
        if address in REAL_PORTS:
            path, comm = REAL_PORTS[address]
            base = ae.PORT2_BASE if path == 2 else ae.PORT1_BASE
            emu.port_writes.append((base + 0x10 * comm, value & 0xFFFFFFFF, emu.reg(15)))

    emu.uc.hook_add(emu._u.UC_HOOK_MEM_WRITE, hook, begin=REAL_WINDOW[0], end=REAL_WINDOW[1] - 1)


def capture_boot(
    code: Path, sram: Path, windows: int = 12, window: int = 20_000_000
) -> tuple[BootLog, ae.ArmEmu]:  # pragma: no cover - needs the firmware
    """Boot to the main loop and settle, keeping every port store; returns `(log, emulator)`."""
    import arm_boot_full as bf
    import arm_steady as ast

    code_size, sram_size = code.stat().st_size, sram.stat().st_size
    log, phase = BootLog(), ["init"]
    emu = bf.prepare(code, sram)
    instrument(emu, log, phase, code_size, sram_size)
    real_port_capture(emu)
    log.phases.append(("init", len(emu.port_writes)))
    phase[0] = "boot"
    pc, n, err = ast.boot_to_main_loop(emu)
    if pc != ast.MAIN_LOOP:
        raise SystemExit(f"boot stopped at {pc:#x} {err}")
    emu.install_dsp_ports()
    ast.enable_real_timer(emu)
    log.phases.append(("boot", len(emu.port_writes)))
    log.instructions = n
    for k in range(1, windows + 1):
        phase[0] = f"settle {k}"
        before = len(emu.port_writes)
        ast.run_ticking(emu, window)
        log.phases.append((phase[0], len(emu.port_writes)))
        if len(emu.port_writes) == before:
            break
    log.messages = phase_messages(emu.port_writes, log.phases, emu.pad)
    log.null_calls, log.auto_mapped = len(emu.null_calls), len(emu.auto_mapped)
    log.poll_hits = sum(emu.poll_hits.values())
    return log, emu


def write_outputs(log: BootLog, out: Path) -> dict[str, object]:  # pragma: no cover - file output
    out.mkdir(parents=True, exist_ok=True)
    msgs = [p.message for p in log.messages]
    (out / "boot_log.jsonl").write_text(messages_jsonl(log.messages), encoding="utf-8")
    state = final_state(msgs)
    cores = sorted({(k[0], k[1]) for k in state if k[2] == 2})
    for chip in sorted({m.chip for m in msgs if is_l2_word(m)}):
        rows = [
            f"{m.word1:#010x} {m.word2:#010x}" for m in msgs if m.chip == chip and is_l2_word(m)
        ]
        (out / f"{chip.lower()}_l2.script").write_text("\n".join(rows) + "\n", encoding="utf-8")
    for chip, core in cores:
        name = f"{chip.lower()}_core{core}.script"
        (out / name).write_text(replay_script(msgs, chip, core), encoding="utf-8")
    summary = {
        "messages": len(msgs),
        "phases": log.phases,
        "by_phase": summarise(log.messages),
        "bulk": bulk_rows(log.bulk),
        "stubs": transfer_rows(log.transfers),
        "file_read_callers": {f"{k:#010x}": v for k, v in sorted(log.file_reads.items())},
        "state": {
            f"{chip}/{core}": dict(
                zip(("words", "sha"), state_fingerprint(state, chip, core), strict=True)
            )
            for chip, core in cores
        },
        "l2": {
            chip: dict(zip(("words", "sha"), l2_fingerprint(msgs, chip), strict=True))
            for chip in sorted({m.chip for m in msgs if is_l2_word(m)})
        },
        "null_calls": log.null_calls,
        "poll_hits": log.poll_hits,
    }
    (out / "summary.json").write_text(json.dumps(summary, indent=1), encoding="utf-8")
    return summary


def main(argv: list[str] | None = None) -> int:  # pragma: no cover - thin CLI
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)
    cap = sub.add_parser("capture", help="boot, settle, write the log, scripts and summary")
    cap.add_argument("image", type=Path)
    cap.add_argument("sram", type=Path)
    cap.add_argument("--out", type=Path, default=Path("target/bulk"))
    cap.add_argument("--windows", type=int, default=12)
    ns = ap.parse_args(argv)
    log, _emu = capture_boot(ns.image, ns.sram, ns.windows)
    summary = write_outputs(log, ns.out)
    print(json.dumps({k: summary[k] for k in ("messages", "phases", "state")}, indent=1))
    return 0


if __name__ == "__main__":
    sys.exit(main())
