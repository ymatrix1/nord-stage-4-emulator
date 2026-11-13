#!/usr/bin/env python3
"""Complete PNO core 1 passes of the Farfisa (model 2) and B3 Bass (model 5) organ models.

The booted ARM firmware (`arm_steady.py` snapshot) supplies the messages the real organ gets:

* `ctor`    the engine constructor `0xC002187C` (the only caller of the 33/61/14 tap tables and the
            per-model words `DM 0x95471..0x954E5`; the settled harness never ran it, 15,289 words),
* `boot`    the first windows of the booted main loop (default model B3 on both layers),
* `model`   `Edt_Org_kModel = N` on organ layer 0 (`arm_model_paths.install_model`),
* keys      drawbar update of record 0 and the key-down of the drawbar and percussion items for each
            key of the chord (`0xC00BE348`, `0xC00BF890`, as `organ_b3_pass.py keys`).

`script` writes the messages as `0 ADDR VALUE` lines for `sharc-pass --script`; `run` runs the pass
and prints, per output lane, peak, rms and the strongest spectral lines in Hz at the DSP frame rate.

    uv run --with unicorn --with capstone --with numpy python tools/organ_fb_pass.py script \
        target/fw_unpacked/bank0_CODE.bin target/fw_unpacked/bank0_SRAM.bin fb_m5.script \
        --model 5 --keys 60,64,67 --cache target/settled.snap
    python tools/organ_fb_pass.py run SRAM/core1_sram.bin fb_m5.script \
        --exe target/release/sharc-pass

Method, results and what is still silent: `docs/research/organ-farfisa-bass.md`.
"""

from __future__ import annotations

import argparse
import os
import subprocess
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path

import numpy as np

# VERIFIED (docs/research/organ-farfisa-bass.md s2): the constructor that sends the filter tables.
CTOR = 0xC002187C
# ASSUMED anchor (organ-farfisa-bass.md s6): oscillator 0 of the Farfisa divider is C0 = 16.3516 Hz.
DSP_FRAME_RATE = 43_402.6
FRAMES_PER_PASS = 2
# VERIFIED (pno-full-pass.md s1): the audio is set 2, pair 0; one pass = 3 pairs x 4 words per set.
RAW_SETS, RAW_PAIRS = 6, 3


@dataclass(frozen=True)
class Line:
    """One strongest spectral line of a lane."""

    hz: float
    level_db: float


def script_lines(messages: list[dict]) -> list[str]:
    """`0 ADDR VALUE` lines of the PNO core 1 path 2 messages, in send order."""
    return [
        f"0 {int(m['dm'], 16):#x} {int(m['w2'], 16):#010x}"
        for m in messages
        if m["chip"] == "PNO" and m["core"] == 1 and m["path"] == 2
    ]


def lane_series(raw: np.ndarray, set_index: int, pair: int, lane: int) -> np.ndarray:
    """Samples of one lane from a `--raw` dump (`[pass, set, pair, word]`, words `L0 R0 L1 R1`).

    Lane 0 is the left channel (words 0 and 2, frames 0 and 1), lane 1 the right (words 1 and 3).
    """
    words = raw.reshape(-1, RAW_SETS, RAW_PAIRS, 4)[:, set_index, pair, :]
    return words[:, lane::2].reshape(-1)  # words 0, 2 (lane 0) or 1, 3 (lane 1)


def top_lines(x: np.ndarray, count: int = 6, rate: float = DSP_FRAME_RATE) -> list[Line]:
    """Strongest local spectral maxima (Hann window) of `x`, level in dB below the strongest."""
    x = x.astype(np.float64)
    spec = np.abs(np.fft.rfft((x - x.mean()) * np.hanning(len(x))))
    if spec.max() == 0:
        return []
    peaks = [i for i in range(1, len(spec) - 1) if spec[i] > spec[i - 1] and spec[i] >= spec[i + 1]]
    peaks.sort(key=lambda i: -spec[i])
    return [
        Line(i * rate / len(x), 20 * float(np.log10(spec[i] / spec.max()))) for i in peaks[:count]
    ]


def lane_report(raw: np.ndarray, skip: int = 400) -> list[str]:
    """One text line per non-silent lane of the pass dump (the first `skip` passes dropped)."""
    out = []
    data = raw.reshape(-1, RAW_SETS * RAW_PAIRS * 4)[skip:]
    for s in range(RAW_SETS):
        for p in range(RAW_PAIRS):
            for lane in (0, 1):
                x = lane_series(data, s, p, lane)
                if not np.isfinite(x).all() or np.abs(x).max() < 1e-6 or np.abs(x).max() > 10:
                    continue
                lines = top_lines(x)
                text = ", ".join(f"{ln.hz:.1f} Hz ({ln.level_db:.0f} dB)" for ln in lines[:4])
                out.append(
                    f"set {s} pair {p} lane {lane}: peak {np.abs(x).max():.5f} rms "
                    f"{x.std():.5f}; {text}"
                )
    return out


