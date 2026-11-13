#!/usr/bin/env python3
"""Amp type words of the firmware: what `Edt_Effects_kAmpTypeNew` writes into the drive stage.

`fx_clients.py --slots amp.csv --slot Amp --units 3 --types-only` applies the 10 values of
`kAmpTypeNew` (0..9, the amp mode of the program: off, Twin, Twin II, JC, JC II, Small, Small II,
To Rotary, LP24, HP24) to the organ unit L3 of the settled firmware and records every DSP word
the type switch sends. This tool

* `extract`: keeps the words that fall into the context of the drive stage `0x1228CF` (module
  `0xC04438D8#0`, DM `0xC016A..0xC019F` of FX core 1, `I1 = 0xC018A`) as
  `docs/research/data/fx-amp-types.csv` (`type,ctx,dm_word,word`), and counts the cabinet words
  (FIR taps and biquad bank of `0xC0442A94`) per type;
* `golden`: runs the drive stage in the `sharc-fx` oracle with the real context (PNO image
  template words + type words + `kAmpDrive`) and the real tables on a seeded signal and prints a
  Rust test-vector file (`tests_amp_golden.rs`) for `fw_amp_stage` (needs the FX core 2 and the
  PNO core 1 flat SRAM images of `tools/ns4_dsp_images.py` + `ns4_dsp_ldr.py --sram`).

    uv run --with unicorn --with capstone python tools/fx_clients.py CODE.bin SRAM.bin \\
        --cache target/settled.snap --slots amp.csv --slot Amp --units 3 --types-only
    python tools/fx_amp_types.py extract amp.csv --out docs/research/data/fx-amp-types.csv
    python tools/fx_amp_types.py golden fx/core2_sram.bin pno/core1_sram.bin \
        > crates/nord-core/src/fx/tests_amp_golden.rs

Evidence and limits: `docs/research/fx-type-map.md` s8.
"""

from __future__ import annotations

import argparse
import csv
import math
import struct
import sys
from pathlib import Path

#: `I1` of the drive stage of the organ unit L3 on FX core 1 (VERIFIED: the sections of the type
#: switch land at `I1 + 7` and `I1 + 0x10`; `kAmpDrive` lands at `I1 - 0x20`).
CTX_BASE = 0xC018A
CTX_FIRST, CTX_LAST = -0x20, 0x15
#: Cabinet FIR taps of the type switch (`0xC0442A94#1`, 130 words) and the biquad bank (`#0`).
FIR_FIRST, FIR_LAST = 0x969C0, 0x96A41
TYPE_NAMES = {
    0: "Off",
    1: "Twin",
    2: "Twin II",
    3: "JC",
    4: "JC II",
    5: "Small",
    6: "Small II",
    7: "To Rotary",
    8: "LP24",
    9: "HP24",
}
FIELDS = ["type", "ctx", "dm_word", "word"]


def f32(word: int) -> float:
    """The float a 32-bit word holds."""
    return struct.unpack("<f", struct.pack("<I", word & 0xFFFFFFFF))[0]


def bits(value: float) -> int:
    """The 32-bit word of a float."""
    return struct.unpack("<I", struct.pack("<f", value))[0]


def ctx_offset(dm_word: int) -> int | None:
    """Context offset (`dm - I1`) of a DM word, or `None` outside the stage's 54 words."""
    off = dm_word - CTX_BASE
    return off if CTX_FIRST <= off <= CTX_LAST else None


def extract(rows: list[dict[str, str]]) -> tuple[list[dict[str, int]], dict[int, dict[str, int]]]:
    """Drive-context words per type from a `fx_clients.py --slots` capture (core 1 only).

    Returns `(ctx_rows, cabinet)`; `cabinet[type]` counts the non-zero FIR taps and all words
    sent for the cabinet (`0x969C0..0x96A41`, the other cabinet words are not classified).
    """
    out: list[dict[str, int]] = []
    cabinet: dict[int, dict[str, int]] = {}
    seen: set[tuple[int, int]] = set()
    for r in rows:
        if r["slot"] != "Amp" or r["stage"] != "type" or r["core"] != "1":
            continue
        t, dm, word = int(r["type"]), int(r["dm_word"], 16), int(r["word"], 16)
        off = ctx_offset(dm)
        if off is not None and (t, off) not in seen:
            seen.add((t, off))
            out.append({"type": t, "ctx": off, "dm_word": dm, "word": word})
        if FIR_FIRST <= dm <= FIR_LAST:
            c = cabinet.setdefault(t, {"fir_taps": 0, "fir_words": 0})
            c["fir_words"] += 1
            c["fir_taps"] += int(word != 0)
    out.sort(key=lambda x: (x["type"], x["ctx"]))
    return out, cabinet


