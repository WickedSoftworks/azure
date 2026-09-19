//! The thread that owns the display.
//!
//! Every handle the backends hold — the Magnification session, one device
//! context per monitor — lives on this one thread and is dropped by it.
//! That is not tidiness: the effect has to hold while no window exists, so
//! ownership cannot sit anywhere tied to a window or to a request.
//!
//! It owns the presets too, and for the same reason turned sideways: the
//! watcher can activate a preset at the moment someone is dragging a
//! slider in a different one. With one owner that is a queue. With two it
//! is a race for control of the screen.
//!
//! The loop restores the display on the way out of every exit path it has.

use std::path::PathBuf;
use std::sync::mpsc::{channel, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use azure_color::{ChannelReport, ColorState, Environment};
use azure_presets::{MatchKind, Preset, PresetError, PresetSet, Store, DESKTOP_ID};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::backend::{DisplayInfo, LutTarget};
use crate::engine::{ApplyReport, Core};
use crate::probe::{environment, real_core};

/// How long a change sits before it is written to disk. A slider drag
/// produces dozens of states a second and none of the intermediate ones
/// are worth a file write.
const SAVE_AFTER: Duration = Duration::from_millis(400);

/// Everything the surface needs to draw itself once.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Snapshot {
    pub channels: Vec<ChannelReport>,
    pub displays: Vec<DisplayInfo>,
    pub environment: Environment,
    pub enabled: bool,
    pub presets: Vec<Preset>,
    pub active_id: String,
    /// How the active preset was reached, when the watcher reached it.
    /// `None` means someone clicked it.
    pub matched_by: Option<MatchKind>,
    pub state: ColorState,
    pub target: LutTarget,
    /// Why a stage is missing, or what was recovered at startup. Straight
    /// to the event log.
    pub notices: Vec<String>,
}

/// The engine stopped answering: either `shutdown` was called, or the
/// worker thread panicked. A panic unwinds through the backends, so the
/// display is put back either way — see the release profile, which does
/// not abort for exactly that reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EngineDown;

impl std::fmt::Display for EngineDown {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "the colour engine is not running")
    }
}

impl std::error::Error for EngineDown {}

enum Msg {
    ApplyState(ColorState, LutTarget, Sender<ApplyReport>),
    SetEnabled(bool, Sender<ApplyReport>),
    SetBypass(bool, Sender<ApplyReport>),
    SetTarget(LutTarget, Sender<ApplyReport>),
    AddPreset(String, Option<String>, Sender<Result<String, PresetError>>),
    RemovePreset(String, Sender<Result<ApplyReport, PresetError>>),
    SelectPreset(String, Sender<Result<ApplyReport, PresetError>>),
    RenamePreset(String, String, Sender<Result<(), PresetError>>),
    BindPreset(String, Option<String>, Sender<Result<(), PresetError>>),
    Foreground(Option<String>, String, Sender<ApplyReport>),
    Snapshot(Sender<Snapshot>),
    Restore(Sender<()>),
    Shutdown(Sender<()>),
}

/// A cloneable handle to the engine thread. Cheap to clone; every clone
/// talks to the same display.
///
/// The mutex is around the channel, not the display: `mpsc::Sender` is
/// `Send` but not `Sync`, and Tauri's managed state needs both. It is held
/// only for the length of a `send`.
#[derive(Clone)]
pub struct EngineHandle {
    tx: Arc<Mutex<Sender<Msg>>>,
}

struct Worker {
    core: Core,
    store: Store,
    set: PresetSet,
    enabled: bool,
    /// Hold-to-bypass. The display is stood down but nothing is forgotten,
    /// and an activation arriving now waits for the key.
    bypassed: bool,
    target: LutTarget,
    matched_by: Option<MatchKind>,
    notices: Vec<String>,
    /// A preset the watcher chose while the bypass was held.
    deferred: Option<(String, MatchKind)>,
    unsaved: bool,
    marked_dirty: bool,
}

impl Worker {
    fn env(&self) -> Environment {
        let displays = self.core.displays();
        let lut = !displays.is_empty();
        environment(&displays, true, lut)
    }

