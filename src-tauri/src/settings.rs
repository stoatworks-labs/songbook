//! `settings.json` in the app's config directory: where the library is,
//! who is saving, which desks are on the network, and how to sync.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use songbook_library::oauth::Tokens;
use songbook_model::Platform;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DeviceEntry {
    pub name: String,
    pub platform: Platform,
    pub host: String,
    /// The desk's MIDI channel (Allen & Heath), 1–16.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub midi_channel: Option<u8>,
    /// Desk model, for the shape of a pulled show.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct SyncSettings {
    /// `none`, `folder`, `dropbox`, `google`, `onedrive`.
    #[serde(default)]
    pub provider: String,
    /// Folder path, or the remote folder name.
    #[serde(default)]
    pub root: String,
    #[serde(default)]
    pub client_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_secret: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tokens: Option<Tokens>,
    #[serde(default)]
    pub last_run: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub library_path: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub devices: Vec<DeviceEntry>,
    #[serde(default)]
    pub sync: SyncSettings,
    /// Which platform/model Companion exports default to, per show id.
    #[serde(default)]
    pub companion_host: String,
}

impl Settings {
    pub fn load(config_dir: &Path, default_library: &Path) -> Settings {
        let file = config_dir.join("settings.json");
        if let Ok(text) = std::fs::read_to_string(&file) {
            if let Ok(s) = serde_json::from_str::<Settings>(&text) {
                return s;
            }
        }
        Settings {
            library_path: default_library.display().to_string(),
            author: String::new(),
            devices: vec![],
            sync: SyncSettings::default(),
            companion_host: String::new(),
        }
    }

    pub fn save(&self, config_dir: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(config_dir)?;
        std::fs::write(config_dir.join("settings.json"), serde_json::to_string_pretty(self).unwrap_or_default())
    }

    pub fn library(&self) -> PathBuf {
        PathBuf::from(&self.library_path)
    }
}
