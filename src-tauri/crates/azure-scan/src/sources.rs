//! What each launcher's own bookkeeping says, parsed out of it.
//!
//! Every function here takes the text of a file, or values already read
//! from the registry, and returns what it means. Nothing opens anything.
//! That is what makes six launchers testable on a machine that has two.

use serde::Deserialize;

/// A title from Steam's `appmanifest_*.acf`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SteamApp {
    pub name: String,
    /// Directory name under `steamapps/common`, not a full path.
    pub installdir: String,
}

pub mod steam {
    use super::SteamApp;
    use crate::vdf;

    /// Steam ships tools through the same manifests as games.
    ///
    /// Matched by name rather than by app id: an id list would need
    /// maintaining as Valve adds runtimes, and these names do not change.
    const NOT_A_GAME: &[&str] = &[
        "steamworks common",
        "steam linux runtime",
        "proton",
        "steamvr",
        "steam controller",
        "steamworks shared",
    ];

    /// Every library folder in `libraryfolders.vdf`.
    ///
    /// The Steam install itself is one of them, so a player with no extra
    /// libraries still gets one path back.
    pub fn libraries(text: &str) -> Vec<String> {
        let parsed = vdf::parse(text);
        let Some(folders) = parsed.block("libraryfolders") else {
            return Vec::new();
        };
        folders
            .children()
            .filter_map(|b| b.get("path"))
            .map(str::to_string)
            .collect()
    }

    /// The title an `appmanifest` describes, or `None` when it describes
    /// a runtime rather than a game.
    pub fn app(text: &str) -> Option<SteamApp> {
        let parsed = vdf::parse(text);
        let state = parsed.block("AppState")?;
        let name = state.get("name")?.trim().to_string();
        let installdir = state.get("installdir")?.trim().to_string();

        if name.is_empty() || installdir.is_empty() || is_tool(&name) {
            return None;
        }
        Some(SteamApp { name, installdir })
    }

    pub fn is_tool(name: &str) -> bool {
        let lower = name.to_lowercase();
        NOT_A_GAME.iter().any(|bad| lower.contains(bad))
    }
}

/// A title from one of Epic's `.item` manifests.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EpicGame {
    pub name: String,
    pub install_location: String,
    /// Relative to `install_location`, forward-slashed as Epic writes it.
    pub launch_executable: String,
}

pub mod epic {
    use super::EpicGame;
    use serde::Deserialize;

    #[derive(Deserialize)]
    #[serde(rename_all = "PascalCase")]
    struct Item {
        display_name: Option<String>,
        install_location: Option<String>,
        launch_executable: Option<String>,
        #[serde(rename = "bIsIncompleteInstall")]
        incomplete: Option<bool>,
    }

    /// Epic is the only launcher that names the executable, so nothing
    /// here has to be guessed at.
    ///
    /// A manifest for a partly-downloaded game is skipped: binding a
    /// preset to a binary that is not on disk yet produces a preset that
    /// never fires.
    pub fn manifest(json: &str) -> Option<EpicGame> {
        let item: Item = serde_json::from_str(json).ok()?;
        if item.incomplete.unwrap_or(false) {
            return None;
        }

        let name = item.display_name?.trim().to_string();
        let install_location = item.install_location?.trim().to_string();
        let launch_executable = item.launch_executable?.trim().to_string();
        if name.is_empty() || install_location.is_empty() || launch_executable.is_empty() {
            return None;
        }

        Some(EpicGame { name, install_location, launch_executable })
    }
}

pub mod battlenet {
    use serde::Deserialize;
    use std::collections::BTreeMap;

    #[derive(Deserialize)]
    struct Config {
        #[serde(rename = "Games")]
        games: Option<BTreeMap<String, Game>>,
    }

    #[derive(Deserialize)]
    struct Game {
        #[serde(rename = "LastActiveInstallPath")]
        last_active: Option<String>,
        #[serde(rename = "InstallPath")]
        install: Option<String>,
    }

    /// Install directories from `Battle.net.config`.
    ///
    /// The keys are Blizzard's internal code names — `prometheus` is
    /// Overwatch — so the folder's own name is used instead, which is
    /// what the player would recognise.
    pub fn installs(json: &str) -> Vec<String> {
        let Ok(config) = serde_json::from_str::<Config>(json) else {
            return Vec::new();
        };
        let Some(games) = config.games else {
            return Vec::new();
        };

        games
            .into_values()
            .filter_map(|g| g.last_active.or(g.install))
            .map(|p| p.trim().to_string())
            .filter(|p| !p.is_empty())
            .collect()
    }
}

/// `G:/Ubisoft/Tom Clancy's Rainbow Six Siege/` becomes
/// `Tom Clancy's Rainbow Six Siege`.
///
/// Used wherever a launcher gives a directory and no title. The folder is
/// what the player named or what the installer named, and either way it
/// reads better than an internal id.
pub fn name_from_directory(path: &str) -> String {
    path.trim_end_matches(['/', '\\'])
        .rsplit(['/', '\\'])
        .find(|part| !part.is_empty())
        .unwrap_or("")
        .to_string()
}

