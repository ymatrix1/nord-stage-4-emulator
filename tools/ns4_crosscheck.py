#!/usr/bin/env python3
"""Cross-check our Synth `0x32`/`0x2F` bit map against external evidence.

Two independent checks (neither is firmware validation, so the map stays EMPIRICAL):

1. ``--program`` / ``--csv``: compare our decoded fields of an `.ns4p` against the third-party
   ns4decode CSV of the same program (``regressionTest_synth.csv``).
2. ``--corpus``: scan every synth preset of a `.ns4b` backup and report values outside each
   field's legal range (a wrong offset produces out-of-range or impossible combinations).

    python tools/ns4_crosscheck.py --program regressionTest.ns4p --csv regressionTest_synth.csv
    python tools/ns4_crosscheck.py --corpus "<backup>.ns4b"
"""

from __future__ import annotations

import argparse
import csv
import io
import re
import sys
from pathlib import Path

import ns4_bitscan
import ns4_synth_fields as sf

OCTAVE_NAMES = {0: "0", 1: "+1", 2: "+2", -1: "-1", -2: "-2"}
PRIORITY_NAMES = {0: "no priority", 1: "LO", 2: "HI"}
LFO_TARGETS = {0: "nothing", 1: "OSC CTRL", 2: "PITCH", 3: "FILTER"}
LFO_SHAPES = {0: "triangle", 1: "saw", 2: "inv saw", 3: "square", 4: "S/H"}
ON_OFF = {0: "off", 1: "on"}

# Legal raw ranges (inclusive) used by the corpus scan.
LEGAL_RANGES: dict[str, tuple[int, int]] = {
    "octave": (-2, 2),
    "vibrato_mode": (0, 5),
    "voice_priority": (0, 2),
    "unison": (0, 3),
    "osc_type": (0, 3),
    "lfo_target": (0, 3),
    "lfo_shape": (0, 4),
    "filter_type": (0, 5),
    "pitch_coarse": (-24, 24),
}


def load_env_tables(presets_rs: Path) -> dict[str, list[float]]:
    """Parses the ADR time tables (ms) out of `cbin/presets.rs`."""
    src = presets_rs.read_text(encoding="utf-8")
    tables: dict[str, list[float]] = {}
    for name in ("ENV_ATTACK_MS_TABLE", "ENV_DECAY_MS_TABLE"):
        m = re.search(rf"pub const {name}: \[f32; 128\] = \[(.*?)\];", src, re.S)
        if not m:
            raise ValueError(f"{name} not found in {presets_rs}")
        tables[name] = [float(x) for x in re.findall(r"-?\d+(?:\.\d+)?", m.group(1))]
    return tables


def parse_time_ms(text: str) -> float:
    """`'1.82 s'` -> 1820.0, `'24 ms'` -> 24.0, `'inf'` -> inf."""
    t = text.strip().lower()
    if t == "inf":
        return float("inf")
    m = re.fullmatch(r"([\d.]+)\s*(ms|s|sec)", t)
    if not m:
        raise ValueError(f"unparseable time {text!r}")
    value = float(m.group(1))
    return value if m.group(2) == "ms" else value * 1000.0


def read_synth_csv(text: str) -> dict[str, list[str]]:
    """`parameter -> [layer A, layer B, layer C]` from an ns4decode synth CSV."""
    out: dict[str, list[str]] = {}
    for row in csv.reader(io.StringIO(text)):
        if len(row) >= 4 and row[0].strip() != "parameter":
            out[row[0].strip()] = [c.strip() for c in row[1:4]]
    return out


def _adr_matches(table: list[float], idx: int, expected: str, hold_at_max: bool) -> bool:
    """Decay index 127 is the panel "Hold" (inf); attack/release index 127 is the table maximum."""
    got = float("inf") if (hold_at_max and idx == 127) else table[idx]
    want = parse_time_ms(expected)
    if want == float("inf") or got == float("inf"):
        return got == want
    return abs(got - want) <= max(0.06 * want, 0.51)


