use std::process::{Command, ExitCode};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("kitenest-privileged: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let mut arguments = std::env::args().skip(1);
    let operation = arguments
        .next()
        .ok_or_else(|| "missing operation".to_owned())?;
    let package = arguments.next();
    if arguments.next().is_some() {
        return Err("unexpected extra arguments".into());
    }

    let (program, args) = match operation.as_str() {
        "install" | "remove" => {
            let package = package.ok_or_else(|| "missing package name".to_owned())?;
            validate_package(&package)?;
            let manager = if command_exists("dnf5") {
                "dnf5"
            } else {
                "dnf"
            };
            (
                manager,
                vec![operation.clone(), "--assumeyes".to_owned(), package],
            )
        }
        "ostree-install" | "ostree-remove" => {
            let package = package.ok_or_else(|| "missing package name".to_owned())?;
            validate_package(&package)?;
            let action = if operation == "ostree-install" {
                "install"
            } else {
                "uninstall"
            };
            ("rpm-ostree", vec![action.to_owned(), package])
        }
        "upgrade" => {
            if package.is_some() {
                return Err("upgrade does not accept a package".into());
            }
            let manager = if command_exists("dnf5") {
                "dnf5"
            } else {
                "dnf"
            };
            (
                manager,
                vec!["upgrade".to_owned(), "--assumeyes".to_owned()],
            )
        }
        _ => return Err(format!("unsupported operation '{operation}'")),
    };

    let status = Command::new(program)
        .args(args)
        .status()
        .map_err(|error| format!("could not run {program}: {error}"))?;
    if !status.success() {
        return Err(format!("{program} exited with {status}"));
    }
    Ok(())
}

fn validate_package(package: &str) -> Result<(), String> {
    if package.is_empty()
        || package.starts_with('-')
        || !package
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"+._:-".contains(&byte))
    {
        return Err(format!("invalid package name '{package}'"));
    }
    Ok(())
}

fn command_exists(name: &str) -> bool {
    std::env::var_os("PATH")
        .into_iter()
        .flat_map(|paths| std::env::split_paths(&paths).collect::<Vec<_>>())
        .any(|directory| directory.join(name).is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refuses_option_like_package_names() {
        assert!(validate_package("--assumeyes").is_err());
        assert!(validate_package("firefox").is_ok());
    }
}
