# Per-Layer Effects & Audio Routing Specification (`DSPF`)

## 1. Scope

Specifies the 7 independent per-layer effects pipelines (`Organ A`, `Organ B`, `Piano A`, `Piano B`, `Synth A`, `Synth B`, `Synth C`), the global Master Compressor, and the Master Clock.

Implemented in [`crates/nord-core/src/fx/`](../../crates/nord-core/src/fx/mod.rs) and [`crates/nord-core/src/engine/`](../../crates/nord-core/src/engine/mod.rs).

---

## 2. Per-Layer Signal Chain

Every one of the 7 layers processes its stereo audio through an isolated 5-stage FX rack:

```text
Layer Synth/Piano/Organ Output (Stereo)
  └──► [1. Mod 1 (Tremolo / AutoPan / RingMod / Wah / AutoWah / Pump)]
         └──► [2. Mod 2 (Phaser / Flanger / Chorus / Vibe / Ensemble / Spin)]
                └──► [3. Amp Sim & 3-Band EQ (Small / JC / Twin / British + Drive + Bass/Mid/Treble)]
                       └──► [4. Delay (Stereo / Ping-Pong / Analog Tape + HP/LP Feedback Filter)]
                              └──► [5. Layer Reverb (Booth / Room / Stage / Hall / Cathedral / Spring)]
                                     └──► Layer Level & Scene I/II Mute/Crossfade
```

### 2.1 Slot 1 — Modulation 1 (`Mod 1`)

- **`Tremolo`:** Stereo LFO amplitude modulation (`Rate`, `Amount`).
- **`AutoPan`:** Constant-power left/right panning LFO (`Rate`, `Amount`).
- **`RingMod`:** Quadrature carrier multiplier (`Rate` = carrier frequency $20\text{ Hz}..2\text{ kHz}$, `Amount` = wet mix).
- **`Wah`:** LFO/Pedal-swept resonant 2-pole bandpass filter (350 Hz to 2.8 kHz).
- **`AutoWah`** (`fx/mod1_dynamic.rs`, manual p.49): envelope follower (stereo-linked rectified level, attack 5 ms, release 120 ms) sets the sweep s = 1 - exp(-env * g), g = 2 * 10^(2 * norm(Rate)) (the Rate knob is SENS). Standard model: band-pass at 350 * 8^s Hz. VARIATION (different model): resonant low-pass at 150 * 24^s Hz with lower damping. AMOUNT = dry/wet. Louder input opens the filter further (test `awah_louder_input_opens_the_filter_further`).
- **`Pump`** (`fx/mod1_dynamic.rs`, manual p.50): gain = 1 - amt * d(p); phase p advances at the Rate or, with `mod1_sync_beats`, at bpm/60/beats Hz. d rises linearly over min(4 ms * rate, 0.1) of the cycle (quick duck), then recovers as (1-u)^2 (slow recovery). PED mode: one duck per rising edge of `mod1_ped_pos` through 0.5 (re-arm below 0.4), no LFO. The manual documents no VARIATION.

### 2.2 Slot 2 — Modulation 2 (`Mod 2`)

- **`Chorus` / `Vibe`:** Dual modulated fractional delay lines ($7..18\text{ ms}$) with stereo $90^\circ$ phase offset.
- **`Flanger`:** Short modulated delay line ($0.8..6\text{ ms}$) with regenerative feedback.
- **`Phaser`:** 4-stage / 6-stage all-pass cascade swept by LFO.
- **`Ensemble`** (`fx/mod2_spin.rs`, manual p.50): mono input into three modulated lines (centre 9 ms, depth 3 ms, 120 degrees apart) at the Rate; wet 0.6 x AMOUNT; VARIATION = mono.
- **`Spin`** (`fx/mod2_spin.rs`, manual p.51): neutral rotating-speaker style effect, no separate rotor/horn. The Rate is approached with a 0.8 s exponential ramp (`spin_rate_hz`); Doppler delay (0.8 + 0.25 m) * amt ms, then an 800 Hz split with amplitude modulation 1 + depth * amt * m per band, m = sin(phase + ch * 90 degrees). Normal: treble depth 0.6, low 0.15; VARIATION swaps them (movement shifts to lower frequencies).

### 2.3 Slot 3 — Amp Simulation, Drive & 3-Band Semi-Parametric EQ

