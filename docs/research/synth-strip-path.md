# Synth firmware strip path: wiring, oracle check and the Smooth Lead gap

- **Checked:** 2026-11-13
- **Sources (local, never committed):** `Nord Stage 4 Factory Restore RevK v1.66.ns4b`, the Part A demo video (`NEW NORD STAGE 4 ... [_zksT69H30c].webm`), ORG core 2 image (`tools/ns4_dsp_ldr.py`), `sharc-org`; `tools/emu_vs_hw.py`, `tools/synth_strip_e2e.py`, example `synth_strip_e2e`, `ns4 render-patch --synth-chain strip`. Builds on [synth-adr-and-strip-law](synth-adr-and-strip-law.md), [synth-stage-equations](synth-stage-equations.md), [synth-voice-control](synth-voice-control.md), [dsp-frame-rate](dsp-frame-rate.md), [emulator-vs-hardware-audio](emulator-vs-hardware-audio.md).
- **Updated:** 2026-11-13 (section 6: wave stage probe, key-down velocity, default decision)
- **Question:** Can the verified synth firmware pieces (saw, ADR record chain, strip-law cutoff, four-pole filter, gain de-zipper) run as one per-voice chain, does the chain agree with the oracle end to end, and what explains the Smooth Lead gap (presence 1.5 to 3 kHz -16.5 dB, level -6.4 dB) against the hardware video?

Tags follow `.agents/rules/provenance-in-code.md`. Audio is lossy YouTube audio of a live performance with knob moves that are not replayed: band numbers are shape evidence, not bit-exactness.

## 1. Short answer

| Question | Result | Tag |
|---|---|---|
| Chain | `SynthLayer::chain = SynthChain::FirmwareStrip` (default `Legacy`, `render_hash` unchanged) runs per voice: `SawModel::FirmwareSy` -> ADR records (`fw_adr_chain`, coefficients `fw_env_rates`) -> strip-law cutoff -> `fw_filter` -> gain de-zipper. `ns4 render-patch --synth-chain strip`. | implemented |
| Oracle end to end (stage level) | envelope records and levels bit exact over 2 x 15 passes, cutoff `R` 3.5e-6 absolute, filter output 7.5e-7 absolute (2.9e-6 relative) | VERIFIED (section 3) |
| Panel to strip word mappings | cutoff Hz, attack, decay, release, resonance, pass length, filter type | ASSUMED (section 2) |
| Cause of the presence gap | the decoded waveform, not the filter flag or gain: Smooth Lead is ANALOG category 3 (SHAPE) wave 0, which the decoder maps to an ideal `Triangle` (ASSUMED placeholder). The hardware note has a second harmonic 3.6 dB under the fundamental; a triangle has none. | EMPIRICAL (section 4) |
| Result | presence -16.5 -> -9.0 dB, level RMS delta -6.4 -> +3.8 dB, score 4.93 -> 4.05 | EMPIRICAL (section 4) |
| Defaults | **not flipped** (section 5) | |

## 2. Panel to strip mappings (all ASSUMED)

The ARM word for a knob position is open ([synth-voice-control](synth-voice-control.md) s3), so each mapping inverts a VERIFIED DSP law on the value the classic engine already uses (`synth/strip_map.rs`).

