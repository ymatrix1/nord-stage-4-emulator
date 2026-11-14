import io

import fx_chorus_clip as fc
import numpy as np
import pytest

FS = 48_000.0


def edge_mask(period: int = 480, sep: int = 50, y0: int = 800) -> np.ndarray:
    """Two periods of one trace: a first edge, then a taller-up second edge `sep` pixels later."""
    m = np.zeros((fc.FRAME_H, fc.FRAME_W), bool)
    for x0 in (100, 100 + period):
        m[y0 : y0 + 200, x0 : x0 + 2] = True
        m[y0 - 100 : y0 + 80, x0 + sep : x0 + sep + 2] = True
    return m


def test_period_is_the_f0_period():
    assert fc.PERIOD_MS == pytest.approx(11.4535, abs=1e-3)
    assert fc.delay_ms(50, 500) == pytest.approx(0.1 * fc.PERIOD_MS)


def test_edge_groups_find_vertical_edges_and_ignore_short_runs():
    m = edge_mask()
    m[1500, 10:900] = True  # a horizontal line is not an edge
    edges = fc.edge_groups(m)
    assert len(edges) == 4
    assert sorted(round(e[0]) for e in edges) == [100, 150, 580, 630]


def test_trace_pairs_and_clusters_give_sep_period_and_height_ratio():
    rows = fc.trace_pairs(fc.edge_groups(edge_mask()))
    assert rows
    tr = fc.cluster_traces(rows)
    assert len(tr) == 1
    assert tr[0]["sep"] == pytest.approx(50, abs=1.5)
    assert tr[0]["period"] == pytest.approx(480, abs=1.5)
    assert tr[0]["h"] == pytest.approx(180 / 200, abs=0.02)


def test_two_traces_are_split_by_screen_row():
    m = edge_mask(y0=700) | edge_mask(y0=1250)
    tr = fc.cluster_traces(fc.trace_pairs(fc.edge_groups(m)))
    assert [round(t["y"]) for t in tr] == [700, 1250]


def test_lfo_stats_recovers_rate_and_range():
    t = np.arange(0, 12, 0.25)
    d = 1.2 + 0.6 * np.sin(2 * np.pi * 0.34 * t)
    s = fc.lfo_stats(t, d)
    assert s["rate_hz"] == pytest.approx(0.34, abs=0.03)
    assert s["mean"] == pytest.approx(1.2, abs=0.05)
    assert s["max"] == pytest.approx(1.8, abs=0.1) and s["min"] == pytest.approx(0.6, abs=0.1)


def test_saw_decay_has_one_edge_per_period():
    x = fc.saw_decay(FS, 1.0)
    rises = np.where(np.diff(x) > 0.1)[0]
    assert len(rises) == pytest.approx(fc.F0_HZ, abs=1)
    assert x[rises[0] + 1] > x[rises[0]]


def test_edge_separation_measures_a_known_delayed_copy():
    x = fc.saw_decay(FS, 3.0)
    d = 100  # samples
    y = x.copy()
    y[d:] += 0.8 * x[:-d]
    r = fc.edge_separation(x, y, FS)
    assert len(r) > 100
    assert np.median(r[:, 1]) == pytest.approx(d / FS * 1000, abs=0.05)
    assert np.median(r[:, 3] / r[:, 2]) == pytest.approx(0.8, abs=0.1)


def test_probe_summary_reports_the_second_channel_delay(monkeypatch, tmp_path):
    x = fc.saw_decay(FS, 14.0)
    mod = (40 + 20 * np.sin(2 * np.pi * 0.34 * np.arange(len(x)) / FS)).astype(int)
    wet = np.array([x[max(0, i - m)] for i, m in enumerate(mod)])

    def fake(x_in, mode, rate, amount, fs, hz, work):
        return np.stack([x_in, x_in + 0.9 * wet], axis=1)

    monkeypatch.setattr(fc, "run_probe", fake)
    res = fc.probe_summary("fw4", 30, 64, 0.34, tmp_path)
    assert res[0] is None
    assert res[1]["rate_hz"] == pytest.approx(0.34, abs=0.05)
    assert res[1]["min"] < res[1]["mean"] < res[1]["max"]
    assert res[1]["mean"] == pytest.approx(40 / FS * 1000, abs=0.2)


def test_scope_rows_read_frames_from_the_decoder(monkeypatch):
    frame = np.zeros((fc.FRAME_H, fc.FRAME_W, 3), np.uint8)
    frame[edge_mask()] = 255

    class FakeProc:
        stdout = io.BytesIO(frame.tobytes() * 2)

    monkeypatch.setattr(fc.subprocess, "Popen", lambda *a, **k: FakeProc())
    rows = fc.scope_rows("clip.mp4", 10.0, 0.5)
    assert [r["t"] for r in rows] == [10.0, 10.25]
    assert rows[0]["ms"] == pytest.approx(fc.delay_ms(50, 480), abs=0.1)


def test_main_stats_prints_the_trace_statistics(tmp_path, capsys):
    p = tmp_path / "s.csv"
    lines = ["t,trace,y,sep,period,h,ms"]
    for i in range(48):
        t = 22.5 + i / 4
        lines.append(f"{t},0,1000,50,500,0.86,{1.2 + 0.6 * np.sin(2 * np.pi * 0.34 * t)}")
    p.write_text("\n".join(lines))
    assert fc.main(["stats", str(p)]) == 0
    assert "rate_hz" in capsys.readouterr().out


def test_main_grid_writes_a_csv(monkeypatch, tmp_path):
    seen = []

    def fake(mode, rate, amount, hz, work):
        seen.append((mode, rate, amount))
        v = {k: 1.0 for k in fc.GRID_KEYS}
        return {0: None, 1: v}

    monkeypatch.setattr(fc, "probe_summary", fake)
    out = tmp_path / "g.csv"
    assert fc.main(["grid", "--workdir", str(tmp_path), "--out", str(out)]) == 0
    rows = out.read_text().splitlines()
    assert rows[0].startswith("mode,rate,amount,ch")
    assert len(rows) == 1 + len(fc.GRID) and len(seen) == len(fc.GRID)