# -----------------------------------------------------------------------------------------------
# ARM side (needs `unicorn`).
# -----------------------------------------------------------------------------------------------


def capture_ctor(code: Path, sram: Path, cache: Path | None) -> list[dict]:  # pragma: no cover
    import arm_edit_capture as aec
    import arm_steady as ast

    emu, _ = aec.settled_emulator(code, sram, cache)
    emu.clear_messages()
    ast.call_preserving(emu, CTOR, max_insns=20_000_000)
    return [m.to_dict() for m in emu.messages]


def capture_chord(  # pragma: no cover
    code: Path, sram: Path, cache: Path | None, model: int, keys: list[int], drawbars: str
) -> list[dict]:
    """Model install, drawbar update and the key-downs of `keys`, in one emulator session."""
    import arm_edit_capture as aec
    import arm_model_paths as mp
    import arm_steady as ast
    import organ_b3_pass as ob

    emu, _ = aec.settled_emulator(code, sram, cache)
    mp.ensure_scratch(emu)
    store = next(s for s in aec.first_store_per_layer(aec.find_stores(emu)) if s.kind == "organ")
    mp.install_model(emu, store, model)
    rows = [m.to_dict() for m in emu.messages]
    rec = emu.read32(ob.RECORD_TABLE)
    for k, v in enumerate(int(c) for c in drawbars):
        emu.write32(rec + ob.DRAWBAR_WORDS + 4 * k, v)
    emu.uc.mem_write(rec + ob.DIRTY, bytes([1]))
    emu.uc.mem_write(rec + ob.BUSY, bytes([1]))
    emu.clear_messages()
    ast.call_preserving(emu, ob.RECORD_UPDATE, rec, max_insns=20_000_000)
    rows += [m.to_dict() for m in emu.messages]
    for key in keys:
        for off in (ob.SUB_DRAWBARS, ob.SUB_PERCUSSION):
            emu.clear_messages()
            ast.call_preserving(
                emu, ob.ITEM_KEYDOWN, emu.read32(rec + off), key, 100, 0, max_insns=20_000_000
            )
            rows += [m.to_dict() for m in emu.messages]
    return rows


def cmd_script(a: argparse.Namespace) -> int:  # pragma: no cover - needs the ARM harness
    import organ_b3_pass as ob

    keys = [int(k) for k in a.keys.split(",")]
    rows = capture_ctor(a.code, a.sram, a.cache) + ob.capture_boot(a.code, a.sram)
    rows += capture_chord(a.code, a.sram, a.cache, a.model, keys, a.drawbars)
    a.out.write_text("\n".join(script_lines(rows)) + "\n", encoding="utf-8")
    print(f"{len(script_lines(rows))} words -> {a.out}")
    return 0


def cmd_run(a: argparse.Namespace) -> int:  # pragma: no cover - runs the oracle
    with tempfile.TemporaryDirectory() as tmp:
        raw = Path(tmp) / "raw.bin"
        cmd = [
            os.path.abspath(a.exe),
            a.image,
            "--script",
            str(a.script),
            "--passes",
            str(a.passes),
            "--quiet",
        ]
        subprocess.run([*cmd, "--raw", str(raw)], check=True, capture_output=True)
        data = np.fromfile(raw, dtype="<f4")
    for line in lane_report(data) or ["all lanes silent"]:
        print(line)
    return 0


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)
    s = sub.add_parser("script")
    s.add_argument("code", type=Path)
    s.add_argument("sram", type=Path)
    s.add_argument("out", type=Path)
    s.add_argument("--model", type=int, default=5)
    s.add_argument("--keys", default="60,64,67")
    s.add_argument("--drawbars", default="888000000")
    s.add_argument("--cache", type=Path)
    r = sub.add_parser("run")
    r.add_argument("image")
    r.add_argument("script", type=Path)
    r.add_argument("--exe", default="sharc-pass")
    r.add_argument("--passes", type=int, default=6000)
    ns = ap.parse_args(argv)
    return cmd_script(ns) if ns.cmd == "script" else cmd_run(ns)


if __name__ == "__main__":
    sys.exit(main())
