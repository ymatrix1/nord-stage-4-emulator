from __future__ import annotations

import ns4_arp_gate as g


def test_modelled_notes_short_and_rest_steps():
    # E = half a step (4 eighths) at the first slot of its step; a rest plays nothing.
    assert g.modelled_notes("E.E") == [(0, 4), (4, 4)]


def test_modelled_notes_q_is_a_full_step():
    assert g.modelled_notes("q.") == [(0, 8)]
    assert g.modelled_notes("EEq") == [(0, 4), (2, 4), (4, 8)]


def test_modelled_notes_q_ties_into_the_next_step_without_a_new_note():
    # Q + E = one note of 8 + 4 eighths; Q Q E = 8 + 8 + 4; Q + q = 8 + 8; Q + . = 8.
    assert g.modelled_notes("QE") == [(0, 12)]
    assert g.modelled_notes("QQE") == [(0, 20)]
    assert g.modelled_notes("Qq.") == [(0, 16)]
    assert g.modelled_notes("Q.E") == [(0, 8), (4, 4)]


def test_modelled_notes_tie_stops_at_the_pattern_end():
    assert g.modelled_notes("EQ") == [(0, 4), (2, 8)]


def test_notes_from_records_reads_on_hold_and_off():
    # Documented firmware records: E E = 0 3 0 3, Q E = 0 2 2 3, q . = 0 2 3 2.
    assert g.notes_from_records([0, 3, 0, 3]) == [(0, 4), (2, 4)]
    assert g.notes_from_records([0, 2, 2, 3]) == [(0, 12)]
    assert g.notes_from_records([0, 2, 3, 2]) == [(0, 8)]


def test_notes_from_records_new_note_on_ends_the_previous_one_and_open_notes_run_to_the_end():
    # q E: the second note starts (type 1) without a note-off between them.
    assert g.notes_from_records([0, 2, 1, 3]) == [(0, 8), (2, 4)]
    # Last step q: held to the end of the pattern.
    assert g.notes_from_records([3, 2, 0, 2]) == [(2, 8)]


def test_rule_covers_every_gate_code():
    assert set(g.STEP_RULE) == set("Eq Q.".replace(" ", ""))
    assert g.SLOT_EIGHTHS * g.SLOTS_PER_STEP == 8


def test_check_gates_reports_only_disagreements(monkeypatch):
    # Records of "E." (firmware: 0 3 3 2) agree with the rule; a wrong record list does not.
    good = [(0, 0, 0, 0), (3, 0, 0, 0), (3, 0, 0, 0), (2, 0, 0, 0)]
    monkeypatch.setattr(g, "compile_slots", lambda _e, _g, _p=None: good)
    rows = [{"preset": 1, "gate": "E.", "pan": "CC"}]
    assert g.check_gates(object(), rows) == []
    bad = [(0, 0, 0, 0), (2, 0, 0, 0), (2, 0, 0, 0), (2, 0, 0, 0)]
    monkeypatch.setattr(g, "compile_slots", lambda _e, _g, _p=None: bad)
    (problem,) = g.check_gates(object(), rows)
    assert problem.startswith("preset 1 E.")
