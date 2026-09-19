//! Global hotkeys: what was asked for, and what Windows allowed.
//!
//! Registration is the only place in Azure where a capability can be
//! refused by another program rather than by hardware. The refusal is kept
//! per binding, because "hotkeys failed" would be the same kind of lie as
//! a boolean fidelity: four of five may have registered perfectly.

use azure_settings::{Action, BindingSet, Chord};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Whether a bound key went down or came back up.
///
/// Only `HoldBypass` ever reports `Released`. Windows gives a hotkey no
/// release event, so that one is watched for rather than delivered.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum Phase {
    Pressed,
    Released,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HotkeyEvent {
    pub action: Action,
    pub phase: Phase,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", rename_all_fields = "camelCase", tag = "kind")]
#[ts(export)]
pub enum Outcome {
    /// Windows accepted it. The key fires while Azure runs, except where
    /// UIPI blocks it — see `Foreground::elevated`.
    Registered,
    /// Another program holds this chord, or Windows keeps it for itself.
    /// It does not say which program, so neither does Azure.
    Refused { reason: String },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Registration {
    pub action: Action,
    #[ts(type = "string")]
    pub chord: Chord,
    pub outcome: Outcome,
}

impl Registration {
    pub fn refused(action: Action, chord: Chord, reason: impl Into<String>) -> Self {
        Registration { action, chord, outcome: Outcome::Refused { reason: reason.into() } }
    }
}

/// A running set of registrations. Dropping it gives every chord back to
/// the system, which is what makes rebinding a matter of replacing this.
pub struct Hotkeys {
    /// Held, never read. Its `Drop` is the whole contract: it posts the
    /// hotkey thread a quit and the thread gives every chord back to the
    /// system. Rebinding replaces this value, and the old chords are
    /// released by the replacement going out of scope.
    #[cfg(windows)]
    #[allow(dead_code)]
    inner: crate::win::HotkeyThread,
    registrations: Vec<Registration>,
}

impl Hotkeys {
    /// Registers every bound action and returns with the answer for each.
    ///
    /// `sink` is called on the hotkey thread. It must not block: the same
    /// thread carries the message loop that notices the next key.
    #[cfg(windows)]
    pub fn spawn(bindings: &BindingSet, sink: impl Fn(HotkeyEvent) + Send + 'static) -> Hotkeys {
        let (inner, registrations) = crate::win::HotkeyThread::spawn(bindings, sink);
        Hotkeys { inner, registrations }
    }

    #[cfg(not(windows))]
    pub fn spawn(bindings: &BindingSet, _sink: impl Fn(HotkeyEvent) + Send + 'static) -> Hotkeys {
        Hotkeys {
            registrations: bindings
                .bound()
                .into_iter()
                .map(|(action, chord)| {
                    Registration::refused(action, chord, "global hotkeys are a Windows facility")
                })
                .collect(),
        }
    }

    pub fn registrations(&self) -> &[Registration] {
        &self.registrations
    }

    /// The ones that did not take. Empty is the ordinary case, and the
    /// interface stays quiet when it is.
    pub fn refusals(&self) -> Vec<&Registration> {
        self.registrations
            .iter()
            .filter(|r| !matches!(r.outcome, Outcome::Registered))
            .collect()
    }
}
