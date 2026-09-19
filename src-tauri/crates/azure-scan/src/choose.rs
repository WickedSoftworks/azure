//! Deciding which executable in a game's folder is the game.
//!
//! Four of the six launchers Azure can read hand over a directory and
//! nothing else. A directory holds the game, its crash handler, its
//! anti-cheat bootstrapper, two redistributable installers and an
//! uninstaller — and binding a preset to the wrong one produces a preset
//! that never fires, whose only symptom is that nothing happens.
//!
//! So this scores rather than filters to one, and what it could not be
//! sure about it labels `Guess`.

use crate::library::Confidence;

/// One executable found under a game's directory.
///
/// Taken as data rather than read here, so the whole of this file is
/// testable without a game installed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Found {
    /// Path relative to the game's directory, with `/` or `\` separators.
    pub relative: String,
    pub bytes: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Chosen {
    pub relative: String,
    pub confidence: Confidence,
}

/// Executables that are never the game, matched against the file stem.
const NOT_A_GAME: &[&str] = &[
    "crashhandler",
    "crashreport",
    "crashpad",
    "unins",
    "uninstall",
    "vcredist",
    "directx",
    "dxsetup",
    "dxwebsetup",
    "dotnet",
    "prereq",
    "redist",
    "setup",
    "installer",
    "helper",
    "subprocess",
    "webhelper",
    "handler",
    "updater",
    "patcher",
    "activation",
];

/// Directories whose contents are never the game.
const NOT_A_GAME_DIR: &[&str] = &[
    "_commonredist",
    "commonredist",
    "redist",
    "redistributables",
    "directx",
    "dotnet",
    "vcredist",
    "easyanticheat",
    "battleye",
    "thirdparty",
    "support",
];

/// Picks the executable most likely to be the one that reaches the
/// foreground, or `None` when nothing plausible is left.
///
/// A directory with no game in it is not reported as a game with no
/// executable; it is not reported.
pub fn choose(listing: &[Found], game_name: &str) -> Option<Chosen> {
    let mut scored: Vec<(Score, &Found)> = listing
        .iter()
        .filter(|f| plausible(&f.relative))
        .map(|f| (score(f, game_name), f))
        .collect();

    if scored.is_empty() {
        return None;
    }

    // Highest score first; ties go to the larger file, which is the
    // better proxy for "the game" than any name rule.
    scored.sort_by(|a, b| b.0.points.cmp(&a.0.points).then(b.1.bytes.cmp(&a.1.bytes)));

    let (top, found) = &scored[0];
    // Confidence answers "did the name match", not "did it score well".
    // A game at `game/bin/win64/cs2.exe` pays a depth penalty that would
    // otherwise drag a perfectly good name match down into a guess.
    let confidence = if top.named || scored.len() == 1 {
        Confidence::Likely
    } else {
        Confidence::Guess
    };

    Some(Chosen { relative: found.relative.clone(), confidence })
}

/// Everything plausible, best first. Used where the interface offers a
/// choice rather than taking one — an anti-cheat variant such as
/// `RainbowSix_BE.exe` is a real alternative, not noise, because which
/// one reaches the foreground depends on how the game was started.
pub fn rank(listing: &[Found], game_name: &str) -> Vec<String> {
    let mut scored: Vec<(Score, &Found)> = listing
        .iter()
        .filter(|f| plausible(&f.relative))
        .map(|f| (score(f, game_name), f))
        .collect();
    scored.sort_by(|a, b| b.0.points.cmp(&a.0.points).then(b.1.bytes.cmp(&a.1.bytes)));
    scored.into_iter().map(|(_, f)| f.relative.clone()).collect()
}

/// The score a stem matching the game's name earns.
const NAME_MATCH: i64 = 500;

/// How well one executable answers, and whether its name was the reason.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Score {
    points: i64,
    /// True when the stem resembles the game's title. Kept apart from
    /// `points` because position and size move the score without saying
    /// anything about whether this is the right binary.
    named: bool,
}

fn plausible(relative: &str) -> bool {
    let lower = relative.to_lowercase();
    if !lower.ends_with(".exe") {
        return false;
    }

    let parts: Vec<&str> = lower.split(['/', '\\']).collect();
    let (stem, dirs) = match parts.split_last() {
        Some((stem, dirs)) => (stem.trim_end_matches(".exe"), dirs),
        None => return false,
    };

    if dirs.iter().any(|d| NOT_A_GAME_DIR.contains(d)) {
        return false;
    }
    if NOT_A_GAME.iter().any(|bad| stem.contains(bad)) {
        return false;
    }

    true
}

