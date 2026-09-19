//! Reading what the machine is: which displays exist, which are in HDR,
//! and the two registry values that decide what Azure is allowed to do.
//!
//! Every call in this file is a documented, read-only Win32 enumeration of
//! the caller's own session. The one exception is `unlock_gamma_range`,
//! which writes a single well-known ICM registry value and only when the
//! user asks for it.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use windows::core::{w, PCWSTR};
use windows::Win32::Devices::Display::{
    DisplayConfigGetDeviceInfo, GetDisplayConfigBufferSizes, QueryDisplayConfig,
    DISPLAYCONFIG_DEVICE_INFO_GET_ADVANCED_COLOR_INFO, DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME,
    DISPLAYCONFIG_DEVICE_INFO_HEADER, DISPLAYCONFIG_GET_ADVANCED_COLOR_INFO,
    DISPLAYCONFIG_MODE_INFO, DISPLAYCONFIG_PATH_INFO, DISPLAYCONFIG_TARGET_DEVICE_NAME,
    QDC_ONLY_ACTIVE_PATHS,
};
use windows::Win32::Foundation::{ERROR_SUCCESS, WIN32_ERROR};
use windows::Win32::Graphics::Gdi::{EnumDisplayDevicesW, DISPLAY_DEVICEW};
use windows::Win32::System::Registry::{
    RegSetKeyValueW, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, REG_DWORD,
};

use crate::backend::DisplayInfo;

/// `DISPLAY_DEVICE_ATTACHED_TO_DESKTOP`. A display that is not attached has
/// no scanout LUT to write.
const ATTACHED_TO_DESKTOP: u32 = 0x0000_0001;
/// `DISPLAY_DEVICE_PRIMARY_DEVICE`.
const PRIMARY_DEVICE: u32 = 0x0000_0004;
/// `EDD_GET_DEVICE_INTERFACE_NAME`, which makes `DeviceID` the stable
/// interface path that `QueryDisplayConfig` also reports.
const EDD_GET_DEVICE_INTERFACE_NAME: u32 = 0x0000_0001;

/// A display as the OS describes it, plus the adapter name the GDI device
/// context is opened on.
#[derive(Clone, Debug)]
pub struct Adapter {
    pub info: DisplayInfo,
    /// `\\.\DISPLAY1` — what `CreateDCW` wants.
    pub device_name: String,
}

fn from_wide(buf: &[u16]) -> String {
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..end])
}

fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Enumerates attached displays.
///
/// `EnumDisplayDevicesW` twice — the adapter, then its monitor — for the
/// stable device key. The friendly name and the HDR state come from
/// `QueryDisplayConfig`, which is the only API that reports either.
pub fn enumerate_adapters() -> Vec<Adapter> {
    let extra = advanced_color_by_device_path();
    let mut out = Vec::new();

    for index in 0.. {
        let mut adapter = DISPLAY_DEVICEW {
            cb: std::mem::size_of::<DISPLAY_DEVICEW>() as u32,
            ..Default::default()
        };
        // SAFETY: read-only enumeration; the struct is sized as the API
        // requires and lives for the whole call.
        let ok = unsafe { EnumDisplayDevicesW(PCWSTR::null(), index, &mut adapter, 0) };
        if !ok.as_bool() {
            break;
        }
        if adapter.StateFlags.0 & ATTACHED_TO_DESKTOP == 0 {
            continue;
        }

        let device_name = from_wide(&adapter.DeviceName);
        let mut monitor = DISPLAY_DEVICEW {
            cb: std::mem::size_of::<DISPLAY_DEVICEW>() as u32,
            ..Default::default()
        };
        let wide = to_wide(&device_name);
        // SAFETY: as above; `wide` outlives the call.
        let has_monitor = unsafe {
            EnumDisplayDevicesW(
                PCWSTR(wide.as_ptr()),
                0,
                &mut monitor,
                EDD_GET_DEVICE_INTERFACE_NAME,
            )
        }
        .as_bool();

        let key = if has_monitor {
            from_wide(&monitor.DeviceID)
        } else {
            device_name.clone()
        };
        let reported = extra.get(&key);
        let name = reported
            .map(|r| r.friendly_name.clone())
            .filter(|n| !n.is_empty())
            .or_else(|| {
                let s = from_wide(&monitor.DeviceString);
                (!s.is_empty()).then_some(s)
            })
            .unwrap_or_else(|| device_name.clone());

        out.push(Adapter {
            info: DisplayInfo {
                key,
                name,
                primary: adapter.StateFlags.0 & PRIMARY_DEVICE != 0,
                hdr: reported.map(|r| r.hdr).unwrap_or(false),
            },
            device_name,
        });
    }

    out
}

pub fn enumerate_displays() -> Vec<DisplayInfo> {
    enumerate_adapters().into_iter().map(|a| a.info).collect()
}

struct Reported {
    friendly_name: String,
    hdr: bool,
}