- **Amp Sim / EQ modes (`amp_type_new & 0x0F`):** `0` Off, `1` Twin, `2` Twin II, `3` JC, `4` JC II, `5` Small Speaker, `6` Very Small Speaker, `7` To Rotary, `8` LP24, `9` HP24.
  - Names and order: **VERIFIED** from UI strings at `0x2cc6cc` in OS v1.66 `bank0_CODE.bin` (`Twin Type I/II`, `JC Type I/II`, `Small Speaker`, `Very Small Speaker`, `To Rotary`, `LP24`, `HP24`).
  - Numeric index: **EMPIRICAL**, identical to third-party `ns4decode` (MIT, Randy 2024) on 10 factory programs covering modes 0..9.
  - `Twin` / `JC` / `Small` cabinet voicings (HP/LP/mid-peak Hz and dB): **ASSUMED**, inherited from the earlier 1-pole code. `II` variants are stored (`amp_variation`) but not modelled. `To Rotary` is a pass-through here (the firmware Leslie is the PNO core 1 processor `0x1256AD`, ported as opt-in `--leslie firmware`, [fx-leslie](../research/fx-leslie.md) s8) (routing into the rotary is not implemented). The old `British` model does not exist on hardware and was removed.
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

### 2.7b Rotary ANGLE: rotor stop position (implemented, `ns4-uo53`)

Manual p.53: with STOP MODE active, Shift + STOP MODE selects ANGLE from a list so the rotor and horn always stop at the same position; the default `Free` leaves the stop position to chance. Course video ORG 10:08-10:22 shows the screen `ROTOR STOP ANGLE` with `Free`, `1: Front Center`, `2: Front Left`, `4: Back Left`, `6: Back Center`.

| Item | Behaviour | Fields / code |
|---|---|---|
| Value | `rotary_stop_angle`: `0` Free, `1..=6` = `STOP_ANGLE_LABELS`; clamped by `clamp_stop_angle` | `organ/rotor_stop.rs`, `OrganLayer::rotary_stop_angle` (mirrored to the TO ROTARY bus) |
| Free | The rotors keep their residual rate (no pull to a fixed phase), so the rest phase depends on where they stopped. Before this change the legacy path always pulled to phase 0 | `organ/rotary.rs` |
| Angle 1..6 | Once Stop is active and a rate is below 0.05 Hz, horn (6/s) and drum (3/s) phases ease along the shortest arc to `STOP_ANGLE_RAD[angle]`; no allocation, deterministic | `ease_phase`, test `stop_angle_parks_horn_and_rotor_at_the_picked_position_from_any_start` |
| Firmware Leslie | The firmware Leslie path (`leslie_fw`, opt-in) does not use the angle: its rotor phase lives in the ported state | not modelled |
| Host | `organy_stop_angle_{1,2}` (int `0..=6`, default 0, additive; `organ_extra_params.rs`) | `param-ids.snapshot` |
| GUI | `OrganEdit::RotaryStopAngle`; popup `panel/rotor_angle.rs` opened by Shift + STOP MODE (a last row keeps the wide/narrow mic toggle, previously the Shift function) | inventory `rotary.stop_angle` |

Provenance: list text `VERIFIED` for the five entries seen on screen; entries 3 (Front Right) and 5 (Back Right) `ASSUMED`; the rest phases (0, +-60, +-120, 180 degrees) are `ASSUMED` and the horn and drum share one phase (no recording of the hardware at each angle).

### 2.8 Variations, PED, TO ROTARY, GLOBAL and GROUP (implemented, `ns4-e3zs`)

Structure VERIFIED by the manual (pp. 48-53); every waveform shape constant below is ASSUMED (no firmware or recording source).

