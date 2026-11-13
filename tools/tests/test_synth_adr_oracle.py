import os
import sys

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

import synth_adr_oracle as o  # noqa: E402


def test_float_word_conversions_round_trip():
    assert o.f2u(1.0) == 0x3F800000
    assert o.u2f(0x3F000000) == 0.5
    assert o.u2f(o.f2u(-0.25)) == -0.25


def test_build_command_orders_the_options():
    cmd = o.build_command(
        "sharc-org",
        "img.bin",
        4,
        {o.CONTROLLER: 0x01000100},
        [(2, 0xC0DC9, 0x3DCCCCCD)],
        [(1, 3, 0x40000000)],
        ["0xc1168:64:f32"],
        "out.csv",
    )
    assert cmd[:2] == ["sharc-org", "img.bin"]
    assert ["--passes", "4"] == cmd[cmd.index("--passes") : cmd.index("--passes") + 2]
    assert "0xc1dcc=0x1000100" in cmd
    assert "2:0xc0dc9=0x3dcccccd" in cmd
    assert "1:3=0x40000000" in cmd
    assert cmd[-2:] == ["--csv", "out.csv"]


def test_parse_rows_groups_by_pass_and_channel():
    text = "pass,channel,index,address,raw,value\n0,0,1,0x1,0x00000002,0\n1,2,0,0x1,0x3f800000,1\n"
    rows = o.parse_rows(text)
    assert rows == {(0, 0): {1: 2}, (1, 2): {0: 0x3F800000}}


def test_chain_pokes_rewrite_every_record_word_from_pass_one():
    pokes = o.chain_pokes({o.ATTACK: (1.0, 0.5, 0.0, o.DECAY)}, 3)
    assert len(pokes) == 8  # 2 passes x 4 words
    assert pokes[0] == (1, o.ATTACK, 0x3F800000)
    assert pokes[3] == (1, o.ATTACK + 3, o.DECAY)
    assert pokes[4][0] == 2


def test_default_paths_read_the_environment(monkeypatch):
    monkeypatch.setenv("NS4_SHARC_ORG", "x")
    monkeypatch.setenv("NS4_ORG_IMAGE", "y")
    assert o.default_paths() == ("x", "y")
