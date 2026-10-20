use crate::{
    error::{Error, Result},
    model::{Package, Source},
    process,
};

pub struct FlatpakBackend;

impl FlatpakBackend {
    pub fn search(&self, query: &str) -> Result<Vec<Package>> {
        let mut packages = Vec::new();
        for (user, has_remotes) in [
            (false, self.has_remotes(false)?),
            (true, self.has_remotes(true)?),
        ] {
            if !has_remotes {
                continue;
            }
            let mut args = vec!["search"];
            if user {
                args.push("--user");
            }
            args.push("--columns=application,name,description");
            args.push(query);
            let output = process::text(process::run("flatpak", &args)?);
            packages.extend(output.lines().filter_map(|line| parse_row(line, false)));
        }
        packages.sort_by(|left, right| left.id.cmp(&right.id));
        packages.dedup_by(|left, right| left.id == right.id);
        Ok(packages)
    }

    pub fn list_installed(&self) -> Result<Vec<Package>> {
        let mut packages = Vec::new();
        for args in [
            vec!["list", "--app", "--columns=application,name,version"],
            vec![
                "list",
                "--user",
                "--app",
                "--columns=application,name,version",
            ],
        ] {
            let output = process::text(process::run("flatpak", &args)?);
            packages.extend(output.lines().filter_map(|line| parse_row(line, true)));
        }
        packages.sort_by(|left, right| left.id.cmp(&right.id));
        packages.dedup_by(|left, right| left.id == right.id);
        Ok(packages)
    }

    pub fn install(&self, reference: &str) -> Result<String> {
        validate_ref(reference)?;
        let user_available = self.application_available(reference, true)?;
        let system_available = self.application_available(reference, false)?;
        let scope = match (user_available, system_available) {
            (true, _) => "--user",
            (false, true) => "--system",
            (false, false) => {
                return Err(Error::InvalidInput(format!(
                    "Flatpak '{reference}' was not found in configured remotes"
                )));
            }
        };
        let output = process::run(
            "flatpak",
            &[
                "install",
                scope,
                "--assumeyes",
                "--noninteractive",
                reference,
            ],
        )?;
        Ok(process::text(output))
    }

    pub fn remove(&self, reference: &str) -> Result<String> {
        validate_ref(reference)?;
        let mut results = Vec::new();
        for (user, installed) in [
            (false, self.is_installed(reference, false)?),
            (true, self.is_installed(reference, true)?),
        ] {
            if !installed {
                continue;
            }
            let mut args = vec!["uninstall"];
            args.push(if user { "--user" } else { "--system" });
            args.extend(["--assumeyes", "--noninteractive", reference]);
            results.push(process::text(process::run("flatpak", &args)?));
        }
        if results.is_empty() {
            return Err(Error::InvalidInput(format!(
                "Flatpak '{reference}' is not installed"
            )));
        }
        Ok(results.join("\n"))
    }

    pub fn updates(&self) -> Result<Vec<String>> {
        let mut updates = Vec::new();
        for (user, has_remotes) in [
            (false, self.has_remotes(false)?),
            (true, self.has_remotes(true)?),
        ] {
            if !has_remotes {
                continue;
            }
            let mut args = vec!["remote-ls"];
            args.push(if user { "--user" } else { "--system" });
            args.extend(["--updates", "--columns=application,name"]);
            let output = process::text(process::run("flatpak", &args)?);
            updates.extend(
                output
                    .lines()
                    .map(str::trim)
                    .filter(|line| !line.is_empty())
                    .map(str::to_owned),
            );
        }
        updates.sort();
        updates.dedup();
        Ok(updates)
    }

    pub fn upgrade(&self) -> Result<String> {
        let mut outputs = Vec::new();
        for (user, has_remotes) in [
            (false, self.has_remotes(false)?),
            (true, self.has_remotes(true)?),
        ] {
            if !has_remotes {
                continue;
            }
            let output = process::run(
                "flatpak",
                &[
                    "update",
                    if user { "--user" } else { "--system" },
                    "--assumeyes",
                    "--noninteractive",
                ],
            )?;
            outputs.push(process::text(output));
        }
        if outputs.is_empty() {
            Ok("No configured Flatpak remotes.".into())
        } else {
            Ok(outputs.join("\n"))
        }
    }

    fn is_installed(&self, reference: &str, user: bool) -> Result<bool> {
        let mut args = vec!["list"];
        args.push(if user { "--user" } else { "--system" });
        args.extend(["--app", "--columns=application"]);
        let output = process::text(process::run("flatpak", &args)?);
        Ok(output.lines().any(|line| line.trim() == reference))
    }

    fn has_remotes(&self, user: bool) -> Result<bool> {
        let mut args = vec!["remotes"];
        args.push(if user { "--user" } else { "--system" });
        args.push("--columns=name");
        let output = process::text(process::run("flatpak", &args)?);
        Ok(output.lines().any(|line| !line.trim().is_empty()))
    }

    fn application_available(&self, reference: &str, user: bool) -> Result<bool> {
        if !self.has_remotes(user)? {
            return Ok(false);
        }
        let mut args = vec!["remote-ls"];
        args.push(if user { "--user" } else { "--system" });
        args.extend(["--app", "--columns=application"]);
        let output = process::text(process::run("flatpak", &args)?);
        Ok(output.lines().any(|line| line.trim() == reference))
    }
}

fn parse_row(line: &str, installed: bool) -> Option<Package> {
    let columns: Vec<_> = line.split('\t').collect();
    if columns.len() < 2 || columns[0].trim().is_empty() {
        return None;
    }
    let id = columns[0].trim().to_owned();
    if !id.contains('.') {
        return None;
    }
    Some(Package {
        id: id.clone(),
        name: columns[1].trim().to_owned(),
        version: if installed {
            columns
                .get(2)
                .copied()
                .unwrap_or_default()
                .trim()
                .to_owned()
        } else {
            String::new()
        },
        source: Source::Flatpak,
        description: if installed {
            String::new()
        } else {
            columns
                .get(2)
                .copied()
                .unwrap_or_default()
                .trim()
                .to_owned()
        },
    })
}

fn validate_ref(reference: &str) -> Result<()> {
    if reference.is_empty()
        || reference.starts_with('-')
        || !reference
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"+._:-".contains(&byte))
    {
        return Err(Error::InvalidInput(format!(
            "invalid Flatpak application reference '{reference}'"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_flatpak_search_row() {
        let package = parse_row("org.mozilla.firefox\tFirefox\tWeb Browser", false).unwrap();
        assert_eq!(package.id, "org.mozilla.firefox");
        assert_eq!(package.name, "Firefox");
        assert_eq!(package.description, "Web Browser");
    }

    #[test]
    fn parses_flatpak_installed_row() {
        let package = parse_row("org.mozilla.firefox\tFirefox\t150.0", true).unwrap();
        assert_eq!(package.id, "org.mozilla.firefox");
        assert_eq!(package.name, "Firefox");
        assert_eq!(package.version, "150.0");
    }

    #[test]
    fn validates_flatpak_reference_without_shell_interpretation() {
        assert!(validate_ref("org.mozilla.firefox").is_ok());
        assert!(validate_ref("org.example.App;touch /tmp/oops").is_err());
    }
}
