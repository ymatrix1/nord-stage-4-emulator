//! UI state of the Synth area: per-layer control values, the display page machine, GROUP latches,
//! knob pick-up, and the queue of engine edits the bay sends after each frame.
//!
//! `Stage4PanelState::synth_ui` holds the struct. Logic is split by control family:
//! `buttons.rs` (button + SHIFT triggers), `dials.rs` (the three OLED dials), `knobs.rs` (pots with
//! pick-up), `legacy.rs` (the older `synth_*` fields of the panel state), `layer.rs` (values).

mod buttons;
mod dials;
mod dials_pages;
mod groups;
mod knobs;
mod layer;
mod layer_btn;
mod layer_emit;
mod legacy;
mod page;
mod pattern;
mod pots;
mod press;
mod samples;
mod tables;
mod waves;

pub use buttons::Btn;
pub use knobs::Knob;
pub use layer::{step_time, Env, SynthLayerUi};
pub use layer_btn::LayerAction;
pub use legacy::Legacy;
pub use page::{dial_defs, DialDef, SynthPage, ARP_PAGES};
pub use pots::{Pots, FREQ_MAX, FREQ_MIN, GLIDE_MAX, LFO_MAX, LFO_MIN};
pub use samples::SampleBrowser;
pub use tables::*;

use crate::audio::{SynthEdit, SynthGroup, SynthParam, SynthTarget};
use nord_panel::behaviour::{ButtonTracker, Latch};
use std::collections::HashMap;

/// Number of dials under the display.
pub const DIAL_COUNT: usize = 3;

/// Which settings group an edit belongs to: decides whether a GROUP spreads it to all layers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Scope {
    /// Always the focused layer only.
    Layer,
    Filter,
    Lfo,
    Arp,
    /// Section-wide control (KB Hold): always all layers.
    Section,
}

/// UI state for the Synth area.
#[derive(Debug, Clone)]
pub struct SynthUi {
    /// Focused layer `0..=2` (A, B, C): the one the dials, knobs and buttons edit.
    pub focus: usize,
    pub layers: [SynthLayerUi; 3],
    pub page: SynthPage,
    /// LIST view of the waveform dials (Shift+dial, manual p.29).
    pub list_view: bool,
    /// INFO screen of a sample (Shift+left dial).
    pub info_view: bool,
    /// Section ON button.
    pub section_on: bool,
    /// SOLO monitor mode (hold ON); UI-only (manual p.43).
    pub solo: bool,
    /// KB HOLD, shared by all layers (manual p.36).
    pub kb_hold: bool,
    /// SUSTPED of the section (Shift+Layer A): the layers follow the sustain pedal.
    pub sustped: bool,
    /// PSTICK of the section (Shift+Layer B): the layers follow the pitch stick.
    pub pstick: bool,
    /// PSTICK/RNG bend range in semitones (Shift+Layer B, manual p.28), `1..=12`.
    pub pstick_range: u8,
    /// Fractional scroll points of the PSTICK/RNG list, kept between frames.
    pub(crate) range_acc: f32,
    pub group_filter: Latch,
    pub group_lfo: Latch,
    pub group_arp: Latch,
    pub pots: Pots,
    /// Sample Library browser behind the CAT / SAMPLE dials of the Sample mode.
    pub samples: SampleBrowser,
    /// Fractional dial clicks, kept between frames.
    pub dial_acc: [f32; 3],
    pub(crate) trackers: HashMap<&'static str, ButtonTracker>,
    out: Vec<SynthEdit>,
    drawn: Vec<&'static str>,
    legacy_seen: Option<Legacy>,
}

impl Default for SynthUi {
    fn default() -> Self {
        Self::new()
    }
}

impl SynthUi {
    pub fn new() -> Self {
        let layers: [SynthLayerUi; 3] = Default::default();
        let mut pots = Pots::default();
        pots.reset(&layers[0]);
        Self {
            focus: 0,
            layers,
            page: SynthPage::default(),
            list_view: false,
            info_view: false,
            section_on: true,
            solo: false,
            kb_hold: false,
            sustped: false,
            pstick: false,
            pstick_range: nord_core::engine::PSTICK_DEFAULT_RANGE,
            range_acc: 0.0,
            group_filter: Latch::default(),
            group_lfo: Latch::default(),
            group_arp: Latch::default(),
            pots,
            samples: SampleBrowser::default(),
            dial_acc: [0.0; 3],
            trackers: HashMap::new(),
            out: Vec::new(),
            drawn: Vec::new(),
            legacy_seen: None,
        }
    }

