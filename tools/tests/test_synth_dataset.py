"""Tests for tools/synth_dataset.py (ns4 is replaced by a fake runner)."""

from __future__ import annotations

import json
import sys
from pathlib import Path

import numpy as np

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(Path(__file__).resolve().parent))

import synth_dataset as sd  # noqa: E402
import synth_wav  # noqa: E402
from test_synth_wav import write_float_wav  # noqa: E402

LISTING = """Program/Bank A/12dB Sweep.ns4p (868 B, method=0)
Program/Bank A/Horizon Whl.ns4p (868 B, method=0)
Piano/Grand/White Grand XL 6.3.npno (123 B, method=1)
"""


def fake_render(cmd: list[str]) -> None:
    """Writes 2 s of stereo float noise to ``--wav``; silent when the program file is 'quiet'."""
    wav = Path(cmd[cmd.index("--wav") + 1])
    amp = 0.0 if "quiet" in cmd[2] else 0.2
    noise = np.random.default_rng(0).standard_normal((88200, 2)).astype(np.float32) * amp
    write_float_wav(wav, noise)


def test_parse_program_list_keeps_only_ns4p():
    assert sd.parse_program_list(LISTING) == [
        "Program/Bank A/12dB Sweep.ns4p",
        "Program/Bank A/Horizon Whl.ns4p",
    ]


def test_safe_name_stable_and_unique():
    a, b = "Program/Bank A/Horizon Whl.ns4p", "Program/Bank B/Horizon Whl.ns4p"
    assert sd.safe_name(a) == sd.safe_name(a) and sd.safe_name(a) != sd.safe_name(b)
    assert "/" not in sd.safe_name(a) and " " not in sd.safe_name(a)


def test_split_holds_out_whole_programs():
    entries = [f"Program/Bank A/P{i}.ns4p" for i in range(400)]
    frac = sum(sd.split_of(e, 10) == "val" for e in entries) / len(entries)
    assert 0.05 < frac < 0.15
    assert sd.split_of(entries[0], 10) == sd.split_of(entries[0], 10)
    assert all(sd.split_of(e, 0) == "train" for e in entries)


def test_plan_items_round_robin_and_ids_unique():
    progs = [{"entry": f"Program/Bank A/P{i}.ns4p", "file": f"p{i}.ns4p"} for i in range(3)]
    items = sd.plan_items(progs, 7, seed=2, val_percent=10)
    assert [it["program"] for it in items[:4]] == [p["entry"] for p in progs] + [progs[0]["entry"]]
    assert len({it["id"] for it in items}) == 7
    assert sd.plan_items(progs, 7, seed=2, val_percent=10) == items


def test_render_dataset_end_to_end_with_fake_ns4(tmp_path):
    progs = [
        {"entry": "Program/Bank A/Loud.ns4p", "file": "loud.ns4p"},
        {"entry": "Program/Bank A/Quiet.ns4p", "file": "quiet.ns4p"},
    ]
    (tmp_path / "programs.json").write_text(json.dumps(progs), encoding="utf-8")
    calls: list[list[str]] = []

    def runner(cmd):
        calls.append(cmd)
        fake_render(cmd)

    hours = 60 * 4 / 3600
    doc = sd.render_dataset(tmp_path, "ns4", hours, 2, 1, 10, 60.0, run=runner, augment=False)
    assert len(doc["entries"]) == 4 and len(calls) == 4
    loud = [e for e in doc["entries"] if "Loud" in e["program"]]
    quiet = [e for e in doc["entries"] if "Quiet" in e["program"]]
    assert all(not e["silent"] and e["raw_rms"] > 0.05 for e in loud)
    assert all(e["silent"] for e in quiet)
    e = loud[0]
    assert e["duration"] == 2.0 and e["notes"]
    assert set(e["notes"][0]) == {"start", "end", "pitch", "velocity"}
    wav, rate = synth_wav.read_wav(tmp_path / e["audio"])
    assert rate == 44100 and wav.shape[1] == 1  # mono
    saved = json.loads((tmp_path / "dataset.json").read_text(encoding="utf-8"))
    assert saved["entries"][0]["id"] == doc["entries"][0]["id"]
    assert "4 items" in sd.summarize(doc)
    # re-run resumes: existing WAVs are not rendered again
    sd.render_dataset(tmp_path, "ns4", hours, 2, 1, 10, 60.0, run=runner, augment=False)
    assert len(calls) == 4


