use azure_color::{Mat5, Ramp};

use crate::backend::{
    BackendError, DisplayInfo, LutTarget, MatrixBackend, RampBackend, RampLanding, CLAMP_TOLERANCE,
};

#[derive(Default)]
pub struct MockMatrix {
    pub last: Option<Mat5>,
    pub clears: u32,
    pub fail: bool,
}

impl MockMatrix {
    pub fn failing() -> Self {
        MockMatrix { fail: true, ..Default::default() }
    }
}

impl MatrixBackend for MockMatrix {
    fn name(&self) -> &'static str {
        "mock-matrix"
    }

    fn apply(&mut self, m: &Mat5) -> Result<(), BackendError> {
        if self.fail {
            return Err(BackendError::Unavailable("mock matrix is unavailable".into()));
        }
        self.last = Some(*m);
        Ok(())
    }

    fn clear(&mut self) -> Result<(), BackendError> {
        self.clears += 1;
        self.last = None;
        Ok(())
    }
}

pub struct MockRamp {
    pub last: Option<Ramp>,
    pub clears: u32,
    pub displays: Vec<DisplayInfo>,
    /// Deviation the simulated readback reports, standing in for the GDI
    /// range clamp that accepts a write and applies something else.
    pub clamp_deviation: u16,
}

impl Default for MockRamp {
    fn default() -> Self {
        MockRamp {
            last: None,
            clears: 0,
            displays: vec![DisplayInfo {
                key: "MOCK0".into(),
                name: "MOCK DISPLAY".into(),
                primary: true,
                hdr: false,
            }],
            clamp_deviation: 0,
        }
    }
}

impl MockRamp {
    pub fn clamping(deviation: u16) -> Self {
        MockRamp { clamp_deviation: deviation, ..Default::default() }
    }
}

impl RampBackend for MockRamp {
    fn name(&self) -> &'static str {
        "mock-lut"
    }

    fn displays(&self) -> Vec<DisplayInfo> {
        self.displays.clone()
    }

    fn apply(&mut self, target: &LutTarget, r: &Ramp) -> Result<Vec<RampLanding>, BackendError> {
        self.last = Some(*r);
        Ok(self
            .displays
            .iter()
            .filter(|d| match target {
                LutTarget::All => true,
                LutTarget::One(key) => &d.key == key,
            })
            .map(|d| RampLanding {
                display: d.key.clone(),
                deviation: self.clamp_deviation,
                clamped: self.clamp_deviation > CLAMP_TOLERANCE,
            })
            .collect())
    }

    fn clear(&mut self) -> Result<(), BackendError> {
        self.clears += 1;
        self.last = None;
        Ok(())
    }
}
