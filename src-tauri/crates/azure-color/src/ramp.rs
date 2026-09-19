use crate::ops::{AffineOp, PowerOp};

/// The scanout lookup table exactly as `SetDeviceGammaRamp` wants it: three
/// planes of 256 sixteen-bit entries, red first.
///
/// This is the stage that survives process exit and keeps working in
/// exclusive fullscreen, so everything that can be expressed here is.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Ramp(pub [[u16; 256]; 3]);

impl std::fmt::Debug for Ramp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Ramp(r0={} r128={} r255={}, g128={}, b128={})",
            self.0[0][0], self.0[0][128], self.0[0][255], self.0[1][128], self.0[2][128]
        )
    }
}

impl Ramp {
    pub fn identity() -> Ramp {
        let mut out = [[0u16; 256]; 3];
        for i in 0..256 {
            let v = (i as u32 * 65535 / 255) as u16;
            out[0][i] = v;
            out[1][i] = v;
            out[2][i] = v;
        }
        Ramp(out)
    }

    /// Evaluates the affine group then the power group, in that order,
    /// because that is the order of the device path. The three affine steps
    /// are the same three `Mat5::affine` collapses, so a fallback to the
    /// matrix is the same picture minus the gamma.
    pub fn build(affine: &AffineOp, power: &PowerOp) -> Ramp {
        let exponent = 1.0 / power.gamma.max(1e-3);
        let mut out = [[0u16; 256]; 3];
        for c in 0..3 {
            for i in 0..256 {
                let x = i as f32 / 255.0;
                let x = x * affine.brightness;
                let x = (x - 0.5) * affine.contrast + 0.5;
                let x = x * affine.gains[c];
                // Clamp before the power: a negative base with a fractional
                // exponent is NaN, and NaN on a display is a black screen.
                let x = x.clamp(0.0, 1.0).powf(exponent);
                out[c][i] = (x.clamp(0.0, 1.0) * 65535.0).round() as u16;
            }
        }
        Ramp(out)
    }

    pub fn is_identity(&self) -> bool {
        *self == Ramp::identity()
    }

    /// Worst-case distance between two ramps. Used to decide whether a write
    /// landed: `SetDeviceGammaRamp` reports success while silently applying
    /// something else, so the only honest check is reading it back.
    pub fn max_deviation(&self, other: &Ramp) -> u16 {
        let mut worst = 0u16;
        for c in 0..3 {
            for i in 0..256 {
                worst = worst.max(self.0[c][i].abs_diff(other.0[c][i]));
            }
        }
        worst
    }

    pub fn as_gdi(&self) -> [u16; 768] {
        let mut flat = [0u16; 768];
        for c in 0..3 {
            flat[c * 256..c * 256 + 256].copy_from_slice(&self.0[c]);
        }
        flat
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const UNIT: AffineOp = AffineOp { brightness: 1.0, contrast: 1.0, gains: [1.0, 1.0, 1.0] };
    const FLAT: PowerOp = PowerOp { gamma: 1.0 };

    #[test]
    fn neutral_input_builds_the_identity_ramp() {
        assert_eq!(Ramp::build(&UNIT, &FLAT), Ramp::identity());
        assert!(Ramp::build(&UNIT, &FLAT).is_identity());
    }

    #[test]
    fn identity_ramp_spans_the_full_16_bit_range() {
        let r = Ramp::identity();
        assert_eq!(r.0[0][0], 0);
        assert_eq!(r.0[0][255], 65535);
        assert_eq!(r.0[2][255], 65535);
    }

    #[test]
    fn every_ramp_is_monotonic_non_decreasing() {
        let cases = [
            (
                AffineOp { brightness: 1.3, contrast: 1.4, gains: [1.2, 1.0, 0.8] },
                PowerOp { gamma: 2.8 },
            ),
            (
                AffineOp { brightness: 0.5, contrast: 0.3, gains: [0.8, 1.0, 1.2] },
                PowerOp { gamma: 0.4 },
            ),
            (UNIT, PowerOp { gamma: 1.12 }),
        ];
        for (affine, power) in cases {
            let r = Ramp::build(&affine, &power);
            for c in 0..3 {
                for i in 1..256 {
                    assert!(r.0[c][i] >= r.0[c][i - 1], "channel {c} dips at {i}");
                }
            }
        }
    }

    #[test]
    fn gamma_above_one_lifts_the_midtone() {
        let lifted = Ramp::build(&UNIT, &PowerOp { gamma: 2.2 });
        assert!(lifted.0[0][128] > Ramp::identity().0[0][128]);
    }

    #[test]
    fn gamma_below_one_lowers_the_midtone() {
        let crushed = Ramp::build(&UNIT, &PowerOp { gamma: 0.5 });
        assert!(crushed.0[0][128] < Ramp::identity().0[0][128]);
    }

    #[test]
    fn contrast_pivots_on_mid_grey() {
        let r = Ramp::build(
            &AffineOp { brightness: 1.0, contrast: 1.5, gains: [1.0, 1.0, 1.0] },
            &FLAT,
        );
        let identity_mid = Ramp::identity().0[0][128];
        assert!((r.0[0][128] as i32 - identity_mid as i32).abs() < 300);
        assert!(r.0[0][200] > Ramp::identity().0[0][200]);
        assert!(r.0[0][40] < Ramp::identity().0[0][40]);
    }

    #[test]
    fn temperature_gain_separates_the_channels() {
        let warm = Ramp::build(
            &AffineOp { brightness: 1.0, contrast: 1.0, gains: [1.2, 1.0, 0.8] },
            &FLAT,
        );
        assert!(warm.0[0][200] > warm.0[1][200]);
        assert!(warm.0[1][200] > warm.0[2][200]);
    }

    #[test]
    fn values_clamp_instead_of_wrapping() {
        let blown = Ramp::build(
            &AffineOp { brightness: 1.5, contrast: 2.0, gains: [1.2, 1.2, 1.2] },
            &FLAT,
        );
        assert_eq!(blown.0[0][255], 65535);
        // Contrast above 1 drives the bottom of the range below zero. It
        // must floor at black, not wrap to white. (Contrast below 1 does
        // the opposite and lifts the black point, which is why the crushed
        // case here is a high-contrast one.)
        let crushed = Ramp::build(
            &AffineOp { brightness: 1.0, contrast: 2.0, gains: [1.0, 1.0, 1.0] },
            &PowerOp { gamma: 0.4 },
        );
        assert_eq!(crushed.0[0][0], 0);
        assert_eq!(crushed.0[1][0], 0);
    }

    #[test]
    fn deviation_measures_the_worst_entry() {
        let a = Ramp::identity();
        let mut b = Ramp::identity();
        b.0[1][7] = b.0[1][7].saturating_add(900);
        assert_eq!(a.max_deviation(&b), 900);
        assert_eq!(a.max_deviation(&a), 0);
    }

    #[test]
    fn gdi_layout_is_three_contiguous_planes() {
        let r = Ramp::build(
            &AffineOp { brightness: 1.0, contrast: 1.0, gains: [1.2, 1.0, 0.8] },
            &FLAT,
        );
        let flat = r.as_gdi();
        assert_eq!(flat.len(), 768);
        assert_eq!(flat[0], r.0[0][0]);
        assert_eq!(flat[256], r.0[1][0]);
        assert_eq!(flat[512], r.0[2][0]);
    }
}
