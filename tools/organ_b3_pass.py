#!/usr/bin/env python3
"""Run the real slot program of the B3 model (model 0) in the SHARC oracle and read the partials.

Chain of evidence (`docs/research/b3-tone-source.md`):

1. `arm_slot_owners.py boot` shows that the default organ model B3 puts the generator `0x121BBE`
   into slot 1 and the processor `0x121E2B` into slot 4 (and 6); the "idle generator" of
   `organ-model-paths.md` is the layer-private slot whose generator is shared.
2. The ARM side is replayed by `keys`: model install, drawbar update of the organ record
   (`0xC00BE348`), key-down of the nine drawbar items and the two percussion items
   (`0xC00BF890`). Its PNO core 1 messages are the DM pokes.
3. `run` replays *every slot routine of the pass* (`DM(0xC01D6 + 4 slot)`, the pass loads
   `I8 = routine, I0 = DM(+1), I1 = DM(+2)`) in the oracle `sharc-chain` for N passes and captures
   the generator ring `DM(0xB32E8..)` and the processor output words. `peaks` reads the partials.

    uv run --with unicorn python tools/organ_b3_pass.py boot-messages CODE SRAM boot.jsonl
    uv run --with unicorn python tools/organ_b3_pass.py keys CODE SRAM keys.json --key 60
    python tools/organ_b3_pass.py run SRAM/core1_sram.bin boot.jsonl keys.json --exe sharc-chain

Output of `run`: for each ring word with a tone, its spectral peak (bin of `calls` samples), rms,
and the ratio to the lowest peak; the Hammond drawbar series is `0.5 1 1.5 2 3 4 5 6 8` times the
8' wheel, i.e. `1 2 3 4 6 8 10 12 16` times the 16' wheel.
"""

from __future__ import annotations

import argparse
import json
import struct
import subprocess
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path

import numpy as np

# VERIFIED (pno-module-chain.md s3, listing 0x1212E7..0x12113D): slot k = DM(0xC01D6 + 4 k) holds
# {routine, stream pointer (I0), record (I1), 0}; the pass makes the 8 calls in slot order.
SLOT_TABLE_DM = 0xC01D6
SLOT_COUNT = 8
# VERIFIED (ADI listing, `rts (db)` pc of each routine; `sharc-chain` stops at that pc, the
# delayed `rts` itself is not simulated): routine entry -> stop pc.
ROUTINE_STOP = {
    0x122A71: 0x122B84,
    0x121BBE: 0x121E28,
    0x121E2B: 0x122293,
    0x122296: 0x12261D,
    0x122B85: 0x122EF5,
    0x122620: 0x12268C,
    0x12268F: 0x12272A,
    0x12272D: 0x1229B2,
    0x1229B3: 0x122A70,
}
# Generator ring written by 0x121BBE (VERIFIED run: 142 words overwritten from 0xB32E0) and the
# processor output words of 0x121E2B (`pm(i11 - 0xE)` = 0xB46B6 - 0xE).
RING_BASE, RING_WORDS = 0xB32E8, 0x70
OUT_BASE, OUT_WORDS = 0xB46A0, 0x20
# Hammond drawbar footages relative to the 8' wheel: 16, 5 1/3, 8, 4, 2 2/3, 2, 1 3/5, 1 1/3, 1.
HAMMOND_SERIES = (0.5, 1.5, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 8.0)


@dataclass(frozen=True)
class Peak:
    address: int
    bin: int
    rms: float


def last_words(messages: list[dict], chip: str = "PNO", core: int = 1) -> dict[int, int]:
    """DM word -> last value of the path 2 messages of one chip core (`Message.to_dict` rows)."""
    out: dict[int, int] = {}
    for m in messages:
        if m["chip"] == chip and m["core"] == core and m["path"] == 2:
            out[int(m["dm"], 16)] = int(m["w2"], 16)
    return out