| Item | Behaviour | Fields / code |
|---|---|---|
| Mod 1 VARIATION | Tremolo: square-like wave `tanh(6 x)/tanh(6)`; A-Pan: harder pan `tanh(3 x)/tanh(3)`; Ring mod: right carrier 90 degrees ahead (stereo) | `mod1_variation`, `fx/modulation.rs` |
| Mod 1 PED (Wah) | Filter position = `mod1_ped_pos` (0..1) instead of the LFO | `mod1_ped`, `mod1_ped_pos` |
| Mod 2 VARIATION | Flanger and Vibe: one shared LFO (mono); Chorus: 7 ms base, 6.5 ms depth, 0.6 wet; Phaser (Bi-Phase): feedback 0.6 instead of 0.35, zero at minimum amount | `mod2_variation` |
| Appended Mod types | `Mod1Type::AutoWah`, `Mod1Type::Pump`, `Mod2Type::Spin`, `Mod2Type::Ensemble` are appended after the existing variants (host indices stable); see 2.1 / 2.2. Provenance: every constant is ASSUMED or EMPIRICAL (generic envelope-filter, side-chain pump and rotary-speaker practice: Wikipedia "Envelope follower", "Auto-Wah", "Dynamic range compression", "Leslie speaker", "String ensemble"); the firmware DSP of these types is not decoded. Not yet wired from `.ns4p` presets (the Fx1/Fx2 type map in `cbin/fx_preset.rs` is unchanged) | `mod1_dynamic.rs`, `mod2_spin.rs` |
| Mod 1 Master Clock | `mod1_sync_beats: Option<f32>` replaces the Rate of A-Pan / Trem / Wah / Pump by `bpm / 60 / beats` Hz (the `bpm` passed to `process_frame_with_bpm`); default `None`; exposed to hosts as `fxz_mod1_beats_*` (same 17 fractions as the delay) | `fx/modulation.rs`, `fx_variation_params.rs` |
| Reverb VARIATION | `reverb_variation` (default off, bit-identical): Spring feedback gain 0.77 to 0.86 (about 1.7 times the decay, manual p.52 "longer"; ASSUMED), Booth and Room 35 ms pre-delay on the reverb input only (manual p.52-53 "longer pre-delay"; ASSUMED). Stage, Hall and Cathedral keep CHORALE (`ReverbColor::Chorale`). Exposed to hosts as `fxz_rev_var_*`; carried by GROUP / GLOBAL copies and `FxParams` | `fx/reverb_variation.rs` |
| TO ROTARY | A Piano / Synth layer (index 2..6) with `AmpCabinet::ToRotary` bypasses the master mix: its post-fader mono sum feeds one shared rotary instance (`rotary_bus`) whose speed, drive, mic and stop settings mirror `organ_a`. Organ layers keep their built-in rotary | `engine/mod.rs`, `OrganLayer::mirror_rotary_settings`, `process_external_rotary` |
| GROUP | `FxSharing::piano_group` / `synth_group` (source layer): `copy_all_settings_from` copies every user setting to the other layers of the section (DSP state stays per layer) | `engine/sharing.rs`, `fx/sharing.rs` |
| GLOBAL Delay / Compressor | `FxSharing::delay_global` / `comp_global`: the source layer's settings are copied to all layers, each keeps its own instance | same |
| GLOBAL Reverb | `FxSharing::reverb_global`: every layer's reverb is bypassed (`bypass_reverb`) and one shared reverb (`global_reverb`, settings of the source layer) runs on the summed mix after the rotary and the level faders, before the master bus | `render_frame` |
| Master Clock division | `delay_sync_beats` (17 hardware fractions) is exposed to hosts | `fxy_delay_beats_*` |
| Reverb BRIGHT / DARK / CHORALE | already present as `ReverbColor` (`fxsel_rev_color`) | |

`Stage4Engine::apply_fx_sharing` runs at the start of `process_block` (field copies only). Callers that drive `render_frame` directly call it after changing settings. All modes default off, so default rendering is bit-identical (existing golden tests unchanged). Limitation: the TO ROTARY bus keeps its own rotor phase, independent of the organ's rotor.

### 2.9 Standalone panel: Layer Effects bay (`ns4-hcuh`, B8f)

`crates/nord-gui/src/panel/bay_effects/` draws all 51 inventory ids of the Effects section at their `nord_panel::layout` rectangles and edits the focused FX slot through `AudioCommand::Fx(FxEdit)` (`audio/edits/fx.rs`; allocation-free `apply`, every value clamped).

