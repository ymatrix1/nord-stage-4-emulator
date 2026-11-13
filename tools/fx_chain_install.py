#!/usr/bin/env python3
"""FX effect chain: ARM routine directory, stage record rows and the Amp type switch.

The FX DSP image runs a 35-stage chain whose stages all hold the null stub; the ARM installs the
real routines. This tool recovers what the ARM knows about it (`docs/research/fx-chain-install.md`):

* `scan`: every 16-byte channel record `{target, base, size, tag}` of the ARM RAM after the static
  initialiser whose `base` is a code address (`0x28000000 + 4 pc` core 1, `0x28800000 + 4 pc`
  core 2) in the FX image. This finds rows the table extents of `tools/arm_params.py` miss (the
  compressor, delay, multi-tap and resonant filter modules). Output: CSV, with the shape census of
  each routine when FX listings are given.
* `stages`: the 35 stage rows of channel table `0xC0547668` (one 9-word record per stage) and a
  check against a flat SRAM image that word 0 of every row holds the null stub.
* `amp`: runs the layer constructor `0xC0220E50` and the 10-case switch `0xC021D064` and groups the
  messages of every case into DM words (the Amp type tables).

    python tools/fx_chain_install.py stages docs/research/arm-chan-tables.csv core1_sram.bin
    uv run --with unicorn python tools/fx_chain_install.py scan CODE.bin SRAM.bin --out OUT.csv \\
        --listing 0=FX_c1.txt --listing 1=FX_c2.txt
    uv run --with unicorn python tools/fx_chain_install.py amp CODE.bin SRAM.bin
"""

from __future__ import annotations

import argparse
import csv
import struct
import sys
from pathlib import Path
from typing import NamedTuple

if __package__ in (None, ""):
    sys.path.insert(0, str(Path(__file__).resolve().parent))
import pno_module_chain as pm

#: VERIFIED: docs/research/fx-chain-install.md s2 (`scan`): FX image targets of channel records.
FX_TARGETS = {0: 1, 1: 2}  # target -> core
#: VERIFIED: channel table `0xC0547668`, rows 0..34 target 0: the 35 stage records (s3).
STAGE_TABLE = 0xC0547668
STAGE_COUNT = 35
#: VERIFIED: null stubs of the FX image, core 1 and core 2 (docs/research/fx-dsp-map.md s3).
NULL_STUB = {1: 0x125346, 2: 0x1249D2}
#: VERIFIED: file offset of DM word `w` in a flat SRAM image is `4 w` (`tools/fx_dm_tables.py`).
SRAM_BASE_WORD = 0
#: VERIFIED: docs/research/fx-chain-install.md s5: layer constructor, 10-case Amp type switch.
LAYER_CTOR = 0xC0220E50
AMP_SWITCH = 0xC021D064
AMP_CASES = 10
CSV_FIELDS = (
    "record_addr,run,target,core,pc,size,tag,instr,mul,biquad_pairs,fir,loops,recips,clip"
).split(",")


class Record(NamedTuple):
    addr: int
    target: int
    core: int
    pc: int
    size: int
    tag: int


def base_to_pc(base: int) -> tuple[int, int] | None:
    """`(core, pc)` for a channel-record base that names FX code, else None."""
    if pm.CORE2_BASE <= base < 0x29000000:
        core, pc = 2, (base - pm.CORE2_BASE) >> 2
    elif pm.CORE1_BASE <= base < pm.CORE2_BASE:
        core, pc = 1, (base - pm.CORE1_BASE) >> 2
    else:
        return None
    return (core, pc) if pm.CODE_LO <= pc < pm.CODE_HI else None


def scan_code_records(mem: bytes, lo: int) -> list[Record]:
    """Records `{target, base, size, tag}` with an FX code `base` in a RAM dump starting at `lo`."""
    out: list[Record] = []
    for off in range(4, len(mem) - 11, 4):
        hit = base_to_pc(struct.unpack_from("<I", mem, off)[0])
        if hit is None:
            continue
        target = struct.unpack_from("<I", mem, off - 4)[0]
        core, pc = hit
        if FX_TARGETS.get(target) != core:
            continue
        size, tag = struct.unpack_from("<II", mem, off + 4)
        out.append(Record(lo + off - 4, target, core, pc, size, tag))
    return out


def group_runs(records: list[Record]) -> list[int]:
    """Run number per record: records 16 bytes apart (one table segment) share a run."""
    runs: list[int] = []
    run = -1
    last = None
    for r in records:
        if last is None or r.addr - last != 16:
            run += 1
        runs.append(run)
        last = r.addr
    return runs


def csv_rows(
    records: list[Record], listings: dict[int, list[tuple[int, str]]]
) -> list[dict[str, object]]:
    """CSV rows (addresses and census counts only) for `records`, shapes taken from `listings`."""
    runs = group_runs(records)
    rows = []
    for r, run in zip(records, runs, strict=True):
        row: dict[str, object] = {
            "record_addr": f"{r.addr:#010x}",
            "run": run,
            "target": r.target,
            "core": r.core,
            "pc": f"{r.pc:#08x}",
            "size": f"{r.size:#x}",
            "tag": f"{r.tag:#x}",
        }
        ins = listings.get(r.target)
        f = pm.sr.signature(pm.routine_at(ins, r.pc)) if ins else {}
        for key, name in (
            ("n", "instr"),
            ("mul", "mul"),
            ("biquad", "biquad_pairs"),
            ("fir", "fir"),
            ("loop", "loops"),
            ("recips", "recips"),
            ("clip", "clip"),
        ):
            row[name] = f.get(key, "")
        rows.append(row)
    return rows


