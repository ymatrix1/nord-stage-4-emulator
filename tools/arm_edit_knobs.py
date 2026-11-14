#!/usr/bin/env python3
"""Knob -> DSP word laws of the edit ids, captured with Q16.16 value cells.

`arm_edit_capture.py` (the first capture) wrote plain integers `0..127` into the value cells,
which hold Q16.16 numbers (`knob << 16`, [synth-stage-models-10](../docs/research/synth-stage-models-10.md)
s1). Every integer-knob id then saw position 0 and looked value independent. This tool re-drives
the ids with `arm_edit_capture.cell_word` in both encodings (`auto`: Q16.16 for ranges of 12 or
more steps; `q16`: Q16.16 for every id, run for the small ranges as well), applies **every** value
of a range of up to 128 steps (9 samples for wider ones) from the settled state, and records the
last word that each DSP address received (`last_words`). The CSV is the input of
`tools/knob_laws.py`.

    uv run --with unicorn --with capstone python tools/arm_edit_knobs.py \\
        target/fw_unpacked/bank0_CODE.bin target/fw_unpacked/bank0_SRAM.bin \\
        --cache target/settled.snap --filter "Edt_Syn" --layers 4 --shard 0/8 --out knobs0.csv

Output columns: `id,name,layer,kind,mode,value,dm_word,word,dest` (`dest` = receiving chips and
cores). Run the shards in parallel (one process each) and concatenate with `merge`.
"""

from __future__ import annotations

import argparse
import csv
import io
import sys
from pathlib import Path

if __package__ in (None, ""):
    sys.path.insert(0, str(Path(__file__).resolve().parent))

import arm_edit_capture as cap  # noqa: E402

FIELDS = ["id", "name", "layer", "kind", "mode", "value", "dm_word", "word", "dest"]
#: Largest range (in steps) that is swept value by value.
DENSE_SPAN = 127
#: Instructions run after the dispatch (deferred senders; the dispatch itself speaks directly).
WINDOW = 20_000


