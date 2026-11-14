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

In this engine, incoming CC and channel aftertouch affect global controls only when received on the configured Global Channel. The Ctrl / Dev / PC / Type receive filters of the MIDI menu apply to the Global channel only (section 2b); per-layer-channel CC/NRPN filters are not modeled. Aux KB routing is described in section 2a.

## 2a. Aux KB, pedals, pitch stick and Extern (Phase 6b B9d, `ns4-ymci`)

Source: manual OS v1.6x p.21-22, p.26-28, p.43-47, p.55-56, p.59-64. Code: `engine/aux_kb.rs`, `engine/pedals.rs`, `engine/pedal_assign.rs`, `engine/extern_out.rs`, `midi.rs` (`MidiEvent::PitchBend`).

| Area | Behaviour | Status |
|---|---|---|
| Aux KB | `AuxKb { enabled, channel (default 16), layers[7], curve }`. A layer in Aux KB mode ignores the Global channel (internal keybed) and answers only the Aux KB channel, bypassing keyboard zones. Direct layer channels still win. | Manual p.43-45 |
| Aux KB panel LED | `Stage4Engine::set_layer_aux(layer, on)` (`engine/zone_assign.rs`) sets `layers[i]` and, when switching on, `enabled`; the Organ / Piano Aux KB LEDs (`aux_led.a/b`) send it via `OrganEdit::AuxKb` / `PianoEdit::AuxKb` and display the UI flag until telemetry reads the routing back | switch-on enabling `ASSUMED` |
| Aux curve | Soft/Medium/Hard: identity for Medium, power law 0.7 / 1.4 otherwise | `ASSUMED` exponents |
| SUSTPED | Per layer. Default On for Piano A/B only (the previous hard-wired behaviour), Off elsewhere. Organ and Synth layers defer note release while the pedal is down (`held` bitmask, no allocation); Pianos use their damper model | Manual p.18, p.28 |
| Aux sustain/bend | `CC64` and pitch bend on the Aux KB channel reach Aux layers regardless of SUSTPED/PSTICK; the internal pedal needs the flag | Manual p.45 |
| Half pedaling | Sustain value `>= 64` = down. Between 20 % and 50 % depth the Pianos use the slow soft-release decay (`PianoLayer::damper_half`) | Manual p.26; band and mapping `ASSUMED` |
| Sostenuto, soft | `CC66`, `CC67` on the Global channel stand for the TP-2 Middle / Left pedals and go through `triple_pedal`; with the default assignments (Sostenuto / Una Corda) they drive the Piano layers exactly as before | Manual p.27 |
| Triple Pedal assignments | `triple_pedal(unit, pedal, value)`; `TripleFunc`: Una Corda (Left default), Sostenuto (Middle default), Panel Assigned, Layer Scene Hold/Toggle, Pump, Mst Clk Tap, Synth Vibrato, Program Up/Down, Rotor Hold/Toggle, Control (TP-2 only). Left/Middle of a TP-1 (`SustainPedalType::Tp1`) share the list and are ignored ("Sustain Only") when a TP-2 is attached. Switch functions act on the press edge (`>= 64`); Una Corda, Sostenuto, Pump, Synth Vibrato and Control follow the continuous value. Panel Assigned resolves through `triple.panel_assigned` (Layer Scene II taken as Hold, `ASSUMED`); the panel's Shift+Mst Clk (PEDAL TAP) sets it to Mst Clk Tap (`ProgramEdit::PedalTap`, control inventory section 8). The function list is the manual's (p.60); retailer listings of the TP-2 agree on it (https://www.kirstein.de/en/Pedals-and-Buttons/Clavia-Nord-Triple-Pedal-2.html, https://www.perfectcircuit.com/nord-ntp2-triple-pedal-2.html; `EMPIRICAL`, secondary sources, no behaviour detail) | Manual p.26, p.60 |
| Pump | The Pump FX type does not exist yet: `PedalState::pump_value()` exposes `0..1`. When Pump lands, feed it to `mod1_ped_pos` of the chains in Mod 1 PED mode | Open |
| Synth Vibrato pedal | Writes `SynthLayer::vibrato_ctrl` of all Synth layers (audible in vibrato mode PED) | Manual p.60 |
| Program Up/Down | The engine does not own the program list: `take_program_step()` returns the net presses (`+1` / `-1`) since the last call; the host loads the neighbouring program | Manual p.60 |
| Mst Clk Tap | `tap_tempo()`: an interval between two taps of `0.2..2.0` s sets `master_clock.bpm = 60 / interval` and re-snaps delay times; a longer gap starts a new sequence. Sample-counted (`render_frame` ticks), no allocation. Window and last-interval-only law `ASSUMED` | Manual p.60 |
| Sustain Pedal Type | `SustainPedalType { Sp1, Sp2, Tp1, NormallyOpen, NormallyClosed }` conditions the jack value (Global source only, never Aux KB): `Sp1` / `NormallyOpen` binary, `NormallyClosed` inverted, `Sp2` / `Tp1` continuous. The hardware default is SP-1, but the engine default is `Sp2` so continuous `CC64` half pedaling keeps working and existing renders stay identical. Control / Swell pedal brand ranges (Roland, Yamaha, ...) are analog calibrations with no MIDI equivalent: not modelled | Manual p.26, p.60 |
| Control pedal | `CC11` is the Morph source; `Ctrl + Swell` also drives Swell; gain 1-10 | Manual p.60 |
| Swell | `CC4` scales the Organ layers. B3 floor 0.1 (-20 dB, `ASSUMED`); other models reach 0. Function `Volume` scales the master mix, `Wheel Morph` mirrors the mod wheel | Manual p.22, p.60 |
| Rotor pedal, foot switch | `rotor_pedal(down)` and `foot_switch(down)`; Toggle/Hold; foot switch can switch Layer Scene or the rotor; Sustain+Rotor Hold/Toggle on the sustain jack. No MIDI CC exists for these jacks in the manual, so they are API/host-parameter inputs only | Manual p.53, p.61 |
| Pitch stick | `MidiEvent::PitchBend` (14 bit). PSTICK per layer. Range: the Synth PSTICK/RNG setting `1..=12` semitones (default 2; the list is not printed in the manual, the 12 ceiling is `ASSUMED`), set through `Stage4Engine::set_pstick_range` (clamps; `engine/pstick.rs`); Organ and Piano ignore the setting and bend a fixed 2 semitones (`VERIFIED` manual p.18, p.23; test `organ_and_piano_ignore_the_range_setting`). Applied to every layer: Synth through `SynthLayer::bend_cents`, Organ through `OrganLayer::set_bend_cents` (Legacy wheels scale their phase step, the firmware / B3 / Farfisa / Vox sources add it to Fine Tune) and Piano through `PianoLayer::set_bend_cents` (sampled voices scale the sample step, the harmonic model its frequency); all exactly neutral at centre and with PSTICK off (default), so renders are unchanged | Manual p.18, p.28 |
| Extern mode | A Synth layer with `extern_out.layers[i].enabled` emits MIDI instead of audio. Notes are transposed by the octave shift, limited by the keyboard zones (crossfade gain scales velocity) and the sent pitch is remembered so Note Off always matches. CC VAL 1/2 (`extern_set_cc_value`), Bank Select + Program Change (`extern_set_program`), Send On Load, CC7 from the layer fader (`extern_sync_volume`, run once per `process_block`), CC64 and pitch bend forwarded when the layer's SUSTPED/PSTICK are on | Manual p.46-47, p.63-64 |
| Output queue | `MidiOutQueue`: fixed 256-entry ring buffer; backends drain with `pop_midi_out()`; overflow drops the newest event and counts it (`dropped()`). No allocation on the audio path | `ASSUMED` capacity |
| Channel helpers | `set_global_channel`, `set_layer_channel`, `set_aux_channel` take panel numbers 1-16 or `None` (Off); `route_for(layer, channel)` resolves `NoteRoute::{Layer, Aux, Global}` | Manual p.62 |

Not modelled: the Pump FX itself (the pedal value is exposed), TP-2 jack detection, the plugin MIDI output port for Extern (the queue is drained by `pop_midi_out`). The Extern menu filters and the MIDI menu are in section 2b.

## 2b. MIDI menu, Extern menu, Output routing, trigger point, SW B (`ns4-zb9o`)

Source: manual OS v1.6x p.46-47, p.55-56, p.58-64. Code: `engine/midi_menu.rs`, `engine/extern_menu.rs`, `engine/output_routing.rs` + `engine/mix.rs`, `engine/organ_trigger.rs`, `engine/foot_switch.rs`. Every default keeps the engine render bit-identical (tests `defaults_are_inert`, `default_routing_is_bit_identical_to_the_stereo_mix`, the existing render-hash tests).

| Area | Behaviour | Status |
|---|---|---|
| Local Control (MIDI 1) | `midi_menu.local_control` (default On, **not** a host parameter: reverts to On at start, manual p.57). `handle_keyboard_midi(event)` is the entry for the instrument's own keys (the plugin editor's on-screen keys use it when Local is Off): it always transmits on the Global channel (queue, `pop_midi_out`) and dispatches to the sound engines only with Local On. `handle_midi` stays MIDI In | Manual p.56, p.61; the standalone keybed, mod wheel and pitch stick send `AudioCommand::Keyboard` (`AudioHandle::send_keyboard_midi`), which runs `handle_keyboard_midi`, so Local Off silences them (tests `local_control_off_silences_the_on_screen_keybed`, `keyboard_commands_obey_local_control_but_midi_in_does_not`); external MIDI keeps `AudioCommand::Midi` / `handle_midi` |
| Sect On (MIDI 3-5) | `midi_menu.sect_on[organ, piano, synth]`: the channel of Layer A addresses every layer of the section (`layer_channel`, used by `route_for`); the B / C own channels are then ignored | Manual p.62 |
| Type / Ctrl / Dev (MIDI 7) | `MidiMode { Off, Send, Receive, SendReceive }`. Dev = pitch bend, mod wheel, aftertouch, sustain, control / swell / volume pedals (+ CC66 / 67, `ASSUMED`): receive gates them on the Global channel, send gates their transmission from `handle_keyboard_midi`. Ctrl = every other CC. Type CC only drops the NRPN registers (CC 99 / 98 / 6 / 38) which are otherwise decoded into `take_nrpn()` (no parameter map yet). The Aux KB channel is not filtered | Group list `VERIFIED` p.62; default Ctrl = Send & Receive (hardware Off) `ASSUMED`, keeps panel CC receive |
| PC / PC Ext (MIDI 8) | Incoming Program Change on the Global channel queues a `ProgramRequest { bank_msb, bank_lsb, program }` (`take_program_change()`; the host owns the program list). Bank MSB 0 / 1 (Program, Live) obey PC, MSB 2-4 (Synth, Piano, Organ presets) obey PC Ext; Bank Select CC0 / CC32 are tracked when either receives. `send_program_change(msb, lsb, pc)` transmits CC0, CC32, PC on the Global channel when the matching mode sends | Bank table `VERIFIED` p.56 |
| Transpose MIDI At (MIDI 9) | `In` (default): the transpose shifts keyboard and incoming MIDI notes, the transmitted keyboard notes stay untransposed. `Out`: transmitted notes are shifted, incoming MIDI notes are not. The keyboard sound and the Aux KB channel are always transposed (p.55); Extern layer output keeps its existing transposition | Manual p.62 |
| Echo Global to Extern Ch (Extern 1) | `extern_menu.echo_global`. Off keeps Global-channel MIDI-In notes from Extern layers; keyboard notes (`handle_keyboard_midi`) always reach them. Engine default On (hardware Off) because the emulator feeds keybed and MIDI In through one path | default `ASSUMED` |
| Keyboard Velocity (Extern 2) | `extern_menu.velocity[3]`: Soft / Mid / Hard through the Aux curve (`AuxVelocityCurve::map`, exponents `ASSUMED`) on transmitted note velocity | Manual p.63 |
| Device MIDI (Extern 8-12) | `extern_menu.devices[3]`: mod wheel (CC1), aftertouch (channel pressure), control pedal (CC11), swell pedal (CC4) and volume pedal (CC7, when the Swell function is Volume) received on the Global channel are forwarded to each enabled Extern layer whose switch is On (default On) | Manual p.64; CC1 / CC7 numbers `ASSUMED` |
| Output routing (Output 1-5) | `OutputRouting { mode, bus_outputs[4], global_out[7], bus[7], trim_db[4] }`, sources Organ, Rotary, Piano A / B, Synth A / B / C. Choices `1-2, 3-4, 1, 2, 3, 4, 12+34` (+ arrow / Auto for B, C and Rotary, which follow the left layer, Rotary Auto the Organ or the first Piano / Synth layer feeding it). Per Program resolves source to bus to output. `render_frame_multi()` and `process_block_multi([&mut [f32]; 4])` return outputs 1-4; `render_frame` / `process_block` stay the Main pair 1-2. A single output carries `(L + R) / 2`. Master bus, Mono Output and global reverb act on the Main pair | choices, defaults, arrows `VERIFIED` p.59; mono sum and Main-only master `ASSUMED`; output trims +/-12 dB are an emulator extra |
| Organ Keyboard Trigger Point (Sound 4) | `organ_trigger.point`: High (default) sounds the organ at once, Low delays Organ note-ons by `LOW_TRIGGER_DELAY_SEC` (8 ms, 16 pending slots, a release before the trigger cancels the note) | choices `VERIFIED` p.58; delay `ASSUMED` |
| Foot Switch SW B (Pedal 6) | `pedals.foot_switch_b` (`FootSwitchB`) and `foot_switch_b(down)`: the ten choices (User Assigned .. Rotor Toggle) run through the same function table as the Triple Pedal (`apply_pedal_func`); User Assigned resolves through the panel assignment. SW A keeps its five-function enum | Manual p.60; User Assigned `ASSUMED` |

