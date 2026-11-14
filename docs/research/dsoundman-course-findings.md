# DSoundman course: hardware behaviour findings

**Checked:** 2026-11-14
**Sources:** local, git-ignored `.reference/DSoundman-course/` (8 tutorial videos, 3840x2160, overhead camera, plus auto subtitles). Short names used below: PL = Preset Library video, BG = Beginners Guide, PP = Piano + Synth Pad layer, SP = Soft Piano, SG = Soft Grand, ORG = Organ Drawbars, LFO = Synth LFOs (Nord clip, 93 s). The Grand + String layer video has no subtitle file and was not analysed. Frames were extracted with ffmpeg into a scratch directory and read by eye; nothing from the videos is stored in git. Manual: OS v1.6x. The narrator says (SG 2:21) his unit runs an older OS (about 1.12), so display text may differ from v1.6x.
**Question:** Which hardware behaviours that the manual does not state can be read off the footage, and where does the GUI differ?

## Findings

| Behaviour | Evidence (video, time) | Ours (file) | Verdict | Action |
|---|---|---|---|---|
| Location label `E:85`, bank letter plus colon plus page and slot | PL 11:25, BG 17:10 | `ProgLoc::label` (`bay_program/store.rs`) | matches | none |
| Banks A..H exist (`H:13`, `G:82`, `E:85` seen) | PP 5:40, BG 19:35 | 8 banks (`store.rs`); manual p.12 states A-H | matches | bank count now VERIFIED |
| Unused location shows `<empty>` | BG 19:35 (`G:82`), PP 5:45 | was `(empty)` (`nav.rs`) | differed | fixed, test |
| Boxed `E` after the location once edited; cleared after STORE and on load | BG 17:10, PP 5:30 vs 6:20 and 6:30, ORG 5:30 | `bay_program/edited.rs` edit counter (`AudioHandle::edit_count`) | matches | fixed, tests `edited::tests`, `oled_home::tests` |
| Live Mode shows `Live 1` as the big title | BG 19:45 | `oled_home::location_title` | matches | fixed, test `live_mode_titles_the_slot_and_drops_the_mark` |
| Home rows: section icon, preset id `1:05` or source name plus size (`White Grand XL`, `Silver Grand M`), organ model plus mini drawbars | PL 11:25, BG 17:10, SG 8:40, ORG 5:30 | `oled_home::section_rows`: section name rows only | partly | names done; preset id, size letter, mini drawbars open (bean ns4-ow85 out of scope) |
| Piano dial changes the piano row in place (no overlay) | SG 8:38-8:55 | `piano_overlay.rs` LIST/INFO overlay | unknown (other lane) | note to piano lane |
| Preset screen titled `LOAD SYNTH PRESET`, id `1:12`, only `Cancel` soft button | PL 5:22 | `PIANO PRESET`, Num/Cat/Cancel (matches manual p.42) | differs from video, matches v1.6x manual | none; OS differs |
| Preset entry shows boxed `E` until the dial loads it | PL 5:22, manual p.42 | edited mark after any preset load through the browser (loading is an edit) | unknown | ASSUMED, same counter |
| Layer Init screen: `LAYER INIT`, `Press to initialize`, letters `AB / A / A`, soft `All Organ Piano Synth` | SP 1:16, PP 1:02 | text differed | differed | fixed, test |
| Layer Init All leaves only Piano A on (white grand) | PP 1:06, SP 1:28 | `engine/layer_init.rs` | matches | none |
| After Init the synth wave shows `Shape Saw`, shape 0.0 | PP 6:30 | `VaWaveform::Saw` | matches | none |
| `STORE PROGRAM TO`: location, name at target (`<empty>`), `As: <name>` | PP 5:40-5:45 | `Replaces:` row | differed | fixed, test |
| Name proposed as `NS4 Palette 2` | PP 5:40 | keeps current name | differs | bean ns4-qdzu |
| `STORE PROGRAM AS`: name line, `Cat: Piano Acoustic`, soft `Abc Cat Del Ins` | PP 5:47 | `ABC`, `Category:` | differed | label fixed; categories bean ns4-qdzu |
| ABC strip order: blank, a-z, A-Z, 0-9, `-` | PP 5:54-6:00 | upper case first | differed | fixed, test; tail after `-` stays ASSUMED |
| Second STORE press writes; program shows new location without `E` | PP 6:12-6:20 | `store.rs` | matches | none |
| LFO destination taps: Off, OSC CTRL, OSC PITCH, FILTER (both LEDs), then wraps | LFO 0:30-0:35 | cycled Filter, Pitch, Ctrl, Off | differed | fixed, test |
| Synth OLED top row shows hints (`LFO Rate 0.08 Hz`, `LFO Destination OscPitch`, `LFO Amount 7.1`) | LFO 0:27-0:45 | none | differs | bean ns4-wxly |
| LFO waveform: button opens page, first dial picks shape | LFO 1:05 narration | `SynthPage::LfoWave` | matches (text only) | none |
| Rotor stop ANGLE screen: `Free`, `1: Front Center`, `2: Front Left`, `4: Back Left`, `6: Back Center` | ORG 10:08-10:22 | none | differs | bean ns4-uo53 |
| STOP MODE stops rotors, SLOW/FAST switches speed when active | ORG 9:32-9:55 | `bay_master.rs`, `organ/rotary.rs` | matches | none |
| Percussion single-triggered, non-legato | ORG 6:50-7:03 | `organ/mod.rs` | matches | none |
| Preset LED on by default; Live turns ladders off; Shift+Preset syncs | ORG 7:52-9:15 | spec section 9 | matches | none |
| Six organ models, one selector: Farf, Vox, B3, Pipe1, Pipe2, B3 Bass | BG 12:46, panel frame | organ model list | matches (panel print) | none |
| Dynamic compression cycles off, 25, 50, 75 | BG 16:12-16:25 | `dyn_comp_cycle` (4 states) | matches | none |
| Layer off = hold the layer button; tap turns on; both together | BG 13:31-14:00 | hold gesture 500 ms | matches (gesture); timing unknown | none |
| Morph assign: hold WHEEL, move control; WHEEL LED turns green | PP 18:23, frames 1110-1118 s | `morph.rs` | matches in effect | none |
| Layer Scene II toggles layer mutes; no scene marker on the default display | SP 7:40-8:05 | Scene text only in Full view | unknown | none |
| Soft Release and String Res: video says one button cycles the options | BG 15:46 | two toggles (`piano_logic.rs`) | unknown (panel photo unclear) | none |

## What the videos could not answer

- Hold threshold (500 ms) and double-tap interval (300 ms): the footage does not make either measurable by eye at 30 fps; OQ13 stays open.
- Pot pick-up, LED ladder segment counts, LFO rate range or MST CLK division values: not shown beyond one 0.08 Hz reading.
- No audio measurement was done: the LFO clip has no playable audio worth fitting and the organ video mixes speech.

## Implications

- `Verified` now: bank letters A-H, `<empty>` name, Layer Init and Store screen texts, ABC strip order up to `-`, LFO destination tap order, boxed E meaning (also manual p.13).
- Display work: the E marker and `Live N` title are done (ns4-ow85); preset ids, size letters and mini drawbars in the section rows, hints and rotor angle stay open (ns4-wxly, ns4-uo53, ns4-qdzu).

## Follow-ups

- Re-check display texts against OS v1.6x footage; this footage may be OS 1.1x.
- Grab the full ABC strip tail, the Cat list and the six stop angles from further footage.
