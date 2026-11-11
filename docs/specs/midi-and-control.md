# MIDI Implementation & Control Surface Specification

## 1. Scope

Specifies the MIDI Control Change (`CC`), Bank Select, Program Change, and Aftertouch mapping for the Nord Stage 4 Emulator (`NordStage4` / `MIDIOutKeyboard` in firmware).

Implemented in [`crates/nord-core/src/midi.rs`](../../crates/nord-core/src/midi.rs).

---

## 2. Channel Modes

1. **Global Channel (Default Ch 1):** Receives notes, Morph controllers (`CC1`, `CC11`, `Aftertouch`), Sustain (`CC64`), Sostenuto (`CC66`), Soft Pedal (`CC67`), Swell (`CC4`), Program Change, and panel CCs.
2. **Per-Layer Auxiliary Channels (Optional Ch 1..7):** Direct independent MIDI routing to `Organ A/B`, `Piano A/B`, and `Synth A/B/C` bypassing the keybed split router.

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
