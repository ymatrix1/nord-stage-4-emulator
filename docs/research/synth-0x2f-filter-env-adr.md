# Synth `0x2F` filter envelope ADR location

**Checked:** 2026-11-11
**Sources:** `.reference/Nord Stage 4 Factory Restore RevK v1.66.ns4b` (384 `.ns4y` synth presets = 1152 `0x2F` Sound blocks, layers A/B/C); `tools/ns4_bitscan.py`.
**Question:** Where are the dedicated filter envelope Attack/Decay/Release fields inside the 384-bit `0x2F` Sound block?

## Method

```bash
python tools/ns4_bitscan.py ".reference/Nord Stage 4 Factory Restore RevK v1.66.ns4b" --hist 318:7:3 --hist 195:7:3
```

## Findings (status: EMPIRICAL)

| Field (7b, block-relative) | Most common values (of 1152) |
|---|---|
| `+195` amp attack | `0` x408, `10` x197 |
| `+202` amp decay | `127` (Hold) x975 (85%) |
| `+209` amp release | `0` x96, `40` x79 |
| `+318` filter attack | `0` x597, `19` x55, `24` x51 |
| `+325` filter decay | `67` x376, `127` (Hold) x82 (7%), `44` x62 |
| `+332` filter release | `0` x449, `22` x58 |

- `+318..+339` directly follows the 31-bit `filter_env_amt` morph (`+287..+318`), the same placement as amp ADR after `lfo_amount` (`+164..+195`).
- Decay is Hold in 85% of amp envelopes but only 7% of filter envelopes, as expected for a decaying filter sweep.

## Implications

- `presets.rs` decodes/packs `+318..+339` with the amp ADR tables (`ENV_*_MS_TABLE`). Offset: EMPIRICAL. Curve identity: ASSUMED.
- No firmware byte-for-byte or hardware timing check exists yet; do not upgrade to `VERIFIED` without one.

## Follow-ups

- Locate the SHARC/ARM envelope table used for the filter envelope and compare with `ENV_*_MS_TABLE`.
- Candidate 2-bit fields still unmapped: `+123`, `+127`, `+131`, `+216` (see `docs/OPEN-QUESTIONS.md`).
