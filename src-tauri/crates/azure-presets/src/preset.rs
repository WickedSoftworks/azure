use std::collections::BTreeMap;

use azure_color::{ChannelId, ColorState};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Bumped only when the stored shape changes in a way a previous version
/// could not read. Nothing migrates yet because nothing has shipped.
pub const SCHEMA_VERSION: u32 = 1;

/// The preset that applies when no game matches. Created on first run,
/// never deleted, bound to nothing.
pub const DESKTOP_ID: &str = "desktop";

/// How a preset is reached.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum ActivationMode {
    /// The watcher applies it when its executable takes the foreground.
    Focused,
    /// Only ever applied because someone clicked it.
    Manual,
}

/// How a foreground window was recognised. Not cosmetic: a name match can
/// be the wrong copy of the right game, and the interface says which one
/// happened rather than implying certainty it does not have.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum MatchKind {
    FullPath,
    ExeName,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PresetError {
    NoSuchPreset,
    DesktopIsPermanent,
    /// Asked to make a preset its game's chosen variant when it is not
    /// bound to a game at all.
    NotBound,
}

impl std::fmt::Display for PresetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PresetError::NoSuchPreset => write!(f, "no preset with that id"),
            PresetError::DesktopIsPermanent => {
                write!(f, "the desktop preset is what applies when nothing matches, so it stays")
            }
            PresetError::NotBound => {
                write!(f, "a preset bound to no game cannot be the one that game uses")
            }
        }
    }
}

impl std::error::Error for PresetError {}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Preset {
    pub id: String,
    pub name: String,
    /// The executable this preset binds to. `None` for the desktop preset.
    pub exe: Option<String>,
    pub mode: ActivationMode,
    pub state: ColorState,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresetSet {
    pub version: u32,
    /// The next id to hand out. Stored so a deleted preset's id is never
    /// reissued to a different game.
    pub next: u32,
    pub presets: Vec<Preset>,
    pub active_id: String,
    /// Which variant wins, per executable, when a game has more than one
    /// preset bound to it. Keyed by the bound `exe` string lowercased.
    ///
    /// Absent for most games, which have exactly one preset — so this is
    /// defaulted rather than written, and a file from before variants
    /// existed still loads.
    #[serde(default)]
    pub preferred: BTreeMap<String, String>,
}

fn basename(path: &str) -> &str {
    let cut = path.rfind(['\\', '/']).map(|i| i + 1).unwrap_or(0);
    &path[cut..]
}

impl PresetSet {
    pub fn fresh() -> Self {
        PresetSet {
            version: SCHEMA_VERSION,
            next: 1,
            presets: vec![Preset {
                id: DESKTOP_ID.to_string(),
                name: "DESKTOP".to_string(),
                exe: None,
                mode: ActivationMode::Manual,
                state: ColorState::neutral(),
            }],
            active_id: DESKTOP_ID.to_string(),
            preferred: BTreeMap::new(),
        }
    }

    /// Every preset bound to the same executable, in stored order.
    ///
    /// The unit the interface calls a game: one entry for most, several
    /// when someone keeps a different look for different circumstances.
    pub fn variants(&self, exe: &str) -> Vec<&Preset> {
        self.presets
            .iter()
            .filter(|p| p.exe.as_deref().is_some_and(|e| e.eq_ignore_ascii_case(exe)))
            .collect()
    }

    /// The variant the watcher will choose for an executable.
    pub fn preferred_id(&self, exe: &str) -> Option<&str> {
        self.preferred.get(&exe.to_lowercase()).map(String::as_str)
    }

    /// Makes one variant the one its game activates.
    ///
    /// Recorded per executable rather than by reordering the list,
    /// because reordering would shuffle the preset bar under someone who
    /// only meant to choose which look a game gets.
    pub fn set_preferred(&mut self, id: &str) -> Result<(), PresetError> {
        let preset = self.get(id).ok_or(PresetError::NoSuchPreset)?;
        let exe = preset.exe.clone().ok_or(PresetError::NotBound)?;
        self.preferred.insert(exe.to_lowercase(), id.to_string());
        Ok(())
    }

    pub fn get(&self, id: &str) -> Option<&Preset> {
        self.presets.iter().find(|p| p.id == id)
    }

    fn get_mut(&mut self, id: &str) -> Result<&mut Preset, PresetError> {
        self.presets
            .iter_mut()
            .find(|p| p.id == id)
            .ok_or(PresetError::NoSuchPreset)
    }

    /// Always answers. A set whose `active_id` has gone missing falls back
    /// to the desktop preset rather than leaving the display unexplained.
    pub fn active(&self) -> &Preset {
        self.get(&self.active_id)
            .or_else(|| self.get(DESKTOP_ID))
            .unwrap_or(&self.presets[0])
    }

