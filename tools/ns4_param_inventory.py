#!/usr/bin/env python3
"""Inventory of the Nord Stage 4 preset parameters: bit offset, width and observed range.

The field map is the one of `docs/specs/cbin-and-presets.md` section 4.1 (same offsets as
`crates/nord-core/src/cbin/*_preset.rs`). Every field is read straight from the bitstream and the
minimum, maximum and number of distinct values over a corpus are printed, so a width that is too
small, a range that the format never uses, or a 7-bit field that only ever holds a few values is
visible. Morphable fields (`4W + 3` bits) report the base value only.

    python tools/ns4_param_inventory.py FACTORY.ns4b             # whole backup (presets only)
    python tools/ns4_param_inventory.py presets_dir --section organ
    python tools/ns4_param_inventory.py FACTORY.ns4b --list      # the field map, no corpus
"""

from __future__ import annotations

import argparse
import sys
import zipfile
from collections.abc import Iterator
from dataclasses import dataclass
from pathlib import Path

CBIN_HEADER = 48
# Program payload after the 48-byte CBIN header: global 45 B, organ 135, piano 147, synth 493.
PROGRAM_BLOCKS = {"organ": (45, 135), "piano": (180, 147), "synth": (327, 493)}
STANDALONE = {"ns4o": "organ", "ns4n": "piano", "ns4y": "synth"}


@dataclass(frozen=True)
class Param:
    block: str  # organ, piano, synth or fx
    name: str
    bit: int  # bit offset inside the block (MSB-first), layer fields are relative to `base`
    width: int
    signed: bool = False
    morph: bool = False  # base value of a `4W+3` morph field
    note: str = ""


def _db(i: int, base: int) -> Param:
    return Param("organ", f"drawbar_{i + 1}", base + 40 + 19 * i, 4, morph=True, note="0..8")


def organ_params() -> list[Param]:
    out = [
        Param("organ", "enable_i", 9, 2),
        Param("organ", "enable_ii", 12, 2),
        Param("organ", "level_a", 14, 7, morph=True),
        Param("organ", "level_b", 45, 7, morph=True),
        Param("organ", "rotary_enabled", 80, 1),
        Param("organ", "leslie_speed", 81, 2, note="0|2 stop, 1 slow, 3 fast"),
        Param("organ", "rotary_drive", 92, 7),
        Param("organ", "vibrato_type", 130, 3, note="0..5 V1 C1 V2 C2 V3 C3"),
    ]
    for tag, base in (("a", 144), ("b", 392)):
        out += [
            Param("organ", f"octave_{tag}", base + 12, 4, signed=True),
            Param(
                "organ", f"model_{tag}", base + 17, 3, note="0 B3 1 Vox 2 Farfisa 3 Pipe1 4 Pipe2"
            ),
            Param("organ", f"vibrato_on_{tag}", base + 232, 1),
            Param("organ", f"perc_on_{tag}", base + 233, 1),
            Param("organ", f"perc_third_{tag}", base + 234, 1),
            Param("organ", f"perc_fast_{tag}", base + 235, 1),
            Param("organ", f"perc_soft_{tag}", base + 236, 1),
        ]
        out += [
            Param(p.block, f"{p.name}_{tag}", p.bit, p.width, morph=True, note=p.note)
            for p in (_db(i, base) for i in range(9))
        ]
    return out


def fx_params() -> list[Param]:
    f = "fx"
    return [
        Param(f, "mod1_on", 8, 1),
        Param(f, "mod1_rate", 10, 7, morph=True),
        Param(f, "mod1_amt", 41, 7, morph=True),
        Param(f, "mod1_type", 72, 4, note="0..11"),
        Param(f, "mod2_on", 76, 1),
        Param(f, "mod2_rate", 77, 7, morph=True),
        Param(f, "mod2_amt", 108, 7, morph=True),
        Param(f, "mod2_type", 139, 4, note="0..11"),
        Param(f, "amp_eq_on", 143, 1),
        Param(f, "eq_treble", 144, 7, signed=True, note="x0.25 dB"),
        Param(f, "eq_mid", 151, 7, signed=True, note="x0.25 dB"),
        Param(f, "eq_bass", 158, 7, signed=True, note="x0.25 dB"),
        Param(f, "eq_mid_freq", 165, 7, morph=True, note="200 Hz..8 kHz"),
        Param(f, "drive", 196, 7, morph=True),
        Param(f, "comp_on", 231, 1),
        Param(f, "comp_amt", 232, 7),
        Param(f, "delay_on", 240, 1),
        Param(f, "delay_rate", 242, 14, morph=True),
        Param(f, "delay_mix", 301, 7, morph=True),
        Param(f, "delay_fb", 336, 7, morph=True),
        Param(f, "reverb_on", 371, 1),
        Param(f, "reverb_amt", 372, 7, morph=True),
        Param(f, "reverb_type", 405, 3, note="0..5"),
    ]


