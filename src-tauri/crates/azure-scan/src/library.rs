//! What a scan produces, and how confident it is about each part of it.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Where a candidate came from. Shown against every result, because
/// "Steam found this" and "Azure guessed this from a folder" deserve
/// different amounts of trust from the person reading the list.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum Launcher {
    Steam,
    Epic,
    Gog,
    Ea,
    Ubisoft,
    BattleNet,
    Xbox,
}

pub const LAUNCHERS: [Launcher; 7] = [
    Launcher::Steam,
    Launcher::Epic,
    Launcher::Gog,
    Launcher::Ea,
    Launcher::Ubisoft,
    Launcher::BattleNet,
    Launcher::Xbox,
];

/// How the executable was arrived at.
///
/// Never hidden. A preset bound to the wrong binary simply never fires,
/// and the player's only clue would be that nothing happens — so the
/// guess is labelled a guess where it is offered.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum Confidence {
    /// The launcher named the executable itself.
    Exact,
    /// One plausible executable, or one that matches the game's name.
    Likely,
    /// Several were plausible and this one scored highest.
    Guess,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Candidate {
    pub name: String,
    /// Full path to the executable.
    pub exe: String,
    pub launcher: Launcher,
    pub confidence: Confidence,
}

/// What happened with one launcher.
///
/// Every source answers. Most players have two or three launchers, and
/// five quiet absences must not read as five failures.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", rename_all_fields = "camelCase", tag = "kind")]
#[ts(export)]
pub enum SourceOutcome {
    Scanned { found: u32 },
    /// No trace of the launcher. The ordinary case, and not a problem.
    NotInstalled,
    /// Installed, but Azure could not read what it needed, and why.
    Unreadable { reason: String },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SourceReport {
    pub launcher: Launcher,
    pub outcome: SourceOutcome,
}

#[derive(Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ScanReport {
    pub candidates: Vec<Candidate>,
    pub sources: Vec<SourceReport>,
}

impl ScanReport {
    pub fn record(&mut self, launcher: Launcher, outcome: SourceOutcome) {
        self.sources.push(SourceReport { launcher, outcome });
    }

    /// Records a source that produced candidates, and takes them.
    pub fn absorb(&mut self, launcher: Launcher, found: Vec<Candidate>) {
        self.record(launcher, SourceOutcome::Scanned { found: found.len() as u32 });
        self.candidates.extend(found);
    }

    /// Drops candidates that point at the same executable.
    ///
    /// A game owned on two launchers, or a Steam library listed twice,
    /// would otherwise appear twice. The first one wins, and sources are
    /// scanned in the order they answer most precisely.
    pub fn dedupe(&mut self) {
        let mut seen = Vec::new();
        self.candidates.retain(|c| {
            let key = c.exe.to_lowercase();
            if seen.contains(&key) {
                return false;
            }
            seen.push(key);
            true
        });
    }

    /// Sorted for a person reading the list, not for a machine.
    pub fn sort(&mut self) {
        self.candidates.sort_by_key(|c| c.name.to_lowercase());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(name: &str, exe: &str, launcher: Launcher) -> Candidate {
        Candidate {
            name: name.into(),
            exe: exe.into(),
            launcher,
            confidence: Confidence::Likely,
        }
    }

    #[test]
    fn a_source_that_found_nothing_still_answers() {
        let mut report = ScanReport::default();
        report.absorb(Launcher::Steam, Vec::new());
        assert_eq!(
            report.sources,
            vec![SourceReport {
                launcher: Launcher::Steam,
                outcome: SourceOutcome::Scanned { found: 0 }
            }]
        );
    }

    #[test]
    fn the_same_executable_from_two_launchers_is_listed_once() {
        let mut report = ScanReport::default();
        report.absorb(
            Launcher::Epic,
            vec![candidate("Rocket League", "D:\\rl\\RocketLeague.exe", Launcher::Epic)],
        );
        report.absorb(
            Launcher::Steam,
            vec![candidate("Rocket League", "D:\\RL\\rocketleague.EXE", Launcher::Steam)],
        );

        report.dedupe();
        assert_eq!(report.candidates.len(), 1);
        assert_eq!(
            report.candidates[0].launcher,
            Launcher::Epic,
            "the source that answered first should win"
        );
        assert_eq!(report.sources.len(), 2, "both sources still reported");
    }

    #[test]
    fn candidates_are_sorted_the_way_a_person_reads_them() {
        let mut report = ScanReport::default();
        report.absorb(
            Launcher::Steam,
            vec![
                candidate("valorant", "a.exe", Launcher::Steam),
                candidate("Apex Legends", "b.exe", Launcher::Steam),
                candidate("CS2", "c.exe", Launcher::Steam),
            ],
        );
        report.sort();
        let names: Vec<_> = report.candidates.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, vec!["Apex Legends", "CS2", "valorant"]);
    }
}
