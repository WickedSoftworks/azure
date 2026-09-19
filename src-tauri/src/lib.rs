mod commands;
mod residency;
mod tray;

use std::sync::Mutex;

use azure_display::{EngineHandle, Snapshot, HIDDEN_FLAG};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, Runtime, WindowEvent};
use tauri_plugin_dialog::{DialogExt, MessageDialogKind};

use residency::Residency;

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
            let engine = EngineHandle::spawn(dir.clone());

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
                    // The tray menu names the active preset, and the
                    // watcher is the one thing that changes it while
                    // nobody is looking.
                    tray::refresh(&handle);
                });
                // Held in state so it lives as long as the app does; its
                // Drop is what unhooks.
                app.manage(watcher);
            }

            app.manage(engine);
            app.manage(Mutex::new(Residency::load(dir)));

            // The tray is built before the hotkeys so that a refusal has
            // somewhere to be reported, and before the window is shown so
            // that hiding it always leaves an icon behind.
            tray::build(app.handle())?;
            residency::rebind(app.handle());

            // `--hidden` is what the autostart entry passes. Azure exists
            // so the preset is already right when the game opens; a window
            // appearing at sign-in would be the opposite of the point.
            if std::env::args().any(|a| a == HIDDEN_FLAG) {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.hide();
                }
            }

            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                // Closing is not exiting. The engine, the watcher and the
                // hotkeys all have to keep running with no window open —
                // that is the whole point of the tray.
                api.prevent_close();
                hide_window(window.app_handle());
            }
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
            commands::get_residency,
            commands::set_binding,
            commands::reset_bindings,
            commands::set_autostart,
            commands::restart_elevated,
            commands::hide_to_tray,
            commands::show_window,
        ])
        .build(tauri::generate_context!())
        .expect("error while building Azure")
        .run(|app, event| {
            if let tauri::RunEvent::ExitRequested { .. } = event {
                // Last chance to put the display back while our own code
                // can still run. Quit is the only path here now, which is
                // what lets the dirty marker mean one thing: a run that
                // died rather than one that left deliberately.
                if let Some(engine) = app.try_state::<EngineHandle>() {
                    let _ = engine.shutdown();
                }
            }
        });
}

/// Hides the window and, the first time ever, says where it went.
///
/// A tool whose window vanishes and keeps changing the screen is alarming
/// if you did not expect it. Saying so once is enough; saying it every
/// time would be its own kind of noise, which is why the fact that it has
/// been said is stored rather than remembered for the session.
pub fn hide_window<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.hide();
    }

    if residency::mark_told_about_tray(app) {
        return;
    }

    app.dialog()
        .message(
            "Azure is still running in the notification area. Your presets keep switching and your hotkeys keep working with the window closed.\n\nTo stop it, right-click the tray icon and choose Quit Azure.",
        )
        .kind(MessageDialogKind::Info)
        .title("Azure is still running")
        .show(|_| {});
}