| Item | Behaviour |
|---|---|
| FX slots | `0` Organ (engine chains 0 **and** 1: one shared chain, manual p.19), `1`/`2` Piano A/B (chains 2/3), `3`/`4`/`5` Synth A/B/C (chains 4/5/6). `Stage4PanelState::fx_focus` keeps its meaning (`0` Organ, `1` Piano, `2..=4` Synth A/B/C); `FxUi::piano_b` is the Piano layer |
| FX FOCUS | ORGAN / PIANO / SYNTH buttons; pressing the focused Piano or Synth button again steps to its next Layer. LEDs: one per Layer (Organ, Piano A/B, Synth A/B/C); GROUP lights the whole Section |
| ON buttons | The engine has no ON switch per unit: Mod 1 / Mod 2 send type `Off`, Amp sends cabinet `Off` with zero drive and EQ, Reverb sends model `Off`; LAYER EFFECTS ON gates all of them. TO ROTARY is a routing and survives Amp OFF |
| Shift / hold | A button with a shifted function treats Shift (keyboard) + click, or a press held for 500 ms (`nord_panel::behaviour::HOLD_MS`), as the shifted press (VARIATION / PED, GLOBAL, GROUP, PING PONG, ANALOG). Shift + turn of TEMPO enters MST CLK (17 divisions, `DELAY_SYNC_BEATS`), Shift + turn of COMP AMOUNT sets FAST |
| GROUP / GLOBAL | `FxUi` keeps the source slot of each mode and sends `FxEdit::Sharing`. Editing another Layer while a mode covers it re-points the source to that Layer, otherwise the engine copy would undo the edit |
| Mod 1 PED | Wah and Pump: Shift + selector toggles PED and the RATE knob becomes the static pedal position (`mod1_ped_pos`; Wah filter position, Pump ducks once each time it rises through 0.5). A-WAH keeps VARIATION on the Shift function and its RATE knob is SENS; the other types have VARIATION |
| Mod 1 MST CLK | `mod1.mst_clk` (Shift + turn RATE, manual p.50): A-PAN, TREM, WAH and PUMP follow the Master Clock with the delay's 17 divisions (`mod1_sync_beats`); dimmed for RM and A-WAH, which the engine does not clock |
| Filter RES | UI `0..1`, sent as `0..10` (`eq_filter_res`, engine range). The plugin parameter `fxx_filter_res_*` is `0..1` and is written unscaled, so it only reaches the lowest tenth of the range (plugin follow-up, not changed here) |
| Selector LEDs | Every LED of both selectors has an engine type: A-WAH, PUMP, ENS and SPIN are `Mod1Type::AutoWah`, `Pump`, `Mod2Type::Ensemble`, `Spin`. The panel keeps the printed LED order (A-PAN, TREM, RM, A-WAH, WAH, PUMP; PHAS, FLANG, CHOR, VIBE, ENS, SPIN) and maps LED to type by name, because the engine and host enums append the later types and so differ in order. Their DSP is ASSUMED (`mod1_dynamic.rs`, `mod2_spin.rs`) |
| Still dimmed | Amp VARIATION (stored by the engine, no distinct voicing yet), TO ROTARY on the Organ slot (inert), Mod 1 MST CLK on RM and A-WAH |
| UI-only | Per-unit ON state, LAYER EFFECTS ON, focus, GROUP / GLOBAL sources and tap tempo live in `FxUi`; after a program load the legacy `mod1_type`, `delay_on`, `reverb_model` and knob fields are adopted into the focused chain (`StateEcho`), the other five slots keep their last panel view |

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

The master bus has an optional firmware 4-section IIR (`fx/master_iir.rs`, `MasterBus::master_iir_enabled`,
default `false`, so the render hash does not change). Sets (`fx/fw_coeffs.rs::MASTER_IIR_WORDS`, PM
`0x945F2..0x94605`, palindromic numerators) are `VERIFIED` firmware words (20 words dumped from the Core2 flat
SRAM image, 0 differing). Together they are a flat low pass: -0.0001 dB up to 5 kHz, about -12 dB at 12 kHz, under
-50 dB at 20 kHz at 48 kHz (`EMPIRICAL`, computed in a unit test). Not `VERIFIED`: that the four sets are a
plain cascade in memory order and where the inline code stores to memory (`ASSUMED`; the oracle was run on
`0x120FDB` only), and the bus position (before the compressor, `ASSUMED`). Test:
`python tools/fx_biquad_check.py --golden-master` prints the golden words for `tests_master_golden.rs`
(ext40 reference bit-exact; the fast f32 path stays within 1e-5 of it). The pair at `0x9437A` (fs/4
Butterworth) is a different table, not part of the master IIR. The compressor, delays, LFOs and resonant filters
stay as they were (`ASSUMED` or `EMPIRICAL` per their doc comments); the firmware routine classes are listed in
[fx-dsp-map](../research/fx-dsp-map.md) and not ported.

### 4.0 Firmware Amp biquad sets

`fx/fw_coeffs.rs::AMP_FW_WORDS` holds the two cascaded sets (`b0 b1 b2 a1 a2`, TDF II) the ARM Amp type switch
`0xC021D064` sends for cases 0..9 (`VERIFIED` words, `python tools/fx_chain_install.py amp ...`,
[fx-chain-install](../research/fx-chain-install.md) s5): cases 0..3 boost sections, case 4 identity, case 5 high
pass then low pass, cases 6..9 none. Pole radii are inside the unit circle (unit test). `LayerFxChain::amp_fw_coeffs`
(default `false`) replaces the RBJ cabinet by the case-5 sets for `Small Speaker` (non-variation) only; this mapping
is `ASSUMED` (shape fit: -33 dB at 50 Hz, -39 dB at 20 kHz, +4.4 dB at 1 kHz, 48 kHz). The order of cases 0..3 against
`Twin`/`JC` is not decoded, so they stay unmapped and the default path is unchanged.

