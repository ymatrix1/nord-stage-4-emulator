# Piano model types

Checked: 2026-11-13

Sources:
- `Nord Stage 4 Factory Restore RevK v1.66.ns4b` (local, `.reference/`), 38 `.npno` entries.
- Nord Stage 4 User Manual v1.6X, p.24-26 (TYPE, PIANO SELECT, LIST/INFO, Timbre).
- `crates/nord-core/src/cbin/piano_preset.rs` (`.ns4p` piano layer `category`, `model_id`, `name_hash`).

Question: where does the type (Grand, Upright, Electric, Clav, Digital, Misc) of a piano model come from, so that TYPE and the PIANO SELECT dial can behave like the hardware?

## Findings

| Item | Where | Evidence |
|---|---|---|
| Type of a model | `.npno` CBIN header `bank_index`, `u16` LE at offset `0x0C` (Grand 0, Upright 1, Electric 2, Clav 3, Digital 4, Misc 5) | VERIFIED: `python tools/npno_types.py "<backup>.ns4b"` prints `38 npno files; header type vs folder mismatches: 0` |
| Position of a model inside its type | `.npno` CBIN header `location_index`, `u16` LE at offset `0x0E` | VERIFIED: per type the values are `0..N-1` without gaps (Grand 8, Upright 7, Electric 11, Clav 4, Digital 6, Misc 2) |
| Archive folder | `Piano/<Grand\|Upright\|Electric\|Clavinet\|Digital\|Misc>/` | VERIFIED: equals the header type for all 38 entries; used as a fallback for entries whose header cannot be read |
| Type in a program | `.ns4p` piano layer `category` (3 bits) | VERIFIED earlier (`piano_preset.rs`); same index order as the header |

Hardware order inside a type (header `location_index`): Grand White Grand XL, Royal Grand 3D XL, Velvet Grand, Astoria Grand, Silver Grand, Studio Grand 2, Bright Grand, EGrand 3; Electric EP1..EP9, Wurlitzer 1, Wurlitzer 2; Clav Clavinet D6, Ital Harpsich 1B Long, Ital Harpsich 1D Lute, French Harpsi 1D; Digital DX7 FullTines, DX RubbaTines, DigiGrand 1, DigiGrand 2, Ballad EP1, Hybrid EP; Misc Marimba, Vibraphone; Upright Amber, Felt, Pearl, Bambino, Mellow, Romantic, Black. This is not alphabetical, so the GUI sorts by `(type, location_index)`.

No name or hash heuristic is needed. A `.npno` outside the backup (dropped file) carries the same header; only a file with an unreadable header falls back to the folder name, then to `Misc`.

## Implications

- The GUI library lists models as `(entry, type)` in hardware order from 16 header bytes per entry (no sample data is read).
- Timbre sets depend on the type (manual p.26): Electric has tailored Soft/Mid/Bright and Dyno 1/2; Clav has seven pick-up EQ combinations (UI only for now).

## Follow-ups

- Electric Soft/Mid/Bright gains and the 2.2 kHz bell are ASSUMED; confirm with a hardware sweep.
- Engine Clav pick-up EQ timbre (seven combinations) is not implemented.
- The Nord Piano Library website lists third-party/library models not in the factory backup; their types would come from their own `.npno` header, not verified here (no such file available).
