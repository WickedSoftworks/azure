//! The compositor colour matrix, through `MagSetFullscreenColorEffect`.
//!
//! A documented user-mode API that asks the desktop compositor to
//! post-process its own output. It reads nothing, touches no other
//! process, loads no code anywhere and installs no driver — which is the
//! entire reason Azure can sit next to kernel anti-cheat.
//!
//! Two properties shape everything around it: the effect covers the whole
//! desktop including Azure's own window, and Windows drops it the moment
//! this process exits, deliberately, so that a crash can never strand a
//! display.

use azure_color::Mat5;
use windows::Win32::UI::Magnification::{
    MagInitialize, MagSetFullscreenColorEffect, MagUninitialize, MAGCOLOREFFECT,
};

use crate::backend::{BackendError, MatrixBackend};
use crate::win::displays::color_filters_active;

pub struct MagnifierMatrix {
    initialised: bool,
}

impl MagnifierMatrix {
    pub fn new() -> Result<MagnifierMatrix, BackendError> {
        // SAFETY: documented entry point of the Magnification API; paired
        // with MagUninitialize in Drop.
        let ok = unsafe { MagInitialize() };
        if !ok.as_bool() {
            return Err(BackendError::Unavailable(
                "the Magnification API would not initialise, so the compositor matrix is unreachable"
                    .into(),
            ));
        }
        Ok(MagnifierMatrix { initialised: true })
    }

    fn set(&self, m: &Mat5) -> Result<(), BackendError> {
        if color_filters_active() {
            return Err(BackendError::Unavailable(
                "Windows Colour Filters owns the fullscreen colour effect; Azure yields to it"
                    .into(),
            ));
        }

        let effect = MAGCOLOREFFECT { transform: m.as_flat() };
        // SAFETY: the effect is a 5x5 f32 array in the documented layout and
        // lives for the call.
        let ok = unsafe { MagSetFullscreenColorEffect(&effect) };
        if !ok.as_bool() {
            return Err(BackendError::Rejected(
                "MagSetFullscreenColorEffect refused the matrix".into(),
            ));
        }
        Ok(())
    }
}

impl MatrixBackend for MagnifierMatrix {
    fn name(&self) -> &'static str {
        "matrix"
    }

    fn apply(&mut self, m: &Mat5) -> Result<(), BackendError> {
        self.set(m)
    }

    fn clear(&mut self) -> Result<(), BackendError> {
        // Writing the identity rather than trusting process exit to do it:
        // disabling Azure has to put the desktop back while it keeps running.
        self.set(&Mat5::IDENTITY)
    }
}

impl Drop for MagnifierMatrix {
    fn drop(&mut self) {
        if self.initialised {
            let _ = self.set(&Mat5::IDENTITY);
            // SAFETY: paired with the MagInitialize in `new`.
            unsafe {
                let _ = MagUninitialize();
            }
        }
    }
}
