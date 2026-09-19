//! Windows colour backends and the engine that owns display state.
//!
//! Everything that talks to the OS lives here; everything that decides what
//! to write lives in `azure-color`.

pub mod backend;
#[cfg(windows)]
pub mod win;
pub mod engine;
pub mod hotkeys;
pub mod machine;
pub mod mock;
pub mod probe;
pub mod thread;

pub use backend::{
    BackendError, DisplayInfo, LutTarget, MatrixBackend, RampBackend, Foreground, GammaRangeOutcome, RampLanding, Unavailable,
    CLAMP_TOLERANCE,
};
pub use engine::{ApplyReport, Core, StageLanding};
pub use hotkeys::{HotkeyEvent, Hotkeys, Outcome, Phase, Registration};
pub use machine::{autostart_enabled, is_elevated, restart_elevated, set_autostart, HIDDEN_FLAG};
pub use probe::{environment, real_core, unlock_gamma_range};
pub use thread::{EngineDown, EngineHandle, Snapshot};
pub use azure_presets::{ActivationMode, MatchKind, Preset, PresetError};
pub use azure_settings::{Action, BindingSet, Chord, Conflict, Settings};
