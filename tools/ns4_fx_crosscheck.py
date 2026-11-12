"""Cross-checks our decoded FX enums against third-party ``ns4decode`` CSV output.

For every FX block of every program in a decoded corpus, compares 9 fields (Mod 1/2 mode, Reverb
type and color, Delay filter/effects/analog/ping-pong, Amp mode) as predicted from the raw
``ns4 convert --to-json`` fields with ``ns4decode``'s text. Result on the 384 factory programs
(OS v1.66 Factory Restore RevK): 17,280 comparisons, 0 mismatches (see
``docs/research/hardware-audio-reference-fx.md``).

Usage::

    ns4decode.py --nogui --csv --writeto <dir>/pNNN <dir>/pNNN/prog.ns4p   # per program
    python tools/ns4_fx_crosscheck.py <corpus_dir> <path/to/ns4.exe>

Name order of the tables is VERIFIED against UI strings in OS v1.66 ``bank0_CODE.bin``
(``0x2cc4e4..0x2cc918``); this script verifies the bit layout and numeric indices only.
"""

from __future__ import annotations

import csv
import json
import subprocess
import sys
from collections import Counter, defaultdict
from pathlib import Path
from typing import Any

MOD1 = [
    "A-PAN",
    "A-PAN variation",
    "TREM",
    "TREM variation",
    "RM",
    "RM variation",
    "A-WAH",
    "A-WAH variation",
    "WAH",
    "WAH variation",
    "PUMP",
    "PUMP variation",
]
MOD2 = [
    "PHAS",
    "PHAS variation",
    "FLANG",
    "FLANG variation",
    "CHOR",
    "CHOR variation",
    "VIBE",
    "VIBE variation",
    "ENS",
    "ENS variation",
    "SPIN",
    "SPIN variation",
]
REV = [
    "SPRING",
    "SPRING variation",
    "BOOTH",
    "BOOTH variation",
    "ROOM",
    "ROOM variation",
    "STAGE",
    "STAGE variation",
    "HALL",
    "HALL variation",
    "CATH",
    "CATH variation",
]
DFX = [
    "none",
    "none",
    "ENS",
    "ENS variation",
    "VIBE",
    "VIBE variation",
    "CHOR",
    "CHOR variation",
    "FLAM",
    "FLAM variation",
    "SPACE",
    "SPACE variation",
]
DFLT = ["none", "LP", "HP", "BP"]
AMP = [
    "off",
    "TWIN",
    "TWIN variation",
    "JC",
    "JC variation",
    "SMALL",
    "SMALL variation",
    "TO ROTARY",
    "LP FILTER",
    "HP FILTER",
]
COLOR = ["none", "DARK", "BRIGHT"]


def predict(fx: dict[str, Any]) -> dict[str, str]:
    t4 = (fx["reverb_type_raw"] << 1) | ((fx["amp_type_new"] >> 8) & 1)
    mode = fx["delay_mode"]
    return {
        "FX mod 1 mode": MOD1[fx["mod1_type_raw"]] if fx["mod1_type_raw"] < 12 else "?",
        "FX mod 2 mode": MOD2[fx["mod2_type_raw"]] if fx["mod2_type_raw"] < 12 else "?",
        "FX reverb type": REV[t4] if t4 < 12 else "?",
        "FX reverb dark/bright": COLOR[fx["reverb_color"]] if fx["reverb_color"] < 3 else "?",
        "FX delay filter type": DFLT[
            (int(fx["delay_ping_pong"]) << 1) | int(fx["delay_fb_filter"])
        ],
        "FX delay normal/analog": "analog" if mode & 2 else "normal",
        "FX delay ping pong on/off": "on" if mode & 1 else "off",
        "FX delay effects": DFX[fx["delay_fb_fx_type"]] if fx["delay_fb_fx_type"] < 12 else "?",
        "FX amp sim/EQ mode": AMP[fx["amp_type_new"] & 15]
        if (fx["amp_type_new"] & 15) < 10
        else "?",
    }


def read_csv(path: Path) -> dict[str, list[str]]:
    rows: dict[str, list[str]] = {}
    with open(path, encoding="utf-8", errors="replace", newline="") as fh:
        for r in csv.reader(fh):
            if r:
                rows[r[0].strip()] = [c.strip() for c in r[1:]]
    return rows


def compare_corpus(root: Path, exe: Path) -> tuple[Counter, dict[str, Counter], int]:
    stats: Counter = Counter()
    bad: dict[str, Counter] = defaultdict(Counter)
    total = 0
    for pd in sorted(p for p in root.iterdir() if p.is_dir()):
        prog_path, json_path = pd / "prog.ns4p", pd / "prog.json"
        subprocess.run(
            [str(exe), "convert", str(prog_path), "--to-json", str(json_path)],
            capture_output=True,
            check=False,
        )
        try:
            prog = json.loads(json_path.read_text(encoding="utf-8"))
        except (OSError, ValueError):
            continue
        blocks = {
            "organ": [prog["organ"]["fx"]],
            "piano": [prog["piano"]["fx_a"], prog["piano"]["fx_b"]],
            "synth": [prog["synth"]["fx_a"], prog["synth"]["fx_b"], prog["synth"]["fx_c"]],
        }
        for sec, fxs in blocks.items():
            csv_path = pd / f"prog_{sec}.csv"
            if not csv_path.exists():
                continue
            rows = read_csv(csv_path)
            for ci, fx in enumerate(fxs):
                for name, want in predict(fx).items():
                    got_row = next(
                        (
                            v
                            for k, v in rows.items()
                            if k.startswith(name) and not k.endswith(("wheel", "A.T.", "ctrlped"))
                        ),
                        None,
                    )
                    if got_row is None or ci >= len(got_row):
                        continue
                    total += 1
                    ok = got_row[ci] == want
                    stats[(name, ok)] += 1
                    if not ok:
                        bad[name][(want, got_row[ci])] += 1
    return stats, bad, total


def main(argv: list[str] | None = None) -> int:
    args = sys.argv[1:] if argv is None else argv
    if len(args) != 2:
        print(__doc__)
        return 2
    stats, bad, total = compare_corpus(Path(args[0]), Path(args[1]))
    print("compared", total)
    mismatches = 0
    for name in sorted({k[0] for k in stats}):
        ok, ko = stats[(name, True)], stats[(name, False)]
        mismatches += ko
        print(f"{name:28s} match {ok:5d}  mismatch {ko:4d}")
        for (w, g), c in bad[name].most_common(4):
            print(f"     ours={w!r} ns4decode={g!r} x{c}")
    return 1 if mismatches else 0


if __name__ == "__main__":
    raise SystemExit(main())
