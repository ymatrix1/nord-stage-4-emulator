//! Deterministic worst-case audio benchmark (B13): `ns4 bench` and the quick budget test.
//!
//! Workload: all 7 layers sounding (Scene II), 88 keys held (MIDI 21..=108) so every layer is
//! fully loaded (tonewheels, piano voice pool, synth voices), every layer's FX rack on (Mod 1,
//! Mod 2, Amp, EQ, compressor, delay with feedback FX, Hall reverb), master compressor on, and
//! the four-output routing spread over all outputs (`process_block_multi`). The audio is a pure
//! function of the engine state, so the output hash is reproducible and shows that an
//! optimisation left the sound bit-identical. Timing is wall clock and therefore machine
//! dependent; only the hash is deterministic.

use crate::engine::Stage4Engine;
use crate::fx::{
    AmpCabinet, DelayFeedbackFx, DelayFilterMode, Mod1Type, Mod2Type, ReverbColor, ReverbModel,
};
use crate::midi::MidiEvent;
use std::time::Instant;

/// Block length of the benchmark, in frames (a common host buffer, PRD section 5 budget row).
pub const BLOCK_FRAMES: usize = 128;
/// Sample rate of the benchmark.
pub const SAMPLE_RATE: f32 = 48_000.0;
/// Lowest and highest key of an 88-key keybed.
pub const KEY_RANGE: std::ops::RangeInclusive<u8> = 21..=108;
/// PRD section 5 budget: CPU time of one 128-frame block at 48 kHz with all layers and FX active.
pub const PRD_BLOCK_BUDGET_MS: f64 = 0.8;

/// Which workload `ns4 bench` renders.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Scenario {
    /// 7 layers, 88 keys held, every FX rack on (the module docs).
    #[default]
    Worst,
    /// Default Scene I (Organ A, Piano A, Synth A), default FX, a 10-note chord.
    Typical,
    /// The worst-case engine with no key held (every FX rack still runs).
    Idle,
}

impl Scenario {
    /// Name as given to `ns4 bench --scenario`.
    pub fn name(self) -> &'static str {
        match self {
            Self::Worst => "worst",
            Self::Typical => "typical",
            Self::Idle => "idle",
        }
    }

    /// Scenario of a command line name.
    pub fn parse(name: &str) -> Option<Self> {
        [Self::Worst, Self::Typical, Self::Idle]
            .into_iter()
            .find(|s| s.name() == name)
    }

    /// The engine of this scenario, keys already pressed.
    pub fn engine(self, sample_rate: f32) -> Stage4Engine {
        match self {
            Self::Worst => {
                let mut e = worst_case_engine(sample_rate);
                press_all_keys(&mut e);
                e
            }
            Self::Typical => typical_engine(sample_rate),
            Self::Idle => worst_case_engine(sample_rate),
        }
    }
}

/// What to render.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BenchConfig {
    /// Workload.
    pub scenario: Scenario,
    /// Blocks excluded from timing (lazy tables, caches, voice ramp-up).
    pub warmup_blocks: usize,
    /// Timed blocks.
    pub blocks: usize,
    /// Use `process_block_multi` (all four outputs) instead of `process_block`.
    pub multi_out: bool,
}

impl Default for BenchConfig {
    fn default() -> Self {
        Self {
            scenario: Scenario::Worst,
            warmup_blocks: 200,
            blocks: 2000,
            multi_out: true,
        }
    }
}

/// Result of one benchmark run.
#[derive(Debug, Clone, PartialEq)]
pub struct BenchReport {
    pub blocks: usize,
    /// Mean, median, 99th percentile and worst CPU time of one block, in milliseconds.
    pub mean_ms: f64,
    pub p50_ms: f64,
    pub p99_ms: f64,
    pub max_ms: f64,
    /// Audio seconds rendered per CPU second (higher is better; 1.0 is the real-time limit).
    pub realtime_factor: f64,
    /// FNV-1a hash of every sample of the timed blocks (all outputs), bit-exact.
    pub hash: u64,
    /// Every output sample was finite.
    pub all_finite: bool,
    /// Peak absolute sample over the timed blocks.
    pub peak: f32,
}

impl BenchReport {
    /// CPU load of the block loop as a fraction of the real-time deadline (`1 / realtime_factor`).
    pub fn cpu_load(&self) -> f64 {
        1.0 / self.realtime_factor
    }
}

