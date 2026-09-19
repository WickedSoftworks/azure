//! The tray icon: Azure's whole interface while the window is closed.
//!
//! Deliberately small. It carries the things a player might want without
//! opening anything — is Azure on, which preset, put the display back —
//! and nothing that needs explaining. Anything that needs a sentence
//! belongs in the window.

use azure_display::EngineHandle;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, Runtime};

pub const TRAY_ID: &str = "azure";

const OPEN: &str = "open";
const QUIT: &str = "quit";
const ENABLED: &str = "enabled";
const RESTORE: &str = "restore";
/// Preset items carry their id after this prefix, because a menu event
/// gives back only the id it was built with.
const PRESET: &str = "preset:";

pub fn build<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    let menu = menu(app)?;

    TrayIconBuilder::with_id(TRAY_ID)
        .icon(app.default_window_icon().cloned().expect("Azure ships an icon"))
        .tooltip(tooltip(app))
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(on_menu_event)
        .on_tray_icon_event(|tray, event| {
            // Left click opens the window. The menu is on right click,
            // which is where Windows users look for it.
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_window(tray.app_handle());
            }
        })
        .build(app)?;

    Ok(())
}

/// Rebuilds the menu and tooltip from the engine's current state.
///
/// Called after anything that could change what the menu says, including
/// a hotkey pressed while the window is shut. Menu work happens on the
/// main thread because that is where Windows expects it, and the hotkey
/// thread is not it.
pub fn refresh<R: Runtime>(app: &AppHandle<R>) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        let Some(tray) = handle.tray_by_id(TRAY_ID) else { return };
        if let Ok(menu) = menu(&handle) {
            let _ = tray.set_menu(Some(menu));
        }
        let _ = tray.set_tooltip(Some(tooltip(&handle)));
    });
}

fn menu<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Menu<R>> {
    let snapshot = app.try_state::<EngineHandle>().and_then(|e| e.snapshot().ok());

    let enabled = CheckMenuItem::with_id(
        app,
        ENABLED,
        "Azure enabled",
        true,
        snapshot.as_ref().is_none_or(|s| s.enabled),
        None::<&str>,
    )?;

    let presets = Submenu::with_id(app, "presets", "Preset", true)?;
    if let Some(snapshot) = snapshot.as_ref() {
        for preset in &snapshot.presets {
            let item = CheckMenuItem::with_id(
                app,
                format!("{PRESET}{}", preset.id),
                &preset.name,
                true,
                preset.id == snapshot.active_id,
                None::<&str>,
            )?;
            presets.append(&item)?;
        }
    }

    let restore = MenuItem::with_id(app, RESTORE, "Restore display", true, None::<&str>)?;
    let open = MenuItem::with_id(app, OPEN, "Open Azure", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, QUIT, "Quit Azure", true, None::<&str>)?;

    Menu::with_items(
        app,
        &[
            &enabled,
            &presets,
            &PredefinedMenuItem::separator(app)?,
            &restore,
            &open,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ],
    )
}

/// What hovering the icon answers: whether Azure is on, and what is on the
/// screen. The only question a hidden application gets asked.
fn tooltip<R: Runtime>(app: &AppHandle<R>) -> String {
    let Some(snapshot) = app.try_state::<EngineHandle>().and_then(|e| e.snapshot().ok()) else {
        return "Azure".to_string();
    };
    let preset = snapshot
        .presets
        .iter()
        .find(|p| p.id == snapshot.active_id)
        .map(|p| p.name.as_str())
        .unwrap_or("—");

    if snapshot.enabled {
        format!("Azure — {preset}")
    } else {
        format!("Azure — off ({preset})")
    }
}

fn on_menu_event<R: Runtime>(app: &AppHandle<R>, event: tauri::menu::MenuEvent) {
    let id = event.id().as_ref().to_string();
    let engine = app.try_state::<EngineHandle>().map(|e| e.inner().clone());

    match id.as_str() {
        OPEN => show_window(app),
        QUIT => {
            // The one real exit. `RunEvent::ExitRequested` puts the
            // display back on the way out.
            app.exit(0);
        }
        ENABLED => {
            if let Some(engine) = engine {
                let _ = engine.toggle_enabled();
                announce(app, &engine);
            }
        }
        RESTORE => {
            if let Some(engine) = engine {
                let _ = engine.restore();
                announce(app, &engine);
            }
        }
        other => {
            let Some(id) = other.strip_prefix(PRESET) else { return };
            if let Some(engine) = engine {
                let _ = engine.select_preset(id.to_string());
                announce(app, &engine);
            }
        }
    }
}

/// Tells the window what the tray just did, and puts the menu back in step
/// with it. A checkbox that does not follow what it changed is worse than
/// no checkbox.
fn announce<R: Runtime>(app: &AppHandle<R>, engine: &EngineHandle) {
    if let Ok(snapshot) = engine.snapshot() {
        let _ = app.emit("snapshot", snapshot);
    }
    refresh(app);
}

pub fn show_window<R: Runtime>(app: &AppHandle<R>) {
    let Some(window) = app.get_webview_window("main") else { return };
    let _ = window.unminimize();
    let _ = window.show();
    let _ = window.set_focus();
}
