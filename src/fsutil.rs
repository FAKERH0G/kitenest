use std::{io::Write, path::Path};

use tempfile::NamedTempFile;

use crate::error::{Error, Result};

pub fn atomic_write(path: &Path, contents: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| Error::InvalidInput("target path has no parent directory".into()))?;
    let mut temporary = NamedTempFile::new_in(parent)?;
    temporary.write_all(contents)?;
    temporary.as_file().sync_all()?;
    temporary
        .persist(path)
        .map_err(|failure| Error::FileSystem(failure.error))?;
    Ok(())
}