def test_extract_programs_uses_listing_and_skips_existing(tmp_path, monkeypatch):
    class Done:
        stdout = LISTING

    monkeypatch.setattr(sd.subprocess, "run", lambda *a, **k: Done())
    extracted: list[str] = []

    def runner(cmd):
        extracted.append(cmd[cmd.index("--extract") + 1])
        Path(cmd[-1]).write_bytes(b"x")

    progs = sd.extract_programs("ns4", "b.ns4b", tmp_path, 2, run=runner)
    assert [p["entry"] for p in progs] == sd.parse_program_list(LISTING)
    assert len(extracted) == 2
    sd.extract_programs("ns4", "b.ns4b", tmp_path, 2, run=runner)
    assert len(extracted) == 2  # second call finds the files
    assert json.loads((tmp_path / "programs.json").read_text(encoding="utf-8")) == progs


def test_main_render_prints_summary(tmp_path, monkeypatch, capsys):
    monkeypatch.setattr(sd, "render_dataset", lambda *a, **k: {"entries": []})
    assert sd.main(["render", "--out", str(tmp_path), "--hours", "0.01"]) == 0
    assert "0 items" in capsys.readouterr().out


def test_main_extract_prints_count(tmp_path, monkeypatch, capsys):
    monkeypatch.setattr(sd, "extract_programs", lambda *a, **k: [{"entry": "e", "file": "f"}])
    assert sd.main(["extract", "--backup", "b.ns4b", "--out", str(tmp_path)]) == 0
    assert "extracted 1 programs" in capsys.readouterr().out


def test_render_item_sets_final_peak_and_writes_sidecar(tmp_path):
    (tmp_path / "programs").mkdir()

    def hot(cmd):
        wav = Path(cmd[cmd.index("--wav") + 1])
        noise = np.random.default_rng(1).standard_normal((88200, 2)).astype(np.float32)
        write_float_wav(wav, noise * 4)  # clips full scale

    item = {
        "id": "hot",
        "program": "Program/A/Hot.ns4p",
        "program_file": "hot.ns4p",
        "seed": 5,
        "split": "train",
        "wide": False,
        "vel_mode": "ramp",
        "pedal": True,
        "augment": False,
    }
    meta = sd.render_item(item, tmp_path, "ns4", 20.0, run=hot)
    mono, _ = synth_wav.read_wav(tmp_path / meta["audio"])
    assert abs(float(np.abs(mono).max()) - 10 ** (-1.0 / 20)) < 2e-3  # -1 dBFS without augmentation
    assert (tmp_path / "meta" / "hot.json").exists() and not meta["silent"]
    assert meta["pedal"] and all(p["end"] > p["start"] for p in meta["pedal"])
    assert meta["vel_mode"] == "ramp"


def test_pedal_intervals_pairs_down_and_up():
    ped = [{"t": 1.0, "value": 127}, {"t": 2.0, "value": 0}, {"t": 3.0, "value": 100}]
    assert sd.pedal_intervals(ped) == [{"start": 1.0, "end": 2.0}]


def test_plan_items_have_velocity_modes_and_pedal_share():
    progs = [{"entry": "Program/A/P.ns4p", "file": "p.ns4p"}]
    items = sd.plan_items(progs, 400, seed=1, val_percent=10)
    modes = {it["vel_mode"] for it in items}
    assert modes == set(sd.synth_phrases.VEL_MODES)
    share = sum(it["pedal"] for it in items) / len(items)
    assert 0.15 < share < 0.35
