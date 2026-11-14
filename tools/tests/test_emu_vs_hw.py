import json
from pathlib import Path

import emu_vs_hw as ev
import numpy as np
import pytest

SR = 48_000


def burst(rise_s: float, onset_s: float = 0.1, total_s: float = 0.6) -> np.ndarray:
    t = np.arange(int(total_s * SR)) / SR
    carrier = np.sin(2 * np.pi * 440 * t)
    env = np.clip((t - onset_s) / rise_s, 0.0, 1.0)
    return 0.5 * carrier * env


def test_attack_orders_fast_and_slow():
    fast = ev.attack_ms(burst(0.005), SR, [0.1])
    slow = ev.attack_ms(burst(0.08), SR, [0.1])
    assert fast is not None and slow is not None
    assert fast < slow
    assert slow == pytest.approx(0.8 * 80, rel=0.3)


def test_attack_none_for_silence():
    assert ev.attack_ms(np.zeros(SR), SR, [0.2]) is None


def make_metrics(deltas):
    return {"bands_delta_db": dict(zip(ev.BAND_NAMES, deltas, strict=True))}


def test_gap_score_and_worst_band():
    m = make_metrics([1, -2, 3, -4, 5, -6])
    assert ev.gap_score(m) == pytest.approx(3.0)  # air band (-6) not scored
    assert ev.worst_band(m) == (ev.BAND_NAMES[4], 5)


def test_render_cmd_flags_and_order(tmp_path):
    cmd = ev.render_cmd(
        "ns4", "e", "b.ns4b", tmp_path / "ev.json", tmp_path / "o.wav", 90, ["--firmware-fx"]
    )
    assert cmd[1:3] == ["render-patch", "e"]
    assert cmd[-1] == "--firmware-fx"
    assert cmd[cmd.index("--velocity") + 1] == "90"


def test_shipped_spec_is_valid():
    spec = ev.load_spec(Path(__file__).resolve().parents[2] / ev.DEFAULT_SPEC)
    assert len(spec["segments"]) >= 4
    assert len({s["id"] for s in spec["segments"]}) == len(spec["segments"])


def test_load_spec_rejects_missing_default(tmp_path):
    bad = {
        "segments": [
            {
                "id": "a",
                "entry": "e",
                "engine": "x",
                "at_s": 1,
                "duration_s": 2,
                "variants": [{"label": "other"}],
            }
        ]
    }
    p = tmp_path / "s.json"
    p.write_text(json.dumps(bad))
    with pytest.raises(ValueError):
        ev.load_spec(p)


def test_markdown_table_row_count():
    row = {
        "label": "default",
        "bands_delta_db": dict.fromkeys(ev.BAND_NAMES, 1.0),
        "centroid_delta_hz": 5.0,
        "rms_delta_db": 1.0,
        "width_hw": 0.5,
        "width_emu": 0.4,
        "attack_hw_ms": 3.0,
        "attack_emu_ms": None,
        "score": 1.0,
    }
    out = ev.markdown_table({"segments": [{"id": "s", "variants": [row, row]}]})
    assert len(out.splitlines()) == 4


def test_note_onsets_filters_confirmed(tmp_path):
    p = tmp_path / "ev.json"
    events = [{"onset_s": 1.0, "confirmed": True}, {"onset_s": 2.0, "confirmed": False}]
    p.write_text(json.dumps({"events": events}))
    assert ev.note_onsets(p) == [1.0, 2.0]
    assert ev.note_onsets(p, confirmed_only=True) == [1.0]


def decaying_tail(t60_s: float, release_s: float = 0.5, total_s: float = 3.0) -> np.ndarray:
    """Noise burst until `release_s`, then an exponential decay of `t60_s` (60 dB in `t60_s`)."""
    rng = np.random.default_rng(3)
    t = np.arange(int(total_s * SR)) / SR
    env = np.where(t < release_s, 0.3, 0.3 * 10 ** (-3 * (t - release_s) / t60_s))
    return env * rng.standard_normal(len(t))


def test_tail_t60_recovers_the_decay_time():
    for t60 in (0.6, 2.0):
        got = ev.tail_t60_s(decaying_tail(t60), SR, [(0.0, 0.5)])
        assert got == pytest.approx(t60, rel=0.15)


def test_tail_t60_skips_releases_followed_by_notes_and_silence():
    mono = decaying_tail(1.0)
    assert ev.tail_t60_s(mono, SR, [(0.0, 0.5), (0.7, 2.0)]) is None, "a note follows the release"
    assert ev.tail_t60_s(np.zeros(3 * SR), SR, [(0.0, 0.5)]) is None, "below the floor"
    assert ev.tail_t60_s(mono, SR, [(0.0, 2.8)]) is None, "the clip ends first"


def test_note_intervals_filters_confirmed(tmp_path):
    p = tmp_path / "ev.json"
    events = [
        {"onset_s": 1.0, "offset_s": 1.5, "confirmed": True},
        {"onset_s": 2.0, "offset_s": 2.5, "confirmed": False},
    ]
    p.write_text(json.dumps({"events": events}))
    assert ev.note_intervals(p) == [(1.0, 1.5), (2.0, 2.5)]
    assert ev.note_intervals(p, confirmed_only=True) == [(1.0, 1.5)]


def test_variant_flags_adds_confirmed_only():
    var = {"label": "x", "flags": ["--firmware-fx"]}
    assert ev.variant_flags(var, False) == ["--firmware-fx"]
    assert ev.variant_flags(var, True) == ["--firmware-fx", "--confirmed-only"]
    assert ev.variant_flags({"label": "default"}, True) == ["--confirmed-only"]


def test_main_stops_when_inputs_missing(tmp_path, capsys):
    rc = ev.main(
        [
            "--video",
            str(tmp_path / "nope.webm"),
            "--backup",
            str(tmp_path / "n.ns4b"),
            "--workdir",
            str(tmp_path),
        ]
    )
    assert rc == 2
    assert "nothing downloaded" in capsys.readouterr().err


def test_apply_edits_sets_dotted_and_indexed_fields_and_rejects_unknown():
    doc = {"organ": {"drawbars_b": [8, 8, 8], "fx": {"reverb_enabled": True}}}
    ev.apply_edits(doc, {"organ.fx.reverb_enabled": False, "organ.drawbars_b[1]": 0})
    assert doc == {"organ": {"drawbars_b": [8, 0, 8], "fx": {"reverb_enabled": False}}}
    with pytest.raises(KeyError):
        ev.apply_edits(doc, {"organ.fx.nope": 1})


def test_filter_events_keeps_the_note_range(tmp_path):
    src = tmp_path / "ev.json"
    events = [{"midi": m, "onset_s": float(m)} for m in (40, 48, 60, 64, 70)]
    src.write_text(json.dumps({"start_s": 1.0, "events": events}))
    out = ev.filter_events(src, 48, 64, tmp_path / "f.json")
    doc = json.loads(out.read_text())
    assert [e["midi"] for e in doc["events"]] == [48, 60, 64]
    assert doc["start_s"] == 1.0


def test_variant_velocity_prefers_variant_then_segment_then_100():
    assert ev.variant_velocity({}, {}) == 100
    assert ev.variant_velocity({"velocity": 90}, {}) == 90
    assert ev.variant_velocity({"velocity": 90}, {"velocity": 64}) == 64
