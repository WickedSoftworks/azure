//! The thread that owns the display.
//!
//! Every handle the backends hold — the Magnification session, one device
//! context per monitor — lives on this one thread and is dropped by it.
//! That is not tidiness: the effect has to hold while no window exists, so
//! ownership cannot sit anywhere tied to a window or to a request.
//!
//! The loop restores the display on the way out of every exit path it has.

use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex};
use std::thread;

use azure_color::{ChannelReport, ColorState, Environment};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::backend::{DisplayInfo, LutTarget};
use crate::engine::{ApplyReport, Core};
use crate::probe::{environment, real_core};

/// Everything the surface needs to draw itself once.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Snapshot {
    pub channels: Vec<ChannelReport>,
    pub displays: Vec<DisplayInfo>,
    pub environment: Environment,
    pub enabled: bool,
    pub state: ColorState,
    pub target: LutTarget,
    /// Why a stage is missing, if one is. Straight to the event log.
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
    Apply(ColorState, LutTarget, Sender<ApplyReport>),
    SetEnabled(bool, Sender<ApplyReport>),
    SetTarget(LutTarget, Sender<ApplyReport>),
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

impl EngineHandle {
    /// Starts the engine on the real backends this machine supports.
    pub fn spawn() -> EngineHandle {
        EngineHandle::start(real_core)
    }

    /// Starts the engine on mock backends. Tests only — a mock reports
    /// success, so nothing user-facing may ever be built on one.
    pub fn spawn_mock() -> EngineHandle {
        EngineHandle::start(|| (Core::mock(), Vec::new()))
    }

    fn start(build: fn() -> (Core, Vec<String>)) -> EngineHandle {
        let (tx, rx) = channel::<Msg>();

        thread::Builder::new()
            .name("azure-colour-core".into())
            .spawn(move || {
                let (mut core, notices) = build();
                let mut state = ColorState::neutral();
                let mut enabled = true;
                let mut target = LutTarget::All;

                let env = |core: &Core| {
                    let displays = core.displays();
                    let lut = !displays.is_empty();
                    environment(&displays, true, lut)
                };

                while let Ok(msg) = rx.recv() {
                    match msg {
                        Msg::Apply(next, next_target, reply) => {
                            state = next;
                            target = next_target;
                            let report = if enabled {
                                core.apply(&state, &env(&core), &target)
                            } else {
                                // Switched off: report the routing so the
                                // field can still draw it, write nothing.
                                dry_run(&state, &env(&core))
                            };
                            let _ = reply.send(report);
                        }
                        Msg::SetEnabled(next, reply) => {
                            enabled = next;
                            let report = if enabled {
                                core.apply(&state, &env(&core), &target)
                            } else {
                                let _ = core.restore();
                                let mut report = dry_run(&state, &env(&core));
                                report.dirty = core.is_dirty();
                                report
                            };
                            let _ = reply.send(report);
                        }
                        Msg::SetTarget(next, reply) => {
                            // The LUT is per display, so retargeting means
                            // putting every ramp back before writing the
                            // new one: a display that is no longer targeted
                            // must not keep yesterday's curve.
                            let _ = core.restore();
                            target = next;
                            let report = if enabled {
                                core.apply(&state, &env(&core), &target)
                            } else {
                                dry_run(&state, &env(&core))
                            };
                            let _ = reply.send(report);
                        }
                        Msg::Snapshot(reply) => {
                            let environment = env(&core);
                            let _ = reply.send(Snapshot {
                                channels: azure_color::plan(&state, &environment).reports,
                                displays: core.displays(),
                                environment,
                                enabled,
                                state,
                                target: target.clone(),
                                notices: notices.clone(),
                            });
                        }
                        Msg::Restore(reply) => {
                            let _ = core.restore();
                            let _ = reply.send(());
                        }
                        Msg::Shutdown(reply) => {
                            let _ = core.restore();
                            let _ = reply.send(());
                            break;
                        }
                    }
                }

                // Every way out of that loop lands here, including the
                // sender being dropped. Never strand the display.
                let _ = core.restore();
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
        self.ask(|reply| Msg::Apply(state, target, reply))
    }

    pub fn set_enabled(&self, enabled: bool) -> Result<ApplyReport, EngineDown> {
        self.ask(|reply| Msg::SetEnabled(enabled, reply))
    }

    pub fn set_target(&self, target: LutTarget) -> Result<ApplyReport, EngineDown> {
        self.ask(|reply| Msg::SetTarget(target, reply))
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
/// Azure is switched off.
fn dry_run(state: &ColorState, env: &Environment) -> ApplyReport {
    ApplyReport {
        reports: azure_color::plan(state, env).reports,
        stages: Vec::new(),
        micros: 0,
        dirty: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use azure_color::ChannelId;

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
}
