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

- **Amp Cabinet Models:** `Off` (clean tube drive only), `Small` (combo speaker bandpass), `JC` (solid-state bright stereo), `Twin` (mid-scoop American cabinet), `British` (warm mid-forward stack).
- **Overdrive (`Drive`):** Soft-clipping asymmetric saturation stage (`1.0..20.0x` pre-gain with makeup compensation).
- **3-Band EQ:**
  - `Bass`: Low-shelf @ $100\text{ Hz}$ ($\pm 15\text{ dB}$)
  - `Mid`: Peaking bell with sweepable frequency ($200\text{ Hz}..8\text{ kHz}$, $\pm 15\text{ dB}$, $Q = 1.0$)
  - `Treble`: High-shelf @ $4\text{ kHz}$ ($\pm 15\text{ dB}$)

### 2.4 Slot 4 — Delay

- **Tempo & Time:** Free millisecond mode (`delay_time_ms` $10\text{ ms}..2000\text{ ms}$) or `MasterClock` BPM subdivision sync (`delay_clock_sync` with `DelaySubdivision`: `Quarter`, `DottedEighth`, `Eighth`, `Sixteenth`).
- **Analog Mode (`delay_analog`):** Smoothly slews read-pointer delay time ($\tau = 80\text{ ms}$) on tempo or rate changes with soft `tanh` tape feedback saturation, producing authentic tape pitch-bend glides.
- **Feedback Path:** Configurable `Feedback` ($0..95\%$), `delay_ping_pong` stereo cross-feed (mono input into alternating Left/Right taps), `DelayFilterMode` (`Off`, `Hp`, `Lp`, `Bp`) feedback loop filters, and `delay_variation` slow tape flutter/cross-shimmer.

### 2.5 Slot 5 — Reverb

- **Algorithms (`ReverbModel`):** `Booth`, `Room`, `Stage`, `Spring` (dispersive spring tap ratios), `Hall`, `Cathedral`.
- **Tone Voicing (`ReverbColor`):** `Normal`, `Bright`, `Dark` high-frequency FDN damping coefficients, plus `Chorale` ($0.75\text{ Hz}$ LFO tap modulation inside the late feedback matrix).

---

## 3. Master Bus: Global Compressor & Reconstruction Stage

After summing all 7 active layers:
1. **Master Compressor:** RMS/Peak feed-forward stereo bus compressor (`Amount` $0..10$, `Fast` mode $\tau_{\text{atk}} = 1.5\text{ ms}$ / `Normal` mode $\tau_{\text{atk}} = 12\text{ ms}$, ratio $3.5:1$, automatic makeup gain).
2. **Post-DAC Reconstruction Filter:** Models the hardware Burr-Brown `PCM1791A` DAC (`U44`/`U45`) + `NJM4580M` dual op-amp active output stage ([research](../research/hardware-architecture.md)).
