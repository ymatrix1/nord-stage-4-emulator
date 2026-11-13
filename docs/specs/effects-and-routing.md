# Per-Layer Effects & Audio Routing Specification (`DSPF`)

## 1. Scope

Specifies the 7 independent per-layer effects pipelines (`Organ A`, `Organ B`, `Piano A`, `Piano B`, `Synth A`, `Synth B`, `Synth C`), the global Master Compressor, and the Master Clock.

Implemented in [`crates/nord-core/src/fx.rs`](../../crates/nord-core/src/fx.rs) and [`crates/nord-core/src/engine.rs`](../../crates/nord-core/src/engine.rs).

---

## 2. Per-Layer Signal Chain

Every one of the 7 layers processes its stereo audio through an isolated 5-stage FX rack:

```text
Layer Synth/Piano/Organ Output (Stereo)
  └──► [1. Mod 1 (Tremolo / AutoPan / RingMod / Wah / PedalWah)]
         └──► [2. Mod 2 (Phaser / Flanger / Chorus / Vibe / Spin)]
                └──► [3. Amp Sim & 3-Band EQ (Small / JC / Twin / British + Drive + Bass/Mid/Treble)]
                       └──► [4. Delay (Stereo / Ping-Pong / Analog Tape + HP/LP Feedback Filter)]
                              └──► [5. Layer Reverb (Booth / Room / Stage / Hall / Cathedral / Spring)]
                                     └──► Layer Level & Scene I/II Mute/Crossfade
```

### 2.1 Slot 1 — Modulation 1 (`Mod 1`)

- **`Tremolo`:** Stereo LFO amplitude modulation (`Rate`, `Amount`).
- **`AutoPan`:** Constant-power left/right panning LFO (`Rate`, `Amount`).
- **`RingMod`:** Quadrature carrier multiplier (`Rate` = carrier frequency $20\text{ Hz}..2\text{ kHz}$, `Amount` = wet mix).
- **`Wah` / `AutoWah`:** Envelope-follower or LFO/Pedal-swept resonant 2-pole bandpass filter ($350\text{ Hz}..2.8\text{ kHz}$).

### 2.2 Slot 2 — Modulation 2 (`Mod 2`)

- **`Chorus` / `Vibe`:** Dual modulated fractional delay lines ($7..18\text{ ms}$) with stereo $90^\circ$ phase offset.
- **`Flanger`:** Short modulated delay line ($0.8..6\text{ ms}$) with regenerative feedback.
- **`Phaser`:** 4-stage / 6-stage all-pass cascade swept by LFO.
- **`Spin`:** Rotary-style horn modulation for non-organ layers.

### 2.3 Slot 3 — Amp Simulation, Drive & 3-Band Semi-Parametric EQ

- **Amp Sim / EQ modes (`amp_type_new & 0x0F`):** `0` Off, `1` Twin, `2` Twin II, `3` JC, `4` JC II, `5` Small Speaker, `6` Very Small Speaker, `7` To Rotary, `8` LP24, `9` HP24.
  - Names and order: **VERIFIED** from UI strings at `0x2cc6cc` in OS v1.66 `bank0_CODE.bin` (`Twin Type I/II`, `JC Type I/II`, `Small Speaker`, `Very Small Speaker`, `To Rotary`, `LP24`, `HP24`).
  - Numeric index: **EMPIRICAL**, identical to third-party `ns4decode` (MIT, Randy 2024) on 10 factory programs covering modes 0..9.
  - `Twin` / `JC` / `Small` cabinet voicings (HP/LP/mid-peak Hz and dB): **ASSUMED**, inherited from the earlier 1-pole code. `II` variants are stored (`amp_variation`) but not modelled. `To Rotary` is a pass-through here (routing into the rotary is not implemented). The old `British` model does not exist on hardware and was removed.
  - `LP24` / `HP24`: cutoff = EQ Mid Freq knob, resonance `0..10` = Mid knob (`10 * (60 + 4 * mid_db) / 120`, from `ns4decode`); implemented as two cascaded 2nd-order sections (Butterworth-4 Q `0.5412` / `1.3066`, resonance scales the 2nd Q): topology **ASSUMED**.
