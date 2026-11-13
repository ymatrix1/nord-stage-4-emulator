#!/usr/bin/env python3
"""Delay time law of the Nord Stage 4 FX delay: edit id value -> DSP context word -> echo delay.

Two halves, each checked against the firmware (`docs/research/fx-delay-time.md`):

* **ARM half** (`kDlyRateClkDiv`, 14 bit value `v`, `0..=16383`): the handler `0xC010C514` searches
  a descending table of 1501 `i16` thresholds (ARM `0xC02B8C44`) for the bracket of `v`, takes the
  fractional index `k` (linear between the two entries, in `f64`), and the delay unit turns it into
  the context word `c[0x15] = min(61438, floor(K * k))` (a float holding an integer, delay samples
  at 48 kHz). `v >= 16256` gives `k = 20`, `v <= 0` gives `k = 1500`.
* **DSP half** (routine `0x120AB5`): `c[0x15]` and `c[0x16]` are two read taps of one ring buffer,
  the echo of an input sits `trunc(c) + 2` samples later, and `c[0x17]` is a cross-fade weight
  that moves by `trunc(c[0x19])` per call (2 samples) between the two taps. The ARM writes
  `c[0x16] = old time`, `c[0x19] = -32768` (weight snaps to the old tap), `c[0x15] = new time`,
  `c[0x19] = 30.199` (weight ramps to the new tap in about 45 ms).

    python tools/fx_delay_time.py check CODE.bin docs/research/data/fx-delay-time.csv
    python tools/fx_delay_time.py rust CODE.bin           # the table as Rust (fw_delay_table.rs)
    python tools/fx_delay_time.py oracle core2_sram.bin   # DSP half in the sharc-fx oracle

`CODE.bin` is the unpacked ARM image (`target/fw_unpacked/bank0_CODE.bin`, never committed). The
committed table (`crates/nord-core/src/fx/fw_delay_table.rs`) is a derived value list; `check`
compares it with the local firmware through [`table_fingerprint`].
"""

from __future__ import annotations

import argparse
import csv
import hashlib
import math
import re
import struct
import sys
from pathlib import Path

ARM_BASE = 0xC0000000
# VERIFIED: literal `0xC010C968` of the handler `0xC010C514` holds this address (`check` below).
TABLE_ADDR = 0xC02B8C44
TABLE_LEN = 1501
# VERIFIED: handler branches `0xC010C914` (value >= 0x3F80 = 16256) and `0xC010C920` (value <= 0)
# load the double literals 20.0 and 1500.0.
V_TOP, K_TOP, K_BOTTOM = 16256, 20.0, 1500.0
# VERIFIED: interval of `K` for which `floor(K * k)` reproduces every captured word (253 points).
SAMPLES_PER_INDEX = 43.402339
MAX_SAMPLES = 61438  # VERIFIED: clamp seen for every v <= 170 (`fx-delay-time.csv`)
ECHO_LATENCY = 2  # VERIFIED in the oracle: first echo at `trunc(c15) + 2` samples
SAMPLE_RATE = 48_000.0
# VERIFIED: the table fingerprint of OS v1.66 (`sha256` of the little endian `i16` list).
TABLE_FINGERPRINT = "797dcb4b2e2cb14ec4e1eb240ee795c7450e1cc77a3022657dd41fc57081d912"


def table_fingerprint(table: list[int]) -> str:
    """Hex SHA-256 of the table as packed little endian `i16`."""
    return hashlib.sha256(struct.pack(f"<{len(table)}h", *table)).hexdigest()


def load_table(code: bytes, addr: int = TABLE_ADDR) -> list[int]:
    """The `i16` table from the unpacked ARM image (`bank0_CODE.bin`)."""
    return list(struct.unpack_from(f"<{TABLE_LEN}h", code, addr - ARM_BASE))


