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

Source: manual OS v1.6x p.21-22, p.26-28, p.43-47, p.55-56, p.59-64. Code: `engine/aux_kb.rs`, `engine/pedals.rs`, `engine/pedal_assign.rs`, `engine/extern_out.rs`, `midi.rs` (`MidiEvent::PitchBend`).

| Area | Behaviour | Status |
|---|---|---|
| Aux KB | `AuxKb { enabled, channel (default 16), layers[7], curve }`. A layer in Aux KB mode ignores the Global channel (internal keybed) and answers only the Aux KB channel, bypassing keyboard zones. Direct layer channels still win. | Manual p.43-45 |
| Aux curve | Soft/Medium/Hard: identity for Medium, power law 0.7 / 1.4 otherwise | `ASSUMED` exponents |
| SUSTPED | Per layer. Default On for Piano A/B only (the previous hard-wired behaviour), Off elsewhere. Organ and Synth layers defer note release while the pedal is down (`held` bitmask, no allocation); Pianos use their damper model | Manual p.18, p.28 |
| Aux sustain/bend | `CC64` and pitch bend on the Aux KB channel reach Aux layers regardless of SUSTPED/PSTICK; the internal pedal needs the flag | Manual p.45 |
| Half pedaling | Sustain value `>= 64` = down. Between 20 % and 50 % depth the Pianos use the slow soft-release decay (`PianoLayer::damper_half`) | Manual p.26; band and mapping `ASSUMED` |
| Sostenuto, soft | `CC66`, `CC67` on the Global channel stand for the TP-2 Middle / Left pedals and go through `triple_pedal`; with the default assignments (Sostenuto / Una Corda) they drive the Piano layers exactly as before | Manual p.27 |
| Triple Pedal assignments | `triple_pedal(unit, pedal, value)`; `TripleFunc`: Una Corda (Left default), Sostenuto (Middle default), Panel Assigned, Layer Scene Hold/Toggle, Pump, Mst Clk Tap, Synth Vibrato, Program Up/Down, Rotor Hold/Toggle, Control (TP-2 only). Left/Middle of a TP-1 (`SustainPedalType::Tp1`) share the list and are ignored ("Sustain Only") when a TP-2 is attached. Switch functions act on the press edge (`>= 64`); Una Corda, Sostenuto, Pump, Synth Vibrato and Control follow the continuous value. Panel Assigned resolves through `triple.panel_assigned` (Layer Scene II taken as Hold, `ASSUMED`). The function list is the manual's (p.60); retailer listings of the TP-2 agree on it (https://www.kirstein.de/en/Pedals-and-Buttons/Clavia-Nord-Triple-Pedal-2.html, https://www.perfectcircuit.com/nord-ntp2-triple-pedal-2.html; `EMPIRICAL`, secondary sources, no behaviour detail) | Manual p.26, p.60 |
| Pump | The Pump FX type does not exist yet: `PedalState::pump_value()` exposes `0..1`. When Pump lands, feed it to `mod1_ped_pos` of the chains in Mod 1 PED mode | Open |
| Synth Vibrato pedal | Writes `SynthLayer::vibrato_ctrl` of all Synth layers (audible in vibrato mode PED) | Manual p.60 |
| Program Up/Down | The engine does not own the program list: `take_program_step()` returns the net presses (`+1` / `-1`) since the last call; the host loads the neighbouring program | Manual p.60 |
| Mst Clk Tap | `tap_tempo()`: an interval between two taps of `0.2..2.0` s sets `master_clock.bpm = 60 / interval` and re-snaps delay times; a longer gap starts a new sequence. Sample-counted (`render_frame` ticks), no allocation. Window and last-interval-only law `ASSUMED` | Manual p.60 |
| Sustain Pedal Type | `SustainPedalType { Sp1, Sp2, Tp1, NormallyOpen, NormallyClosed }` conditions the jack value (Global source only, never Aux KB): `Sp1` / `NormallyOpen` binary, `NormallyClosed` inverted, `Sp2` / `Tp1` continuous. The hardware default is SP-1, but the engine default is `Sp2` so continuous `CC64` half pedaling keeps working and existing renders stay identical. Control / Swell pedal brand ranges (Roland, Yamaha, ...) are analog calibrations with no MIDI equivalent: not modelled | Manual p.26, p.60 |
| Control pedal | `CC11` is the Morph source; `Ctrl + Swell` also drives Swell; gain 1-10 | Manual p.60 |
| Swell | `CC4` scales the Organ layers. B3 floor 0.1 (-20 dB, `ASSUMED`); other models reach 0. Function `Volume` scales the master mix, `Wheel Morph` mirrors the mod wheel | Manual p.22, p.60 |
| Rotor pedal, foot switch | `rotor_pedal(down)` and `foot_switch(down)`; Toggle/Hold; foot switch can switch Layer Scene or the rotor; Sustain+Rotor Hold/Toggle on the sustain jack. No MIDI CC exists for these jacks in the manual, so they are API/host-parameter inputs only | Manual p.53, p.61 |
| Pitch stick | `MidiEvent::PitchBend` (14 bit). PSTICK per layer with range 1-12 semitones (default 2). Applied to Synth layers through `SynthLayer::bend_cents` (exactly neutral at centre). Organ and Piano bend is not implemented | Manual p.28 |
| Extern mode | A Synth layer with `extern_out.layers[i].enabled` emits MIDI instead of audio. Notes are transposed by the octave shift, limited by the keyboard zones (crossfade gain scales velocity) and the sent pitch is remembered so Note Off always matches. CC VAL 1/2 (`extern_set_cc_value`), Bank Select + Program Change (`extern_set_program`), Send On Load, CC7 from the layer fader (`extern_sync_volume`, run once per `process_block`), CC64 and pitch bend forwarded when the layer's SUSTPED/PSTICK are on | Manual p.46-47, p.63-64 |
| Output queue | `MidiOutQueue`: fixed 256-entry ring buffer; backends drain with `pop_midi_out()`; overflow drops the newest event and counts it (`dropped()`). No allocation on the audio path | `ASSUMED` capacity |
| Channel helpers | `set_global_channel`, `set_layer_channel`, `set_aux_channel` take panel numbers 1-16 or `None` (Off); `route_for(layer, channel)` resolves `NoteRoute::{Layer, Aux, Global}` | Manual p.62 |

Not modelled: Echo Global To Extern (the emulator has no separate internal keybed, so Global notes always reach Extern layers inside their zone), Extern device filters (mod wheel, aftertouch, control/swell/volume pedal forwarding), Extern keyboard velocity curve, Piano/Organ pitch bend, the Pump FX itself (the pedal value is exposed), TP-2 jack detection, the plugin MIDI output port for Extern.

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
