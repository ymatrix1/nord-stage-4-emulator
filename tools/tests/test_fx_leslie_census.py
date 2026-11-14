"""Tests for tools/fx_leslie_census.py: operand reduction and the address window."""

import fx_leslie_census as lc


def test_form_key_reduces_registers_and_numbers():
    assert lc.form_key("f12=f9*f12, dm(0xfffffffc,i3)=r12") == "R=R*R , dm(N,R)=R"
    assert lc.form_key("r6<->s5") == "R<->S"


def test_census_counts_only_the_routine_window(tmp_path):
    p = tmp_path / "l.txt"
    p.write_text(
        "000056ac  0abe  rts\n"
        "000056ad  c001  r0=r0+r1\n"
        "000056ae  c001  r2=r2+r3\n"
        "00006367  c001  r0=r0+r1\n",
        encoding="utf-8",
    )
    forms = lc.census(p)
    assert forms == {"R=R+R": 2}
