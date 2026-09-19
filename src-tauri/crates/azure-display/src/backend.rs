use azure_color::{Mat5, Ramp};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use ts_rs::TS;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DisplayInfo {
    /// Stable device key, used to target the LUT.
    pub key: String,
    pub name: String,
    pub primary: bool,
    pub hdr: bool,
}

/// The matrix is desktop-wide; the LUT is the only per-display stage.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum LutTarget {
    All,
    One(String),
}

#[derive(Debug, Error)]
pub enum BackendError {
    #[error("{0}")]
    Unavailable(String),
    #[error("{0}")]
    Rejected(String),
    #[error("the write was accepted but readback differs by {deviation} of 65535")]
    NotApplied { deviation: u16 },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RampLanding {
    pub display: String,
    pub deviation: u16,
    pub clamped: bool,
}

pub trait MatrixBackend: Send {
    fn name(&self) -> &'static str;
    fn apply(&mut self, m: &Mat5) -> Result<(), BackendError>;
    fn clear(&mut self) -> Result<(), BackendError>;
}

pub trait RampBackend: Send {
    fn name(&self) -> &'static str;
    fn displays(&self) -> Vec<DisplayInfo>;
    /// Implementations MUST read the ramp back and report the deviation.
    /// `SetDeviceGammaRamp` returns TRUE while silently not applying.
    fn apply(&mut self, target: &LutTarget, r: &Ramp) -> Result<Vec<RampLanding>, BackendError>;
    fn clear(&mut self) -> Result<(), BackendError>;
}

/// What was in front: its full path when the process would give one up,
/// and always its executable name.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Foreground {
    pub path: Option<String>,
    pub exe: String,
}

/// What came of asking Windows to lift the GDI gamma clamp.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", rename_all_fields = "camelCase", tag = "kind")]
#[ts(export)]
pub enum GammaRangeOutcome {
    /// Written. Windows reads this at sign-in, so it is not live yet.
    Unlocked { requires_sign_out: bool },
    /// HKLM is not writable without elevation, and Azure does not elevate
    /// itself behind the user's back.
    NeedsElevation,
    Failed { reason: String },
}

/// Quantisation noise between a written and a read-back ramp is a few
/// counts; anything past this is the driver applying something else.
pub const CLAMP_TOLERANCE: u16 = 512;

/// A stage that is not there. It carries the reason it is not there and
/// returns it from every call.
///
/// This is deliberately not a mock: a mock reports success, and a stage
/// that cannot reach the panel must never report success. Using one of
/// these keeps the failure visible all the way to the event log.
pub struct Unavailable {
    reason: String,
}

impl Unavailable {
    pub fn new(reason: String) -> Self {
        Unavailable { reason }
    }
}

impl MatrixBackend for Unavailable {
    fn name(&self) -> &'static str {
        "matrix (unavailable)"
    }

    fn apply(&mut self, _m: &Mat5) -> Result<(), BackendError> {
        Err(BackendError::Unavailable(self.reason.clone()))
    }

    fn clear(&mut self) -> Result<(), BackendError> {
        // Nothing was ever written, so there is nothing to put back.
        Ok(())
    }
}

impl RampBackend for Unavailable {
    fn name(&self) -> &'static str {
        "lut (unavailable)"
    }

    fn displays(&self) -> Vec<DisplayInfo> {
        Vec::new()
    }

    fn apply(&mut self, _t: &LutTarget, _r: &Ramp) -> Result<Vec<RampLanding>, BackendError> {
        Err(BackendError::Unavailable(self.reason.clone()))
    }

    fn clear(&mut self) -> Result<(), BackendError> {
        Ok(())
    }
}
