#!/usr/bin/env python3
"""Validate the OLED / menu text we draw against the string pool of the OS v1.66 ARM firmware.

The ARM0 image (`bank0_CODE.bin`, load base `0xC0000000`) keeps every user-visible string as a
NUL-terminated ASCII run: menu titles, item labels, enum value names, messages, edit-parameter
names. This tool dumps those runs and checks the literals of our Rust sources (menus, inventory,
Program / Synth OLED pages, piano overlays, type / category tables) against them.

    python tools/ns4_oled_strings.py dump  target/fw_unpacked/bank0_CODE.bin [--min-len 3]
    python tools/ns4_oled_strings.py check target/fw_unpacked/bank0_CODE.bin [--csv out.csv] [--md]
    python tools/ns4_oled_strings.py pointers target/fw_unpacked/bank0_CODE.bin 0x2BF3C8 52

Statuses (strongest first): `exact`, `case` (differs only by letter case), `norm` (differs only by
spacing / punctuation), `affix` (ours is a whole-word prefix / suffix of a firmware string or the
other way round: abbreviation or extension), `frag` (a format string whose fixed words all occur in
firmware strings), `missing`. Strings of 1-3 characters are `weak` (a hit proves little).
Surfaces: `menu` (menu pages), `oled` (OLED page composers), `table` (type / category / model name
tables), `gui` (hover text and widget captions), `legend` (panel silkscreen / control inventory).
Local only: the firmware is never committed and the tool only prints derived text facts.
"""

from __future__ import annotations

import argparse
import csv
import re
import struct
import sys
from dataclasses import dataclass
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
LOAD_BASE = 0xC0000000
STATUSES = ("exact", "case", "norm", "affix", "frag", "missing")
SURFACES = ("menu", "oled", "table", "gui", "legend")

# Source trees whose string literals can reach the screen (relative to the repo root).
SOURCE_GLOBS = (
    "crates/nord-panel/src/**/*.rs",
    "crates/nord-gui/src/panel/**/*.rs",
    "crates/nord-plugin/src/fx_selectors.rs",
)
# First match wins; paths are repo-relative with `/`.
SURFACE_RULES = (
    (r"nord-panel/src/menus/", "menu"),
    (r"nord-panel/src/(inventory|layout|behaviour)/|nord-panel/src/types\.rs", "legend"),
    (
        r"bay_program/(oled|menu_screen|list|name|soft|model|split|morph|clock|actions"
        r"|menu_actions|nav|store|catalog)\.rs",
        "oled",
    ),
    (r"bay_synth/(oled|oled_view|oled_arp|oled_types)\.rs|piano_overlay\.rs", "oled"),
    (r"ui_state/.*(tables|samples)\.rs|fx_selectors\.rs", "table"),
    (
        r"bay_program/controls\.rs|bay_synth/(controls|buttons|page)|bay_piano_select|ui_state/.*buttons",
        "legend",
    ),
    (r".", "gui"),
)
TEST_FILE = re.compile(r"(^|[_/\\])tests?\.rs$|_tests\.rs$|support\.rs$")
TEST_MOD = re.compile(r"#\[cfg\(test\)\]\s*(?:#\[[^\]]*\]\s*)*mod\s+\w+\s*[{;]")
LITERAL = re.compile(r'"((?:[^"\\\n]|\\.)*)"')
FMT_HOLE = re.compile(r"\{[^{}]*\}|%[-+0-9.]*[a-zA-Z]")
IDENT_LIKE = re.compile(r"[a-z0-9_.:/#<>=+()*,\-\[\]]+")
STRING_RUN = re.compile(rb"[\x20-\x7e]{2,}(?=\x00)")
# A run is UI text when it only has text characters and a letter pair (drops code-as-ASCII noise).
UI_TEXT = re.compile(r"[A-Za-z0-9%<(\[+\-][A-Za-z0-9 .,:;'()/+&#%!?=<>\[\]*\-]*")
# File-offset window of the menu / label string pool in OS v1.66 (outside it: edit-parameter names,
# class names, the 128-step value tables).
UI_POOL = (0x283000, 0x2E2000)


def pointer_table(image: bytes, offset: int, count: int) -> list[str | None]:
    """Strings behind `count` consecutive 32-bit pointers at file `offset` (`None` = not a string).

    Pointers are absolute (`LOAD_BASE + file offset`); a pair table such as the program category
    table (`0xC02BF3C8`: full name, short name) is read with `count = 2 * entries`.
    """
    out: list[str | None] = []
    for i in range(count):
        (word,) = struct.unpack_from("<I", image, offset + 4 * i)
        start = word - LOAD_BASE
        end = image.find(b"\x00", start) if 0 <= start < len(image) else -1
        raw = image[start:end] if end > start else b""
        ok = 0 < len(raw) <= 60 and all(0x20 <= c < 0x7F for c in raw)
        out.append(raw.decode("ascii") if ok else None)
    return out


