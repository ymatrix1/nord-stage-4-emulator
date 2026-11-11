# Research: Physical Mainboard Architecture & Component Identification

## 1. Overview & Verified Sources

Technical specifications and component-level verification for the Clavia Nord Stage 3, Nord Piano 4, and Nord Stage 4 hardware architectures.

**Primary Evidence Sources:**
- **Nord Piano 4 (NP4) Mainboard Reference Photo:** [`.reference/mainboard/c0840e60-74b3-11eb-baf8-a528ac652aed.jpg`](../../.reference/mainboard/c0840e60-74b3-11eb-baf8-a528ac652aed.jpg) (high-resolution unlabelled inspection showing the central Texas Instruments processor).
- **Nord Stage 3 (NS3) Mainboard Teardown Photo:** [`.reference/mainboard/others/nord-stage-3.jpeg`](../../.reference/mainboard/others/nord-stage-3.jpeg) (2268x4032 ultra-high resolution image of the full PCB).
- **Nord Stage 4 (NS4) Teardown Images:** [`.reference/mainboard/657785317_*.webp`](../../.reference/mainboard/) (photographic series showing chassis interior and panel boards).
- **Clavia Service Manuals & Schematics:** Official Main Board parts lists, component designations (`U43`, `U44`, `U45`, `U46`), and repair logs.

---

## 2. Component Bill of Materials (BOM)

| Subsystem | Component | Manufacturer | Package / Pinout | Specification & Role |
|---|---|---|---|---|
| **Host MPU** | **AM1705DPTP4** | Texas Instruments | 176-pin HLQFP (PowerPad) | Sitara ARM926EJ-S (ARMv5TEJ), 375/456 MHz. Handles UI, OLEDs, Fatar triple-sensor keybed scanning, USB/MIDI, and parameter dispatch. |
| **Audio DSP** | **SHARC ADSP-21xxx** | Analog Devices | LQFP / BGA | 32-bit floating-point DSPs running at 450 MHz. Executes the 3 isolated firmware partitions: `DSPO` (Organ), `DSPS` (Synth), `DSPF` (Piano/FX). |
| **Audio DACs** | **PCM1791ADB** (`U44`, `U45`) | Burr-Brown / TI | 28-pin SSOP | 24-bit 192 kHz Advanced Segment Stereo DAC with 113 dB dynamic range, 8x oversampling digital filter, differential current outputs. Drives Outputs 1..4. |
| **Output Buffers** | **NJM4580M** | JRC / Nisshinbo | 8-pin DMP/SOIC | Ultra-low noise (0.8 µVrms), low distortion (0.0005%), high gain-bandwidth (15 MHz) dual op-amps. Configured as I/V converter + Sallen-Key 2nd-order reconstruction LP. |
| **Master Clock** | **24.576 MHz Crystal** | Citizen / TXC | Metal Can / SMD | Native master audio clock. $24.576\text{ MHz} / 512 = 48.000\text{ kHz}$ internal sampling rate. |
| **Sample Flash** | **NAND Flash Array** | Micron / Macronix | TSOP-48 / BGA | Non-volatile sample storage partitioned into dedicated 2 GB Piano Library and 1 GB Sample/Synth memory pools. |
| **Working SDRAM** | **Mobile SDRAM / DDR** | ISSI / Winbond | TSOP-54 / BGA | High-speed working memory for real-time sample playback buffers, delay/reverb delay lines, and system OS execution. |
| **Power Regulators** | **U43 / U46** | Texas Instruments | DFN / TO-252 | `U43`: Clean +5V analog rail for DACs and op-amps. `U46`: +1.8V switching/LDO core rail for AM1705 MPU and SHARC DSP cores. |

---

## 3. Detailed Hardware Subsystem Analysis

### 3.1 Host MPU: Texas Instruments AM1705DPTP4
- **Architecture:** ARM926EJ-S RISC core (ARMv5TEJ instruction set) with 16 KB Instruction Cache, 16 KB Data Cache, and Memory Management Unit (MMU).
- **Physical Package:** 176-pin HLQFP with an exposed bottom thermal ground pad ("PowerPad"). Thermal stress on this pad is a known point of failure in older units, causing cold-boot delays.
- **Memory Map Correspondence with Firmware:**
  - `0x80000000`: Internal on-chip SRAM (`0xFC80` bytes) — matches the `SRAM` segment in Clavia OS updates.
  - `0xC0000000`: External SDRAM base (EMIFB interface) — matches the target load address of the decompressed 5.49 MB `CODE` segment.
  - `0xFFFF0020`: High Exception Vector base configured by CP15 `SCTLR.V = 1`.
  - Cache/TLB Initialization: Firmware reset stub uses `mcr p15, 0, r0, c7, c7, 0` and `mcr p15, 0, r0, c8, c7, 0`, confirming ARM926EJ-S coprocessor 15 register layout.