- **Overdrive (`Drive`):** Soft-clipping asymmetric saturation stage (`1.0..20.0x` pre-gain with makeup compensation).
- **3-Band EQ:**
  - `Bass`: Low-shelf @ $100\text{ Hz}$ ($\pm 15\text{ dB}$)
  - `Mid`: Peaking bell with sweepable frequency (128 steps, $200\text{ Hz}..8\text{ kHz}$, table `EQ_FREQ_HZ`; EMPIRICAL from `ns4decode`/OLED readings, e.g. idx 64 = 999 Hz, not geometric), $\pm 15\text{ dB}$ in 0.25 dB steps, $Q$ ASSUMED $\approx 1$
  - `Treble`: High-shelf @ $4\text{ kHz}$ ($\pm 15\text{ dB}$)
  - **Implementation:** RBJ 2nd-order biquads (transposed DF-II, per channel, stereo preserved; coefficients computed in `f64`, redesigned only when a parameter changes). Shelf `Q = 1/sqrt(2)`; Mid `Q = firmware_q(64) = 1.018`. Cabinet voicings use 2nd-order Butterworth HP/LP plus a peaking mid band.
  - **Resonance scale (`firmware_q`):** `Q(idx) = 0.1 * 100^(idx/127)`, `idx 0..127`; matches the 128 `f32` entries at `0x2f970c` in OS v1.66 `bank0_CODE.bin` (max rel. error 6.4e-8).
- **Morph (`Wheel` / `A.T.` / `CtrlPed`):** `LayerFxChain::apply_morph` is called per frame by `Stage4Engine::render_frame`. Morphable: `mod1_rate`, `mod1_amount`, `mod2_rate`, `mod2_amount`, `eq_mid_freq`, `amp_drive`, `delay_mix`, `delay_feedback`, `reverb_amount`. Value = `clamp(base + W*dW + P*dP + A*dA, 0, 1)`; slots disabled in the program are not morphed. `delay_rate` morph (14-bit) is not wired yet.

### 2.4 Slot 4 — Delay

- **Tempo & Time:** the `Delay Time` knob is a 128-step hand-crafted curve `DELAY_TIME_MS` (`1500 ms` at step 0 down to `20 ms` at step 127; EMPIRICAL, `ns4decode`; step = top 7 bits of the 14-bit field at block bit 242, matched on all 1,920 factory FX blocks). With `MST CLK` on the same knob selects one of 17 fractions (`DELAY_SYNC_BEATS`: `1/2, 1/4D, 1/2T, 1/4S, 1/4, 1/8D, 1/4T, 1/8S, 1/8, 1/16D, 1/8T, 1/16S, 1/16, 1/16T, 1/32, 1/32T, 1/64`; `S` = swing per the manual, ratio ASSUMED) applied to `MasterClock` BPM (`delay_sync_beats`, overriding `delay_subdivision`).
- **Analog Mode (`delay_analog`):** Smoothly slews read-pointer delay time ($\tau = 80\text{ ms}$) on tempo or rate changes with soft `tanh` tape feedback saturation, producing authentic tape pitch-bend glides.
- **Feedback Path:** Configurable `Feedback` ($0..95\%$), `delay_ping_pong` stereo cross-feed (mono input into alternating Left/Right taps), `DelayFilterMode` (`Off`, `Hp`, `Lp`, `Bp`) feedback loop filters, and `delay_variation` slow tape flutter/cross-shimmer.

### 2.5 Slot 5 — Reverb

