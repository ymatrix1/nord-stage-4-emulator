# Audio Host Specification

## Scope

Defines the desktop audio host boundary in `crates/nord-gui/src/audio.rs`. The
host adapts the Stage 4 engine's stereo `f32` frames to the output format and
channel layout reported by CPAL.

## Output Format and Channels

- `f32` and signed `i16` device buffers are supported.
- Stereo engine output maps to the first two device channels.
- Mono output is the arithmetic mean of left and right.
- Channels after the first two are always zero-filled.
- Unsupported sample formats are reported and the stream is not started.
- A device reporting zero channels is rejected before callback setup.
- The callback performs no heap allocation, locking, or I/O. It drains queued
  commands with nonblocking receives and renders directly into the device
  buffer.

## MIDI Command Delivery

MIDI events use blocking queue delivery so a full command queue cannot silently
drop a Note Off and leave a voice held. Non-MIDI UI parameter updates remain
best-effort and nonblocking. The audio callback drains commands at the start of
each output buffer.

## Verification

Unit tests cover mono downmix, stereo and extra-channel layout, `f32` to `i16`
clamping/conversion, zero-channel callback input, and Note Off delivery after
the bounded command queue is saturated. CPAL device enumeration remains a
manual integration check because CI runners do not provide a physical audio
device.