    /// Writes the active preset to the display, or reports what would
    /// happen without writing when Azure is switched off or stood down.
    fn apply_current(&mut self) -> ApplyReport {
        let env = self.env();
        let state = self.set.active().state;
        let report = if self.enabled && !self.bypassed {
            self.core.apply(&state, &env, &self.target)
        } else {
            dry_run(&state, &env)
        };
        self.sync_dirty_marker();
        report
    }

    /// Keeps the on-disk marker in step with the display, so a run that
    /// dies leaves behind the fact that it did.
    fn sync_dirty_marker(&mut self) {
        let wanted = self.core.is_dirty();
        if wanted == self.marked_dirty {
            return;
        }
        let wrote = if wanted {
            self.store.mark_dirty()
        } else {
            self.store.clear_dirty()
        };
        if wrote.is_ok() {
            self.marked_dirty = wanted;
        }
    }

    fn flush(&mut self) {
        if !self.unsaved {
            return;
        }
        match self.store.save(&self.set) {
            Ok(()) => self.unsaved = false,
            Err(e) => {
                // Leave the flag set so the next tick tries again, and say
                // it once rather than filling the log with one line.
                let notice = format!("could not save presets: {e}");
                if !self.notices.contains(&notice) {
                    self.notices.push(notice);
                }
            }
        }
    }

    fn activate(&mut self, id: &str, how: Option<MatchKind>) -> ApplyReport {
        if self.set.select(id).is_ok() {
            self.matched_by = how;
            self.unsaved = true;
        }
        self.apply_current()
    }
}

impl EngineHandle {
    /// Starts the engine on the real backends, keeping presets in `dir`.
    pub fn spawn(dir: PathBuf) -> EngineHandle {
        EngineHandle::start(real_core, dir)
    }

    /// Starts the engine on mock backends with a scratch directory. Tests
    /// only — a mock reports success, so nothing user-facing may be built
    /// on one.
    pub fn spawn_mock() -> EngineHandle {
        EngineHandle::start(|| (Core::mock(), Vec::new()), scratch_dir("mock"))
    }

