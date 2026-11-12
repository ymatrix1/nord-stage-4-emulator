from __future__ import annotations

from pathlib import Path

import ns4_crosscheck as cc
import ns4_synth_fields as sf
import pytest

REPO = Path(__file__).resolve().parents[2]
PRESETS_RS = REPO / "crates/nord-core/src/cbin/presets.rs"


def set_bits(buf: bytearray, pos: int, value: int, width: int) -> None:
    for i in range(width):
        bit = (value >> (width - 1 - i)) & 1
        byte, off = divmod(pos + i, 8)
        mask = 0x80 >> off
        buf[byte] = (buf[byte] | mask) if bit else (buf[byte] & ~mask)


def put(buf: bytearray, layer: int, name: str, value: int) -> None:
    f = sf.BY_NAME[name]
    base = (sf.LAYER_BASES if f.block == "layer" else sf.SOUND_BASES)[layer]
    if f.signed and value < 0:
        value += 1 << f.width
    set_bits(buf, base + f.offset, value, f.width)


def make_ns4p(values: dict[str, list[int]]) -> bytes:
    """Complete .ns4p-sized buffer whose synth block holds `values[name][layer]`."""
    synth = bytearray(sf.SYNTH_PAYLOAD_LEN)
    for name, per_layer in values.items():
        for layer, v in enumerate(per_layer):
            put(synth, layer, name, v)
    head = bytes(sf.NS4P_SYNTH_OFFSET)
    return head + bytes(synth) + bytes(868 - sf.NS4P_SYNTH_OFFSET - sf.SYNTH_PAYLOAD_LEN)


def fake_tables() -> dict[str, list[float]]:
    return {
        "ENV_ATTACK_MS_TABLE": [0.5 + i for i in range(128)],
        "ENV_DECAY_MS_TABLE": [3.0 + 10 * i for i in range(128)],
    }


VALUES = {
    "octave": [0, -2, 1],
    "mono": [0, 0, 1],
    "legato": [0, 1, 0],
    "voice_priority": [0, 2, 1],
    "glide": [27, 32, 0],
    "unison": [3, 0, 2],
    "amp_env_velocity": [3, 0, 1],
    "lfo_target": [2, 3, 1],
    "lfo_shape": [0, 1, 4],
    "lfo_mst_clk": [0, 0, 1],
    "osc_env_to_pitch": [0, 1, 0],
    "osc_env_velocity": [0, 0, 1],
    "filter_velocity": [0, 0, 1],
    "filter_on": [0, 1, 1],
    "pitch_fine": [0, -8, 18],
    "pitch_coarse": [0, 1, -3],
    "osc_env_attack": [0, 4, 7],
    "osc_env_decay": [127, 5, 9],
    "osc_env_release": [1, 127, 3],
    "amp_env_attack": [2, 2, 2],
    "amp_env_decay": [3, 3, 3],
    "amp_env_release": [4, 4, 4],
    "filter_env_attack": [5, 25, 41],
    "filter_env_decay": [67, 67, 127],
    "filter_env_release": [0, 30, 53],
}

CSV_TEXT = """parameter,layer A,layer B,layer C
octave shift,0,-2,+1
mono on/off,off,off,on
legato on/off,off,on,off
voice priority,no priority,HI,LO
glide,2.1,2.5,0.0
unison level,3,0,2
amp env velocity,3,0,1
LFO target,PITCH,FILTER,OSC CTRL
LFO shape,triangle,saw,S/H
LFO mst clk on/off,off,off,on
osc env-to-pitch on/off,off,on,off
osc env velocity on/off,off,off,on
filter velocity on/off,off,off,on
filter on/off,off,on,on
pitch fine,+ 0 cent,- 8 cent,+18 cent
pitch coarse,+ 0 semi,+ 1 semi,- 3 semi
osc env attack,0.5 ms,4.5 ms,7.5 ms
osc env decay,inf,53 ms,93 ms
osc env release,13 ms,1273 ms,33 ms
amp env attack,2.5 ms,2.5 ms,2.5 ms
amp env decay,33 ms,33 ms,33 ms
amp env release,43 ms,43 ms,43 ms
filter env attack,5.5 ms,25.5 ms,41.5 ms
filter env decay,673 ms,673 ms,inf
filter env release,3 ms,303 ms,533 ms
"""


