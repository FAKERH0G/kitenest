use std::process::Command;

use crate::{
    distro::Distro,
    error::{Error, Result},
    model::{Package, Source},
    process,
};

pub struct RpmBackend {
    distro: Distro,
}

impl RpmBackend {
    pub fn new(distro: Distro) -> Self {
        Self { distro }
    }

    fn manager(&self) -> Result<&'static str> {
        if which("dnf5") {
            Ok("dnf5")
        } else if which("dnf") {
            Ok("dnf")
        } else {
            Err(Error::BackendUnavailable(
                "neither dnf5 nor dnf is installed".into(),
            ))
        }
    }

    pub fn search(&self, query: &str) -> Result<Vec<Package>> {
        let manager = self.manager()?;
        let args = [
            "repoquery",
            "--available",
            "--queryformat",
            "%{name}\t%{evr}\t%{summary}",
            query,
        ];
        let output = process::text(process::run(manager, &args)?);
        Ok(parse_query_output(&output))
    }

    pub fn list_installed(&self) -> Result<Vec<Package>> {
        let format = "%{NAME}\t%{VERSION}-%{RELEASE}\t%{SUMMARY}\n";
        let output = process::text(process::run("rpm", &["-qa", "--qf", format])?);
        Ok(output
            .lines()
            .filter_map(|line| {
                let mut columns = line.splitn(3, '\t');
                let id = columns.next()?.to_owned();
                let version = columns.next().unwrap_or_default().to_owned();
                let description = columns.next().unwrap_or_default().to_owned();
                Some(Package {
                    id: id.clone(),
                    name: id,
                    version,
                    source: Source::Rpm,
                    description,
                })
            })
            .collect())
    }

    pub fn install(&self, package: &str, privileged: bool) -> Result<String> {
        validate_rpm_id(package)?;
        self.transaction("install", package, privileged)
    }

    pub fn remove(&self, package: &str, privileged: bool) -> Result<String> {
        validate_rpm_id(package)?;
        self.transaction("remove", package, privileged)
    }

    fn transaction(&self, operation: &str, package: &str, privileged: bool) -> Result<String> {
        let mut args = vec![operation.to_owned(), package.to_owned()];
        if self.distro.atomic && !self.distro.supports_rpm() {
            if !which("rpm-ostree") {
                return Err(Error::BackendUnavailable(
                    "rpm-ostree is not installed; use Flatpak or AppImage on this immutable system"
                        .into(),
                ));
            }
            args[0] = if operation == "install" {
                "ostree-install".into()
            } else {
                "ostree-remove".into()
            };
        } else if !self.distro.supports_rpm() {
            return Err(Error::InvalidInput(
                "RPM management is disabled on this immutable system".into(),
            ));
        }
        let result = execute_privileged(&args, privileged)?;
        if self.distro.atomic {
            Ok(format!(
                "{result}Staged with rpm-ostree. Reboot required to activate this change."
            ))
        } else {
            Ok(result)
        }
    }

    pub fn updates(&self) -> Result<Vec<String>> {
        let manager = self.manager()?;
        let output = Command::new(manager)
            .arg("check-upgrade")
            .output()
            .map_err(|source| Error::Io {
                program: manager.into(),
                source,
            })?;
        if !output.status.success() && output.status.code() != Some(100) {
            return Err(Error::Command {
                program: manager.into(),
                status: output.status,
                stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
            });
        }
        Ok(String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .filter(|line| !line.starts_with("Last metadata") && !line.starts_with("Last checked"))
            .map(str::to_owned)
            .collect())
    }

    pub fn upgrade(&self, privileged: bool) -> Result<String> {
        if self.distro.atomic && !self.distro.supports_rpm() {
            return Err(Error::InvalidInput(
                "Use an explicit rpm-ostree deployment on immutable systems; this app does not stage system upgrades."
                    .into(),
            ));
        }
        execute_privileged(&["upgrade".into()], privileged)
    }
}

fn execute_privileged(args: &[String], privileged: bool) -> Result<String> {
    let executable = std::env::current_exe().map_err(|source| Error::Io {
        program: "current executable".into(),
        source,
    })?;
    let helper = executable.with_file_name("kitenest-privileged");
    if !helper.is_file() {
        return Err(Error::BackendUnavailable(format!(
            "privileged helper was not found next to {}",
            executable.display()
        )));
    }
    let output = if privileged && !is_root() {
        let mut helper_args = vec![helper.to_string_lossy().into_owned()];
        helper_args.extend_from_slice(args);
        process::run("pkexec", &helper_args)?
    } else {
        process::run(helper.to_string_lossy().as_ref(), args)?
    };
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn is_root() -> bool {
    libc_geteuid() == 0
}

#[cfg(unix)]
fn libc_geteuid() -> u32 {
    unsafe extern "C" {
        fn geteuid() -> u32;
    }
    unsafe { geteuid() }
}

#[cfg(not(unix))]
fn libc_geteuid() -> u32 {
    1
}

fn which(program: &str) -> bool {
    std::env::var_os("PATH")
        .into_iter()
        .flat_map(|paths| std::env::split_paths(&paths).collect::<Vec<_>>())
        .any(|path| path.join(program).is_file())
}

fn parse_query_output(output: &str) -> Vec<Package> {
    output
        .lines()
        .filter_map(|line| {
            let mut columns = line.splitn(3, '\t');
            let id = columns.next()?.to_owned();
            let version = columns.next().unwrap_or_default().to_owned();
            let description = columns.next().unwrap_or_default().to_owned();
            Some(Package {
                id: id.clone(),
                name: id,
                version,
                source: Source::Rpm,
                description,
            })
        })
        .collect()
}

fn validate_rpm_id(value: &str) -> Result<()> {
    if value.is_empty()
        || value.starts_with('-')
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"+._:-".contains(&byte))
    {
        return Err(Error::InvalidInput(format!(
            "invalid RPM package name '{value}'"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_rpm_query_output() {
        let packages = parse_query_output("nano\t8.1-1.fc44\tSmall text editor\n");
        assert_eq!(packages.len(), 1);
        assert_eq!(packages[0].id, "nano");
        assert_eq!(packages[0].name, "nano");
        assert_eq!(packages[0].version, "8.1-1.fc44");
        assert_eq!(packages[0].description, "Small text editor");
    }

    #[test]
    fn rejects_option_injection_as_package_id() {
        assert!(validate_rpm_id("--assumeyes").is_err());
        assert!(validate_rpm_id("firefox").is_ok());
    }
}
