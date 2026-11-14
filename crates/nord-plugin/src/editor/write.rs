//! Host parameter writes: the output of the panel-edit mapping and the trait that applies them.
//!
//! The editor never touches the engine. A panel edit becomes one or more [`ParamWrite`]s; the
//! real writer wraps them in `begin_set_parameter` / `set_parameter_normalized` /
//! `end_set_parameter` on the GUI thread, so the host records automation and `process()` picks
//! the value up through `Stage4Params::sync_to_engine`.

use super::index::ParamIndex;
use nih_plug::prelude::GuiContext;

/// One host parameter assignment in plain (unnormalized) units.
///
/// Bool params use 0.0 / 1.0, int and enum params use the integer index as a float.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ParamWrite {
    /// Parameter id without the layer suffix, for example `fx_drive`.
    pub family: &'static str,
    /// 1-based index appended as `_{n}`: the layer (1..=7) for `fx_*` / `ped_*`, the layer within
    /// its area for `organx_*` / `pianox_*` / `synthx_*`, or `None` for a global parameter.
    pub layer: Option<u8>,
    pub plain: f32,
}

impl ParamWrite {
    pub fn global(family: &'static str, plain: f32) -> Self {
        Self {
            family,
            layer: None,
            plain,
        }
    }

    pub fn layer(family: &'static str, layer: u8, plain: f32) -> Self {
        Self {
            family,
            layer: Some(layer),
            plain,
        }
    }

    /// Full host parameter id.
    pub fn host_id(&self) -> String {
        match self.layer {
            Some(n) => format!("{}_{n}", self.family),
            None => self.family.to_string(),
        }
    }
}

/// Sink for [`ParamWrite`]s; the editor uses [`GuiParamWriter`], tests use a recorder.
pub trait ParamWriter {
    /// Apply one write; `false` when the id is unknown.
    fn write(&mut self, w: &ParamWrite) -> bool;
}

/// Writes through the host's `GuiContext` (GUI thread only).
pub struct GuiParamWriter<'a> {
    pub ctx: &'a dyn GuiContext,
    pub index: &'a ParamIndex,
}

impl ParamWriter for GuiParamWriter<'_> {
    fn write(&mut self, w: &ParamWrite) -> bool {
        let Some(ptr) = self.index.ptr(&w.host_id()) else {
            return false;
        };
        // SAFETY: the pointers come from the `Stage4Params` the editor keeps alive via `Arc`,
        // and this is only called from the GUI thread, which is where the host expects
        // parameter edits.
        unsafe {
            let norm = ptr.preview_normalized(w.plain);
            self.ctx.raw_begin_set_parameter(ptr);
            self.ctx.raw_set_parameter_normalized(ptr, norm);
            self.ctx.raw_end_set_parameter(ptr);
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::params::Stage4Params;
    use nih_plug::prelude::{ParamPtr, PluginApi, PluginState};
    use std::sync::Mutex;

    /// Records the begin / set / end calls the host would see.
    #[derive(Default)]
    struct MockHost {
        calls: Mutex<Vec<(&'static str, Option<f32>)>>,
    }

    impl GuiContext for MockHost {
        fn plugin_api(&self) -> PluginApi {
            PluginApi::Clap
        }
        fn request_resize(&self) -> bool {
            false
        }
        unsafe fn raw_begin_set_parameter(&self, _p: ParamPtr) {
            self.calls.lock().unwrap().push(("begin", None));
        }
        unsafe fn raw_set_parameter_normalized(&self, _p: ParamPtr, n: f32) {
            self.calls.lock().unwrap().push(("set", Some(n)));
        }
        unsafe fn raw_end_set_parameter(&self, _p: ParamPtr) {
            self.calls.lock().unwrap().push(("end", None));
        }
        fn get_state(&self) -> PluginState {
            unimplemented!()
        }
        fn set_state(&self, _state: PluginState) {}
    }

    #[test]
    fn gui_writer_wraps_each_write_in_begin_set_end_with_the_normalized_value() {
        let params = Stage4Params::default();
        let index = ParamIndex::new(&params);
        let host = MockHost::default();
        let mut w = GuiParamWriter {
            ctx: &host,
            index: &index,
        };
        assert!(w.write(&ParamWrite::global("master_level", 0.25)));
        let calls = host.calls.lock().unwrap().clone();
        assert_eq!(calls.len(), 3);
        assert_eq!(calls[0].0, "begin");
        assert_eq!(calls[1].0, "set");
        // master_level is a 0..1 linear float, so plain equals normalized.
        assert!((calls[1].1.unwrap() - 0.25).abs() < 1e-6);
        assert_eq!(calls[2].0, "end");
    }

    #[test]
    fn gui_writer_rejects_unknown_ids_without_touching_the_host() {
        let params = Stage4Params::default();
        let index = ParamIndex::new(&params);
        let host = MockHost::default();
        let mut w = GuiParamWriter {
            ctx: &host,
            index: &index,
        };
        assert!(!w.write(&ParamWrite::global("no_such_param", 1.0)));
        assert!(host.calls.lock().unwrap().is_empty());
    }

    #[test]
    fn host_id_appends_the_layer_suffix() {
        assert_eq!(
            ParamWrite::global("master_level", 0.5).host_id(),
            "master_level"
        );
        assert_eq!(
            ParamWrite::layer("fx_drive", 3, 0.5).host_id(),
            "fx_drive_3"
        );
    }
}