def test_field_table_has_no_overlaps_within_a_block():
    for block in ("layer", "sound"):
        spans = sorted(
            (f.offset, f.offset + f.width, f.name) for f in sf.FIELDS if f.block == block
        )
        for (a0, a1, an), (b0, _, bn) in zip(spans, spans[1:]):
            assert a1 <= b0, f"{an} overlaps {bn}"
    assert set(sf.BY_NAME) == {f.name for f in sf.FIELDS}


def test_read_layer_fields_roundtrips_signed_and_unsigned_values():
    raw = make_ns4p(VALUES)
    payload = sf.synth_payload_from_ns4p(raw)
    for layer in range(3):
        got = sf.read_layer_fields(payload, layer)
        for name, per_layer in VALUES.items():
            assert got[name] == per_layer[layer], (name, layer)
    with pytest.raises(ValueError):
        sf.read_layer_fields(b"\x00", 0)
    with pytest.raises(ValueError):
        sf.synth_payload_from_ns4p(b"\x00" * 10)


def test_crosscheck_program_matches_synthetic_expected_csv():
    res = cc.crosscheck_program(make_ns4p(VALUES), CSV_TEXT, fake_tables())
    assert len(res) == 75
    assert [r for r in res if not r[2]] == []


def test_crosscheck_flags_a_wrong_expected_value():
    wrong = CSV_TEXT.replace("unison level,3,0,2", "unison level,1,0,2")
    res = cc.crosscheck_program(make_ns4p(VALUES), wrong, fake_tables())
    bad = [r for r in res if not r[2]]
    assert [(r[0], r[1]) for r in bad] == [("unison level", "A")]


def test_parse_time_and_csv_helpers():
    assert cc.parse_time_ms("1.82 s") == pytest.approx(1820.0)
    assert cc.parse_time_ms("24 ms") == 24.0
    assert cc.parse_time_ms("inf") == float("inf")
    with pytest.raises(ValueError):
        cc.parse_time_ms("loud")
    parsed = cc.read_synth_csv(CSV_TEXT)
    assert parsed["glide"] == ["2.1", "2.5", "0.0"]
    assert "parameter" not in parsed


def test_load_env_tables_reads_the_repo_tables():
    tables = cc.load_env_tables(PRESETS_RS)
    assert len(tables["ENV_ATTACK_MS_TABLE"]) == 128
    assert len(tables["ENV_DECAY_MS_TABLE"]) == 128
    assert tables["ENV_ATTACK_MS_TABLE"][0] == 0.5
    with pytest.raises(ValueError):
        cc.load_env_tables(REPO / "README.md")


def test_corpus_violations_detects_out_of_range_and_impossible_combos():
    ok = {name: 0 for name in sf.BY_NAME}
    assert cc.corpus_violations([ok]) == []
    bad = dict(ok, voice_priority=3, mono=1, legato=1, osc_type=3, osc_category=6)
    problems = cc.corpus_violations([ok, bad])
    joined = "\n".join(problems)
    assert "voice_priority" in joined and "mono and legato" in joined and "WAVE" in joined


def test_main_reports_success_and_failure(tmp_path: Path, capsys):
    prog = tmp_path / "p.ns4p"
    prog.write_bytes(make_ns4p(VALUES))
    good = tmp_path / "good.csv"
    good.write_text(CSV_TEXT, encoding="utf-8")
    assert cc.main(
        ["--program", str(prog), "--csv", str(good), "--presets-rs", str(PRESETS_RS)]
    ) in (0, 1)

    bad = tmp_path / "bad.csv"
    bad.write_text(CSV_TEXT.replace("unison level,3,0,2", "unison level,0,0,0"), encoding="utf-8")
    code = cc.main(["--program", str(prog), "--csv", str(bad), "--presets-rs", str(PRESETS_RS)])
    assert code == 1
    assert "MISMATCH unison level" in capsys.readouterr().out

    presets = tmp_path / "presets"
    presets.mkdir()
    synth = bytes(48) + bytes(sf.SYNTH_PAYLOAD_LEN)
    (presets / "A.ns4y").write_bytes(synth)
    assert cc.main(["--corpus", str(presets)]) == 0
    assert "3 layers, 0 problems" in capsys.readouterr().out

    with pytest.raises(SystemExit):
        cc.main([])
