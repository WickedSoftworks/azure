//! Where presets are kept, and the marker that says the display was left
//! changed.
//!
//! The directory arrives as an argument rather than being discovered here,
//! so the tests write to a temporary one and the Tauri layer supplies the
//! real config path.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::preset::{PresetSet, SCHEMA_VERSION};

const PRESETS: &str = "presets.json";
/// Present means display state was changed and not put back. An empty
/// file: its existence is the whole payload, so there is nothing to parse
/// and nothing to get wrong while a display is sitting in a bad state.
const DIRTY: &str = "display-dirty";

/// A load always succeeds. What it could not do, it reports.
pub struct Loaded {
    pub set: PresetSet,
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
        self.dir.join(PRESETS)
    }

    /// Reads the stored presets.
    ///
    /// Never fails and never silently discards: a file that will not parse
    /// is moved aside rather than overwritten, because someone's tuning is
    /// in it and the next save would be the thing that destroyed it.
    pub fn load(&self) -> Loaded {
        let path = self.path();
        let raw = match fs::read_to_string(&path) {
            Ok(raw) => raw,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                return Loaded { set: PresetSet::fresh(), notices: Vec::new() }
            }
            Err(e) => {
                return Loaded {
                    set: PresetSet::fresh(),
                    notices: vec![format!("could not read {}: {e}", path.display())],
                }
            }
        };

        match serde_json::from_str::<PresetSet>(&raw) {
            Ok(set) if set.version == SCHEMA_VERSION => Loaded { set, notices: Vec::new() },
            Ok(set) => {
                let kept = self.quarantine(&path);
                Loaded {
                    set: PresetSet::fresh(),
                    notices: vec![format!(
                        "presets.json is schema version {} and this build reads {}; kept the old file at {}",
                        set.version,
                        SCHEMA_VERSION,
                        kept.unwrap_or_else(|| "(could not be moved aside)".into()),
                    )],
                }
            }
            Err(e) => {
                let kept = self.quarantine(&path);
                Loaded {
                    set: PresetSet::fresh(),
                    notices: vec![format!(
                        "presets.json could not be read ({e}); kept it at {} and started fresh",
                        kept.unwrap_or_else(|| "(could not be moved aside)".into()),
                    )],
                }
            }
        }
    }

    /// Writes to a temporary file and renames over the target, so an
    /// interrupted save leaves the previous presets intact rather than a
    /// half-written file.
    pub fn save(&self, set: &PresetSet) -> io::Result<()> {
        fs::create_dir_all(&self.dir)?;
        let target = self.path();
        let temp = self.dir.join(format!("{PRESETS}.tmp"));
        fs::write(&temp, serde_json::to_string_pretty(set)? + "\n")?;
        fs::rename(&temp, &target)
    }

    fn quarantine(&self, path: &Path) -> Option<String> {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let kept = self.dir.join(format!("{PRESETS}.corrupt-{stamp}"));
        fs::rename(path, &kept).ok()?;
        Some(kept.display().to_string())
    }

    // ── the dirty marker ────────────────────────────────────────────────

    /// True when a previous run changed the display and never put it back.
    pub fn was_dirty(&self) -> bool {
        self.dir.join(DIRTY).exists()
    }

    /// Called once when display state first diverges, not per slider tick.
    pub fn mark_dirty(&self) -> io::Result<()> {
        fs::create_dir_all(&self.dir)?;
        fs::write(self.dir.join(DIRTY), b"")
    }

    pub fn clear_dirty(&self) -> io::Result<()> {
        match fs::remove_file(self.dir.join(DIRTY)) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            other => other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use azure_color::ChannelId;

    /// A directory of our own under the system temp dir. Avoids a
    /// dependency for something this small.
    fn scratch(tag: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .subsec_nanos();
        let dir = std::env::temp_dir().join(format!("azure-presets-{tag}-{nanos}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_missing_file_loads_a_fresh_set_without_complaint() {
        let store = Store::new(scratch("missing"));
        let loaded = store.load();
        assert_eq!(loaded.set.presets.len(), 1);
        assert!(loaded.notices.is_empty());
    }

    #[test]
    fn presets_survive_a_round_trip() {
        let store = Store::new(scratch("roundtrip"));
        let mut set = PresetSet::fresh();
        let id = set.add("CS2".into(), Some("D:\\games\\cs2.exe".into()));
        set.set_channel(&id, ChannelId::Vibrance, 178).unwrap();
        set.select(&id).unwrap();

        store.save(&set).unwrap();
        let back = store.load();
        assert!(back.notices.is_empty());
        assert_eq!(back.set, set);
        assert_eq!(back.set.active_id, id);
        assert_eq!(back.set.get(&id).unwrap().state.vibrance, 178);
    }

    #[test]
    fn saving_twice_leaves_no_temporary_file_behind() {
        let dir = scratch("temp");
        let store = Store::new(&dir);
        store.save(&PresetSet::fresh()).unwrap();
        store.save(&PresetSet::fresh()).unwrap();
        assert!(!dir.join("presets.json.tmp").exists());
        assert!(dir.join("presets.json").exists());
    }

    #[test]
    fn a_corrupt_file_is_kept_rather_than_overwritten() {
        let dir = scratch("corrupt");
        let store = Store::new(&dir);
        fs::write(store.path(), "{ this is not json").unwrap();

        let loaded = store.load();
        assert_eq!(loaded.set, PresetSet::fresh());
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
    fn a_future_schema_version_is_kept_rather_than_guessed_at() {
        let dir = scratch("version");
        let store = Store::new(&dir);
        let mut set = PresetSet::fresh();
        set.version = SCHEMA_VERSION + 1;
        fs::write(store.path(), serde_json::to_string(&set).unwrap()).unwrap();

        let loaded = store.load();
        assert_eq!(loaded.set.version, SCHEMA_VERSION);
        assert!(loaded.notices[0].contains("schema version"));
    }

    #[test]
    fn the_dirty_marker_records_that_the_display_was_left_changed() {
        let store = Store::new(scratch("dirty"));
        assert!(!store.was_dirty());

        store.mark_dirty().unwrap();
        assert!(store.was_dirty(), "a run that died here would be detected");

        store.clear_dirty().unwrap();
        assert!(!store.was_dirty());
    }

    #[test]
    fn clearing_a_marker_that_is_not_there_is_not_an_error() {
        let store = Store::new(scratch("clean"));
        store.clear_dirty().unwrap();
    }
}
