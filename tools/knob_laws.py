#!/usr/bin/env python3
"""Fold the Q16.16 knob capture (`arm_edit_knobs.py`) into one law row per (id, DSP word).

    python tools/knob_laws.py knobs.csv --out docs/research/data/synth-knob-laws.csv

A row says which DSP word a knob drives and what it receives for every knob value: `const` when
the word does not depend on the value, else `varies` with the words listed in value order
(`v_lo` to `v_hi`). The float32 reading of the words is fitted with a line (`lin_err`) and with an
exponential (`exp_err`, relative error of `log2(word)` against the value) so the law can be read
off without the table; both are `-` when the words are not finite positive floats. `q16_only`
marks ids whose range is below 12 steps that speak only with the Q16.16 cell (plain integers left
the word at its idle value: the first capture's blind spot).

Locations use the channel-strip layout of `tools/synth_strips.py` (`strip K word W`, `common L word
W`). The result feeds `crates/nord-core/src/synth/fw_knob_laws.rs` (fingerprint test) and
`docs/research/synth-knob-laws.md`.
"""

from __future__ import annotations

import argparse
import csv
import io
import math
import struct
import sys
from pathlib import Path

if __package__ in (None, ""):
    sys.path.insert(0, str(Path(__file__).resolve().parent))

from synth_strips import word_location  # noqa: E402

FIELDS = [
    "id",
    "name",
    "layer",
    "dm_word",
    "where",
    "dest",
    "mode",
    "v_lo",
    "v_hi",
    "values",
    "kind",
    "lin_err",
    "exp_err",
    "q16_only",
    "words",
]


def as_float(word: int) -> float:
    """The IEEE float32 whose bit pattern is `word`."""
    return struct.unpack("<f", struct.pack("<I", word & 0xFFFFFFFF))[0]


def where(dm: int) -> str:
    """Human location of a DM word (`strip K word W`, `common L word W` or the address)."""
    kind, k, w = word_location(dm)
    return f"{kind} {k} word {w}" if kind != "other" else f"{dm:#x}"


def linear_fit_error(values: list[int], words: list[int]) -> float | None:
    """Max |float - line| / (max float - min float) of the float32 words against the values."""
    ys = [as_float(w) for w in words]
    if len(set(ys)) < 2 or not all(math.isfinite(y) for y in ys):
        return None
    n = len(values)
    mx, my = sum(values) / n, sum(ys) / n
    sxx = sum((x - mx) ** 2 for x in values)
    slope = sum((x - mx) * (y - my) for x, y in zip(values, ys)) / sxx
    err = max(abs(y - (my + slope * (x - mx))) for x, y in zip(values, ys))
    return err / (max(ys) - min(ys))


def exp_fit_error(values: list[int], words: list[int]) -> float | None:
    """Max error of `log2(float)` against a line in the value, in octaves; None unless positive."""
    ys = [as_float(w) for w in words]
    if len(set(ys)) < 2 or not all(math.isfinite(y) and y > 0 for y in ys):
        return None
    logs = [math.log2(y) for y in ys]
    n = len(values)
    mx, my = sum(values) / n, sum(logs) / n
    sxx = sum((x - mx) ** 2 for x in values)
    slope = sum((x - mx) * (y - my) for x, y in zip(values, logs)) / sxx
    return max(abs(y - (my + slope * (x - mx))) for x, y in zip(values, logs))


def fmt_err(err: float | None) -> str:
    return "-" if err is None else f"{err:.3g}"


def fold(rows: list[dict[str, str]]) -> list[dict[str, str]]:
    """One law row per `(id, layer, dm_word, dest, mode)`; words listed in value order."""
    groups: dict[tuple, dict[int, int]] = {}
    names: dict[int, str] = {}
    for r in rows:
        eid = int(r["id"], 0)
        names[eid] = r["name"]
        key = (eid, r["layer"], int(r["dm_word"], 0), r["dest"], r["mode"])
        groups.setdefault(key, {})[int(r["value"])] = int(r["word"], 0)
    out = []
    for (eid, layer, dm, dest, mode), series in sorted(groups.items()):
        values = sorted(series)
        words = [series[v] for v in values]
        varies = len(set(words)) > 1
        other = groups.get((eid, layer, dm, dest, "auto" if mode == "q16" else "q16"))
        out.append(
            {
                "id": f"{eid:#x}",
                "name": names[eid],
                "layer": layer,
                "dm_word": f"{dm:#x}",
                "where": where(dm),
                "dest": dest,
                "mode": mode,
                "v_lo": str(values[0]),
                "v_hi": str(values[-1]),
                "values": value_list(values),
                "kind": "varies" if varies else "const",
                "lin_err": fmt_err(linear_fit_error(values, words)) if varies else "-",
                "exp_err": fmt_err(exp_fit_error(values, words)) if varies else "-",
                "q16_only": str(mode == "q16" and varies and other is not None and _flat(other)),
                "words": " ".join(f"{w:#010x}" for w in words) if varies else f"{words[0]:#010x}",
            }
        )
    return out


def value_list(values: list[int]) -> str:
    """`lo-hi` for a contiguous run of values, else the comma separated list (a value that sent
    no message leaves a gap)."""
    if values == list(range(values[0], values[-1] + 1)):
        return f"{values[0]}-{values[-1]}"
    return ",".join(str(v) for v in values)


def _flat(series: dict[int, int]) -> bool:
    return len(set(series.values())) <= 1


def laws_csv(laws: list[dict[str, str]]) -> str:
    buf = io.StringIO()
    writer = csv.DictWriter(buf, FIELDS, lineterminator="\n")
    writer.writeheader()
    writer.writerows(laws)
    return buf.getvalue()


def read_laws(text: str) -> list[dict[str, str]]:
    return list(csv.DictReader(io.StringIO(text)))


def series_of(law: dict[str, str]) -> dict[int, int]:
    """`{value: word}` of a `varies` row (a `const` row has one word for every value)."""
    lo, hi = int(law["v_lo"]), int(law["v_hi"])
    words = [int(w, 0) for w in law["words"].split()]
    if len(words) == 1:
        return {v: words[0] for v in range(lo, hi + 1)}
    spec = law["values"]
    if "," in spec:
        values = [int(v) for v in spec.split(",")]
    else:
        values = list(range(lo, hi + 1))
    return dict(zip(values, words))


def main(argv: list[str] | None = None) -> int:  # pragma: no cover - thin CLI
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("knobs", type=Path)
    ap.add_argument("--out", type=Path)
    ns = ap.parse_args(argv)
    rows = list(csv.DictReader(ns.knobs.open(encoding="utf-8")))
    text = laws_csv(fold(rows))
    if ns.out:
        ns.out.write_text(text, encoding="utf-8")
    else:
        sys.stdout.write(text)
    return 0


if __name__ == "__main__":
    sys.exit(main())
