"""Unit tests for tools/arm_events.py: event decoding and the encoder id map."""

from __future__ import annotations

import arm_events as ev
import arm_keys as ak


def test_an_encoder_entry_decodes_with_a_signed_delta() -> None:
    e = ev.decode_event((ev.TAG_ENCODER, 1, 6, 0xFFFFFFFE))
    assert (e.kind, e.ident, e.flags, e.delta) == ("encoder", 6, 1, -2)
    assert ev.decode_event((ev.TAG_ENCODER, 0, 7, 3)).delta == 3


def test_key_and_unknown_tags_are_told_apart() -> None:
    assert ev.decode_event((ev.TAG_KEY, 0, 0x13, 1)).kind == "key"
    assert ev.decode_event((0x1234, 0, 0, 0)).kind == "other"


def test_the_five_encoder_ids_map_to_the_five_pairs_both_ways() -> None:
    assert len(ev.ENCODER_PAIR_OF_ID) == len(ak.ENCODER_PAIRS) == 5
    for ident, pair in ev.ENCODER_PAIR_OF_ID.items():
        assert ev.ID_OF_ENCODER_PAIR[pair] == ident
        assert ev.encoder_pair_for(ident) == pair
    assert ev.encoder_pair_for(99) is None
    # The Program dial is pair 1 (index 6), PIANO SELECT is pair 0 (index 7).
    assert (ev.ID_OF_ENCODER_PAIR[1], ev.ID_OF_ENCODER_PAIR[0]) == (6, 7)


def test_the_queue_merges_deltas_and_wraps_its_ring() -> None:
    assert ev.merge_delta(1, 2) == 3 and ev.merge_delta(-1, -1) == -2
    assert ev.queue_depth(7, 8) == 1 and ev.queue_depth(8, 8) == 0
    assert ev.ring_slot(256, 0xFF) == 0 and ev.ring_slot(257, 0xFF) == 1
    assert ev.EVENT_BYTES == 64


def test_the_key_id_table_names_the_undo_dialog_and_the_two_popups() -> None:
    assert ev.KEY_IDS["undo"] == (21, 1)
    assert ev.KEY_IDS["master_clock"] == (16, 0) and ev.KEY_IDS["transpose"] == (17, 0)
    assert ev.KEY_IDS["program2"][0] == ev.KEY_IDS["program1"][0] + 1


def test_patch_key_entry_rewrites_only_the_matching_key_event() -> None:
    mem = {0: ev.TAG_KEY, 4: 0, 8: 146, 12: 1}
    assert ev.patch_key_entry(mem.__getitem__, mem.__setitem__, 0, 146, 21, 1)
    assert (mem[4], mem[8]) == (1, 21)
    assert not ev.patch_key_entry(mem.__getitem__, mem.__setitem__, 0, 146, 5, 0)
    other = {0: ev.TAG_ENCODER, 4: 0, 8: 146, 12: 1}
    assert not ev.patch_key_entry(other.__getitem__, other.__setitem__, 0, 146, 21, 1)
    assert other[8] == 146
