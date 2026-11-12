import os
import sys

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

import adi_compare as c  # noqa: E402


def test_numbers_become_signed_integers_but_register_names_are_untouched():
    assert c.canonical("r0=dm(0xffffffe4,i3)") == "r0=dm(-28,i3)"
    assert c.canonical("R0 = DM(-28, I3)") == "r0=dm(-28,i3)"
    assert c.canonical("r10=r11+r12") == "r10=r11+r12"
    assert c.canonical("r15=0xffffffe1") == c.canonical("R15 = -31")
    assert c.canonical("f0=0x3f800000") == "f0=1065353216"


def test_case_spacing_and_trailing_semicolon_do_not_matter():
    assert c.canonical("F9 = F9 - F8 ;") == c.canonical("f9=f9-f8")


def test_pre_modify_accesses_are_written_modify_first():
    assert c.canonical("R8 = DM(I6, M6) [pre-mod]") == c.canonical("r8=dm(m6,i6)")
    # post-modify keeps index first
    assert c.canonical("R8 = DM(I2, M0)") == "r8=dm(i2,m0)"
    assert c.canonical("R4 = PM(I9, M10) [pre-mod]") == "r4=pm(m10,i9)"


def test_parts_of_a_multifunction_instruction_are_order_independent():
    adi = "f0=float r0 by r15, r3=dm(0xffffffe4,i5)"
    ours = "R3 = DM(-28, I5), F0 = FLOAT R0 BY R15"
    assert c.canonical(adi) == c.canonical(ours)
    assert c.canonical("comp(r1,r0)") != c.canonical("comp(r0,r1)")


def test_split_parts_keeps_commas_inside_parentheses():
    assert c.split_parts("comp(r1,r0),r2=dm(i1,m1)") == ["comp(r1,r0)", "r2=dm(i1,m1)"]


def test_long_word_marker_and_not_conditions_are_normalised():
    assert c.canonical("i0=dm(0xc1386) (LW)") == c.canonical("I0 = DM(0xc1386)")
    assert c.canonical("if not_ac") == c.canonical("if notac")


def test_our_listing_tiles_and_decodes_parcels():
    parcels = [0x0F8F, 0xFFE1, 0x0001, 0x023E, 0x1010, 0x1810]
    listing = c.our_listing(parcels)
    assert [(a, n) for a, (n, _p, _t) in listing.items()] == [(0, "17b"), (2, "21c"), (3, "6n")]
    assert listing[3][2] == "R1 = FEXT R0 BY 24:8"


def test_compare_counts_same_different_and_tiling_mismatches():
    ours = c.our_listing([0x023E, 0x1010, 0x1810, 0x0001])
    adi = {
        0: ([0x023E, 0x1010, 0x1810], "r1=fext r0 by 0x18:0x8"),  # same
        3: ([0x0001], "r1=r2"),  # different text
        9: ([0x0001], "nop"),  # no instruction at that address
    }
    stats, examples = c.compare(adi, ours)
    assert stats["6n"] == [1, 0, 0]
    assert stats["21c"][1] == 1 and examples["21c"][0][2] == "r1=r2"
    assert stats["(tiling)"][2] == 1