def sample_values(lo: int, hi: int) -> list[int]:
    """Every value of a range of up to `DENSE_SPAN + 1` steps, else nine evenly spaced ones."""
    if hi - lo <= DENSE_SPAN:
        return list(range(lo, hi + 1))
    return sorted({lo + (hi - lo) * k // 8 for k in range(9)})


def modes_for(p: cap.Param) -> list[str]:
    """Cell encodings to try: `auto`, plus `q16` where `auto` would write plain integers."""
    return ["auto"] if p.maximum - p.minimum >= cap.Q16_MIN_SPAN else ["auto", "q16"]


def last_words(messages) -> dict[tuple[int, str], int]:  # noqa: ANN001
    """`{(dm_word, dest): word}` of the last message that each address received."""
    out: dict[tuple[int, str], int] = {}
    for m in messages:
        out[(m.dm_word, f"{m.chip}/{m.core}/{m.path}")] = m.word2
    return out


def fold_destinations(words: dict[tuple[int, str], int]) -> list[tuple[int, int, str]]:
    """`(dm_word, word, dest)` with the destinations that got the same word merged."""
    by_word: dict[tuple[int, int], list[str]] = {}
    for (dm, dest), word in sorted(words.items()):
        by_word.setdefault((dm, word), []).append(dest)
    return [(dm, word, " ".join(d)) for (dm, word), d in sorted(by_word.items())]


def shard_of(items: list, index: int, count: int) -> list:
    """Round-robin shard `index` of `count`."""
    return [x for i, x in enumerate(items) if i % count == index]


def parse_shard(text: str) -> tuple[int, int]:
    """`"2/8"` -> `(2, 8)`."""
    index, count = (int(x) for x in text.split("/"))
    if not 0 <= index < count:
        raise ValueError(f"bad shard {text}")
    return index, count


def csv_text(rows: list[dict[str, str]], header: bool = True) -> str:
    """CSV of `FIELDS` rows."""
    buf = io.StringIO()
    writer = csv.DictWriter(buf, FIELDS, lineterminator="\n")
    if header:
        writer.writeheader()
    writer.writerows(rows)
    return buf.getvalue()


def merge(texts: list[str]) -> str:
    """Concatenate shard CSVs (one header) sorted by id, mode and value."""
    rows = [r for t in texts for r in csv.DictReader(io.StringIO(t))]
    rows.sort(
        key=lambda r: (int(r["id"], 0), r["layer"], r["mode"], int(r["value"]), int(r["dm_word"], 0))
    )
    return csv_text(rows)


def capture(  # pragma: no cover - needs the booted firmware
    code: Path,
    sram: Path | None,
    cache: Path | None,
    params: list[cap.Param],
    layers: set[int] | None,
    fx_units: set[int] | None,
    window: int,
) -> list[dict[str, str]]:
    import arm_emu as ae
    import arm_steady as ast

    emu, blob = cap.settled_emulator(code, sram, cache)
    if not any(b <= cap.SCRATCH <= e for b, e, _ in emu.uc.mem_regions()):
        emu.uc.mem_map(cap.SCRATCH, ae.PAGE)
    if fx_units is None:
        stores = cap.first_store_per_layer(cap.find_stores(emu))
    else:  # the FX unit objects are no dispatcher clients (`docs/research/fx-contexts.md` s2)
        import fx_clients as fc

        all_params = cap.load_params(Path("docs/research/arm-edit-params.csv"), ".")
        markers = {p.name: p.edit_id for p in all_params if p.name in fc.KIND_MARKERS}
        stores = fc.stores_of(fc.find_units(emu, markers), fx_units)
    out: list[dict[str, str]] = []
    for store in stores:
        if layers is not None and store.layer not in layers:
            continue
        for p in params:
            if cap.value_cell(emu, store.address, p.edit_id) is None:
                continue
            for mode in modes_for(p):
                for v in sample_values(p.minimum, p.maximum):
                    ast.restore(emu, blob)
                    emu.install_dsp_ports()
                    during, later, _err = cap.apply_value(
                        emu, store.address, p.edit_id, cap.cell_word(p, v, mode), window
                    )
                    for dm, word, dest in fold_destinations(last_words(during + later)):
                        out.append(
                            {
                                "id": f"{p.edit_id:#x}",
                                "name": p.name,
                                "layer": str(store.layer),
                                "kind": store.kind,
                                "mode": mode,
                                "value": str(v),
                                "dm_word": f"{dm:#x}",
                                "word": f"{word:#010x}",
                                "dest": dest,
                            }
                        )
        print(f"layer {store.layer} ({store.kind}): {len(out)} rows", flush=True)
    return out


def main(argv: list[str] | None = None) -> int:  # pragma: no cover - thin CLI
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = ap.add_subparsers(dest="cmd", required=True)
    run = sub.add_parser("run")
    run.add_argument("code", type=Path)
    run.add_argument("sram", type=Path, nargs="?")
    run.add_argument("--cache", type=Path)
    run.add_argument("--params", type=Path, default=Path("docs/research/arm-edit-params.csv"))
    run.add_argument("--filter", default="Edt_Syn")
    run.add_argument("--layers", help="comma separated client indices")
    run.add_argument("--fx", help="comma separated FX unit indices (instead of the layer clients)")
    run.add_argument("--small", action="store_true", help="only ids below 12 steps")
    run.add_argument("--shard", default="0/1")
    run.add_argument("--window", type=int, default=WINDOW)
    run.add_argument("--out", type=Path, required=True)
    mg = sub.add_parser("merge")
    mg.add_argument("inputs", type=Path, nargs="+")
    mg.add_argument("--out", type=Path, required=True)
    ns = ap.parse_args(argv)
    if ns.cmd == "merge":
        ns.out.write_text(merge([p.read_text(encoding="utf-8") for p in ns.inputs]), encoding="utf-8")
        return 0
    index, count = parse_shard(ns.shard)
    found = cap.load_params(ns.params, ns.filter)
    if ns.small:
        found = [p for p in found if p.maximum - p.minimum < cap.Q16_MIN_SPAN]
    params = shard_of(found, index, count)
    layers = {int(x) for x in ns.layers.split(",")} if ns.layers else None
    fx = {int(x) for x in ns.fx.split(",")} if ns.fx else None
    rows = capture(ns.code, ns.sram, ns.cache, params, layers, fx, ns.window)
    ns.out.write_text(csv_text(rows), encoding="utf-8")
    print(f"{len(rows)} rows from {len(params)} ids")
    return 0


if __name__ == "__main__":
    sys.exit(main())
