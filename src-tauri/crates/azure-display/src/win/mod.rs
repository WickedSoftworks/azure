//! The Windows half. Nothing outside this module calls a Win32 function.

mod displays;
mod lut;
mod magnifier;

pub use displays::{
    color_filters_active, enumerate_adapters, enumerate_displays, gamma_range_unlocked,
    unlock_gamma_range,
};
pub use lut::GdiRamp;
pub use magnifier::MagnifierMatrix;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{LutTarget, RampBackend, CLAMP_TOLERANCE};
    use azure_color::Ramp;

    #[test]
    fn enumerate_finds_at_least_the_primary_display() {
        let found = enumerate_displays();
        assert!(!found.is_empty(), "no displays enumerated");
        assert_eq!(found.iter().filter(|d| d.primary).count(), 1);
        assert!(found.iter().all(|d| !d.key.is_empty()));
        assert!(found.iter().all(|d| !d.name.is_empty()));
    }

    #[test]
    fn a_ramp_backend_round_trips_the_identity_without_stranding_the_display() {
        let mut b = match GdiRamp::new() {
            Ok(b) => b,
            Err(e) => {
                eprintln!("skipping: no writable display handle here ({e})");
                return;
            }
        };
        let landings = b
            .apply(&LutTarget::All, &Ramp::identity())
            .expect("identity ramp");
        assert!(!landings.is_empty());
        assert!(landings.iter().all(|l| l.deviation < CLAMP_TOLERANCE));
        b.clear().expect("restore");
    }

    #[test]
    fn the_registry_probes_answer_without_panicking() {
        // Both are plain reads of well-known values; the only contract is
        // that a missing value reads as "not set" rather than exploding.
        let _ = color_filters_active();
        let _ = gamma_range_unlocked();
    }
}
