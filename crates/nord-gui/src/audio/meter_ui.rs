//! Status-bar readout of the [`AudioMeter`](super::meter::AudioMeter): late blocks, worst block
//! time against the deadline, output peak, with a Reset button.

use super::meter::{AudioMeter, MeterSnapshot};

/// Colour class of the readout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Ok,
    Warn,
    Late,
}

/// Red once a block overran, amber above 70 % load or after a sample above 0 dBFS.
///
/// ASSUMED: the 70 % amber threshold is a UI choice (headroom for a loaded host), not measured on
/// hardware; the red state (late block) is exact. Confirm with device data from the meter.
pub fn severity(s: &MeterSnapshot) -> Severity {
    if s.late > 0 {
        Severity::Late
    } else if s.over_0db > 0 || s.load() > 0.7 {
        Severity::Warn
    } else {
        Severity::Ok
    }
}

/// Short label: `late 3/9120  worst 7.20/2.67 ms  load 35%  peak -3.1 dB`.
pub fn label(s: &MeterSnapshot) -> String {
    if s.blocks == 0 {
        return "audio: waiting".into();
    }
    let over = if s.over_0db > 0 {
        format!("  over 0 dB: {}", s.over_0db)
    } else {
        String::new()
    };
    format!(
        "late {}/{}  worst {:.2}/{:.2} ms  load {:.0}%  peak {:.1} dB{over}",
        s.late,
        s.blocks,
        s.worst_ms,
        s.deadline_ms(),
        s.load() * 100.0,
        s.peak_dbfs(),
    )
}

/// One row: readout (hover for the full summary) and a Reset button.
pub fn show(ui: &mut egui::Ui, meter: &AudioMeter) {
    let s = meter.snapshot();
    let color = match severity(&s) {
        Severity::Ok => egui::Color32::from_rgb(140, 200, 140),
        Severity::Warn => egui::Color32::from_rgb(230, 180, 70),
        Severity::Late => egui::Color32::from_rgb(240, 90, 80),
    };
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(label(&s)).color(color).monospace())
            .on_hover_text(format!(
                "{}\nworst command drain {:.3} ms, worst telemetry publish {:.3} ms",
                s.summary(),
                s.worst_drain_ms,
                s.worst_publish_ms
            ));
        if ui.button("Reset").clicked() {
            meter.request_reset();
        }
    });
}

#[cfg(test)]
mod tests {
    use super::super::meter::BlockSample;
    use super::*;

    fn snap(total_ns: u64, peak: f32, over: u32) -> MeterSnapshot {
        let m = AudioMeter::default();
        m.set_sample_rate(48_000);
        m.record(BlockSample {
            frames: 128,
            total_ns,
            peak,
            over_0db: over,
            ..Default::default()
        });
        m.snapshot()
    }

    #[test]
    fn severity_follows_late_blocks_then_level_and_load() {
        assert_eq!(severity(&snap(500_000, 0.5, 0)), Severity::Ok);
        assert_eq!(severity(&snap(2_000_000, 0.5, 0)), Severity::Warn);
        assert_eq!(severity(&snap(500_000, 1.2, 4)), Severity::Warn);
        assert_eq!(severity(&snap(3_000_000, 0.5, 0)), Severity::Late);
    }

    #[test]
    fn label_shows_counts_and_deadline() {
        assert_eq!(label(&MeterSnapshot::default()), "audio: waiting");
        let t = label(&snap(3_000_000, 1.2, 4));
        assert!(t.starts_with("late 1/1  worst 3.00/2.67 ms"), "{t}");
        assert!(t.ends_with("over 0 dB: 4"), "{t}");
    }

    #[test]
    fn readout_builds_headless() {
        let m = AudioMeter::default();
        let ctx = egui::Context::default();
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| show(ui, &m));
        });
    }
}