/// An installed title as a registry-backed launcher describes it.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Installed {
    pub name: String,
    pub directory: String,
    /// Some launchers record the executable too. GOG does.
    pub exe: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steam_libraries_come_out_of_the_real_file_shape() {
        let text = r#"
"libraryfolders"
{
	"0"
	{
		"path"		"C:\\Program Files (x86)\\Steam"
	}
	"1"
	{
		"path"		"G:\\SteamLibrary"
	}
}
"#;
        assert_eq!(
            steam::libraries(text),
            vec![r"C:\Program Files (x86)\Steam", r"G:\SteamLibrary"]
        );
    }

    #[test]
    fn a_missing_libraryfolders_block_yields_no_libraries() {
        assert!(steam::libraries("").is_empty());
        assert!(steam::libraries(r#""something else" { "path" "C:\\x" }"#).is_empty());
    }

    #[test]
    fn steam_redistributables_are_not_a_game() {
        // This one is really in the Steam library on the machine this was
        // written on, and it has an appmanifest exactly like a game's.
        let acf = r#"
"AppState"
{
	"appid"		"228980"
	"name"		"Steamworks Common Redistributables"
	"installdir"		"Steamworks Shared"
}
"#;
        assert_eq!(steam::app(acf), None);
    }

    #[test]
    fn steam_runtimes_and_tools_are_not_games_either() {
        for name in [
            "Proton 9.0",
            "Steam Linux Runtime 3.0 (sniper)",
            "SteamVR",
            "Steam Controller Configs",
        ] {
            assert!(steam::is_tool(name), "{name} should be filtered");
        }
        assert!(!steam::is_tool("Counter-Strike 2"));
        assert!(!steam::is_tool("Apex Legends"));
    }

    #[test]
    fn a_steam_game_yields_its_name_and_install_directory() {
        let acf = r#"
"AppState"
{
	"appid"		"730"
	"name"		"Counter-Strike 2"
	"installdir"		"Counter-Strike Global Offensive"
}
"#;
        assert_eq!(
            steam::app(acf),
            Some(SteamApp {
                name: "Counter-Strike 2".into(),
                installdir: "Counter-Strike Global Offensive".into(),
            })
        );
    }

    #[test]
    fn an_appmanifest_missing_what_matters_is_skipped() {
        assert_eq!(steam::app(r#""AppState" { "appid" "1" }"#), None);
        assert_eq!(steam::app(""), None);
    }

    /// The shape of a real `.item`, forward slashes and all.
    #[test]
    fn epic_names_the_executable_itself() {
        let json = r#"{
            "DisplayName": "Fortnite",
            "InstallLocation": "G:\\Epic Games\\Fortnite",
            "LaunchExecutable": "FortniteGame/Binaries/Win64/FortniteClient-Win64-Shipping.exe",
            "AppName": "Fortnite",
            "bIsIncompleteInstall": false
        }"#;
        assert_eq!(
            epic::manifest(json),
            Some(EpicGame {
                name: "Fortnite".into(),
                install_location: r"G:\Epic Games\Fortnite".into(),
                launch_executable:
                    "FortniteGame/Binaries/Win64/FortniteClient-Win64-Shipping.exe".into(),
            })
        );
    }

    #[test]
    fn a_half_downloaded_epic_game_is_not_offered() {
        let json = r#"{
            "DisplayName": "Something",
            "InstallLocation": "G:\\Epic Games\\Something",
            "LaunchExecutable": "Something.exe",
            "bIsIncompleteInstall": true
        }"#;
        assert_eq!(epic::manifest(json), None);
    }

    #[test]
    fn a_manifest_that_is_not_json_is_skipped_rather_than_fatal() {
        assert_eq!(epic::manifest("{ not json"), None);
        assert_eq!(epic::manifest("{}"), None);
    }

    #[test]
    fn battle_net_installs_come_out_of_the_games_map() {
        let json = r#"{
            "Games": {
                "prometheus": { "LastActiveInstallPath": "C:\\Program Files (x86)\\Overwatch" },
                "fenris": { "InstallPath": "D:\\Games\\Diablo IV" },
                "battle_net": {}
            }
        }"#;
        let mut installs = battlenet::installs(json);
        installs.sort();
        assert_eq!(
            installs,
            vec![r"C:\Program Files (x86)\Overwatch", r"D:\Games\Diablo IV"]
        );
    }

    #[test]
    fn a_battle_net_config_with_no_games_is_not_an_error() {
        assert!(battlenet::installs("{}").is_empty());
        assert!(battlenet::installs("not json").is_empty());
    }

    #[test]
    fn a_folder_name_stands_in_for_a_title_the_launcher_did_not_give() {
        assert_eq!(
            name_from_directory("G:/Ubisoft/Tom Clancy's Rainbow Six Siege/"),
            "Tom Clancy's Rainbow Six Siege"
        );
        assert_eq!(name_from_directory(r"C:\Games\Overwatch"), "Overwatch");
        assert_eq!(name_from_directory(""), "");
    }
}