| Panel value | Mapping | Evidence for plausibility |
|---|---|---|
| Cutoff Hz | word 3 = `round(2^24 i)`, `i = (tan(pi fc / F) - 0.36083) / 0.0079301` (clip `+-0x6A000000`), `R = fw_strip_cutoff_r(word)`, then `fc' = F atan(R) / pi` re-applied at the engine rate | identity inside `fc` 55 Hz .. 10.9 kHz (test `cutoff_word_round_trips_inside_the_dsp_range`, 2e-3); above `R = 1.0111` (0.2517 F = 10.9 kHz) the DSP saturates, so a 32 kHz panel cutoff is 10.9 kHz |
| Pass length | 2 SY frames (`STRIP_PASS_FRAMES`) | attack table 10.9 .. 1.0e6 passes = 0.50 ms .. 46 s, the panel 0.5 ms floor and 45 s ceiling (one frame per pass stops at 23 s); decay table tau 0.67 ms .. 9.2 s = panel 3 ms .. 45 s x 0.18 (0.54 ms .. 8.1 s) |
| Attack seconds | integer index with `1 / cA(idx)` passes closest (log) to the time | tables repeat a value 5 to 6 times: resolution factor 3 (test `time_indices_hit_the_table_within_one_plateau`) |
| Decay, release seconds | tau = 0.18 x displayed time (the classic filter-envelope fraction, now also amp and release), index with `1 / (1 - a)` passes closest | table range above; the classic amp envelope uses tau = displayed time and is not covered by a clip |
| Hold (decay >= 44 s) | index 64, decay record threshold 1.0 | the classic Hold rule |
| Sustain | decay record `thr` = `sustain_level` (0 or 1 from the preset) | firmware default is 0.5 (one run), preset ADR falls to silence (manual) |
| Resonance | `0..1` -> table position `0..127` linear (existing `TablePos::from_resonance`) | `W28 = 0x40000000` default = index 64 = resonance 0.5 |
| Filter type | `FwFilterType::from_model` (existing) | `kFltType` map open |
| Saw | `SawModel::FirmwareSy`: wrap step in engine increments, fade knee at `inc * fs / F` (5.0 kHz in Hz at any rate) | closes the TD-11 note of [dsp-frame-rate](dsp-frame-rate.md) |
| Velocity, key-down gain | classic law (`amp_velocity`), de-zipper on the note gain | the key-down gain series (OQ22) is open |

Not mapped: the key-down message `(layer<<29)|(0x0B on/0x08 off)<<24|((note-60)&0xFF)<<16|(0x80+g(vel))<<8` and `keydown-synth-velocity.csv` (branch `research/keydown-zones`, not merged here): the gate and velocity enter the pass through controller words whose modulation of the rate index is not decoded ([synth-adr-and-strip-law](synth-adr-and-strip-law.md) s2), so the level of a voice stays classic.

## 3. Oracle check (end to end of the stage words)

`cargo run --release -p nord-core --example synth_strip_e2e > strip.json`, then `python tools/synth_strip_e2e.py strip.json` (env `NS4_SHARC_ORG`, `NS4_ORG_IMAGE`, as in [synth-adr-and-strip-law](synth-adr-and-strip-law.md) Reproduce). The Rust example derives the words from the panel mappings above (Smooth Lead: cutoff 370 Hz, resonance 0.516; envelope at the fast table end) and the script feeds the same words to the ORG core 2 pass of the OS v1.66 image.

| Check | Oracle run | Result |
|---|---|---|
| Envelope A (gate on, attack to 1, decay to 0.5) | slot 0 chain, records from `StripEnvRecords::from_panel(0.5 ms, 3 ms, 3 ms, 0.5)`, 15 passes | 15 of 15 level words equal (0 mismatches) |
| Envelope B (release record from pass 8) | same, controller `0x00000200` before pass 8 | 15 of 15 equal |
| Cutoff | strip word 3 (`0xD5E07044` for 370 Hz) and its common block target `DM(0xE02E9)`, 40 passes | oracle `R = 0.0267846`, Rust `R = 0.0267881`, error 3.5e-6 |
| Filter `0x12566E` | firmware saw 220 Hz at F, 192 samples, Rust `R` and `W28 = idx<<24 + fr`, patched controls | max abs error 7.45e-7 (peak 0.259, 2.9e-6 relative) |

Limits: the pass installer rewrites the oracle records every 16th pass, so 15 passes per scenario is what poked records can cover; the strip word to rate index route (installer plus the controller modulation) is not run. The strip word chases the common block word (`w <- 0.99609 w + 0.00488 c`), so the cutoff is only reproduced once both hold the target. This check says the **stage words are right**; it says nothing about the panel to word mappings (ASSUMED) or the hardware audio.

## 4. Smooth Lead gap (A:87, `tools/emu_vs_hw.py`, confirmed notes)

Preset (decoded): ANALOG, category 3 (SHAPE), wave 0 -> `Triangle`, `Lp24`, cutoff 370 Hz, resonance 0.516, filter env amount 0.244, dedicated filter ADR (39 ms / 33 s / 0.55 s), amp ADR (16 ms / 45 s Hold / 0.9 s), mono, `filter_on` true.

