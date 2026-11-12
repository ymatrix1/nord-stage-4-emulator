import os
import sys

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

import sharc_visa as v  # noqa: E402

# Bytes of bank0_CODE (OS v1.64) at 0x45D630, two bytes after the record header.
RECORD = bytes.fromhex(
    "1a72bf00fe6abf10a40f70003e043fe77e683f10150f0c0040047e688a100010861400002000"
)


def test_classify_known_forms():
    assert v.classify(0x721A, 0x003F) == ("5b", 2)  # 011 1 0 ..., marker
    assert v.classify(0x1900 | 0x01, None) == ("25", 3)
    assert v.classify(0xC123, None) == ("2c", 1)
    assert v.classify(0x0FA4, 0x006C) == ("17b", 2)
    assert v.classify(0x0F15, 0x000C) == ("17a", 3)
    assert v.classify(0x1486, 0x0000) == ("18a", 3)


def test_record_tiles_exactly_only_from_the_right_phase():
    par = v.parcels_le(RECORD)
    insns, exact = v.tile(par)
    assert exact
    assert [n for n, _ in insns][:3] == ["5b", "4b", "17b"]
    header_phase = bytes.fromhex("2000") + RECORD[:-2]  # start at the record header
    _, exact_shifted = v.tile(v.parcels_le(header_phase))
    assert not exact_shifted


def test_ureg_load_reads_immediates():
    par = v.parcels_le(RECORD)
    insns, _ = v.tile(par)
    loads = [v.ureg_load(ps) for n, ps in insns if n in ("17a", "17b")]
    assert (0x24, 0x70) in loads  # M4 = 0x70 (0x74 in v1.62)
    assert (0x15, 0x000C0440) in loads  # I5 = 0xC0440


def test_main_prints_and_reports(tmp_path, capsys):
    f = tmp_path / "x.bin"
    f.write_bytes(RECORD)
    assert v.main([str(f), "0", str(len(RECORD))]) == 0
    out = capsys.readouterr().out
    assert "17b" in out and "tiles exactly" in out


def test_decode_known_instructions():
    d = v.decode
    assert d((0x0FA4, 0x0070)) == "M4 = 0x70"
    assert d((0x0F15, 0x000C, 0x0440)) == "I5 = 0xc0440"
    assert d((0x1486, 0x0000, 0x0020)) == "BIT TST ASTATx 0x20"
    assert d((0x6A3E, 0x060A, 0x1880)) == "R12 = DM(0, I5), F8 = PASS F8"
    assert d((0x687E, 0x143F)) == "R8 = DM(2, I4)"


def test_names_and_fields():
    assert v.ureg_name(0x24) == "M4"
    assert v.ureg_name(0x62) == "?"
    assert v.ureg_name(0x73) == "MMASK"
    assert v.bits(0xABCD00000000, 47, 40) == 0xAB
    assert v.compute_text(0) == "NOP"
    assert v.compute_text(0x0A1880) == "F8 = PASS F8"


def test_sweep_finds_index_register_loads():
    load = bytes.fromhex("150f0c003804")  # I5 = 0xC0438
    pad = bytes(0x400)
    data = pad + load * 3 + pad[len(load) * 3 :] + pad
    runs = v.sweep(data, 0, len(data), window=0x400)
    assert len(runs) == 1 and runs[0][0] == 0x400 and runs[0][2] == 3
    assert v.sweep(pad * 4, 0, 0x1000) == []


def test_main_sweep(tmp_path, capsys):
    f = tmp_path / "x.bin"
    f.write_bytes(bytes(0x800) + bytes.fromhex("150f0c003804") + bytes(0x800))
    assert v.main([str(f), "0", "0x1000", "--sweep"]) == 0
    assert "index-register loads" in capsys.readouterr().out


def test_decode_added_forms():
    d = v.decode
    assert d((0x1000, 0x0001, 0x06BE)) == "R0 = DM(0x106be)"  # Type 14a read
    assert d((0x1103, 0x000C, 0x02C0)) == "DM(0xc02c0) = R3"  # Type 14a write
    assert d((0xC000 | 0x0F34,)) == "F3 = F3 * F4"  # Type 2c short compute
    assert v.short_compute_text(0x0804) == "F0 = F0 + F4"
    assert v.short_compute_text(0x0428) == "R2 = NOT R8"


def test_compute_units():
    assert v.mult_text(0x30, 1, 2, 3) == "F1 = F2 * F3"
    assert v.mult_text(0x78, 0, 1, 2) == "R0 = R1 * R2 (SSF)"
    assert v.mult_text(0x00, 0, 1, 2) == "R0 = SAT MRF (SI)"
    assert v.mult_text(0x10, 0, 1, 2) is None  # 0001 0000 is not in Table 12-5
    assert v.compute_text((0b0000111 << 16) | 0x4321) == "R4 = R2 - R1, R3 = R2 + R1"
    assert v.compute_text(0x4E4321) == "R4 = MRF + R0 * R6 (SSFR), R3 = (R8 + R13)/2"
    assert v.compute_text(0x4F0000).startswith("MULTI?")  # opcode 001111 is not in Table 12-12


def test_shifter_and_multifunction_compute():
    assert v.compute_text(0x200312) == "R3 = LSHIFT R1 BY R2"
    assert v.compute_text(0x445666) == "R5 = R1 * R6 (SSFR), R6 = R9 + R14"
    assert v.compute_text(0x635666) == "R5 = R1 * R6, R6 = R9 + R14, R3 = R9 - R14"
    assert v.compute_text(0x585666) == "F5 = F1 * F6, F6 = F9 + F14"
    assert v.multi_text(0x0) is None  # bit 22 clear: not a multifunction field


def test_undocumented_0x02_form_and_type_only_forms():
    # First byte 0x02: 48-bit "IF cond compute" with extra bits 31..23 (see docs 5.9).
    assert v.classify(0x023E, 0x2019) == ("2x", 3)
    assert v.decode((0x023E, 0x2019, 0x1002)).startswith("R0 = MRF + R0 * R2 (SUIR)")
    assert "extra bits" in v.decode((0x023E, 0x4019, 0x1002))
    assert v.decode((0x1629, 0x0000, 0x0012)) == "I4 = MODIFY(I1, 0x12)"


def test_multiplier_register_ops():
    assert v.mult_text(0x01, 3, 0, 0) == "R3 = SAT MRF (UI)"
    assert v.mult_text(0x14, 0, 0, 0) == "MRF = 0"
    assert v.mult_text(0x16, 0, 0, 0) == "MRB = 0"
    assert v.mult_text(0x1C, 2, 0, 0) == "MRF = RND MRF (SF)"
