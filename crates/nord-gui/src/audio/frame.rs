//! Stereo frame to interleaved device-buffer conversion (F32 / I16).

/// Converts a normalized float [-1.0, 1.0] to a signed 16-bit PCM integer [-32767, 32767].
#[inline]
pub fn f32_to_i16(sample: f32) -> i16 {
    (sample.clamp(-1.0, 1.0) * 32767.0).round() as i16
}

/// Writes a stereo frame [out_l, out_r] into an F32 audio buffer of `channels` width.
///
/// If `channels >= 2`:
/// - `frame[0] = out_l`
/// - `frame[1] = out_r`
/// - All additional channels `frame[2..channels]` are explicitly zeroed.
///
/// If `channels == 1`:
/// - `frame[0] = (out_l + out_r) * 0.5`
pub fn render_frame_to_f32(channels: usize, out_l: f32, out_r: f32, frame: &mut [f32]) {
    if channels >= 2 {
        frame[0] = out_l;
        frame[1] = out_r;
        for ch in frame[2..channels].iter_mut() {
            *ch = 0.0;
        }
    } else if channels == 1 && !frame.is_empty() {
        frame[0] = (out_l + out_r) * 0.5;
    }
}

/// Writes a stereo frame [out_l, out_r] into an I16 audio buffer of `channels` width.
///
/// If `channels >= 2`:
/// - `frame[0] = f32_to_i16(out_l)`
/// - `frame[1] = f32_to_i16(out_r)`
/// - All additional channels `frame[2..channels]` are explicitly zeroed.
///
/// If `channels == 1`:
/// - `frame[0] = f32_to_i16((out_l + out_r) * 0.5)`
pub fn render_frame_to_i16(channels: usize, out_l: f32, out_r: f32, frame: &mut [i16]) {
    if channels >= 2 {
        frame[0] = f32_to_i16(out_l);
        frame[1] = f32_to_i16(out_r);
        for ch in frame[2..channels].iter_mut() {
            *ch = 0;
        }
    } else if channels == 1 && !frame.is_empty() {
        frame[0] = f32_to_i16((out_l + out_r) * 0.5);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_frame_to_f32_and_extra_channel_zeroing() {
        // Multi-channel (4 channels, e.g. surround / quad interface)
        let mut frame_quad = [1.0f32, 1.0, 1.0, 1.0];
        render_frame_to_f32(4, 0.75, -0.5, &mut frame_quad);
        assert_eq!(frame_quad[0], 0.75);
        assert_eq!(frame_quad[1], -0.5);
        assert_eq!(frame_quad[2], 0.0);
        assert_eq!(frame_quad[3], 0.0);

        // Mono channel
        let mut frame_mono = [0.0f32];
        render_frame_to_f32(1, 0.4, 0.6, &mut frame_mono);
        assert!((frame_mono[0] - 0.5).abs() < 1e-6);
    }

    #[test]
    fn test_render_frame_to_i16_and_format_conversion() {
        assert_eq!(f32_to_i16(1.0), 32767);
        assert_eq!(f32_to_i16(-1.0), -32767);
        assert_eq!(f32_to_i16(0.0), 0);
        assert_eq!(f32_to_i16(1.5), 32767); // Clamped
        assert_eq!(f32_to_i16(-2.0), -32767); // Clamped

        let mut frame_quad = [999i16; 4];
        render_frame_to_i16(4, 1.0, -1.0, &mut frame_quad);
        assert_eq!(frame_quad[0], 32767);
        assert_eq!(frame_quad[1], -32767);
        assert_eq!(frame_quad[2], 0);
        assert_eq!(frame_quad[3], 0);

        let mut frame_mono = [0i16];
        render_frame_to_i16(1, 0.5, 0.5, &mut frame_mono);
        assert_eq!(frame_mono[0], 16384);
    }
}