Host parameters: `midix_*` (MIDI + Extern menus, `midi_menu_params.rs`), `outx_*` (Output menu, `output_params.rs`), `menx_*` (trigger point and SW B, `menu_extra_params.rs`); see `docs/HOST_COMPATIBILITY.md`. The plugin keeps one stereo audio port (outputs 1-2); outputs 3-4 are an engine API. No web source changed a value; the manual text is the only source.

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
| Sound (2) | 11 | 11 | all bound |
| Organize (3) | 2 | 11 | Both pages `UiOnly` (act on the in-memory program and preset libraries, sections 5.2 and 5.3) |
| Aux KB (4) | 3 | 5 | all bound to `auxkb_*` |
| Output (5) | 6 | 21 | routing bound to `engine:output_routing.*`; `sys_mono` is the emulator's Mono Output |
| Pedal (6) | 7 | 19 | Func/Gain/Right/Rotor, Triple Pedal and Foot Switch SW A / B bound; jack types `UiOnly` |
| MIDI (7) | 10 | 23 | channels, Local Control, Sect On, Type / Ctrl / Dev, PC / PC Ext and Transpose At bound to `engine:midi_menu.*` |
| Extern (8) | 13 | 37 | all bound (`engine:extern_out.*`, `engine:extern_menu.*`) |