/// Friendly name and HDR state per monitor interface path.
///
/// `QueryDisplayConfig` is the only vendor-neutral way to learn either. A
/// failure here is not fatal: the caller falls back to the GDI name and
/// reports no HDR, which is what the machine looks like to the backends.
fn advanced_color_by_device_path() -> HashMap<String, Reported> {
    let mut map = HashMap::new();

    let (mut n_paths, mut n_modes) = (0u32, 0u32);
    // SAFETY: both out-params are live for the call.
    let sized = unsafe {
        GetDisplayConfigBufferSizes(QDC_ONLY_ACTIVE_PATHS, &mut n_paths, &mut n_modes)
    };
    if sized != ERROR_SUCCESS || n_paths == 0 {
        return map;
    }

    let mut paths = vec![DISPLAYCONFIG_PATH_INFO::default(); n_paths as usize];
    let mut modes = vec![DISPLAYCONFIG_MODE_INFO::default(); n_modes as usize];
    // SAFETY: the two buffers are sized by the call above and live here.
    let queried = unsafe {
        QueryDisplayConfig(
            QDC_ONLY_ACTIVE_PATHS,
            &mut n_paths,
            paths.as_mut_ptr(),
            &mut n_modes,
            modes.as_mut_ptr(),
            None,
        )
    };
    if queried != ERROR_SUCCESS {
        return map;
    }

    for path in paths.iter().take(n_paths as usize) {
        let mut target = DISPLAYCONFIG_TARGET_DEVICE_NAME {
            header: DISPLAYCONFIG_DEVICE_INFO_HEADER {
                r#type: DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME,
                size: std::mem::size_of::<DISPLAYCONFIG_TARGET_DEVICE_NAME>() as u32,
                adapterId: path.targetInfo.adapterId,
                id: path.targetInfo.id,
            },
            ..Default::default()
        };
        // SAFETY: header describes the struct it is embedded in, as the API
        // requires. Read-only.
        let got = unsafe { DisplayConfigGetDeviceInfo(&mut target.header) };
        if WIN32_ERROR(got as u32) != ERROR_SUCCESS {
            continue;
        }

        let mut colour = DISPLAYCONFIG_GET_ADVANCED_COLOR_INFO {
            header: DISPLAYCONFIG_DEVICE_INFO_HEADER {
                r#type: DISPLAYCONFIG_DEVICE_INFO_GET_ADVANCED_COLOR_INFO,
                size: std::mem::size_of::<DISPLAYCONFIG_GET_ADVANCED_COLOR_INFO>() as u32,
                adapterId: path.targetInfo.adapterId,
                id: path.targetInfo.id,
            },
            ..Default::default()
        };
        // SAFETY: as above.
        let colour_ok = unsafe { DisplayConfigGetDeviceInfo(&mut colour.header) };
        // Bit 1 of the flags word is `advancedColorEnabled`: the display is
        // in HDR right now, not merely capable of it.
        let hdr = WIN32_ERROR(colour_ok as u32) == ERROR_SUCCESS
            && unsafe { colour.Anonymous.value } & 0x2 != 0;

        map.insert(
            from_wide(&target.monitorDevicePath),
            Reported {
                friendly_name: from_wide(&target.monitorFriendlyDeviceName),
                hdr,
            },
        );
    }

    map
}

/// Windows Colour Filters occupies the identical fullscreen colour-effect
/// slot. Detect it and yield; never silently disable an accessibility
/// setting.
pub fn color_filters_active() -> bool {
    read_dword(HKEY_CURRENT_USER, w!("Software\\Microsoft\\ColorFiltering"), w!("Active"))
        .is_some_and(|v| v == 1)
}

/// `GdiIcmGammaRange` == 256 lifts the GDI clamp on gamma ramps. Until it
/// is set, the ends of the gamma slider do not reach the panel.
pub fn gamma_range_unlocked() -> bool {
    read_dword(HKEY_LOCAL_MACHINE, ICM_KEY, w!("GdiIcmGammaRange")).is_some_and(|v| v == 256)
}

const ICM_KEY: PCWSTR = w!("SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion\\ICM");

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", tag = "kind")]
#[ts(export)]
pub enum GammaRangeOutcome {
    /// Written. Windows reads this at sign-in, so it is not live yet.
    Unlocked { requires_sign_out: bool },
    /// HKLM is not writable without elevation, and Azure does not elevate
    /// itself behind the user's back.
    NeedsElevation,
    Failed { reason: String },
}

/// Writes `GdiIcmGammaRange`. Only ever called because the user pressed the
/// button that says so.
pub fn unlock_gamma_range() -> GammaRangeOutcome {
    let value: u32 = 256;
    // SAFETY: a single DWORD write to a documented, well-known ICM value.
    let result = unsafe {
        RegSetKeyValueW(
            HKEY_LOCAL_MACHINE,
            ICM_KEY,
            w!("GdiIcmGammaRange"),
            REG_DWORD.0,
            Some(&value as *const u32 as *const std::ffi::c_void),
            std::mem::size_of::<u32>() as u32,
        )
    };

    match result {
        e if e == ERROR_SUCCESS => GammaRangeOutcome::Unlocked { requires_sign_out: true },
        // ERROR_ACCESS_DENIED
        WIN32_ERROR(5) => GammaRangeOutcome::NeedsElevation,
        WIN32_ERROR(code) => GammaRangeOutcome::Failed {
            reason: format!("RegSetKeyValueW failed with Win32 error {code}"),
        },
    }
}

fn read_dword(root: windows::Win32::System::Registry::HKEY, key: PCWSTR, value: PCWSTR) -> Option<u32> {
    use windows::Win32::System::Registry::{RegGetValueW, RRF_RT_REG_DWORD};

    let mut data: u32 = 0;
    let mut size = std::mem::size_of::<u32>() as u32;
    // SAFETY: read-only; `data` and `size` live for the call.
    let status = unsafe {
        RegGetValueW(
            root,
            key,
            value,
            RRF_RT_REG_DWORD,
            None,
            Some(&mut data as *mut u32 as *mut std::ffi::c_void),
            Some(&mut size),
        )
    };
    (status == ERROR_SUCCESS).then_some(data)
}