    fn start(build: fn() -> (Core, Vec<String>), dir: PathBuf) -> EngineHandle {
        let (tx, rx) = channel::<Msg>();

        thread::Builder::new()
            .name("azure-colour-core".into())
            .spawn(move || {
                let (core, mut notices) = build();
                let store = Store::new(dir);
                let loaded = store.load();
                notices.extend(loaded.notices);

                let mut w = Worker {
                    core,
                    set: loaded.set,
                    enabled: true,
                    bypassed: false,
                    target: LutTarget::All,
                    matched_by: None,
                    deferred: None,
                    unsaved: false,
                    marked_dirty: store.was_dirty(),
                    store,
                    notices,
                };

                // A marker left behind means the last run died without
                // putting the display back. Clear the ramps before doing
                // anything else, and say so.
                if w.marked_dirty {
                    let _ = w.core.restore();
                    let _ = w.store.clear_dirty();
                    w.marked_dirty = false;
                    w.notices.push(
                        "the previous session ended without restoring the display; its colour was cleared at startup"
                            .into(),
                    );
                }

                loop {
                    let msg = match rx.recv_timeout(SAVE_AFTER) {
                        Ok(msg) => msg,
                        Err(RecvTimeoutError::Timeout) => {
                            w.flush();
                            continue;
                        }
                        Err(RecvTimeoutError::Disconnected) => break,
                    };

                    match msg {
                        Msg::ApplyState(state, target, reply) => {
                            let id = w.set.active_id.clone();
                            let _ = w.set.set_state(&id, state);
                            w.target = target;
                            w.unsaved = true;
                            let report = w.apply_current();
                            let _ = reply.send(report);
                        }
                        Msg::SetEnabled(next, reply) => {
                            w.enabled = next;
                            if !next {
                                let _ = w.core.restore();
                            }
                            let report = w.apply_current();
                            let _ = reply.send(report);
                        }
                        Msg::SetBypass(next, reply) => {
                            w.bypassed = next;
                            if next {
                                let _ = w.core.restore();
                                let report = w.apply_current();
                                let _ = reply.send(report);
                            } else if let Some((id, how)) = w.deferred.take() {
                                // The watcher chose while the key was down.
                                let report = w.activate(&id, Some(how));
                                let _ = reply.send(report);
                            } else {
                                let report = w.apply_current();
                                let _ = reply.send(report);
                            }
                        }
                        Msg::SetTarget(next, reply) => {
                            // The LUT is per display, so retargeting means
                            // putting every ramp back before writing the
                            // new one: a display that is no longer targeted
                            // must not keep yesterday's curve.
                            let _ = w.core.restore();
                            w.target = next;
                            let report = w.apply_current();
                            let _ = reply.send(report);
                        }
                        Msg::AddPreset(name, exe, reply) => {
                            let id = w.set.add(name, exe);
                            w.unsaved = true;
                            w.flush();
                            let _ = reply.send(Ok(id));
                        }
                        Msg::RemovePreset(id, reply) => {
                            let answer = w.set.remove(&id).map(|()| {
                                w.unsaved = true;
                                w.apply_current()
                            });
                            w.flush();
                            let _ = reply.send(answer);
                        }
                        Msg::SelectPreset(id, reply) => {
                            let answer = match w.set.get(&id) {
                                Some(_) => Ok(w.activate(&id, None)),
                                None => Err(PresetError::NoSuchPreset),
                            };
                            w.flush();
                            let _ = reply.send(answer);
                        }
                        Msg::RenamePreset(id, name, reply) => {
                            let answer = w.set.rename(&id, name);
                            if answer.is_ok() {
                                w.unsaved = true;
                                w.flush();
                            }
                            let _ = reply.send(answer);
                        }
                        Msg::BindPreset(id, exe, reply) => {
                            let answer = w.set.bind(&id, exe);
                            if answer.is_ok() {
                                w.unsaved = true;
                                w.flush();
                            }
                            let _ = reply.send(answer);
                        }
                        Msg::Foreground(path, exe, reply) => {
                            let hit = w
                                .set
                                .match_foreground(path.as_deref(), &exe)
                                .map(|(p, kind)| (p.id.clone(), kind));

                            let report = match hit {
                                Some((id, kind)) if w.bypassed => {
                                    // Hold-to-bypass exists to give a stable
                                    // comparison. Changing the screen under
                                    // it would defeat the one thing it does.
                                    w.deferred = Some((id, kind));
                                    w.apply_current()
                                }
                                Some((id, kind)) => w.activate(&id, Some(kind)),
                                None if w.set.active_id == DESKTOP_ID => w.apply_current(),
                                None => {
                                    let desktop = DESKTOP_ID.to_string();
                                    w.activate(&desktop, None)
                                }
                            };
                            let _ = reply.send(report);
                        }
                        Msg::Snapshot(reply) => {
                            let environment = w.env();
                            let state = w.set.active().state;
                            let _ = reply.send(Snapshot {
                                channels: azure_color::plan(&state, &environment).reports,
                                displays: w.core.displays(),
                                environment,
                                enabled: w.enabled,
                                presets: w.set.presets.clone(),
                                active_id: w.set.active_id.clone(),
                                matched_by: w.matched_by,
                                state,
                                target: w.target.clone(),
                                notices: w.notices.clone(),
                            });
                        }
                        Msg::Restore(reply) => {
                            let _ = w.core.restore();
                            w.sync_dirty_marker();
                            let _ = reply.send(());
                        }
                        Msg::Shutdown(reply) => {
                            let _ = w.core.restore();
                            w.sync_dirty_marker();
                            w.flush();
                            let _ = reply.send(());
                            break;
                        }
                    }
                }

                // Every way out of that loop lands here, including the
                // sender being dropped. Never strand the display.
                let _ = w.core.restore();
                w.sync_dirty_marker();
                w.flush();
            })
            .expect("the colour engine thread could not start");

        EngineHandle { tx: Arc::new(Mutex::new(tx)) }
    }

    fn ask<T>(&self, make: impl FnOnce(Sender<T>) -> Msg) -> Result<T, EngineDown> {
        let (tx, rx) = channel::<T>();
        self.tx
            .lock()
            .map_err(|_| EngineDown)?
            .send(make(tx))
            .map_err(|_| EngineDown)?;
        rx.recv().map_err(|_| EngineDown)
    }

    pub fn apply(&self, state: ColorState, target: LutTarget) -> Result<ApplyReport, EngineDown> {
        self.ask(|reply| Msg::ApplyState(state, target, reply))
    }

    pub fn set_enabled(&self, enabled: bool) -> Result<ApplyReport, EngineDown> {
        self.ask(|reply| Msg::SetEnabled(enabled, reply))
    }