/// Builds the worst-case engine described in the module docs (notes not yet pressed).
pub fn worst_case_engine(sample_rate: f32) -> Stage4Engine {
    let mut e = Stage4Engine::new(sample_rate);
    e.scene.scene_2_active = true; // Scene II: all 7 layers active
    for chain in &mut e.fx_chains {
        chain.mod1_type = Mod1Type::AutoPan;
        chain.mod2_type = Mod2Type::Chorus;
        chain.amp_cabinet = AmpCabinet::Twin;
        chain.drive_amount = 0.4;
        chain.eq_bass_db = 3.0;
        chain.eq_mid_db = -2.0;
        chain.eq_treble_db = 2.0;
        chain.comp_enabled = true;
        chain.comp_amount = 0.5;
        chain.delay_enabled = true;
        chain.delay_analog = true;
        chain.delay_ping_pong = true;
        chain.delay_filter = DelayFilterMode::Lp;
        chain.delay_fx = DelayFeedbackFx::Chorus;
        chain.reverb_model = ReverbModel::Hall;
        chain.reverb_color = ReverbColor::Normal;
        chain.reverb_mix = 0.3;
    }
    e.master_bus.compressor_amount = 0.5;
    // Spread the sources over all four physical outputs: Organ 1-2, Rotary 3-4, Piano A 3,
    // Piano B 4, Synth A 1-2+3-4, Synth B 1, Synth C 2 (indices of `OutChoice`, see
    // `output_routing.rs`).
    e.output_routing.global_out = [0, 1, 4, 5, 6, 2, 3];
    e
}

/// Presses every key of the 88-key range on channel 0 (the Global channel).
pub fn press_all_keys(e: &mut Stage4Engine) {
    for note in KEY_RANGE {
        e.handle_midi(MidiEvent::NoteOn {
            channel: 0,
            note,
            velocity: 100,
        });
    }
}

/// Typical program: default layers (Organ A, Piano A, Synth A), default FX, a 10-note chord.
pub fn typical_engine(sample_rate: f32) -> Stage4Engine {
    let mut e = Stage4Engine::new(sample_rate);
    for note in 55..65 {
        e.handle_midi(MidiEvent::NoteOn {
            channel: 0,
            note,
            velocity: 100,
        });
    }
    e
}

/// FNV-1a over the bit patterns of `samples`, continuing from `hash`.
pub fn fnv1a_f32(mut hash: u64, samples: &[f32]) -> u64 {
    for s in samples {
        for b in s.to_bits().to_le_bytes() {
            hash ^= u64::from(b);
            hash = hash.wrapping_mul(0x0100_0000_01b3);
        }
    }
    hash
}

/// FNV-1a offset basis.
pub const FNV_BASIS: u64 = 0xcbf2_9ce4_8422_2325;

fn render_block(e: &mut Stage4Engine, multi: bool, bufs: &mut [[f32; BLOCK_FRAMES]; 4]) {
    let [b0, b1, b2, b3] = bufs;
    if multi {
        e.process_block_multi([&mut b0[..], &mut b1[..], &mut b2[..], &mut b3[..]]);
    } else {
        e.process_block(&mut b0[..], &mut b1[..]);
    }
}

/// Runs the workload of `cfg.scenario` and returns timing and the output hash.
pub fn run(cfg: BenchConfig) -> BenchReport {
    match cfg.scenario {
        Scenario::Worst => run_with(cfg, |_| {}),
        other => run_engine(cfg, other.engine(SAMPLE_RATE)),
    }
}

/// Like [`run`], with `setup` applied to the worst-case engine before the keys are pressed.
pub fn run_with(cfg: BenchConfig, setup: impl FnOnce(&mut Stage4Engine)) -> BenchReport {
    let mut e = worst_case_engine(SAMPLE_RATE);
    setup(&mut e);
    press_all_keys(&mut e);
    run_engine(cfg, e)
}

