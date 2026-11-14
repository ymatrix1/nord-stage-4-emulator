# MIDI Implementation & Control Surface Specification

## 1. Scope

Specifies the MIDI Control Change (`CC`), Bank Select, Program Change, and Aftertouch mapping for the Nord Stage 4 Emulator (`NordStage4` / `MIDIOutKeyboard` in firmware).

Implemented in [`crates/nord-core/src/midi.rs`](../../crates/nord-core/src/midi.rs) and [`crates/nord-core/src/engine/`](../../crates/nord-core/src/engine/mod.rs).

---

## 2. Channel Modes

MIDI channels are represented internally as `0..15` and displayed to users as channels `1..16`.

1. **Global Channel (Default Ch 1; `Off` supported):** Receives notes through the active Layer Scene and keybed split router. Global control messages are processed only on this channel. Setting it to `Off` disables global-channel input.
2. **Layer Channels (Default `Off` per layer):** Each Organ, Piano, and Synth layer can receive notes on its own assigned channel (`1..16` or `Off`). Layer-channel notes target that layer directly and bypass keybed split assignments. The target layer must be active in the current Layer Scene. `Off` disables that layer's direct MIDI input. If a channel is assigned to multiple layers, each matching active layer receives the note.
3. **Channel ownership:** Note Off is matched to the same MIDI channel and the layer recipients recorded for Note On. This prevents a release on one channel from releasing another channel's held pitch.