def piano_params() -> list[Param]:
    out = [
        Param("piano", "level_a", 14, 7, morph=True),
        Param("piano", "level_b", 45, 7, morph=True),
    ]
    for tag, base in (("a", 104), ("b", 200)):
        out += [
            Param("piano", f"octave_{tag}", base + 12, 4, signed=True),
            Param("piano", f"category_{tag}", base + 18, 3, note="0..5"),
            Param("piano", f"model_id_{tag}", base + 21, 5),
            Param("piano", f"variation_{tag}", base + 60, 2),
            Param(
                "piano", f"acoustic_{tag}", base + 62, 2, note="bit1 string res, bit0 soft release"
            ),
            Param("piano", f"pedal_noise_{tag}", base + 64, 1),
            Param("piano", f"unison_{tag}", base + 65, 2),
            Param("piano", f"dyn_comp_{tag}", base + 67, 2),
            Param("piano", f"timbre_{tag}", base + 70, 3),
            Param("piano", f"kb_touch_{tag}", base + 73, 2),
        ]
    return out


def synth_params() -> list[Param]:
    out = []
    for n, tag in enumerate("abc"):
        lay, snd = 248 + 408 * n, 1472 + 384 * n
        s = "synth"
        out += [
            Param(s, f"level_{tag}", 18 + 31 * n, 7, morph=True),
            Param(s, f"pan_{tag}", 111 + 31 * n, 7),
            Param(s, f"octave_{tag}", lay + 62, 4, signed=True),
            Param(s, f"vibrato_mode_{tag}", lay + 72, 3),
            Param(s, f"glide_{tag}", lay + 81, 7),
            Param(s, f"unison_{tag}", lay + 306, 2),
            Param(s, f"osc_type_{tag}", snd + 10, 2),
            Param(s, f"osc_ctrl_{tag}", snd + 25, 7, morph=True),
            Param(s, f"pitch_fine_{tag}", snd + 56, 7, signed=True, note="cents"),
            Param(s, f"pitch_coarse_{tag}", snd + 63, 6, signed=True, note="semitones"),
            Param(s, f"osc_env_attack_{tag}", snd + 71, 7),
            Param(s, f"osc_env_decay_{tag}", snd + 78, 7),
            Param(s, f"osc_env_release_{tag}", snd + 85, 7),
            Param(s, f"lfo_rate_{tag}", snd + 133, 7, morph=True),
            Param(s, f"lfo_amount_{tag}", snd + 164, 7, morph=True),
            Param(s, f"amp_attack_{tag}", snd + 195, 7),
            Param(s, f"amp_decay_{tag}", snd + 202, 7),
            Param(s, f"amp_release_{tag}", snd + 209, 7),
            Param(s, f"filter_type_{tag}", snd + 218, 3, note="0 LP12 .. 5 HP"),
            Param(s, f"filter_freq_{tag}", snd + 221, 7, morph=True),
            Param(s, f"filter_res_{tag}", snd + 252, 7, morph=True),
            Param(s, f"filter_env_amt_{tag}", snd + 287, 7, morph=True),
            Param(s, f"filter_attack_{tag}", snd + 318, 7),
            Param(s, f"filter_decay_{tag}", snd + 325, 7),
            Param(s, f"filter_release_{tag}", snd + 332, 7),
        ]
    return out


# Where the 55-byte FX chains sit inside each block (byte offsets).
FX_CHAINS = {"organ": (80,), "piano": (37, 92), "synth": (328, 383, 438)}


