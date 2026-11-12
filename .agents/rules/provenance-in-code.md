# Provenance in code

Every constant, table, curve, or coefficient in source must say where it came from. Put it in the doc comment next to the item, not only in `docs/`.

Tag each item with exactly one status:

| Tag | Meaning |
|---|---|
| `VERIFIED` | Matches firmware or a file byte-for-byte or within a stated tolerance. Name the source file, offset or address, the check used (test name or `tools/` command), and the measured error. |
| `EMPIRICAL` | Fitted or tuned by ear or against measurements/recordings. Name the reference (video, recording, manual page), what was compared, and the fit quality. |
| `ASSUMED` | Chosen without evidence (textbook default, placeholder, inherited from older code). Say why it is plausible and what would confirm it. |

Rules:

- Format: `/// VERIFIED: <what> = <source file>@<offset>, checked by <test or command>, max err <x>.`
- A `VERIFIED` tag needs a reproducible check. Prefer a unit test; if the source is a local-only file under `.reference/` or `target/fw_unpacked/`, cite the exact command or script and put the result in `docs/research/`.
- Do not upgrade a tag without new evidence. Code that only matches its own design formula (e.g. a biquad hitting its target gain) proves the implementation, not the hardware: tag the underlying parameter `ASSUMED`.
- When a constant is inherited from older code with no recorded source, tag it `ASSUMED` rather than leaving it untagged.
- New or touched constants in `crates/nord-core/src/` must carry a tag in the same commit.
- Mirror the status in the relevant spec (`docs/specs/`) and move unresolved items to `docs/OPEN-QUESTIONS.md`.
