//! Real-Time Audio Engine Host Thread & CPAL Output Callback & MIDIR Input.
//!
//! - `command`: `AudioCommand` + `AudioHandle` queue
//! - `apply`: command to engine state mapping
//! - `frame`: stereo frame to device-buffer conversion
//! - `io`: CPAL stream, MIDIR listener, raw MIDI bytes

mod apply;
mod command;
mod frame;
mod io;

pub use apply::apply_audio_command;
pub use command::{AudioCommand, AudioHandle};
pub use frame::{f32_to_i16, render_frame_to_f32, render_frame_to_i16};
pub use io::{handle_midi_bytes, start_audio_and_midi};
