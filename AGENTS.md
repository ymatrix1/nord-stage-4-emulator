# AGENTS.md

Canonical instructions for every AI coding agent (Claude Code, Google Antigravity / `agy`, Codex, OpenCode). Tool-specific files only point here.

## Project

**Nord Stage 4 Emulator** (`nord-stage-4-emulator`) is an open-source software emulator, DSP engine, and Clavia Binary (`CBIN` / `.ns4b` / `.ns4p`) interoperability toolkit for the Nord Stage 4 hardware keyboard (`product_id=46`), built for **interoperability, research, preservation, and compatibility**.

**Status:** Phase 0 (specs & reverse engineering) and Phase 1–5 core architecture (`crates/nord-core` + `tools/`) implemented and verified.

## Sources of truth

| Topic | Doc |
|---|---|
| Goals, scope, budgets | [docs/PRD.md](docs/PRD.md) |
| Phases & parallel lanes | [docs/ROADMAP.md](docs/ROADMAP.md) |
| Architecture & crates | [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) |
| Subsystem specs | [docs/specs/README.md](docs/specs/README.md) |
| Decisions | [docs/DECISIONS.md](docs/DECISIONS.md) |
| Open questions | [docs/OPEN-QUESTIONS.md](docs/OPEN-QUESTIONS.md) |
| Technical debt | [docs/TECH_DEBT.md](docs/TECH_DEBT.md) |
| Glossary | [docs/GLOSSARY.md](docs/GLOSSARY.md) |
| Reverse-engineering findings | [docs/research/](docs/research/) |

If code and docs disagree, raise it; don't silently pick one.

## Rules

Detailed rules live in `.agents/rules/`:

- [.agents/rules/docs-and-specs.md](.agents/rules/docs-and-specs.md): keeping specs, decisions, and roadmap checkboxes in sync.
- [.agents/rules/evidence-over-claims.md](.agents/rules/evidence-over-claims.md): never claim work is done without test/build output.
- [.agents/rules/testing.md](.agents/rules/testing.md): every source file ships unit tests enforced by `tools/check_tests.py`.
- [.agents/rules/dsp-and-realtime.md](.agents/rules/dsp-and-realtime.md): real-time audio invariants (no allocations/locks in `process_block`).
- [.agents/rules/provenance-in-code.md](.agents/rules/provenance-in-code.md): tag constants `VERIFIED` / `EMPIRICAL` / `ASSUMED` with source and check.
- [.agents/rules/git-workflow.md](.agents/rules/git-workflow.md): branches, Conventional Commits, PR structure, and pushing.

Short version:

1. **Docs & specs first.** A change to behaviour, binary layout, DSP signal flow, or public API updates its spec in `docs/specs/` in the same commit/PR. Decisions record **Context, Decision, Why, Alternatives, Impact**, plus **Status**.
2. **Evidence over claims.** Don't claim work is done without running the build, linter, and test suite after the last edit and showing the output.
3. **Every code change ships unit tests** in the same commit/PR; `python tools/check_tests.py` enforces that every `.rs` and `.py` source file has unit tests (`tools/test-baseline.txt` stays at 0 uncovered files).
4. **Real-time DSP safety.** Audio synthesis (`organ.rs`, `piano.rs`, `synth.rs`, `fx.rs`, `engine.rs`) never allocates on the heap, locks a mutex, or performs I/O inside `process_block()` / `render_frame()`.
5. **Lossless `CBIN` round-tripping.** When parsing and re-serializing `.ns4p`, `.ns4y`, `.ns4o`, `.ns4n`, `.ns4l`, or `.ns4t` files, preserve the 48-byte `CBIN` header and raw bitstream payload so unedited fields remain bit-identical.
6. **Keep `.reference/` git-ignored.** Large local reference dumps (`.ns4b` is 3.14 GB, updater binaries, PDFs) live in `.reference/` for local inspection and stay out of git. Unit tests use deterministic synthetic fixtures in memory.
7. **Standardize task plans, beans, and PR descriptions** with: **Context**, **Higher Goal**, **Acceptance Criteria** (`- [ ]` / `- [x]`), **Out of Scope**, **Summary of Changes**.
8. **Keep `docs/ROADMAP.md` checkboxes in sync** with delivered scope in the same commit/PR.

## Task tracking (beans)

Tasks live in `.beans/` ([hmans/beans](https://github.com/hmans/beans), CLI `beans`, config `.beans.yml`, prefix `ns4-`). Run `beans list` at session start. Use beans, not ad-hoc todo lists.

- **Before work:** find or create the bean (`beans list --ready`; `beans create "Title" -t task -s in-progress --parent <epic> -d ...`). Body follows rule 7.
- **When completing work:** tick criteria, add `## Summary of Changes`, `beans update <id> -s completed`, `beans archive`.
- **Dependencies:** `--blocked-by <id>`, `--parent <epic>`.
- Commit `.beans/` files together with the change they track.

## Skills

| Skill | Use |
|---|---|
| `start-task` | Bean, branch/worktree, structured plan |
| `phase-plan` | Turn a ROADMAP phase into an epic + child beans |
| `update-spec` | Update subsystem specs and verify acceptance criteria |
| `build-and-test` | Run full Rust + Python + test-ratchet quality gates |

Skills live in `.agents/skills/`; `.claude/skills` points there.

## Layout

`crates/nord-core/` Rust DSP & `CBIN`/firmware parser + `ns4` CLI · `crates/nord-sharc/` SHARC VISA decoder (Track B oracle) · `tools/` Python inspection CLI (`ns4_inspect.py`) & test ratchet (`check_tests.py`) · `docs/` PRD, ROADMAP, ARCHITECTURE, DECISIONS, `specs/`, `research/` · `.agents/` rules & skills · `.beans/` tracked tasks

## Checks before pushing

Run all of these before committing/pushing (mirrors `.github/workflows/ci.yml`):

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-targets --all-features
uv run ruff check tools/
uv run ruff format --check tools/
uv run pytest
python tools/check_tests.py
```

## Landing the Plane (Session Completion)

**When ending a work session**, you MUST complete ALL steps below. Work is NOT complete until `git push` succeeds:

1. **Run quality gates** — All checks in "Checks before pushing" pass.
2. **Update docs & beans** — Tick completed `docs/ROADMAP.md` and `.beans/` checkboxes.
3. **Commit & push to remote**:
   ```bash
   git pull --rebase
   git push
   git status  # MUST show "up to date with origin"
   ```
