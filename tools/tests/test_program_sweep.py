import json

import program_sweep as ps

FX_OFF = {
    "mod1_enabled": False,
    "mod1_type_raw": 0,
    "mod2_enabled": False,
    "mod2_type_raw": 0,
    "amp_enabled": False,
    "amp_type_new": 0,
    "eq_treble_db": 0.0,
    "eq_mid_db": 0.0,
    "eq_bass_db": 0.0,
    "comp_enabled": False,
    "delay_enabled": False,
    "delay_mode": 0,
    "delay_fb_fx_type": 0,
    "reverb_enabled": False,
    "reverb_type_raw": 3,
}


def synth_layer(**kw):
    base = {
        "osc_mode": "Analog",
        "osc_category": 0,
        "osc_wave": 2,
        "filter_on": True,
        "filter_model": "Lp24",
        "unison": 0,
        "arp_enabled": False,
        "mono": False,
        "lfo_dest": "Off",
        "enabled": True,
    }
    base.update(kw)
    return base


def render(nan=0, inf=0, peak=0.5, rms=0.1, clipped=0, silent=False):
    return {
        "frames": 1000,
        "nan": nan,
        "inf": inf,
        "peak": peak,
        "rms": rms,
        "clipped": clipped,
        "silent": silent,
    }


def record(masks=(0b0110000, 0), fx=None, delta=0.5, default=None, firmware=None, **synth_a):
    piano = {"category": "Grand", "string_res": True, "enabled": True}
    return {
        "entry": "Program/Bank A/Test.ns4p",
        "rms_delta_db": delta,
        "routing": {"exact_masks": list(masks), "kb_zone": [5] * 7},
        "sounds": {
            "piano_a": {"ref": True, "loaded": True},
            "piano_b": {"ref": False, "loaded": False},
            "synth_a": {"ref": False, "loaded": False},
            "synth_b": {"ref": False, "loaded": False},
            "synth_c": {"ref": False, "loaded": False},
            "piano_a_class": 5,
            "piano_b_class": None,
        },
        "renders": {"default": default or render(), "firmware": firmware or render()},
        "program": {
            "organ": {
                "model": "B3",
                "model_b": "Pipe1",
                "rotary_enabled": True,
                "rotary_speed": "Slow",
                "vibrato_on": True,
                "vibrato_on_b": False,
                "vibrato_type": 5,
                "percussion_on": True,
                "fx": dict(FX_OFF),
            },
            "piano": {
                "layer_a": piano,
                "layer_b": dict(piano),
                "fx_a": fx or dict(FX_OFF),
                "fx_b": dict(FX_OFF),
            },
            "synth": {
                "layer_a": synth_layer(**synth_a),
                "layer_b": synth_layer(),
                "layer_c": synth_layer(),
                "fx_a": dict(FX_OFF),
                "fx_b": dict(FX_OFF),
                "fx_c": dict(FX_OFF),
            },
            "global": {"split_enabled": True, "split_zone_enabled": [False, True, False]},
        },
    }


def test_used_layers_follow_scene_masks():
    assert ps.used_layers(record(masks=(0b0010001, 0))) == [0, 4]
    assert ps.used_layers(record(masks=(0, 0b100))) == [2]
    assert ps.used_layers(record(masks=(0, 0))) == []


def test_organ_piano_synth_paths():
    paths = ps.program_paths(record(masks=(0b0010101, 0)))
    assert "organ.model=B3" in paths["organ_a"] and "organ.rotary" in paths["organ_a"]
    assert "organ.vibrato" in paths["organ_a"] and "organ.percussion" in paths["organ_a"]
    assert "piano.class=5" in paths["piano_a"]
    assert "piano.extra_voice=not_played" in paths["piano_a"]
    assert "synth.analog=PURE" in paths["synth_a"]
    assert "synth.filter=Lp24" in paths["synth_a"]
    assert "routing.split" in paths["program"]
    assert "routing.engines=organ+piano+synth" in paths["program"]


def test_b3_bass_flag_replaces_the_b3_model_path():
    rec = record(masks=(0b01, 0))
    assert "organ.model=B3" in ps.program_paths(rec)["organ_a"]
    rec["program"]["organ"]["b3_bass_a"] = True
    paths = ps.program_paths(rec)["organ_a"]
    assert "organ.model=B3 Bass" in paths and "organ.model=B3" not in paths


def test_piano_classes_and_missing_samples():
    rec = record(masks=(0b100, 0))
    rec["sounds"]["piano_a_class"] = 4
    assert "piano.extra_voice=not_played" not in ps.program_paths(rec)["piano_a"]
    rec["sounds"]["piano_a_class"] = 9
    assert "piano.class=unchecked" in ps.program_paths(rec)["piano_a"]
    rec["sounds"]["piano_a"]["loaded"] = False
    assert "piano.sample=missing" in ps.program_paths(rec)["piano_a"]