### 4.0a Firmware compressor gain computer (opt-in)

Code: `fx/fw_compressor.rs` (`FwCompressor`, context offsets in `fw_compressor_ctx`), `fx/fw_stage.rs`,
`MasterBus::set_fw_compressor`, golden vector `fx/tests_compressor_golden.rs`. Reference model and equations:
[fx-type-map](../research/fx-type-map.md) s7 (`tools/fx_routine_models.py`).

| Item | Rust | Status |
|---|---|---|
| Algorithm | block of 4 stereo frames: linked peak, release-smoothed level, `log2 = e + frac` estimate, threshold, attack/release excess smoother, linked reduction, `FIX` and a float-bit-pattern interpolation `T0 - T0 t + T1 t` (fractional `SSF` products), makeup smoother | VERIFIED: model equals the oracle on 2,100 random scenarios and a 6-block golden scenario, 0 differing words |
| Precision | f32 (the DSP uses 40-bit registers and truncating stores) | worst relative output error 2.1e-7 against the golden vector (test bound 1e-6) |
| Context layout | 26 words, offsets from `I1` (`-0x20 .. -0x07`); loads through `I1` are broadcast to both elements, the left threshold word overlaps the right makeup word | VERIFIED (oracle) |
| Parameter values | `FwCompressor::new()`: unity makeup, threshold far above any level (no reduction), `k = 0.9908`, `T0 = 1.0`, `T1 = 0.5` | ASSUMED; the ARM words behind `kCmpAmt` (0..127, default 95) and `kCmpResponse` are not decoded |
| Bus position and latency | replaces the peak-envelope compressor when set; 4 frames of latency (block collection) | ASSUMED position; default off, `render_hash` unchanged (`cb8d83b7...`) |

### 4.0b Firmware routine ports: delay, amp drive, all-pass, tone stack (opt-in)

Code: `fx/fw_delay.rs` (`FwDelay`, `DelayIo`), `fx/fw_amp_drive.rs` (`FwAmpDrive`, `DriveTables`), `fx/fw_allpass.rs`
(`FwAllpass`), `fx/fw_tone_stack.rs` (`FwToneStack2`, `FwToneStack4`), shared `fx/fw_words.rs` and
`fx/fw_short_float.rs`; golden vectors `fx/tests_{delay,lfo,allpass,resonant}_golden.rs` (the file names keep the model names). Role correction ([fx-contexts](../research/fx-contexts.md) s5.2, s5.3): `0x1228CF` is the Amp **drive** stage (`FwAmpDrive`, phase word = `kAmpDrive / 127`) and `0x1243F4` / `0x124497` the Amp **tone stack** (`FwToneStack2/4`). Equations, field tables and
the verification record: [fx-routine-models](../research/fx-routine-models.md).

| Item | Rust | Status |
|---|---|---|
| Algorithms | the five FX routines `0x1206B0`, `0x1228CF`, `0x122C39`, `0x1243F4`, `0x124497` as straight-line f32 code over the firmware's context words | VERIFIED: models equal the oracle on 2,200 to 4,600 random scenarios each and register for register on the trace |
| Precision | f32 (the DSP uses 40-bit registers and truncating stores) | worst relative error against the golden vectors 3e-6 (resonant), 3e-6 (all-pass), 1.5e-5 (LFO), below 1e-6 (delay), packed delay words exact |
| Wiring | exported types; the delay mix law and the compressor are wired through `FirmwareFx` (s4.0c), the rest only have parameter fills (`fw_params`) | default `render_hash` unchanged |
| Parameter values | none shipped (`Default` is an all-zero context) | ASSUMED roles of the words (EMPIRICAL where an equation shows the role) |

Module map of the FX library (which module owns which routine pcs, 28 ARM descriptor lists for all 100 code
records) and the finding that the Amp unit's type values are coefficient data and a mode word, not routine
pointers, are in [fx-type-map](../research/fx-type-map.md). Parity to-do list: (1) map `Edt_Effects_kFx1Type`,
`kFx2Type`, `kRvbType`, `kDlyFbFxType` to their handlers (blocked on the part constructor chain), (2) reference
models and ports for the delay `0x1206B0`, table LFO `0x1228CF`, phaser `0x12379B` / all-pass `0x122C39` and
the resonant filter `0x1243F4`, (3) the 16-bit packed-float delay memory with the `0.9908` smoother once the delay
routine is modelled, (4) legacy Amp types: the 12-way handler `0xC020506C` sends a 130-word FIR and up to 12
biquad sets per type (`fx-type-map` s4).