def surface_of(path: str) -> str:
    for pattern, name in SURFACE_RULES:
        if re.search(pattern, path):
            return name
    return "gui"


@dataclass(frozen=True)
class Literal:
    path: str
    line: int
    text: str

    @property
    def surface(self) -> str:
        return surface_of(self.path)


@dataclass(frozen=True)
class Verdict:
    status: str
    fw_text: str
    offset: int
    weak: bool


def fw_strings(image: bytes, min_len: int = 2) -> list[tuple[int, str]]:
    """`(file offset, text)` of each NUL-terminated printable ASCII run."""
    return [
        (m.start(), m.group().decode("ascii"))
        for m in STRING_RUN.finditer(image)
        if len(m.group()) >= min_len
    ]


def ui_text(text: str) -> bool:
    return bool(UI_TEXT.fullmatch(text)) and bool(re.search(r"[A-Za-z0-9]", text))


def norm(text: str) -> str:
    return re.sub(r"[^0-9a-z]", "", text.casefold())


def words(text: str) -> list[str]:
    return re.findall(r"[0-9a-z]+", text.casefold())


class Pool:
    """Lookup structure over the firmware strings."""

    def __init__(self, runs: list[tuple[int, str]], window: tuple[int, int] = UI_POOL) -> None:
        self.runs = [(o, t) for o, t in runs if ui_text(t)]
        self.exact: dict[str, int] = {}
        self.fold: dict[str, tuple[int, str]] = {}
        self.norm: dict[str, tuple[int, str]] = {}
        for off, text in self.runs:
            self.exact.setdefault(text, off)
            self.fold.setdefault(text.casefold(), (off, text))
            self.norm.setdefault(norm(text), (off, text))
        lo, hi = window
        self._words = [
            (off, text, words(text))
            for off, text in self.runs
            if lo <= off < hi and "_" not in text and "%" not in text
        ]
        self._all_words = {w for _, _, ws in self._words for w in ws}

    def affix(self, text: str) -> tuple[int, str] | None:
        """Shortest pool string that contains ours (or that ours contains) on word boundaries."""
        mine = words(text)
        if not mine or len(norm(text)) < 4:
            return None
        best: tuple[int, str] | None = None
        for off, fw, theirs in self._words:
            if not theirs or len(norm(fw)) < 4 or theirs == mine:
                continue
            short, long_ = (mine, theirs) if len(mine) <= len(theirs) else (theirs, mine)
            if long_[: len(short)] == short or long_[len(long_) - len(short) :] == short:
                if best is None or len(fw) < len(best[1]):
                    best = (off, fw)
        return best

    def has_word(self, word: str) -> bool:
        return word in self._all_words


def classify(text: str, pool: Pool) -> Verdict:
    weak = len(norm(text)) < 4
    if FMT_HOLE.search(text):  # a Rust format string: only its fixed words can be compared
        frags = [
            w for f in FMT_HOLE.split(text) for w in words(f) if len(w) >= 3 and not w.isdigit()
        ]
        if frags and all(pool.has_word(w) for w in frags):
            return Verdict("frag", "", 0, weak)
        return Verdict("missing", "", 0, weak)
    if text in pool.exact:
        return Verdict("exact", text, pool.exact[text], weak)
    hit = pool.fold.get(text.casefold())
    if hit:
        return Verdict("case", hit[1], hit[0], weak)
    hit = pool.norm.get(norm(text))
    if hit and norm(text):
        return Verdict("norm", hit[1], hit[0], weak)
    hit = pool.affix(text)
    if hit:
        return Verdict("affix", hit[1], hit[0], weak)
    return Verdict("missing", "", 0, weak)


def displayable(text: str) -> bool:
    """Literals that could be screen text (drops ids, paths, key names, prose sentences)."""
    if not re.search(r"[A-Za-z]", text) or "\\" in text:
        return False
    if IDENT_LIKE.fullmatch(text) and " " not in text:
        return False
    if text.startswith(("engine:", "http", ".")) or "::" in text:
        return False
    return len(text) <= 40 and len(text.split()) <= 5 and ". " not in text


def source_files(root: Path = ROOT) -> list[Path]:
    seen: dict[Path, None] = {}
    for pattern in SOURCE_GLOBS:
        for p in sorted(root.glob(pattern)):
            if not TEST_FILE.search(p.name) and "/tests/" not in p.as_posix():
                seen[p] = None
    return list(seen)


