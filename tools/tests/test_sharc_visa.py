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
    assert d((0x687E, 0x143F)) == "R8 = DM(I4, 2)"


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


def test_shift_immediate_form_and_type_only_forms():
    # First byte 0x02: 48-bit "IF cond shift immediate" (identified with ADI elfdump, docs 5.34).
    assert v.classify(0x023E, 0x2019) == ("6n", 3)
    assert v.decode((0x023E, 0x2019, 0x1002)) == "R0 = R0 OR FDEP R2 BY 16:16"
    assert v.decode((0x023E, 0x4019, 0x1002)).startswith("R0 = R0 OR FDEP R2 BY ")
    assert v.decode((0x1629, 0x0000, 0x0012)) == "I4 = MODIFY(I1, 0x12)"


def test_multiplier_register_ops():
    assert v.mult_text(0x01, 3, 0, 0) == "R3 = SAT MRF (UI)"
    assert v.mult_text(0x14, 0, 0, 0) == "MRF = 0"
    assert v.mult_text(0x16, 0, 0, 0) == "MRB = 0"
    assert v.mult_text(0x1C, 2, 0, 0) == "MRF = RND MRF (SF)"


def test_16_bit_nop_and_entry_code_shape():
    assert v.classify(0x0001, 0x06BE) == ("21c", 1)
    assert v.decode((0x0001,)) == "NOP"
    assert v.classify(0x0000, 0x0000) == ("21/22", 3)  # 48-bit form stays
    # entry code of the FX Core1 section: BIT CLR; NOP; CALL; CALL; R0 = 0; I0 = 0xc0430
    stream = [0x1422, 0x0000, 0x1000, 0x0001, 0x06BE, 0x0012, 0x0528, 0x06BE, 0x0012, 0x05A0]
    stream += [0x0F80, 0x0000, 0x0F10, 0x000C, 0x0430]
    insns, exact = v.tile(stream)
    assert exact
    assert [v.decode(ps) for _, ps in insns] == [
        "BIT CLR MODE1 0x1000",
        "NOP",
        "CALL 0x120528",
        "CALL 0x1205a0",
        "R0 = 0x0",
        "I0 = 0xc0430",
    ]


def test_type_15b_register_and_signed_offset_fields():
    # UREG is bits 29..23, DATA7 is bits 22..16 as a signed offset (docs 5.28)
    assert v.decode((0x9E08, 0x0C00)) == "I8 = DM(0, I7)"
    assert v.decode((0x9208, 0x007A)) == "R0 = DM(-6, I1)"
    assert v.decode((0x9908, 0x0014)) == "DM(20, I4) = R0"


def test_shift_immediate_matches_the_adi_disassembly():
    # ground truth from ADI elfdump (CCES 2.12.1), docs 5.34
    assert v.decode((0x023E, 0x1010, 0x1810)) == "R1 = FEXT R0 BY 24:8"
    assert v.decode((0x023E, 0x3011, 0x0700)) == "R0 = FDEP R0 BY 7:24"
    assert v.decode((0x023E, 0x1012, 0x9614)) == "R1 = FEXT R4 BY 22:10 (SE)"
    assert v.decode((0x023E, 0x7801, 0xFF11)) == "R1 = ASHIFT R1 BY -1"
    assert v.decode((0x023E, 0x0032, 0x0044)) == "R4 = BTGL R4 BY 0"
    assert v.decode((0x023E, 0x0033, 0x0005)) == "BTST R5 BY 0"


def test_bit_fifo_forms_match_the_adi_disassembly():
    # ground truth from ADI elfdump (CCES 2.12.1), docs/research/sharc-bitfifo.md; register forms
    # read Ry (bits 3..0) for BITEXT and BFFWRP, immediates keep DATA12[5:0] / DATA12[6:0]
    assert v.compute_text(0x250312) == "R3 = BITEXT R2"
    assert v.compute_text(0x258312) == "R3 = BITEXT R2 (NU)"
    assert v.compute_text(0x270312) == "R3 = BFFWRP"
    assert v.compute_text(0x274312) == "BITDEP R1 BY R2"
    assert v.compute_text(0x27C312) == "BFFWRP = R2"
    assert v.decode((0x023E, 0x0014, 0x7F35)) == "R3 = BITEXT 63"
    assert v.decode((0x023E, 0x7816, 0xFF35)) == "R3 = BITEXT 63 (NU)"
    assert v.decode((0x023E, 0x081D, 0x23AC)) == "BITDEP R12 BY 35"
    assert v.decode((0x023E, 0x001D, 0x8035)) == "BITDEP R5 BY 0"
    assert v.decode((0x023E, 0x781F, 0xFF35)) == "BFFWRP = 127"
    assert v.decode((0x023E, 0x401F, 0x0035)) == "BFFWRP = 0"
    assert v.decode((0x013E, 0x0025, 0x0312)) == "R3 = BITEXT R2"
    assert v.decode((0x013E, 0x0027, 0xC312)) == "BFFWRP = R2"


