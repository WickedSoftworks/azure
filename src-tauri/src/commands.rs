//! The command surface.
//!
//! Each of these is a pass-through to the engine thread. There is no logic
//! here on purpose: everything that decides anything is in `azure-color`,
//! `azure-presets` or `azure-display`, where a test can reach it.

use azure_color::ColorState;
use azure_display::{
    ApplyReport, EngineDown, EngineHandle, GammaRangeOutcome, LutTarget, PresetError, Snapshot,
};
use tauri::State;

type Answer<T> = Result<T, String>;

/// Preset operations answer with a message rather than a typed error: the
/// `PresetError` variants already read as sentences, and the interface
/// prints them verbatim.
fn flatten<T>(outer: Result<Result<T, PresetError>, EngineDown>) -> Answer<T> {
    match outer {
        Ok(Ok(value)) => Ok(value),
        Ok(Err(e)) => Err(e.to_string()),
        Err(e) => Err(e.to_string()),
    }
}

#[tauri::command]
pub fn get_snapshot(engine: State<'_, EngineHandle>) -> Answer<Snapshot> {
    engine.snapshot().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn apply_state(
    engine: State<'_, EngineHandle>,
    state: ColorState,
    target: LutTarget,
) -> Answer<ApplyReport> {
    engine.apply(state, target).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_enabled(engine: State<'_, EngineHandle>, enabled: bool) -> Answer<ApplyReport> {
    engine.set_enabled(enabled).map_err(|e| e.to_string())
}

/// Hold-to-bypass. The engine stands the display down without forgetting
/// the preset, and holds any activation the watcher makes meanwhile.
#[tauri::command]
pub fn set_bypass(engine: State<'_, EngineHandle>, bypassed: bool) -> Answer<ApplyReport> {
    engine.set_bypass(bypassed).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_lut_target(engine: State<'_, EngineHandle>, target: LutTarget) -> Answer<ApplyReport> {
    engine.set_target(target).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn restore_display(engine: State<'_, EngineHandle>) -> Answer<()> {
    engine.restore().map_err(|e| e.to_string())
}

/// Writes the ICM registry value that lifts the GDI gamma clamp. The
/// outcome is returned rather than thrown: needing elevation is a normal
/// answer, not an error, and the surface has to be able to say which.
#[tauri::command]
pub fn unlock_gamma_range() -> GammaRangeOutcome {
    azure_display::unlock_gamma_range()
}

// ── presets ─────────────────────────────────────────────────────────────

#[tauri::command]
pub fn add_preset(
    engine: State<'_, EngineHandle>,
    name: String,
    exe: Option<String>,
) -> Answer<String> {
    flatten(engine.add_preset(name, exe))
}

#[tauri::command]
pub fn remove_preset(engine: State<'_, EngineHandle>, id: String) -> Answer<ApplyReport> {
    flatten(engine.remove_preset(id))
}

#[tauri::command]
pub fn select_preset(engine: State<'_, EngineHandle>, id: String) -> Answer<ApplyReport> {
    flatten(engine.select_preset(id))
}

#[tauri::command]
pub fn rename_preset(
    engine: State<'_, EngineHandle>,
    id: String,
    name: String,
) -> Answer<()> {
    flatten(engine.rename_preset(id, name))
}

#[tauri::command]
pub fn bind_preset(
    engine: State<'_, EngineHandle>,
    id: String,
    exe: Option<String>,
) -> Answer<()> {
    flatten(engine.bind_preset(id, exe))
}

// ── residency ───────────────────────────────────────────────────────────

use crate::residency::{self, Residency, ResidencyView};
use azure_display::{Action, BindingSet, Chord, Registration};
use std::sync::Mutex;
use tauri::AppHandle;

#[tauri::command]
pub fn get_residency(state: State<'_, Mutex<Residency>>) -> Answer<ResidencyView> {
    state
        .lock()
        .map(|r| r.view())
        .map_err(|_| "the settings are held by a thread that panicked".to_string())
}

/// Binds one action, or clears it when `chord` is absent.
///
/// The chord arrives as the text the capture widget showed, and is parsed
/// here so that an unbindable combination is refused with the reason
/// rather than stored and quietly never registered.
#[tauri::command]
pub fn set_binding(
    app: AppHandle,
    action: Action,
    chord: Option<String>,
) -> Answer<Vec<Registration>> {
    let chord = match chord.as_deref() {
        Some(text) => Some(Chord::parse(text).map_err(|e| e.to_string())?),
        None => None,
    };
    Ok(residency::set_binding(&app, action, chord))
}

/// Puts every binding back to what Azure ships with.
#[tauri::command]
pub fn reset_bindings(app: AppHandle) -> Answer<Vec<Registration>> {
    Ok(residency::set_bindings(&app, BindingSet::default()))
}

#[tauri::command]
pub fn set_autostart(enabled: bool) -> Answer<bool> {
    azure_display::set_autostart(enabled)?;
    // Read it back rather than reporting what was asked for: the answer
    // the interface shows should be the state of the machine.
    Ok(azure_display::autostart_enabled())
}

/// Relaunches Azure with administrator rights and exits this one.
///
/// Two Azures must never hold the display at once, so the exit is not
/// optional — and it happens only once Windows has confirmed the new
/// process was started.
#[tauri::command]
pub fn restart_elevated(app: AppHandle) -> Answer<()> {
    azure_display::restart_elevated()?;
    app.exit(0);
    Ok(())
}

/// Hides the window without exiting. The same thing the close button
/// does, for a button inside the interface that wants to say so.
#[tauri::command]
pub fn hide_to_tray(app: AppHandle) -> Answer<()> {
    crate::hide_window(&app);
    Ok(())
}

#[tauri::command]
pub fn show_window(app: AppHandle) -> Answer<()> {
    crate::tray::show_window(&app);
    Ok(())
}

// ── the library scan ────────────────────────────────────────────────────

/// Walks every launcher's bookkeeping and reports what it found.
///
/// Runs off the main thread: it is disk-bound, and a library across
/// several drives takes long enough that holding the interface still for
/// it would be felt. It creates nothing — the candidates come back, the
/// person chooses, and `add_preset` does the rest.
#[tauri::command]
pub async fn scan_libraries() -> Answer<azure_scan::ScanReport> {
    tauri::async_runtime::spawn_blocking(azure_scan::scan)
        .await
        .map_err(|e| format!("the scan did not finish: {e}"))
}
