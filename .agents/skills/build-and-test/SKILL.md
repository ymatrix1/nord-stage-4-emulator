---
name: build-and-test
description: Run all Rust, Python, and unit-test coverage ratchet checks required before pushing.
---

# Build and test

Run from the repository root:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-targets --all-features
uv run ruff check tools/
uv run ruff format --check tools/
uv run pytest
python tools/check_tests.py
```

All commands must exit with code `0` before committing or pushing.
