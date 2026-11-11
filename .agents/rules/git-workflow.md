# Git workflow

- `main` is current state. Feature branches use `feat/`, `fix/`, `docs/`, `research/`, `chore/`, one concern each.
- Conventional Commits (`feat(organ): ...`, `docs(spec): ...`, `ci: ...`), subject ≤ 72 chars.
- Verify all checks locally before committing and pushing.
- Task / bean / PR descriptions use: **Context**, **Higher Goal**, **Acceptance Criteria**, **Out of Scope**, **Summary of Changes**.
- Never commit `.reference/` (large binary archives > 3 GB), `target/`, `.venv/`, or temporary build artifacts.
- GitHub account is `ymatrix1`.
