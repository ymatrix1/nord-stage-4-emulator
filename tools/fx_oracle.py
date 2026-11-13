#!/usr/bin/env python3
"""Run FX routine scenarios through the `sharc-fx` oracle and compare them with a model.

A scenario is a flat memory image `{address: word}` that holds every word the routine reads; the
registers the pass context does not provide (`I0`, `I1`, `M0`, `M2`, ...) come from `Routine.sets`.
`compare` runs the model first (it reports every address it wrote), then the oracle on the same
memory with one `--calls` batch, dumps the written addresses and compares them word for word.
`trace_call` returns the `--trace` rows of one scenario to localise a mismatch to an instruction.
"""

from __future__ import annotations

import subprocess
import tempfile
from collections.abc import Callable
from dataclasses import dataclass, field
from pathlib import Path

import fx_biquad_check as fb
from fx_pe import Missing, parse_trace

#: Pass context of the compressor / delay runs (`docs/research/fx-type-map.md` s7): boot the
#: section, run one pass to `0x12139b` so `MODE1` (`PEYEN`, `BDCST1`) and the M registers are set.
PASS_ARGS = [
    "--dev", "0x28300b00=1", "--dev", "0x3108d008=5", "--trigger", "0xC02C2",
    "--stop-at", "0x12139b", "--max", "200000",
]  # fmt: skip
BATCH = 24  # scenarios per oracle process (the boot costs about 0.3 s)


@dataclass
class Routine:
    """What the harness needs to run one routine."""

    name: str
    pc: int
    end: int
    sets: dict[str, int]
    #: model(mem, trace=None) -> Machine (updates `mem` in place)
    model: Callable
    #: scenario(rng) -> {address: word}
    scenario: Callable
    #: addresses filled once in call 0 and shared by all scenarios (tables); not dumped
    static: dict[int, int] = field(default_factory=dict)


def runs(mem: dict[int, int]) -> list[tuple[int, list[int]]]:
    """Consecutive-address runs of `mem` as `(start, words)`."""
    out: list[tuple[int, list[int]]] = []
    for a in sorted(mem):
        if out and out[-1][0] + len(out[-1][1]) == a:
            out[-1][1].append(mem[a])
        else:
            out.append((a, [mem[a]]))
    return out


def oracle_args(image: str, rt: Routine, cases: list[dict[int, int]], dumps: set[int],
                trace: bool = False) -> list[str]:  # fmt: skip
    args = [image, *PASS_ARGS, "--call", hex(rt.pc), "--until", hex(rt.end),
            "--calls", str(len(cases))]  # fmt: skip
    for reg, v in rt.sets.items():
        args += ["--set", f"{reg}={v:#x}"]
    for a, words in runs(rt.static):
        args += ["--fill", f"0:{a:#x}=" + ",".join(f"{w:#x}" for w in words)]
    for k, mem in enumerate(cases):
        for a, words in runs(mem):
            args += ["--fill", f"{k}:{a:#x}=" + ",".join(f"{w:#x}" for w in words)]
    for a, words in runs({d: 0 for d in dumps}):
        args += ["--dump", f"{a:#x}:{len(words)}"]
    if trace:
        args.append("--trace")
    return args


def run_oracle(image: str, binary: Path, rt: Routine, cases: list[dict[int, int]],
               dumps: set[int], trace: bool = False) -> str:  # fmt: skip
    args = oracle_args(image, rt, cases, dumps, trace)
    with tempfile.NamedTemporaryFile("w", suffix=".args", delete=False) as f:
        f.write("\n".join(args[1:]))
    try:
        run = subprocess.run([str(binary), args[0], f"@{f.name}"], capture_output=True, text=True)
    finally:
        Path(f.name).unlink(missing_ok=True)
    if run.returncode != 0:
        raise RuntimeError(f"sharc-fx failed: {run.stderr.strip()}")
    return run.stdout


def model_run(rt: Routine, case: dict[int, int], trace=None) -> tuple[dict[int, int], set[int]]:
    """The model on one scenario: `(final memory, written addresses)`."""
    mem = Missing({**rt.static, **case})
    m = rt.model(mem, trace)
    return dict(mem), m.written


def compare(image: str, binary: Path, rt: Routine, rng, scenarios: int) -> tuple[int, int, list]:
    """`(scenarios with a differing word, scenarios, first failures)` of model against oracle."""
    bad, fails = 0, []
    done = 0
    while done < scenarios:
        n = min(BATCH, scenarios - done)
        cases = [rt.scenario(rng) for _ in range(n)]
        expect, wrote, dumps = [], [], set()
        for c in cases:
            mem, written = model_run(rt, c)
            expect.append(mem)
            wrote.append(written)
            dumps |= written
        out = run_oracle(image, binary, rt, cases, dumps)
        got = fb.parse_dumps(out)
        for k in range(n):
            diff = [a for a in sorted(wrote[k]) if got[k].get(a) != expect[k][a]]
            if diff:
                bad += 1
                if len(fails) < 3:
                    fails.append(
                        (done + k, [(hex(a), got[k].get(a), expect[k].get(a)) for a in diff[:4]])
                    )
        done += n
    return bad, scenarios, fails


def trace_call(image: str, binary: Path, rt: Routine, case: dict[int, int]) -> list[dict]:
    """The oracle's per-instruction `--trace` rows of one scenario (run alone)."""
    _, written = model_run(rt, case)
    return parse_trace(run_oracle(image, binary, rt, [case], written, trace=True), 0)