def slot_steps(words: dict[int, int], image: dict[int, int] | None = None) -> list[str]:
    """`--step ENTRY:STOP:I0=..,I1=..` strings of the non-idle slots in slot order."""
    image = image or {}

    def w(a: int) -> int:
        return words.get(a, image.get(a, 0))

    steps = []
    for k in range(SLOT_COUNT):
        base = SLOT_TABLE_DM + 4 * k
        routine = w(base)
        if routine in ROUTINE_STOP:
            steps.append(
                f"{routine:#x}:{ROUTINE_STOP[routine]:#x}:I0={w(base + 1):#x},I1={w(base + 2):#x}"
            )
    return steps


def script_text(words: dict[int, int]) -> str:
    """`sharc-chain --script` lines `0 ADDR VALUE` (all pokes before the first call)."""
    return "\n".join(f"0 {a:#x} {v:#x}" for a, v in sorted(words.items()))


def build_command(
    exe: str, image: str, script: str, steps: list[str], calls: int, out: str
) -> list[str]:
    cmd = [exe, image, "--calls", str(calls), "--script", script]
    for s in steps:
        cmd += ["--step", s]
    cmd += ["--range", f"{RING_BASE:#x}:{RING_WORDS}", "--range", f"{OUT_BASE:#x}:{OUT_WORDS}"]
    return [*cmd, "--out", out]


def peaks(series: np.ndarray, base: int, skip: int = 200, min_rms: float = 1e-4) -> list[Peak]:
    """Dominant FFT bin of every column with a tone (rows = calls, columns = consecutive words)."""
    out = []
    x = series[skip:].astype(np.float64)
    window = np.hanning(len(x))
    for c in range(x.shape[1]):
        col = x[:, c] - x[:, c].mean()
        if not np.isfinite(col).all() or col.std() < min_rms:
            continue
        spec = np.abs(np.fft.rfft(col * window))
        out.append(Peak(base + c, int(np.argmax(spec[1:])) + 1, float(col.std())))
    return out


def ratios(found: list[Peak]) -> list[float]:
    """Peak bins relative to the lowest one (sorted by bin)."""
    bins = sorted(p.bin for p in found)
    return [b / bins[0] for b in bins] if bins else []


def matches_series(found: list[Peak], series: tuple[float, ...], tol: float = 0.03) -> bool:
    """Do the peak bins, as ratios of the lowest, equal `series / min(series)` within `tol`?"""
    want = sorted(s / min(series) for s in series)
    got = ratios(found)
    return len(got) == len(want) and all(abs(g / w - 1) <= tol for g, w in zip(got, want))


def run(exe: str, image: str, words: dict[int, int], calls: int) -> np.ndarray:  # pragma: no cover
    """Run the oracle; returns an array `(calls, RING_WORDS + OUT_WORDS)` of float32 words."""
    with tempfile.TemporaryDirectory() as tmp:
        script, out = Path(tmp) / "s.txt", Path(tmp) / "o.bin"
        script.write_text(script_text(words), encoding="utf-8")
        steps = slot_steps(words, image_words(image))
        subprocess.run(build_command(exe, image, str(script), steps, calls, str(out)), check=True)
        return np.fromfile(out, dtype="<u4").reshape(calls, -1).view("<f4")


def image_words(image: str, lo: int = SLOT_TABLE_DM, n: int = 4 * SLOT_COUNT) -> dict[int, int]:
    """Slot table words of the flat SRAM image (DM word `a` = u32 at byte `4 a`)."""
    data = Path(image).read_bytes()
    return {a: struct.unpack_from("<I", data, 4 * a)[0] for a in range(lo, lo + n)}


# -----------------------------------------------------------------------------------------------
# ARM side (needs `unicorn`).
# -----------------------------------------------------------------------------------------------


def capture_boot(code: Path, sram: Path, windows: int = 10) -> list[dict]:  # pragma: no cover
    import arm_boot_full as bf
    import arm_steady as ast

    emu = bf.prepare(code, sram)
    ast.boot_to_main_loop(emu)
    emu.install_dsp_ports()
    ast.enable_real_timer(emu)
    rows: list[dict] = []
    for _ in range(windows):
        emu.clear_messages()
        ast.run_ticking(emu, 20_000_000)
        rows += [m.to_dict() for m in emu.messages]
    return rows