def write_csv(path: Path, rows: list[dict[str, int]]) -> None:
    with path.open("w", encoding="utf-8", newline="") as fh:
        w = csv.writer(fh, lineterminator="\n")
        w.writerow(FIELDS)
        for r in rows:
            w.writerow([r["type"], r["ctx"], f"{r['dm_word']:#x}", f"{r['word']:#010x}"])


def read_csv(path: Path) -> dict[int, dict[int, int]]:
    """`{type: {ctx offset: word}}` of the committed data file."""
    out: dict[int, dict[int, int]] = {}
    with path.open(encoding="utf-8") as fh:
        for r in csv.DictReader(fh):
            out.setdefault(int(r["type"]), {})[int(r["ctx"])] = int(r["word"], 16)
    return out


def section(words: dict[int, int], first: int) -> list[float]:
    """The five floats `b0 b1 b2 a1 a2` of a biquad section at ctx offset `first`."""
    return [f32(words[first + k]) for k in range(5)]


# ---------------------------------------------------------------------------------------------
# oracle golden vectors (needs the FX core 2 and PNO core 1 images and the sharc-fx binary)
# ---------------------------------------------------------------------------------------------

IN_ADDR = 0xB1000
CALLS = 24
WORDS_PER_CALL = 8


def test_signal(calls: int = CALLS, amp: float = 0.2) -> list[float]:
    """Deterministic input: 180 Hz and 2.6 kHz at 48 kHz, `WORDS_PER_CALL` words per call."""
    n = calls * WORDS_PER_CALL // 2
    out = []
    for k in range(n):
        s = amp * (
            0.7 * math.sin(2 * math.pi * 180 * k / 48000)
            + 0.3 * math.sin(2 * math.pi * 2600 * k / 48000)
        )
        out += [s, 0.8 * s]
    return out


TAB_A, TAB_B, TAB_C0 = 0xE0802, 0xE0883, 0xE0000
TABLE_SPAN = (0xE0000, 0xE0960)


def static_image(image: str, binary: Path) -> dict[int, int]:
    """DM words `0xE0000..0xE095F` and the 54 context words of the PNO image (FX core 1 copy).

    The PNO DSP image (`dsp_image_1_PNO.bin`, core 1) holds the context template of the organ
    unit's drive stage at `I1 = 0xC018A` and the tables; the boot of `sharc-fx` is enough.
    """
    import subprocess

    lo, hi = TABLE_SPAN
    args = [str(binary), image, "--call", "0x1228CF", "--until", "0x1228d2", "--max", "5"]
    args += ["--dump", f"{lo:#x}:{hi - lo}", "--dump", f"{CTX_BASE + CTX_FIRST:#x}:54"]
    run = subprocess.run(args, capture_output=True, text=True, check=True)
    out = {}
    for line in run.stdout.splitlines():
        if line.startswith("dump "):
            _, a, w, *_ = line.split()
            out[int(a, 16)] = int(w, 16)
    return out


def golden_cases(
    image: str, static: dict[int, int], binary: Path, words: dict[int, int], drive: float
) -> tuple[list[int], list[int]]:
    """`(frames_in, frames_out)` words of the drive stage on the real context.

    The routine runs in the FX core 2 image (`image`, pc `0x1228CF`, the copy that the models of
    `fx_model_lfo.py` are verified on) with the context and tables of `static` placed at the DM
    addresses of the PNO core 1 template.
    """
    import fx_biquad_check as fb
    import fx_oracle as fo

    ctx = dict(static)
    for off, w in words.items():
        ctx[CTX_BASE + off] = w
    ctx[CTX_BASE - 0x20] = bits(drive / 127.0)
    ctx[CTX_BASE - 0x1F] = bits(drive / 127.0)  # settled smoother state (static setting)
    sig = [bits(x) for x in test_signal()]
    sets = {"I0": IN_ADDR, "I1": CTX_BASE, "M0": 0, "M2": 2, "M9": 1}
    rt = fo.Routine("amp", 0x1228CF, 0x122A1C, sets, lambda *a: None, lambda r: {}, ctx)
    cases = [
        {IN_ADDR + k: sig[c * WORDS_PER_CALL + k] for k in range(WORDS_PER_CALL)}
        for c in range(CALLS)
    ]
    dumps = {IN_ADDR + k for k in range(WORDS_PER_CALL)}
    res = fb.parse_dumps(fo.run_oracle(image, binary, rt, cases, dumps))
    out = [res[c][IN_ADDR + k] for c in range(CALLS) for k in range(WORDS_PER_CALL)]
    return sig, out


