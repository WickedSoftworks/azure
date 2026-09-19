//! Azure's standing on the machine: does it start with Windows, and is it
//! running elevated.
//!
//! A thin cfg-split over `win`, in the manner of `probe`. It exists so the
//! Tauri layer can ask these questions without a `#[cfg(windows)]` at
//! every call site, and so a non-Windows build answers honestly rather
//! than failing to compile.

#[cfg(windows)]
pub use crate::win::{autostart_enabled, is_elevated, restart_elevated, set_autostart, HIDDEN_FLAG};

#[cfg(not(windows))]
pub const HIDDEN_FLAG: &str = "--hidden";

#[cfg(not(windows))]
pub fn autostart_enabled() -> bool {
    false
}

#[cfg(not(windows))]
pub fn set_autostart(_enabled: bool) -> Result<(), String> {
    Err("starting with Windows is a Windows facility".to_string())
}

#[cfg(not(windows))]
pub fn is_elevated() -> bool {
    false
}

#[cfg(not(windows))]
pub fn restart_elevated() -> Result<(), String> {
    Err("elevation is a Windows facility".to_string())
}