def test_register_moves_and_swaps_match_the_adi_disassembly():
    # ground truth from ADI elfdump (CCES 2.12.1), docs 5.34
    assert v.decode((0x703E, 0x933F)) == "M6 = R1"
    assert v.decode((0x753E, 0x93BF)) == "M7 = S1"
    assert v.decode((0x7A3E, 0x04BF)) == "R9 <-> S8"
    assert v.decode((0x72BE, 0x0188, 0x1C00)) == "R3 = M8, F12 = F0 + F0"
    assert v.decode((0x7A3E, 0x0480, 0x1001)) == "R9 <-> S8, R0 = R0 + R1"


def test_type_4_signed_offset_and_empty_compute():
    # ground truth from ADI elfdump (CCES 2.12.1), docs 5.34
    assert v.decode((0x6A3F, 0x218D, 0xA00F)) == "R3 = DM(-28, I5), F0 = FLOAT R0 BY R15"
    assert v.decode((0x507E, 0x8000, 0x0000)) == "DM(I0, M1) = R0"


def test_dual_moves_immediate_stores_and_modify_match_the_adi_disassembly():
    # ground truth from ADI elfdump (CCES 2.12.1), docs 5.34
    assert v.decode((0x2C40, 0x9211, 0x4000)) == "R0 = DM(I6, M1), R4 = PM(I10, M10), MRF = 0"
    assert v.decode((0x2CC0, 0x921B, 0xC004)).startswith("R0 = DM(I6, M3), R4 = PM(I10, M10), MRF")
    assert v.decode((0x3D89, 0xC83F)) == "DM(I6, M6) = R4, R0 = PM(I15, M9)"
    assert v.decode((0x9040, 0x0000, 0x0000)) == "DM(I0, M1) = 0x0"
    assert v.decode((0x92C3, 0x3D0B, 0x22A7)) == "DM(I1, M3) = 0x3d0b22a7"
    assert v.decode((0x9BC4, 0x3E9D)) == "DM(I5, M7) = 0x3e9d"
    assert v.decode((0x043E, 0x583F)) == "I1 = MODIFY(I1, M3)"
    assert v.decode((0x043F, 0xD808, 0x1BAE)) == "I7 = MODIFY(I7, M3), F11 = F10 + F14"


def test_post_modify_modify_bank_and_loop_counts_match_the_adi_disassembly():
    # ground truth from ADI elfdump (CCES 2.12.1), docs 5.34
    assert v.decode((0x6C7E, 0x10BF)) == "R1 = DM(I6, 2)"
    assert v.decode((0x6CFE, 0x1008, 0x122E)) == "DM(I6, 2) = R0, F2 = F2 + F14"
    assert v.decode((0x043E, 0xE73F)) == "I4 = MODIFY(I3, M4)"
    assert v.decode((0x0D05, 0x0080, 0x0006)) == "DO (PC, 6) UNTIL LCE, LCNTR = R5"
    assert v.decode((0x0C00, 0x6480, 0x0003)) == "DO (PC, 3) UNTIL LCE, LCNTR = 100"
    assert v.decode((0x783E, 0x0080, 0x0000)) == "R1 <-> S0"


def test_nop_branch_return_and_immediate_forms_match_the_adi_disassembly():
    # ground truth from ADI elfdump (CCES 2.12.1), docs 5.34
    assert v.decode((0x0000, 0x0000, 0x0000)) == "NOP"
    assert v.decode((0x0003,)) == "NOP"
    assert v.decode((0x08BE, 0x003F)) == "CALL (M8, I8)"
    assert v.decode((0x083E, 0x043F)) == "JUMP (M8, I8) (DB)"
    assert v.decode((0x0AFE,)) == "RTS (DB)"
    assert v.decode((0x0ABE,)) == "RTS"
    assert v.decode((0x0F8F, 0xFFE1)) == "R15 = 0xffffffe1"