    /// Hold-to-bypass: stand the display down without forgetting anything.
    pub fn set_bypass(&self, bypassed: bool) -> Result<ApplyReport, EngineDown> {
        self.ask(|reply| Msg::SetBypass(bypassed, reply))
    }

    pub fn set_target(&self, target: LutTarget) -> Result<ApplyReport, EngineDown> {
        self.ask(|reply| Msg::SetTarget(target, reply))
    }

    pub fn add_preset(
        &self,
        name: String,
        exe: Option<String>,
    ) -> Result<Result<String, PresetError>, EngineDown> {
        self.ask(|reply| Msg::AddPreset(name, exe, reply))
    }

    pub fn remove_preset(&self, id: String) -> Result<Result<ApplyReport, PresetError>, EngineDown> {
        self.ask(|reply| Msg::RemovePreset(id, reply))
    }

    pub fn select_preset(&self, id: String) -> Result<Result<ApplyReport, PresetError>, EngineDown> {
        self.ask(|reply| Msg::SelectPreset(id, reply))
    }

    pub fn rename_preset(
        &self,
        id: String,
        name: String,
    ) -> Result<Result<(), PresetError>, EngineDown> {
        self.ask(|reply| Msg::RenamePreset(id, name, reply))
    }

    pub fn bind_preset(
        &self,
        id: String,
        exe: Option<String>,
    ) -> Result<Result<(), PresetError>, EngineDown> {
        self.ask(|reply| Msg::BindPreset(id, exe, reply))
    }

    /// Told by the watcher, not asked for by the interface.
    pub fn foreground(
        &self,
        path: Option<String>,
        exe: String,
    ) -> Result<ApplyReport, EngineDown> {
        self.ask(|reply| Msg::Foreground(path, exe, reply))
    }

    pub fn snapshot(&self) -> Result<Snapshot, EngineDown> {
        self.ask(Msg::Snapshot)
    }

    pub fn restore(&self) -> Result<(), EngineDown> {
        self.ask(Msg::Restore)
    }

    pub fn shutdown(&self) -> Result<(), EngineDown> {
        self.ask(Msg::Shutdown)
    }
}

/// The routing report with nothing written. What the surface draws while
/// Azure is switched off or stood down.
fn dry_run(state: &ColorState, env: &Environment) -> ApplyReport {
    ApplyReport {
        reports: azure_color::plan(state, env).reports,
        stages: Vec::new(),
        micros: 0,
        dirty: false,
    }
}

