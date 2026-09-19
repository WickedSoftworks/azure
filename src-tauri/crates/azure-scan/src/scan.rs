//! The scan itself: the only place in this crate that opens anything.

use std::fs;
use std::path::{Path, PathBuf};

use crate::choose::{choose, Found};
use crate::library::{Candidate, Confidence, Launcher, ScanReport, SourceOutcome};
use crate::sources::{battlenet, epic, name_from_directory, steam, Installed};

/// How deep into a game's folder to look for its executable. Four is
/// enough for `Game/Binaries/Win64/game.exe`, which is where Unreal puts
/// it, and shallow enough that a library on a slow disk stays quick.
const MAX_DEPTH: usize = 4;

/// A game with more executables than this is not hiding its own in the
/// tail of the list, and walking the rest costs more than it is worth.
const MAX_FILES: usize = 1000;

/// Scans every launcher and reports per source.
///
/// Sources are visited most-precise first — Epic and GOG name their
/// executables, so their answers win the deduplication over a folder
/// Azure had to guess at.
pub fn scan() -> ScanReport {
    let mut report = ScanReport::default();

    scan_epic(&mut report);
    scan_gog(&mut report);
    scan_steam(&mut report);
    scan_ubisoft(&mut report);
    scan_ea(&mut report);
    scan_battlenet(&mut report);
    scan_xbox(&mut report);

    report.dedupe();
    report.sort();
    report
}

// ── Epic ────────────────────────────────────────────────────────────────

fn epic_manifest_dir() -> PathBuf {
    let program_data =
        std::env::var("ProgramData").unwrap_or_else(|_| r"C:\ProgramData".to_string());
    Path::new(&program_data).join("Epic/EpicGamesLauncher/Data/Manifests")
}

/// Epic is the one launcher that needs no guessing at all.
fn scan_epic(report: &mut ScanReport) {
    let dir = epic_manifest_dir();
    if !dir.is_dir() {
        report.record(Launcher::Epic, SourceOutcome::NotInstalled);
        return;
    }

    let entries = match fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(e) => {
            report.record(
                Launcher::Epic,
                SourceOutcome::Unreadable { reason: format!("{}: {e}", dir.display()) },
            );
            return;
        }
    };

    let mut found = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("item") {
            continue;
        }
        let Ok(text) = fs::read_to_string(&path) else { continue };
        let Some(game) = epic::manifest(&text) else { continue };

        // Epic writes the executable forward-slashed and relative.
        let exe = Path::new(&game.install_location).join(game.launch_executable.replace('/', "\\"));
        if !exe.is_file() {
            continue;
        }
        found.push(Candidate {
            name: game.name,
            exe: exe.to_string_lossy().to_string(),
            launcher: Launcher::Epic,
            confidence: Confidence::Exact,
        });
    }

    report.absorb(Launcher::Epic, found);
}

// ── Steam ───────────────────────────────────────────────────────────────

fn scan_steam(report: &mut ScanReport) {
    let Some(root) = steam_root() else {
        report.record(Launcher::Steam, SourceOutcome::NotInstalled);
        return;
    };

    let folders = Path::new(&root).join("steamapps/libraryfolders.vdf");
    let libraries = match fs::read_to_string(&folders) {
        Ok(text) => steam::libraries(&text),
        Err(e) => {
            // Steam is installed but its library index is unreadable,
            // which is a different thing from not having Steam.
            report.record(
                Launcher::Steam,
                SourceOutcome::Unreadable { reason: format!("{}: {e}", folders.display()) },
            );
            return;
        }
    };

    let mut found = Vec::new();
    for library in libraries {
        let steamapps = Path::new(&library).join("steamapps");
        let Ok(entries) = fs::read_dir(&steamapps) else { continue };

        for entry in entries.flatten() {
            let path = entry.path();
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if !name.starts_with("appmanifest_") || !name.ends_with(".acf") {
                continue;
            }
            let Ok(text) = fs::read_to_string(&path) else { continue };
            let Some(app) = steam::app(&text) else { continue };

            let dir = steamapps.join("common").join(&app.installdir);
            if let Some(candidate) = from_directory(&dir, &app.name, Launcher::Steam) {
                found.push(candidate);
            }
        }
    }

    report.absorb(Launcher::Steam, found);
}