def crosscheck_program(
    ns4p: bytes, csv_text: str, tables: dict[str, list[float]]
) -> list[tuple[str, str, bool, str, str]]:
    """Returns `(parameter, layer, ok, ours, theirs)` for every comparable field."""
    payload = sf.synth_payload_from_ns4p(ns4p)
    expect = read_synth_csv(csv_text)
    atk, dcy = tables["ENV_ATTACK_MS_TABLE"], tables["ENV_DECAY_MS_TABLE"]
    results: list[tuple[str, str, bool, str, str]] = []

    def record(param: str, layer: int, ok: bool, ours: object) -> None:
        results.append((param, "ABC"[layer], ok, str(ours), expect[param][layer]))

    for layer in range(3):
        v = sf.read_layer_fields(payload, layer)
        mapped = {
            "octave shift": OCTAVE_NAMES.get(v["octave"], "?"),
            "mono on/off": ON_OFF[v["mono"]],
            "legato on/off": ON_OFF[v["legato"]],
            "voice priority": PRIORITY_NAMES.get(v["voice_priority"], "?"),
            "glide": f"{int(10 * v['glide'] / 127 * 10) / 10:.1f}",
            "unison level": str(v["unison"]),
            "amp env velocity": str(v["amp_env_velocity"]),
            "LFO target": LFO_TARGETS.get(v["lfo_target"], "?"),
            "LFO shape": LFO_SHAPES.get(v["lfo_shape"], "?"),
            "LFO mst clk on/off": ON_OFF[v["lfo_mst_clk"]],
            "osc env-to-pitch on/off": ON_OFF[v["osc_env_to_pitch"]],
            "osc env velocity on/off": ON_OFF[v["osc_env_velocity"]],
            "filter velocity on/off": ON_OFF[v["filter_velocity"]],
            "filter on/off": ON_OFF[v["filter_on"]],
        }
        for param, ours in mapped.items():
            if param in expect:
                record(param, layer, ours == expect[param][layer], ours)
        for param, key, table, hold in (
            ("osc env attack", "osc_env_attack", atk, False),
            ("osc env decay", "osc_env_decay", dcy, True),
            ("osc env release", "osc_env_release", dcy, False),
            ("amp env attack", "amp_env_attack", atk, False),
            ("amp env decay", "amp_env_decay", dcy, True),
            ("amp env release", "amp_env_release", dcy, False),
            ("filter env attack", "filter_env_attack", atk, False),
            ("filter env decay", "filter_env_decay", dcy, True),
            ("filter env release", "filter_env_release", dcy, False),
        ):
            if param in expect:
                ok = _adr_matches(table, v[key], expect[param][layer], hold)
                record(param, layer, ok, v[key])
        if "pitch fine" in expect:
            want = int(re.sub(r"[^\d+-]", "", expect["pitch fine"][layer].replace(" ", "")) or 0)
            record("pitch fine", layer, v["pitch_fine"] == want, v["pitch_fine"])
        if "pitch coarse" in expect:
            want = int(re.sub(r"[^\d+-]", "", expect["pitch coarse"][layer].replace(" ", "")) or 0)
            record("pitch coarse", layer, v["pitch_coarse"] == want, v["pitch_coarse"])
    return results


def corpus_violations(layers: list[dict[str, int]]) -> list[str]:
    """Out-of-range values or impossible combinations across many decoded layers."""
    problems: list[str] = []
    for name, (lo, hi) in LEGAL_RANGES.items():
        bad = sorted({r[name] for r in layers if not lo <= r[name] <= hi})
        if bad:
            problems.append(f"{name} outside {lo}..{hi}: {bad}")
    both = sum(1 for r in layers if r["mono"] and r["legato"])
    if both:
        problems.append(f"{both} layers have mono and legato both set")
    bad_wave = sum(1 for r in layers if r["osc_type"] == 3 and r["osc_category"] > 4)
    if bad_wave:
        problems.append(f"{bad_wave} WAVE layers have category > 4")
    return problems


def corpus_layers(path: Path) -> list[dict[str, int]]:
    layers: list[dict[str, int]] = []
    for payload in ns4_bitscan.load_presets(path).values():
        layers.extend(sf.read_layer_fields(payload, k) for k in range(3))
    return layers


def main(argv: list[str] | None = None) -> int:
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    p.add_argument("--program", type=Path, help="complete .ns4p file")
    p.add_argument("--csv", type=Path, help="ns4decode *_synth.csv for --program")
    p.add_argument("--corpus", type=Path, help=".ns4b archive or dir of .ns4y files")
    p.add_argument(
        "--presets-rs",
        type=Path,
        default=Path(__file__).resolve().parents[1] / "crates/nord-core/src/cbin/presets.rs",
    )
    args = p.parse_args(argv)
    failed = 0
    if args.program and args.csv:
        res = crosscheck_program(
            args.program.read_bytes(),
            args.csv.read_text(encoding="utf-8"),
            load_env_tables(args.presets_rs),
        )
        bad = [r for r in res if not r[2]]
        print(f"program cross-check: {len(res) - len(bad)}/{len(res)} fields match")
        for param, layer, _, ours, theirs in bad:
            print(f"  MISMATCH {param} layer {layer}: ours={ours} ns4decode={theirs}")
        failed += len(bad)
    if args.corpus:
        layers = corpus_layers(args.corpus)
        problems = corpus_violations(layers)
        print(f"corpus check: {len(layers)} layers, {len(problems)} problems")
        for line in problems:
            print("  " + line)
        failed += len(problems)
    if not (args.program and args.csv) and not args.corpus:
        p.error("give --program with --csv, and/or --corpus")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
