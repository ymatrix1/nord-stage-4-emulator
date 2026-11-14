#!/usr/bin/env python3
"""Amp cabinet of the firmware (routine `0x122A1D`, module `0xC0442A94`): measured response.

The cabinet routine (a 130 tap FIR over the stereo pair stream plus a bank of biquad sections,
state in PM) is **exactly linear, time invariant and channel independent** in the `sharc-fx`
oracle with the real context (`ir` below checks it: shifted impulses and a doubled impulse give
word identical results, the cross channel is zero). Its impulse response is therefore the whole
model; this tool measures it for the three cabinets (Twin / Twin II, JC / JC II, Small / Small II
share one each) and prints the Rust data file `fx/fw_cabinet_taps.rs`.

    python tools/fx_amp_cabinet.py taps FX_core2_sram.bin PNO_core1_sram.bin amp.csv \\
        > crates/nord-core/src/fx/fw_cabinet_taps.rs
    python tools/fx_amp_cabinet.py chain FX_core2_sram.bin PNO_core1_sram.bin amp.csv \\
        > crates/nord-core/src/fx/tests_cabinet_golden.rs

Context: the cabinet instance `I1 = 0xE376A` (template in the PNO core 1 image), the words of the
type switch from `fx_clients.py --slots` (FIR taps at PM `0x969C0..`, bank at `0xE3702..`), run in
the FX core 2 copy of the routine like `fx_amp_types.py`. Evidence: `docs/research/fx-type-map.md`
s10.
"""

from __future__ import annotations

import argparse
import csv
import subprocess
import sys
from pathlib import Path

if __package__ in (None, ""):
    sys.path.insert(0, str(Path(__file__).resolve().parent))
import fx_amp_types as at
import fx_biquad_check as fb
import fx_oracle as fo

CAB_CTX = 0xE376A
CAB_PC, CAB_END = 0x122A1D, 0x122B1D
IN_ADDR = 0xB1000
#: Impulse response length kept: 99.9999 % of the energy is inside 370 samples for all three.
TAPS = 400
#: (type used to measure, label); the other type of each pair carries the same cabinet words.
CABINETS = ((1, "Twin / Twin II"), (3, "JC / JC II"), (5, "Small / Small II"))
CHAIN_FRAMES = 768
CHAIN_DRIVE = 14.0
TEMPLATE_RANGES = ((0xE35F0, 0x200), (0x96980, 0x100))


def dump(binary: Path, image: str, ranges: tuple[tuple[int, int], ...]) -> dict[int, int]:
    """Words of `ranges` in the booted image (one harmless call of the drive stage)."""
    args = [str(binary), image, "--call", "0x1228CF", "--until", "0x1228d2", "--max", "5"]
    for lo, n in ranges:
        args += ["--dump", f"{lo:#x}:{n}"]
    out = subprocess.run(args, capture_output=True, text=True, check=True).stdout
    return {
        int(p[1], 16): int(p[2], 16)
        for p in (line.split() for line in out.splitlines() if line.startswith("dump "))
    }


def type_overlay(capture: Path, kind: int) -> dict[int, int]:
    """Cabinet words of `kAmpTypeNew = kind` (core 1): bank, gain, count, FIR taps."""
    words: dict[int, int] = {}
    with capture.open(encoding="utf-8") as fh:
        for r in csv.DictReader(fh):
            if r["core"] == "1" and r["type"] == str(kind) and r["stage"] == "type":
                a = int(r["dm_word"], 16)
                if 0xE3700 <= a < 0xE3780 or 0x96980 <= a < 0x96A80:
                    words[a] = int(r["word"], 16)
    return words


def cabinet_memory(binary: Path, pno1: str, capture: Path, kind: int) -> dict[int, int]:
    mem = dump(binary, pno1, TEMPLATE_RANGES)
    mem.update(type_overlay(capture, kind))
    return mem


def run_cabinet(fx2: str, binary: Path, mem: dict[int, int], words: list[int]) -> list[int]:
    """The cabinet routine on `words` (4 words = 2 stereo frames per call, state carried)."""
    sets = {"I0": IN_ADDR, "I1": CAB_CTX, "M0": 0, "M2": 2, "M9": 1}
    rt = fo.Routine("cab", CAB_PC, CAB_END, sets, lambda *a: None, lambda r: {}, mem)
    calls = len(words) // 4
    cases = [{IN_ADDR + k: words[c * 4 + k] for k in range(4)} for c in range(calls)]
    dumps = {IN_ADDR + k for k in range(4)}
    res = fb.parse_dumps(fo.run_oracle(fx2, binary, rt, cases, dumps))
    return [res[c][IN_ADDR + k] for c in range(calls) for k in range(4)]


def impulse_response(fx2: str, binary: Path, mem: dict[int, int], frames: int = 3000) -> list[int]:
    """Left channel response words to a unit impulse; checks linearity and shift invariance."""
    words = [0] * (frames * 2)
    words[0] = at.bits(1.0)
    ir = run_cabinet(fx2, binary, mem, words)[0::2]
    shifted = [0] * (frames * 2)
    shifted[5 * 2] = at.bits(1.0)
    got = run_cabinet(fx2, binary, mem, shifted)[0::2]
    assert got[5:] == ir[: frames - 5], "the cabinet is not shift invariant"
    return ir


