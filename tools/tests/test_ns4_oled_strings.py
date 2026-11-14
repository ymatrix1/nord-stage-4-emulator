from __future__ import annotations

import os
import re
import struct
from pathlib import Path

import ns4_oled_strings as os_
import pytest

ROOT = Path(__file__).resolve().parents[2]
NAME_RS = ROOT / "crates/nord-gui/src/panel/bay_program/name.rs"


def image(*strings: str, pad: int = 8) -> bytes:
    """A fake image: NUL-terminated strings, each 4-aligned, after a zero header."""
    out = bytearray(pad)
    for s in strings:
        out += s.encode() + b"\x00"
        while len(out) % 4:
            out += b"\x00"
    return bytes(out)


def pool_of(*strings: str) -> os_.Pool:
    return os_.Pool(os_.fw_strings(image(*strings)), window=(0, 1 << 30))


def test_fw_strings_reports_offsets_of_nul_terminated_runs() -> None:
    img = image("Hall", "Ab")
    assert os_.fw_strings(img, min_len=2) == [(8, "Hall"), (16, "Ab")]
    assert os_.fw_strings(img, min_len=3) == [(8, "Hall")]  # "Ab" is shorter than min_len


def test_classify_exact_case_norm_affix_frag_missing() -> None:
    pool = pool_of("Memory Protect", "OUTPUT OVERVIEW", "MIDI CC #1", "Fine Tune", "Cathedral")
    assert os_.classify("Memory Protect", pool).status == "exact"
    v = os_.classify("Output Overview", pool)
    assert (v.status, v.fw_text) == ("case", "OUTPUT OVERVIEW")
    assert os_.classify("MIDI CC#1", pool).status == "norm"
    assert os_.classify("Fine Tune (cents)", pool).status == "affix"
    assert os_.classify("Cathedral Plus Extra", pool).status == "affix"
    assert os_.classify("Quantum Flux", pool).status == "missing"


def test_format_strings_compare_their_fixed_words_only() -> None:
    pool = pool_of("Category", "Layer")
    assert os_.classify("Category: {cat}", pool).status == "frag"
    assert os_.classify("{:+} st", pool).status == "missing"
    assert os_.classify("Unknown {}", pool).status == "missing"


def test_short_strings_are_weak() -> None:
    pool = pool_of("On", "Hall")
    assert os_.classify("On", pool).weak
    assert not os_.classify("Hall", pool).weak


def test_offsets_come_back_for_non_exact_matches() -> None:
    pool = pool_of("filler", "Fine Tune")
    v = os_.classify("Fine Tune (cents)", pool)
    assert (v.fw_text, v.offset) == ("Fine Tune", 16)


def test_ui_text_rejects_code_noise() -> None:
    assert os_.ui_text("Hall Chorale") and os_.ui_text("1/4T") and os_.ui_text("A--")
    assert not os_.ui_text("\\OO]PQ^RO_TO`SO"[:6] + "@@")
    assert not os_.ui_text("@a") and not os_.ui_text("..")


def test_displayable_drops_ids_paths_and_prose() -> None:
    assert os_.displayable("Memory Protect")
    assert not os_.displayable("system.1.memory_protect")
    assert not os_.displayable("engine:extern_out.layers.bank_msb")
    assert not os_.displayable("this is a long sentence. It goes on and on forever")
    assert not os_.displayable("123")


def test_surface_rules_split_menus_oled_tables_and_legends() -> None:
    f = os_.surface_of
    assert f("crates/nord-panel/src/menus/system.rs") == "menu"
    assert f("crates/nord-panel/src/inventory/organ.rs") == "legend"
    assert f("crates/nord-gui/src/panel/bay_program/oled.rs") == "oled"
    assert f("crates/nord-gui/src/panel/bay_synth/oled_view.rs") == "oled"
    assert f("crates/nord-gui/src/panel/ui_state/synth/tables.rs") == "table"
    assert f("crates/nord-plugin/src/fx_selectors.rs") == "table"
    assert f("crates/nord-gui/src/panel/bay_organ.rs") == "gui"


def test_strip_tests_cuts_the_test_module() -> None:
    src = 'const A: &str = "Real";\n#[cfg(test)]\nmod tests {\n    const B: &str = "Fake";\n}\n'
    assert "Fake" not in os_.strip_tests(src)
    assert "Real" in os_.strip_tests(src)


def test_our_literals_reads_the_repo_menus() -> None:
    lits = os_.our_literals(ROOT)
    texts = {lit.text for lit in lits if lit.surface == "menu"}
    assert {"Memory Protect", "Global Transpose", "Fine Tune"} <= texts
    assert all(lit.line >= 1 for lit in lits)


