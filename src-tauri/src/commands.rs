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
