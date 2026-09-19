//! What keeps running when the window is gone.
//!
//! The engine already outlives the window — that was M1's doing. This
//! holds the rest of it: the settings, and the live hotkey registrations
//! that turn a key press into an engine command.
//!
//! Rebinding replaces the registrations wholesale rather than diffing
//! them. Dropping the old set is what gives the chords back to Windows,
//! and a chord that is still held cannot be re-registered — so the drop
//! has to happen first, and doing it in one place is how that stays true.

use std::sync::Mutex;

use azure_display::{
    autostart_enabled, is_elevated, Action, BindingSet, EngineHandle, HotkeyEvent, Hotkeys, Phase,
    Registration, Settings,
};
use azure_settings::{Chord, Conflict, Store as SettingsStore};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, Runtime};
use ts_rs::TS;

/// Everything the settings screen needs in one answer.
///
/// `autostart` is read from the registry rather than from the settings
/// file: a player can remove Azure from Task Manager's Startup tab, and
/// the toggle should agree with the machine rather than with what Azure
/// last wrote.
#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ResidencyView {
    pub bindings: azure_display::BindingSet,
    /// What Windows allowed, per binding. Empty before the first bind.
    pub registrations: Vec<Registration>,
    /// Two of Azure's own actions on one chord. Azure's to fix, unlike a
    /// refusal.
    pub conflicts: Vec<Conflict>,
    pub autostart: bool,
    /// Whether Azure is already elevated, so the interface does not offer
    /// a restart that would change nothing.
    pub elevated: bool,
    pub notices: Vec<String>,
}

pub struct Residency {
    store: SettingsStore,
    settings: Settings,
    /// `None` until the first bind, and between dropping one set of
    /// registrations and making the next.
    hotkeys: Option<Hotkeys>,
    /// Notices from loading settings, handed to the interface with the
    /// next snapshot rather than printed where nobody looks.
    notices: Vec<String>,
}

impl Residency {
    pub fn load(dir: std::path::PathBuf) -> Residency {
        let store = SettingsStore::new(dir);
        let loaded = store.load();
        Residency {
            store,
            settings: loaded.settings,
            hotkeys: None,
            notices: loaded.notices,
        }
    }

    fn registrations(&self) -> Vec<Registration> {
        self.hotkeys
            .as_ref()
            .map(|h| h.registrations().to_vec())
            .unwrap_or_default()
    }

    pub fn view(&self) -> ResidencyView {
        ResidencyView {
            bindings: self.settings.bindings.clone(),
            registrations: self.registrations(),
            conflicts: self.settings.bindings.conflicts(),
            autostart: autostart_enabled(),
            elevated: is_elevated(),
            notices: self.notices.clone(),
        }
    }

    fn save(&mut self) {
        if let Err(e) = self.store.save(&self.settings) {
            let notice = format!("could not save settings: {e}");
            if !self.notices.contains(&notice) {
                self.notices.push(notice);
            }
        }
    }
}

/// Registers the current bindings, replacing whatever was registered
/// before, and answers with what Windows allowed.
pub fn rebind<R: Runtime>(app: &AppHandle<R>) -> Vec<Registration> {
    let engine = app.state::<EngineHandle>().inner().clone();
    let state = app.state::<Mutex<Residency>>();
    let Ok(mut residency) = state.lock() else {
        return Vec::new();
    };

    // Give the old chords back before asking for the new ones. Without
    // this, rebinding a key to itself would be refused by the previous
    // registration of the same key.
    residency.hotkeys = None;

    let bindings = residency.settings.bindings.clone();
    let for_sink = app.clone();
    let hotkeys = Hotkeys::spawn(&bindings, move |event| {
        perform(&for_sink, &engine, event);
    });

    let registrations = hotkeys.registrations().to_vec();
    residency.hotkeys = Some(hotkeys);
    registrations
}

/// Runs the action a key stands for, then tells the window what changed.
///
/// Called on the hotkey thread. Everything it touches is a channel send
/// or an event emit; nothing here waits on the window, which may not
/// exist.
fn perform<R: Runtime>(app: &AppHandle<R>, engine: &EngineHandle, event: HotkeyEvent) {
    let changed = match (event.action, event.phase) {
        (Action::ToggleEnabled, Phase::Pressed) => engine.toggle_enabled().is_ok(),
        (Action::CycleNext, Phase::Pressed) => engine.cycle(1).is_ok(),
        (Action::CyclePrev, Phase::Pressed) => engine.cycle(-1).is_ok(),
        (Action::RestoreDisplay, Phase::Pressed) => engine.restore().is_ok(),
        (Action::HoldBypass, Phase::Pressed) => engine.set_bypass(true).is_ok(),
        (Action::HoldBypass, Phase::Released) => engine.set_bypass(false).is_ok(),
        // Only hold-to-bypass has a second half; the rest are done on the
        // press and Windows never reports their release anyway.
        (_, Phase::Released) => return,
    };

    if changed {
        if let Ok(snapshot) = engine.snapshot() {
            let _ = app.emit("snapshot", snapshot);
        }
        crate::tray::refresh(app);
    }
}

/// Changes one binding and re-registers. Returns the new registrations so
/// the settings screen can show a refusal against the chord that earned
/// it, without a second round trip.
pub fn set_binding<R: Runtime>(
    app: &AppHandle<R>,
    action: Action,
    chord: Option<Chord>,
) -> Vec<Registration> {
    {
        let state = app.state::<Mutex<Residency>>();
        let Ok(mut residency) = state.lock() else {
            return Vec::new();
        };
        residency.settings.bindings.set(action, chord);
        residency.save();
    }
    rebind(app)
}

pub fn set_bindings<R: Runtime>(app: &AppHandle<R>, bindings: BindingSet) -> Vec<Registration> {
    {
        let state = app.state::<Mutex<Residency>>();
        let Ok(mut residency) = state.lock() else {
            return Vec::new();
        };
        residency.settings.bindings = bindings;
        residency.save();
    }
    rebind(app)
}

/// Records that the player has been told where the window went, so the
/// notification happens once ever rather than once a session.
pub fn mark_told_about_tray<R: Runtime>(app: &AppHandle<R>) -> bool {
    let state = app.state::<Mutex<Residency>>();
    let Ok(mut residency) = state.lock() else {
        return true;
    };
    if residency.settings.told_about_tray {
        return true;
    }
    residency.settings.told_about_tray = true;
    residency.save();
    false
}
