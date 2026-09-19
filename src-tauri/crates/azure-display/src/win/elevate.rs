//! Restarting with administrator rights, only when asked.
//!
//! Azure runs unelevated. It writes no machine-wide state in normal
//! operation, and asking for administrator next to kernel anti-cheat is
//! the worst first impression available to a tool like this.
//!
//! There is one thing elevation buys that nothing else can: Windows UIPI
//! refuses to deliver an unelevated process's hotkeys while an elevated
//! window has focus, and some games and launchers run elevated. So this
//! exists, behind a button that appears only when Azure has actually
//! noticed the problem — never as a standing invitation.

use windows::core::{w, HSTRING, PCWSTR};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

/// Whether this process is already running elevated.
///
/// Used to keep the interface from offering a restart that would change
/// nothing.
pub fn is_elevated() -> bool {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::Security::{GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY};
    use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    let mut token = Default::default();
    // SAFETY: opens this process's own token for one query and closes it.
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) }.is_err() {
        return false;
    }

    let mut elevation = TOKEN_ELEVATION::default();
    let mut size = std::mem::size_of::<TOKEN_ELEVATION>() as u32;
    // SAFETY: `elevation` is sized as the API requires and outlives the call.
    let ok = unsafe {
        GetTokenInformation(
            token,
            TokenElevation,
            Some(&mut elevation as *mut _ as *mut std::ffi::c_void),
            size,
            &mut size,
        )
    }
    .is_ok();
    // SAFETY: the handle came from OpenProcessToken above and is closed once.
    unsafe {
        let _ = CloseHandle(token);
    }

    ok && elevation.TokenIsElevated != 0
}

/// Launches a second, elevated Azure. The caller exits once this returns
/// `Ok`; two Azures must not hold the display at once.
///
/// The UAC prompt is Windows', and a player who declines it gets `Err`
/// rather than a silent nothing — declining is an answer, and the
/// interface says so rather than leaving a button that appears not to
/// work.
pub fn restart_elevated() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| format!("could not find Azure's own path ({e})"))?;
    let file = HSTRING::from(exe.as_os_str());

    // SAFETY: asks the shell to run our own executable with the `runas`
    // verb. Nothing of ours enters the new process.
    let result = unsafe {
        ShellExecuteW(
            None,
            w!("runas"),
            PCWSTR(file.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };

    // ShellExecuteW returns a value above 32 on success. The one failure
    // worth naming is the player saying no.
    match result.0 as usize {
        code if code > 32 => Ok(()),
        // ERROR_CANCELLED
        1223 => Err("the administrator prompt was declined".to_string()),
        code => Err(format!("Windows would not restart Azure elevated ({code})")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_elevation_check_answers_without_panicking() {
        // Which answer depends on how the suite was launched, and both are
        // legitimate. That it reads this process's own token without
        // leaking a handle is the contract.
        let _ = is_elevated();
    }
}
