//! Settings: what keys do what, and whether Azure starts with Windows.
//!
//! Pure, like `azure-color` and `azure-presets`. A chord here is a value,
//! not a registration — nothing in this crate can ask Windows for a key or
//! be told no. `azure-display::win::hotkey` does that, and reports what it
//! was told back against the binding that earned it.

pub mod chord;
pub mod settings;
pub mod store;

pub use chord::{Chord, ChordError, Key, Modifiers};
pub use settings::{Action, BindingSet, Conflict, Settings, ACTIONS, SCHEMA_VERSION};
pub use store::{Loaded, Store};
