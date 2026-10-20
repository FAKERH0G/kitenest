pub mod appimage;
pub mod config;
pub mod distro;
pub mod error;
pub mod flatpak;
pub mod fsutil;
pub mod model;
pub mod process;
pub mod rpm;

use crate::{
    appimage::AppImageBackend,
    config::Config,
    distro::Distro,
    error::{Error, Result},
    flatpak::FlatpakBackend,
    model::{Package, Source},
    rpm::RpmBackend,
};

pub struct Manager {
    pub distro: Distro,
    pub config: Config,
    rpm: RpmBackend,
    flatpak: FlatpakBackend,
    appimage: AppImageBackend,
}

impl Manager {
    pub fn detect() -> Result<Self> {
        Self::new(Distro::detect()?, Config::load()?)
    }

    pub fn new(distro: Distro, config: Config) -> Result<Self> {
        Ok(Self {
            rpm: RpmBackend::new(distro.clone()),
            flatpak: FlatpakBackend,
            appimage: AppImageBackend::new()?,
            distro,
            config,
        })
    }

    pub fn search(&self, source: Source, query: &str) -> Result<Vec<Package>> {
        if query.trim().is_empty() {
            return Err(Error::InvalidInput("search term cannot be empty".into()));
        }
        match source {
            Source::Rpm => self.rpm.search(query),
            Source::Flatpak => self.flatpak.search(query),
            Source::AppImage => self.appimage.search(query),
            Source::All => {
                let mut packages = Vec::new();
                for backend in [
                    &self.rpm as &dyn SearchBackend,
                    &self.flatpak,
                    &self.appimage,
                ] {
                    match backend.search(query) {
                        Ok(found) => packages.extend(found),
                        Err(Error::BackendUnavailable(_)) => {}
                        Err(error) => return Err(error),
                    }
                }
                Ok(packages)
            }
        }
    }

    pub fn list_installed(&self, source: Source) -> Result<Vec<Package>> {
        match source {
            Source::Rpm => self.rpm.list_installed(),
            Source::Flatpak => self.flatpak.list_installed(),
            Source::AppImage => self.appimage.list_installed(),
            Source::All => {
                let mut packages = Vec::new();
                for result in [
                    self.rpm.list_installed(),
                    self.flatpak.list_installed(),
                    self.appimage.list_installed(),
                ] {
                    match result {
                        Ok(found) => packages.extend(found),
                        Err(Error::BackendUnavailable(_)) => {}
                        Err(error) => return Err(error),
                    }
                }
                Ok(packages)
            }
        }
    }

    pub fn install(&self, source: Source, package: &str, privileged: bool) -> Result<String> {
        match source {
            Source::Rpm => self.rpm.install(package, privileged),
            Source::Flatpak => self.flatpak.install(package),
            Source::AppImage => self.appimage.install(package),
            Source::All => Err(Error::InvalidInput(
                "choose a package source when installing".into(),
            )),
        }
    }

    pub fn remove(&self, source: Source, package: &str, privileged: bool) -> Result<String> {
        match source {
            Source::Rpm => self.rpm.remove(package, privileged),
            Source::Flatpak => self.flatpak.remove(package),
            Source::AppImage => self.appimage.remove(package),
            Source::All => Err(Error::InvalidInput(
                "choose a package source when removing".into(),
            )),
        }
    }

    pub fn updates(&self, source: Source) -> Result<Vec<String>> {
        let mut updates = Vec::new();
        if source == Source::Rpm && !self.distro.supports_rpm() {
            return Err(Error::InvalidInput(
                "RPM update checks are disabled on this immutable system.".into(),
            ));
        }
        if source.includes(Source::Rpm) && self.distro.supports_rpm() {
            updates.extend(self.rpm.updates()?);
        } else if source == Source::All {
            updates.push("RPM system packages skipped on this immutable system.".into());
        }
        if source.includes(Source::Flatpak)
            && (source != Source::All || !self.config.skip_flatpak_updates)
        {
            updates.extend(self.flatpak.updates()?);
        }
        if source.includes(Source::AppImage)
            && (source != Source::All || !self.config.skip_appimage_updates)
        {
            updates.extend(self.appimage.updates()?);
        }
        Ok(updates)
    }

    pub fn upgrade(&self, source: Source, privileged: bool) -> Result<Vec<String>> {
        let mut results = Vec::new();
        if source == Source::Rpm && !self.distro.supports_rpm() {
            return Err(Error::InvalidInput(
                "RPM management is disabled by default on this immutable system; use Flatpak and AppImage, or explicitly stage a package with rpm-ostree."
                    .into(),
            ));
        }
        if source.includes(Source::Rpm) && self.distro.supports_rpm() {
            if self.distro.id == "nobara" {
                if source == Source::Rpm {
                    return Err(Error::InvalidInput(
                        "Use Nobara's system updater for RPM system updates; KiteNest leaves that workflow untouched."
                            .into(),
                    ));
                }
                results.push(
                    "Skipped system RPM updates on Nobara; use Nobara's system updater.".into(),
                );
            } else {
                results.push(self.rpm.upgrade(privileged)?);
            }
        } else if source == Source::All {
            results.push("Skipped host RPM updates on this immutable system.".into());
        }
        if source.includes(Source::Flatpak)
            && (source != Source::All || !self.config.skip_flatpak_updates)
        {
            results.push(self.flatpak.upgrade()?);
        }
        if source.includes(Source::AppImage)
            && (source != Source::All || !self.config.skip_appimage_updates)
        {
            results.push(self.appimage.upgrade()?);
        }
        Ok(results)
    }

    pub fn set_skip_updates(&mut self, source: Source, skip: bool) -> Result<()> {
        match source {
            Source::Flatpak => self.config.skip_flatpak_updates = skip,
            Source::AppImage => self.config.skip_appimage_updates = skip,
            _ => {
                return Err(Error::InvalidInput(
                    "only flatpak and appimage update checks can be skipped".into(),
                ));
            }
        }
        self.config.save()
    }
}

trait SearchBackend {
    fn search(&self, query: &str) -> Result<Vec<Package>>;
}

impl SearchBackend for RpmBackend {
    fn search(&self, query: &str) -> Result<Vec<Package>> {
        RpmBackend::search(self, query)
    }
}

impl SearchBackend for FlatpakBackend {
    fn search(&self, query: &str) -> Result<Vec<Package>> {
        FlatpakBackend::search(self, query)
    }
}

impl SearchBackend for AppImageBackend {
    fn search(&self, query: &str) -> Result<Vec<Package>> {
        AppImageBackend::search(self, query)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_sources_includes_each_source() {
        assert!(Source::All.includes(Source::Rpm));
        assert!(Source::All.includes(Source::Flatpak));
        assert!(Source::All.includes(Source::AppImage));
        assert!(!Source::Rpm.includes(Source::Flatpak));
    }

    #[test]
    fn nobara_system_upgrade_is_left_to_its_native_updater() {
        let manager = Manager::new(
            Distro {
                id: "nobara".into(),
                name: "Nobara".into(),
                atomic: false,
            },
            Config::default(),
        )
        .unwrap();
        let error = manager.upgrade(Source::Rpm, false).unwrap_err();
        assert!(error.to_string().contains("Nobara's system updater"));
    }
}
