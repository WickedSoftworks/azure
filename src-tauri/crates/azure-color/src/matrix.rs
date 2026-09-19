use crate::ops::{AffineOp, MixOp};

/// Rec.709 luma weights. sRGB primaries, so these are the right ones.
const LR: f32 = 0.2126;
const LG: f32 = 0.7152;
const LB: f32 = 0.0722;

/// A 5x5 colour matrix in the layout `MAGCOLOREFFECT` wants: row-major,
/// row-vector convention, `out = [r g b a 1] * m`. Column 4 is pinned to
/// [0,0,0,0,1]; row 4 is the offset row.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mat5(pub [[f32; 5]; 5]);

impl Mat5 {
    pub const IDENTITY: Mat5 = Mat5([
        [1.0, 0.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 0.0, 1.0],
    ]);

    /// Apply `self`, then `rhs`. Row-vector convention, so this is the
    /// plain product self*rhs and the reading order is the device order.
    #[allow(clippy::needless_range_loop)] // a matrix product reads better with indices
    pub fn then(self, rhs: Mat5) -> Mat5 {
        let mut out = [[0.0f32; 5]; 5];
        for i in 0..5 {
            for j in 0..5 {
                let mut acc = 0.0;
                for k in 0..5 {
                    acc += self.0[i][k] * rhs.0[k][j];
                }
                out[i][j] = acc;
            }
        }
        Mat5(out)
    }

    /// Unclamped on purpose: the caller decides where the clamp goes, and
    /// the whole point of keeping the affine group on one backend is to not
    /// clamp twice.
    pub fn apply(&self, rgb: [f32; 3]) -> [f32; 3] {
        let v = [rgb[0], rgb[1], rgb[2], 1.0, 1.0];
        let mut out = [0.0f32; 3];
        for (j, o) in out.iter_mut().enumerate() {
            *o = (0..5).map(|i| v[i] * self.0[i][j]).sum();
        }
        out
    }

    pub fn is_identity(&self) -> bool {
        self.0.iter().zip(Mat5::IDENTITY.0.iter()).all(|(r, e)| {
            r.iter().zip(e.iter()).all(|(a, b)| (a - b).abs() < 1e-6)
        })
    }

    pub fn as_flat(&self) -> [f32; 25] {
        let mut out = [0.0f32; 25];
        for (i, row) in self.0.iter().enumerate() {
            out[i * 5..i * 5 + 5].copy_from_slice(row);
        }
        out
    }

    /// Luma-preserving saturation. `s` is a gain: 0 collapses to luma, 1 is
    /// identity, above 1 pushes away from the grey axis.
    pub fn saturation(s: f32) -> Mat5 {
        let (r, g, b) = ((1.0 - s) * LR, (1.0 - s) * LG, (1.0 - s) * LB);
        Mat5([
            [r + s, r, r, 0.0, 0.0],
            [g, g + s, g, 0.0, 0.0],
            [b, b, b + s, 0.0, 0.0],
            [0.0, 0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 0.0, 1.0],
        ])
    }

    /// Rotation about the grey axis, the matrix from the SVG filter effects
    /// `feColorMatrix type="hueRotate"` definition, transposed into the
    /// row-vector layout. Row sums are exactly 1, so grey is untouched.
    pub fn hue(degrees: f32) -> Mat5 {
        let (s, c) = degrees.to_radians().sin_cos();
        // Column-vector rows from the spec, transposed on write.
        let a = [
            [
                0.213 + 0.787 * c - 0.213 * s,
                0.715 - 0.715 * c - 0.715 * s,
                0.072 - 0.072 * c + 0.928 * s,
            ],
            [
                0.213 - 0.213 * c + 0.143 * s,
                0.715 + 0.285 * c + 0.140 * s,
                0.072 - 0.072 * c - 0.283 * s,
            ],
            [
                0.213 - 0.213 * c - 0.787 * s,
                0.715 - 0.715 * c + 0.715 * s,
                0.072 + 0.928 * c + 0.072 * s,
            ],
        ];
        Mat5([
            [a[0][0], a[1][0], a[2][0], 0.0, 0.0],
            [a[0][1], a[1][1], a[2][1], 0.0, 0.0],
            [a[0][2], a[1][2], a[2][2], 0.0, 0.0],
            [0.0, 0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 0.0, 1.0],
        ])
    }

