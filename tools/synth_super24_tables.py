#!/usr/bin/env python3
"""Extracts and loads the table words of the SUPER 2..4 stage `0x124985`.

The stage reads four tables (`docs/research/synth-stage-models-12.md` s2):

* `rows`: PM `0x9631F`, 3072 words, the ARM boot log block `0x96xxx` (`pno_core2.script`);
* `wave`: DM `0x08030000`, 985 words: the ARM boot copier `0xC01C0B3C` writes 5,910 words to the
  L2 byte window `0x200C0000..` (`pno_l2.script`), six identical segments of 985 words at
  `+10000 k` words (checked by `extract`); one is kept;
* `fir`: PM `0x9531F`, 4096 words of the DSP image (512 phases x 8 taps);
* `lattice`: PM `0x95000`, 5 words of the DSP image (the three biquads).

    python tools/synth_super24_tables.py extract --image SRAM_1_PNO/core2_sram.bin
    --boot-dir target/bulk (and --out, default: the committed file)
    python tools/synth_super24_tables.py check          # fingerprint of the committed file

The committed file holds numbers only (derived tables).
"""

from __future__ import annotations

import argparse
import csv
import hashlib
import subprocess
import sys
import tempfile
from pathlib import Path

DEFAULT_FILE = (
    Path(__file__).resolve().parent.parent / "docs/research/data/synth-super24-tables.txt"
)
# VERIFIED (run of `extract`, section sizes of the dump): word counts of the four tables.
SECTIONS = (("rows", 3072), ("wave", 985), ("fir", 4096), ("lattice", 5))
ROWS_BASE = 0x9631F
FIR_BASE = 0x9531F
LATTICE_BASE = 0x95000
WAVE_BASE = 0x08030000
# VERIFIED (`tools/synth_super24_tables.py check`): SHA-256 of the committed file.
FINGERPRINT = "96eb31050eec542ed6c6ca2b68bd31620701485c53e2ae4e7a6b64612084a7c8"
PER_LINE = 12


def render(tables: dict[str, list[int]]) -> str:
    lines = []
    for name, count in SECTIONS:
        words = tables[name]
        if len(words) != count:
            raise ValueError(f"{name}: {len(words)} words, want {count}")
        lines.append(f"# {name} {count}")
        for i in range(0, count, PER_LINE):
            lines.append(" ".join(f"{w:08x}" for w in words[i : i + PER_LINE]))
    return "\n".join(lines) + "\n"


def parse(text: str) -> dict[str, list[int]]:
    tables: dict[str, list[int]] = {}
    cur = None
    for line in text.splitlines():
        if line.startswith("#"):
            cur = line.split()[1]
            tables[cur] = []
        elif line.strip() and cur:
            tables[cur] += [int(t, 16) for t in line.split()]
    return tables


def load(path: Path = DEFAULT_FILE) -> dict[str, list[int]]:
    tables = parse(path.read_text())
    for name, count in SECTIONS:
        if len(tables.get(name, [])) != count:
            raise ValueError(f"{path}: section {name} has the wrong length")
    return tables


def fingerprint(path: Path = DEFAULT_FILE) -> str:
    return hashlib.sha256(path.read_bytes().replace(b"\r\n", b"\n")).hexdigest()


def boot_words(script: Path, lo: int, count: int) -> list[int]:  # pragma: no cover - local data
    vals = {}
    for line in script.read_text().splitlines():
        parts = line.split()
        if len(parts) == 3:
            vals[int(parts[1], 16)] = int(parts[2], 16)
    return [vals[lo + i] for i in range(count)]


def l2_words(script: Path) -> list[int]:  # pragma: no cover - local data
    """One 985-word segment of the PNO L2 stores (the 6 segments at `+40000 k` bytes are equal)."""
    pairs = sorted(
        (int(a, 16), int(v, 16)) for a, v in (ln.split() for ln in script.read_text().splitlines())
    )
    words = [v for _, v in pairs]
    if len(words) != 6 * 985 or any(
        words[985 * k : 985 * (k + 1)] != words[:985] for k in range(6)
    ):
        raise ValueError("the six L2 segments are not identical")
    return words[:985]


def image_words(
    sharc_org: str, image: str, lo: int, count: int
) -> list[int]:  # pragma: no cover - local data
    with tempfile.TemporaryDirectory() as tmp:
        cp = Path(tmp, "o.csv")
        cmd = [sharc_org, image, "--passes", "1", "--buf", f"{lo:#x}:{count}:f32", "--csv", str(cp)]
        subprocess.run(cmd, capture_output=True, text=True, check=False)
        rows = list(csv.DictReader(cp.open()))
    return [int(r["raw"], 16) for r in rows]


def main(argv: list[str] | None = None) -> int:  # pragma: no cover - thin CLI
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = ap.add_subparsers(dest="cmd", required=True)
    ex = sub.add_parser("extract")
    ex.add_argument("--image", required=True)
    ex.add_argument("--boot-dir", required=True)
    ex.add_argument("--sharc-org", default="target/release/sharc-org")
    ex.add_argument("--out", default=str(DEFAULT_FILE))
    sub.add_parser("check")
    ns = ap.parse_args(argv)
    if ns.cmd == "check":
        print(fingerprint())
        return 0
    boot = Path(ns.boot_dir)
    tables = {
        "rows": boot_words(boot / "pno_core2.script", ROWS_BASE, 3072),
        "wave": l2_words(boot / "pno_l2.script"),
        "fir": image_words(ns.sharc_org, ns.image, FIR_BASE, 4096),
        "lattice": image_words(ns.sharc_org, ns.image, LATTICE_BASE, 5),
    }
    Path(ns.out).write_text(render(tables), newline="\n")
    print(fingerprint(Path(ns.out)))
    return 0


if __name__ == "__main__":
    sys.exit(main())
