//! What the keys do, and the handful of preferences that outlive a run.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::chord::Chord;

/// Bumped only when the stored shape changes in a way a previous version
/// could not read. Nothing migrates yet because nothing has shipped.
pub const SCHEMA_VERSION: u32 = 1;

/// Something a key can be bound to.
///
/// Every one of these is reachable from the window too. A hotkey is a
/// faster route to an existing action, never the only route — a binding
/// Windows refuses must not take a capability away with it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum Action {
    ToggleEnabled,
    CycleNext,
    CyclePrev,
    RestoreDisplay,
    /// Held, not pressed: the display stands down while the key is down
    /// and comes back when it is released. The only action whose binding
    /// costs a poll, because Windows reports no key release to a hotkey.
    HoldBypass,
}

pub const ACTIONS: [Action; 5] = [
    Action::ToggleEnabled,
    Action::CycleNext,
    Action::CyclePrev,
    Action::RestoreDisplay,
    Action::HoldBypass,
];

// What each action is *called* is not here. Every other label a person
// reads lives in `src/lib/model.ts` beside the rest of the interface's
// strings, and a second copy in Rust would be one that could disagree.

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct BindingSet {
    #[ts(type = "string | null")]
    pub toggle_enabled: Option<Chord>,
    #[ts(type = "string | null")]
    pub cycle_next: Option<Chord>,
    #[ts(type = "string | null")]
    pub cycle_prev: Option<Chord>,
    #[ts(type = "string | null")]
    pub restore_display: Option<Chord>,
    #[ts(type = "string | null")]
    pub hold_bypass: Option<Chord>,
}

/// Two actions asking for the same key. Distinct from Windows refusing a
/// registration, and reported separately: this one is Azure's to fix, the
/// other belongs to whatever program got there first.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Conflict {
    #[ts(type = "string")]
    pub chord: Chord,
    pub actions: Vec<Action>,
}

impl Default for BindingSet {
    /// The three the surface advertised before M1 removed the footer for
    /// claiming what did not exist, and a fourth left unset.
    ///
    /// `hold_bypass` has no default on purpose. A key held down mid-match
    /// is a choice about what your hand is already near, and a guess costs
    /// whatever the game wanted that key for.
    fn default() -> Self {
        let chord = |text: &str| Some(Chord::parse(text).expect("a default that does not parse"));
        BindingSet {
            toggle_enabled: chord("ALT+SHIFT+V"),
            cycle_next: chord("ALT+SHIFT+RIGHT"),
            cycle_prev: chord("ALT+SHIFT+LEFT"),
            restore_display: chord("CTRL+ALT+SHIFT+R"),
            hold_bypass: None,
        }
    }
}

impl BindingSet {
    pub fn get(&self, action: Action) -> Option<Chord> {
        match action {
            Action::ToggleEnabled => self.toggle_enabled,
            Action::CycleNext => self.cycle_next,
            Action::CyclePrev => self.cycle_prev,
            Action::RestoreDisplay => self.restore_display,
            Action::HoldBypass => self.hold_bypass,
        }
    }

    pub fn set(&mut self, action: Action, chord: Option<Chord>) {
        let slot = match action {
            Action::ToggleEnabled => &mut self.toggle_enabled,
            Action::CycleNext => &mut self.cycle_next,
            Action::CyclePrev => &mut self.cycle_prev,
            Action::RestoreDisplay => &mut self.restore_display,
            Action::HoldBypass => &mut self.hold_bypass,
        };
        *slot = chord;
    }

    /// Every action, bound or not, in a fixed order. The interface lists
    /// unbound actions too — an action you cannot see is one you cannot
    /// bind.
    pub fn all(&self) -> Vec<(Action, Option<Chord>)> {
        ACTIONS.iter().map(|&a| (a, self.get(a))).collect()
    }

