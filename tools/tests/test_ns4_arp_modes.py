from __future__ import annotations

import ns4_arp_modes as am
import ns4_bitscan as bs


def _block(**values: int) -> str:
    bits = ["0"] * bs.LAYER_BITS
    for name, value in values.items():
        start, width = am.FIELDS[name]
        bits[start : start + width] = list(f"{value:0{width}b}")
    return "".join(bits)


def _payload(blocks: dict[int, str]) -> bytes:
    bits = bs.to_bits(bytes(bs.SYNTH_PAYLOAD_LEN))
    for layer, block in blocks.items():
        base = bs.LAYER_BASES[layer]
        bits = bits[:base] + block + bits[base + bs.LAYER_BITS :]
    return int(bits, 2).to_bytes(bs.SYNTH_PAYLOAD_LEN, "big")


def test_decode_modes_reads_every_field_at_its_offset():
    f = am.decode_modes(
        _block(run=1, mode=2, kb_sync=1, range=100, direction=3, zigzag=1, mst_clk=1, rate=67)
    )
    assert f == {
        "run": 1,
        "mode": 2,
        "pattern_on": 0,
        "kb_sync": 1,
        "range": 100,
        "direction": 3,
        "zigzag": 1,
        "mst_clk": 1,
        "rate": 67,
    }


def test_field_offsets_match_the_rust_decoder_and_the_documented_neighbours():
    # Run is 135 and the rate 175 (already used by cbin/synth_preset.rs); the mode fields fit
    # between them without overlapping the pattern enable at 138.
    assert am.FIELDS["run"] == (135, 1)
    assert am.FIELDS["rate"] == (175, 7)
    spans = sorted((s, s + w, n) for n, (s, w) in am.FIELDS.items())
    for (_, end, _), (start, _, _) in zip(spans, spans[1:], strict=False):
        assert end <= start


def test_scan_walks_the_layers_of_a_payload_and_flags_unnamed_modes():
    rows = am.scan({"p": _payload({0: _block(mode=1), 2: _block(mode=3)})})
    assert [(r[1], r[2]["mode"]) for r in rows] == [("A", 1), ("B", 0), ("C", 3)]
    assert am.violations(rows) == ["p C: mode 3 has no name"]


def test_summary_counts_values():
    rows = am.scan({"p": _payload({0: _block(mode=2, run=1, range=0), 1: _block(range=127)})})
    text = "\n".join(am.summary(rows))
    assert "3 layers" in text
    assert "Gate: 1" in text
    assert "range      2 distinct, min 0, max 127" in text


def test_main_reports_error_for_missing_path(tmp_path, capsys):
    assert am.main([str(tmp_path / "missing" / "x.ns4b")]) == 1
    assert "error" in capsys.readouterr().err