def rust_array(name: str, words: list[int], doc: str) -> str:
    lines = [f"/// {doc}", f"pub const {name}: [u32; {len(words)}] = ["]
    for i in range(0, len(words), 8):
        lines.append("    " + ", ".join(f"{w:#010x}" for w in words[i : i + 8]) + ",")
    lines.append("];")
    return "\n".join(lines)


def rust_golden(fx2: str, pno1: str, binary: Path, data: Path) -> str:
    """The Rust golden file: two real contexts (Twin at the Vox drive, JC at a high drive)."""
    types = read_csv(data)
    static = static_image(pno1, binary)
    parts = [
        "//! Golden vectors of the Amp drive stage `0x1228CF` with the **real** context words of the",  # noqa: E501
        "//! organ unit's drive stage (PNO image core 1 template at `I1 = 0xC018A`, the words of",
        "//! `kAmpTypeNew` and `kAmpDrive`) and the real tables, recorded from the `sharc-fx` oracle by",  # noqa: E501
        "//! `python tools/fx_amp_types.py golden FX_core2_sram.bin PNO_core1_sram.bin`. Derived values",  # noqa: E501
        f"//! only: the seeded test signal ({CALLS} calls of {WORDS_PER_CALL} words) and the oracle output.",  # noqa: E501
        "",
    ]
    for tag, t, drive in (("TWIN", 1, 14.0), ("JC", 3, 100.0)):
        fi, fo_ = golden_cases(fx2, static, binary, types[t], drive)
        parts += [
            rust_array(
                f"{tag}_FRAMES_IN", fi, f"{TYPE_NAMES[t]} (type {t}), kAmpDrive {drive:g}: input."
            ),
            rust_array(f"{tag}_FRAMES_OUT", fo_, "Oracle output words of every call."),
            "",
        ]
    parts += [
        "#[test]",
        "fn golden_vectors_have_the_documented_shapes() {",
        f"    assert_eq!(TWIN_FRAMES_IN.len(), {CALLS * WORDS_PER_CALL});",
        "    assert_eq!(TWIN_FRAMES_OUT.len(), TWIN_FRAMES_IN.len());",
        "    assert_eq!(JC_FRAMES_OUT.len(), JC_FRAMES_IN.len());",
        "    assert!(TWIN_FRAMES_OUT != JC_FRAMES_OUT);",
        "}",
    ]
    return "\n".join(parts).rstrip() + "\n"


def main(argv: list[str] | None = None) -> int:  # pragma: no cover - thin CLI
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)
    ex = sub.add_parser("extract")
    ex.add_argument("capture", type=Path)
    ex.add_argument("--out", type=Path, required=True)
    go = sub.add_parser("golden")
    go.add_argument("fx_core2", help="FX core 2 flat SRAM image (the oracle copy of the routine)")
    go.add_argument("pno_core1", help="PNO core 1 flat SRAM image (context template and tables)")
    go.add_argument("--data", type=Path, default=Path("docs/research/data/fx-amp-types.csv"))
    go.add_argument("--binary", type=Path, default=Path("target/release/sharc-fx.exe"))
    ns = ap.parse_args(argv)
    if ns.cmd == "extract":
        with ns.capture.open(encoding="utf-8") as fh:
            rows, cabinet = extract(list(csv.DictReader(fh)))
        write_csv(ns.out, rows)
        for t, c in sorted(cabinet.items()):
            print(f"type {t} ({TYPE_NAMES[t]}): {c['fir_taps']} of {c['fir_words']} FIR words set")
        return 0
    sys.stdout.write(rust_golden(ns.fx_core2, ns.pno_core1, ns.binary, ns.data))
    return 0


if __name__ == "__main__":
    sys.exit(main())