/// A directory of our own under the system temp dir, for tests and for the
/// mock engine.
fn scratch_dir(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("azure-{tag}-{nanos}-{:?}", thread::current().id()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use azure_color::ChannelId;

    fn bound(engine: &EngineHandle, name: &str, exe: &str) -> String {
        engine
            .add_preset(name.into(), Some(exe.into()))
            .unwrap()
            .unwrap()
    }

    #[test]
    fn the_handle_applies_and_reports_from_another_thread() {
        let engine = EngineHandle::spawn_mock();
        let mut s = ColorState::neutral();
        s.set(ChannelId::Vibrance, 156);
        let report = engine.apply(s, LutTarget::All).unwrap();
        assert_eq!(report.stages.len(), 1);
        assert_eq!(engine.snapshot().unwrap().state.vibrance, 156);
    }

    #[test]
    fn disabling_restores_the_display_but_keeps_the_state() {
        let engine = EngineHandle::spawn_mock();
        let mut s = ColorState::neutral();
        s.set(ChannelId::Gamma, 112);
        engine.apply(s, LutTarget::All).unwrap();
        let report = engine.set_enabled(false).unwrap();
        assert!(!report.dirty, "disabling must put the display back");
        let snap = engine.snapshot().unwrap();
        assert_eq!(snap.state.gamma, 112, "the preset is not lost by disabling");
        assert!(!snap.enabled);
    }

    #[test]
    fn re_enabling_puts_the_state_back_on_the_screen() {
        let engine = EngineHandle::spawn_mock();
        let mut s = ColorState::neutral();
        s.set(ChannelId::Gamma, 112);
        engine.apply(s, LutTarget::All).unwrap();
        engine.set_enabled(false).unwrap();
        let report = engine.set_enabled(true).unwrap();
        assert!(report.dirty);
        assert_eq!(report.stages.len(), 1);
    }

    #[test]
    fn a_disabled_engine_does_not_write_when_a_channel_moves() {
        let engine = EngineHandle::spawn_mock();
        engine.set_enabled(false).unwrap();
        let mut s = ColorState::neutral();
        s.set(ChannelId::Gamma, 112);
        let report = engine.apply(s, LutTarget::All).unwrap();
        assert!(report.stages.is_empty(), "a disabled engine must not write");
        // but it still reports what the machine could do, so the field can
        // draw the routing while Azure is switched off.
        assert_eq!(report.reports.len(), 8);
        assert_eq!(engine.snapshot().unwrap().state.gamma, 112);
    }

    #[test]
    fn the_snapshot_carries_the_channel_table_and_the_displays() {
        let engine = EngineHandle::spawn_mock();
        let snap = engine.snapshot().unwrap();
        assert_eq!(snap.channels.len(), 8);
        assert_eq!(snap.displays.len(), 1);
        assert!(snap.enabled);
    }

    #[test]
    fn shutdown_restores_the_display_and_closes_the_handle() {
        let engine = EngineHandle::spawn_mock();
        let mut s = ColorState::neutral();
        s.set(ChannelId::Gamma, 112);
        engine.apply(s, LutTarget::All).unwrap();
        engine.shutdown().unwrap();
        assert!(engine.snapshot().is_err());
    }

    // ── presets ─────────────────────────────────────────────────────────

    #[test]
    fn a_new_engine_starts_on_the_desktop_preset() {
        let engine = EngineHandle::spawn_mock();
        let snap = engine.snapshot().unwrap();
        assert_eq!(snap.presets.len(), 1);
        assert_eq!(snap.active_id, DESKTOP_ID);
        assert_eq!(snap.matched_by, None);
    }

    #[test]
    fn editing_a_channel_edits_the_active_preset_only() {
        let engine = EngineHandle::spawn_mock();
        let id = bound(&engine, "CS2", "D:\\games\\cs2.exe");
        engine.select_preset(id.clone()).unwrap().unwrap();

        let mut s = ColorState::neutral();
        s.set(ChannelId::Vibrance, 178);
        engine.apply(s, LutTarget::All).unwrap();

        let snap = engine.snapshot().unwrap();
        let cs2 = snap.presets.iter().find(|p| p.id == id).unwrap();
        let desktop = snap.presets.iter().find(|p| p.id == DESKTOP_ID).unwrap();
        assert_eq!(cs2.state.vibrance, 178);
        assert_eq!(desktop.state.vibrance, 100, "the desktop preset was not being edited");
    }

    #[test]
    fn selecting_a_preset_puts_its_state_on_the_display() {
        let engine = EngineHandle::spawn_mock();
        let id = bound(&engine, "CS2", "D:\\games\\cs2.exe");
        engine.select_preset(id.clone()).unwrap().unwrap();
        let mut s = ColorState::neutral();
        s.set(ChannelId::Vibrance, 178);
        engine.apply(s, LutTarget::All).unwrap();

        engine.select_preset(DESKTOP_ID.into()).unwrap().unwrap();
        assert_eq!(engine.snapshot().unwrap().state.vibrance, 100);

        let report = engine.select_preset(id).unwrap().unwrap();
        assert_eq!(report.stages.len(), 1, "switching back has to write it again");
        assert_eq!(engine.snapshot().unwrap().state.vibrance, 178);
    }

    #[test]
    fn a_matching_foreground_window_activates_its_preset() {
        let engine = EngineHandle::spawn_mock();
        let id = bound(&engine, "CS2", "D:\\games\\cs2.exe");

        engine
            .foreground(Some("D:\\games\\cs2.exe".into()), "cs2.exe".into())
            .unwrap();

        let snap = engine.snapshot().unwrap();
        assert_eq!(snap.active_id, id);
        assert_eq!(snap.matched_by, Some(MatchKind::FullPath));
    }

    #[test]
    fn an_elevated_game_that_hides_its_path_still_matches_by_name() {
        let engine = EngineHandle::spawn_mock();
        let id = bound(&engine, "VALORANT", "C:\\Riot\\VALORANT-Win64-Shipping.exe");

        engine
            .foreground(None, "valorant-win64-shipping.exe".into())
            .unwrap();

        let snap = engine.snapshot().unwrap();
        assert_eq!(snap.active_id, id);
        assert_eq!(snap.matched_by, Some(MatchKind::ExeName));
    }

    #[test]
    fn an_unmatched_foreground_window_falls_back_to_the_desktop() {
        let engine = EngineHandle::spawn_mock();
        let id = bound(&engine, "CS2", "D:\\games\\cs2.exe");
        engine
            .foreground(Some("D:\\games\\cs2.exe".into()), "cs2.exe".into())
            .unwrap();
        assert_eq!(engine.snapshot().unwrap().active_id, id);

        engine
            .foreground(Some("C:\\chrome.exe".into()), "chrome.exe".into())
            .unwrap();
        let snap = engine.snapshot().unwrap();
        assert_eq!(snap.active_id, DESKTOP_ID);
        assert_eq!(snap.matched_by, None, "falling back is not a match");
    }

    #[test]
    fn an_activation_during_a_bypass_waits_for_the_key() {
        let engine = EngineHandle::spawn_mock();
        let id = bound(&engine, "CS2", "D:\\games\\cs2.exe");

        engine.set_bypass(true).unwrap();
        let report = engine
            .foreground(Some("D:\\games\\cs2.exe".into()), "cs2.exe".into())
            .unwrap();
        assert!(report.stages.is_empty(), "a bypass that writes is not a bypass");
        assert_eq!(
            engine.snapshot().unwrap().active_id,
            DESKTOP_ID,
            "the deferred preset must not be active yet"
        );

        engine.set_bypass(false).unwrap();
        let snap = engine.snapshot().unwrap();
        assert_eq!(snap.active_id, id);
        assert_eq!(snap.matched_by, Some(MatchKind::FullPath));
    }

    #[test]
    fn a_bypass_stands_the_display_down_and_releasing_rewrites_it() {
        let engine = EngineHandle::spawn_mock();
        let mut s = ColorState::neutral();
        s.set(ChannelId::Gamma, 112);
        engine.apply(s, LutTarget::All).unwrap();

        let down = engine.set_bypass(true).unwrap();
        assert!(down.stages.is_empty());
        let up = engine.set_bypass(false).unwrap();
        assert_eq!(up.stages.len(), 1);
    }

    #[test]
    fn the_desktop_preset_refuses_to_be_removed() {
        let engine = EngineHandle::spawn_mock();
        let answer = engine.remove_preset(DESKTOP_ID.into()).unwrap();
        assert_eq!(answer.unwrap_err(), PresetError::DesktopIsPermanent);
    }

    #[test]
    fn removing_the_active_preset_returns_to_the_desktop_and_rewrites() {
        let engine = EngineHandle::spawn_mock();
        let id = bound(&engine, "CS2", "D:\\games\\cs2.exe");
        engine.select_preset(id.clone()).unwrap().unwrap();
        let mut s = ColorState::neutral();
        s.set(ChannelId::Vibrance, 178);
        engine.apply(s, LutTarget::All).unwrap();

        engine.remove_preset(id).unwrap().unwrap();
        let snap = engine.snapshot().unwrap();
        assert_eq!(snap.active_id, DESKTOP_ID);
        assert_eq!(snap.state.vibrance, 100);
    }

    #[test]
    fn renaming_and_binding_report_a_missing_preset() {
        let engine = EngineHandle::spawn_mock();
        assert_eq!(
            engine.rename_preset("nope".into(), "X".into()).unwrap(),
            Err(PresetError::NoSuchPreset)
        );
        assert_eq!(
            engine.bind_preset("nope".into(), None).unwrap(),
            Err(PresetError::NoSuchPreset)
        );
    }

    #[test]
    fn presets_come_back_after_a_restart() {
        let dir = scratch_dir("restart");

        let first = EngineHandle::start(|| (Core::mock(), Vec::new()), dir.clone());
        let id = bound(&first, "CS2", "D:\\games\\cs2.exe");
        first.select_preset(id.clone()).unwrap().unwrap();
        let mut s = ColorState::neutral();
        s.set(ChannelId::Vibrance, 178);
        first.apply(s, LutTarget::All).unwrap();
        first.shutdown().unwrap();

        let second = EngineHandle::start(|| (Core::mock(), Vec::new()), dir);
        let snap = second.snapshot().unwrap();
        let cs2 = snap.presets.iter().find(|p| p.id == id).expect("preset survived");
        assert_eq!(cs2.state.vibrance, 178);
        assert_eq!(cs2.exe.as_deref(), Some("D:\\games\\cs2.exe"));
        assert_eq!(snap.active_id, id, "and it is still the one in use");
        second.shutdown().unwrap();
    }

    #[test]
    fn a_session_that_died_dirty_is_cleaned_up_at_the_next_start() {
        let dir = scratch_dir("unclean");
        std::fs::create_dir_all(&dir).unwrap();
        // What a killed process leaves behind.
        Store::new(dir.clone()).mark_dirty().unwrap();

        let engine = EngineHandle::start(|| (Core::mock(), Vec::new()), dir.clone());
        let snap = engine.snapshot().unwrap();
        assert!(
            snap.notices.iter().any(|n| n.contains("without restoring")),
            "the recovery has to be reported, not silent: {:?}",
            snap.notices
        );
        assert!(!Store::new(dir).was_dirty(), "and the marker cleared");
        engine.shutdown().unwrap();
    }

    /// Writes the fixture the interface falls back to in a browser
    /// session.
    ///
    /// `bun run dev` has no Rust core, so the surface needs something to
    /// draw while the design is being worked on. Generating it here rather
    /// than hand-writing a TypeScript copy keeps one implementation of the
    /// routing: the stage and fidelity in that file are the real router's
    /// output for an ordinary SDR desktop. The displays and the second
    /// preset are placeholders, and the interface says so in a banner
    /// rather than in small print.
    #[test]
    fn export_preview_snapshot() {
        use azure_presets::ActivationMode;

        let environment = Environment::ideal();
        let mut set = PresetSet::fresh();
        let id = set.add("EXAMPLE GAME".into(), Some(r"D:\games\example.exe".to_string()));
        set.set_mode(&id, ActivationMode::Focused).unwrap();
        let state = set.active().state;

        let snapshot = Snapshot {
            channels: azure_color::plan(&state, &environment).reports,
            displays: vec![
                DisplayInfo {
                    key: "PREVIEW-1".into(),
                    name: "DISPLAY 1".into(),
                    primary: true,
                    hdr: false,
                },
                DisplayInfo {
                    key: "PREVIEW-2".into(),
                    name: "DISPLAY 2".into(),
                    primary: false,
                    hdr: false,
                },
            ],
            environment,
            enabled: true,
            presets: set.presets.clone(),
            active_id: set.active_id.clone(),
            matched_by: None,
            state,
            target: LutTarget::All,
            notices: vec![
                "no colour core in this session: nothing here is reaching a display".into(),
            ],
        };

        let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../src/lib/preview-snapshot.json");
        let json = serde_json::to_string_pretty(&snapshot).expect("snapshot serialises");
        std::fs::write(&out, json + "
").expect("preview snapshot written");
    }

    /// The only test that drives the real backends end to end.
    ///
    /// One percent of gamma: enough that a ramp is built and written and
    /// read back, not enough for anyone to see it happen. On a machine
    /// with no scanout LUT — anything that is not Windows — the report has
    /// to say the channel went nowhere rather than claim success, and
    /// that is what is asserted there.
    #[test]
    fn the_real_engine_applies_and_restores_on_this_machine() {
        use azure_color::{Fidelity, Stage};

        let engine = EngineHandle::spawn(scratch_dir("real"));
        let snap = engine.snapshot().expect("snapshot");
        assert_eq!(snap.channels.len(), 8);

        let mut state = snap.state;
        state.set(ChannelId::Gamma, 101);
        let report = engine.apply(state, LutTarget::All).expect("apply");

        let gamma = report
            .reports
            .iter()
            .find(|r| r.id == ChannelId::Gamma)
            .expect("gamma is reported");

        match gamma.stage {
            Some(Stage::Lut) => {
                assert_eq!(report.stages.len(), 1, "only the LUT should have been written");
                assert!(report.stages[0].ok, "{:?}", report.stages[0].detail);
                assert!(report.micros > 0, "an apply that took no time did not happen");
            }
            _ => assert_eq!(gamma.fidelity, Fidelity::Unrealised),
        }

        engine.shutdown().expect("shutdown restores the display");
    }
}