### 4.0c Firmware parameter maps and the `FirmwareFx` switch (opt-in)

Code: `fx/fw_map.rs` (closed forms, `pub mod fw_map`), `fx/fw_delay_time.rs` + `fx/fw_delay_table.rs` (delay time law, `pub mod fw_delay_time`), `fx/fw_params.rs` (fills of the stage contexts), `fx/firmware_fx.rs`
(`FirmwareFx`, `LayerFxChain::firmware_fx`), `fx/master.rs` (`MasterBus::use_fw_compressor_amount`). Source:
[fx-contexts](../research/fx-contexts.md) s5 and s8, `tools/fx_forms.py`. Tests: `fx/tests_fw_map.rs` against the captured series words in
`fx/tests_fw_series.rs` (copied from `docs/research/data/fx-param-fits.csv`, error below 1e-5 of the series range), `fx/tests_firmware_fx.rs`.

The type value selects a module and data picks the variant (two types per module, `fx-type-modules.csv`); the Rust enums keep one
parameter set per engine mode. The engine's `0..=1` fields are the presets' `raw7 / 127` (`cbin/fx_preset.rs`), so the firmware UI value is
`v = 127 x` (`fw_map::unit_to_ui`, scale equality `ASSUMED` for the layer fields); EQ dB maps to counts with `EQ_GAIN_DB_PER_COUNT`.

