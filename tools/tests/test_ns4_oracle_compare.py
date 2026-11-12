import math
import os
import struct
import sys

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

import ns4_oracle_compare as c  # noqa: E402


def write_csv(path, series, meta=None):
    lines = ["# ns4 oracle dump v1"]
    for k, v in (meta or {}).items():
        lines.append(f"# {k}: {v}")
    names = list(series)
    lines.append("index," + ",".join(names))
    rows = max(len(v) for v in series.values())
    for n in range(rows):
        cells = [repr(series[name][n]) if n < len(series[name]) else "" for name in names]
        lines.append(f"{n}," + ",".join(cells))
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")


def write_wav(path, channels, fmt_tag=3, bits=32, rate=48000):
    frames = len(channels[0])
    if fmt_tag == 3:
        payload = b"".join(struct.pack("<f", ch[n]) for n in range(frames) for ch in channels)
    else:
        payload = b"".join(
            struct.pack("<h", int(ch[n] * 32767)) for n in range(frames) for ch in channels
        )
    fmt = struct.pack(
        "<HHIIHH",
        fmt_tag,
        len(channels),
        rate,
        rate * len(channels) * bits // 8,
        len(channels) * bits // 8,
        bits,
    )
    body = (
        b"WAVE"
        + b"fmt "
        + struct.pack("<I", 16)
        + fmt
        + b"data"
        + struct.pack("<I", len(payload))
        + payload
    )
    path.write_bytes(b"RIFF" + struct.pack("<I", len(body)) + body)


def test_read_csv_metadata_and_round_to_float32(tmp_path):
    p = tmp_path / "d.csv"
    write_csv(p, {"L": [0.1, 0.2], "src": [1.0]}, {"ref.mode": "resample"})
    meta, cols = c.read_csv(p)
    assert meta["ref.mode"] == "resample"
    assert cols["L"] == [c.f32(0.1), c.f32(0.2)]
    assert cols["src"] == [1.0]
    _, raw = c.read_csv(p, round32=False)
    assert raw["L"][0] == 0.1


def test_read_wav_float_and_pcm16(tmp_path):
    f = tmp_path / "f.wav"
    write_wav(f, [[0.5, -0.25], [1.0, 0.0]])
    meta, cols = c.read_wav(f)
    assert meta["rate"] == "48000"
    assert cols["ch0"] == [0.5, -0.25] and cols["ch1"] == [1.0, 0.0]
    q = tmp_path / "q.wav"
    write_wav(q, [[0.5, -0.5]], fmt_tag=1, bits=16)
    assert abs(c.read_any(q)[1]["ch0"][0] - 0.5) < 1e-4
    bad = tmp_path / "bad.wav"
    bad.write_bytes(b"nope" * 8)
    try:
        c.read_wav(bad)
        raise AssertionError("expected ValueError")
    except ValueError:
        pass


def test_ulp_distance_and_f32():
    assert c.ulp_distance(1.0, 1.0) == 0
    assert c.ulp_distance(1.0, struct.unpack("<f", struct.pack("<I", 0x3F800001))[0]) == 1
    assert c.ulp_distance(-0.0, 0.0) == 0
    assert c.ulp_distance(1.0, -1.0) > 1_000_000_000
    assert c.f32(0.1) != 0.1


def test_compare_identical_and_scaled():
    ref = [math.sin(0.1 * n) for n in range(200)]
    same = c.compare(ref, ref)
    assert same["max_abs"] == 0.0 and same["snr_db"] == math.inf and same["exact"] == 200
    off = [v * 1.001 for v in ref]
    s = c.compare(off, ref)
    assert 59.0 < s["snr_db"] < 61.0  # 20 log10(1/0.001) = 60 dB
    assert s["max_abs"] < 1.0e-3 + 1e-12
    assert c.compare([], [])["n"] == 0
    assert c.compare([1.0], [0.0])["snr_db"] == -math.inf


def test_estimate_delay_integer_and_subsample():
    ref = [math.sin(0.2 * n) * math.exp(-((n - 100) ** 2) / 800.0) for n in range(220)]
    delayed = [0.0, 0.0, 0.0] + ref[:-3]
    assert abs(c.estimate_delay(delayed, ref) - 3.0) < 0.01
    advanced = ref[2:] + [0.0, 0.0]
    assert abs(c.estimate_delay(advanced, ref) + 2.0) < 0.01
    half = [
        math.sin(0.2 * (n - 0.5)) * math.exp(-(((n - 0.5) - 100) ** 2) / 800.0) for n in range(220)
    ]
    assert abs(c.estimate_delay(half, ref) - 0.5) < 0.05
    assert c.estimate_delay([], [1.0]) == 0.0
    assert c.estimate_delay([0.0] * 8, [1.0] * 8) == 0.0


def test_text_plots():
    spark = c.sparkline([0.0] * 10 + [1.0] + [0.0] * 10, width=21)
    assert len(spark) == 21 and spark[10] == "@" and spark[0] == " "
    assert c.sparkline([0.0, 0.0]) == "__"
    assert c.sparkline([]) == ""
    rows = c.text_plot([math.sin(0.1 * n) for n in range(128)], width=32, height=5)
    assert len(rows) == 5 and all("|" in r for r in rows)
    assert c.text_plot([]) == []


def test_pairs_for():
    o = {"L": [1.0], "R": [1.0], "srcL": [1.0]}
    r = {"L": [1.0], "srcL": [1.0], "x": [1.0]}
    assert c.pairs_for(o, r, None) == [("L", "L")]
    assert c.pairs_for(o, r, "L:x,R") == [("L", "x"), ("R", "R")]


def test_main_exit_codes_and_report(tmp_path, capsys):
    ref = [0.1 * n for n in range(20)]
    a, b = tmp_path / "o.csv", tmp_path / "r.csv"
    write_csv(a, {"L": ref, "srcL": [1.0]}, {"scenario": "t.scn"})
    write_csv(b, {"L": ref})
    assert c.main([str(a), str(b), "--tol-abs", "1e-9", "--plot"]) == 0
    out = capsys.readouterr().out
    assert "max |err|" in out and "error profile" in out and "scenario: t.scn" in out
    write_csv(b, {"L": [v + 1e-3 for v in ref]})
    assert c.main([str(a), str(b), "--tol-abs", "1e-6", "--quiet"]) == 1
    assert "FAIL" in capsys.readouterr().out
    assert c.main([str(a), str(b), "--tol-snr", "200"]) == 1
    assert "tolerance exceeded" in capsys.readouterr().out
    assert c.main([str(a), str(tmp_path / "missing.csv")]) == 2
    assert c.main([str(a), str(b), "--series", "nope"]) == 2
    other = tmp_path / "x.csv"
    write_csv(other, {"Z": ref})
    assert c.main([str(a), str(other)]) == 2