Totals: 58 pages, 133 items; 111 bound, 22 `UiOnly`, 0 `Unsupported` (the last two, Preset Organize, became `UiOnly` with section 5.3; bean `ns4-zb9o`). Tests (`menus/tests.rs`): unique ids, pages `1..n` per menu, defaults match the kind and lie in range, every `Bound` host id or engine path exists in the sources, every `Unsupported` bean exists in `.beans/`.

Provenance: names, ranges, defaults and page numbers come from the manual text (`VERIFIED`). `ASSUMED`: Program Levels range, 0.5 dB step of the two piano levels, pedal gain default/step, foot switch defaults, Device page order, Preset Organize ranges, section choice and location text. Web sources (names only, no values copied): Stage 3 Sound menu <https://manualshive.com/en/manual/nord/stage-3/1701214?page=20>, Stage 4 user forum <https://www.norduserforum.com/viewtopic.php?p=183703>; they did not change any value.

Wording: page titles, item labels and enum choices were checked against the string pool of the OS v1.66 host firmware (`tools/ns4_oled_strings.py`, [oled-strings-vs-firmware](../research/oled-strings-vs-firmware.md)). Where the firmware text is unambiguous the menus use it (for example `OUTPUT OVERVIEW`, `Organ Keyboard Trig`, `Roland EV7`, `ModWheel MIDI`); our longer descriptive labels (`Organ output (Global)`, `Piano A bus (Per Program)`) and the menu structure itself stay manual-derived. The firmware prints values with its own formats (`%d cent`, `%+.1lf dB`), so unit suffixes are no longer part of a label.

