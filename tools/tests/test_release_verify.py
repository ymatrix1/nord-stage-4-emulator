import struct

import pytest
import release_package as rp
import release_verify as rv
from test_release_package import LIN, MAC, WIN, make_tree

README_OK = "This plugin has NOT been verified in any DAW yet."


def fake_pe(names):
    """Minimal PE32+ DLL image whose export table lists `names`."""
    raw, vaddr = 0x400, 0x1000
    image = bytearray(raw + 0x400)
    image[0:2] = b"MZ"
    struct.pack_into("<I", image, 0x3C, 0x40)
    image[0x40:0x44] = b"PE\0\0"
    struct.pack_into("<H", image, 0x40 + 6, 1)  # sections
    struct.pack_into("<H", image, 0x40 + 20, 240)  # optional header size
    opt = 0x40 + 24
    struct.pack_into("<H", image, opt, 0x20B)
    struct.pack_into("<I", image, opt + 112, vaddr)  # export directory RVA
    sec = opt + 240
    struct.pack_into("<IIII", image, sec + 8, 0x400, vaddr, 0x400, raw)
    exp = raw
    struct.pack_into("<I", image, exp + 24, len(names))
    struct.pack_into("<I", image, exp + 32, vaddr + 0x100)  # names array RVA
    text = 0x200
    for i, name in enumerate(names):
        struct.pack_into("<I", image, raw + 0x100 + 4 * i, vaddr + text)
        image[raw + text : raw + text + len(name) + 1] = name.encode() + b"\0"
        text += len(name) + 1
    return bytes(image)


def build(tmp_path, platform, *, clap=None, vst3=None, readme=README_OK, extra=None):
    root = make_tree(tmp_path, platform)
    (root / "docs" / "RELEASE_README.txt").write_text(readme)
    bundled = root / "target" / "bundled"
    if platform.os == "macos":
        clap_path = bundled / "nord-plugin.clap" / "Contents" / "MacOS" / "nord-plugin"
        for plist in ("nord-plugin.clap", "nord-plugin.vst3"):
            (bundled / plist / "Contents" / "Info.plist").write_text("<plist/>")
    else:
        clap_path = bundled / "nord-plugin.clap"
    clap_path.write_bytes(clap if clap is not None else b"\0clap_entry\0")
    (bundled / "nord-plugin.vst3" / platform.vst3_binary).write_bytes(
        vst3 if vst3 is not None else b"\0GetPluginFactory\0"
    )
    if extra:
        for rel, data in extra.items():
            (root / rel).parent.mkdir(parents=True, exist_ok=True)
            (root / rel).write_bytes(data)
    return rp.package(platform.triple, "v1.2.3", tmp_path / "out", root)


def test_pe_exports_parses_names():
    assert rv.pe_exports(fake_pe(["clap_entry", "other"])) == {"clap_entry", "other"}


def test_pe_exports_empty_when_no_table():
    image = bytearray(fake_pe(["x"]))
    struct.pack_into("<I", image, 0x40 + 24 + 112, 0)
    assert rv.pe_exports(bytes(image)) == set()


def test_pe_exports_rejects_non_pe():
    with pytest.raises(ValueError):
        rv.pe_exports(b"ELF....")


def test_has_symbol_elf_style_requires_whole_name():
    assert rv.has_symbol(b"\0clap_entry\0", "clap_entry")
    assert rv.has_symbol(b"\0_GetPluginFactory\0", "GetPluginFactory")
    assert not rv.has_symbol(b"\0my_clap_entry_x\0", "clap_entry")
    assert not rv.has_symbol(b"nothing here", "clap_entry")


def test_windows_archive_ok(tmp_path):
    archive = build(
        tmp_path, WIN, clap=fake_pe(["clap_entry"]), vst3=fake_pe(["GetPluginFactory", "InitDll"])
    )
    assert rv.verify(archive) == []
    assert rv.main([str(archive)]) == 0


