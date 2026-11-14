import wave

import numpy as np
import pytest
import synth_validate as sv


def row(variant, score, presence=0.0, rms=0.0):
    return {
        "variant": variant,
        "score": score,
        "presence_db": presence,
        "brilliance_db": 0.0,
        "rms_db": rms,
        "centroid_hz": 0.0,
        "attack_hw_ms": None,
        "attack_emu_ms": None,
    }


def clip(cid, source, role, legacy, new):
    return {
        "id": cid,
        "source": source,
        "role": role,
        "wave": "saw",
        "variants": [row("legacy", legacy[0], *legacy[1:]), row("new", new[0], *new[1:])],
    }


def test_shipped_spec_is_valid_and_starts_with_legacy():
    spec = sv.load_spec()
    assert spec["variants"][0]["label"] == "legacy"
    ids = [c["id"] for c in spec["clips"]]
    assert len(ids) == len(set(ids))
    assert {c["role"] for c in spec["clips"]} == {"fit", "holdout"}
    for c in spec["clips"]:
        if c["source"] == "ag":
            assert c["window_s"][0] < c["window_s"][1] and "ag" in c
        else:
            assert "segment" in c


def test_spec_validation_rejects_bad_variants(tmp_path):
    bad = tmp_path / "s.json"
    bad.write_text('{"variants": [{"label": "x"}], "clips": []}')
    with pytest.raises(ValueError):
        sv.load_spec(bad)


def test_p_table_override_replaces_only_the_table_selection():
    var = {
        "label": "t",
        "flags": ["--synth-chain", "strip", "--synth-p", "table"],
        "ag": {"p": "table", "chain": "strip"},
    }
    assert sv.variant_flags(var, None)[-1] == "table"
    assert sv.variant_flags(var, "table:saw=1/0/1/.5/1")[-1] == "table:saw=1/0/1/.5/1"
    assert sv.variant_ag(var, "table:x")["p"] == "table:x"
    fixed = {"label": "f", "flags": ["--synth-p", "fitted"], "ag": {"p": "fitted"}}
    assert sv.variant_flags(fixed, "table:x") == ["--synth-p", "fitted"]
    assert sv.variant_ag(fixed, "table:x")["p"] == "fitted"


def test_cut_wav_slices_and_caches(tmp_path):
    sr = 8000
    src = tmp_path / "a.wav"
    with wave.open(str(src), "wb") as w:
        w.setnchannels(2)
        w.setsampwidth(2)
        w.setframerate(sr)
        w.writeframes((np.arange(sr * 4 * 2) % 30000).astype("<i2").tobytes())
    dst = sv.cut_wav(src, 1.0, 2.5, tmp_path / "sub" / "b.wav")
    with wave.open(str(dst)) as w:
        assert w.getnframes() == int(1.5 * sr) and w.getnchannels() == 2
    assert sv.cut_wav(src, 0.0, 1.0, dst) == dst  # cached file is returned untouched
    with wave.open(str(dst)) as w:
        assert w.getnframes() == int(1.5 * sr)


def test_verdict_passes_on_a_clear_win_and_fails_on_a_regression():
    win = {
        "clips": [
            clip("a", "partA", "fit", (6.0, -9, 3), (1.0, -1, 1)),
            clip("b", "partA", "holdout", (5.0, -5, 2), (3.0, -2, 1)),
        ]
    }
    v = sv.verdict(win, "new")
    assert v["passes"] and v["mean_gain"] == 3.5 and v["better"] == 2
    bad = {
        "clips": [
            clip("a", "partA", "fit", (6.0, -1, 0), (1.0, -1, 0)),
            clip("b", "partA", "holdout", (2.0, -1, 0), (2.5, -6, 0)),
        ]
    }
    v = sv.verdict(bad, "new")
    assert not v["passes"] and v["worse_than_bar"] == ["b:presence_db -1.0 -> -6.0"]


def test_rms_bar_is_ignored_for_lesson_clips_but_not_for_part_a():
    ag = {"clips": [clip("z", "ag", "holdout", (5.0, 0, 0), (1.0, 0, -9))]}
    assert sv.verdict(ag, "new")["passes"]
    pa = {"clips": [clip("z", "partA", "holdout", (5.0, 0, 0), (1.0, 0, -9))]}
    v = sv.verdict(pa, "new")
    assert not v["passes"] and "rms_db" in v["worse_than_bar"][0]


def test_table_and_summary_render_every_variant():
    res = {"clips": [clip("a", "partA", "fit", (6.0, -9, 3), (1.0, -1, 1))]}
    t = sv.table(res)
    assert "| a | fit | legacy | 6.00 |" in t and "| a | fit | new | 1.00 |" in t
    s = sv.summary(res)
    assert "new (all)" in s and "new (holdout)" in s and "pass" in s


def test_verdict_against_another_variant_drops_the_majority_rule():
    res = {
        "clips": [
            {
                "id": "a",
                "source": "partA",
                "role": "fit",
                "wave": "saw",
                "variants": [
                    row("legacy", 6.0, -9, 3),
                    row("base", 3.0, -2, 1),
                    row("kl", 2.0, -2, 1),
                ],
            },
            {
                "id": "b",
                "source": "partA",
                "role": "holdout",
                "wave": "saw",
                "variants": [
                    row("legacy", 6.0, -9, 3),
                    row("base", 3.0, -2, 1),
                    row("kl", 3.0, -2, 1),
                ],
            },
        ]
    }
    assert sv.verdict(res, "kl")["better"] == 2  # both beat legacy
    v = sv.verdict(res, "kl", "base")
    assert v["passes"] and v["better"] == 1 and v["mean_gain"] == 0.5
    assert "kl (all)" in sv.summary(res, "base") and "base (all)" not in sv.summary(res, "base")