def test_pointer_table_resolves_absolute_pointers() -> None:
    img = bytearray(image("Bass", "Choir"))
    base = len(img)
    img += struct.pack("<III", os_.LOAD_BASE + 8, os_.LOAD_BASE + 16, 0x1234)
    assert os_.pointer_table(bytes(img), base, 3) == ["Bass", "Choir", None]


def test_counts_and_markdown_group_by_surface() -> None:
    pool = pool_of("Hall", "Fine Tune")
    rows = os_.check(
        pool,
        [
            os_.Literal("crates/nord-panel/src/menus/a.rs", 3, "Hall"),
            os_.Literal("crates/nord-panel/src/menus/a.rs", 4, "Fine Tune (cents)"),
            os_.Literal("crates/nord-panel/src/menus/a.rs", 5, "Nope Nope"),
            os_.Literal("crates/nord-gui/src/panel/bay_organ.rs", 9, "Hall"),
        ],
    )
    menu = os_.counts(rows, "menu")
    assert (menu["exact"], menu["affix"], menu["missing"]) == (1, 1, 1)
    md = os_.markdown(rows, ("affix", "missing"), ("menu",))
    assert "| `Fine Tune (cents)` | `Fine Tune` | `0x000010` | affix |" in md
    assert "Nope Nope" in md and "Hall" not in md


def test_main_reports_missing_image(capsys: pytest.CaptureFixture[str]) -> None:
    assert os_.main(["check", str(ROOT / "no-such-firmware.bin")]) == 2
    assert "not found" in capsys.readouterr().err


def test_main_check_and_dump_on_a_synthetic_image(
    tmp_path: Path, capsys: pytest.CaptureFixture[str]
) -> None:
    fw = tmp_path / "fw.bin"
    fw.write_bytes(image("Memory Protect", "Global Transpose"))
    assert os_.main(["dump", str(fw), "--min-len", "3"]) == 0
    assert "Memory Protect" in capsys.readouterr().out
    csv_path = tmp_path / "out.csv"
    assert os_.main(["check", str(fw), "--csv", str(csv_path), "--md"]) == 0
    out = capsys.readouterr().out
    assert "menu" in out and csv_path.read_text(encoding="utf-8").startswith("surface,status")


# --- local firmware (skipped without it) ---------------------------------------------------------


def firmware() -> Path | None:
    candidates = [
        os.environ.get("NS4_BANK0_CODE", ""),
        str(ROOT / "target/fw_unpacked/bank0_CODE.bin"),
    ]
    for c in candidates:
        if c and Path(c).is_file():
            return Path(c)
    return None


needs_fw = pytest.mark.skipif(firmware() is None, reason="local bank0_CODE.bin not present")

# Lower-case enum tokens of the pedal / MIDI source lists (firmware `ctrl`, `swell`, ...) that our
# menus keep in title case; the firmware copies live outside the menu string pool.
CASE_ALLOWED = {"Aftertouch", "Ctrl", "Swell", "Sustain", "Sostenuto"}


@needs_fw
def test_program_category_table_matches_our_names() -> None:
    fw = firmware()
    assert fw is not None
    table = os_.pointer_table(fw.read_bytes(), 0x2BF3C8, 52)
    src = NAME_RS.read_text(encoding="utf-8")

    def rust_list(name: str) -> list[str]:
        body = re.search(rf"pub const {name}: \[&str; \d+\] = \[(.*?)\];", src, re.S)
        assert body is not None
        return re.findall(r'"([^"]*)"', body.group(1))

    assert rust_list("CATEGORIES") == table[0::2]


@needs_fw
def test_menu_surface_has_no_unexpected_case_or_missing_titles() -> None:
    fw = firmware()
    assert fw is not None
    pool = os_.Pool(os_.fw_strings(fw.read_bytes()))
    rows = os_.check(pool, os_.our_literals(ROOT))
    bad = [
        (lit.text, v.fw_text)
        for lit, v in rows
        if lit.surface == "menu" and v.status == "case" and lit.text not in CASE_ALLOWED
    ]
    assert bad == []


@needs_fw
def test_fixed_wording_is_in_the_firmware() -> None:
    fw = firmware()
    assert fw is not None
    pool = os_.Pool(os_.fw_strings(fw.read_bytes()))
    for text in (
        "OUTPUT OVERVIEW",
        "PEDAL OVERVIEW",
        "MIDI OVERVIEW",
        "Reset all settings",
        "Nothing to Undo",
        "Nothing to Paste",
        "Sub Square Saw",
        "Roland EV7",
        "Sustain+Rotor Hold",
        "Organ Keyboard Trig",
        "B3 Organ Tone-Wheel",
        "ModWheel MIDI",
    ):
        assert text in pool.exact, text
