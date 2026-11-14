//! Pass runner shared by the Core1/Core2 section binaries (`sharc-org`, `sharc-pass`): one
//! processing pass started by a mailbox write or by the P4I receive interrupt ([`pass`]), ARM
//! style `(word 1, word 2)` message injection ([`msg`]) and audio buffer capture with WAV output
//! ([`audio`]). Moved out of `bin/sharc_org/` so that several binaries use one implementation.
pub mod audio;
pub mod msg;
pub mod pass;

/// Parses `0x` hex or decimal.
pub fn parse_number(s: &str) -> Option<u32> {
    match s.strip_prefix("0x") {
        Some(hex) => u32::from_str_radix(hex, 16).ok(),
        None => s.parse().ok(),
    }
}

/// A data value: number, `0x` hex or `f<float>` (float bit pattern).
pub fn parse_value(s: &str) -> Option<u32> {
    match s.strip_prefix('f') {
        Some(f) => f.parse::<f32>().ok().map(f32::to_bits),
        None => parse_number(s),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_and_float_values_parse() {
        assert_eq!(parse_number("0x10"), Some(16));
        assert_eq!(parse_number("17"), Some(17));
        assert_eq!(parse_number("zz"), None);
        assert_eq!(parse_value("f1.0"), Some(0x3F80_0000));
        assert_eq!(parse_value("0x3f800000"), Some(0x3F80_0000));
        assert_eq!(parse_value("fx"), None);
    }
}