def fractional_index(table: list[int], v: int) -> float:
    """Index `k` the handler computes for the UI value `v` (`0xC010C514`, `f64` arithmetic).

    Coarse search down in steps of 46 entries (while `v > table[i - 46]`), then entry by entry
    until `v <= table[i]`; `k = i + (table[i] - v) / (table[i] - table[i + 1])`.
    """
    if v >= V_TOP:
        return K_TOP
    if v <= 0:
        return K_BOTTOM
    i = TABLE_LEN - 1
    while i != 28 and v > table[i - 46]:
        i -= 46
        if i == 28:
            break
    while v > table[i]:
        i -= 1
    if v == table[i]:
        return float(i)
    return i + (table[i] - v) / (table[i] - table[i + 1])


def delay_samples(table: list[int], v: int) -> int:
    """The word `c[0x15]` (delay samples at 48 kHz) the ARM writes for `kDlyRateClkDiv = v`."""
    return min(MAX_SAMPLES, math.floor(SAMPLES_PER_INDEX * fractional_index(table, v)))


def echo_delay(c15: float) -> int:
    """Samples from an input to its first echo for the tap word `c15` (steady cross-fade)."""
    return int(c15) + ECHO_LATENCY


def crossfade_weight(c17: int, c19: float, calls: int) -> int:
    """`c[0x17]` after `calls` calls: `clamp(c17 + trunc(c19), 0, 32768)` each call."""
    step = int(c19)
    for _ in range(calls):
        c17 = min(32768, max(0, c17 + step))
    return c17


def rust_table(table: list[int]) -> str:
    """The table as the body of `fw_delay_table.rs` (16 values per line)."""
    lines = []
    for i in range(0, len(table), 16):
        lines.append("    " + ", ".join(str(x) for x in table[i : i + 16]) + ",")
    return "\n".join(lines)


def parse_rust_table(source: str) -> list[int]:
    """The values of the committed `FW_DELAY_TABLE` back as a list."""
    body = source.split("FW_DELAY_TABLE", 1)[1].split("= [", 1)[1].split("];", 1)[0]
    return [int(x) for x in re.findall(r"-?\d+", body)]


def read_series(path: Path) -> dict[int, float]:
    """`{value: c15}` of `fx-delay-time.csv` (`id,value,ctx,word`, float32 words)."""
    out: dict[int, float] = {}
    with path.open(encoding="utf-8", newline="") as fh:
        for r in csv.DictReader(fh):
            out[int(r["value"])] = struct.unpack("<f", struct.pack("<I", int(r["word"])))[0]
    return out


def check_series(table: list[int], series: dict[int, float]) -> list[tuple[int, float, int]]:
    """Points where the closed form differs: `(v, captured, predicted)`."""
    return [(v, w, delay_samples(table, v)) for v, w in sorted(series.items())
            if delay_samples(table, v) != w]  # fmt: skip


# --- DSP half: sharc-fx oracle -------------------------------------------------------------

IN_ADDR, CTX, OUT_ADDR, RING, TABLE = 0xB1000, 0xB2040, 0xB3000, 0xB4000, 0xB6200
ROUTINE_PC, ROUTINE_END = 0x120AB5, 0x120D11  # variant with its own ring buffer (rts)


def f32_bits(x: float) -> int:
    return struct.unpack("<I", struct.pack("<f", x))[0]


def oracle_context(
    t_a: float, t_b: float, c17: int, c19: float, ring: int = 2048
) -> dict[int, int]:
    """Context words of variant `0x120AB5`: unit send, no feedback, damping filter `1, 0, 0`."""
    c = {a: 0 for a in range(CTX - 0x20, CTX + 0x30)}
    one = f32_bits(1.0)
    c.update({
        CTX + 0xB: f32_bits(0.9), CTX + 0xC: one, CTX + 0xD: one, CTX + 0xE: f32_bits(0.9908),
        CTX + 0x24: f32_bits(1.5), CTX + 0x25: f32_bits(0.25), CTX + 6: TABLE, CTX + 0xF: OUT_ADDR,
        CTX - 0x1E: 22, CTX - 0x1D: (-22) & 0xFFFFFFFF, CTX - 0x1C: 22 << 6,
        CTX - 0xC: f32_bits(300.0), CTX - 0x1B: one, CTX - 0x14: one, CTX - 0x13: one,
        CTX - 0xB: f32_bits(0.5), CTX - 0xA: f32_bits(0.5), CTX - 7: f32_bits(0.5),
        CTX - 6: f32_bits(0.5), CTX - 1: one,
        CTX + 0x14: 0, CTX + 0x15: f32_bits(t_a), CTX + 0x16: f32_bits(t_b), CTX + 0x17: c17,
        CTX + 0x19: f32_bits(c19), CTX + 0x1A: RING, CTX + 0x1B: RING, CTX + 0x21: ring,
    })  # fmt: skip
    return c


