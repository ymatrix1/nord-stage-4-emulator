"""Tests for tools/fx_leslie_run.py: argument construction for the oracle run."""

import fx_leslie_run as lr


def test_boot_pokes_turn_script_lines_into_call_zero_pokes(tmp_path):
    p = tmp_path / "b.script"
    p.write_text("0 0xb2f29 0x1237f8\n\n0 0xb2f2a 0xe359e\n", encoding="utf-8")
    assert lr.boot_pokes(p) == ["--poke", "0:0xb2f29=0x1237f8", "--poke", "0:0xb2f2a=0xe359e"]


def test_noise_is_seeded_four_words_per_call_in_range():
    a, b = lr.noise_pokes(3), lr.noise_pokes(3)
    assert a == b and len(a) == 3 * 4 * 2
    vals = [float(x.split("=f")[1]) for x in a[1::2]]
    assert all(abs(v) <= 0.3 for v in vals) and len(set(vals)) == len(vals)


def test_build_args_calls_the_leslie_routine_with_the_row_state(tmp_path):
    p = tmp_path / "b.script"
    p.write_text("0 0xb2f2d 0x0\n", encoding="utf-8")
    args = lr.build_args(p, 2, ["--poke", "0:0xb2f31=f1"])
    assert args[args.index("--call") + 1] == "0x1256ad"
    assert "I1=0xb2f2a" in args and "--poke" in args
    assert args[-2:] == ["--dump", "0xb6000:4"]