# VERIFIED (run): record table of the four organ sets, update and key-down of its sub-engines.
RECORD_TABLE = 0xC055F4B8
RECORD_UPDATE, ITEM_KEYDOWN = 0xC00BE348, 0xC00BF890
DRAWBAR_WORDS, DIRTY, BUSY = 0x40, 0x6C, 0x6D
SUB_DRAWBARS, SUB_PERCUSSION = 0x28, 0x2C


def capture_keys(  # pragma: no cover
    code: Path, sram: Path, cache: Path | None, key: int, drawbars: str, model: int = 0
) -> dict[str, list[dict]]:
    import arm_edit_capture as aec
    import arm_model_paths as mp
    import arm_steady as ast

    emu, _ = aec.settled_emulator(code, sram, cache)
    mp.ensure_scratch(emu)
    store = next(s for s in aec.first_store_per_layer(aec.find_stores(emu)) if s.kind == "organ")
    mp.install_model(emu, store, model)
    install = [m.to_dict() for m in emu.messages]
    rec = emu.read32(RECORD_TABLE)
    for k, v in enumerate(int(c) for c in drawbars):
        emu.write32(rec + DRAWBAR_WORDS + 4 * k, v)
    emu.uc.mem_write(rec + DIRTY, bytes([1]))
    emu.uc.mem_write(rec + BUSY, bytes([1]))
    emu.clear_messages()
    ast.call_preserving(emu, RECORD_UPDATE, rec, max_insns=20_000_000)
    out = {"install": install, "update": [m.to_dict() for m in emu.messages]}
    for name, off in (("drawbar_items", SUB_DRAWBARS), ("percussion_items", SUB_PERCUSSION)):
        emu.clear_messages()
        ast.call_preserving(
            emu, ITEM_KEYDOWN, emu.read32(rec + off), key, 100, 0, max_insns=20_000_000
        )
        out[name] = [m.to_dict() for m in emu.messages]
    return out


def main(argv: list[str] | None = None) -> int:  # pragma: no cover - thin CLI
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)
    b = sub.add_parser("boot-messages")
    b.add_argument("code", type=Path)
    b.add_argument("sram", type=Path)
    b.add_argument("out", type=Path)
    k = sub.add_parser("keys")
    k.add_argument("code", type=Path)
    k.add_argument("sram", type=Path)
    k.add_argument("out", type=Path)
    k.add_argument("--key", type=int, default=60)
    k.add_argument("--drawbars", default="888000000")
    k.add_argument("--cache", type=Path)
    r = sub.add_parser("run")
    r.add_argument("image")
    r.add_argument("boot", type=Path)
    r.add_argument("keys", type=Path)
    r.add_argument("--exe", default="sharc-chain")
    r.add_argument("--calls", type=int, default=4800)
    ns = ap.parse_args(argv)
    if ns.cmd == "boot-messages":
        rows = capture_boot(ns.code, ns.sram)
        ns.out.write_text("\n".join(json.dumps(x) for x in rows), encoding="utf-8")
    elif ns.cmd == "keys":
        data = capture_keys(ns.code, ns.sram, ns.cache, ns.key, ns.drawbars)
        ns.out.write_text(json.dumps(data), encoding="utf-8")
    else:
        rows = [json.loads(x) for x in ns.boot.read_text().splitlines() if x]
        keys = json.loads(ns.keys.read_text())
        for part in ("install", "update", "drawbar_items", "percussion_items"):
            rows += keys[part]
        words = last_words(rows)
        data = run(ns.exe, ns.image, words, ns.calls)
        ring = data[:, :RING_WORDS]
        found = peaks(ring, RING_BASE)
        for p in found:
            print(f"{p.address:#x} bin {p.bin} rms {p.rms:.4f}")
        print("ratios to the lowest peak:", [round(x, 2) for x in ratios(found)])
    return 0


if __name__ == "__main__":
    sys.exit(main())
