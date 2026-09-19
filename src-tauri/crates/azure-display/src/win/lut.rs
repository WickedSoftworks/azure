//! The scanout LUT, through `SetDeviceGammaRamp`.
//!
//! This is the same documented GDI call every vendor control panel and
//! every calibration tool makes. It writes a 256-entry ramp into the
//! display pipeline after the compositor, which is why what it carries
//! keeps working in exclusive fullscreen and survives Azure exiting.
//!
//! It also lies: it returns TRUE while the GDI range clamp quietly applies
//! something narrower. So every write here is read back and measured.

use azure_color::Ramp;
use windows::core::PCWSTR;
use windows::Win32::Graphics::Gdi::{CreateDCW, DeleteDC, HDC};
use windows::Win32::UI::ColorSystem::{GetDeviceGammaRamp, SetDeviceGammaRamp};

use crate::backend::{
    BackendError, DisplayInfo, LutTarget, RampBackend, RampLanding, CLAMP_TOLERANCE,
};
use crate::win::displays::enumerate_adapters;

struct Target {
    info: DisplayInfo,
    dc: HDC,
}

pub struct GdiRamp {
    targets: Vec<Target>,
}

/// An `HDC` is a raw pointer, so Rust will not infer `Send`. Asserting it
/// is sound here for one reason, which the rest of the design exists to
/// keep true: the engine thread creates these handles, is the only thread
/// that ever touches them, and drops them itself.
unsafe impl Send for GdiRamp {}

impl GdiRamp {
    /// Opens one device context per attached display.
    pub fn new() -> Result<GdiRamp, BackendError> {
        let mut targets = Vec::new();

        for adapter in enumerate_adapters() {
            let wide: Vec<u16> = adapter
                .device_name
                .encode_utf16()
                .chain(std::iter::once(0))
                .collect();
            // SAFETY: opens a DC on a display this session already owns.
            // `wide` outlives the call; the handle is released in Drop.
            let dc = unsafe {
                CreateDCW(
                    PCWSTR(wide.as_ptr()),
                    PCWSTR(wide.as_ptr()),
                    PCWSTR::null(),
                    None,
                )
            };
            if dc.is_invalid() {
                continue;
            }
            targets.push(Target { info: adapter.info, dc });
        }

        if targets.is_empty() {
            return Err(BackendError::Unavailable(
                "no display accepted a device context, so the scanout LUT is unreachable".into(),
            ));
        }
        Ok(GdiRamp { targets })
    }

    fn write(&self, dc: HDC, ramp: &Ramp) -> Result<u16, BackendError> {
        let flat = ramp.as_gdi();
        // SAFETY: the buffer is exactly the 3x256 WORD array the call
        // documents, and it lives for the duration of the call.
        let wrote = unsafe { SetDeviceGammaRamp(dc, flat.as_ptr() as *const std::ffi::c_void) };
        if !wrote.as_bool() {
            return Err(BackendError::Rejected(
                "SetDeviceGammaRamp refused the ramp".into(),
            ));
        }

        // The call above returns TRUE even when the GDI range clamp applied
        // something else, so the readback is the only evidence.
        let mut back = [0u16; 768];
        // SAFETY: as above.
        let read = unsafe { GetDeviceGammaRamp(dc, back.as_mut_ptr() as *mut std::ffi::c_void) };
        if !read.as_bool() {
            return Err(BackendError::Rejected(
                "the ramp was written but GetDeviceGammaRamp would not confirm it".into(),
            ));
        }

        Ok(ramp.max_deviation(&Ramp::from_gdi(&back)))
    }
}

impl RampBackend for GdiRamp {
    fn name(&self) -> &'static str {
        "lut"
    }

    fn displays(&self) -> Vec<DisplayInfo> {
        self.targets.iter().map(|t| t.info.clone()).collect()
    }

    fn apply(&mut self, target: &LutTarget, r: &Ramp) -> Result<Vec<RampLanding>, BackendError> {
        let mut landings = Vec::new();
        let mut last_error = None;

        for t in &self.targets {
            let selected = match target {
                LutTarget::All => true,
                LutTarget::One(key) => &t.info.key == key,
            };
            if !selected {
                continue;
            }
            match self.write(t.dc, r) {
                Ok(deviation) => landings.push(RampLanding {
                    display: t.info.key.clone(),
                    deviation,
                    clamped: deviation > CLAMP_TOLERANCE,
                }),
                Err(e) => last_error = Some(e),
            }
        }

        match (landings.is_empty(), last_error) {
            (true, Some(e)) => Err(e),
            (true, None) => Err(BackendError::Unavailable(
                "the targeted display is no longer attached".into(),
            )),
            _ => Ok(landings),
        }
    }

    fn clear(&mut self) -> Result<(), BackendError> {
        let identity = Ramp::identity();
        let mut last_error = None;
        for t in &self.targets {
            if let Err(e) = self.write(t.dc, &identity) {
                last_error = Some(e);
            }
        }
        match last_error {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }
}

impl Drop for GdiRamp {
    /// Never strand the display: put every ramp back before the handles go.
    fn drop(&mut self) {
        let _ = self.clear();
        for t in &self.targets {
            // SAFETY: each DC was created by this struct and is released once.
            unsafe { let _ = DeleteDC(t.dc); }
        }
    }
}
