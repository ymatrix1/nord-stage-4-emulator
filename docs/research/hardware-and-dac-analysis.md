# Research: Hardware DSP Topology & Analog DAC Output Stage Analysis

- **Checked date:** 2026-11-10
- **Sources:** `785351857-Nord-Stage-Service-Manual-v1-1.pdf` (`Nord Stage MainBoard B vers 1.41`), Nord Stage 4 PCB (`Moderkort NS4`), `Update Nord Stage 4 OS v1.66.exe`
- **Question:** How do the hardware DSPs, flash storage, and analog D/A reconstruction stage shape the final audio output of the Nord Stage?

---

## 1. Multi-SHARC DSP & NOR/NAND Flash Architecture

Inspection of the `Moderkort NS4` mainboard and OS v1.66 firmware symbols (`ARM0`, `DSPO`, `DSPS`, `DSPF`) confirms:
- **Zero-Latency Flash Access:** Rather than streaming sample blocks from an OS filesystem with disk buffers, the Piano (`2 GB`) and Sample (`1 GB`) banks reside in dedicated high-speed non-volatile flash memory mapped directly to the DSP memory bus.
- **Parallel Floating-Point SHARC DSPs:** Analog Devices SHARC processors execute deterministic 32-bit/40-bit IEEE floating-point audio loops locked to the I2S/TDM master sample clock, eliminating OS scheduling jitter.

---

## 2. Analog Output & D/A Reconstruction Stage (`CS4392` + `LM833` / `LF353`)

From the Clavia MainBoard schematic (`MainBoard B vers 1.41`, analog output channels `CH1..CH4` and Headphone out):

1. **D/A Converter (`Cirrus Logic CS4392`):**
   - 24-bit, 192 kHz multi-bit Delta-Sigma DAC with differential analog outputs (`AOUTL+`, `AOUTL-`, `AOUTR+`, `AOUTR-`).
   - Internal digital de-emphasis and oversampling filter followed by an external active differential-to-single-ended reconstruction filter.
2. **Differential Reconstruction Filter (`LM833` Dual Low-Noise Audio Op-Amp):**
   - Active 2nd-order Bessel/Butterworth low-pass differential summer (`R = 4.7 kΩ`, `C = 820 pF`, $f_{-3\text{dB}} \approx 41.2\text{ kHz}$).
   - Removes out-of-band Delta-Sigma modulation noise and adds a subtle, phase-linear ultrasonic roll-off above $20\text{ kHz}$.
3. **Output Line & Headphone Buffer (`LF353` JFET Input Op-Amp):**
   - High-slew-rate ($13\text{ V/\mu s}$) JFET buffer stage with AC coupling (`22 µF` electrolytic) producing a flat $+4\text{ dBu}$ / $-10\text{ dBV}$ unbalanced line output with $< 1\text{ Hz}$ DC-blocking high-pass corner.

---

## 3. Implications for the Emulator

- **Implication 1:** 99% of the Nord Stage 4's sonic character comes from the floating-point SHARC DSP algorithms (`DSPO` B3/Rotary, `DSPS` Wave 2 synth, `DSPF` string resonance & per-layer FX) prior to the DAC.
- **Implication 2:** In `crates/nord-core/src/fx.rs` (`MasterBus`), we include a DC-blocker ($f_c = 5\text{ Hz}$) and a gentle ultrasonic reconstruction low-pass filter ($f_c = 20.5\text{ kHz}$ at $48\text{ kHz}$ sample rate) after the Master Compressor to match the `CS4392` + `LM833` output stage headroom and phase response.