def stage_rows(chan_rows: list[dict[str, str]]) -> list[dict[str, int]]:
    """Stage records of channel table `STAGE_TABLE`: `{index, target, core, dm, words}`."""
    out = []
    for r in chan_rows:
        if int(r["table"], 16) != STAGE_TABLE:
            continue
        base = int(r["base"], 16)
        core = 2 if base >= pm.CORE2_BASE else 1
        origin = pm.CORE2_BASE if core == 2 else pm.CORE1_BASE
        out.append(
            {
                "index": int(r["index"]),
                "target": int(r["target"]),
                "core": core,
                "dm": (base - origin) >> 2,
                "words": (int(r["size"], 16) >> 4) + 1,
            }
        )
    return out


def sram_word(image: bytes, dm_word: int) -> int:
    """Little-endian 32-bit data word `dm_word` of a flat SRAM image (`SRAM_BASE_WORD` based)."""
    return struct.unpack_from("<I", image, (dm_word - SRAM_BASE_WORD) * 4)[0]


def check_stages(rows: list[dict[str, int]], image: bytes, core: int) -> tuple[int, int]:
    """`(rows of `core` holding the null stub in word 0, rows of `core`)`."""
    mine = [r for r in rows if r["core"] == core]
    ok = sum(sram_word(image, r["dm"]) & 0xFFFFFF == NULL_STUB[core] for r in mine)
    return ok, len(mine)


def amp_cases(messages: list[tuple[int, int]]) -> dict[int, int]:
    """DM word -> last value written, from `(dm_word, value)` pairs of one Amp type call."""
    out: dict[int, int] = {}
    for dm, value in messages:
        out[dm] = value
    return out


def f32(word: int) -> float:
    return struct.unpack("<f", struct.pack("<I", word & 0xFFFFFFFF))[0]


def cmd_stages(args: argparse.Namespace) -> int:
    rows = stage_rows(pm.read_rows(args.csv))
    image = args.sram.read_bytes() if args.sram else None
    core = 1 if args.sram and args.sram.name.startswith("core1") else 2
    for r in rows:
        print(f"stage row {r['index']:>2} core {r['core']} DM {r['dm']:#x} words {r['words']}")
    if image is not None:
        ok, n = check_stages(rows, image, core)
        print(f"core {core}: {ok} of {n} rows hold the null stub {NULL_STUB[core]:#x} in word 0")
    return 0


def cmd_scan(args: argparse.Namespace) -> int:  # pragma: no cover - needs unicorn + firmware
    import arm_boot as ab

    emu = ab._fresh(args.code, args.sram)
    lo, hi = 0xC0540000, 0xC055C000
    records = scan_code_records(emu.read_bytes(lo, hi - lo), lo)
    listings = {}
    for spec in args.listing:
        t, _, path = spec.partition("=")
        listings[int(t)] = pm.load_listing(Path(path))
    rows = csv_rows(records, listings)
    with args.out.open("w", newline="") as fh:
        w = csv.DictWriter(fh, fieldnames=CSV_FIELDS)
        w.writeheader()
        w.writerows(rows)
    print(f"{len(rows)} FX code records")
    return 0


def cmd_amp(args: argparse.Namespace) -> int:  # pragma: no cover - needs unicorn + firmware
    import arm_boot as ab

    emu = ab._fresh(args.code, args.sram)
    ctx = ab.build_edit_registry(emu)
    emu.write32(ab.REGISTRY_GLOBAL + 0x1C, ctx)
    layer = 0x60000000
    emu.uc.mem_map(layer, 0x100000)
    emu.write32(ab.DISPATCHER_GLOBAL, layer + 0x100000 - 0x1000)
    emu.call(LAYER_CTOR, layer, 0, 0, 0, max_insns=50_000_000)
    for case in range(AMP_CASES):
        emu.clear_messages()
        emu.call(AMP_SWITCH, layer, case, 0, 0, max_insns=5_000_000)
        words = amp_cases([(m.dm_word, m.word2) for m in emu.messages])
        print(f"case {case}:")
        for dm in sorted(words):
            print(f"  DM {dm:#x} = {words[dm]:#010x} ({f32(words[dm]):.6g})")
    return 0


def main(argv: list[str] | None = None) -> int:
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    sub = p.add_subparsers(dest="cmd", required=True)
    st = sub.add_parser("stages", help="the 35 stage rows and the null-stub check")
    st.add_argument("csv", type=Path)
    st.add_argument("sram", type=Path, nargs="?")
    st.set_defaults(fn=cmd_stages)
    sc = sub.add_parser("scan", help="FX code records of the ARM RAM (needs unicorn)")
    sc.add_argument("code", type=Path)
    sc.add_argument("sram", type=Path)
    sc.add_argument("--out", type=Path, required=True)
    sc.add_argument("--listing", action="append", default=[], metavar="T=FILE")
    sc.set_defaults(fn=cmd_scan)
    am = sub.add_parser("amp", help="Amp type switch messages (needs unicorn)")
    am.add_argument("code", type=Path)
    am.add_argument("sram", type=Path)
    am.set_defaults(fn=cmd_amp)
    args = p.parse_args(argv)
    return args.fn(args)


if __name__ == "__main__":
    sys.exit(main())