    /// The focused layer's values.
    pub fn layer(&self) -> &SynthLayerUi {
        &self.layers[self.focus.min(2)]
    }

    pub(crate) fn layer_mut(&mut self) -> &mut SynthLayerUi {
        let f = self.focus.min(2);
        &mut self.layers[f]
    }

    /// GROUP of `scope` is on: edits reach all layers.
    pub(crate) fn grouped(&self, scope: Scope) -> bool {
        match scope {
            Scope::Layer => false,
            Scope::Filter => self.group_filter.is_active(),
            Scope::Lfo => self.group_lfo.is_active(),
            Scope::Arp => self.group_arp.is_active(),
            Scope::Section => true,
        }
    }

    /// Apply `f` to the focused layer (all layers when grouped) and queue the engine write `p`.
    pub(crate) fn edit(&mut self, scope: Scope, p: SynthParam, f: impl Fn(&mut SynthLayerUi)) {
        if self.grouped(scope) {
            self.layers.iter_mut().for_each(f);
            self.out.push(SynthEdit::Param(SynthTarget::All, p));
        } else {
            f(self.layer_mut());
            let l = self.focus.min(2) as u8;
            self.out.push(SynthEdit::Param(SynthTarget::Layer(l), p));
        }
    }

    /// Queue the focused layer's oscillator writes (mode, waveform, OSC CTRL ...).
    pub(crate) fn resend_osc(&mut self) {
        let layer = self.focus.min(2) as u8;
        let mut v = Vec::new();
        self.layer().emit_osc(&mut |p| v.push(p));
        self.out.extend(
            v.into_iter()
                .map(|p| SynthEdit::Param(SynthTarget::Layer(layer), p)),
        );
    }

    /// Queue a GROUP copy from the focused layer.
    pub(crate) fn push_group_copy(&mut self, group: SynthGroup) {
        self.out.push(SynthEdit::Group {
            src: self.focus.min(2) as u8,
            group,
        });
    }

    /// Focus layer `i`; the physical knobs must be picked up again (manual: pots are physical).
    pub fn focus_layer(&mut self, i: usize) {
        let i = i.min(2);
        if i == self.focus {
            return;
        }
        self.focus = i;
        self.list_view = false;
        self.info_view = false;
        self.pots.layer_changed(&self.layers[i]);
    }

    /// Page state machine: a function button selects its page; pressing the page's button again
    /// returns to the default waveform view.
    /// ASSUMED: toggle-back behaviour and no timeout; the manual does not say how the display
    /// leaves a page. Confirm on hardware.
    pub fn open(&mut self, page: SynthPage) {
        self.page = if self.page == page && page != SynthPage::Waveform {
            SynthPage::Waveform
        } else {
            page
        };
        self.list_view = false;
        self.info_view = false;
        self.dial_acc = [0.0; 3];
    }

    /// Queue an engine edit that has no panel state of its own to update.
    pub(crate) fn push_edit(&mut self, e: SynthEdit) {
        self.out.push(e);
    }

    /// Edits queued since the last call (the bay sends them to the DSP thread).
    pub fn take_edits(&mut self) -> Vec<SynthEdit> {
        std::mem::take(&mut self.out)
    }

    /// Start of a frame: forget which widgets were painted.
    pub fn begin_frame(&mut self) {
        self.drawn.clear();
    }

    /// Record that the widget of inventory id `id` was painted this frame.
    pub fn mark(&mut self, id: &'static str) {
        if !self.drawn.contains(&id) {
            self.drawn.push(id);
        }
    }

    /// Ids painted this frame.
    pub fn drawn(&self) -> &[&'static str] {
        &self.drawn
    }

    /// Button tracker of inventory id `id` (created on first use).
    pub fn tracker(&mut self, id: &'static str) -> &mut ButtonTracker {
        self.trackers.entry(id).or_default()
    }
}

