---
# ns4-ow85
title: 'Program display fidelity: E marker, Live N, section rows with preset ids and size letters'
status: todo
type: task
created_at: 2026-11-14T03:34:07Z
updated_at: 2026-11-14T03:34:07Z
---

## Context
Hardware (DSoundman videos) shows a boxed E after the location when a program is edited (cleared by STORE or loading), Live N instead of the location in Live Mode, and per-section rows with preset id (1:05) or source name plus size letter (XL, M) and mini drawbars for the organ. ProgramUi has no dirty flag and bays do not report edits. See docs/research/dsoundman-course-findings.md.

## Higher Goal
Match hardware display behaviour.

## Acceptance Criteria
- [ ] Behaviour implemented with tests
- [ ] Spec and provenance tags updated

## Out of Scope
Engine changes unrelated to the display.