fn score(found: &Found, game_name: &str) -> Score {
    let lower = found.relative.to_lowercase();
    let parts: Vec<&str> = lower.split(['/', '\\']).collect();
    let stem = parts.last().unwrap_or(&"").trim_end_matches(".exe");
    let depth = parts.len() as i64 - 1;

    let mut points = 0;
    let mut named = true;

    let wanted = squash(game_name);
    let actual = squash(stem);
    if !wanted.is_empty() && !actual.is_empty() {
        if actual == wanted {
            points += NAME_MATCH + 200;
        } else if wanted.starts_with(&actual) || actual.starts_with(&wanted) {
            // `RainbowSix` against `tomclancysrainbowsixsiege` does not
            // match either way round, which is what the initials rule
            // below is for.
            points += NAME_MATCH + 100;
        } else if wanted.contains(&actual)
            || actual.contains(&wanted)
            || words_match(game_name, stem)
            || acronym_match(game_name, stem)
        {
            points += NAME_MATCH;
        } else {
            named = false;
        }
    } else {
        named = false;
    }

    // A game usually sits at or near the top of its own folder; deep
    // paths are engine and tooling binaries.
    points -= depth * 40;

    // Size as the tie-breaker of last resort, flattened so a large
    // redistributable cannot outweigh a name match.
    points += (found.bytes / (4 * 1024 * 1024)) as i64;

    Score { points, named }
}

