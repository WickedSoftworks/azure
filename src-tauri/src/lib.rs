mod commands;

use azure_display::EngineHandle;

pub fn run() {
    // The engine starts before the window and outlives it. Closing the
    // window is not exiting: the effect has to hold with no window open.
    let engine = EngineHandle::spawn();
    let on_exit = engine.clone();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(engine)
        .invoke_handler(tauri::generate_handler![
            commands::get_snapshot,
            commands::apply_state,
            commands::set_enabled,
            commands::set_lut_target,
            commands::restore_display,
            commands::unlock_gamma_range,
        ])
        .build(tauri::generate_context!())
        .expect("error while building Azure")
        .run(move |_app, event| {
            if let tauri::RunEvent::ExitRequested { .. } = event {
                // Last chance to put the display back while our own code
                // can still run.
                let _ = on_exit.shutdown();
            }
        });
}
