use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// The eight controls. Order is the order they are drawn in the field, which
/// is the order of the device path: mix first, then the tone curve.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum ChannelId {
    Vibrance,
    Saturation,
    Hue,
    Brightness,
    Contrast,
    Gamma,
    Temperature,
    Tint,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum Unit {
    Percent,
    Degrees,
    Factor,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ChannelRange {
    pub min: i32,
    pub max: i32,
    pub neutral: i32,
    pub step: i32,
    pub unit: Unit,
}

impl ChannelId {
    pub const ALL: [ChannelId; 8] = [
        ChannelId::Vibrance,
        ChannelId::Saturation,
        ChannelId::Hue,
        ChannelId::Brightness,
        ChannelId::Contrast,
        ChannelId::Gamma,
        ChannelId::Temperature,
        ChannelId::Tint,
    ];

    /// Fixed-width key. The field is a table; keys must not vary in width.
    pub fn key(self) -> &'static str {
        match self {
            ChannelId::Vibrance => "VIB",
            ChannelId::Saturation => "SAT",
            ChannelId::Hue => "HUE",
            ChannelId::Brightness => "BRT",
            ChannelId::Contrast => "CON",
            ChannelId::Gamma => "GAM",
            ChannelId::Temperature => "TMP",
            ChannelId::Tint => "TNT",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            ChannelId::Vibrance => "VIBRANCE",
            ChannelId::Saturation => "SATURATION",
            ChannelId::Hue => "HUE",
            ChannelId::Brightness => "BRIGHTNESS",
            ChannelId::Contrast => "CONTRAST",
            ChannelId::Gamma => "GAMMA",
            ChannelId::Temperature => "TEMPERATURE",
            ChannelId::Tint => "TINT",
        }
    }

    pub fn range(self) -> ChannelRange {
        use ChannelId::*;
        let (min, max, neutral, unit) = match self {
            Vibrance | Saturation => (0, 200, 100, Unit::Percent),
            Hue => (-180, 180, 0, Unit::Degrees),
            Brightness => (-50, 50, 0, Unit::Percent),
            Contrast => (30, 200, 100, Unit::Factor),
            Gamma => (40, 280, 100, Unit::Factor),
            Temperature | Tint => (-100, 100, 0, Unit::Percent),
        };
        ChannelRange { min, max, neutral, step: 1, unit }
    }
}

/// Channel values in UI units: percent, degrees, or factor x100. Integer on
/// purpose — this is what the sliders emit and what the log prints.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ColorState {
    pub vibrance: i32,
    pub saturation: i32,
    pub hue: i32,
    pub brightness: i32,
    pub contrast: i32,
    pub gamma: i32,
    pub temperature: i32,
    pub tint: i32,
}

impl ColorState {
    pub fn neutral() -> Self {
        let mut s = ColorState {
            vibrance: 0,
            saturation: 0,
            hue: 0,
            brightness: 0,
            contrast: 0,
            gamma: 0,
            temperature: 0,
            tint: 0,
        };
        for id in ChannelId::ALL {
            *s.slot(id) = id.range().neutral;
        }
        s
    }

    fn slot(&mut self, id: ChannelId) -> &mut i32 {
        match id {
            ChannelId::Vibrance => &mut self.vibrance,
            ChannelId::Saturation => &mut self.saturation,
            ChannelId::Hue => &mut self.hue,
            ChannelId::Brightness => &mut self.brightness,
            ChannelId::Contrast => &mut self.contrast,
            ChannelId::Gamma => &mut self.gamma,
            ChannelId::Temperature => &mut self.temperature,
            ChannelId::Tint => &mut self.tint,
        }
    }

    pub fn get(&self, id: ChannelId) -> i32 {
        let mut copy = *self;
        *copy.slot(id)
    }

    /// Clamps. A value outside the range is a bug upstream, not a reason to
    /// write something the panel cannot show.
    pub fn set(&mut self, id: ChannelId, value: i32) {
        let r = id.range();
        *self.slot(id) = value.clamp(r.min, r.max);
    }

    pub fn is_neutral(&self, id: ChannelId) -> bool {
        self.get(id) == id.range().neutral
    }

    pub fn touched(&self) -> impl Iterator<Item = ChannelId> + '_ {
        ChannelId::ALL.into_iter().filter(move |id| !self.is_neutral(*id))
    }
}

impl Default for ColorState {
    fn default() -> Self {
        Self::neutral()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn neutral_state_is_neutral_on_every_channel() {
        let s = ColorState::neutral();
        for id in ChannelId::ALL {
            assert!(s.is_neutral(id), "{id:?} not neutral in ColorState::neutral()");
            assert_eq!(s.get(id), id.range().neutral);
        }
        assert_eq!(s.touched().count(), 0);
    }

    #[test]
    fn set_clamps_to_the_channel_range() {
        let mut s = ColorState::neutral();
        s.set(ChannelId::Gamma, 9000);
        assert_eq!(s.get(ChannelId::Gamma), 280);
        s.set(ChannelId::Gamma, -9000);
        assert_eq!(s.get(ChannelId::Gamma), 40);
        s.set(ChannelId::Hue, -181);
        assert_eq!(s.get(ChannelId::Hue), -180);
    }

    #[test]
    fn keys_are_fixed_width_because_the_field_is_a_table() {
        for id in ChannelId::ALL {
            assert_eq!(id.key().len(), 3, "{id:?} key is not 3 characters");
        }
    }

    #[test]
    fn touched_lists_only_moved_channels() {
        let mut s = ColorState::neutral();
        s.set(ChannelId::Vibrance, 156);
        s.set(ChannelId::Gamma, 112);
        let touched: Vec<_> = s.touched().collect();
        assert_eq!(touched, vec![ChannelId::Vibrance, ChannelId::Gamma]);
    }
}