/// Times `cfg.blocks` blocks of an already prepared engine (keys already pressed).
pub fn run_engine(cfg: BenchConfig, mut e: Stage4Engine) -> BenchReport {
    let mut bufs = [[0.0f32; BLOCK_FRAMES]; 4];
    for _ in 0..cfg.warmup_blocks {
        render_block(&mut e, cfg.multi_out, &mut bufs);
    }
    let mut times: Vec<f64> = Vec::with_capacity(cfg.blocks);
    let mut hash = FNV_BASIS;
    let mut all_finite = true;
    let mut peak = 0.0f32;
    let outputs = if cfg.multi_out { 4 } else { 2 };
    for _ in 0..cfg.blocks {
        let t0 = Instant::now();
        render_block(&mut e, cfg.multi_out, &mut bufs);
        times.push(t0.elapsed().as_secs_f64() * 1000.0);
        for buf in &bufs[..outputs] {
            hash = fnv1a_f32(hash, buf);
            for s in buf {
                all_finite &= s.is_finite();
                peak = peak.max(s.abs());
            }
        }
    }
    let total_ms: f64 = times.iter().sum();
    times.sort_by(|a, b| a.total_cmp(b));
    let at = |q: f64| {
        times
            .get(((times.len() as f64 - 1.0) * q).round() as usize)
            .copied()
            .unwrap_or(0.0)
    };
    let audio_ms = cfg.blocks as f64 * BLOCK_FRAMES as f64 / f64::from(SAMPLE_RATE) * 1000.0;
    BenchReport {
        blocks: cfg.blocks,
        mean_ms: total_ms / cfg.blocks.max(1) as f64,
        p50_ms: at(0.5),
        p99_ms: at(0.99),
        max_ms: at(1.0),
        realtime_factor: audio_ms / total_ms.max(1e-9),
        hash,
        all_finite,
        peak,
    }
}

/// Mean block time of the worst case with parts switched off, to show where the time goes.
/// Each line is `(label, mean ms per block)`; the layers are rendered one at a time with their
/// FX racks bypassed (reverb off, the other effects at their defaults), so they do not add up
/// to the full figure.
pub fn breakdown(cfg: BenchConfig) -> Vec<(String, f64)> {
    let cfg = BenchConfig {
        scenario: Scenario::Worst,
        ..cfg
    };
    let mut rows = vec![("full worst case".to_string(), run(cfg).mean_ms)];
    let fx_off = |e: &mut Stage4Engine| {
        for chain in &mut e.fx_chains {
            *chain = crate::fx::LayerFxChain::new(SAMPLE_RATE);
            chain.reverb_model = ReverbModel::Off;
        }
        e.master_bus.compressor_amount = 0.0;
    };
    rows.push((
        "all FX racks and master comp off".into(),
        run_with(cfg, fx_off).mean_ms,
    ));
    let idle = run_engine(cfg, Scenario::Idle.engine(SAMPLE_RATE)).mean_ms;
    rows.push(("no keys held, all FX racks on (idle)".into(), idle));
    let typical = run_engine(cfg, Scenario::Typical.engine(SAMPLE_RATE)).mean_ms;
    rows.push((
        "typical: Scene I (3 layers), 10 keys, default FX".into(),
        typical,
    ));
    let names = [
        "Organ A", "Organ B", "Piano A", "Piano B", "Synth A", "Synth B", "Synth C",
    ];
    for (i, name) in names.iter().enumerate() {
        let only = move |e: &mut Stage4Engine| {
            fx_off(e);
            e.scene.scene_2_mask = 1 << i;
        };
        rows.push((
            format!("{name} only, 88 keys, FX off"),
            run_with(cfg, only).mean_ms,
        ));
    }
    rows
}

