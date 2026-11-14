from __future__ import annotations

import ns4_arp_patterns as ap
import ns4_bitscan as bs


def _block(on: bool, steps: int, gate: int, accent: int = 0, pan: int = 0) -> str:
    bits = ["0"] * bs.LAYER_BITS
    bits[ap.PATTERN_ON] = "1" if on else "0"

    def put(spec: tuple[int, int], value: int) -> None:
        start, width = spec
        bits[start : start + width] = list(f"{value:0{width}b}")

    put(ap.LENGTH, steps - 2)
    put(ap.GATE, gate)
    put(ap.ACCENT, accent)
    put(ap.PAN, pan)
    return "".join(bits)


def test_digits_reads_step_zero_from_the_low_pair():
    assert ap.digits(0b11_00_00, ap.GATE_CHARS, 3) == "EE."


def test_decode_pattern_off_returns_none():
    assert ap.decode_pattern(_block(False, 8, 0)) is None


def test_decode_pattern_tresillo_from_factory_word():
    # 0x0003cf3c is the 9-step gate word of the factory programs Perpetuum and Rudiment Seq.
    pattern = ap.decode_pattern(_block(True, 9, 0x0003CF3C))
    assert pattern == {
        "steps": 9,
        "gate": "E..E..E..",
        "accent": ".........",
        "pan": "CCCCCCCCC",
    }


def test_decode_pattern_accent_and_pan():
    pattern = ap.decode_pattern(_block(True, 4, 0, accent=0b01, pan=0b0110_00))
    assert pattern["accent"] == ">..."
    assert pattern["pan"] == "CRL" + "C"


def test_scan_walks_layers_of_a_payload():
    block_on = _block(True, 4, 0xFC)
    payload = bytearray(bs.SYNTH_PAYLOAD_LEN)
    bits = bs.to_bits(bytes(payload))
    base = bs.LAYER_BASES[1]
    bits = bits[:base] + block_on + bits[base + bs.LAYER_BITS :]
    raw = int(bits, 2).to_bytes(bs.SYNTH_PAYLOAD_LEN, "big")
    rows = ap.scan({"x": raw})
    assert [(r[0], r[1], r[2]["gate"]) for r in rows] == [("x", "B", "E...")]


def test_main_reports_error_for_missing_path(tmp_path, capsys):
    assert ap.main([str(tmp_path / "missing" / "x.ns4b")]) == 1
    assert "error" in capsys.readouterr().err