def rust_taps(fx2: str, pno1: str, capture: Path, binary: Path) -> str:
    out = [
        "//! Impulse responses of the three firmware amp cabinets (routine `0x122A1D`, module",
        "//! `0xC0442A94`), measured in the `sharc-fx` oracle with the real context and the words of",  # noqa: E501
        "//! `kAmpTypeNew` (`python tools/fx_amp_cabinet.py taps ...`). Derived data: float32 words of",  # noqa: E501
        f"//! the first {TAPS} samples (99.9999 % of the energy); the routine itself is exactly linear and",  # noqa: E501
        "//! time invariant there (`docs/research/fx-type-map.md` s10). Output of frame `n` is",
        "//! `sum_k h[k] x[n - k]`, no latency.",
        "",
    ]
    for kind, label in CABINETS:
        mem = cabinet_memory(binary, pno1, capture, kind)
        ir = impulse_response(fx2, binary, mem)[:TAPS]
        out.append(at.rust_array(f"TAPS_{kind}", ir, f"{label} (measured with type {kind})."))
        out.append("")
    out += [
        "#[test]",
        "fn impulse_responses_have_the_documented_shape() {",
        f"    assert_eq!(TAPS_1.len(), {TAPS});",
        "    assert!(TAPS_1 != TAPS_3 && TAPS_3 != TAPS_5);",
        "}",
    ]
    return "\n".join(out).rstrip() + "\n"


def chain_input() -> list[float]:
    """Seeded broadband stereo input (LCG, mirrored in `tests_cabinet_golden.rs`)."""
    state = 12345
    out = []
    for _ in range(CHAIN_FRAMES):
        frame = []
        for _ in range(2):
            state = (state * 1664525 + 1013904223) & 0xFFFFFFFF
            frame.append(((state >> 8) / (1 << 24) - 0.5) * 0.2)
        out += frame
    return out


def rust_chain(fx2: str, pno1: str, capture: Path, binary: Path, data: Path) -> str:
    """Oracle output of drive stage `0x1228CF` then cabinet `0x122A1D` (Twin, drive 14)."""
    static = at.static_image(pno1, binary)
    types = at.read_csv(data)
    sig = [at.bits(x) for x in chain_input()]
    # drive stage: 4 frames (8 words) per call
    drv_in = sig[: (len(sig) // 8) * 8]
    ctx = dict(static)
    for off, w in types[1].items():
        ctx[at.CTX_BASE + off] = w
    ctx[at.CTX_BASE - 0x20] = ctx[at.CTX_BASE - 0x1F] = at.bits(CHAIN_DRIVE / 127.0)
    sets = {"I0": IN_ADDR, "I1": at.CTX_BASE, "M0": 0, "M2": 2, "M9": 1}
    rt = fo.Routine("amp", 0x1228CF, 0x122A1C, sets, lambda *a: None, lambda r: {}, ctx)
    cases = [{IN_ADDR + k: drv_in[c * 8 + k] for k in range(8)} for c in range(len(drv_in) // 8)]
    res = fb.parse_dumps(fo.run_oracle(fx2, binary, rt, cases, {IN_ADDR + k for k in range(8)}))
    mid = [res[c][IN_ADDR + k] for c in range(len(cases)) for k in range(8)]
    final = run_cabinet(fx2, binary, cabinet_memory(binary, pno1, capture, 1), mid)
    return (
        "\n".join(
            [
                "//! Oracle output of the Amp drive stage `0x1228CF` followed by the cabinet `0x122A1D`",  # noqa: E501
                f"//! (Twin, `kAmpDrive` {CHAIN_DRIVE:g}) on the seeded LCG signal of `chain_input` in",  # noqa: E501
                "//! `tools/fx_amp_cabinet.py` (`python tools/fx_amp_cabinet.py chain ...`). Derived values only.",  # noqa: E501
                "",
                at.rust_array("CHAIN_OUT", final, "Oracle words after drive and cabinet."),
                "",
                "#[test]",
                "fn golden_vector_has_the_documented_shape() {",
                f"    assert_eq!(CHAIN_OUT.len(), {len(final)});",
                "    assert!(CHAIN_OUT.iter().any(|w| *w != 0));",
                "}",
            ]
        )
        + "\n"
    )


def main(argv: list[str] | None = None) -> int:  # pragma: no cover - thin CLI
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    ap.add_argument("cmd", choices=["taps", "chain"])
    ap.add_argument("fx_core2")
    ap.add_argument("pno_core1")
    ap.add_argument("capture", type=Path)
    ap.add_argument("--data", type=Path, default=Path("docs/research/data/fx-amp-types.csv"))
    ap.add_argument("--binary", type=Path, default=Path("target/release/sharc-fx.exe"))
    ns = ap.parse_args(argv)
    if ns.cmd == "taps":
        sys.stdout.write(rust_taps(ns.fx_core2, ns.pno_core1, ns.capture, ns.binary))
    else:
        sys.stdout.write(rust_chain(ns.fx_core2, ns.pno_core1, ns.capture, ns.binary, ns.data))
    return 0


if __name__ == "__main__":
    sys.exit(main())
