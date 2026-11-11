# DSP and real-time audio

- **Zero allocations on the audio path.** `render_frame()` and `process_block()` in `organ.rs`, `piano.rs`, `synth.rs`, `fx.rs`, and `engine.rs` must never allocate heap memory (`Vec::push`, `Box::new`, `String`), lock a `Mutex`, or perform file/network I/O.
- Pre-allocate all voice arrays, delay buffers, tonewheel phase accumulators, and filter states in `new(sample_rate)`.
- **Numerical stability:** Guard filter cutoffs to `[10.0, 0.49 * sample_rate]` and resonances to stable bounds so `NaN` or `Inf` can never propagate to the master bus.
- **Deterministic rendering:** Given the same sample rate, initial state, and MIDI event sequence, offline rendering must be 100% deterministic across platforms.
