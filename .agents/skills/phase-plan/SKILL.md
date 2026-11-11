---
name: phase-plan
description: Turn an approved ROADMAP phase into an epic bean with child beans and dependencies.
---

# Phase plan

1. Confirm the user approved the phase.
2. `beans create "Phase N: title" -t epic -s todo -d ...` with Context/Higher Goal/Acceptance Criteria from `docs/ROADMAP.md`.
3. One child bean per checkbox: `--parent <epic>`, `--blocked-by` for real dependencies only.
4. `beans list --ready` to show what can run in parallel. Report the tree to the user.
