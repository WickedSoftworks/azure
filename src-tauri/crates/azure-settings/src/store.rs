//! Where settings are kept.
//!
//! The same discipline as the preset store, for the same reason: the file
//! holds something a person chose, so a load that cannot read it keeps it
//! rather than letting the next save be the thing that destroyed it.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::settings::{Settings, SCHEMA_VERSION};

const SETTINGS: &str = "settings.json";

/// A load always succeeds. What it could not do, it reports.
pub struct Loaded {
    pub settings: Settings,
    /// Straight to the event log. Empty on an ordinary load.
    pub notices: Vec<String>,
}

pub struct Store {
    dir: PathBuf,
}

impl Store {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Store { dir: dir.into() }
    }

    pub fn path(&self) -> PathBuf {
        self.dir.join(SETTINGS)
    }

    pub fn load(&self) -> Loaded {
        let path = self.path();
        let raw = match fs::read_to_string(&path) {
            Ok(raw) => raw,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                return Loaded { settings: Settings::default(), notices: Vec::new() }
            }
            Err(e) => {
                return Loaded {
                    settings: Settings::default(),
                    notices: vec![format!("could not read {}: {e}", path.display())],
                }
            }
        };

        match serde_json::from_str::<Settings>(&raw) {
            Ok(settings) if settings.version == SCHEMA_VERSION => {
                Loaded { settings, notices: Vec::new() }
            }
            Ok(settings) => {
                let kept = self.quarantine(&path);
                Loaded {
                    settings: Settings::default(),
                    notices: vec![format!(
                        "settings.json is schema version {} and this build reads {}; kept the old file at {}",
                        settings.version,
                        SCHEMA_VERSION,
                        kept.unwrap_or_else(|| "(could not be moved aside)".into()),
                    )],
                }
            }
            Err(e) => {
                let kept = self.quarantine(&path);
                Loaded {
                    settings: Settings::default(),
                    notices: vec![format!(
                        "settings.json could not be read ({e}); kept it at {} and started with the default bindings",
                        kept.unwrap_or_else(|| "(could not be moved aside)".into()),
                    )],
                }
            }
        }
    }

    /// Writes to a temporary file and renames over the target, so an
    /// interrupted save leaves the previous settings intact rather than a
    /// half-written file.
    pub fn save(&self, settings: &Settings) -> io::Result<()> {
        fs::create_dir_all(&self.dir)?;
        let target = self.path();
        let temp = self.dir.join(format!("{SETTINGS}.tmp"));
        fs::write(&temp, serde_json::to_string_pretty(settings)? + "\n")?;
        fs::rename(&temp, &target)
    }

    fn quarantine(&self, path: &Path) -> Option<String> {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let kept = self.dir.join(format!("{SETTINGS}.corrupt-{stamp}"));
        fs::rename(path, &kept).ok()?;
        Some(kept.display().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chord::Chord;
    use crate::settings::Action;

    fn scratch(tag: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .subsec_nanos();
        let dir = std::env::temp_dir().join(format!("azure-settings-{tag}-{nanos}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_missing_file_loads_the_defaults_without_complaint() {
        let loaded = Store::new(scratch("missing")).load();
        assert_eq!(loaded.settings, Settings::default());
        assert!(loaded.notices.is_empty());
    }

    #[test]
    fn settings_survive_a_round_trip() {
        let store = Store::new(scratch("roundtrip"));
        let mut settings =
            Settings { autostart: true, told_about_tray: true, ..Default::default() };
        settings
            .bindings
            .set(Action::HoldBypass, Chord::parse("ALT+PAUSE").ok());
        settings
            .bindings
            .set(Action::ToggleEnabled, Chord::parse("CTRL+F9").ok());

        store.save(&settings).unwrap();
        let back = store.load();
        assert!(back.notices.is_empty());
        assert_eq!(back.settings, settings);
    }

    #[test]
    fn saving_twice_leaves_no_temporary_file_behind() {
        let dir = scratch("temp");
        let store = Store::new(&dir);
        store.save(&Settings::default()).unwrap();
        store.save(&Settings::default()).unwrap();
        assert!(!dir.join("settings.json.tmp").exists());
        assert!(dir.join("settings.json").exists());
    }

    #[test]
    fn a_corrupt_file_is_kept_rather_than_overwritten() {
        let dir = scratch("corrupt");
        let store = Store::new(&dir);
        fs::write(store.path(), "{ not json").unwrap();

        let loaded = store.load();
        assert_eq!(loaded.settings, Settings::default());
        assert_eq!(loaded.notices.len(), 1);
        assert!(loaded.notices[0].contains("could not be read"));

        let kept: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|n| n.contains("corrupt"))
            .collect();
        assert_eq!(kept.len(), 1, "the unreadable file should still exist");
    }

    #[test]
    fn a_binding_that_no_longer_parses_keeps_the_file_rather_than_dropping_it() {
        let dir = scratch("stale-chord");
        let store = Store::new(&dir);
        // A bare key: something an older build might have allowed, and
        // something this one refuses to register.
        fs::write(
            store.path(),
            r#"{"version":1,"bindings":{"toggleEnabled":"V","cycleNext":null,"cyclePrev":null,"restoreDisplay":null,"holdBypass":null},"autostart":false,"toldAboutTray":false}"#,
        )
        .unwrap();

        let loaded = store.load();
        assert_eq!(loaded.settings, Settings::default());
        assert_eq!(loaded.notices.len(), 1);
        assert!(!dir.join("settings.json").exists());
        let kept = fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .any(|e| e.file_name().to_string_lossy().contains("corrupt"));
        assert!(kept, "the old bindings should still be on disk somewhere");
    }

    #[test]
    fn a_future_schema_version_is_kept_rather_than_guessed_at() {
        let dir = scratch("version");
        let store = Store::new(&dir);
        let settings = Settings { version: SCHEMA_VERSION + 1, ..Default::default() };
        fs::write(store.path(), serde_json::to_string(&settings).unwrap()).unwrap();

        let loaded = store.load();
        assert_eq!(loaded.settings.version, SCHEMA_VERSION);
        assert!(loaded.notices[0].contains("schema version"));
    }
}
