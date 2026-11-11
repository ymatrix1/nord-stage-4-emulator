# Testing

- **Every code change ships tests in the same commit/PR.** New or changed behaviour (Rust or Python) gets a unit test that fails without the change. Docs-only changes are exempt.
- A bug fix starts with a test that reproduces the bug.
- No `#[ignore]`, `pytest.mark.skip`, or deleted tests to get green.
- CI runs every suite listed in `AGENTS.md` ("Checks before pushing") and fails on any warning or test failure.
- **Enforced in CI by `tools/check_tests.py`** (job "Every source file has a unit test"). Per language, a source file is covered when:
  - Rust (`*.rs` under `crates/**/src/`): the same file has a `#[test]`. Declaration-only files (`lib.rs`: modules, re-exports, cfg attributes) and binary entrypoints (`src/bin/*.rs`) are exempt if their logic lives in tested library modules, or they include `#[test]` directly.
  - Python (`tools/**/*.py`): a test file (`tools/tests/test_*.py`) imports the module.
- **Ratchet:** `tools/test-baseline.txt` tracks any legacy uncovered files (currently **0**). The check fails on any new untested file or stale baseline entry.
