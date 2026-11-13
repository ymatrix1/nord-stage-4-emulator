//! Control value normalised to `0.0..=1.0`.

/// A control position clamped to `0.0..=1.0`; NaN maps to 0.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default)]
pub struct Normalized(f32);

impl Normalized {
    pub fn new(v: f32) -> Self {
        Self(if v.is_nan() { 0.0 } else { v.clamp(0.0, 1.0) })
    }

    pub fn get(self) -> f32 {
        self.0
    }

    /// Maps `v` in `min..=max` to a normalised position (`max == min` gives 0).
    pub fn from_range(v: f32, min: f32, max: f32) -> Self {
        if max == min {
            return Self(0.0);
        }
        Self::new((v - min) / (max - min))
    }

    /// Maps the position back to `min..=max`.
    pub fn to_range(self, min: f32, max: f32) -> f32 {
        min + self.0 * (max - min)
    }

    /// Returns the value moved by `delta`, clamped.
    pub fn nudged(self, delta: f32) -> Self {
        Self::new(self.0 + delta)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clamps_and_rejects_nan() {
        assert_eq!(Normalized::new(2.0).get(), 1.0);
        assert_eq!(Normalized::new(-1.0).get(), 0.0);
        assert_eq!(Normalized::new(f32::NAN).get(), 0.0);
    }

    #[test]
    fn test_range_round_trip_and_nudge() {
        let n = Normalized::from_range(1200.0, 0.0, 2400.0);
        assert!((n.get() - 0.5).abs() < 1e-6);
        assert!((n.to_range(0.0, 2400.0) - 1200.0).abs() < 1e-3);
        assert_eq!(Normalized::from_range(3.0, 3.0, 3.0).get(), 0.0);
        assert_eq!(n.nudged(0.9).get(), 1.0);
    }
}
