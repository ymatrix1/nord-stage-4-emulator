import os
import sys

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

import synth_strips as s  # noqa: E402

HEADER = "id,name,value,layer,kind,stage,chip,comm,core,path,dm_word,w2,pc,error\n"


def line(eid, name, value, layer, chip, core, dm, w2):
    pre = f"{eid},{name},{value},{layer},synth,call,{chip},1,{core},2"
    return f"{pre},{dm:#x},{w2:#010x},0xc0062d8c,\n"


def test_word_location():
    assert s.word_location(0xE0000) == ("strip", 0, 0)
    assert s.word_location(0xE001A) == ("strip", 0, 26)
    assert s.word_location(0xE0177) == ("strip", 6, 3)
    assert s.word_location(0xE02E8) == ("common", 0, 0)
    assert s.word_location(0xE02E8 + 32 + 7) == ("common", 1, 7)
    assert s.word_location(0xB00E6)[0] == "other"
    assert s.COMMON_BASE == 0xE02E8


def test_as_float():
    assert s.as_float(0xC0000000) == -2.0
    assert s.as_float(0x3F800000) == 1.0


def test_summarize_constant_and_value_dependent_words():
    text = HEADER
    for v in (0, 63, 127):
        text += line("0x12e", "Edt_Syn_kAmpEnvAtk", v, 4, "ORG", 1, 0xE001A, 0xC0000000)
        text += line("0x12e", "Edt_Syn_kAmpEnvAtk", v, 4, "PNO", 2, 0xE001A, 0xC0000000)
        text += line("0x135", "Edt_Syn_kFltRes", v, 5, "ORG", 1, 0xE02EF + 32, 0xBF000000 - 4 * v)
    summary = s.summarize(s.parse_rows(text))
    by = {(r["layer"], r["name"]): r for r in summary}
    atk = by[("A", "Edt_Syn_kAmpEnvAtk")]
    assert atk["value_dependent"] == "no"
    assert atk["chips"] == "ORG1 PNO2"
    assert atk["where"] == "strip 0 word 26"
    assert atk["float_first"] == "-2"
    res = by[("B", "Edt_Syn_kFltRes")]
    assert res["value_dependent"] == "yes"
    assert res["where"] == "common 1 word 7"
    assert res["samples"].startswith("0:0xbf000000")


def test_summarize_skips_errors_and_keeps_path1():
    text = HEADER + "0x1,Edt_Syn_kX,0,4,synth,error,SY,3,0,1,0x921a6,0x00000000,0x0,boom\n"
    text += "0x1,Edt_Syn_kX,0,4,synth,call,SY,3,0,1,0x921a6,0x3d4ccccd,0x0,\n"
    rows = s.summarize(s.parse_rows(text))
    assert [r["where"] for r in rows] == ["other"] and rows[0]["chips"] == "SY0"


def test_summary_csv_round_trip():
    text = HEADER + line("0x12e", "Edt_Syn_kAmpEnvAtk", 0, 4, "ORG", 1, 0xE001A, 0xC0000000)
    out = s.summary_csv(s.summarize(s.parse_rows(text)))
    assert out.splitlines()[0].startswith("layer,id,name,chips,dm_word,where")
    assert "0xe001a" in out
    assert s.summary_csv([]) == ""
