#!/usr/bin/env python3
"""Every source file ships tests (AGENTS.md rule 3, .agents/rules/testing.md).

Per language, a source file is covered when:
- Rust (`*.rs` under `crates/**/src/`): the same file has a `#[test]`.
  Declaration-only files (`lib.rs`: modules, re-exports, cfg attributes) are exempt.
- Python (`tools/**/*.py`): a test file (`tools/tests/test_*.py`) imports the module.
"""

from __future__ import annotations

import os
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]

SKIP_DIRS = {
    ".git",
    ".claude",
    ".agents",
    ".beans",
    ".reference",
    "references",
    ".venv",
    "venv",
    "__pycache__",
    ".pytest_cache",
    ".ruff_cache",
    "target",
    "build",
    "dist",
}

RUST_TEST = re.compile(r"#\[test\]")
RUST_DECLARATION_ONLY = re.compile(
    r"^\s*(//.*|(pub(\(crate\))? )?mod \w+;|(pub(\(crate\))? )?use [\w:{}, ]+;|#\[cfg\(.*\)\])?\s*$"
)
GENERATED = re.compile(r"GENERATED|DO NOT EDIT|Code generated")


def walk(base: Path, suffix: str):
    if not base.exists():
        return
    for dirpath, dirnames, filenames in os.walk(base):
        dirnames[:] = [d for d in dirnames if d not in SKIP_DIRS]
        for name in filenames:
            if name.endswith(suffix):
                yield Path(dirpath) / name


def rel(path: Path, root: Path) -> str:
    return path.relative_to(root).as_posix()


def is_rust_declaration_only(text: str) -> bool:
    return all(RUST_DECLARATION_ONLY.match(line) for line in text.splitlines())


def is_generated(text: str) -> bool:
    return bool(GENERATED.search("\n".join(text.splitlines()[:3])))


def is_python_test(path: Path) -> bool:
    return "tests" in path.parts or path.name.startswith("test_")


def sources(root: Path, lang: str) -> list[Path]:
    out: list[Path] = []
    if lang == "rust":
        for path in walk(root / "crates", ".rs"):
            if "src" not in path.parts:
                continue
            text = path.read_text(encoding="utf-8")
            if is_rust_declaration_only(text):
                continue
            out.append(path)
    elif lang == "python":
        for path in walk(root / "tools", ".py"):
            if is_python_test(path) or path.name in {"__init__.py", "conftest.py"}:
                continue
            out.append(path)
    return [p for p in out if not is_generated(p.read_text(encoding="utf-8"))]


def test_texts(root: Path, lang: str) -> list[str]:
    if lang == "python":
        return [
            p.read_text(encoding="utf-8") for p in walk(root / "tools", ".py") if is_python_test(p)
        ]
    return []


def is_covered(path: Path, lang: str, texts: list[str]) -> bool:
    text = path.read_text(encoding="utf-8")
    if lang == "rust":
        return bool(RUST_TEST.search(text))
    stem = re.escape(path.stem)
    pattern = re.compile(rf"^\s*(from|import)\s+[\w.]*\b{stem}\b", re.MULTILINE)
    return any(pattern.search(t) for t in texts)


def audit(root: Path) -> dict[str, tuple[int, list[str]]]:
    result: dict[str, tuple[int, list[str]]] = {}
    for lang in ("rust", "python"):
        files = sources(root, lang)
        texts = test_texts(root, lang)
        uncovered = [rel(p, root) for p in files if not is_covered(p, lang, texts)]
        result[lang] = (len(files), sorted(uncovered))
    return result


def read_baseline(path: Path) -> set[str]:
    if not path.exists():
        return set()
    return {
        line.strip()
        for line in path.read_text(encoding="utf-8").splitlines()
        if line.strip() and not line.startswith("#")
    }


def compare(uncovered: set[str], baseline: set[str]) -> tuple[list[str], list[str]]:
    new = sorted(uncovered - baseline)
    stale = sorted(e for e in baseline - uncovered)
    return new, stale


def baseline_path(root: Path) -> Path:
    return root / "tools" / "test-baseline.txt"


def main(argv: list[str], root: Path = ROOT) -> int:
    result = audit(root)
    uncovered = {f for _, files in result.values() for f in files}
    baseline_file = baseline_path(root)
    if "--update-baseline" in argv:
        header = (
            "# Source files without a unit test (tools/check_tests.py).\n"
            "# This list may only shrink. Add a test, then delete the line.\n"
        )
        baseline_file.parent.mkdir(parents=True, exist_ok=True)
        baseline_file.write_text(
            header + "".join(f"{f}\n" for f in sorted(uncovered)), encoding="utf-8"
        )
        print(f"wrote {len(uncovered)} entries to {rel(baseline_file, root)}")
        return 0

    total = sum(n for n, _ in result.values())
    for lang, (n, files) in result.items():
        print(f"{lang:7} {n - len(files):4} / {n:4} covered, {len(files)} without a test")
    new, stale = compare(uncovered, read_baseline(baseline_file))
    if new:
        print(f"\n{len(new)} new file(s) without a unit test (add tests in the same change):")
        for f in new:
            print(f"  {f}")
    if stale:
        print(
            f"\n{len(stale)} baseline entr(y/ies) now covered or removed; delete them from "
            f"{rel(baseline_file, root)}:"
        )
        for f in stale:
            print(f"  {f}")
    if new or stale:
        return 1
    print(f"\nok: {total} source file(s) checked, no new untested files")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