| Band delta, emulator minus hardware (dB) | sub | low | mid | presence 1.5-3k | brilliance 3-8k | RMS | centroid Hz | score |
|---|---|---|---|---|---|---|---|---|
| default (ZDF LP24, Triangle) | +0.4 | -0.3 | +0.7 | **-16.5** | -6.9 | -6.4 | -232 | 4.93 |
| `--filter-engine firmware` | +0.7 | +0.1 | +0.5 | -15.2 | -5.8 | +3.9 | -269 | 4.46 |
| `--synth-chain strip` | -0.9 | -1.2 | +1.5 | **-9.0** | -7.6 | +3.8 | -113 | 4.05 |
| what-if: strip, base cutoff 500 Hz (not shipped) | -0.6 | -1.2 | +0.9 | -2.3 | -4.1 | +3.4 | -10 | 1.82 |

Findings, in the order they were ruled in or out (renders with `examples/zz_probe`-style overrides, 1/2-octave band energy re total):

1. **Not the filter flag.** `filter_on` is true; with the filter bypassed the emulator matches the hardware below 800 Hz (-13.3 .. -8.0 vs -15.1 .. -8.1 dB) and sits 6 to 19 dB under it from 1.1 to 2.3 kHz. A low-pass cannot add that energy.
2. **The waveform.** In the 3.85 to 4.55 s window (note B5, 987 Hz) the hardware band holding the second harmonic (1.6 to 2.3 kHz) is 49.5 dB against 53.1 dB for the fundamental: the second harmonic is 3.6 dB down. A triangle has odd harmonics only (the emulator reads 10.5 dB there). A saw has the second at -6 dB. So the SHAPE wave 0 of the hardware is saw like, and the decoder's `Triangle` (a placeholder in `cbin/synth_osc.rs`, tagged ASSUMED) is the largest single error. In the strip chain a decoded `Triangle` of category 3 plays as the firmware saw (`SynthLayer::osc_shape_category`, ASSUMED); presence -16.5 -> -9.0 dB with the same cutoff.
3. **LP24 vs LP4, gain staging.** The firmware filter is 10 dB louder than the ZDF path (RMS -6.4 -> +3.9): ZDF `Ladder24` plus the `0.25` voice constant leave the lead about 15 dB under the other engines, the four-pole firmware stage (DC gain 1, no drive loss) does not. This is a level, not a presence, effect.
4. **Cutoff scale (control, open).** With the saw and the strip filter the remaining -9 dB presence is a cutoff 1.35 to 1.6 times higher: base 500 Hz instead of 370 Hz gives presence -2.3 dB, centroid -10 Hz, score 1.82; 700 Hz overshoots (brilliance +8.4 dB). 1.4 octaves of extra cutoff is inside the +-1 octave tolerance of `FILTER_ENV_OCTAVES` (EMPIRICAL 8, this preset reaches 1.95 octaves) or the performer's knob, and the video cannot separate them. Not applied.
5. **Width and attack unchanged**: stereo width 0.09 vs 0.28 (cause not investigated), attack 33 ms vs 20 ms.

## 5. Why the defaults stay

- The oracle check covers the stage words, not the ARM mappings (cutoff, rate index, resonance), the controller modulation, the key-down gain series or the `kFltType` map (OQ22).
- The waveform fix is a guess from one program: ASSUMED, not VERIFIED, and `render_hash` of the classic chain must stay (`25baa387b0219c0835094150106a35ce751b9c16b68109d2a030596e78e7b59a`, checked).
- The video score moved from 4.93 to 4.05 only; the remaining gap depends on controls that are not replayed.

## 6. Wave stage probe, key-down velocity and the default decision (follow-up PR `feat/synth-wave-stages`)

