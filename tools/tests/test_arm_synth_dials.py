"""Unit tests for tools/arm_synth_dials.py: pair map and the page walk."""

from __future__ import annotations

import arm_events as ev
import arm_synth_dials as sd


def test_the_three_dials_are_pairs_two_to_four_with_event_ids_15_14_13() -> None:
    assert sorted(sd.SYNTH_DIAL_PAIRS) == [2, 3, 4]
    assert [ev.ID_OF_ENCODER_PAIR[p] for p in sorted(sd.SYNTH_DIAL_PAIRS)] == [15, 14, 13]
    assert sd.SYNTH_PAGE_KEY == (0x01, 1, 6)


def test_dial_summary_drops_icon_and_blank_strings() -> None:
    assert sd.dial_summary(["FM Harmonic A", "5", "", " ", "G", "Bell"]) == [
        "FM Harmonic A",
        "Bell",
    ]


class _Session:
    def __init__(self) -> None:
        self.calls: list[tuple] = []
        self.emu = self

    def write32(self, address: int, value: int) -> None:
        self.calls.append(("clear", address, value))

    def press(self, keys, hold=40, **_kw):  # noqa: ANN001, ANN003, ANN201
        self.calls.append(("press", tuple(keys)))

    def turn(self, pair, detents, **_kw):  # noqa: ANN001, ANN003, ANN201
        import arm_screens as sc

        self.calls.append(("turn", pair, detents))
        text = sc.TextEvent(0, 0, False, f"p{pair}d{detents}")
        return sc.Capture(([], []), [text])


def test_capture_dials_opens_the_synth_page_then_turns_each_dial_both_ways() -> None:
    session = _Session()
    result = sd.capture_dials(session)
    assert session.calls[0] == ("press", (sd.SYNTH_PAGE_KEY,))
    turns = [c[1:] for c in session.calls if c[0] == "turn"]
    assert turns == [(2, 3), (2, -3), (3, 3), (3, -3), (4, 3), (4, -3)]
    assert result["left"] == {"up": ["p2d3"], "down": ["p2d-3"]}
    assert set(result) == {"left", "middle", "right"}
