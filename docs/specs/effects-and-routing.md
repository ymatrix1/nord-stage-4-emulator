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

- **Tempo & Time:** Free millisecond mode (`delay_time_ms` $10\text{ ms}..2000\text{ ms}$) or `MasterClock` BPM subdivision sync (`delay_clock_sync` with `DelaySubdivision`: `Quarter`, `DottedEighth`, `Eighth`, `Sixteenth`).
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

## 3. Master Bus: Global Compressor & Reconstruction Stage

After summing all 7 active layers:
1. **Master Compressor:** RMS/Peak feed-forward stereo bus compressor (`Amount` $0..10$, `Fast` mode $\tau_{\text{atk}} = 1.5\text{ ms}$ / `Normal` mode $\tau_{\text{atk}} = 12\text{ ms}$, ratio $3.5:1$, automatic makeup gain).
2. **Post-DAC Reconstruction Filter:** Models the hardware Burr-Brown `PCM1791A` DAC (`U44`/`U45`) + `NJM4580M` dual op-amp active output stage ([research](../research/hardware-architecture.md)).
