from __future__ import annotations

from pathlib import Path

import check_tests


def test_repo_has_zero_uncovered_files():
    result = check_tests.audit(check_tests.ROOT)
    for lang, (total, uncovered) in result.items():
        assert total > 0, f"expected {lang} source files"
        assert uncovered == [], f"uncovered {lang} files: {uncovered}"


def test_detects_untested_rust_and_python_files(tmp_path: Path):
    crate_src = tmp_path / "crates" / "demo" / "src"
    crate_src.mkdir(parents=True)
    (crate_src / "lib.rs").write_text("pub mod foo;\n", encoding="utf-8")
    (crate_src / "foo.rs").write_text(
        "pub fn add(a: i32, b: i32) -> i32 { a + b }\n", encoding="utf-8"
    )

    tools_dir = tmp_path / "tools"
    tools_dir.mkdir(parents=True)
    (tools_dir / "helper.py").write_text("def run(): return 1\n", encoding="utf-8")
    (tools_dir / "test-baseline.txt").write_text("", encoding="utf-8")

    res = check_tests.audit(tmp_path)
    assert res["rust"] == (1, ["crates/demo/src/foo.rs"])
    assert res["python"] == (1, ["tools/helper.py"])
    assert check_tests.main([], root=tmp_path) == 1