- **Algorithms (`ReverbModel`):** `Booth`, `Room`, `Stage`, `Spring` (dispersive spring tap ratios), `Hall`, `Cathedral`.
- **Tone Voicing (`ReverbColor`):** `Normal`, `Bright`, `Dark` high-frequency FDN damping coefficients, plus `Chorale` ($0.75\text{ Hz}$ LFO tap modulation inside the late feedback matrix). A 2-stage stereo allpass input diffuser (`[113, 257]` L, `[149, 311]` R) feeds stereo-decorrelated excitation into an `8192`-sample $4\times 4$ Hadamard FDN with room-model and sample-rate scaled prime tap delays (`Booth..Cathedral`), subtle shimmer modulation, and balanced stereo decorrelation (`0.70 / 0.30`, `0.85 / -0.15`) to eliminate metallic slapback and boxy reflections.

---

### 2.6 FX enum decoding (VERIFIED order, EMPIRICAL bit layout)

Name order is **VERIFIED** from UI strings in OS v1.66 `bank0_CODE.bin` (`0x2cc4e4..0x2cc918`). Bit positions and numeric indices are **EMPIRICAL** (third-party `ns4decode`, MIT, Randy 2024) and were cross-checked on all 384 factory programs: 9 fields x 1,920 FX blocks = 17,280 comparisons, 0 mismatches (`tools/ns4_fx_crosscheck.py`).

| Field | Bits (block offset) | Values (odd = variation) |
|---|---|---|
| Mod 1 mode | 72..=75 | `0/1` A-PAN, `2/3` TREM, `4/5` RM, `6/7` A-WAH, `8/9` WAH, `10/11` PUMP |
| Mod 2 mode | 139..=142 | `0/1` PHAS, `2/3` FLANG, `4/5` CHOR, `6/7` VIBE, `8/9` ENS, `10/11` SPIN |
| Amp mode | 413..=416 | `0` off, `1/2` TWIN, `3/4` JC, `5/6` SMALL, `7` TO ROTARY, `8` LP24, `9` HP24 |
| Delay `Analog` / `Ping Pong` | 332 / 333 | the 2-bit `delay_mode` field is `[analog, ping-pong]` |
| Delay filter type | 334..=335 | `0` Bypass, `1` LP, `2` HP, `3` BP (stored in the bits decoded as `delay_ping_pong`, `delay_fb_filter`) |
| Delay feedback effect | 367..=370 | `0/1` none, `2/3` ENS, `4/5` VIBE, `6/7` CHOR, `8/9` FLAM (20/40 ms), `10/11` SPACE (50/100 wet) |
| Reverb color | 403..=404 | `0` none, `1` DARK, `2` BRIGHT |
| Reverb type | 405..=408 | `0/1` SPRING (Spring2), `2/3` BOOTH (Predelay), `4/5` ROOM (Predelay), `6/7` STAGE (Chorale), `8/9` HALL (Chorale), `10/11` CATHEDRAL (Chorale); bit 408 is the top bit of the 9-bit `amp_type_new` field |

Earlier decoding bugs fixed with this table: Reverb `SPRING` rendered as Booth, Reverb DARK/BRIGHT swapped, Mod 2 `CHOR variation`/`VIBE variation` swapped (raw 5 and 7), Delay Analog/Ping-Pong/filter type read from the wrong bits. `Ensemble` is rendered as Chorus, `Spin` as Vibe, and every Delay feedback effect as one modulated-feedback mode: **ASSUMED** (no dedicated DSP).

**Ensemble, Spin and the Delay feedback effects (what the firmware says).** The DSP algorithms are SHARC code that is not in the `ARM0` firmware image (scans of `bank0_CODE`/`bank1_CODE`, see `docs/OPEN-QUESTIONS.md`), so they cannot be taken from it. The UI strings only carry names and a few parameters: `Ensemble Stereo/Mono` and `Ensemble Deep` (0x2cc63c..0x2cc790), `Spin Treble` / `Spin Bass` (0x2cc65c, 0x2cc668; rotary-speaker style), `Flam 20 ms` / `Flam 40 ms` (delay feedback effect FLAM: a 20 or 40 ms flam tap), `Space 50 Wet` / `Space 100 Wet` (feedback effect SPACE: reverb in the loop at 50 or 100 % wet). These names are VERIFIED; the algorithms behind them are not, so the implementations stay approximations (`ASSUMED`).

