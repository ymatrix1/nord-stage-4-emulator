//! Shared headless egui fixture for the widget tests.

use egui::{CentralPanel, Context, RawInput, Ui};

/// Runs `f` once inside a headless central panel.
pub(super) fn with_ui(f: impl FnOnce(&mut Ui)) {
    let ctx = Context::default();
    let mut f = Some(f);
    let _ = ctx.run(RawInput::default(), |ctx| {
        CentralPanel::default().show(ctx, |ui| {
            if let Some(f) = f.take() {
                f(ui);
            }
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_with_ui_runs_closure_once() {
        let mut calls = 0;
        with_ui(|_ui| calls += 1);
        assert_eq!(calls, 1);
    }
}