/// Human readable report, one metric per line (printed by `ns4 bench`).
pub fn format_report(cfg: BenchConfig, r: &BenchReport) -> String {
    let budget = r.mean_ms <= PRD_BLOCK_BUDGET_MS;
    let workload = match cfg.scenario {
        Scenario::Worst => format!(
            "7 layers (Scene II), {} keys held, 7 FX racks + master comp",
            KEY_RANGE.count()
        ),
        Scenario::Idle => "7 layers (Scene II), no key held, 7 FX racks + master comp".to_string(),
        Scenario::Typical => "Scene I (3 layers), 10 keys held, default FX".to_string(),
    };
    format!(
        "ns4 bench: {workload}, {} @ {} Hz, {} frame blocks\n\
         blocks timed        {}\n\
         mean / p50 / p99 / max  {:.3} / {:.3} / {:.3} / {:.3} ms per block (deadline {:.3} ms)\n\
         realtime factor     {:.2}x  (CPU load {:.1}% of one core)\n\
         PRD budget          mean <= {PRD_BLOCK_BUDGET_MS} ms: {}\n\
         output hash         {:#018x}  (finite: {}, peak {:.3})",
        if cfg.multi_out { "4 outputs" } else { "stereo" },
        SAMPLE_RATE,
        BLOCK_FRAMES,
        r.blocks,
        r.mean_ms,
        r.p50_ms,
        r.p99_ms,
        r.max_ms,
        BLOCK_FRAMES as f64 / f64::from(SAMPLE_RATE) * 1000.0,
        r.realtime_factor,
        r.cpu_load() * 100.0,
        if budget { "met" } else { "NOT met" },
        r.hash,
        r.all_finite,
        r.peak,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quick() -> BenchConfig {
        BenchConfig {
            scenario: Scenario::Worst,
            warmup_blocks: 20,
            blocks: 40,
            multi_out: true,
        }
    }

    #[test]
    fn the_hash_is_deterministic_and_the_output_finite_and_audible() {
        let a = run(quick());
        let b = run(quick());
        assert_eq!(a.hash, b.hash, "same workload must hash identically");
        assert!(a.all_finite);
        assert!(a.peak > 0.0, "the worst case workload was silent");
    }

    #[test]
    fn fnv_hash_follows_the_bit_pattern() {
        let h1 = fnv1a_f32(FNV_BASIS, &[0.0]);
        let h2 = fnv1a_f32(FNV_BASIS, &[-0.0]);
        assert_ne!(h1, h2, "-0.0 and 0.0 differ in bits");
        assert_eq!(fnv1a_f32(FNV_BASIS, &[]), FNV_BASIS);
    }

    #[test]
    fn worst_case_engine_has_all_layers_active_and_four_outputs_used() {
        let mut e = worst_case_engine(SAMPLE_RATE);
        assert!(crate::morph::LayerId::ALL
            .iter()
            .all(|&l| e.scene.is_layer_active(l)));
        press_all_keys(&mut e);
        let mut used = [false; 4];
        for _ in 0..4000 {
            let f = e.render_frame_multi();
            for (u, s) in used.iter_mut().zip(f) {
                *u |= s.abs() > 1e-6;
            }
        }
        assert_eq!(used, [true; 4], "every physical output must carry signal");
    }

    /// Generous guard (debug builds included): a block must stay far below an order-of-magnitude
    /// regression. The real numbers come from `ns4 bench` in release (docs/HOST_COMPATIBILITY.md).
    #[test]
    fn a_block_stays_within_a_generous_budget() {
        let r = run(quick());
        let limit = if cfg!(debug_assertions) { 500.0 } else { 20.0 };
        assert!(
            r.mean_ms < limit,
            "mean block time {:.2} ms exceeds {limit} ms",
            r.mean_ms
        );
    }

    #[test]
    fn report_text_names_the_budget_verdict() {
        let r = run(BenchConfig {
            scenario: Scenario::Worst,
            warmup_blocks: 1,
            blocks: 2,
            multi_out: false,
        });
        let text = format_report(BenchConfig::default(), &r);
        assert!(text.contains("realtime factor"));
        assert!(text.contains("PRD budget"));
    }

    #[test]
    fn scenarios_parse_run_and_differ() {
        for sc in [Scenario::Worst, Scenario::Typical, Scenario::Idle] {
            assert_eq!(Scenario::parse(sc.name()), Some(sc));
        }
        assert_eq!(Scenario::parse("nope"), None);
        let hash = |scenario| {
            run(BenchConfig {
                scenario,
                ..quick()
            })
        };
        let (worst, typical, idle) = (
            hash(Scenario::Worst),
            hash(Scenario::Typical),
            hash(Scenario::Idle),
        );
        assert!(worst.hash != typical.hash && typical.hash != idle.hash);
        assert!(typical.peak > 0.0 && typical.all_finite && idle.all_finite);
        let text = format_report(
            BenchConfig {
                scenario: Scenario::Typical,
                ..quick()
            },
            &typical,
        );
        assert!(text.contains("10 keys held"), "{text}");
    }
}
