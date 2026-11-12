import io
import math
import os
import struct
import sys

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

import organ_tonegen_census as o  # noqa: E402

# Listing excerpt of PNO Core1 pc 0x122656 (listing address 0x2456, base 0x120200).
LOOP_LISTING = """
00002453   56ff 0017 9114   r1=r1*r4(ssfr), r0=pm(i11,m11);
00002456   0c00 5b80 0018   lcntr=0x5b, do (pc,0xff800018) until lce;
00002459   573f 8100 1201   r2=r0+r1, pm(i11,m12)=r2;
0000245c   44fe 02a4 002b   r0=fext r2 by r11, r5=dm(m3,i2);
0000245f   453e 0300 0000   r6=dm(m4,i2);
0000246b   3a8b 1080 0000   dm(i5,m2)=r5, r1=pm(i12,m10);
"""
PLAIN_LOOP = """
00000100   0c00 0280 0003   lcntr=0x2, do (pc,0xff800003) until lce;
00000103   9408 0667   r12=dm(0xffffffe7,i2);
00000105   0180 1ecc   r14=r12+r12;
"""


def test_semitone_runs_finds_float_and_ignores_curves():
    note = [8.0 * SEMI_STEP**k for k in range(30)]
    runs = o.semitone_runs(note)
    assert runs == [(0, 30, runs[0][2])]
    assert abs(runs[0][2] - o.SEMITONE) < 1e-12
    down = [100.0 / o.SEMITONE**k for k in range(14)]
    assert o.semitone_runs(down)[0][1] == 14
    assert o.semitone_runs([1.0 + 0.001 * k for k in range(40)]) == []  # linear ramp
    assert o.semitone_runs(note[:8]) == []  # shorter than min_len


SEMI_STEP = 2 ** (1 / 12)


def test_semitone_runs_rejects_sign_changes_and_zero():
    assert o.semitone_runs([0.0] * 20) == []
    alt = [(-1) ** k * 1.0594**k for k in range(20)]
    assert o.semitone_runs(alt) == []


def test_sine_windows_detects_sine_but_not_ramp_or_noise():
    sine = [math.sin(2 * math.pi * k / 64) for k in range(256)]
    found = o.sine_windows(sine)
    assert found and abs(found[0][2] - 64.0) < 0.01
    ramp = [0.5 * k for k in range(200)]
    assert o.sine_windows(ramp) == []
    smooth = [1.0 - (k / 400) ** 2 for k in range(200)]  # c just below 2, period >> window
    assert o.sine_windows(smooth) == []
    assert o.sine_windows([1.0, 2.0]) == []


def test_harmonic_levels_recovers_known_spectrum():
    n = 1024
    levels_db = {1: 0.0, 2: -19.0, 3: -36.0}
    wave = [
        sum(10 ** (db / 20) * math.sin(2 * math.pi * h * k / n) for h, db in levels_db.items())
        for k in range(n)
    ]
    rows = o.harmonic_levels(wave, 5)
    for h, db in levels_db.items():
        assert abs(rows[h - 1][2] - db) < 1e-6
        assert abs(rows[h - 1][3]) < 1e-6  # sine phase
    assert rows[3][2] == float("-inf") or rows[3][2] < -200


def test_interleaved_table_and_word_helpers():
    pairs = [(0.25, -0.5), (0.5, -0.25)]
    words = [0] * 3 + [struct.unpack("<I", struct.pack("<f", v))[0] for p in pairs for v in p]
    xs, ys = o.interleaved_table(words, 3, 2)
    assert xs == [0.25, 0.5] and ys == [-0.5, -0.25]
    assert o.as_i32(0xFFFFFFFF) == -1 and o.as_i32(5) == 5
    assert o.as_f32(0x3F800000) == 1.0


def test_find_phase_loops_flags_oscillator_but_not_plain_loop():
    hits = o.find_phase_loops(LOOP_LISTING, base=0x120200)
    assert hits == [(0x122656, 0x5B)]
    assert o.find_phase_loops(PLAIN_LOOP) == []


def test_load_words_and_cli(tmp_path):
    vals = [8.0 * SEMI_STEP**k for k in range(16)]
    img = tmp_path / "img.bin"
    img.write_bytes(b"".join(struct.pack("<f", v) for v in vals) + b"\x01")
    assert len(o.load_words(img)) == 16
    out = io.StringIO()
    assert o.cmd_tables(img, out) == 0
    assert "semitone f32 DM 0x0 len 16" in out.getvalue()
    lst = tmp_path / "l.txt"
    lst.write_text(LOOP_LISTING)
    out = io.StringIO()
    assert o.cmd_loops(lst, 0x120200, out) == 0
    assert "pc 0x122656 count 0x5b (91)" in out.getvalue()
    table = tmp_path / "t.bin"
    cyc = [
        v
        for k in range(64)
        for v in (math.sin(2 * math.pi * k / 64), math.cos(2 * math.pi * k / 64))
    ]
    table.write_bytes(b"".join(struct.pack("<f", v) for v in cyc))
    out = io.StringIO()
    assert o.cmd_harmonics(table, 0, 64, out) == 0
    assert "x: n magnitude dB phase" in out.getvalue()
    assert o.main(["harmonics", str(table), "0", "--pairs", "64"]) == 0
