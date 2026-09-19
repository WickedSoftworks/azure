//! Whether a game has taken the screen away from the compositor.
//!
//! This is the one environment fact M1 routed correctly and never set.
//! `Environment.exclusive_fullscreen` makes every matrix-carried channel
//! report `Inert`, because DWM is bypassed and the matrix genuinely does
//! nothing — so guessing it wrong is a lie in either direction.
//!
//! `SHQueryUserNotificationState` is what Windows offers for the question.
//! It is a read, it needs no handle on the game, and it draws exactly the
//! line that matters here: **borderless** fullscreen does not report D3D
//! fullscreen, and borderless is precisely the case where DWM still
//! composites and the matrix still works. A window-rectangle heuristic —
//! compare the foreground window to the monitor bounds — gets that
//! backwards and would mark the matrix inert for the many players who run
//! borderless.

use windows::Win32::UI::Shell::{SHQueryUserNotificationState, QUNS_RUNNING_D3D_FULL_SCREEN};

/// True when a Direct3D application holds the screen exclusively.
///
/// Answers false when Windows will not say. An unknown here becomes a
/// claim that the matrix works; the alternative is claiming it does not,
/// which would hide channels that are in fact applying.
pub fn exclusive_fullscreen() -> bool {
    // SAFETY: a documented read of a process-wide notification state. No
    // handle on any other process, nothing written.
    match unsafe { SHQueryUserNotificationState() } {
        Ok(state) => state == QUNS_RUNNING_D3D_FULL_SCREEN,
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_probe_answers_without_panicking() {
        // Nothing can be asserted about the value: whether this machine is
        // running an exclusive-fullscreen game while the suite runs is not
        // the test's business. That it answers at all is.
        let _ = exclusive_fullscreen();
    }

    #[test]
    fn a_test_run_is_not_an_exclusive_fullscreen_game() {
        // A build machine running `cargo test` is not in a D3D fullscreen
        // game, so a probe that says otherwise is reporting something else
        // and would make every matrix channel falsely inert.
        assert!(!exclusive_fullscreen());
    }
}