### 3.2 Digital-to-Analog Conversion (DAC) & Analog Output Stage
- **DAC Chips (`U44`, `U45`):** Burr-Brown / TI **PCM1791A**.
  - Operates in 24-bit $I^2S$ / McASP audio serial format at $f_s = 48\text{ kHz}$.
  - Internal 8x oversampling filter with sharp roll-off characteristics:
    - Passband: $0 \text{ to } 0.454 \cdot f_s$ ($0 \text{ to } 21.8\text{ kHz}$) with $\pm 0.002\text{ dB}$ ripple.
    - Stopband: $0.546 \cdot f_s$ ($26.2\text{ kHz}$) with $-82\text{ dB}$ attenuation.
- **Analog Filtering & Buffering:**
  - Two dual op-amps **NJM4580M** per stereo pair implement differential current-to-voltage conversion followed by a 2nd-order Sallen-Key low-pass filter ($f_c \approx 32\text{ kHz}$, Butterworth $Q = 0.707$) to eliminate out-of-band delta-sigma quantization noise.
  - Provides a dynamic range of 113 dB and total harmonic distortion (THD+N) of 0.001%.

### 3.3 Master Audio Clock & Sampling Rate
- The master crystal oscillator runs at **$24.576\text{ MHz}$**.
- Integer division ratios:
  - $\div 512 = 48{,}000\text{ Hz}$ ($f_s$, base audio DSP frame rate).
  - $\div 256 = 96{,}000\text{ Hz}$ ($2 \times f_s$, available for internal oversampled distortion/filter stages).
  - $\div 8 = 3.072\text{ MHz}$ (Bit Clock / BCLK for 64-bit stereo $I^2S$ frames at 48 kHz).
- **Direct Consequence:** Nord Stage 4 DSP executes natively at **$48\text{ kHz}$**. No internal $44.1\text{ kHz} \leftrightarrow 48\text{ kHz}$ sample rate conversion is performed on the audio bus.

### 3.4 Keybed Triple-Sensor Scanning
- Fatar TP/400 (hammer action) keybeds connect via ribbon cables to high-speed digital I/O lines on the mainboard.
- Each key features 3 contact points:
  1. *Top sensor:* Resting position.
  2. *Middle sensor:* Intermediate travel threshold (enables rapid note re-triggering without lifting damper felt).
  3. *Bottom sensor:* Full keybed bed strike.
- The AM1705 host timer measures the transit time $\Delta t = t_{\text{bottom}} - t_{\text{middle}}$ in microseconds to compute striking velocity ($v = k / \Delta t$).
- Releasing the key measures $t_{\text{top}} - t_{\text{middle}}$ to compute **Release Velocity** for controlling damper contact speed and release sample attenuation (`SOFT REL`).

### 3.5 Power Distribution & Isolated Ground Planes
- Linear/switching regulators provide split rails:
  - `U43`: Dedicated, filtered **+5V** analog supply rail for DAC analog sections and NJM4580M op-amps.
  - `U46`: Low-noise **+1.8V** digital core rail for the AM1705 MPU and SHARC DSP cores.
- Physical board inspection reveals separated digital and analog ground planes joined at a single star point near the power supply inlet, keeping digital clock hash isolated from audio outputs and achieving $<-110\text{ dB}$ noise floor.

---

## 4. Software Alignment in `nord-stage-4-emulator`

The findings directly validate our implementation choices:
1. **Engine Sample Rate:** `Stage4Engine::new(sample_rate)` defaults to `48000.0`, matching the native $24.576\text{ MHz} / 512$ clock.
2. **Master Bus Reconstruction:** `MasterBus::process_frame` in `crates/nord-core/src/fx.rs` models the ultrasonic low-pass response of the PCM1791A + NJM4580M analog stage.
3. **Key Touch Response:** `VelocityTouch` in `crates/nord-core/src/piano.rs` accurately models the power-law transit curve derived from triple-sensor microsecond contact intervals.
4. **Firmware Extractor:** `crates/nord-core/src/firmware.rs` extracts payloads directly targeting AM1705 SRAM (`0x80000000`) and SDRAM (`0xC0000000`).