| Id | Context word | Form (v = UI value) | Wired | Tags |
|---|---|---|---|---|
| `kDlyMix` | send `c[-0x1B]`, dry `c[4]` | `min(1,(v/64)^2)`, `min(1,(2-v/64)^2)` | layer delay when `FirmwareFx::delay_laws` | words VERIFIED (11 values), roles EMPIRICAL |
| `kDlyFbkAmt` | `c[-0x1A]`, `c[-0x19]` | `v/127` (= the engine scale; the engine keeps its 0.95 clamp) | identity, no change | VERIFIED words |
| `kDlyRateClkDiv` (14 bit) | tap `c[0x15]`, old tap `c[0x16]`, cross-fade step `c[0x19]` | `c[0x15] = min(61438, floor(43.402339 k))`, `k` = fractional index in the 1501-entry list `fw_delay_table.rs`; first echo `trunc(c[0x15]) + 2` samples (18.1 ms to 1.280 s) | free-running delay when `FirmwareFx::delay_time` and `LayerFxChain::delay_rate_raw` is set; tempo sync and the 45 ms tap cross-fade stay the engine's | words VERIFIED (264 values), DSP echo law VERIFIED (oracle), constant `K` interval only, `c[0x14]` mode and ring length ASSUMED ([fx-delay-time](../research/fx-delay-time.md)) |
| `kCmpAmt` | threshold `c[-0x1B]`, makeup target `c[-0x1C]` | `-36 v/127`, `1 + 5.72 (v/127)^2` | layer (`FirmwareFx::compressor`) and master (`use_fw_compressor_amount`) | words VERIFIED, roles EMPIRICAL |
| attack / release pairs | `c[-0x20..-0x1D]` | `(0.102071, 1.0)`, `(4.607e-4, 0.04)` for every value | with `kCmpAmt` | VERIFIED words |
| compressor slope, log gain | slope `c[-0x17]`, `c[-0x0B]` | `0.05` (`CMP_SLOPE`), `1.0` | with `kCmpAmt` | ASSUMED (base words of the Cmp type switch not captured) |
| `kCmpResponse` | none | the handler ignores the value (same four constants, 4 states tried) | no (engine `comp_fast` keeps its legacy meaning) | closed: no DSP effect ([fx-reverb-leslie](../research/fx-reverb-leslie.md) s4) |
| `kRvbAmt` | send `0xC02BB44C` word `-0x1F`, dry word `-0x1E` | `x^2 (2 - x)` with `x = v/128` (1 at 127), `sqrt(1 - wet^2)` | reverb dry and send when `FirmwareFx::reverb_mix`; the reverb network itself stays the engine FDN | words VERIFIED (all 128 values), role EMPIRICAL |
| Reverb types 0, 1 routines | module `0xC02BB54C` | `0x12417E` / `0x12435C`: fs/4 Butterworth bank, hold, second bank, `out = in c[0x15] + wet` around the core `0x123F61` | `FwReverb` = `FwRvbWrap` + the generated core (`fw_rvb_core.rs`, `fw_rvb_steps.rs`), `FirmwareFx::reverb`, Spring (type 0, variation type 1), latency 4 frames; types 2..11 are the next row | wrapper and core VERIFIED word for word against the oracle (`tests_fw_rvb_golden.rs`: impulse and noise, types 0 and 1, 3000 calls each, 40-bit path; the `f32` path is 95 to 120 dB below the signal); real-instance data in the image and oracle responses ([fx-reverb-leslie](../research/fx-reverb-leslie.md) s5, s6, s8) |
| Reverb types 2..11 routines | module `0xC02BB44C` | wrapper `0x120FDB` (three fs/4 biquads, the core, three more biquads, mix) around the packed multi-tap core `0x120D8A` (16:16 short-float line words, SDRAM line of 20,480 words, DM ring of 3,140 words) | `FwReverbMulti` = generated steps (`fw_rvbm_steps_core.rs`, `fw_rvbm_steps_wrap.rs`, `tools/fx_rvbmulti_rust_gen.py`) on the register machine of `fw_leslie_core.rs`; `FirmwareFx::reverb` for Booth 2/3, Room 4/5, Stage 7 (6 has no capture), Hall 8/9, Cathedral 10/11, latency 4 frames | VERIFIED word for word against the oracle (`tests_fw_rvbm_golden.rs`: impulse at 127 and noise at 63, 9 types, 3600 calls each, 40-bit path; `tools/fx_rvbmulti_check.py` traces 9,660 of 9,660 steps per type); the model-to-type map and `kRvbAmt` between the captured points are ASSUMED ([fx-reverb-leslie](../research/fx-reverb-leslie.md) s14) |
| `kAmpDrive`, `kAmpTypeNew` 1..6 | `c[-0x20]` of `FwAmpDrive` (`v/127`); type words `c[-0xD..-0xB]`, sections `c[7..0xB]`, `c[0x10..0x14]` (`fx/fw_amp_types.rs`, `docs/research/data/fx-amp-types.csv`) | `v/127`; type = amp mode (1 Twin, 2 Twin II, 3 JC, 4 JC II, 5 Small, 6 Small II) | layer amp when `FirmwareFx::amp` (`fx/fw_amp_stage.rs`, tables `fx/fw_drive_tables.rs`): replaces the engine `tanh` drive and RBJ cabinet for those six modes, 4 frame latency; the firmware cabinet FIR and the organ tone routines are not modelled, so the engine EQ and compressor still follow. Vox with the stage alone: mid +8.0 to -2.5 dB but presence -4.9 to -9.6 dB and RMS +11.4 dB, so it stays opt-in ([fx-type-map](../research/fx-type-map.md) s8) | words, tables and algorithm VERIFIED against the oracle (`tests_amp_golden.rs`, worst 1.4e-6 and 1.7e-4 of the peak); cabinet and level staging open |
| `kAmpTreble`, `kAmpBass` | `F c[-0x1B]`, `E c[-0x1C]`, `A c[-0x20]`, `a c[-0x14]` | 13-point tables, linear in the word between points | fill only (`set_tone_stack`): the other tone-stack words come from the Amp type switch | words VERIFIED at the 13 counts, interpolation and unit tables (piano) EMPIRICAL |
| `kAmpMid`, `kAmpMidFreq` | `C D B` and `0xC044398C` words | not mapped | no | open |
| `kFx1RateClkDiv`, `kFx1Amt` (types 2, 3) | `c[-0x20]`, wet `c[-0x1A]` / `c[-0x16]`, dry `c[-0x19]` | `12800 + 16384 v`, `v/254`, `1 - v/254` | fill only (`set_allpass`): Fx1 type to engine `Mod1Type` map and base words open | VERIFIED words |

`FirmwareFx` is `Default` = `FirmwareFx::DEFAULT` (only `amp` on; every other switch off, `render_hash` unchanged by them, below). `delay_time` replaces the engine's `delay_time_ms` by `echo_samples_at(v, fs)` (`fx/delay.rs`, `target_delay_samples`) in the free-running delay when the program's raw 14 bit value is present (`engine/program_fx.rs` sets `delay_rate_raw`); it falls back to the engine time in tempo sync. Tests `fx/tests_fw_delay_time.rs` (first echo at sample 870 and 46500, fallbacks), `fx/fw_delay_time.rs` (34 captured words, plateaus, monotone), `tools/tests/test_fx_delay_time.py` (264 words, table fingerprint, oracle arguments). The engine's `DELAY_TIME_MS` table (`1500 ms .. 20 ms`, 128 steps) disagrees with the firmware law and stays the default. **Latency:** the firmware compressor works on blocks of 4 stereo frames, so
the layer or master path gains 4 frames (83 us at 48 kHz) while it is on; the layer stage keeps running with amount 0 (unity) when
`comp_enabled` is false so the latency does not depend on the compressor switch. Measured (687 Hz sine, 0.5 peak, RMS of the second half-second): amount 0 gives
the unity RMS 0.3536, amount 0.5 gives 0.677 (+5.6 dB), amount 1.0 gives 1.337 (+11.6 dB): the makeup `1 + 5.72 u^2` dominates, because the
slope and log gain are `ASSUMED`. There is no hardware recording to compare against, so the end-to-end comparison does **not** support turning
`FirmwareFx` on by default.

