# MIDI Implementation & Control Surface Specification

## 1. Scope

Specifies the MIDI Control Change (`CC`), Bank Select, Program Change, and Aftertouch mapping for the Nord Stage 4 Emulator (`NordStage4` / `MIDIOutKeyboard` in firmware).

Implemented in [`crates/nord-core/src/midi.rs`](../../crates/nord-core/src/midi.rs) and [`crates/nord-core/src/engine.rs`](../../crates/nord-core/src/engine.rs).

---

## 2. Channel Modes

MIDI channels are represented internally as `0..15` and displayed to users as channels `1..16`.

1. **Global Channel (Default Ch 1; `Off` supported):** Receives notes through the active Layer Scene and keybed split router. Global control messages are processed only on this channel. Setting it to `Off` disables global-channel input.
2. **Layer Channels (Default `Off` per layer):** Each Organ, Piano, and Synth layer can receive notes on its own assigned channel (`1..16` or `Off`). Layer-channel notes target that layer directly and bypass keybed split assignments. The target layer must be active in the current Layer Scene. `Off` disables that layer's direct MIDI input. If a channel is assigned to multiple layers, each matching active layer receives the note.
3. **Channel ownership:** Note Off is matched to the same MIDI channel and the layer recipients recorded for Note On. This prevents a release on one channel from releasing another channel's held pitch.

These channel roles follow the [Nord Stage 4 User Manual, OS v1.6x](https://www.nordkeyboards.com/wt/documents/951/Nord%20Stage%204%20User%20Manual%20v1.6X-Edition-N.pdf), MIDI chapter. The hardware also provides per-section `Sect On` settings, a separate Aux KB channel, MIDI receive filters, and NRPN support; those controls are outside this channel-routing change.

In this engine, incoming CC and channel aftertouch affect global controls only when received on the configured Global Channel. Layer-channel CC/NRPN receive filters and the separate Aux KB routing mode are not modeled by this change.

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

Implemented in [`crates/nord-gui/src/audio/`](../../crates/nord-gui/src/audio/mod.rs).

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