#[cfg(windows)]
fn steam_root() -> Option<String> {
    crate::win::steam_path()
}

#[cfg(not(windows))]
fn steam_root() -> Option<String> {
    None
}

// ── the registry-backed launchers ───────────────────────────────────────

fn scan_gog(report: &mut ScanReport) {
    scan_installed(report, Launcher::Gog, gog());
}

fn scan_ubisoft(report: &mut ScanReport) {
    scan_installed(report, Launcher::Ubisoft, ubisoft());
}

fn scan_ea(report: &mut ScanReport) {
    scan_installed(report, Launcher::Ea, ea());
}

/// Shared by every launcher that answers with a directory, and sometimes
/// an executable.
fn scan_installed(report: &mut ScanReport, launcher: Launcher, installs: Vec<Installed>) {
    if installs.is_empty() {
        report.record(launcher, SourceOutcome::NotInstalled);
        return;
    }

    let mut found = Vec::new();
    for install in installs {
        // GOG records the executable, so there is nothing to choose.
        if let Some(exe) = install.exe.as_deref() {
            let path = if Path::new(exe).is_absolute() {
                PathBuf::from(exe)
            } else {
                Path::new(&install.directory).join(exe)
            };
            if path.is_file() {
                found.push(Candidate {
                    name: install.name,
                    exe: path.to_string_lossy().to_string(),
                    launcher,
                    confidence: Confidence::Exact,
                });
                continue;
            }
        }

        if let Some(candidate) =
            from_directory(Path::new(&install.directory), &install.name, launcher)
        {
            found.push(candidate);
        }
    }

    report.absorb(launcher, found);
}

#[cfg(windows)]
fn gog() -> Vec<Installed> {
    crate::win::gog_installs()
}
#[cfg(windows)]
fn ubisoft() -> Vec<Installed> {
    crate::win::ubisoft_installs()
}
#[cfg(windows)]
fn ea() -> Vec<Installed> {
    crate::win::ea_installs()
}

#[cfg(not(windows))]
fn gog() -> Vec<Installed> {
    Vec::new()
}
#[cfg(not(windows))]
fn ubisoft() -> Vec<Installed> {
    Vec::new()
}
#[cfg(not(windows))]
fn ea() -> Vec<Installed> {
    Vec::new()
}

// ── Battle.net ──────────────────────────────────────────────────────────

fn scan_battlenet(report: &mut ScanReport) {
    let Ok(appdata) = std::env::var("APPDATA") else {
        report.record(Launcher::BattleNet, SourceOutcome::NotInstalled);
        return;
    };
    let config = Path::new(&appdata).join("Battle.net/Battle.net.config");
    let Ok(text) = fs::read_to_string(&config) else {
        report.record(Launcher::BattleNet, SourceOutcome::NotInstalled);
        return;
    };

    let found = battlenet::installs(&text)
        .into_iter()
        .filter_map(|directory| {
            let name = name_from_directory(&directory);
            from_directory(Path::new(&directory), &name, Launcher::BattleNet)
        })
        .collect();

    report.absorb(Launcher::BattleNet, found);
}

// ── Xbox ────────────────────────────────────────────────────────────────

/// Xbox and Microsoft Store games live under `WindowsApps`, which is
/// ACL-locked against an unelevated process — and Azure runs unelevated
/// deliberately.
///
/// Reporting nothing found would be a lie by omission. The scan says what
/// it cannot do and why, and the capture-foreground button handles these
/// titles perfectly, because the watcher sees the foreground process
/// wherever it was installed.
fn scan_xbox(report: &mut ScanReport) {
    report.record(
        Launcher::Xbox,
        SourceOutcome::Unreadable {
            reason: "Windows does not let Azure read WindowsApps; use capture-foreground with the game running".into(),
        },
    );
}

