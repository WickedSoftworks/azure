//! Presets, their matching rules, and where they are kept.
//!
//! Pure, like `azure-color`: no Windows API and no idea what directory it
//! is writing to until one is handed to it. The watcher supplies a
//! foreground path and an executable name; everything here is a function
//! of those and the stored set.

pub mod preset;
pub mod store;

pub use preset::{
    ActivationMode, MatchKind, Preset, PresetError, PresetSet, DESKTOP_ID, SCHEMA_VERSION,
};
pub use store::{Loaded, Store};
