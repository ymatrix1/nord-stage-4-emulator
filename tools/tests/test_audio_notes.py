import audio_notes as an
import numpy as np
import pytest

SR = 48_000


def tone(midi: int, dur: float, harmonics: int = 6, amp: float = 0.2) -> np.ndarray:
    t = np.arange(int(dur * SR)) / SR
    f0 = an.midi_to_hz(midi)
    sig = np.zeros_like(t)
    for h in range(1, harmonics + 1):
        sig += (0.8 ** (h - 1)) * np.sin(2 * np.pi * f0 * h * t)
    return amp * sig / harmonics


def found(events):
    return {e.midi for e in events}


def test_midi_to_hz():
    assert an.midi_to_hz(69) == pytest.approx(440.0)
    assert an.midi_to_hz(60) == pytest.approx(261.6256, rel=1e-5)


def test_single_note_detected_exactly():
    ev = an.transcribe(tone(60, 1.5), SR)
    assert found(ev) == {60}
    assert ev[0].onset_s < 0.2 and ev[0].offset_s > 1.2


def test_triad_detected_without_octave_ghosts():
    sig = tone(60, 2.0) + tone(64, 2.0) + tone(67, 2.0)
    ev = an.transcribe(sig, SR)
    assert {60, 64, 67} <= found(ev)
    assert found(ev) <= {60, 64, 67, 48, 72, 76, 79}, found(ev)  # only octave-related extras


def test_low_and_high_register():
    ev = an.transcribe(tone(45, 1.5) + tone(84, 1.5), SR)
    assert {45, 84} <= found(ev)


def test_sequential_notes_have_ordered_onsets():
    sig = np.concatenate([tone(60, 1.0), tone(67, 1.0)])
    ev = an.transcribe(sig, SR)
    on = {e.midi: e.onset_s for e in ev if e.midi in (60, 67)}
    assert on[60] < 0.3 and on[67] > 0.8


def test_silence_has_no_events():
    assert an.transcribe(np.zeros(SR), SR) == []


def test_summarize_events_histogram_and_suggestion():
    sig = tone(60, 2.0) + tone(64, 2.0) + np.pad(tone(67, 1.0), (0, SR))
    ev = an.transcribe(sig, SR)
    s = an.summarize_events(ev, top=3)
    assert s["events"] >= 3
    assert abs(sum(s["histogram"].values()) - 1.0) < 1e-2
    assert set(s["suggested_notes"]) <= found(ev)
    assert s["register"]["p10"] <= s["register"]["p50"] <= s["register"]["p90"]
    assert an.summarize_events([])["suggested_notes"] == []