// ── walking a game's folder ─────────────────────────────────────────────

/// Picks the executable out of a game's directory, or gives up quietly.
fn from_directory(dir: &Path, name: &str, launcher: Launcher) -> Option<Candidate> {
    if !dir.is_dir() {
        return None;
    }

    let mut listing = Vec::new();
    walk(dir, dir, 0, &mut listing);
    let chosen = choose(&listing, name)?;

    Some(Candidate {
        name: name.to_string(),
        exe: dir.join(chosen.relative.replace('/', "\\")).to_string_lossy().to_string(),
        launcher,
        confidence: chosen.confidence,
    })
}

/// Collects executables relative to `root`, bounded in depth and count.
fn walk(root: &Path, dir: &Path, depth: usize, into: &mut Vec<Found>) {
    if depth >= MAX_DEPTH || into.len() >= MAX_FILES {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else { return };

    for entry in entries.flatten() {
        if into.len() >= MAX_FILES {
            return;
        }
        let path = entry.path();
        let Ok(kind) = entry.file_type() else { continue };

        if kind.is_dir() {
            walk(root, &path, depth + 1, into);
            continue;
        }
        if !kind.is_file() {
            continue;
        }
        if path.extension().and_then(|e| e.to_str()).map(|e| e.eq_ignore_ascii_case("exe"))
            != Some(true)
        {
            continue;
        }

        let Ok(relative) = path.strip_prefix(root) else { continue };
        into.push(Found {
            relative: relative.to_string_lossy().to_string(),
            bytes: entry.metadata().map(|m| m.len()).unwrap_or(0),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::LAUNCHERS;

    /// Runs against whatever this machine actually has.
    ///
    /// It cannot assert a game count — that differs on every machine —
    /// so it asserts the thing that must hold everywhere: every source
    /// answers, exactly once, and nothing panics getting there.
    #[test]
    fn every_launcher_answers_exactly_once_on_this_machine() {
        let report = scan();

        assert_eq!(report.sources.len(), LAUNCHERS.len());
        for launcher in LAUNCHERS {
            let answers: Vec<_> = report
                .sources
                .iter()
                .filter(|s| s.launcher == launcher)
                .collect();
            assert_eq!(answers.len(), 1, "{launcher:?} answered {} times", answers.len());
        }
    }

    #[test]
    fn every_candidate_points_at_an_executable_that_exists() {
        for candidate in scan().candidates {
            assert!(
                Path::new(&candidate.exe).is_file(),
                "{} was offered but is not on disk",
                candidate.exe
            );
            assert!(
                candidate.exe.to_lowercase().ends_with(".exe"),
                "{} is not an executable",
                candidate.exe
            );
            assert!(!candidate.name.trim().is_empty(), "a candidate with no name");
        }
    }

    #[test]
    fn xbox_says_why_rather_than_reporting_nothing_found() {
        let report = scan();
        let xbox = report
            .sources
            .iter()
            .find(|s| s.launcher == Launcher::Xbox)
            .expect("Xbox answers");
        match &xbox.outcome {
            SourceOutcome::Unreadable { reason } => {
                assert!(reason.contains("capture-foreground"), "got: {reason}")
            }
            other => panic!("Xbox should say why it cannot scan, got {other:?}"),
        }
    }

    #[test]
    fn no_two_candidates_share_an_executable() {
        let report = scan();
        let mut seen: Vec<String> = Vec::new();
        for candidate in &report.candidates {
            let key = candidate.exe.to_lowercase();
            assert!(!seen.contains(&key), "{} was offered twice", candidate.exe);
            seen.push(key);
        }
    }
}