def strip_tests(src: str) -> str:
    m = TEST_MOD.search(src)
    return src[: m.start()] if m else src


def our_literals(root: Path = ROOT) -> list[Literal]:
    out: list[Literal] = []
    for path in source_files(root):
        src = strip_tests(path.read_text(encoding="utf-8"))
        rel = path.relative_to(root).as_posix()
        for m in LITERAL.finditer(src):
            text = m.group(1).replace('\\"', '"')
            if displayable(text):
                out.append(Literal(rel, src.count("\n", 0, m.start()) + 1, text))
    return out


def check(pool: Pool, lits: list[Literal]) -> list[tuple[Literal, Verdict]]:
    return [(lit, classify(lit.text, pool)) for lit in lits]


def counts(
    rows: list[tuple[Literal, Verdict]], surface: str | None = None, strong: bool = False
) -> dict[str, int]:
    """Unique literal counts per status (optionally one surface, optionally without weak ones)."""
    seen: set[tuple[str, str]] = set()
    out = dict.fromkeys(STATUSES, 0)
    for lit, v in rows:
        if (surface is not None and lit.surface != surface) or (strong and v.weak):
            continue
        if (lit.text, v.status) not in seen:
            seen.add((lit.text, v.status))
            out[v.status] += 1
    return out


def markdown(
    rows: list[tuple[Literal, Verdict]], statuses: tuple[str, ...], surfaces: tuple[str, ...]
) -> str:
    lines = ["| Ours | Firmware | Offset | Status | Where |", "|---|---|---|---|---|"]
    done: set[tuple[str, str]] = set()
    for lit, v in rows:
        if v.status not in statuses or lit.surface not in surfaces:
            continue
        if (lit.text, v.status) in done:
            continue
        done.add((lit.text, v.status))
        fw = f"`{v.fw_text}`" if v.fw_text else ""
        off = f"`0x{v.offset:06x}`" if v.fw_text else ""
        weak = " (weak)" if v.weak else ""
        lines.append(f"| `{lit.text}` | {fw} | {off} | {v.status}{weak} | {lit.path}:{lit.line} |")
    return "\n".join(lines)


def write_csv(rows: list[tuple[Literal, Verdict]], path: Path) -> None:
    with path.open("w", newline="", encoding="utf-8") as fh:
        w = csv.writer(fh)
        w.writerow(["surface", "status", "weak", "ours", "firmware", "fw_offset", "file", "line"])
        for lit, v in rows:
            off = f"0x{v.offset:06x}" if v.fw_text else ""
            w.writerow(
                [lit.surface, v.status, int(v.weak), lit.text, v.fw_text, off, lit.path, lit.line]
            )


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)
    d = sub.add_parser("dump", help="print firmware string runs as `offset<TAB>text`")
    d.add_argument("image", type=Path)
    d.add_argument("--min-len", type=int, default=3)
    p = sub.add_parser(
        "pointers", help="read a table of string pointers (`offset` and `count` in hex ok)"
    )
    p.add_argument("image", type=Path)
    p.add_argument("offset", type=lambda x: int(x, 0))
    p.add_argument("count", type=lambda x: int(x, 0))
    c = sub.add_parser("check", help="match our literals against the firmware strings")
    c.add_argument("image", type=Path)
    c.add_argument("--csv", type=Path, help="write one row per literal occurrence")
    c.add_argument("--md", action="store_true", help="print the non-exact rows as markdown")
    c.add_argument(
        "--surfaces",
        default="menu,oled,table",
        help="surfaces listed by --csv / --md (the counts always cover all of them)",
    )
    args = ap.parse_args(argv)
    if not args.image.is_file():
        print(f"firmware image not found: {args.image}", file=sys.stderr)
        return 2
    data = args.image.read_bytes()
    if args.cmd == "pointers":
        for i, text in enumerate(pointer_table(data, args.offset, args.count)):
            print(f"{args.offset + 4 * i:06x}	{text}")
        return 0
    runs = fw_strings(data, getattr(args, "min_len", 2))
    if args.cmd == "dump":
        for off, text in runs:
            print(f"{off:06x}\t{text}")
        return 0
    pool = Pool(runs)
    rows = check(pool, our_literals())
    print(f"firmware runs: {len(runs)} ({len(pool.runs)} text); our literals: {len(rows)}")
    for surface in SURFACES:
        print(f"{surface:7s}", counts(rows, surface), "strong:", counts(rows, surface, True))
    wanted = tuple(args.surfaces.split(","))
    if args.csv:
        write_csv([r for r in rows if r[0].surface in wanted], args.csv)
    if args.md:
        print(markdown(rows, STATUSES[1:], wanted))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
