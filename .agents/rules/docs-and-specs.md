# Docs and specs

- `docs/PRD.md` scope, `docs/ROADMAP.md` phases, `docs/ARCHITECTURE.md` architecture, `docs/specs/<topic>.md` subsystem design, `docs/DECISIONS.md` decisions, `docs/OPEN-QUESTIONS.md` unknowns, `docs/research/` findings.
- Behaviour, binary layout, DSP equation, or interface change → update the spec in `docs/specs/` in the same PR/commit.
- Every decision: **Context, Decision, Why, Alternatives, Impact**, **Status** (`proposed`/`accepted`/`superseded`/`rejected`), **Decided by** once decided.
- Answered open question → remove it from `docs/OPEN-QUESTIONS.md` and record the answer in `docs/DECISIONS.md` or the relevant spec.
- Research files start with: Checked date, Sources, Question; end with Implications and Follow-ups.
- Keep `docs/ROADMAP.md` checkboxes (`- [ ]` / `- [x]`) in sync with delivered code and tests.
- Write directly: tables, concrete equations, byte offsets, short sentences, no filler.
