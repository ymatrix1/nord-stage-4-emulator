<h1 align="center">Nord Stage 4 Emulator</h1>

<p align="center">
  <strong>Open-source DSP sound engine, binary preset toolkit (<code>CBIN</code> / <code>.ns4b</code> / <code>.ns4p</code>), and High-Level Emulator for the Clavia Nord Stage 4.</strong><br>
  <sub>Built for interoperability, research, preservation, and compatibility purposes.</sub>
</p>

---

The **Nord Stage 4** (`product_id=46`) is built around a **three-engine, 7-layer multitimbral DSP architecture** running across an **`ARM0`** host controller and parallel Analog Devices SHARC floating-point DSPs (**`DSPO`** Organ, **`DSPS`** Synth, **`DSPF`** Effects & Piano Resonance).

Unlike generic ROMpler clones that only imitate the front panel visually, this project implements the real internal signal architecture and binary preset formats documented through empirical hardware and firmware analysis:

- **Clavia Binary (`CBIN`) & Backup (`.ns4b`) Interoperability:** Lossless parsing, inspection, and round-trip serialization of `.ns4p` (868 B Programs), `.ns4l` (868 B Live slots), `.ns4y` (541 B Synth presets), `.ns4n` (195 B Piano presets), `.ns4o` (183 B Organ presets), `.ns4t` (124 B Settings), `.ns4b` backup archives, and OS v1.66 firmware containers.
- **Organ Engine (`DSPO` — 2 Layers: A/B):** 100% mathematical physical modeling of 91 phase-locked spinning B3 tonewheels with authentic 196-tooth gear ratios, harmonic foldback, key click, single-triggered 2nd/3rd harmonic percussion, C1–C3/V1–V3 scanner vibrato, Vox/Farfisa/Pipe models, and the **Model 122 Vintage Rotary Speaker** (800 Hz Linkwitz-Riley crossover, independent horn/drum rotational ballistics, and stereo Doppler delay modulation).
- **Piano Engine (`DSPF` — 2 Layers: A/B, 120-Voice Polyphony):** Multisample playback paired with real-time **Advanced String Resonance** (12-band tuned modal sympathetic resonator bank excited when the damper pedal is depressed), **Dynamic Pedal Noise**, velocity response curves (`Light`, `Medium`, `Heavy`), and Timbre EQ presets (`Soft`, `Mid`, `Bright`, `Dyno1`, `Dyno2`).
- **Synth Engine (`DSPS` — 3 Layers: A/B/C, Nord Wave 2 Derived):** Anti-aliased **PolyBLEP** Virtual Analog oscillators (`Saw`, `Pulse/PWM`, `Triangle`, `Sine`), **Wavetable**, **2/3-Operator FM**, and **SFZ/WAV Sample** modes feeding topology-preserving **Zero-Delay Feedback (`ZDF`)** filters (`12/24 dB Classic Transistor Ladder`, `LP`, `BP`, `HP`), ADR envelopes, LFOs, and a clock-synced Arpeggiator.
- **7 Independent Per-Layer Effects Chains:** Every one of the 7 layers (`Organ A/B`, `Piano A/B`, `Synth A/B/C`) runs its own dedicated 5-stage stereo FX pipeline (`Mod 1` → `Mod 2` → `Amp/Drive/3-Band EQ` → `Ping-Pong/Analog Delay` → `Reverb`), summed into the global **Master Compressor**, **Master Clock**, and `CS4392` + `LM833` post-DAC reconstruction filter.
- **4-Zone Keybed Router, Layer Scene II & 3-Source Morphing Matrix:** Up to 3 split points (4 zones) with `Off`, `Small` (6 st), and `Large` (12 st) crossfades, instant **Layer Scene I/II** switching, and simultaneous **3-Source Morphing** (`Mod Wheel` CC1, `Control Pedal` CC11, `Aftertouch`).

---

## Architecture & Documentation

Read the full specification suite in [`docs/`](docs/README.md):

| Document | Contents |
|---|---|
| [docs/PRD.md](docs/PRD.md) | Goals, target users, latency & polyphony budgets, non-goals |
| [docs/ROADMAP.md](docs/ROADMAP.md) | Phased milestones and parallel development lanes |
| [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) | System diagram, crate layout, and real-time audio invariants |
| [docs/DECISIONS.md](docs/DECISIONS.md) | Architectural Decision Records (`D1`–`D10`) |
| [docs/specs/cbin-and-presets.md](docs/specs/cbin-and-presets.md) | 48-byte `CBIN` header, `.ns4p`/`.ns4y`/`.ns4o`/`.ns4n` bitstream embedding, OS v1.66 container |
| [docs/specs/organ-engine.md](docs/specs/organ-engine.md) | 91-tonewheel B3 math, foldback, percussion, and Model 122 Rotary Speaker |
| [docs/specs/synth-engine.md](docs/specs/synth-engine.md) | PolyBLEP VA oscillators and Zero-Delay Feedback (`ZDF`) 4-pole Ladder filter |
| [docs/specs/piano-engine.md](docs/specs/piano-engine.md) | Modal sympathetic string resonance, pedal noise, and timbre filters |
| [docs/specs/effects-and-routing.md](docs/specs/effects-and-routing.md) | 7 per-layer FX racks, Master Compressor, and Master Clock |
| [docs/specs/morph-and-zones.md](docs/specs/morph-and-zones.md) | 4-zone split crossfades, Layer Scene II, and 3-source Morphing Matrix |
| [docs/specs/midi-and-control.md](docs/specs/midi-and-control.md) | Complete Nord Stage 4 MIDI CC table |
| [docs/research/](docs/research/) | Empirical teardown notes of `.ns4b`, OS v1.66 firmware, and DAC schematics |

---

## Quick Start

### 1. Build & Test the Rust DSP & Parser Core (`crates/nord-core`)

```bash
cargo test --workspace --all-targets --all-features
```

Run the `ns4` CLI to inspect a preset/backup or render an offline 7-layer audio test buffer:

```bash
cargo run -p nord-core --bin ns4 -- info
cargo run -p nord-core --bin ns4 -- render-demo --frames 48000
```

### 2. Inspect `.ns4b` Backups, `.ns4p` Programs & Firmware Images (`tools/ns4_inspect.py`)

```bash
uv sync --locked
uv run python tools/ns4_inspect.py --help
```

### 3. Run Full CI Quality Gates Locally

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-targets --all-features
uv run ruff check tools/
uv run ruff format --check tools/
uv run pytest
python tools/check_tests.py
```

---

## Contributing

Before opening a pull request:
- Read [AGENTS.md](AGENTS.md) and [.agents/rules/](.agents/rules/) for project conventions.
- Tasks are tracked with [beans](https://github.com/hmans/beans) in [`.beans/`](.beans/).
- Every source file in `crates/` and `tools/` must have unit tests (`python tools/check_tests.py` enforces 0 uncovered files in CI).