def test_analog_categories_and_sample_mode():
    rec = record(masks=(0b10000, 0), osc_category=2)
    assert "synth.analog=SYNC" in ps.program_paths(rec)["synth_a"]
    rec = record(masks=(0b10000, 0), osc_mode="Sample")
    assert "synth.sample=missing" in ps.program_paths(rec)["synth_a"]
    rec["sounds"]["synth_a"]["loaded"] = True
    assert "synth.sample=loaded" in ps.program_paths(rec)["synth_a"]


def test_fx_paths_decode_names_and_ignore_disabled_units():
    fx = dict(
        FX_OFF,
        mod1_enabled=True,
        mod1_type_raw=7,
        mod2_enabled=True,
        mod2_type_raw=8,
        amp_enabled=True,
        amp_type_new=424,
        reverb_enabled=True,
        reverb_type_raw=11,
        delay_enabled=True,
        delay_fb_fx_type=7,
        eq_mid_db=3.0,
    )
    paths = ps.fx_paths(fx)
    assert "fx.mod1=A-WAH" in paths and "fx.mod2=ENS" in paths
    assert "fx.amp=LP24" in paths  # 424 & 0xF == 8
    assert "fx.reverb=CATHEDRAL" in paths and "fx.reverb.type=11" in paths
    assert "fx.delay_fb=CHOR" in paths and "fx.eq" in paths
    assert ps.fx_paths(dict(FX_OFF)) == []


def test_fx_only_counted_for_used_layers():
    fx = dict(FX_OFF, reverb_enabled=True)
    assert "fx.reverb" in ps.program_paths(record(masks=(0b100, 0), fx=fx))["fx"]
    assert "fx.reverb" not in ps.program_paths(record(masks=(0b10000, 0), fx=fx))["fx"]


def test_aggregate_counts_programs_and_uses():
    agg = ps.aggregate([record(masks=(0b110100, 0)), record(masks=(0b100, 0))])
    assert agg["piano.sample=loaded"]["programs"] == 2
    assert agg["synth.osc=Analog"] == {"programs": 1, "uses": 2}


def test_classification_covers_every_status_key_reference():
    assert ps.status("organ.model=Pipe1")[0] == ps.V
    assert ps.status("synth.analog=SYNC")[0] == ps.N
    assert ps.status("unknown.path")[2] == "no classification recorded"
    for default, firmware, basis in ps.STATUS.values():
        assert {default, firmware} <= {ps.V, ps.A, ps.N} and basis


def test_outliers_detect_each_kind():
    recs = [
        record(),
        record(default=render(nan=3)),
        record(firmware=render(peak=0.0, rms=0.0, silent=True)),
        record(default=render(clipped=500, peak=1.0)),
        record(delta=9.0),
        record(firmware={"panic": "boom", "frames": 0}),
    ]
    rows = [ps.render_row(r) for r in recs]
    found = ps.outliers(rows)
    assert len(found["non_finite"]) == 1
    assert len(found["silent"]) == 1
    assert len(found["clipping"]) == 1 and found["clipping"][0]["clip_fraction"] == 0.25
    assert [x["rms_delta_db"] for x in found["level_jump"]] == [9.0]
    assert found["panic"][0]["message"] == "boom"


def test_level_summary_and_status_totals():
    rows = [{"rms_delta_db": d} for d in (-2.0, 0.0, 2.0, None)]
    s = ps.level_summary(rows)
    assert s["n"] == 3 and s["min"] == -2.0 and s["max"] == 2.0 and s["median"] == 0.0
    assert ps.level_summary([{"rms_delta_db": None}]) == {}
    table = ps.classification_table(ps.aggregate([record()]), 1)
    totals = ps.status_totals(table)
    assert sum(totals["default"].values()) == sum(r["uses"] for r in table)


def test_markdown_and_main_roundtrip(tmp_path, monkeypatch):
    recs = [record(), record(masks=(0b10000, 0), osc_category=1)]
    jsonl = tmp_path / "sweep.jsonl"
    jsonl.write_text("\n".join(json.dumps(r) for r in recs), encoding="utf-8")
    meta = tmp_path / "meta.json"
    meta.write_text(json.dumps({"checked": "2026-01-01"}), encoding="utf-8")
    monkeypatch.setattr(ps, "DOC_PATH", tmp_path / "doc.md")
    monkeypatch.setattr(ps, "JSON_PATH", tmp_path / "data" / "out.json")
    assert ps.main(["--jsonl", str(jsonl), "--meta", str(meta)]) == 0
    doc = (tmp_path / "doc.md").read_text(encoding="utf-8")
    assert "**Checked:** 2026-01-01" in doc and "## 7. Verification status" in doc
    data = json.loads((tmp_path / "data" / "out.json").read_text(encoding="utf-8"))
    assert data["programs"] == 2 and "outliers" in data
    assert ps.main(["--jsonl", str(jsonl), "--no-write"]) == 0