**Delay feedback effects (implemented).** `DelayFeedbackFx` (`Off, Ensemble, Vibe, Chorus, Flam, Space`) with `delay_fx_variation` runs on every repeat (heard and fed back). Structure per the manual (section 2.7): Ensemble = three modulated lines 120 degrees apart, stereo, variation mono; Vibe = 100 % wet modulated delay, stereo (quadrature LFOs), variation mono; Chorus = opposite-polarity LFOs per channel (widening), variation deeper with cross-channel blend; Flam = right channel offset 20 ms (variation 40 ms), measured by test; Space = two Schroeder all-passes per channel, 50 % wet (variation 100 %). All rates, depths, base delays, gains and the all-pass lengths are ASSUMED. Mapping from `delay_fb_fx_type`: `2/3` ENS, `4/5` VIBE, `6/7` CHOR, `8/9` FLAM, `10/11` SPACE (odd = variation).

### 2.7 Behaviour documented by the Nord Stage 4 User Manual (v1.6X, vendor document)

Source: `Nord Stage 4 User Manual v1.6X-Edition-N.pdf` (local, `.reference/`), pp. 49-53. Statements are VERIFIED as vendor descriptions; they say what an effect does, not how it is built.

- **EQ:** bass 100 Hz, treble 4 kHz, mid 200 Hz to 8 kHz, +-15 dB (p.51). The corner constants in `fx.rs` therefore have a vendor source.
- **Amp models (p.51):** Twin and JC *boost* the treble range, especially in the non-variation modes; Twin variation = less treble boost and a different mid character; JC variation = reduced treble; Small = Wurlitzer 200A internal speaker, variation = an even smaller amp/speaker. `LP24`/`HP24` are resonant 24 dB/oct filters; Bass, Treble and Drive remain active in these modes (p.52). `TO ROTARY` sends the layer to the Rotary Speaker, which is the last unit in the chain (p.52). Cabinet voicings in `fx.rs` now follow this direction with a high shelf per model and modelled variations (`amp_variation`): Twin +6 dB shelf at 3 kHz (variation +2 dB), JC +6 dB (variation -3 dB, 9 kHz low-pass), Small a band-limited small speaker (140 Hz..3.4 kHz; variation 220 Hz..2.6 kHz). Direction VERIFIED (manual); every frequency and dB value ASSUMED.
- **Mod 1 (p.49):** A-PAN pans between L and R (variation: harder pan waveform); TREM is volume modulation with maximum level at zero amount (variation: square wave); RM multiplies with a sine (variation: stereo version); A-WAH is an envelope-follower wah; WAH is an LFO/pedal resonant low-pass sweep; PUMP is a duck-and-recover amplitude modulation.
- **Mod 2 (p.50):** FLANGER variation = mono; VIBE variation = mono; CHORUS adds a delay line above 50 % amount, its variation is a vintage pedal that goes from lush stereo widening to vibrato; ENS (ensemble) = three cross-connected modulated delay lines, variation = mono; SPIN = rotating-speaker motion without separate rotor/horn, speed changes ramp, variation moves the effect from the treble to the lower frequencies.
- **Delay (p.50-51):** feedback effects ENS/VIBE/CHOR are the Mod 2 counterparts; FLAM = ping-pong with offset L/R channels, variation doubles the shift from 20 to 40 ms; SPACE = reverb-like diffusion growing with each tap, mixed 50 % with the dry sound (variation 100 % wet). Delay filters are applied to every tap; Analog mode adds pitch change on tempo change and slight distortion per tap.
- **Reverb (p.52-53):** SPRING (variation: longer), BOOTH and ROOM (variation: longer pre-delay), STAGE/HALL/CATH (Chorale: more pitch modulation); BRIGHT damps lows, DARK damps highs.