### 5.1 Menu screens in the Program bay

Shift+Program 1..8 opens the menu on the Program OLED (`panel/bay_program/menu_screen.rs`, state in `panel/ui_state/menu.rs` as `MenuUi`, behaviour in `bay_program/menu_actions.rs`). Layout: title line, `n/N <page title>`, then one row per item (`>` marks the focus) with its value. PAGE-/PAGE+ (and BANK) step pages and stop at the ends; Program 1-4 are soft buttons that focus an item (an `Action` item runs); on a page with more than four items (Output pages 2 and 5) soft 1-3 are items and soft 4 is `More`. The PROGRAM dial steps the focused item (toggle follows the sign, enum and int clamp, float snaps to its step grid). Shift/Exit leaves; values stay in `ProgramUi::menu`.

| Binding | Behaviour |
|---|---|
| `Bound` | the edit is sent as `ProgramEdit::Settings(SettingsEdit { id, value })` and applied on the DSP thread by `audio/edits/settings/` (plain field writes, no allocation): `GlobalSettings` via `set_global_settings`, `set_program_level_db`, `pedals`, `pedals.triple.set_func`, `aux_kb`, `set_global_channel` / `set_layer_channel` / `set_aux_channel`, `extern_out.layers`, and the `routing.rs` set: `midi_menu`, `extern_menu`, `output_routing`, `organ_trigger`, `pedals.foot_switch_b` |
| `UiOnly` | value kept in `MenuUi` only (LED Intensity, jack types, version info) |
| `Unsupported` | row dimmed and shows the bean id; the dial does nothing |