    pub fn bound(&self) -> Vec<(Action, Chord)> {
        ACTIONS
            .iter()
            .filter_map(|&a| self.get(a).map(|c| (a, c)))
            .collect()
    }

    pub fn conflicts(&self) -> Vec<Conflict> {
        let mut out: Vec<Conflict> = Vec::new();
        for (action, chord) in self.bound() {
            match out.iter_mut().find(|c| c.chord == chord) {
                Some(existing) => existing.actions.push(action),
                None => out.push(Conflict { chord, actions: vec![action] }),
            }
        }
        out.retain(|c| c.actions.len() > 1);
        out
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Settings {
    pub version: u32,
    pub bindings: BindingSet,
    /// Off unless asked for. A tool that adds itself to startup without
    /// being asked is the kind of thing this one is trying not to be.
    pub autostart: bool,
    /// Whether the player has been told where the window went. Once ever,
    /// not once a session.
    pub told_about_tray: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            version: SCHEMA_VERSION,
            bindings: BindingSet::default(),
            autostart: false,
            told_about_tray: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_defaults_are_the_three_the_footer_used_to_promise() {
        let b = BindingSet::default();
        assert_eq!(b.toggle_enabled.unwrap().to_string(), "ALT+SHIFT+V");
        assert_eq!(b.cycle_next.unwrap().to_string(), "ALT+SHIFT+RIGHT");
        assert_eq!(b.cycle_prev.unwrap().to_string(), "ALT+SHIFT+LEFT");
        assert_eq!(b.restore_display.unwrap().to_string(), "CTRL+ALT+SHIFT+R");
    }

    #[test]
    fn hold_to_bypass_ships_unbound() {
        assert_eq!(BindingSet::default().hold_bypass, None);
    }

    #[test]
    fn the_defaults_do_not_conflict_with_each_other() {
        assert!(BindingSet::default().conflicts().is_empty());
    }

    #[test]
    fn every_action_is_listed_whether_bound_or_not() {
        let all = BindingSet::default().all();
        assert_eq!(all.len(), ACTIONS.len());
        assert!(all.iter().any(|(a, c)| *a == Action::HoldBypass && c.is_none()));
    }

    #[test]
    fn two_actions_on_one_chord_are_reported_together() {
        let mut b = BindingSet::default();
        b.set(Action::HoldBypass, Chord::parse("ALT+SHIFT+V").ok());

        let conflicts = b.conflicts();
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].chord.to_string(), "ALT+SHIFT+V");
        assert_eq!(
            conflicts[0].actions,
            vec![Action::ToggleEnabled, Action::HoldBypass]
        );
    }

    #[test]
    fn a_conflict_is_the_same_chord_however_it_was_typed() {
        let mut b = BindingSet::default();
        b.set(Action::HoldBypass, Chord::parse("shift+alt+v").ok());
        assert_eq!(b.conflicts().len(), 1, "order of modifiers is not identity");
    }

    #[test]
    fn unbinding_clears_the_conflict() {
        let mut b = BindingSet::default();
        b.set(Action::HoldBypass, Chord::parse("ALT+SHIFT+V").ok());
        assert_eq!(b.conflicts().len(), 1);
        b.set(Action::HoldBypass, None);
        assert!(b.conflicts().is_empty());
    }

    #[test]
    fn settings_survive_a_round_trip() {
        let mut s = Settings { autostart: true, ..Default::default() };
        s.bindings.set(Action::HoldBypass, Chord::parse("ALT+X").ok());

        let json = serde_json::to_string(&s).unwrap();
        assert_eq!(serde_json::from_str::<Settings>(&json).unwrap(), s);
    }

    #[test]
    fn bindings_cross_the_wire_as_strings_the_interface_can_print() {
        let json = serde_json::to_string(&BindingSet::default()).unwrap();
        assert!(json.contains("\"toggleEnabled\":\"ALT+SHIFT+V\""), "got {json}");
        assert!(json.contains("\"holdBypass\":null"), "got {json}");
    }
}
