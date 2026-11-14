//! Audio port layouts and block rendering of the plugin.
//!
//! The first layout is a stereo main port (outputs 1-2 of the Output menu) plus one stereo aux
//! output port (outputs 3-4). The second is the stereo-only fallback for hosts that offer one bus.
//! Both render through `Stage4Engine::render_frame_multi`; `render_frame` is its first two
//! channels, so the main port is bit-identical in either layout. Nothing here allocates or locks.

use nih_plug::prelude::*;
use nord_core::engine::Stage4Engine;

/// Main 1-2 plus aux 3-4, then stereo only. Hosts pick the first layout they can honour.
pub const LAYOUTS: &[AudioIOLayout] = &[
    AudioIOLayout {
        main_input_channels: None,
        main_output_channels: NonZeroU32::new(2),
        aux_input_ports: &[],
        aux_output_ports: &[new_nonzero_u32(2)],
        names: PortNames {
            layout: Some("Main 1-2 + Aux 3-4"),
            main_input: None,
            main_output: Some("Main 1-2"),
            aux_inputs: &[],
            aux_outputs: &["Aux 3-4"],
        },
    },
    AudioIOLayout {
        main_input_channels: None,
        main_output_channels: NonZeroU32::new(2),
        aux_input_ports: &[],
        aux_output_ports: &[],
        names: PortNames::const_default(),
    },
];

/// Renders `main_l.len()` frames to the main pair only (outputs 1-2).
pub fn render_main(engine: &mut Stage4Engine, main_l: &mut [f32], main_r: &mut [f32]) {
    for (l, r) in main_l.iter_mut().zip(main_r.iter_mut()) {
        let [out_l, out_r] = engine.render_frame();
        *l = out_l;
        *r = out_r;
    }
}

/// Renders to the main pair and the aux pair (outputs 3-4). Anything routed to outputs 3-4 lands in
/// the aux pair; when nothing is routed there the engine yields exact zeros, so it stays silent.
/// Frames beyond the shortest of the four slices are left untouched.
pub fn render_main_aux(
    engine: &mut Stage4Engine,
    main_l: &mut [f32],
    main_r: &mut [f32],
    aux_l: &mut [f32],
    aux_r: &mut [f32],
) {
    let frames = main_l
        .len()
        .min(main_r.len())
        .min(aux_l.len())
        .min(aux_r.len());
    for i in 0..frames {
        let f = engine.render_frame_multi();
        main_l[i] = f[0];
        main_r[i] = f[1];
        aux_l[i] = f[2];
        aux_r[i] = f[3];
    }
}

/// Renders a mono fold-down (a host that gave one channel); kept for odd hosts.
pub fn render_mono(engine: &mut Stage4Engine, mono: &mut [f32]) {
    for m in mono.iter_mut() {
        let [out_l, out_r] = engine.render_frame();
        *m = 0.5 * (out_l + out_r);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nord_core::midi::MidiEvent;

    fn engine() -> Box<Stage4Engine> {
        let mut e = Box::new(Stage4Engine::new(48_000.0));
        for note in [48u8, 60, 64, 67] {
            e.handle_midi(MidiEvent::NoteOn {
                channel: 0,
                note,
                velocity: 100,
            });
        }
        e
    }

    #[test]
    fn two_bus_layout_comes_first_and_stereo_only_is_the_fallback() {
        assert_eq!(LAYOUTS.len(), 2);
        assert_eq!(LAYOUTS[0].main_output_channels, NonZeroU32::new(2));
        assert_eq!(LAYOUTS[0].aux_output_ports, &[new_nonzero_u32(2)]);
        assert!(LAYOUTS[0].aux_input_ports.is_empty());
        assert_eq!(LAYOUTS[1].main_output_channels, NonZeroU32::new(2));
        assert!(LAYOUTS[1].aux_output_ports.is_empty());
        for layout in LAYOUTS {
            assert_eq!(layout.main_input_channels, None);
        }
    }

    #[test]
    fn port_names_carry_no_vendor_name() {
        let n = LAYOUTS[0].names;
        for s in [n.layout, n.main_output]
            .into_iter()
            .flatten()
            .chain(n.aux_outputs.iter().copied())
        {
            assert!(!s.to_lowercase().contains("clavia"), "{s}");
        }
        assert_eq!(n.aux_outputs, &["Aux 3-4"]);
    }

    #[test]
    fn main_is_bit_identical_in_both_layouts_and_aux_is_silent_by_default() {
        let (mut a, mut b) = (engine(), engine());
        for _ in 0..8 {
            let (mut l1, mut r1) = ([0.0f32; 64], [0.0f32; 64]);
            let (mut l2, mut r2) = ([0.0f32; 64], [0.0f32; 64]);
            let (mut xl, mut xr) = ([1.0f32; 64], [1.0f32; 64]);
            render_main(&mut a, &mut l1, &mut r1);
            render_main_aux(&mut b, &mut l2, &mut r2, &mut xl, &mut xr);
            assert_eq!(l1.map(f32::to_bits), l2.map(f32::to_bits));
            assert_eq!(r1.map(f32::to_bits), r2.map(f32::to_bits));
            assert!(xl.iter().chain(&xr).all(|&s| s == 0.0), "aux not silenced");
        }
    }

    #[test]
    fn a_source_routed_to_3_4_leaves_main_and_fills_aux() {
        let mut e = engine();
        e.output_routing.global_out[0] = 1; // Organ -> 3-4
        e.output_routing.global_out[2] = 1; // Piano A -> 3-4
        e.output_routing.global_out[4] = 1; // Synth A -> 3-4
        let (mut ml, mut mr, mut al, mut ar) =
            ([0.0f32; 256], [0.0f32; 256], [0.0f32; 256], [0.0f32; 256]);
        for _ in 0..4 {
            render_main_aux(&mut e, &mut ml, &mut mr, &mut al, &mut ar);
        }
        let peak = |s: &[f32]| s.iter().fold(0.0f32, |m, v| m.max(v.abs()));
        assert!(peak(&al).max(peak(&ar)) > 0.0, "aux is silent");
        assert!(al.iter().chain(&ar).all(|s| s.is_finite()));
        assert_eq!(peak(&ml).max(peak(&mr)), 0.0, "main kept the 3-4 source");
    }

    #[test]
    fn shortest_slice_bounds_the_frames_written() {
        let mut e = engine();
        let (mut ml, mut mr, mut al, mut ar) = ([9.0f32; 8], [9.0f32; 8], [9.0f32; 4], [9.0f32; 8]);
        render_main_aux(&mut e, &mut ml, &mut mr, &mut al, &mut ar);
        assert!(
            ml[4..].iter().all(|&s| s == 9.0),
            "wrote past the aux length"
        );
        assert!(ml[..4].iter().all(|&s| s != 9.0));
    }

    #[test]
    fn mono_is_the_mean_of_the_main_pair() {
        let (mut a, mut b) = (engine(), engine());
        let mut mono = [0.0f32; 32];
        let (mut l, mut r) = ([0.0f32; 32], [0.0f32; 32]);
        render_mono(&mut a, &mut mono);
        render_main(&mut b, &mut l, &mut r);
        for i in 0..32 {
            assert_eq!(mono[i], 0.5 * (l[i] + r[i]));
        }
    }
}