System page 5 Reset All Settings restores every default and sends the bound ones; it keeps Memory Protect. Memory Protect (page 1, default On, manual p.57) blocks STORE to a bank location with the message "Memory Protect is on"; storing to a Live slot is exempt. STORE AS (Shift+Store, `bay_program/name.rs`) proposes the current name; when a program of that exact name is stored or sits in the opened archive it proposes `Name 2`, `Name 3`, ... instead (an existing ` N` suffix is replaced, the base is cut to the 16-character limit; EMPIRICAL for `NS4 Palette` -> `NS4 Palette 2`, piano-pad video 5:40, the counter rules ASSUMED). The `Cat` soft button cycles the 26 firmware categories (`CATEGORIES`, VERIFIED against `bank0_CODE.bin@0x2BF3C8`) and starts on the category of the program stored at the current location, else `Undefined`.

Mapping notes: the Aux KB synth choices (`---`, `A--`, `-B-`, `--C`, `AB-`, `A-C`, `-BC`, `ABC`) are mapped to layer masks by label (the host parameter `auxkb_synth` is a plain bitmask, so its index differs for `--C`, `AB-`); pedal Left/Middle and Sustain Type (Pedal pages 3-4) are `Bound` to the `pedal_assign` state; Foot Switch SW A choices without an engine function map to Off; Extern bank `--` (-1) is stored as 0. Previously `Unsupported`, now wired (see the beans): Organ Trigger Point, Output routing, MIDI Local Control / Sect On / Type / Ctrl / Dev / PC / Transpose At, Extern Echo / Velocity / Device MIDI, Pedal SW B.

### 5.2 Organize (Shift+Program 3, manual p.45-46)

Pure functions in [`nord_core::library_ops`](../../crates/nord-core/src/library_ops.rs) work on any bank array (programs and presets): `swap(bank, a, b)` interchanges two locations; `move_item(bank, src, dst)` takes the source out and inserts it at the destination, shifting the locations in between by one towards the source (`rotate_left` when moving down the list, `rotate_right` when moving up); both return `false` and change nothing for equal or out-of-range locations. `OrganizeUndo` keeps one snapshot. Only the manual text is `VERIFIED`; the behaviour at the ends of the list is `ASSUMED` equal to the middle (no wrap).

The Program Organize page (`bay_program/organize.rs`) is a dedicated screen: the PROGRAM dial moves the location pointer (item `organize.1.location`, 0..511), the soft buttons are **Swap**, **Move**, **Ok**, **Undo**. Swap or Move remembers the pointer as the source (pressing the same button again cancels), the dial then picks the destination, Ok runs the operation on the in-memory library (`UserLibrary`: stored programs plus the archive entries of the opened `.ns4b`, moved together as cells), Undo takes back the last operation or cancels a pending choice. Memory Protect blocks Ok and Undo with "Memory Protect is on", like a bank STORE. With a `.ns4b` open, program recall by location follows the archive headers (so moved programs recall from their new location) instead of the sorted catalog position. The Preset Organize page is section 5.3. Export backup ([cbin-and-presets.md](cbin-and-presets.md) section 5.5) writes the new layout.

Live slots: `.ns4l` files load back into the eight Live slots (`LiveSlots::import_live_file`; the GUI button **Load Live slots** loads all of an opened backup).

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

Program Levels (`prog_level_db`) and per-program Transpose already existed. Not honoured: LED intensity (`UiOnly`). The MIDI, Extern, Output, trigger point and SW B items are in section 2b.

### 5.3 Preset Organize (Shift+Program 3, page 2, manual p.45)

The manual says the order of Programs and Presets can be rearranged with Swap and Move, "organizing presets works in just the same way". Page 2 of the Organize menu does that for the Preset Library of the opened `.ns4b`: `bay_program/organize_presets.rs`, items `organize.2.{location,section,swap,move,ok,undo}` (all `UiOnly`, `ASSUMED` except the manual-given verbs).

