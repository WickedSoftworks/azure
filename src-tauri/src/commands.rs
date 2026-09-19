//! The command surface.
//!
//! Each of these is a pass-through to the engine thread. There is no logic
//! here on purpose: everything that decides anything is in `azure-color`
//! or `azure-display`, where a test can reach it.

use azure_display::{
    ApplyReport, EngineHandle, GammaRangeOutcome, LutTarget, Snapshot,
};
use azure_color::ColorState;
use tauri::State;

type Answer<T> = Result<T, String>;

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

#[tauri::command]
pub fn set_lut_target(
    engine: State<'_, EngineHandle>,
    target: LutTarget,
) -> Answer<ApplyReport> {
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
