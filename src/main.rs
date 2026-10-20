use std::{
    io::{self, Write},
    process::ExitCode,
    str::FromStr,
};

use clap::{Parser, Subcommand};
use kitenest::{
    Manager,
    error::{Error, Result},
    model::Source,
};

#[derive(Debug, Parser)]
#[command(
    name = "kitenest",
    version,
    about = "Manage RPMs, Flatpaks, and AppImages in one place"
)]
struct Cli {
    #[arg(
        short,
        long,
        global = true,
        help = "Run package actions without an extra confirmation"
    )]
    yes: bool,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    Search {
        query: String,
        #[arg(short, long, value_enum, default_value = "all")]
        source: Source,
    },
    List {
        #[arg(short, long, value_enum, default_value = "all")]
        source: Source,
    },
    Install {
        package: String,
        #[arg(short, long, value_enum)]
        source: Source,
    },
    Remove {
        package: String,
        #[arg(short, long, value_enum)]
        source: Source,
    },
    Updates {
        #[arg(value_enum, default_value = "all")]
        source: Source,
    },
    Upgrade {
        #[arg(value_enum, default_value = "all")]
        source: Source,
    },
    Config {
        #[command(subcommand)]
        command: ConfigCommands,
    },
    Distro,
}

#[derive(Debug, Subcommand)]
enum ConfigCommands {
    SkipUpdates {
        #[arg(value_parser = parse_skippable_source)]
        source: Source,
        enabled: bool,
    },
}

fn parse_skippable_source(value: &str) -> std::result::Result<Source, String> {
    let source = Source::from_str(value).map_err(|error| error.to_string())?;
    if matches!(source, Source::Flatpak | Source::AppImage) {
        Ok(source)
    } else {
        Err("source must be flatpak or appimage".into())
    }
}

fn main() -> ExitCode {
    match execute(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(Error::BrokenPipe) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("kitenest: {error}");
            ExitCode::FAILURE
        }
    }
}

fn execute(cli: Cli) -> Result<()> {
    let mut manager = Manager::detect()?;
    match cli.command {
        Commands::Search { query, source } => {
            let packages = manager.search(source, &query)?;
            if packages.is_empty() {
                write_line("No matching packages found.")?;
            } else {
                for package in packages {
                    write_line(&format!(
                        "{}\t{}\t{}\t{}",
                        package.source, package.id, package.version, package.name
                    ))?;
                }
            }
        }
        Commands::List { source } => {
            for package in manager.list_installed(source)? {
                write_line(&format!(
                    "{}\t{}\t{}\t{}",
                    package.source, package.id, package.version, package.name
                ))?;
            }
        }
        Commands::Install { package, source } => {
            let warning = if source == Source::Rpm && manager.distro.atomic {
                " This stages a system deployment; a reboot is required.".to_owned()
            } else {
                String::new()
            };
            confirm(
                &format!("Install {package} from {source}?{warning}"),
                cli.yes,
            )?;
            write_line(&manager.install(source, &package, true)?)?;
        }
        Commands::Remove { package, source } => {
            let warning = if source == Source::Rpm && manager.distro.atomic {
                " This stages a system deployment; a reboot is required.".to_owned()
            } else {
                String::new()
            };
            confirm(
                &format!("Remove {package} from {source}?{warning}"),
                cli.yes,
            )?;
            write_line(&manager.remove(source, &package, true)?)?;
        }
        Commands::Updates { source } => {
            let updates = manager.updates(source)?;
            if updates.is_empty() {
                write_line("No updates available.")?;
            } else {
                for update in updates {
                    write_line(&update)?;
                }
            }
        }
        Commands::Upgrade { source } => {
            confirm(&format!("Apply available {source} updates?"), cli.yes)?;
            for result in manager.upgrade(source, true)? {
                write_line(&result)?;
            }
        }
        Commands::Config { command } => match command {
            ConfigCommands::SkipUpdates { source, enabled } => {
                manager.set_skip_updates(source, enabled)?;
                write_line(&format!(
                    "{} update checks are now {}.",
                    source,
                    if enabled { "skipped" } else { "enabled" }
                ))?;
            }
        },
        Commands::Distro => {
            write_line(&format!(
                "{} ({})",
                manager.distro.name,
                if manager.distro.atomic {
                    "immutable; use Flatpak/AppImage as primary sources"
                } else {
                    "RPM package management enabled"
                }
            ))?;
        }
    }
    Ok(())
}

fn confirm(message: &str, assume_yes: bool) -> Result<()> {
    if assume_yes {
        return Ok(());
    }
    let mut stdout = io::stdout().lock();
    write!(stdout, "{message} [y/N] ").map_err(output_error)?;
    stdout.flush().map_err(output_error)?;
    drop(stdout);
    let mut response = String::new();
    io::stdin()
        .read_line(&mut response)
        .map_err(Error::FileSystem)?;
    if matches!(response.trim().to_ascii_lowercase().as_str(), "y" | "yes") {
        Ok(())
    } else {
        Err(Error::InvalidInput("operation cancelled".into()))
    }
}

fn write_line(message: &str) -> Result<()> {
    writeln!(io::stdout().lock(), "{message}").map_err(output_error)
}

fn output_error(error: std::io::Error) -> Error {
    if error.kind() == std::io::ErrorKind::BrokenPipe {
        Error::BrokenPipe
    } else {
        Error::FileSystem(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn update_commands_accept_source_as_a_positional_argument() {
        let updates = Cli::try_parse_from(["kitenest", "updates", "all"]).unwrap();
        assert!(matches!(
            updates.command,
            Commands::Updates {
                source: Source::All
            }
        ));

        let upgrade = Cli::try_parse_from(["kitenest", "upgrade", "flatpak"]).unwrap();
        assert!(matches!(
            upgrade.command,
            Commands::Upgrade {
                source: Source::Flatpak
            }
        ));
    }
}