**Which strip stage does a Wave select? Not decided by the edit ids.** `tools/synth_wave_probe.py` drives `Edt_Syn_kOscWave` through the real layer client of the booted v1.66 host firmware for the grid `kMode {0, 1} x kOscBank {0..3} x kOscCat {0, 3, 7} x kOscWave {0, 6, 127}` (value cells of mode, bank and category written first, 72 points, [data/synth-wave-probe.csv](data/synth-wave-probe.csv)): strip word 0 is `0x061215FB` at every point (the earlier sweep of five wave values at the default state agrees). So the ARM sender `0xC01CC85C` (`type = [this + 0xC8]`, `word 0 = ((byte & 0x7F) << 24) | table[type]`, table `0xC02FA050`, 74 ids) is re-sent by the wave, category, bank and mode ids but its type index stays 2 (stage `0x1215FB`). Whatever writes `[this + 0xC8]` is not a store of the edit-id handlers: a scan of all `str [rN, #0xC8]` sites of the OS image finds 198, none inside the strip class that was tied to the synth (the nearby ones are constructor field initialisers at `0xC01E5794`, `0xC01EDC00`, `0xC01EDE08`, `0xC01EEB1C`). The preset load / sound select apply path is the next suspect and needs a loaded program in the harness. Consequence: the stage of SHAPE, SUB OSC, SYNC and the SAMPLE / WAVE banks is **still unidentified**, so none of their waveforms can be modelled bit exactly, and the Python and Rust models of this task were **not written** (no stage to anchor them on; a model fitted to the single 3.6 dB second-harmonic reading would be a guess). The decoded `Triangle` of SHAPE wave 0 keeps playing as the firmware saw in the strip chain (ASSUMED, section 4).

**Key-down gain series.** Byte 1 of the key word is `0x80 + g(vel)` with `g` a saturating S-curve of the curve-6 table value (`floor(127 s(c / 600))`, `s` = smoothstep, 97 of 127 exact, max error 1; [keydown-zones](keydown-zones.md) s7). The exact 127-entry `g` list is `KEY_VELOCITY_G` (`synth/key_velocity.rs`, equal to the ARM run, checked by `tools/synth_velocity.py --check`). The strip chain now scales the voice by `g / 127` (`SynthLayer::strip_key_velocity`, default on, no effect on the legacy chain). That the DSP applies `g / 127` as a linear amplitude is ASSUMED: mid velocities get much softer than the linear law (velocity 64: 0.25 instead of 0.50).

**Re-run (3), `tools/emu_vs_hw.py --only synth_lead --confirmed-only`, `--synth-chain strip`:** sub -0.9, low -1.2, mid +1.5, presence **-9.0**, brilliance -7.6 dB, centroid -113 Hz, RMS **+3.8 dB**, score 4.05 (identical to section 4: the render velocity is 100, `g(100) / 127 = 0.77` against the linear 0.79, -0.2 dB).

**Decision (D22 bar): stay opt-in.** Criteria and status:

| Criterion | Status |
|---|---|
| Oracle stage check exact (envelope words, cutoff `R`, filter output) | met (section 3) |
| Video presence within about 3 dB | **failed**: -9.0 dB, the oscillator source of SHAPE is the open stage above |
| Video RMS within about 3 dB | **failed**: +3.8 dB |
| Panel to word mappings, `kFltType` map, controller protocol | ASSUMED / open (OQ22) |

`render_hash` of the classic chain is unchanged (`25baa387...e7b59a`); `SynthChain::Legacy` stays the default.

## Implications

- The strip chain is the better opt-in for A:87 type programs (presence +7.5 dB, level in line with the other engines) and is the right place to add further firmware stages.
- The decoder's analog SHAPE, SYNC, SUB OSC, MULTI and MISC waves (`cbin/synth_osc.rs`) are approximations; the oscillator side, not the filter, dominates the lead's spectrum. Fix them from the firmware stage selection (word 0 of the strip) rather than from names.
- A 1.4x cutoff factor or a deeper filter envelope would close the rest; decide with a hardware sweep ([synth-hardware-sweep-plan](synth-hardware-sweep-plan.md)).

## Follow-ups

1. Decode which strip stage (word 0) a SHAPE / SUB OSC / SYNC wave installs (section 6: not the edit ids; find the writer of `[strip object + 0xC8]` in the program-load path, e.g. a write watch while a program is applied in the harness, then run the stage in `sharc-org` with an initialised frame), then replace the `Triangle` placeholder with the real shape (OQ22).
2. Capture strip words for a loaded preset (ARM harness plus key-down, `research/keydown-zones`) to replace the cutoff, rate index and resonance mappings of section 2.
3. Run the installer route (strip word to index with the controller modulation) over more than 15 passes by writing the ADR strip words instead of poking records.
4. Re-run `tools/emu_vs_hw.py` with `--synth-chain strip` on other synth programs (A:25 lead, pads) before any default flip; the committed JSON has the Smooth Lead row only.