| Part | Behaviour | Status |
|---|---|---|
| Data | `PresetLibrary` keeps, per section (Organ, Piano, Synth), the archive entry at each flat location (`bank * 64 + position`). Read from the `.ns4o` / `.ns4n` / `.ns4y` headers at open (`nord_core::cbin::PresetIndex`). Cells are entry paths, or the virtual path of a preset stored with STORE (section 5.4) | header fields `VERIFIED` (`python tools/preset_slots.py "<backup>.ns4b"`: 544 presets, Organ 64 in bank 0, Piano 64 + 32 in banks 0-1, Synth 6 x 64 in banks 0-5, 0 duplicates, folder `Bank n+1` equals header bank + 1) |
| Capacity | Organ 2 banks, Piano 2, Synth 8 (128 / 128 / 512 locations) | `ASSUMED`: the factory banks are the folders `Bank 1..2`, `1..2`, `1..8` of the backup layout table; the manual gives no capacity |
| Section | Shift + PROGRAM dial steps Organ / Piano / Synth (no wrap); the location pointer is clamped into the new section and a pending choice is cancelled | `ASSUMED`: the manual does not say how the section is chosen |
| Location | PROGRAM dial moves the pointer; text `bank:position`, both 1-based, two digits (`1:05`, as the preset ids of manual p.42) | `ASSUMED` (open: the manual's example `1:27 B3 Flute 2` does not match the header location 8 of that preset in the factory backup, see OPEN-QUESTIONS) |
| Operations | Swap, Move, Ok, Undo, Memory Protect gate and messages as the Program page (section 5.2), one Undo level per section, `nord_core::library_ops` | manual text `VERIFIED`; ends of the list `ASSUMED` equal to the middle |
| Export | `plan_presets` (`library/export.rs`) compares each location with the archive index: an entry not at its old place is written with the new header bank / location (`PresetPut`, folder `Bank n+1`), an emptied location is removed (`preset_edits`); untouched entries stay byte-identical | tests `library/export_presets_tests.rs`, `cbin/backup_presets.rs` |

Storing a new preset and loading presets into the engine are section 5.4.

### 5.4 Preset Library: browse, load and store (manual p.41-42)

| Part | Behaviour | Status |
|---|---|---|
| Browse | ORGAN / PIANO / SYNTH in PRESET LIBRARY (Shift + PIANO / SYNTH: Single Layer) opens the browser; the PROGRAM dial walks the occupied locations of the section (`bay_program/preset_browse.rs`), the OLED lists their names and prints `bank:position name`. A backup without presets keeps the earlier model browser (Organ models, `.npno`, `.nsmp4`) | manual `VERIFIED`; the list order is the location (Num); Cat sort and Cancel (revert) are not built |
| Load | Every dial step requests the entry (`LoadRequest::Preset`); `library/preset_load.rs` decodes it and sends `ProgramEdit::LoadPreset` (undoable, [cbin-and-presets.md](cbin-and-presets.md) section 5.6); the panel controls follow; the sounds the preset names load like a program's. A load counts as a program edit (boxed `E`). Single Layer takes layer A of the file into the focused layer | engine rule `ASSUMED` for the layer choice, otherwise manual p.42-43 |
| Store | STORE, then ORGAN / PIANO / SYNTH: "Store Preset To" (`bay_program/preset_store.rs`); the dial, PAGE and BANK buttons and the Program buttons pick the location (starts on the loaded preset's cell, else the first free one), STORE confirms. A Preset Library button first and then STORE does the same; STORE AS names the preset first. Memory Protect blocks it like a program STORE ("Memory Protect is on"). The name defaults to the loaded preset's | flow `VERIFIED` from the manual text; the screen text and the default name are `ASSUMED` |
| Data | A stored preset is a program snapshot plus the base entry (`bay_program/preset_user.rs`); its cell holds the virtual path `~user/<n>/<name>.<ext>`, so Preset Organize moves it and the browser lists it. Overwritten or organized-away presets are forgotten | |
| Export | Export backup writes each stored preset as a file at its location (`library/preset_export.rs`, `PresetPut`); other entries stay byte-identical (`library/export_presets_tests.rs`) | hardware acceptance open (OQ31) |

Not built: "Cancel" reverting to the previous sound, sorting by Cat, and the Synth sound edits in a stored preset (OQ30).
