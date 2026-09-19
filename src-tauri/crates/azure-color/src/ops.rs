use crate::channel::{ChannelId, ColorState};

/// Routing granularity. Never route a single channel: splitting the affine
/// group across both backends inserts a second clamp to [0,1] at the DWM
/// boundary and silently changes the result.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpGroup {
    /// Cross-channel mixing. Matrix only — a 1D LUT cannot mix channels.
    Mix,
    /// Per-channel gain and offset. Both backends can express it; the LUT is
    /// preferred because it survives exit and exclusive fullscreen.
    Affine,
    /// A power curve. LUT only — the matrix is affine.
    Power,
}

impl ChannelId {
    pub fn group(self) -> OpGroup {
        match self {
            ChannelId::Vibrance | ChannelId::Saturation | ChannelId::Hue => OpGroup::Mix,
            ChannelId::Brightness
            | ChannelId::Contrast
            | ChannelId::Temperature
            | ChannelId::Tint => OpGroup::Affine,
            ChannelId::Gamma => OpGroup::Power,
        }
    }
}

/// Saturation gain and hue rotation, collapsed to the two things a matrix
/// can actually carry.
///
/// Vibrance folds into `saturation` as a flat multiplier. That is the whole
/// of the approximation the spec warns about: true vibrance applies more
/// gain to less-saturated pixels, which is saturation-dependent and
/// therefore outside what any affine matrix can express. The routing report
/// says `Approximate` for exactly this reason.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MixOp {
    pub saturation: f32,
    pub hue_degrees: f32,
}

impl MixOp {
    pub fn from_state(s: &ColorState) -> Self {
        MixOp {
            saturation: (s.saturation as f32 / 100.0) * (s.vibrance as f32 / 100.0),
            hue_degrees: s.hue as f32,
        }
    }

    pub fn is_identity(&self) -> bool {
        (self.saturation - 1.0).abs() < 1e-6 && self.hue_degrees == 0.0
    }
}

/// Per-channel gain and offset.
///
/// `brightness` is a gain about black, `contrast` a gain about mid-grey,
/// `gains` the per-channel weighting that carries temperature and tint.
/// Composed in that order; see `Mat5::affine` and `Ramp::build`, which must
/// agree because either may carry this group.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AffineOp {
    pub brightness: f32,
    pub contrast: f32,
    pub gains: [f32; 3],
}

impl AffineOp {
    /// First-order channel weighting, not a Planckian white-point model.
    /// +/-20% at the ends of the slider.
    const TEMP_TINT_DEPTH: f32 = 0.20;

    pub fn from_state(s: &ColorState) -> Self {
        let t = s.temperature as f32 / 100.0;
        let n = s.tint as f32 / 100.0;
        AffineOp {
            brightness: 1.0 + s.brightness as f32 / 100.0,
            contrast: s.contrast as f32 / 100.0,
            gains: [
                1.0 + Self::TEMP_TINT_DEPTH * t,
                1.0 + Self::TEMP_TINT_DEPTH * n,
                1.0 - Self::TEMP_TINT_DEPTH * t,
            ],
        }
    }

    pub fn is_identity(&self) -> bool {
        self.brightness == 1.0 && self.contrast == 1.0 && self.gains == [1.0, 1.0, 1.0]
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PowerOp {
    /// Display gamma. 1.0 is identity; output is input^(1/gamma).
    pub gamma: f32,
}

impl PowerOp {
    pub fn from_state(s: &ColorState) -> Self {
        PowerOp { gamma: s.gamma as f32 / 100.0 }
    }

    pub fn is_identity(&self) -> bool {
        self.gamma == 1.0
    }
}
