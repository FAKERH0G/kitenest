use std::{fs, path::PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct Config {
    pub skip_flatpak_updates: bool,
    pub skip_appimage_updates: bool,
}

impl Config {
    fn path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from(".config"))
            .join("kitenest")
            .join("config.json")
    }

    pub fn load() -> crate::error::Result<Self> {
        let path = Self::path();
        match fs::read(path) {
            Ok(contents) => Ok(serde_json::from_slice(&contents)?),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(error) => Err(error.into()),
        }
    }

    pub fn save(&self) -> crate::error::Result<()> {
        let path = Self::path();
        fs::create_dir_all(path.parent().expect("config path has a parent"))?;
        crate::fsutil::atomic_write(&path, &serde_json::to_vec_pretty(self)?)
    }
}
