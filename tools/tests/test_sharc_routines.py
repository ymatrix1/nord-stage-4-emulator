import os
import sys

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

import sharc_routines as r  # noqa: E402

BIQUAD = """\
00000000  2080 4a3f  r0=dm(i0,m2), r4=pm(i9,m9)
00000002  527f 0293 0804  f8=f0*f4, r5=pm(i9,m9)
00000005  01d9 8c04  f8=f0*f4, f12=f9-f12
00000007  0193 0c27  f12=f2*f7
00000009  0193 0d37  f13=f3*f7
0000000b  0abe  rts
0000000c  0f80 0001  r0=1
0000000e  0abe  rts
"""

DELAYED = """\
00000000  0f80 0001  r0=1
00000002  0afe  rts (db)
00000003  0001  nop
00000004  0001  nop
00000005  0f81 0002  r1=2
00000007  0abe  rts
"""


def test_parse_listing_applies_base_and_skips_noise():
    text = "garbage\n" + BIQUAD
    ins = r.parse_listing(text, 0x120300)
    assert ins[0] == (0x120300, "r0=dm(i0,m2), r4=pm(i9,m9)")
    assert len(ins) == 8


def test_split_routines_handles_delay_slots():
    routines = r.split_routines(r.parse_listing(DELAYED))
    assert [len(x) for x in routines] == [4, 2]
    assert routines[1][-1][1] == "rts"


def test_split_keeps_unterminated_tail():
    routines = r.split_routines(r.parse_listing("00000000  0f80 0001  r0=1\n"))
    assert len(routines) == 1


def test_signature_counts_a_biquad_section():
    first = r.split_routines(r.parse_listing(BIQUAD))[0]
    f = r.signature(first)
    assert f["n"] == 6 and f["biquad"] == 1 and f["mul"] == 4
    assert f["fir"] == 1 and f["pm"] == 2 and f["dm"] == 1


def test_biquad_pairs_needs_same_multiplier_and_consecutive_registers():
    assert r.biquad_pairs(["f12=f2*f7", "f13=f3*f7"]) == 1
    assert r.biquad_pairs(["f12=f2*f7", "f13=f3*f8"]) == 0
    assert r.biquad_pairs(["f12=f2*f7", "f14=f3*f7"]) == 0
    assert r.biquad_pairs(["f12=f2*f7", "f13=f3*f7", "f12=f2*f7", "f13=f3*f7"]) == 2


def test_other_features():
    texts = [
        "lcntr=0x1c, do (pc,0x40) until lce",
        "f0=recips f12",
        "l0=0x100",
        "f0=clip f14 by f0",
        "r12=funpack f12",
        "r4=fext r13 by r8",
        "call (m8,i8) (db)",
        "rts",
    ]
    f = r.signature([(i, t) for i, t in enumerate(texts)])
    assert (f["loop"], f["recips"], f["circ"], f["clip"], f["cvt"], f["bitf"], f["call"]) == (
        1,
        1,
        1,
        1,
        1,
        1,
        1,
    )


def test_census_table_and_cli(tmp_path, capsys):
    rows = r.census(BIQUAD, 0x1000, 1)
    assert [(a, b) for a, b, _ in rows] == [(0x1000, 0x100B), (0x100C, 0x100E)]
    assert len(r.census(BIQUAD, 0x1000, 5)) == 1
    table = r.format_rows(rows)
    assert table.splitlines()[0].startswith("first") and "001000-00100b" in table
    f = tmp_path / "l.txt"
    f.write_text(BIQUAD)
    assert r.main([str(f), "0x1000", "--min", "1"]) == 0
    assert "001000" in capsys.readouterr().out