    /// The affine group as a matrix, for when no LUT can carry it.
    ///
    /// x1 = x * brightness; x2 = (x1 - 0.5) * contrast + 0.5; x3 = x2 * gain
    /// collapses to gain*contrast*brightness*x + gain*0.5*(1 - contrast).
    /// `Ramp::build` evaluates the same three steps in the same order.
    pub fn affine(op: &AffineOp) -> Mat5 {
        let mut m = Mat5::IDENTITY;
        for (c, gain) in op.gains.iter().enumerate() {
            m.0[c][c] = gain * op.contrast * op.brightness;
            m.0[4][c] = gain * 0.5 * (1.0 - op.contrast);
        }
        m
    }

    /// The whole mix group: saturation (carrying vibrance) then hue.
    pub fn from_mix(op: &MixOp) -> Mat5 {
        Mat5::saturation(op.saturation).then(Mat5::hue(op.hue_degrees))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops::AffineOp;

    fn close(a: [f32; 3], b: [f32; 3]) -> bool {
        a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-4)
    }

    #[test]
    fn identity_leaves_colour_alone() {
        assert!(close(Mat5::IDENTITY.apply([0.2, 0.5, 0.9]), [0.2, 0.5, 0.9]));
    }

    #[test]
    fn saturation_one_is_identity() {
        assert!(Mat5::saturation(1.0).is_identity());
    }

    #[test]
    fn saturation_zero_collapses_to_luma() {
        let out = Mat5::saturation(0.0).apply([1.0, 0.0, 0.0]);
        assert!(close(out, [0.2126, 0.2126, 0.2126]), "{out:?}");
    }

    #[test]
    fn saturation_preserves_luma_at_any_gain() {
        let luma = |c: [f32; 3]| 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
        let src = [0.8, 0.3, 0.1];
        for gain in [0.0, 0.5, 1.0, 1.56, 2.0] {
            let out = Mat5::saturation(gain).apply(src);
            assert!((luma(out) - luma(src)).abs() < 1e-4, "gain {gain} moved luma");
        }
    }

    #[test]
    fn hue_zero_is_identity() {
        assert!(Mat5::hue(0.0).is_identity());
    }

    #[test]
    fn hue_leaves_grey_alone() {
        for deg in [0.0, 37.0, 90.0, -120.0, 180.0] {
            let out = Mat5::hue(deg).apply([0.5, 0.5, 0.5]);
            assert!(close(out, [0.5, 0.5, 0.5]), "{deg} deg moved grey to {out:?}");
        }
    }

    #[test]
    fn hue_is_periodic_over_360_degrees() {
        let src = [0.9, 0.4, 0.2];
        assert!(close(Mat5::hue(360.0).apply(src), Mat5::hue(0.0).apply(src)));
    }

    #[test]
    fn composition_applies_left_operand_first() {
        // Desaturate, then halve. Order matters: the reverse is a different
        // picture, and the device order is not ours to choose.
        let desat_then_dim = Mat5::saturation(0.0).then(Mat5::affine(&AffineOp {
            brightness: 0.5,
            contrast: 1.0,
            gains: [1.0, 1.0, 1.0],
        }));
        let out = desat_then_dim.apply([1.0, 0.0, 0.0]);
        assert!(close(out, [0.1063, 0.1063, 0.1063]), "{out:?}");
    }

    #[test]
    fn affine_contrast_pivots_on_mid_grey() {
        let m = Mat5::affine(&AffineOp {
            brightness: 1.0,
            contrast: 2.0,
            gains: [1.0, 1.0, 1.0],
        });
        assert!(close(m.apply([0.5, 0.5, 0.5]), [0.5, 0.5, 0.5]));
        assert!(close(m.apply([0.75, 0.75, 0.75]), [1.0, 1.0, 1.0]));
    }

    #[test]
    fn flat_layout_is_row_major_with_a_pinned_last_column() {
        let f = Mat5::IDENTITY.as_flat();
        assert_eq!(f.len(), 25);
        assert_eq!(f[24], 1.0);
        assert_eq!(f[4], 0.0);
    }
}