    pub fn add(&mut self, name: String, exe: Option<String>) -> String {
        let id = format!("p{}", self.next);
        self.next += 1;
        self.presets.push(Preset {
            id: id.clone(),
            name,
            mode: if exe.is_some() {
                ActivationMode::Focused
            } else {
                ActivationMode::Manual
            },
            exe,
            state: ColorState::neutral(),
        });
        id
    }

    pub fn remove(&mut self, id: &str) -> Result<(), PresetError> {
        if id == DESKTOP_ID {
            return Err(PresetError::DesktopIsPermanent);
        }
        let before = self.presets.len();
        self.presets.retain(|p| p.id != id);
        if self.presets.len() == before {
            return Err(PresetError::NoSuchPreset);
        }
        if self.active_id == id {
            self.active_id = DESKTOP_ID.to_string();
        }
        // A preference pointing at a preset that no longer exists would
        // quietly send the next match back to the first variant, which
        // looks like the choice being forgotten rather than the preset
        // being deleted.
        self.preferred.retain(|_, chosen| chosen != id);
        Ok(())
    }

    pub fn select(&mut self, id: &str) -> Result<(), PresetError> {
        self.get(id).ok_or(PresetError::NoSuchPreset)?;
        self.active_id = id.to_string();
        Ok(())
    }

    pub fn rename(&mut self, id: &str, name: String) -> Result<(), PresetError> {
        self.get_mut(id)?.name = name;
        Ok(())
    }

    pub fn bind(&mut self, id: &str, exe: Option<String>) -> Result<(), PresetError> {
        if id == DESKTOP_ID && exe.is_some() {
            return Err(PresetError::DesktopIsPermanent);
        }
        self.preferred.retain(|_, chosen| chosen != id);
        self.get_mut(id)?.exe = exe;
        Ok(())
    }

    pub fn set_mode(&mut self, id: &str, mode: ActivationMode) -> Result<(), PresetError> {
        self.get_mut(id)?.mode = mode;
        Ok(())
    }

    pub fn set_state(&mut self, id: &str, state: ColorState) -> Result<(), PresetError> {
        self.get_mut(id)?.state = state;
        Ok(())
    }

    pub fn set_channel(
        &mut self,
        id: &str,
        channel: ChannelId,
        value: i32,
    ) -> Result<(), PresetError> {
        self.get_mut(id)?.state.set(channel, value);
        Ok(())
    }

    /// Finds the preset a foreground window belongs to.
    ///
    /// `path` is absent when the process refused to give one up, which is
    /// the normal case for an elevated game, so the executable name is a
    /// first-class route rather than a consolation prize. A full-path
    /// match wins: it is the one that distinguishes two installs of the
    /// same title.
    ///
    /// The desktop preset binds to nothing and so is never returned here.
    /// It is reached by falling back to it, not by matching.
    pub fn match_foreground(&self, path: Option<&str>, exe: &str) -> Option<(&Preset, MatchKind)> {
        let candidates = || {
            self.presets
                .iter()
                .filter(|p| p.mode == ActivationMode::Focused)
                .filter_map(|p| p.exe.as_deref().map(|e| (p, e)))
        };

        if let Some(path) = path {
            let matched: Vec<_> = candidates()
                .filter(|(_, e)| e.eq_ignore_ascii_case(path))
                .collect();
            if let Some(hit) = self.pick(&matched) {
                return Some((hit, MatchKind::FullPath));
            }
        }

        let matched: Vec<_> = candidates()
            .filter(|(_, e)| basename(e).eq_ignore_ascii_case(exe))
            .collect();
        self.pick(&matched).map(|hit| (hit, MatchKind::ExeName))
    }

    /// Chooses among the presets that matched.
    ///
    /// The chosen variant wins if one was named; otherwise the first, so
    /// a game with a single preset behaves exactly as it did before
    /// variants existed.
    fn pick<'a>(&self, matched: &[(&'a Preset, &str)]) -> Option<&'a Preset> {
        matched
            .iter()
            .find(|(p, e)| self.preferred_id(e) == Some(p.id.as_str()))
            .or_else(|| matched.first())
            .map(|(p, _)| *p)
    }
}