/// `Tom Clancy's Rainbow Six® Siege` becomes `tomclancysrainbowsixsiege`.
fn squash(text: &str) -> String {
    text.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

/// The words of a title, split on spaces and punctuation alike, so that
/// `Counter-Strike 2` is three words rather than two.
fn words(game_name: &str) -> Vec<String> {
    game_name
        .split(|c: char| !c.is_ascii_alphanumeric())
        .map(squash)
        .filter(|w| !w.is_empty())
        .collect()
}

/// Whether a stem is built from the significant words of a title.
///
/// `Tom Clancy's Rainbow Six Siege` ships `RainbowSix.exe`: the words are
/// there, in order, just not all of them. Matching the stem against a
/// run of consecutive words is what connects the two.
fn words_match(game_name: &str, stem: &str) -> bool {
    let words = words(game_name);
    if words.len() < 2 {
        return false;
    }

    let target = squash(stem);
    if target.len() < 4 {
        return false;
    }

    for start in 0..words.len() {
        let mut joined = String::new();
        for word in &words[start..] {
            joined.push_str(word);
            if joined == target {
                return true;
            }
            if joined.len() > target.len() {
                break;
            }
        }
    }
    false
}

/// Whether a stem is the title's acronym.
///
/// `Counter-Strike 2` ships `cs2.exe`, and this is not a corner case —
/// it is the single most important title this product has. Without it
/// the scan picked `vconsole2.exe`, a Source 2 developer console that
/// happens to sit in the same folder and be larger.
///
/// Numbers in a title are kept whole rather than reduced to a first
/// character, because `2` in `Counter-Strike 2` survives into `cs2` as
/// itself.
fn acronym_match(game_name: &str, stem: &str) -> bool {
    let words = words(game_name);
    if words.len() < 2 {
        return false;
    }

    let mut acronym = String::new();
    for word in &words {
        if word.chars().all(|c| c.is_ascii_digit()) {
            acronym.push_str(word);
        } else if let Some(first) = word.chars().next() {
            acronym.push(first);
        }
    }

    // Two characters is not an acronym, it is a coincidence waiting to
    // happen against a folder full of short tool names.
    acronym.len() >= 3 && acronym == squash(stem)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(relative: &str, mb: u64) -> Found {
        Found { relative: relative.into(), bytes: mb * 1024 * 1024 }
    }

    /// The case the whole file exists for, taken from a real install.
    #[test]
    fn rainbow_six_is_found_past_its_anti_cheat_and_crash_handler() {
        let listing = vec![
            found("RainbowSix.exe", 40),
            found("RainbowSix_BE.exe", 1),
            found("RainbowSix_Vulkan.exe", 42),
            found("UnityCrashHandler64.exe", 2),
            found("_CommonRedist/vcredist/2015/vc_redist.x64.exe", 14),
            found("support/EasyAntiCheat/EasyAntiCheat_Setup.exe", 1),
        ];

        let chosen = choose(&listing, "Tom Clancy's Rainbow Six Siege").expect("a game");
        assert_eq!(chosen.relative, "RainbowSix.exe");
    }

    #[test]
    fn the_anti_cheat_variants_stay_on_offer() {
        // Which of these reaches the foreground depends on how the game
        // was launched, so they are alternatives rather than noise.
        let listing = vec![
            found("RainbowSix.exe", 40),
            found("RainbowSix_BE.exe", 1),
            found("RainbowSix_Vulkan.exe", 42),
            found("UnityCrashHandler64.exe", 2),
        ];
        let ranked = rank(&listing, "Tom Clancy's Rainbow Six Siege");
        assert!(ranked.contains(&"RainbowSix_BE.exe".to_string()));
        assert!(ranked.contains(&"RainbowSix_Vulkan.exe".to_string()));
        assert!(
            !ranked.iter().any(|r| r.contains("CrashHandler")),
            "the crash handler is not an alternative: {ranked:?}"
        );
    }

    #[test]
    fn redistributables_and_uninstallers_are_never_offered() {
        let listing = vec![
            found("_CommonRedist/DirectX/DXSETUP.exe", 2),
            found("unins000.exe", 3),
            found("vcredist_x64.exe", 14),
            found("DotNetFx45_Full_setup.exe", 1),
        ];
        assert_eq!(choose(&listing, "Some Game"), None);
        assert!(rank(&listing, "Some Game").is_empty());
    }

    #[test]
    fn a_folder_with_nothing_in_it_is_not_a_game_with_no_executable() {
        assert_eq!(choose(&[], "Some Game"), None);
    }

    #[test]
    fn a_single_plausible_executable_is_likely_not_a_guess() {
        let listing = vec![found("bin/game64.exe", 80), found("unins000.exe", 2)];
        let chosen = choose(&listing, "Whatever").expect("a game");
        assert_eq!(chosen.relative, "bin/game64.exe");
        assert_eq!(chosen.confidence, Confidence::Likely);
    }

    #[test]
    fn a_name_match_beats_a_bigger_unrelated_binary() {
        let listing = vec![
            found("Engine/Binaries/Win64/UnrealEditor.exe", 900),
            found("Valorant.exe", 30),
        ];
        let chosen = choose(&listing, "VALORANT").expect("a game");
        assert_eq!(chosen.relative, "Valorant.exe");
        assert_eq!(chosen.confidence, Confidence::Likely);
    }

    #[test]
    fn several_plausible_and_none_named_like_the_game_is_a_guess() {
        let listing = vec![found("start.exe", 20), found("bin/run.exe", 30)];
        let chosen = choose(&listing, "Some Game").expect("a game");
        assert_eq!(chosen.confidence, Confidence::Guess);
    }

    #[test]
    fn a_shallower_executable_wins_a_tie() {
        let listing = vec![
            found("Binaries/Win64/game.exe", 50),
            found("game.exe", 50),
        ];
        let chosen = choose(&listing, "unrelated").expect("a game");
        assert_eq!(chosen.relative, "game.exe");
    }

    #[test]
    fn a_run_of_words_from_the_title_counts_as_the_name() {
        assert!(words_match("Tom Clancy's Rainbow Six Siege", "RainbowSix"));
        assert!(words_match("Apex Legends", "ApexLegends"));
        assert!(!words_match("Apex Legends", "steam"));
    }

    #[test]
    fn an_acronym_counts_as_the_name() {
        assert!(acronym_match("Counter-Strike 2", "cs2"));
        assert!(acronym_match("Counter-Strike Global Offensive", "csgo"));
        assert!(acronym_match("Grand Theft Auto V", "gtav"));
        // Two letters is a coincidence, not an acronym.
        assert!(!acronym_match("Team Fortress", "tf"));
        assert!(!acronym_match("Counter-Strike 2", "vconsole2"));
    }

    /// The most important title this product has, and the case that was
    /// wrong when the scan first ran against a real Steam library.
    #[test]
    fn counter_strike_2_is_the_game_and_not_the_developer_console() {
        // Straight from the real install: cs2.exe is a small launcher
        // shim and vconsole2.exe is larger, so size alone picks wrong.
        let listing = vec![
            Found { relative: "game/bin/win64/cs2.exe".into(), bytes: 500 * 1024 },
            Found { relative: "game/bin/win64/vconsole2.exe".into(), bytes: 3 * 1024 * 1024 },
            Found { relative: "game/bin/win64/steamerrorreporter64.exe".into(), bytes: 900 * 1024 },
        ];

        let chosen = choose(&listing, "Counter-Strike 2").expect("a game");
        assert_eq!(chosen.relative, "game/bin/win64/cs2.exe");
        assert_eq!(chosen.confidence, Confidence::Likely);
    }

    #[test]
    fn non_executables_are_ignored_whatever_they_are_called() {
        let listing = vec![
            Found { relative: "Valorant.dll".into(), bytes: 900 },
            Found { relative: "readme.txt".into(), bytes: 1 },
        ];
        assert_eq!(choose(&listing, "Valorant"), None);
    }
}
