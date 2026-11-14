//! Golden vectors of the Farfisa divider (`farfisa_wheels.rs`).
//!
//! Regenerate with `python tools/organ_farfisa_oracle.py golden`: the bit-exact SHARC
//! model of `tools/organ_farfisa_ref.py` (verified against the oracle) runs on the
//! closed-form tables. Derived values only, no firmware data. Provenance:
//! `docs/specs/organ-engine.md` section 3.1.8.
#![allow(clippy::approx_constant, clippy::excessive_precision)]

pub(super) const FRAMES: [usize; 3] = [200, 1999, 4001];
pub(super) const OSCILLATORS: [usize; 3] = [0, 5, 11];
#[rustfmt::skip]
pub(super) const ROWS: [[[f32; 3]; 9]; 3] = [
    [
        [1.0, 1.0, 1.0],
        [1.0, 1.0, 1.0],
        [1.0, -0.190590500831604, -0.9018399119377136],
        [1.0, -0.6723325848579407, 1.0],
        [1.0, -0.9462544918060303, 1.0],
        [1.0, 1.0, 1.0],
        [1.0, -0.6936359405517578, 1.0],
        [-0.5426415801048279, -0.957250714302063, 1.0],
        [-0.542702853679657, 0.0, 0.0],
    ],
    [
        [1.0, 1.0, -0.3497653901576996],
        [1.0, 1.0, -0.7885897755622864],
        [1.0, 1.0, 1.0],
        [1.0, 1.0, 1.0],
        [1.0, 1.0, 1.0],
        [1.0, 1.0, 1.0],
        [1.0, 0.06101543828845024, 1.0],
        [1.0, -0.9999979138374329, 0.7234196066856384],
        [1.0, 0.0, 0.0],
    ],
    [
        [-0.7770135998725891, 1.0, 1.0],
        [1.0, 1.0, 0.4345549941062927],
        [1.0, 1.0, 0.029084132984280586],
        [1.0, 1.0, -0.47026780247688293],
        [1.0, 1.0, -0.8594549894332886],
        [1.0, 1.0, 1.0],
        [-0.7674974799156189, 1.0, 1.0],
        [-0.23783019185066223, 0.8993422389030457, 1.0],
        [1.0, 0.0, 0.0],
    ],
];

#[test]
fn golden_vectors_are_bounded_pulses() {
    assert!(ROWS
        .iter()
        .flatten()
        .flatten()
        .all(|v| v.abs() <= 1.0 + 1e-6));
    assert!(ROWS.iter().flatten().flatten().any(|v| *v == 1.0));
}
