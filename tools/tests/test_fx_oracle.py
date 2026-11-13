"""Tests for tools/fx_oracle.py (scenario runs against `sharc-fx`, with a fake oracle process)."""

import random
import sys
from pathlib import Path

import fx_model_delay as delay
import fx_oracle as fo
import pytest
from fx_pe import Machine


def toy_model(mem, trace=None):
    """`mem[0x10] = mem[0x0] + 1` through a one-step machine."""
    m = Machine(mem, {})
    new = [mem[0] + 1, 0]
    m.step(0, w={0: new}, st=[(0x10, new)])
    return m


def toy_routine(**kw):
    return fo.Routine("toy", 0x1000, 0x1004, {"I1": 0x20}, toy_model,
                      lambda rng: {0: rng.randrange(100)}, **kw)  # fmt: skip


def test_runs_groups_consecutive_addresses():
    assert fo.runs({5: 1, 6: 2, 7: 3, 9: 4, 3: 0}) == [(3, [0]), (5, [1, 2, 3]), (9, [4])]
    assert fo.runs({}) == []


def test_oracle_arguments_cover_fills_dumps_and_registers():
    rt = toy_routine(static={0x40: 7, 0x41: 8})
    args = fo.oracle_args("img.bin", rt, [{0: 3}, {0: 4}], {0x10, 0x11, 0x20}, trace=True)
    assert args[0] == "img.bin"
    assert args[args.index("--call") + 1] == "0x1000"
    assert args[args.index("--until") + 1] == "0x1004"
    assert args[args.index("--calls") + 1] == "2"
    assert args[args.index("--set") + 1] == "I1=0x20"
    fills = [args[i + 1] for i, a in enumerate(args) if a == "--fill"]
    assert fills == ["0:0x40=0x7,0x8", "0:0x0=0x3", "1:0x0=0x4"]
    dumps = [args[i + 1] for i, a in enumerate(args) if a == "--dump"]
    assert dumps == ["0x10:2", "0x20:1"]
    assert args[-1] == "--trace"


def fake_oracle(monkeypatch, tamper=None):
    """Replace the `sharc-fx` process by the toy model's own output (optionally tampered)."""

    def fake(image, binary, rt, cases, dumps, trace=False):
        out = []
        for k, case in enumerate(cases):
            mem, _ = fo.model_run(rt, case)
            if tamper:
                mem[0x10] ^= tamper
            out.append(f"call {k}: 1 steps")
            out += [f"dump {a:x} {mem.get(a, 0):08x} 0.0" for a in sorted(dumps)]
        return "\n".join(out)

    monkeypatch.setattr(fo, "run_oracle", fake)


def test_compare_counts_scenarios_with_differing_words(monkeypatch):
    rt = toy_routine()
    fake_oracle(monkeypatch)
    assert fo.compare("img", Path("bin"), rt, random.Random(1), 60) == (0, 60, [])
    fake_oracle(monkeypatch, tamper=1)
    bad, n, fails = fo.compare("img", Path("bin"), rt, random.Random(1), 60)
    assert (bad, n) == (60, 60) and fails[0][1][0][0] == "0x10"


def test_model_run_reports_unset_reads_and_written_addresses():
    rt = toy_routine(static={5: 1})
    mem, written = fo.model_run(rt, {0: 9})
    assert mem[0x10] == 10 and written == {0x10, 0x11}
    with pytest.raises(KeyError, match="unset address"):
        fo.model_run(rt, {})


def test_trace_call_parses_the_rows_of_the_oracle(monkeypatch):
    rt = toy_routine()
    words = " ".join(["0"] * 96)

    def fake(image, binary, rt_, cases, dumps, trace=False):
        assert trace and dumps == {0x10, 0x11}
        return f"trace 1000 {words}\ncall 0: 1 steps"

    monkeypatch.setattr(fo, "run_oracle", fake)
    rows = fo.trace_call("img", Path("bin"), rt, {0: 1})
    assert [r["pc"] for r in rows] == [0x1000]


def test_run_oracle_passes_the_arguments_in_a_file_and_surfaces_failures(tmp_path):
    ok = tmp_path / "ok.py"
    ok.write_text("import sys\nprint(open(sys.argv[1][1:]).read().split()[0])\n")
    out = fo.run_oracle(str(ok), Path(sys.executable), toy_routine(), [{0: 1}], {0x10})
    assert out.strip() == "--dev"
    bad = tmp_path / "bad.py"
    bad.write_text("import sys\nsys.stderr.write('boom')\nsys.exit(3)\n")
    with pytest.raises(RuntimeError, match="boom"):
        fo.run_oracle(str(bad), Path(sys.executable), toy_routine(), [{0: 1}], {0x10})


def test_real_routine_descriptions_are_complete():
    rt = delay.routine()
    assert (rt.pc, rt.end) == (0x1206B0, 0x1208AE)
    assert rt.sets["I1"] == delay.CTX and len(rt.static) == 1026
