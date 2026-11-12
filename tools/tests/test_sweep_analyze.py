from __future__ import annotations

import math
import struct
from pathlib import Path

import pytest
import sweep_analyze as sa

RATE = 48_000


def write_wav(path: Path, samples: list[float], rate: int = RATE) -> None:
    pcm = b"".join(struct.pack("<h", int(max(-1.0, min(1.0, s)) * 32767)) for s in samples)
    header = (
        b"RIFF"
        + struct.pack("<I", 36 + len(pcm))
        + b"WAVEfmt "
        + struct.pack("<IHHIIHH", 16, 1, 1, rate, rate * 2, 2, 16)
        + b"data"
        + struct.pack("<I", len(pcm))
    )
    path.write_bytes(header + pcm)


def sine(freq_of_t, seconds: float) -> list[float]:
    out, phase = [], 0.0
    for n in range(int(seconds * RATE)):
        phase += 2.0 * math.pi * freq_of_t(n / RATE) / RATE
        out.append(0.5 * math.sin(phase))
    return out


def test_pitch_track_recovers_a_steady_tone():
    track = sa.pitch_track(sine(lambda t: 440.0, 0.5), RATE)
    assert len(track) > 100
    assert all(abs(f - 440.0) < 0.5 for _, f in track)


def test_glide_rate_measures_constant_rate_semitone_slope():
    rate_semi = 24.0  # one octave takes 0.5 s

    def freq(t: float) -> float:
        semis = min(max((t - 0.2) * rate_semi, 0.0), 12.0)
        return 220.0 * 2.0 ** (semis / 12.0)

    track = sa.pitch_track(sine(freq, 1.2), RATE)
    g = sa.glide_rate(track)
    assert g["semitones_per_s"] == pytest.approx(rate_semi, rel=0.03)
    assert g["sec_per_octave"] == pytest.approx(0.5, rel=0.03)
    assert g["linearity"] > 0.99
    assert g["total_semitones"] == pytest.approx(12.0, abs=0.1)


def test_glide_rate_rejects_flat_and_short_tracks():
    flat = sa.pitch_track(sine(lambda t: 440.0, 0.5), RATE)
    with pytest.raises(ValueError):
        sa.glide_rate(flat)
    with pytest.raises(ValueError):
        sa.glide_rate(flat[:3])


def test_vibrato_stats_recovers_rate_and_peak_depth():
    depth_cents, rate_hz = 30.0, 6.0
    track = sa.pitch_track(
        sine(
            lambda t: 440.0 * 2.0 ** (depth_cents * math.sin(2 * math.pi * rate_hz * t) / 1200.0),
            2.0,
        ),
        RATE,
    )
    v = sa.vibrato_stats(track)
    assert v["rate_hz"] == pytest.approx(rate_hz, rel=0.03)
    assert v["depth_cents_peak"] == pytest.approx(depth_cents, rel=0.1)
    with pytest.raises(ValueError):
        sa.vibrato_stats(track[:5])
    with pytest.raises(ValueError):
        sa.vibrato_stats(sa.pitch_track(sine(lambda t: 440.0, 0.3), RATE))


def test_unison_spread_finds_symmetric_detune_in_cents():
    f0, cents = 1046.5, 7.0
    lo, hi = f0 * 2 ** (-cents / 1200), f0 * 2 ** (cents / 1200)
    n = 1 << 16
    sig = [
        0.3
        * (
            math.sin(2 * math.pi * f0 * i / RATE)
            + math.sin(2 * math.pi * lo * i / RATE)
            + math.sin(2 * math.pi * hi * i / RATE)
        )
        for i in range(n)
    ]
    u = sa.unison_spread(sig, RATE, f0, size=n)
    assert u["spread_cents"] == pytest.approx(2 * cents, abs=1.0)
    assert u["peak_cents"][0] == pytest.approx(-cents, abs=0.7)
    with pytest.raises(ValueError):
        sa.unison_spread([0.0] * 4096, RATE, 100000.0, size=4096)


def test_pitch_excursion_reports_overshoot_relative_to_settled_pitch():
    def freq(t: float) -> float:
        return 440.0 * 2.0 ** ((5.0 * math.exp(-t * 20.0)) / 12.0)

    ex = sa.pitch_excursion(sa.pitch_track(sine(freq, 0.6), RATE))
    assert ex["max_semitones"] == pytest.approx(5.0, abs=0.3)
    assert abs(ex["min_semitones"]) < 0.1
    with pytest.raises(ValueError):
        sa.pitch_excursion([(0.0, 440.0)])


def test_cli_prints_measurements(tmp_path: Path, capsys):
    wav = tmp_path / "vib.wav"
    write_wav(
        wav,
        sine(lambda t: 440.0 * 2.0 ** (20.0 * math.sin(2 * math.pi * 5.0 * t) / 1200.0), 2.0),
    )
    assert sa.main(["vibrato", str(wav)]) == 0
    assert "rate_hz" in capsys.readouterr().out
    assert sa.main(["excursion", str(wav)]) == 0
    assert "max_semitones" in capsys.readouterr().out
    rate, mono = sa.load_mono(wav)
    assert rate == RATE and len(mono) == 2 * RATE


def test_rms_dbfs_levels_and_cli(tmp_path: Path, capsys):
    loud = sine(lambda t: 440.0, 1.0)
    quiet = [x * 0.1 for x in loud]
    assert sa.rms_dbfs(loud, RATE, 0.1, 0.9) - sa.rms_dbfs(quiet, RATE, 0.1, 0.9) == pytest.approx(
        20.0, abs=0.05
    )
    assert sa.rms_dbfs([0.0] * 4800, RATE, 0.0, 0.1) == -math.inf
    with pytest.raises(ValueError):
        sa.rms_dbfs(loud, RATE, 0.5, 0.5)
    wav = tmp_path / "lvl.wav"
    write_wav(wav, loud)
    assert sa.main(["level", str(wav), "--from", "0.1", "--to", "0.9"]) == 0
    assert "rms_dbfs" in capsys.readouterr().out


def test_cli_unison_harmonic_option(tmp_path: Path, capsys):
    f0, cents = 392.0, 4.0
    voices = [f0 * 2 ** (-cents / 1200), f0 * 2 ** (cents / 1200)]
    n = 1 << 16
    sig = [0.3 * sum(math.sin(2 * math.pi * 10 * f * i / RATE) for f in voices) for i in range(n)]
    wav = tmp_path / "uni.wav"
    write_wav(wav, [x / 2 for x in sig])
    assert sa.main(["unison", str(wav), "--f0", "392", "--harmonic", "10"]) == 0
    out = capsys.readouterr().out
    spread = float(out.split("spread_cents:")[1].split()[0])
    assert spread == pytest.approx(8.0, abs=0.1)
