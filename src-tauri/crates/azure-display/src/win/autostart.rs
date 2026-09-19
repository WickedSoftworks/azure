//! Starting with Windows, and stopping.
//!
//! A `Run` value under `HKEY_CURRENT_USER`, which is the lightest way to
//! do this: no scheduled task, no service, no elevation, and a player can
//! see and remove it from Task Manager's Startup tab without going through
//! Azure at all. Per-user because presets are per-user — two people on one
//! machine do not share vibrance, and they do not share this either.
//!
//! Written directly rather than through a plugin. The registry code for
//! the ICM gamma range already lives in this crate, and a dependency for
//! one string value would be a dependency for nothing.

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
use windows::Win32::System::Registry::{
    RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ,
};

const RUN_KEY: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
const VALUE: PCWSTR = w!("Azure");

/// The switch that tells a launched Azure to stay in the tray.
///
/// Autostart exists so the preset is already right when the game opens. A
/// window appearing at sign-in would be the opposite of the point.
pub const HIDDEN_FLAG: &str = "--hidden";

/// Whether Azure is registered to start with Windows.
///
/// Reads the value rather than remembering what was written: a player can
/// remove it from Task Manager, and the settings screen should agree with
/// the machine rather than with the settings file.
pub fn autostart_enabled() -> bool {
    let mut size = 0u32;
    // SAFETY: read-only. A null buffer asks only for the size, which is
    // how the value's existence is tested without allocating for it.
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            RUN_KEY,
            VALUE,
            RRF_RT_REG_SZ,
            None,
            None,
            Some(&mut size),
        )
    };
    status == ERROR_SUCCESS
}

/// Registers or removes the `Run` value. Returns what Windows said, so a
/// failure reaches the interface instead of a toggle that quietly springs
/// back.
pub fn set_autostart(enabled: bool) -> Result<(), String> {
    if !enabled {
        // SAFETY: deletes one value under a per-user key.
        let status = unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, RUN_KEY, VALUE) };
        return match status {
            s if s == ERROR_SUCCESS || s == ERROR_FILE_NOT_FOUND => Ok(()),
            s => Err(format!("could not remove Azure from startup ({})", s.0)),
        };
    }

    let command = command_line()?;
    let wide: Vec<u16> = command.encode_utf16().chain(std::iter::once(0)).collect();
    // SAFETY: a single string write to a per-user Run key. The size counts
    // bytes including the terminating null, as REG_SZ requires.
    let status = unsafe {
        RegSetKeyValueW(
            HKEY_CURRENT_USER,
            RUN_KEY,
            VALUE,
            REG_SZ.0,
            Some(wide.as_ptr() as *const std::ffi::c_void),
            (wide.len() * std::mem::size_of::<u16>()) as u32,
        )
    };

    match status {
        s if s == ERROR_SUCCESS => Ok(()),
        s => Err(format!("could not add Azure to startup ({})", s.0)),
    }
}

/// `"C:\path\to\Azure.exe" --hidden`.
///
/// Quoted because the path contains spaces on any ordinary installation,
/// and an unquoted `Run` value with a space in it is a well-known way to
/// have Windows launch something else entirely.
fn command_line() -> Result<String, String> {
    let exe = std::env::current_exe()
        .map_err(|e| format!("could not find Azure's own path ({e})"))?;
    Ok(format!("\"{}\" {HIDDEN_FLAG}", exe.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_command_line_is_quoted_and_carries_the_hidden_flag() {
        let command = command_line().expect("this process has a path");
        assert!(command.starts_with('"'), "an unquoted path is a hijack: {command}");
        assert!(command.contains("\" --hidden"), "got {command}");
        assert!(command.to_lowercase().contains(".exe"), "got {command}");
    }

    /// Writes to the real `Run` key and puts it back. It targets Azure's
    /// own value name, so a machine that already has autostart on is left
    /// as it was found.
    #[test]
    fn autostart_can_be_turned_on_and_off_again() {
        let was = autostart_enabled();

        set_autostart(true).expect("registering autostart");
        assert!(autostart_enabled(), "the value should be readable back");

        set_autostart(false).expect("removing autostart");
        assert!(!autostart_enabled());

        if was {
            set_autostart(true).expect("putting the machine back as it was found");
        }
    }

    #[test]
    fn removing_an_autostart_that_is_not_there_is_not_an_error() {
        let was = autostart_enabled();
        set_autostart(false).expect("removing nothing");
        set_autostart(false).expect("removing nothing, twice");
        if was {
            set_autostart(true).expect("putting the machine back as it was found");
        }
    }
}
