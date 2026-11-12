import os
import struct
import subprocess
import sys

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

import adi_dis as a  # noqa: E402

# Output of ADI `elfdump -ns seg_pmco` (CCES 2.12.1) for the parcels in PARCELS, copied verbatim.
ELFDUMP_OUTPUT = """
---- Section: seg_pmco [3] ----

00000000   023e 1010 1810   r1=fext r0 by 0x18:0x8;
00000003   0f8f ffe1   r15=0xffffffe1;
00000005   0001   nop;
"""
PARCELS = [0x023E, 0x1010, 0x1810, 0x0F8F, 0xFFE1, 0x0001]


def sections(elf: bytes) -> dict[str, tuple[int, bytes]]:
    """Parse the section table of an ELF32 object: name -> (type, contents)."""
    (shoff,) = struct.unpack_from("<I", elf, 0x20)
    shnum, shstrndx = struct.unpack_from("<2H", elf, 0x30)
    headers = [struct.unpack_from("<10I", elf, shoff + 40 * i) for i in range(shnum)]
    strtab = headers[shstrndx]
    names = elf[strtab[4] : strtab[4] + strtab[5]]
    result = {}
    for name, typ, _flags, _addr, off, size, *_ in headers[1:]:
        end = names.index(b"\0", name)
        result[names[name:end].decode()] = (typ, elf[off : off + size])
    return result


def test_elf_header_and_sections():
    elf = a.build_elf(PARCELS)
    assert elf[:4] == b"\x7fELF" and elf[4] == 1 and elf[5] == 1  # 32-bit little-endian
    assert struct.unpack_from("<HH", elf, 16) == (1, a.EM_SHARC)  # relocatable, SHARC
    s = sections(elf)
    assert set(s) == {".strtab", ".symtab", "seg_pmco", ".align.seg_pmco", ".adi.attributes"}
    assert s["seg_pmco"][0] == a.SHT_PROGBITS
    assert s[".align.seg_pmco"] == (a.SHT_ADI_ALIGN, a.ALIGN_RECORD)
    assert s[".adi.attributes"] == (a.SHT_ADI_ATTR, a.ADI_ATTRIBUTES)
    assert b"ADSP-21469" in a.ADI_ATTRIBUTES and len(a.ADI_ATTRIBUTES) == 49


def test_code_section_holds_big_endian_parcels_in_order():
    code = sections(a.build_elf(PARCELS))["seg_pmco"][1]
    assert code == bytes.fromhex("023e101018100f8fffe10001")
    # the section is 4-byte aligned in the file whatever the code length
    assert len(sections(a.build_elf([0x0001]))["seg_pmco"][1]) == 2


def test_parse_listing_reads_addresses_parcels_and_text():
    listing = a.parse_listing(ELFDUMP_OUTPUT)
    assert listing == {
        0: ([0x023E, 0x1010, 0x1810], "r1=fext r0 by 0x18:0x8"),
        3: ([0x0F8F, 0xFFE1], "r15=0xffffffe1"),
        5: ([0x0001], "nop"),
    }


def test_parse_listing_accepts_unspaced_words_and_ignores_other_lines():
    text = "garbage\n00000000   023e10101810   r1=fext r0 by 0x18:0x8;\n---- x ----\n"
    assert a.parse_listing(text) == {0: ([0x023E, 0x1010, 0x1810], "r1=fext r0 by 0x18:0x8")}
    assert a.parse_listing("") == {}


def test_image_parcels_are_little_endian_and_cut_to_whole_parcels():
    image = bytes([0, 0, 0x3E, 0x02, 0x10, 0x10, 0xFF])
    assert a.image_parcels(image, 2, 7) == [0x023E, 0x1010]
    assert a.image_parcels(image, 0, 2) == [0]


def test_disassemble_runs_elfdump_on_the_built_object(monkeypatch, tmp_path):
    seen = {}

    def fake_run(cmd, capture_output, text, check):
        seen["cmd"] = cmd
        seen["elf"] = open(cmd[-1], "rb").read()
        return subprocess.CompletedProcess(cmd, 0, stdout=ELFDUMP_OUTPUT, stderr="")

    monkeypatch.setattr(a.subprocess, "run", fake_run)
    listing = a.disassemble(PARCELS, cces=str(tmp_path))
    assert seen["cmd"][0].endswith("elfdump.exe") and str(tmp_path) in seen["cmd"][0]
    assert seen["cmd"][1:3] == ["-ns", "seg_pmco"]
    assert seen["elf"] == a.build_elf(PARCELS)
    assert listing[0] == ([0x023E, 0x1010, 0x1810], "r1=fext r0 by 0x18:0x8")


def test_the_cces_folder_comes_from_the_environment(monkeypatch):
    paths = []
    monkeypatch.setattr(
        a.subprocess,
        "run",
        lambda cmd, **kw: paths.append(cmd[0]) or subprocess.CompletedProcess(cmd, 0, "", ""),
    )
    monkeypatch.setenv("NS4_CCES", "/opt/cces")
    a.disassemble([1])
    monkeypatch.delenv("NS4_CCES")
    a.disassemble([1])
    assert paths[0].replace("\\", "/").startswith("/opt/cces")
    assert paths[1].replace("\\", "/").startswith(a.DEFAULT_CCES)


def test_main_prints_one_line_per_instruction(monkeypatch, tmp_path, capsys):
    image = tmp_path / "img.bin"
    image.write_bytes(b"".join(struct.pack("<H", p) for p in PARCELS))
    monkeypatch.setattr(
        a, "disassemble", lambda parcels, cces=None: a.parse_listing(ELFDUMP_OUTPUT)
    )
    monkeypatch.setattr(sys, "argv", ["adi_dis.py", str(image), "0", "12"])
    assert a.main() == 0
    lines = capsys.readouterr().out.splitlines()
    assert lines[0] == "00000000  023e 1010 1810  r1=fext r0 by 0x18:0x8"
    assert lines[-1] == "00000005  0001  nop"


def test_isa_objects_omit_the_visa_align_record():
    """`visa=False` builds the object ADI decodes as fixed 48-bit ISA words (SY1SY2/SMP kernel)."""
    s = sections(a.build_elf(PARCELS, visa=False))
    assert set(s) == {".strtab", ".symtab", "seg_pmco", ".adi.attributes"}
    assert s["seg_pmco"][1] == sections(a.build_elf(PARCELS))["seg_pmco"][1]


def test_isa_parcels_put_the_most_significant_parcel_first():
    # `call 0x8c091 (db)` = 06be 0408 c091, stored least significant parcel first
    stored = bytes.fromhex("91c00804be06")
    assert a.isa_parcels(stored, 0, 6) == [0x06BE, 0x0408, 0xC091]
    assert a.isa_parcels(stored, 0, 5) == []


def test_main_isa_flag_uses_the_isa_object(monkeypatch, tmp_path, capsys):
    image = tmp_path / "k.bin"
    image.write_bytes(bytes.fromhex("91c00804be06"))
    seen = {}

    def fake(parcels, cces=None, visa=True):
        seen.update(parcels=parcels, visa=visa)
        return {0: (parcels, "call 0x8c091 (db)")}

    monkeypatch.setattr(a, "disassemble", fake)
    monkeypatch.setattr(sys, "argv", ["adi_dis.py", str(image), "0", "6", "--isa"])
    assert a.main() == 0
    assert seen == {"parcels": [0x06BE, 0x0408, 0xC091], "visa": False}
    assert "call 0x8c091 (db)" in capsys.readouterr().out
