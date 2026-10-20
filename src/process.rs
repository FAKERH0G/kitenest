use std::{
    ffi::OsStr,
    process::{Command, Output},
};

use crate::error::{Error, Result};

pub fn run(program: &str, args: &[impl AsRef<OsStr>]) -> Result<Output> {
    let output = Command::new(program)
        .args(args)
        .output()
        .map_err(|source| {
            if source.kind() == std::io::ErrorKind::NotFound {
                Error::BackendUnavailable(program.into())
            } else {
                Error::Io {
                    program: program.into(),
                    source,
                }
            }
        })?;
    if !output.status.success() {
        return Err(Error::Command {
            program: program.into(),
            status: output.status,
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        });
    }
    Ok(output)
}

pub fn text(output: Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}