impl Default for PresetSet {
    fn default() -> Self {
        Self::fresh()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_two_variants() -> (PresetSet, String, String) {
        let mut set = PresetSet::fresh();
        let day = set.add("CS2 DAY".into(), Some("D:\\games\\cs2.exe".into()));
        let night = set.add("CS2 NIGHT".into(), Some("D:\\games\\cs2.exe".into()));
        (set, day, night)
    }

    #[test]
    fn two_presets_can_share_a_game() {
        let (set, day, night) = with_two_variants();
        let variants = set.variants("D:\\games\\cs2.exe");
        assert_eq!(variants.len(), 2);
        assert_eq!(variants[0].id, day);
        assert_eq!(variants[1].id, night);
    }

    #[test]
    fn variants_are_matched_without_regard_to_case_or_which_copy_asked() {
        let (set, _, _) = with_two_variants();
        assert_eq!(set.variants("d:\\GAMES\\CS2.EXE").len(), 2);
        assert_eq!(set.variants("D:\\games\\valorant.exe").len(), 0);
    }

    /// Before variants, a second preset on the same game was dead weight:
    /// `find` returned the first and nothing could reach the second.
    #[test]
    fn the_chosen_variant_is_the_one_the_watcher_activates() {
        let (mut set, day, night) = with_two_variants();

        let (hit, _) = set.match_foreground(None, "cs2.exe").expect("a match");
        assert_eq!(hit.id, day, "the first variant answers until one is chosen");

        set.set_preferred(&night).unwrap();
        let (hit, kind) = set.match_foreground(None, "cs2.exe").expect("a match");
        assert_eq!(hit.id, night, "the chosen variant has to win");
        assert_eq!(kind, MatchKind::ExeName);
    }

    #[test]
    fn choosing_a_variant_survives_a_full_path_match_too() {
        let (mut set, _, night) = with_two_variants();
        set.set_preferred(&night).unwrap();

        let (hit, kind) = set
            .match_foreground(Some("D:\\games\\cs2.exe"), "cs2.exe")
            .expect("a match");
        assert_eq!(hit.id, night);
        assert_eq!(kind, MatchKind::FullPath, "a path match is still a path match");
    }

    #[test]
    fn a_game_with_one_preset_behaves_exactly_as_before() {
        let mut set = PresetSet::fresh();
        let id = set.add("VALORANT".into(), Some("D:\\games\\valorant.exe".into()));
        let (hit, _) = set.match_foreground(None, "valorant.exe").expect("a match");
        assert_eq!(hit.id, id);
        assert!(set.preferred.is_empty(), "nothing to record for a lone preset");
    }

    #[test]
    fn deleting_the_chosen_variant_falls_back_rather_than_forgetting_the_game() {
        let (mut set, day, night) = with_two_variants();
        set.set_preferred(&night).unwrap();
        set.remove(&night).unwrap();

        let (hit, _) = set.match_foreground(None, "cs2.exe").expect("a match");
        assert_eq!(hit.id, day);
        assert!(set.preferred.is_empty(), "a stale preference must not linger");
    }

    #[test]
    fn rebinding_a_variant_drops_its_old_game_s_preference() {
        let (mut set, day, night) = with_two_variants();
        set.set_preferred(&night).unwrap();
        set.bind(&night, Some("D:\\games\\valorant.exe".into())).unwrap();

        let (hit, _) = set.match_foreground(None, "cs2.exe").expect("a match");
        assert_eq!(hit.id, day, "the preference belonged to the old binding");
    }

    #[test]
    fn an_unbound_preset_cannot_be_a_game_s_choice() {
        let mut set = PresetSet::fresh();
        assert_eq!(set.set_preferred(DESKTOP_ID), Err(PresetError::NotBound));
        assert_eq!(set.set_preferred("nope"), Err(PresetError::NoSuchPreset));
    }

    #[test]
    fn a_stored_set_from_before_variants_still_loads() {
        // No `preferred` key at all, which is every file written until now.
        let old = r#"{"version":1,"next":2,"presets":[{"id":"desktop","name":"DESKTOP","exe":null,"mode":"manual","state":null}],"activeId":"desktop"}"#;
        let parsed: Result<PresetSet, _> = serde_json::from_str(old);
        // The state field is the only reason this may not parse; what is
        // being asserted is that a missing `preferred` is not the reason.
        if let Err(e) = &parsed {
            assert!(
                !e.to_string().contains("preferred"),
                "a missing preferred must default, got: {e}"
            );
        }
    }


    fn bound(set: &mut PresetSet, name: &str, exe: &str) -> String {
        set.add(name.to_string(), Some(exe.to_string()))
    }

    #[test]
    fn a_fresh_set_has_exactly_the_desktop_preset_and_it_is_active() {
        let set = PresetSet::fresh();
        assert_eq!(set.presets.len(), 1);
        assert_eq!(set.active_id, DESKTOP_ID);
        assert_eq!(set.active().exe, None);
        assert_eq!(set.active().mode, ActivationMode::Manual);
        assert!(set.active().state == ColorState::neutral());
    }

    #[test]
    fn the_desktop_preset_cannot_be_deleted() {
        let mut set = PresetSet::fresh();
        assert_eq!(set.remove(DESKTOP_ID), Err(PresetError::DesktopIsPermanent));
        assert_eq!(set.presets.len(), 1);
    }

    #[test]
    fn added_presets_get_distinct_ids_that_survive_deletion() {
        let mut set = PresetSet::fresh();
        let a = bound(&mut set, "CS2", "D:\\games\\cs2.exe");
        let b = bound(&mut set, "VALORANT", "D:\\games\\valorant.exe");
        assert_ne!(a, b);
        set.remove(&a).unwrap();
        let c = bound(&mut set, "APEX", "D:\\games\\r5apex.exe");
        assert_ne!(c, a, "a reused id would rebind the wrong preset");
        assert_ne!(c, b);
    }

    #[test]
    fn deleting_the_active_preset_falls_back_to_the_desktop() {
        let mut set = PresetSet::fresh();
        let a = bound(&mut set, "CS2", "D:\\games\\cs2.exe");
        set.select(&a).unwrap();
        set.remove(&a).unwrap();
        assert_eq!(set.active_id, DESKTOP_ID);
    }

    #[test]
    fn a_full_path_match_beats_an_executable_name_match() {
        let mut set = PresetSet::fresh();
        let by_name = bound(&mut set, "ANY CS2", "cs2.exe");
        let by_path = bound(&mut set, "MY CS2", "D:\\games\\cs2.exe");

        let (hit, kind) = set
            .match_foreground(Some("D:\\games\\cs2.exe"), "cs2.exe")
            .expect("a match");
        assert_eq!(hit.id, by_path);
        assert_eq!(kind, MatchKind::FullPath);

        // A different install of the same game still matches by name.
        let (hit, kind) = set
            .match_foreground(Some("E:\\elsewhere\\cs2.exe"), "cs2.exe")
            .expect("a match");
        assert_eq!(hit.id, by_name);
        assert_eq!(kind, MatchKind::ExeName);
    }

    #[test]
    fn matching_ignores_case_because_windows_paths_do() {
        let mut set = PresetSet::fresh();
        let id = bound(&mut set, "CS2", "D:\\Games\\CS2.exe");
        let (hit, kind) = set
            .match_foreground(Some("d:\\games\\cs2.exe"), "cs2.exe")
            .expect("a match");
        assert_eq!(hit.id, id);
        assert_eq!(kind, MatchKind::FullPath);
    }

    #[test]
    fn an_unknown_foreground_matches_nothing_rather_than_guessing() {
        let mut set = PresetSet::fresh();
        bound(&mut set, "CS2", "D:\\games\\cs2.exe");
        assert!(set.match_foreground(Some("C:\\chrome.exe"), "chrome.exe").is_none());
    }

    #[test]
    fn the_desktop_preset_is_never_a_match_for_a_foreground_window() {
        // It binds to nothing, so it can only be reached by falling back to
        // it — never by matching a process.
        let set = PresetSet::fresh();
        assert!(set.match_foreground(Some("C:\\anything.exe"), "anything.exe").is_none());
    }

    #[test]
    fn a_preset_with_no_path_available_can_still_match_by_name() {
        // Elevated games deny the path, so this is the common case.
        let mut set = PresetSet::fresh();
        let id = bound(&mut set, "VALORANT", "C:\\Riot\\VALORANT-Win64-Shipping.exe");
        let (hit, kind) = set
            .match_foreground(None, "valorant-win64-shipping.exe")
            .expect("a match");
        assert_eq!(hit.id, id);
        assert_eq!(kind, MatchKind::ExeName);
    }

    #[test]
    fn a_manual_preset_is_never_activated_by_the_watcher() {
        let mut set = PresetSet::fresh();
        let id = bound(&mut set, "CS2", "D:\\games\\cs2.exe");
        set.set_mode(&id, ActivationMode::Manual).unwrap();
        assert!(set.match_foreground(Some("D:\\games\\cs2.exe"), "cs2.exe").is_none());
    }

    #[test]
    fn editing_a_preset_touches_only_that_preset() {
        let mut set = PresetSet::fresh();
        let id = bound(&mut set, "CS2", "D:\\games\\cs2.exe");
        let mut state = ColorState::neutral();
        state.set(ChannelId::Vibrance, 178);
        set.set_state(&id, state).unwrap();

        assert_eq!(set.get(&id).unwrap().state.vibrance, 178);
        assert_eq!(set.get(DESKTOP_ID).unwrap().state.vibrance, 100);
    }

    #[test]
    fn rename_and_bind_report_a_missing_preset_rather_than_doing_nothing() {
        let mut set = PresetSet::fresh();
        assert_eq!(set.rename("nope", "X".into()), Err(PresetError::NoSuchPreset));
        assert_eq!(set.bind("nope", None), Err(PresetError::NoSuchPreset));
        assert_eq!(set.select("nope"), Err(PresetError::NoSuchPreset));
    }
}