These channel roles follow the [Nord Stage 4 User Manual, OS v1.6x](https://www.nordkeyboards.com/wt/documents/951/Nord%20Stage%204%20User%20Manual%20v1.6X-Edition-N.pdf), MIDI chapter. The hardware also provides per-section `Sect On` settings, a separate Aux KB channel, MIDI receive filters, and NRPN support; those controls are outside this channel-routing change.

In this engine, incoming CC and channel aftertouch affect global controls only when received on the configured Global Channel. Layer-channel CC/NRPN receive filters are not modeled. Aux KB routing is described in section 2a.

## 2a. Aux KB, pedals, pitch stick and Extern (Phase 6b B9d, `ns4-ymci`)

Source: manual OS v1.6x p.21-22, p.26-28, p.43-47, p.55-56, p.59-64. Code: `engine/aux_kb.rs`, `engine/pedals.rs`, `engine/extern_out.rs`, `midi.rs` (`MidiEvent::PitchBend`).

| Area | Behaviour | Status |
|---|---|---|
| Aux KB | `AuxKb { enabled, channel (default 16), layers[7], curve }`. A layer in Aux KB mode ignores the Global channel (internal keybed) and answers only the Aux KB channel, bypassing keyboard zones. Direct layer channels still win. | Manual p.43-45 |
| Aux curve | Soft/Medium/Hard: identity for Medium, power law 0.7 / 1.4 otherwise | `ASSUMED` exponents |
| SUSTPED | Per layer. Default On for Piano A/B only (the previous hard-wired behaviour), Off elsewhere. Organ and Synth layers defer note release while the pedal is down (`held` bitmask, no allocation); Pianos use their damper model | Manual p.18, p.28 |
| Aux sustain/bend | `CC64` and pitch bend on the Aux KB channel reach Aux layers regardless of SUSTPED/PSTICK; the internal pedal needs the flag | Manual p.45 |
| Half pedaling | Sustain value `>= 64` = down. Between 20 % and 50 % depth the Pianos use the slow soft-release decay (`PianoLayer::damper_half`) | Manual p.26; band and mapping `ASSUMED` |
| Sostenuto, soft | `CC66`, `CC67` on the Global channel, Piano layers (unchanged) | Manual p.27 |
| Control pedal | `CC11` is the Morph source; `Ctrl + Swell` also drives Swell; gain 1-10 | Manual p.60 |
| Swell | `CC4` scales the Organ layers. B3 floor 0.1 (-20 dB, `ASSUMED`); other models reach 0. Function `Volume` scales the master mix, `Wheel Morph` mirrors the mod wheel | Manual p.22, p.60 |
| Rotor pedal, foot switch | `rotor_pedal(down)` and `foot_switch(down)`; Toggle/Hold; foot switch can switch Layer Scene or the rotor; Sustain+Rotor Hold/Toggle on the sustain jack. No MIDI CC exists for these jacks in the manual, so they are API/host-parameter inputs only | Manual p.53, p.61 |
| Pitch stick | `MidiEvent::PitchBend` (14 bit). PSTICK per layer with range 1-12 semitones (default 2). Applied to Synth layers through `SynthLayer::bend_cents` (exactly neutral at centre). Organ and Piano bend is not implemented | Manual p.28 |
| Extern mode | A Synth layer with `extern_out.layers[i].enabled` emits MIDI instead of audio. Notes are transposed by the octave shift, limited by the keyboard zones (crossfade gain scales velocity) and the sent pitch is remembered so Note Off always matches. CC VAL 1/2 (`extern_set_cc_value`), Bank Select + Program Change (`extern_set_program`), Send On Load, CC7 from the layer fader (`extern_sync_volume`, run once per `process_block`), CC64 and pitch bend forwarded when the layer's SUSTPED/PSTICK are on | Manual p.46-47, p.63-64 |
| Output queue | `MidiOutQueue`: fixed 256-entry ring buffer; backends drain with `pop_midi_out()`; overflow drops the newest event and counts it (`dropped()`). No allocation on the audio path | `ASSUMED` capacity |
| Channel helpers | `set_global_channel`, `set_layer_channel`, `set_aux_channel` take panel numbers 1-16 or `None` (Off); `route_for(layer, channel)` resolves `NoteRoute::{Layer, Aux, Global}` | Manual p.62 |

Not modelled: Echo Global To Extern (the emulator has no separate internal keybed, so Global notes always reach Extern layers inside their zone), Extern device filters (mod wheel, aftertouch, control/swell/volume pedal forwarding), Extern keyboard velocity curve, Piano/Organ pitch bend, Triple Pedal left/middle assignments (Pump, Mst Clk Tap, Program Up/Down), the plugin MIDI output port for Extern.

---

## 3. Nord Stage 4 MIDI CC Mapping Table

| CC # | Parameter | Range / Encoding | Section |
|---:|---|---|---|
| `0` | Bank Select MSB | `0` | Global |
| `1` | **Modulation Wheel (Morph Source 1)** | `0..127` | Morph / Global |
| `4` | Organ Swell Pedal | `0..127` | Organ |
| `7` | Master / Program Level | `0..127` | Global |
| `11` | **Control / Expression Pedal (Morph Source 2)** | `0..127` | Morph / Global |
| `16`..`24` | Organ Drawbars 1–9 (`16'` .. `1'`) | `0..127` (quantized `0..8`) | Organ |
| `25` | Organ Model (`B3`, `Vox`, `Farf`, `Pipe1`, `Pipe2`) | `0..4` | Organ |
| `26` | Organ Vibrato / Chorus Mode (`V1..C3`) | `0..5` | Organ |
| `27` | Organ Vibrato Enable | `0` = Off, `127` = On | Organ |
| `28` | Organ Percussion Enable | `0` = Off, `127` = On | Organ |
| `29` | Organ Percussion Harmonic (`2nd` / `3rd`) | `0` = 2nd, `127` = 3rd | Organ |
| `30` | Organ Percussion Decay (`Slow` / `Fast`) | `0` = Slow, `127` = Fast | Organ |
| `31` | Organ Percussion Volume (`Normal` / `Soft`) | `0` = Normal, `127` = Soft | Organ |
| `32` | Bank Select LSB (`Bank A..H` = `0..7`) | `0..7` | Global |
| `40` | Piano Model Type (`Grand`..`Misc`) | `0..5` | Piano |
| `42` | Piano Timbre (`Off`, `Soft`, `Mid`, `Bright`, `Dyno1`, `Dyno2`) | `0..5` | Piano |
| `44` | Piano String Resonance Enable | `0` = Off, `127` = On | Piano |
| `45` | Piano Pedal Noise Enable | `0` = Off, `127` = On | Piano |
| `50` | Synth Osc Mode (`Analog`, `Wavetable`, `FM`, `Sample`) | `0..3` | Synth |
| `51` | Synth Osc Shape / PWM / FM Amount | `0..127` | Synth |
| `59` | Synth Filter Type (`LP12`, `LP24`, `Ladder24`, `BP`, `HP`, `LP+HP`) | `0..5` | Synth |
| `60` | Synth Filter Cutoff Frequency | `0..127` | Synth |
| `61` | Synth Filter Resonance | `0..127` | Synth |
| `64` | **Damper / Sustain Pedal** | `0..127` (continuous or switch) | Global |
| `66` | Sostenuto Pedal | `0` / `127` | Global |
| `67` | Soft / Una Corda Pedal | `0` / `127` | Global |
| `71` | Synth Amp Envelope Attack | `0..127` | Synth |
| `72` | Synth Amp Envelope Release | `0..127` | Synth |
| `73` | Synth Filter Envelope Attack | `0..127` | Synth |
| `74` | Synth Filter Envelope Decay | `0..127` | Synth |
| `82` | Rotary Speaker Speed (`Slow` / `Fast`) | `0` = Slow, `127` = Fast | FX / Rotary |
| `85` | **Layer Scene II Toggle** | `0` = Scene I, `127` = Scene II | Global |
| `91` | Layer Reverb Wet Amount | `0..127` | FX |
| `94` | Layer Delay Feedback / Mix | `0..127` | FX |
| `100`..`106` | Individual Layer Levels (`Org A/B`, `Pno A/B`, `Syn A/B/C`) | `0..127` | Mixer |
| `115` | Master Compressor Amount | `0..127` | Master |
| `120` | All Sound Off | `0` | Channel Mode |
| `123` | All Notes Off | `0` | Channel Mode |

---

## 4. Audio Host and MIDI Event Delivery Specification

Implemented in [`crates/nord-gui/src/audio.rs`](../../crates/nord-gui/src/audio.rs).

### 4.1 Audio Output Formats & Channel Layout
- **Sample Formats:**
  - `F32` (normalized float `-1.0..1.0`): direct engine floating point output scaled by master volume.
  - `I16` (signed 16-bit PCM `-32767..32767`): clamped and rounded `(sample.clamp(-1.0, 1.0) * 32767.0).round() as i16`.
- **Channel Allocation:**
  - `channels >= 2`: Channel 0 is Stereo Left, Channel 1 is Stereo Right. All surplus channels (`2..channels`) are strictly zeroed (`0.0` / `0`) to prevent uninitialized memory noise.
  - `channels == 1`: Mono downmix averaging `(Left + Right) * 0.5`.

### 4.2 Event Queue & Backpressure Policy
- The `AudioHandle` provides a bounded queue (`bounded(1024)`) connecting MIDI/UI threads to the real-time audio thread.
- **Non-critical events** (continuous controllers, pitch bend, note-on) use non-blocking `try_send` and drop on queue saturation to preserve real-time responsiveness and avoid latency spikes.
- **Critical note-lifecycle events** (`NoteOff`, `AllNotesOff` / `CC123`, `AllSoundOff` / `CC120`, and `SustainPedal(0)` / `CC64=0`) use a retry timeout (`send_timeout`) allowing the audio thread to drain before delivery, ensuring keys and pedal states are never left permanently stuck.

---

## 5. Menu model (`ns4-yfht`)

Manual OS v1.6x ch. 11 (p.57-63) plus Organize (p.45-46) is modelled as toolkit-free data in [`crates/nord-panel/src/menus/`](../../crates/nord-panel/src/menus/mod.rs), one file per menu. A `MenuPage { menu, page, title, items }` holds `MenuItem { id, label, kind, default, manual_page, provenance, binding, note }`; `kind` is `Toggle`, `Enum(choices)`, `Int`, `Float` or `Action`. Binding uses the control-inventory scheme: `Bound("<host id>")` (a `nord-plugin` parameter), `Bound("engine:<path>")` (an engine field without host parameter), `UiOnly(reason)`, `Unsupported(bean)`. Channels use `0 = Off`.

| Menu (SHIFT + Program) | Pages | Items | Notes |
|---|---:|---:|---|
| System (1) | 6 | 6 | transpose, fine tune bound; memory protect, LED, reset, version `UiOnly` |
| Sound (2) | 11 | 11 | all bound except Organ trigger point |
| Organize (3) | 2 | 7 | Swap/Move: `Unsupported` |
| Aux KB (4) | 3 | 5 | all bound to `auxkb_*` |
| Output (5) | 6 | 21 | routing `Unsupported`; `sys_mono` is the emulator's Mono Output |
| Pedal (6) | 7 | 19 | Func/Gain/Right/Rotor bound; jack types `UiOnly`; Triple Pedal Left/Middle `Unsupported` |
| MIDI (7) | 10 | 23 | channels bound to engine fields; filters and PC modes `Unsupported` |
| Extern (8) | 13 | 37 | Extern layer fields bound; Device toggles, velocity, Echo `Unsupported` |

Totals: 58 pages, 129 items; 56 bound, 12 `UiOnly`, 61 `Unsupported` (bean `ns4-zb9o`). Tests (`menus/tests.rs`): unique ids, pages `1..n` per menu, defaults match the kind and lie in range, every `Bound` host id or engine path exists in the sources, every `Unsupported` bean exists in `.beans/`.

Provenance: names, ranges, defaults and page numbers come from the manual text (`VERIFIED`). `ASSUMED`: Program Levels range, 0.5 dB step of the two piano levels, pedal gain default/step, foot switch defaults, Device page order, Preset Organize ranges. Web sources (names only, no values copied): Stage 3 Sound menu <https://manualshive.com/en/manual/nord/stage-3/1701214?page=20>, Stage 4 user forum <https://www.norduserforum.com/viewtopic.php?p=183703>; they did not change any value.

### Engine-audible settings (`engine/settings.rs`)

`GlobalSettings` (defaults equal the pre-menu hard-coded values) is applied by `Stage4Engine::set_global_settings` (idempotent, no allocation).

| Setting | Effect | Status |
|---|---|---|
| Global Transpose +/-6 | added to the per-program transpose; held keys release at their start pitch | text `VERIFIED` |
| Fine Tune +/-50 ct | Organ wheels (`fine_tune_cents` + master), Piano voices started afterwards, Synth pitch | range `VERIFIED`; mapping implemented, hardware cents unmeasured |
| Piano Pedal Noise / String Res +/-6 dB | linear gain on the pedal thump / sympathetic resonance | range `VERIFIED` |
| B3 Tonewheel Mode | crosstalk factor Clean 0, Vintage 1 1.0, Vintage 2 1.5 | factors `ASSUMED`; confirm with a hardware single-key spectrum |
| B3 Click Level | High = 2x click amplitude | `ASSUMED` |
| Rotary Bass/Horn | band gains `2 * share`, 50/50 = 1.0 | `ASSUMED` |
| Rotor/Horn speed, acceleration | speed 0.85 / 1 / 1.15, time constant 1.5 / 1 / 0.67 | `ASSUMED`; confirm by timing a Slow-Fast change |
| Mono output | both channels carry `(L+R)/2` | implemented |

Program Levels (`prog_level_db`) and per-program Transpose already existed. Not honoured: Organ trigger point, LED intensity (`UiOnly`), output bus routing, MIDI filters, Local Control.
