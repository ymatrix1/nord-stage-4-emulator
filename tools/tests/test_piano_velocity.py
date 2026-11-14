import numpy as np
import piano_velocity as pv

SR = 48_000


def tone(midi: int, amps: list[float], t0: float = 0.1, total_s: float = 0.6) -> np.ndarray:
    """Harmonic note starting at `t0`; `amps[k - 1]` is the amplitude of partial k."""
    t = np.arange(int(total_s * SR)) / SR
    f0 = pv.midi_hz(midi)
    out = sum(a * np.sin(2 * np.pi * (k + 1) * f0 * t) for k, a in enumerate(amps))
    return np.where(t >= t0, out, 0.0)


def test_midi_hz_reference_pitches():
    assert pv.midi_hz(69) == 440.0
    assert abs(pv.midi_hz(60) - 261.6256) < 1e-3


def test_partial_powers_follow_the_amplitudes():
    p = pv.partial_powers(tone(60, [1.0, 0.5, 0.25]), SR, 0.1, 60)
    assert p is not None
    assert p[0] > p[1] > p[2] > 100 * p[3]
    assert abs(10 * np.log10(p[0] / p[1]) - 6.0) < 1.5  # amplitude ratio 2 = 6 dB


def test_partial_powers_stop_below_fmax_and_reject_short_windows():
    p = pv.partial_powers(tone(96, [1.0]), SR, 0.1, 96)  # f0 = 2093 Hz: partials 1..3 below 8 kHz
    assert p is not None and len(p) == 3
    assert pv.partial_powers(np.zeros(SR // 100), SR, 0.0, 60) is None


def test_brightness_orders_a_dark_and_a_bright_note():
    dark = pv.partial_powers(tone(60, [1.0, 0.3, 0.1, 0.03, 0.01]), SR, 0.1, 60)
    bright = pv.partial_powers(tone(60, [1.0, 0.8, 0.7, 0.6, 0.5]), SR, 0.1, 60)
    assert pv.brightness_db(bright) > pv.brightness_db(dark) + 10
    assert pv.brightness_db(np.array([1.0, 1.0, 1.0])) is None
    assert pv.level_db(bright) > pv.level_db(dark)


def test_note_features_flags_a_ringing_repeat():
    t = np.arange(int(1.2 * SR)) / SR
    f0 = pv.midi_hz(60)
    ringing = np.sin(2 * np.pi * f0 * t)  # same note sounding before and after the onset
    assert pv.note_features(ringing, SR, 0.8, 60)["contaminated"]
    fresh = tone(60, [1.0, 0.5], t0=0.8, total_s=1.2)
    assert not pv.note_features(fresh, SR, 0.8, 60)["contaminated"]
    assert pv.note_features(fresh, SR, 0.1, 60) is not None  # no pre-window: never flagged
    assert not pv.note_features(fresh, SR, 0.1, 60)["contaminated"]


def test_reference_events_and_distinct_notes():
    ev = pv.reference_events([60, 62])
    assert abs(ev[1]["onset_s"] - ev[0]["onset_s"] - pv.REF_SPACING_S) < 1e-9
    assert abs(ev[0]["offset_s"] - ev[0]["onset_s"] - pv.REF_HOLD_S) < 1e-9
    assert pv.distinct_notes([{"midi": 62}, {"midi": 60}, {"midi": 62}]) == [60, 62]


def synthetic_table(notes):
    """Level rises 0.4 dB per velocity step, brightness 0.1 dB per step, per grid point."""
    grid = np.asarray(pv.VELOCITY_GRID, dtype=float)
    return {m: {"level": 30.0 + 0.4 * grid, "bright": -20.0 + 0.1 * grid} for m in notes}


def test_interp_curve_hits_grid_points_and_handles_gaps():
    vals = 30.0 + 0.4 * np.asarray(pv.VELOCITY_GRID, dtype=float)
    c = pv.interp_curve(vals)
    assert len(c) == 127
    assert abs(c[63] - (30.0 + 0.4 * 64)) < 1e-9  # index v - 1
    assert np.isnan(pv.interp_curve(np.full(len(pv.VELOCITY_GRID), np.nan))).all()


def test_fit_velocity_inverts_the_curves():
    curves = {k: pv.interp_curve(v) for k, v in synthetic_table([60])[60].items()}
    hw = {"level_db": 30.0 + 0.4 * 90 - 7.0, "bright_db": -20.0 + 0.1 * 90}
    v, cost = pv.fit_velocity(hw, curves, gain_db=-7.0)
    assert v == 90 and cost < 1e-9
    # without brightness only the level term decides
    v2, _ = pv.fit_velocity({"level_db": 30.0 + 0.4 * 50, "bright_db": None}, curves, 0.0)
    assert v2 == 50


def test_estimate_recovers_velocities_and_gain():
    table = synthetic_table([60, 64])
    true_v = [40, 70, 100, 85]
    notes = [60, 64, 60, 64]
    feats = [
        (
            m,
            {
                "level_db": 30.0 + 0.4 * v - 6.0,
                "bright_db": -20.0 + 0.1 * v,
                "contaminated": False,
            },
        )
        for m, v in zip(notes, true_v, strict=True)
    ]
    vels, gain = pv.estimate(feats, table)
    assert vels == true_v
    assert abs(gain + 6.0) < 1e-6


def test_annotate_events_writes_velocity_and_a_summary():
    mono = tone(60, [1.0, 0.5, 0.3, 0.2], t0=0.5, total_s=1.5)
    events = [
        {"midi": 60, "onset_s": 0.5, "offset_s": 1.2},
        {"midi": 61, "onset_s": 0.5, "offset_s": 1.2},  # not in the table: left unchanged
    ]
    out, gain, summary = pv.annotate_events(events, mono, SR, synthetic_table([60]))
    assert 1 <= out[0]["velocity"] <= 127
    assert "velocity" not in out[1]
    assert summary["estimated"] == 1 and summary["notes"] == 2
    assert summary["gain_db"] == round(gain, 2)


def test_build_parser_defaults():
    a = pv.build_parser().parse_args(
        ["--hw", "h", "--events", "e", "--backup", "b", "--workdir", "w", "--out-events", "o"]
    )
    assert a.entry.endswith("Velvet Grand.ns4p") and not a.confirmed_only
