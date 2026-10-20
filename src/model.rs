use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    Rpm,
    Flatpak,
    #[value(name = "appimage", alias = "app-image")]
    AppImage,
    All,
}

impl Source {
    pub fn includes(self, source: Self) -> bool {
        self == Self::All || self == source
    }
}

impl fmt::Display for Source {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{}",
            match self {
                Self::Rpm => "rpm",
                Self::Flatpak => "flatpak",
                Self::AppImage => "appimage",
                Self::All => "all",
            }
        )
    }
}

impl FromStr for Source {
    type Err = Error;

    fn from_str(value: &str) -> Result<Self> {
        match value.to_ascii_lowercase().as_str() {
            "rpm" => Ok(Self::Rpm),
            "flatpak" => Ok(Self::Flatpak),
            "appimage" | "app-image" => Ok(Self::AppImage),
            "all" => Ok(Self::All),
            _ => Err(Error::InvalidInput(format!(
                "unknown source '{value}'; choose rpm, flatpak, appimage, or all"
            ))),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Package {
    pub id: String,
    pub name: String,
    pub version: String,
    pub source: Source,
    pub description: String,
}