def read_bits(data: bytes, bit: int, width: int) -> int:
    """`width` bits at MSB-first bit offset `bit`."""
    value = 0
    for k in range(width):
        pos = bit + k
        value = (value << 1) | ((data[pos >> 3] >> (7 - (pos & 7))) & 1)
    return value


def read_param(block: bytes, p: Param) -> int:
    v = read_bits(block, p.bit, p.width)
    if p.signed and v >= 1 << (p.width - 1):
        v -= 1 << p.width
    return v


def all_params(section: str) -> list[Param]:
    return {"organ": organ_params, "piano": piano_params, "synth": synth_params}[section]() + (
        fx_params() if section else []
    )


def split_blocks(tag: str, payload: bytes) -> dict[str, bytes]:
    """The `organ`, `piano` and `synth` blocks of a preset payload (CBIN header removed)."""
    if tag in ("ns4p", "ns4l"):
        return {
            k: payload[o : o + n] for k, (o, n) in PROGRAM_BLOCKS.items() if len(payload) >= o + n
        }
    if tag in STANDALONE:
        return {STANDALONE[tag]: payload}
    return {}


def iter_presets(path: Path) -> Iterator[tuple[str, bytes]]:
    """`(extension, payload)` of every preset in a `.ns4b` archive, a directory or a single file."""
    if path.suffix.lower() == ".ns4b":
        with zipfile.ZipFile(path) as zf:
            for info in zf.infolist():
                ext = info.filename.rsplit(".", 1)[-1].lower()
                if ext in ("ns4p", "ns4l", "ns4o", "ns4n", "ns4y") and info.file_size > CBIN_HEADER:
                    yield ext, zf.read(info)[CBIN_HEADER:]
        return
    files = sorted(path.rglob("*")) if path.is_dir() else [path]
    for f in files:
        ext = f.suffix.lstrip(".").lower()
        if ext in ("ns4p", "ns4l", "ns4o", "ns4n", "ns4y") and f.stat().st_size > CBIN_HEADER:
            yield ext, f.read_bytes()[CBIN_HEADER:]


@dataclass
class Stat:
    lo: int
    hi: int
    seen: set[int]

    def add(self, v: int) -> None:
        self.lo, self.hi = min(self.lo, v), max(self.hi, v)
        self.seen.add(v)


def observe(presets: Iterator[tuple[str, bytes]], section: str | None = None) -> dict[Param, Stat]:
    """Min, max and distinct values of every field of the requested section (default all)."""
    stats: dict[Param, Stat] = {}
    for tag, payload in presets:
        for sec, block in split_blocks(tag, payload).items():
            if section and sec != section:
                continue
            fields = {"organ": organ_params, "piano": piano_params, "synth": synth_params}[sec]()
            for off in FX_CHAINS[sec]:
                fields += [
                    Param(
                        sec,
                        f"fx{off}_{q.name}",
                        off * 8 + q.bit,
                        q.width,
                        q.signed,
                        q.morph,
                        q.note,
                    )
                    for q in fx_params()
                ]
            for p in fields:
                if (p.bit + p.width + 7) // 8 > len(block):
                    continue
                v = read_param(block, p)
                if p in stats:
                    stats[p].add(v)
                else:
                    stats[p] = Stat(v, v, {v})
    return stats


def format_row(p: Param, st: Stat | None) -> str:
    kind = ("s" if p.signed else "u") + str(p.width) + ("m" if p.morph else "")
    obs = f"{st.lo:>6}..{st.hi:<6} distinct {len(st.seen):>3}" if st else ""
    return f"{p.block:5s} {p.name:24s} bit {p.bit:5d} {kind:5s} {obs}  {p.note}".rstrip()


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    ap.add_argument("path", type=Path)
    ap.add_argument("--section", choices=("organ", "piano", "synth"))
    ap.add_argument("--list", action="store_true", help="print the field map only")
    args = ap.parse_args(argv)
    if args.list:
        for sec in (args.section,) if args.section else ("organ", "piano", "synth"):
            for p in all_params(sec):
                print(format_row(p, None))
        return 0
    stats = observe(iter_presets(args.path), args.section)
    for p, st in stats.items():
        print(format_row(p, st))
    print(f"{len(stats)} fields")
    return 0 if stats else 1


if __name__ == "__main__":
    sys.exit(main())
