# Verification, CI/CD & Testing Specification

## 1. Scope

Specifies the automated quality gates, unit-test coverage ratchet (`tools/check_tests.py`), deterministic DSP verification tests, and GitHub Actions CI/CD workflows (`.github/workflows/ci.yml`, `.github/workflows/release.yml`).

---

## 2. Per-File Unit-Test Ratchet (`tools/check_tests.py`)

Every source file in the repository MUST have an accompanying unit test:
- **Rust (`crates/**/src/**/*.rs`):** Every implementation file must contain a `#[test]` module (declaration-only `lib.rs` files with only `pub mod` / `pub use` are exempt).
- **Python (`tools/**/*.py`):** Every tool module must be imported and exercised by a test file in `tools/tests/test_*.py`.
- **Ratchet Baseline (`tools/test-baseline.txt`):** Maintained at **0 uncovered files**. CI fails if any new file is added without unit tests or if a baseline entry becomes stale.

---

## 3. Deterministic DSP & Binary Format Verification

Because CI runners do not require a physical Nord Stage 4 connected via USB or an audio output device, all subsystems are verified deterministically in memory:

1. **Synthetic `CBIN` & Firmware Fixtures:**
   - Unit tests construct valid 868-byte `.ns4p`, 541-byte `.ns4y`, 183-byte `.ns4o`, 195-byte `.ns4n`, 868-byte `.ns4l`, and 124-byte `.ns4t` buffers matching the exact schema versions (`0x0139`, `0x00D0`, `0x00CD`, `0x00CB`, `0x006C`) and test lossless round-trip serialization.
   - Firmware container tests construct multi-segment headers (`SRAM`, `CODE`, `ARM0`, `DSPO`, `DSPS`, `DSPF`) and verify offset/length bounds checking.
2. **Offline Audio Rendering Assertions:**
   - **Organ Engine:** Verifies 91-tonewheel frequency accuracy ($A_4 = 440.0\text{ Hz}$), drawbar harmonic energy scaling, single-trigger percussion decay, and Rotary 122 Doppler modulation finite/bounded output (`!sample.is_nan()`, peak $\le 1.0$).
   - **Synth Engine:** Verifies PolyBLEP DC-zero balance, anti-aliasing step reduction vs naive sawtooth, and ZDF 24 dB Ladder low-pass attenuation of high-frequency harmonics above cutoff.
   - **Piano Engine:** Verifies velocity curve monotonicity (`Heavy < Medium < Light` at $v = 64$) and sympathetic string resonance energy increase when `CC64` sustain pedal is active.
   - **7-Layer Engine & Morphing:** Verifies 4-zone split crossfade continuity (`G_low(k) + G_high(k) == 1.0`), Layer Scene I/II switching, and 3-source Morph matrix additive clamping.

---

## 4. CI/CD Workflows

### 4.1 Continuous Integration (`.github/workflows/ci.yml`)

Triggered on every push to `main`, `v*` tags, `pull_request`, and `workflow_dispatch`:
- **Smart Concurrency:** Cancels superseded runs on PR pushes while never cancelling `main` or `v*` tag runs.
- **Job `rust` (Linux & Windows matrix):**
  - `cargo fmt --all -- --check`
  - `cargo clippy --workspace --all-targets --all-features -- -D warnings`
  - `cargo test --workspace --all-targets --all-features`
- **Job `python`:**
  - `uv sync --locked`
  - `uv run ruff check tools/`
  - `uv run ruff format --check tools/`
  - `uv run pytest`
- **Job `test-ratchet`:**
  - `python3 tools/check_tests.py`

### 4.2 Continuous Delivery / Releases (`.github/workflows/release.yml`)

Triggered on version tags (`v*.*.*`) and manual `workflow_dispatch`:
- Builds optimized release binaries (`ns4`) across Linux (`x86_64-unknown-linux-gnu`), Windows (`x86_64-pc-windows-msvc`), and macOS (`aarch64-apple-darwin`).
- Packages platform archives (`.tar.gz` / `.zip`) with `SHA256SUMS.txt` and publishes them to GitHub Releases.