Consequence for Jazz Guitar II (A:78): Mod 2 = `VIBE variation` is a *mono* vibe, so the hardware stereo width (0.96, coherence 0.06 at 1..2 kHz) cannot come from Mod 2; the remaining candidates are the delay feedback `CHOR variation` (stereo widening pedal) and the reverb.

## 3. Master Bus: Global Compressor & Reconstruction Stage

After summing all 7 active layers:
1. **Master Compressor:** RMS/Peak feed-forward stereo bus compressor (`Amount` $0..10$, `Fast` mode $\tau_{\text{atk}} = 1.5\text{ ms}$ / `Normal` mode $\tau_{\text{atk}} = 12\text{ ms}$, ratio $3.5:1$, automatic makeup gain).
2. **Post-DAC Reconstruction Filter:** Models the hardware Burr-Brown `PCM1791A` DAC (`U44`/`U45`) + `NJM4580M` dual op-amp active output stage ([research](../research/hardware-architecture.md)).

## 4. Firmware parity of the biquad arithmetic

Code: `crates/nord-core/src/fx/biquad.rs` (`ext40_ref`, `FW_BUTTERWORTH_FS4_WORDS`, test-only) and
`fx/tests_biquad_golden.rs`.

| Item | Rust | Status |
|---|---|---|
| Form | transposed direct form II, `y = b0 x + s1; s1 = b1 x - a1 y + s2; s2 = b2 x - a2 y` | VERIFIED (FX routine `0x120FDB`, [fx-dsp-map](../research/fx-dsp-map.md) section 6) |
| Firmware arithmetic | `ext40_ref::stage`: every product and sum rounded to 32 significant bits, ties to even (`sinc::ext40::round_sig`); output sample and block-end state stored as the upper 32 bits (truncation to 24 bits); state loaded and stored once per block of 4 frames and stage | VERIFIED: 64 + 64 output words and 12 state words identical to the exact-arithmetic model of `tools/fx_biquad_check.py`, which matches the oracle word for word |
| Fast default | `Biquad::process`: plain `f32`, no extra rounding model, real-time safe | engine path, unchanged |
| Fast vs reference | SNR of the engine `f32` path against the reference, 3 cascaded sections, 8192 samples of noise plus a sine | 135.7 dB (test floor 125 dB) |
| Coefficients | Butterworth low pass at `fs / 4`: `b0 0.292893, b1 0.585786, b2 0.292893 (7 ulp off b0), a1 0, a2 0.171573` | VERIFIED (PM `0x9437A`, same filter at `0x9437F`) |

Golden vectors: `python tools/fx_biquad_check.py --golden` prints the Rust arrays (derived words only, no
firmware code); the test regenerates the input in integer arithmetic.

The Rust master bus (section 3) has no counterpart of the firmware's 4-biquad master IIR (end of the FX pass):
the Butterworth set above is recorded as a test constant only. The compressor, delays, LFOs and resonant
filters stay as they were (`ASSUMED` or `EMPIRICAL` per their doc comments); the firmware routine classes
are listed in [fx-dsp-map](../research/fx-dsp-map.md) and not ported.

### 4.1 EQ gain per count

`EQ_GAIN_DB_PER_COUNT = 0.25 dB` (`fx/tables.rs`, `ASSUMED`) is the single place that decodes and encodes the
7-bit `eq_*` fields (`cbin/fx_preset.rs`). The DSP gain ladder step is `EQ_GAIN_DB_PER_COUNT_LADDER = 20 log10(2) / 24 =
0.250858 dB` (`VERIFIED`, closed form). They differ by 0.34 %, which is 0.05 dB at +-60 counts (15.00 against 15.05 dB). The
link from a preset count to the ladder is not decoded, so the Rust step stays 0.25 dB and the factory range stays +-15 dB
exactly; switching is a one-line change (TD-10).
