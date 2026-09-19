mod commands;

use azure_display::{EngineHandle, Snapshot};
use serde::Serialize;
use tauri::{Emitter, Manager};

/// Sent to the window whenever the watcher, rather than the person,
/// changed which preset is on the screen. Without it the field would keep
/// drawing the preset you were last looking at while the display showed
/// another one.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Activated {
    foreground: azure_display::Foreground,
    snapshot: Snapshot,
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            // Presets live beside the app's own configuration, which is
            // per-user: two people on one machine do not share vibrance.
            let dir = app.path().app_config_dir()?;

            // The engine starts before the window and outlives it. Closing
            // the window is not exiting: the effect has to hold with no
            // window open.
            let engine = EngineHandle::spawn(dir);

            #[cfg(windows)]
            {
                let for_watcher = engine.clone();
                let handle = app.handle().clone();
                let watcher = azure_display::win::Watcher::spawn(move |found| {
                    if for_watcher
                        .foreground(found.path.clone(), found.exe.clone())
                        .is_err()
                    {
                        return;
                    }
                    if let Ok(snapshot) = for_watcher.snapshot() {
                        let _ = handle.emit("activated", Activated { foreground: found, snapshot });
                    }
                });
                // Held in state so it lives as long as the app does; its
                // Drop is what unhooks.
                app.manage(watcher);
            }

            app.manage(engine);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_snapshot,
            commands::apply_state,
            commands::set_enabled,
            commands::set_bypass,
            commands::set_lut_target,
            commands::restore_display,
            commands::unlock_gamma_range,
            commands::add_preset,
            commands::remove_preset,
            commands::select_preset,
            commands::rename_preset,
            commands::bind_preset,
        ])
        .build(tauri::generate_context!())
        .expect("error while building Azure")
        .run(|app, event| {
            if let tauri::RunEvent::ExitRequested { .. } = event {
                // Last chance to put the display back while our own code
                // can still run.
                if let Some(engine) = app.try_state::<EngineHandle>() {
                    let _ = engine.shutdown();
                }
            }
        });
}