/// Reason a control has no engine effect (dimmed with a tooltip), `None` when it works.
///
/// Every Synth control reaches the engine now: the strip's KB ZONE and Aux KB LEDs (`SynthEdit::Zone`
/// / `AuxKb`) and EXTERN with its dials (`SynthEdit::Extern*`) were the last dimmed ones.
pub fn unsupported_reason(_id: &str) -> Option<&'static str> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_state_starts_on_the_waveform_page_with_the_section_on() {
        let s = SynthUi::new();
        assert_eq!(s.page, SynthPage::Waveform);
        assert!(s.section_on);
        assert_eq!(s.focus, 0);
        assert!(s.pots.is_engaged(Knob::FilterFreq));
    }

    #[test]
    fn pressing_a_function_button_opens_its_page_and_again_returns() {
        let mut s = SynthUi::new();
        s.open(SynthPage::FilterType);
        assert_eq!(s.page, SynthPage::FilterType);
        s.open(SynthPage::FilterType);
        assert_eq!(s.page, SynthPage::Waveform);
        s.open(SynthPage::Waveform);
        assert_eq!(s.page, SynthPage::Waveform, "waveform page stays");
    }

    #[test]
    fn opening_a_page_clears_list_and_info_views() {
        let mut s = SynthUi::new();
        s.list_view = true;
        s.info_view = true;
        s.open(SynthPage::Vibrato);
        assert!(!s.list_view && !s.info_view);
    }

    #[test]
    fn focus_change_disengages_the_physical_knobs() {
        let mut s = SynthUi::new();
        s.layers[1].cutoff_hz = 300.0;
        s.focus_layer(1);
        assert_eq!(s.focus, 1);
        assert!(!s.pots.is_engaged(Knob::FilterFreq));
        s.focus_layer(7);
        assert_eq!(s.focus, 2, "focus is clamped to layer C");
    }

    #[test]
    fn edit_targets_the_focused_layer_or_all_when_grouped() {
        let mut s = SynthUi::new();
        s.focus_layer(2);
        s.edit(Scope::Filter, SynthParam::Resonance(0.8), |l| {
            l.resonance = 0.8
        });
        assert_eq!(s.layers[2].resonance, 0.8);
        assert_eq!(s.layers[0].resonance, 0.3);
        s.group_filter.double_tap();
        s.edit(Scope::Filter, SynthParam::Resonance(0.5), |l| {
            l.resonance = 0.5
        });
        assert!(s.layers.iter().all(|l| l.resonance == 0.5));
        let e = s.take_edits();
        assert_eq!(
            e,
            vec![
                SynthEdit::Param(SynthTarget::Layer(2), SynthParam::Resonance(0.8)),
                SynthEdit::Param(SynthTarget::All, SynthParam::Resonance(0.5)),
            ]
        );
        assert!(s.take_edits().is_empty());
    }

    #[test]
    fn section_scope_always_reaches_all_layers() {
        let s = SynthUi::new();
        assert!(s.grouped(Scope::Section));
        assert!(!s.grouped(Scope::Layer));
        assert!(!s.grouped(Scope::Lfo));
    }

    #[test]
    fn registry_records_each_id_once_per_frame() {
        let mut s = SynthUi::new();
        s.mark("synth.oled");
        s.mark("synth.oled");
        assert_eq!(s.drawn(), ["synth.oled"]);
        s.begin_frame();
        assert!(s.drawn().is_empty());
    }

    #[test]
    fn no_synth_control_is_dimmed_any_more() {
        for id in [
            "synth.extern",
            "synth.aux_led.b",
            "synth.kb_zone_up",
            "synth.zone_leds",
            "synth.filter.freq",
        ] {
            assert!(unsupported_reason(id).is_none(), "{id} reaches the engine");
        }
    }

    #[test]
    fn resend_osc_targets_the_focused_layer() {
        let mut s = SynthUi::new();
        s.focus_layer(1);
        s.resend_osc();
        let e = s.take_edits();
        assert!(!e.is_empty());
        assert!(e
            .iter()
            .all(|x| matches!(x, SynthEdit::Param(SynthTarget::Layer(1), _))));
    }

    #[test]
    fn trackers_are_created_per_button() {
        let mut s = SynthUi::new();
        s.tracker("synth.mode").press(0);
        assert!(s.tracker("synth.mode").is_down());
        assert!(!s.tracker("synth.waveform").is_down());
    }
}
