use std::{collections::HashMap, fs, path::Path};

use crate::error::{Error, Result};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Distro {
    pub id: String,
    pub name: String,
    pub atomic: bool,
}

impl Distro {
    pub fn detect() -> Result<Self> {
        let contents = fs::read_to_string("/etc/os-release").map_err(|error| Error::Io {
            program: "/etc/os-release".into(),
            source: error,
        })?;
        Self::parse(&contents, Path::new("/run/ostree-booted").exists())
    }

    pub fn parse(contents: &str, ostree_booted: bool) -> Result<Self> {
        let fields: HashMap<_, _> = contents
            .lines()
            .filter_map(|line| line.split_once('='))
            .map(|(key, value)| {
                (
                    key.trim(),
                    value.trim().trim_matches('"').trim_matches('\'').to_owned(),
                )
            })
            .collect();
        let id = fields
            .get("ID")
            .ok_or_else(|| Error::InvalidInput("os-release has no ID".into()))?
            .to_ascii_lowercase();
        let name = fields
            .get("PRETTY_NAME")
            .cloned()
            .unwrap_or_else(|| id.clone());
        let variant = fields
            .get("VARIANT_ID")
            .map(|value| value.to_ascii_lowercase())
            .unwrap_or_default();
        let atomic = ostree_booted
            || ["bazzite", "kinoite", "silverblue", "sericea", "aurora"].contains(&id.as_str())
            || ["atomic", "kinoite", "silverblue", "sericea", "bazzite"]
                .contains(&variant.as_str());
        Ok(Self { id, name, atomic })
    }

    pub fn supports_rpm(&self) -> bool {
        !self.atomic
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_fedora_workstation() {
        let distro = Distro::parse("ID=fedora\nPRETTY_NAME=\"Fedora Linux\"", false).unwrap();
        assert_eq!(distro.id, "fedora");
        assert!(!distro.atomic);
        assert!(distro.supports_rpm());
    }

    #[test]
    fn detects_bazzite_as_immutable() {
        let distro = Distro::parse("ID=bazzite\nPRETTY_NAME=\"Bazzite\"", false).unwrap();
        assert!(distro.atomic);
        assert!(!distro.supports_rpm());
    }

    #[test]
    fn detects_ostree_booted_fedora() {
        assert!(Distro::parse("ID=fedora", true).unwrap().atomic);
    }
}
