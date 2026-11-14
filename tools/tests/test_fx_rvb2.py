"""Unit tests of the types 2..11 reverb tools (no firmware image needed)."""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import fx_model_rvb2 as mdl
import fx_model_rvb2_wrap as wrap
import fx_rvb2_check as chk
import fx_rvb2_lift as lift


def test_split_top_keeps_memory_operands_together():
    assert lift.split_top("R0 = DM(I2, M3), F1 = F0 * F4") == ["R0 = DM(I2, M3)", "F1 = F0 * F4"]


def test_lift_load_with_postmodify_and_broadcast():
    st = lift.lift_ins(lift.Ins(0x100, 2, "R4 = PM(I9, M9), F8 = F0 * F4"))
    assert st.w[4] == ("ldb", ("I", 9))
    assert st.d["i9"] == ("mod", 9, 9)
    assert st.w[8] == ("fmul", ("r", 0), ("r", 4))


def test_lift_flag_and_condition():
    st = lift.lift_ins(lift.Ins(0x100, 1, "R4 = PASS R4"))
    assert st.after == [("az", 4)]
    st = lift.lift_ins(lift.Ins(0x101, 1, "IF NE R1 = S1"))
    assert st.w[1][0] == "cnd"


def test_rendering_of_a_step():
    st = lift.lift_ins(lift.Ins(0x100, 2, "DM(-13, I3) = R4"))
    assert lift.py_step(st) == ["step(0x100, st=[(A(3, -13), r(4))])"]
    assert "m.wr2(sa0, sv0)" in lift.rs_step(st)


def test_model_helpers():
    assert mdl.btgl([1, 0], 0) == [0, 1]
    assert mdl.flag_az([0, 1]) == [True, False]
    assert mdl.cnd("NE", {"az": [False, True], "sz": [False, False]}, [5, 6], [7, 8]) == [5, 8]
    assert mdl.fext_p([0xABCD1234, 0], 16, 16, False) == [0xABCD, 0]
    assert mdl.fdep_or([0x1234, 0], [0xABCD, 0], 16, 16) == [0xABCD1234, 0]
    assert callable(wrap.wrap4)


def test_instance_has_the_table_pointers():
    mem = chk.instance(2)
    assert mem[chk.C + 0] == 0xB0F74 and mem[chk.C + 1] == 0xB0F84
    assert mem[0x95013] == 0x106  # the per-type words of `fx-type-words.csv`
    assert chk.instance(3)[0x95013] == 1324
