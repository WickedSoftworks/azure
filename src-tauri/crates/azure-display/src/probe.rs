//! Measuring what this machine can actually do, and assembling the core
//! that matches it.
//!
//! Nothing here guesses. A backend that will not initialise is replaced by
//! one that reports why, never by one that pretends — a stage that cannot
//! land has to say so, because the whole product is the claim that Azure
//! tells you what happened.

use azure_color::Environment;

use crate::backend::{DisplayInfo, Unavailable};
#[cfg(not(windows))]
use crate::backend::GammaRangeOutcome;
use crate::engine::Core;

/// Builds the core this machine can support. The second return is the list
/// of reasons a stage is missing, for the event log.
pub fn real_core() -> (Core, Vec<String>) {
    #[cfg(windows)]
    {
        use crate::win::{GdiRamp, MagnifierMatrix};

        let mut notices = Vec::new();

        let matrix: Box<dyn crate::backend::MatrixBackend> = match MagnifierMatrix::new() {
            Ok(b) => Box::new(b),
            Err(e) => {
                let reason = e.to_string();
                notices.push(format!("matrix stage unavailable: {reason}"));
                Box::new(Unavailable::new(reason))
            }
        };

        let ramp: Box<dyn crate::backend::RampBackend> = match GdiRamp::new() {
            Ok(b) => Box::new(b),
            Err(e) => {
                let reason = e.to_string();
                notices.push(format!("lut stage unavailable: {reason}"));
                Box::new(Unavailable::new(reason))
            }
        };

        (Core::new(matrix, ramp), notices)
    }

    #[cfg(not(windows))]
    {
        let reason = "Azure's colour backends are Windows-only".to_string();
        (
            Core::new(
                Box::new(Unavailable::new(reason.clone())),
                Box::new(Unavailable::new(reason.clone())),
            ),
            vec![reason],
        )
    }
}

/// Reads the parts of the environment that are not about the backends
/// themselves.
///
/// `exclusive_fullscreen` is read here rather than guessed: it is probed
/// on every call, so a game taking the screen between one apply and the
/// next changes what the matrix channels report.
pub fn environment(
    displays: &[DisplayInfo],
    matrix_available: bool,
    lut_available: bool,
) -> Environment {
    Environment {
        matrix_available,
        lut_available,
        exclusive_fullscreen: exclusive_fullscreen(),
        hdr_active: displays.iter().any(|d| d.hdr),
        gamma_range_unlocked: gamma_range_unlocked(),
        color_filters_active: color_filters_active(),
    }
}

#[cfg(windows)]
pub use crate::win::{color_filters_active, exclusive_fullscreen, gamma_range_unlocked};

#[cfg(not(windows))]
pub fn color_filters_active() -> bool {
    false
}

#[cfg(not(windows))]
pub fn exclusive_fullscreen() -> bool {
    false
}

#[cfg(not(windows))]
pub fn gamma_range_unlocked() -> bool {
    false
}

#[cfg(windows)]
pub use crate::win::unlock_gamma_range;

#[cfg(not(windows))]
pub fn unlock_gamma_range() -> GammaRangeOutcome {
    GammaRangeOutcome::Failed {
        reason: "the gamma range is a Windows registry setting".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn display(hdr: bool) -> DisplayInfo {
        DisplayInfo { key: "K".into(), name: "N".into(), primary: true, hdr }
    }

    #[test]
    fn one_hdr_display_puts_the_whole_environment_in_hdr() {
        assert!(environment(&[display(false), display(true)], true, true).hdr_active);
        assert!(!environment(&[display(false)], true, true).hdr_active);
    }

    #[test]
    fn exclusive_fullscreen_is_probed_rather_than_inferred_from_the_displays() {
        // It used to be hardcoded false because nothing could tell. Now it
        // is read from Windows on every call, and the displays it is
        // handed have no say in it — a second monitor, or an HDR one, does
        // not make a game fullscreen.
        assert!(!environment(&[display(false)], true, true).exclusive_fullscreen);
        assert!(!environment(&[display(true), display(false)], true, true).exclusive_fullscreen);
    }

    #[test]
    fn an_unavailable_stage_reports_its_reason_rather_than_succeeding() {
        use crate::backend::{LutTarget, MatrixBackend, RampBackend};
        use azure_color::{Mat5, Ramp};

        let mut m = Unavailable::new("no compositor here".to_string());
        let err = MatrixBackend::apply(&mut m, &Mat5::IDENTITY).unwrap_err();
        assert!(err.to_string().contains("no compositor here"));

        let mut r = Unavailable::new("no scanout here".to_string());
        let err = RampBackend::apply(&mut r, &LutTarget::All, &Ramp::identity()).unwrap_err();
        assert!(err.to_string().contains("no scanout here"));
        assert!(RampBackend::displays(&r).is_empty());
    }
}