def impulse_args(image: str, ctx: dict[int, int], calls: int) -> list[str]:
    """`sharc-fx` argument list of an impulse run (input only in call 0, output dumped per call)."""
    import fx_oracle as fo

    rt = fo.Routine("delay", ROUTINE_PC, ROUTINE_END,
                    {"I0": IN_ADDR, "I1": CTX, "M0": 0, "M2": 2, "M9": 1}, None, None,
                    {TABLE - 512 + k: 0 for k in range(1026)})  # fmt: skip
    args = fo.oracle_args(image, rt, [ctx], set())
    args[args.index("--calls") + 1] = str(calls)
    args += ["--dump", f"{IN_ADDR:#x}:4", "--poke", f"0:{IN_ADDR:#x}={f32_bits(1.0):#x}"]
    for n in range(1, calls):
        for k in range(4):
            args += ["--poke", f"{n}:{IN_ADDR + k:#x}=0"]
    return args


def first_echo(rows: list[list[float]], threshold: float = 1e-3) -> int | None:
    """Sample index of the first left-channel output above `threshold` (rows of 4 words per call:
    `l0, r0, l1, r1`, two samples per call)."""
    for k, row in enumerate(rows):
        for j in (0, 2):
            if abs(row[j]) > threshold:
                return 2 * k + j // 2
    return None


def measure_echo(image: str, binary: Path, t: float) -> int | None:  # pragma: no cover - oracle
    import subprocess
    import tempfile

    import fx_biquad_check as fb

    calls = int(t) // 2 + 8
    args = impulse_args(image, oracle_context(t, 0.0, 0x8000, 0.0), calls)
    with tempfile.NamedTemporaryFile("w", suffix=".args", delete=False) as f:
        f.write("\n".join(args[1:]))
    run = subprocess.run([str(binary), args[0], f"@{f.name}"], capture_output=True, text=True)
    Path(f.name).unlink(missing_ok=True)
    if run.returncode:
        raise RuntimeError(run.stderr.strip())
    rows = [[struct.unpack("<f", struct.pack("<I", c[IN_ADDR + j]))[0] for j in range(4)]
            for c in fb.parse_dumps(run.stdout)]  # fmt: skip
    return first_echo(rows)


def main(argv: list[str] | None = None) -> int:  # pragma: no cover - thin CLI
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)
    for name in ("check", "rust"):
        sp = sub.add_parser(name)
        sp.add_argument("code", type=Path)
        if name == "check":
            sp.add_argument("series", type=Path)
    sp = sub.add_parser("oracle")
    sp.add_argument("image")
    sp.add_argument("--times", default="2,3,20,21,100,513,1000,2000")
    sp.add_argument("--binary", type=Path, default=Path("target/release/sharc-fx"))
    ns = ap.parse_args(argv)
    if ns.cmd == "oracle":
        bad = 0
        for t in (float(x) for x in ns.times.split(",")):
            got = measure_echo(ns.image, ns.binary, t)
            ok = got == echo_delay(t)
            bad += not ok
            verdict = "ok" if ok else "DIFF"
            print(f"c15={t:g}: first echo at sample {got}, law {echo_delay(t)} {verdict}")
        return 1 if bad else 0
    table = load_table(ns.code.read_bytes())
    print(f"table sha256 {table_fingerprint(table)}")
    if ns.cmd == "rust":
        print(rust_table(table))
        return 0
    diffs = check_series(table, read_series(ns.series))
    print(f"{len(read_series(ns.series))} captured points, {len(diffs)} differ")
    for d in diffs[:10]:
        print("  v=%d captured %g predicted %d" % d)
    return 1 if diffs else 0


if __name__ == "__main__":
    sys.exit(main())