`FirmwareFx::reverb` (opt-in, `ns4 render-patch --firmware-reverb`, also in `--firmware-fx`) runs the firmware reverb of `kRvbType` 0 and 1 for `ReverbModel::Spring` (the variation flag `LayerFxChain::reverb_variation` picks type 1; `engine/program_fx.rs` sets it from the program): `FwReverb` = the half-rate wrapper `0x12417E` (`fw_rvb_wrap.rs`, generic over `Fp`) around the core `0x123F61` generated from the instruction model (`tools/fx_rvbcore_gen.py` to `fx/fw_rvb_steps.rs`, machine and memory in `fx/fw_rvb_core.rs`, 40-bit arithmetic `fx/fw_ext40.rs`). The routine mixes dry and wet itself from the `kRvbAmt` words (`fw_map::reverb_send` / `reverb_dry` into `c[0x12]`, `c[0x13]`), so `reverb_mix` is not applied on top; latency is 4 frames; no allocation in the audio path after the first use (the instance, 15 k words, is built on first use like `fw_amp`). The other models run the multi-tap routine below (Stage without the variation flag keeps the engine FDN). Golden: `fx/tests_fw_rvb_golden.rs` (oracle output, impulse and noise, types 0, 1, 3000 calls, every word equal on the 40-bit path), `fx/tests_fw_reverb_chain.rs`, `render_opts` flag test. Default off: the video comparison (`fx_reverb`, `organ_b3`, `fx_amp` segments of `tools/emu_vs_hw.py`) shows a 0.75 dB closer spectrum at amount 0.62, neutral at 0.2, a narrower stereo image, and no measurable hardware decay ([fx-reverb-leslie](../research/fx-reverb-leslie.md) s10).

`FirmwareFx::amp` (s4.0c table above) swaps the amp drive and cabinet for the firmware drive stage in the Twin, JC and Small families. Tests: `fx/tests_fw_amp_chain.rs` (off is bit identical, on differs and is finite with a 4 frame latency, Off / To Rotary / LP24 / HP24 keep the engine path, type changes rebuild the stage), `fx/fw_amp_stage.rs` (oracle golden for Twin at `kAmpDrive` 14 and JC at 100), `fx/fw_amp_types.rs`, `fx/fw_drive_tables.rs`. `FirmwareFx::amp` is **on by default** since the cabinet is modelled (`fx/fw_cabinet.rs`, measured impulse response of `0x122A1D`, oracle chain golden `fx/tests_cabinet_golden.rs`, Vox score 3.98 to 1.30, RMS +0.5 dB; [fx-type-map](../research/fx-type-map.md) s10, s11; `render_hash` now `c7f06ca7ff6f8663be3bf2af772c6d6854f23593808ff3382d1895cf5f5ddbd5`, D22); `render-patch --amp-path engine` or `FirmwareFx::OFF` restore the engine amp. The Leslie drive has no firmware counterpart to wire (`v/127` into a null-stub row, [fx-type-map](../research/fx-type-map.md) s9); the engine rotary keeps `tanh((1 + 3 drive) x)` by default; `--rotary-drive unity` (`OrganLayer::rotary_drive_unity`) selects `tanh(g x) / g` (s11).

Stability tests (`tests_firmware_fx.rs`, `master.rs`): silence in gives exact silence out, loud (1.5) and quiet (1e-3) alternating input
stays finite, bounded by the makeup and free of denormals, for amounts 0, 0.3, 1.

### 4.1 EQ gain per count

`EQ_GAIN_DB_PER_COUNT = 0.25 dB` (`fx/tables.rs`, `ASSUMED`) is the single place that decodes and encodes the
7-bit `eq_*` fields (`cbin/fx_preset.rs`). The DSP gain ladder step is `EQ_GAIN_DB_PER_COUNT_LADDER = 20 log10(2) / 24 =
0.250858 dB` (`VERIFIED`, closed form). They differ by 0.34 %, which is 0.05 dB at +-60 counts (15.00 against 15.05 dB). The
link from a preset count to the ladder is not decoded, so the Rust step stays 0.25 dB and the factory range stays +-15 dB
exactly; switching is a one-line change (TD-10).