def test_linux_and_macos_archives_ok(tmp_path):
    assert rv.verify(build(tmp_path / "l", LIN)) == []
    assert rv.verify(build(tmp_path / "m", MAC)) == []


def test_windows_missing_export_detected(tmp_path):
    archive = build(tmp_path, WIN, clap=fake_pe(["wrong"]), vst3=fake_pe(["GetPluginFactory"]))
    problems = rv.verify(archive)
    assert problems == ["CLAP module does not export clap_entry"]


def test_non_pe_windows_module_reported(tmp_path):
    archive = build(tmp_path, WIN, clap=fake_pe(["clap_entry"]), vst3=b"MZ\0\0")
    assert any("VST3 module unreadable" in p for p in rv.verify(archive))


def test_missing_vst3_binary_detected(tmp_path):
    archive = build(tmp_path, WIN, clap=fake_pe(["clap_entry"]), vst3=fake_pe(["GetPluginFactory"]))
    files = rv.read_archive(archive)
    paths = [p for p in files if not p.endswith("x86_64-win/nord-plugin.vst3")]
    problems = rv.check_layout(paths, WIN)
    assert problems == ["missing nord-plugin.vst3/Contents/x86_64-win/nord-plugin.vst3"]


def test_forbidden_paths_and_types_detected():
    stem = "p"
    base = [f"{stem}/{rel}" for rel in rv.expected_files(LIN)]
    extra = [f"{stem}/.reference/x.txt", f"{stem}/presets/a.ns4p", f"{stem}/fw/os.bin"]
    problems = rv.check_layout(base + extra, LIN)
    assert len(problems) == 3
    assert any(".reference" in p for p in problems)
    assert any("a.ns4p" in p for p in problems)
    assert any("os.bin" in p for p in problems)


def test_multiple_top_folders_detected():
    problems = rv.check_layout(["a/x", "b/y"], LIN)
    assert "one top-level folder" in problems[0]


def test_backslash_paths_detected():
    paths = [f"p/{rel}" for rel in rv.expected_files(LIN)] + ["p/dir\\file"]
    assert any("backslash" in p for p in rv.check_layout(paths, LIN))


def test_manufacturer_name_in_binary_detected(tmp_path):
    archive = build(tmp_path, LIN, clap=b"\0clap_entry\0 Clavia ")
    assert any("manufacturer name" in p for p in rv.verify(archive))


def test_readme_must_disclaim_daw_verification(tmp_path):
    archive = build(tmp_path, LIN, readme="all good")
    assert "README.txt must state the plugin is not DAW-verified" in rv.verify(archive)


def test_sha256_mismatch_and_crlf_detected(tmp_path):
    archive = build(tmp_path, LIN)
    sidecar = archive.with_name(archive.name + ".sha256")
    good = sidecar.read_bytes()
    sidecar.write_bytes(good.replace(b"\n", b"\r\n"))
    assert any("CR line endings" in p for p in rv.check_sha256(archive))
    sidecar.write_bytes(b"0" * 64 + b"  " + archive.name.encode() + b"\n")
    assert any("does not match" in p for p in rv.check_sha256(archive))
    sidecar.unlink()
    assert rv.check_sha256(archive) == [f"missing {sidecar.name}"]


def test_detect_platform_from_name_and_unknown(tmp_path):
    assert rv.detect_platform(tmp_path / "x-v1-macos-arm64.tar.gz") is MAC
    assert rv.detect_platform(tmp_path / "x", "x86_64-pc-windows-msvc") is WIN
    with pytest.raises(ValueError):
        rv.detect_platform(tmp_path / "unknown.zip")


def test_main_reports_problems(tmp_path, capsys):
    archive = build(tmp_path, LIN, clap=b"nothing")
    assert rv.main([str(archive)]) == 1
    assert "does not export clap_entry" in capsys.readouterr().err


def test_main_unreadable_archive(tmp_path, capsys):
    bad = tmp_path / "nord-stage-4-emulator-v1-linux-x86_64.tar.gz"
    bad.write_bytes(b"not gzip")
    assert rv.main([str(bad)]) == 1
