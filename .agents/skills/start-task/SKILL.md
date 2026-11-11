---
name: start-task
description: Start any Nord Stage 4 Emulator task. Finds or creates the bean, reads the relevant specs, and verifies the baseline test suite.
---

# Start a task

1. `git fetch origin`.
2. Read the `docs/ROADMAP.md` phase and every spec in `docs/specs/` the task touches; check `docs/DECISIONS.md` and `docs/OPEN-QUESTIONS.md`.
3. Run `beans list`. Create if missing: `beans create "Title" -t task -s in-progress --parent <epic> -d ...`; else `beans update <id> -s in-progress`.
4. Bean body: **Context**, **Higher Goal**, **Acceptance Criteria** (`- [ ]`), **Out of Scope**, **Summary of Changes**.
5. Run the test suite (`cargo test --workspace` and `uv run pytest`) before changing anything to confirm a clean baseline.
