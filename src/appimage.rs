use std::{
    fs::{self, File},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    time::Duration,
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tempfile::NamedTempFile;

use crate::{
    error::{Error, Result},
    model::{Package, Source},
    process,
};

const GENERIC_ICON: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="128" height="128" viewBox="0 0 128 128"><rect x="8" y="8" width="112" height="112" rx="28" fill="#3584e4"/><path d="M64 26 96 44v40L64 102 32 84V44z" fill="#fff"/><circle cx="64" cy="64" r="13" fill="#3584e4"/></svg>"##;

#[derive(Clone, Debug, Deserialize, Serialize)]
struct Record {
    id: String,
    name: String,
    path: PathBuf,
    sha256: String,
    update_info: Option<String>,
}

pub struct AppImageBackend {
    applications: PathBuf,
    desktop_entries: PathBuf,
    icons: PathBuf,
    registry: PathBuf,
}

impl AppImageBackend {
    pub fn new() -> Result<Self> {
        let home = dirs::home_dir()
            .ok_or_else(|| Error::InvalidInput("could not determine the home directory".into()))?;
        let data = dirs::data_local_dir().unwrap_or_else(|| home.join(".local/share"));
        let applications = home.join("Applications").join("KiteNest");
        let desktop_entries = data.join("applications");
        let icons = data.join("icons/hicolor/128x128/apps");
        let registry = data.join("kitenest/appimages.json");
        Ok(Self {
            applications,
            desktop_entries,
            icons,
            registry,
        })
    }

    #[cfg(test)]
    fn with_paths(root: &Path) -> Self {
        Self {
            applications: root.join("Applications/KiteNest"),
            desktop_entries: root.join("applications"),
            icons: root.join("icons"),
            registry: root.join("registry.json"),
        }
    }

    pub fn search(&self, query: &str) -> Result<Vec<Package>> {
        let needle = query.to_ascii_lowercase();
        Ok(self
            .records()?
            .into_iter()
            .filter(|record| {
                record.name.to_ascii_lowercase().contains(&needle)
                    || record.id.to_ascii_lowercase().contains(&needle)
            })
            .map(record_to_package)
            .collect())
    }

    pub fn list_installed(&self) -> Result<Vec<Package>> {
        Ok(self.records()?.into_iter().map(record_to_package).collect())
    }

    pub fn install(&self, source: &str) -> Result<String> {
        let temporary_download = if source.starts_with("https://") {
            Some(TemporaryDownload::new(self.download(source)?))
        } else if source.starts_with("http://") {
            return Err(Error::InvalidInput(
                "AppImage downloads must use HTTPS".into(),
            ));
        } else {
            None
        };
        let source_path = temporary_download
            .as_ref()
            .map(|temporary| temporary.0.clone())
            .unwrap_or_else(|| PathBuf::from(source));
        let name = if source.starts_with("https://") {
            source
                .split('/')
                .next_back()
                .unwrap_or_default()
                .split('?')
                .next()
                .unwrap_or_default()
                .strip_suffix(".AppImage")
                .or_else(|| {
                    source
                        .split('/')
                        .next_back()
                        .unwrap_or_default()
                        .split('?')
                        .next()
                })
                .unwrap_or_default()
                .to_owned()
        } else {
            source_path
                .file_stem()
                .and_then(|value| value.to_str())
                .ok_or_else(|| Error::InvalidInput("AppImage filename is not valid UTF-8".into()))?
                .to_owned()
        };
        let id = safe_id(&name);
        if id.is_empty() {
            return Err(Error::InvalidInput(
                "AppImage filename has no usable characters".into(),
            ));
        }
        fs::create_dir_all(&self.applications)?;
        fs::create_dir_all(&self.desktop_entries)?;
        fs::create_dir_all(&self.icons)?;
        let target = self.applications.join(format!("{id}.AppImage"));
        let mut staged = NamedTempFile::new_in(&self.applications)?;
        let mut source_file = File::open(&source_path)?;
        std::io::copy(&mut source_file, staged.as_file_mut())?;
        staged.flush()?;
        validate_appimage(staged.path())?;
        set_executable(staged.path())?;
        let update_info = read_update_info(staged.path())?;
        let sha256 = digest_file(staged.path())?;
        let record = Record {
            id: id.clone(),
            name: name.clone(),
            path: target.clone(),
            sha256,
            update_info,
        };
        staged
            .persist(&target)
            .map_err(|failure| Error::FileSystem(failure.error))?;
        write_icon(&self.icons.join(format!("{id}.svg")))?;
        write_desktop_file(&self.desktop_entries.join(format!("{id}.desktop")), &record)?;
        let mut records = self.records()?;
        records.retain(|existing| existing.id != id);
        records.push(record);
        self.save_records(&records)?;
        Ok(format!(
            "Installed {name}; added an application menu entry."
        ))
    }

    pub fn remove(&self, id: &str) -> Result<String> {
        validate_id(id)?;
        let mut records = self.records()?;
        let removed = records
            .iter()
            .find(|record| record.id == id)
            .ok_or_else(|| Error::InvalidInput(format!("AppImage '{id}' is not installed")))?;
        remove_if_exists(&removed.path)?;
        remove_if_exists(&self.desktop_entries.join(format!("{id}.desktop")))?;
        remove_if_exists(&self.icons.join(format!("{id}.svg")))?;
        records.retain(|record| record.id != id);
        self.save_records(&records)?;
        Ok(format!("Removed {id}."))
    }

    pub fn updates(&self) -> Result<Vec<String>> {
        let records = self.records()?;
        let mut updates = Vec::new();
        for record in records {
            let Some(update_info) = record.update_info.as_deref() else {
                updates.push(format!("{}: no embedded update metadata", record.name));
                continue;
            };
            if !which("appimageupdatetool") {
                updates.push(format!(
                    "{}: update metadata found; install AppImageUpdate to check/apply it",
                    record.name
                ));
                continue;
            }
            let output = process::run(
                "appimageupdatetool",
                &["--check-for-update", record.path.to_string_lossy().as_ref()],
            )?;
            let report = format!(
                "{}\n{}",
                String::from_utf8_lossy(&output.stdout).trim(),
                String::from_utf8_lossy(&output.stderr).trim()
            );
            let report = report.trim();
            let normalized = report.to_ascii_lowercase();
            if normalized.contains("update available")
                && !normalized.contains("no update available")
            {
                updates.push(format!("{}: update available ({update_info})", record.name));
            } else if !report.is_empty() {
                updates.push(format!("{}: {report}", record.name));
            }
        }
        Ok(updates)
    }

    pub fn upgrade(&self) -> Result<String> {
        let mut records = self.records()?;
        if records.is_empty() {
            return Ok("No AppImages are installed.".into());
        }
        let mut results = Vec::new();
        for record in &mut records {
            if record.update_info.is_none() {
                continue;
            }
            if !which("appimageupdatetool") {
                return Err(Error::BackendUnavailable(
                    "AppImageUpdate is required to apply AppImage updates".into(),
                ));
            }
            let output = process::run(
                "appimageupdatetool",
                &[record.path.to_string_lossy().as_ref()],
            )?;
            results.push(format!(
                "{}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            ));
            record.sha256 = digest_file(&record.path)?;
            record.update_info = read_update_info(&record.path)?;
        }
        if !results.is_empty() {
            self.save_records(&records)?;
        } else if !records.is_empty() {
            return Ok("No AppImage updates applied (no embedded update metadata).".into());
        }
        Ok(results.join("\n"))
    }

    fn download(&self, url: &str) -> Result<PathBuf> {
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(300))
            .redirect(reqwest::redirect::Policy::custom(|attempt| {
                if attempt.url().scheme() == "https" && attempt.previous().len() < 10 {
                    attempt.follow()
                } else {
                    attempt.stop()
                }
            }))
            .build()?;
        let response = client.get(url).send()?.error_for_status()?;
        const MAX_IMAGE_SIZE: u64 = 2 * 1024 * 1024 * 1024;
        if response
            .content_length()
            .is_some_and(|length| length > MAX_IMAGE_SIZE)
        {
            return Err(Error::InvalidInput(
                "AppImage download exceeds the 2 GiB limit".into(),
            ));
        }
        let mut temporary = NamedTempFile::new()?;
        let mut limited = response.take(MAX_IMAGE_SIZE + 1);
        let written = std::io::copy(&mut limited, temporary.as_file_mut())?;
        if written > MAX_IMAGE_SIZE {
            return Err(Error::InvalidInput(
                "AppImage download exceeds the 2 GiB limit".into(),
            ));
        }
        temporary.flush()?;
        let (_, path) = temporary
            .keep()
            .map_err(|failure| Error::FileSystem(failure.error))?;
        Ok(path)
    }

    fn records(&self) -> Result<Vec<Record>> {
        match fs::read(&self.registry) {
            Ok(data) => Ok(serde_json::from_slice(&data)?),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
            Err(error) => Err(error.into()),
        }
    }

    fn save_records(&self, records: &[Record]) -> Result<()> {
        fs::create_dir_all(self.registry.parent().expect("registry path has a parent"))?;
        crate::fsutil::atomic_write(&self.registry, &serde_json::to_vec_pretty(records)?)
    }
}

fn record_to_package(record: Record) -> Package {
    Package {
        id: record.id,
        name: record.name,
        version: if record.update_info.is_some() {
            "update metadata available".into()
        } else {
            "AppImage".into()
        },
        source: Source::AppImage,
        description: record.path.to_string_lossy().into_owned(),
    }
}

struct TemporaryDownload(PathBuf);

impl TemporaryDownload {
    fn new(path: PathBuf) -> Self {
        Self(path)
    }
}

impl Drop for TemporaryDownload {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

fn read_update_info(path: &Path) -> Result<Option<String>> {
    let mut file = File::open(path)?;
    const ISO_SECTOR: u64 = 2048;
    const PRIMARY_VOLUME_DESCRIPTOR: u64 = 16;
    const APPLICATION_USED_OFFSET: u64 = 883;
    let offset = ISO_SECTOR * PRIMARY_VOLUME_DESCRIPTOR + APPLICATION_USED_OFFSET;
    if file.metadata()?.len() < offset + 512 {
        return Ok(None);
    }
    file.seek(SeekFrom::Start(offset))?;
    let mut field = [0_u8; 512];
    file.read_exact(&mut field)?;
    let field = field.split(|byte| *byte == 0).next().unwrap_or_default();
    let value = String::from_utf8_lossy(field).trim().to_owned();
    if !value.contains("zsync|") {
        return Ok(None);
    }
    Ok(Some(value))
}

fn validate_appimage(path: &Path) -> Result<()> {
    let mut file = File::open(path)?;
    let mut magic = [0_u8; 4];
    file.read_exact(&mut magic)
        .map_err(|_| Error::InvalidInput("file is empty or too short to be an AppImage".into()))?;
    if &magic != b"\x7fELF" {
        return Err(Error::InvalidInput(
            "downloaded file is not an ELF AppImage".into(),
        ));
    }
    const ISO_PVD: u64 = 32768;
    let size = file.metadata()?.len();
    const MAX_IMAGE_SIZE: u64 = 2 * 1024 * 1024 * 1024;
    if size > MAX_IMAGE_SIZE {
        return Err(Error::InvalidInput(
            "AppImage file exceeds the 2 GiB limit".into(),
        ));
    }
    if size >= ISO_PVD + 2048 {
        file.seek(SeekFrom::Start(ISO_PVD))?;
        let mut descriptor = [0_u8; 2048];
        file.read_exact(&mut descriptor)?;
        if descriptor[0] == 1 && &descriptor[1..6] == b"CD001" && descriptor[6] == 1 {
            let sectors = u32::from_le_bytes(descriptor[80..84].try_into().unwrap());
            let block_size = u16::from_le_bytes(descriptor[128..130].try_into().unwrap());
            if sectors > 0 && block_size > 0 && u64::from(sectors) * u64::from(block_size) <= size {
                return Ok(());
            }
        }
    }
    const CHUNK_SIZE: usize = 64 * 1024;
    let mut offset = 0;
    let mut chunk = vec![0_u8; CHUNK_SIZE];
    let mut overlap = Vec::new();
    while offset < size {
        file.seek(SeekFrom::Start(offset))?;
        let read = file.read(&mut chunk)?;
        if read == 0 {
            break;
        }
        let mut data = overlap.clone();
        data.extend_from_slice(&chunk[..read]);
        if data
            .windows(4)
            .any(|window| window == b"hsqs" || window == b"sqsh")
        {
            return Ok(());
        }
        overlap.clear();
        overlap.extend_from_slice(&data[data.len().saturating_sub(3)..]);
        offset += read as u64;
    }
    Err(Error::InvalidInput(
        "file does not contain an AppImage ISO or SquashFS payload".into(),
    ))
}

fn set_executable(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o755))?;
    }
    Ok(())
}

fn digest_file(path: &Path) -> Result<String> {
    let mut file = File::open(path)?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let length = file.read(&mut buffer)?;
        if length == 0 {
            break;
        }
        digest.update(&buffer[..length]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

fn write_icon(path: &Path) -> Result<()> {
    crate::fsutil::atomic_write(path, GENERIC_ICON.as_bytes())
}

fn write_desktop_file(path: &Path, record: &Record) -> Result<()> {
    let executable = record
        .path
        .to_string_lossy()
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    let contents = format!(
        "[Desktop Entry]\nType=Application\nName={}\nExec=\"{}\"\nTryExec=\"{}\"\nIcon={}\nTerminal=false\nCategories=Utility;\n",
        record.name.replace(['\n', '\r'], " "),
        executable,
        executable,
        record.id
    );
    crate::fsutil::atomic_write(path, contents.as_bytes())
}

fn safe_id(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '-' || character == '_' {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_owned()
}

fn validate_id(value: &str) -> Result<()> {
    if value.is_empty()
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-_".contains(&byte))
    {
        return Err(Error::InvalidInput(format!(
            "invalid AppImage id '{value}'"
        )));
    }
    Ok(())
}

fn remove_if_exists(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn which(program: &str) -> bool {
    std::env::var_os("PATH")
        .into_iter()
        .flat_map(|paths| std::env::split_paths(&paths).collect::<Vec<_>>())
        .any(|path| path.join(program).is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_elf_file_without_registering_it() {
        let root = tempfile::tempdir().unwrap();
        let invalid = root.path().join("not-an-app.AppImage");
        fs::write(&invalid, b"not an elf").unwrap();
        let backend = AppImageBackend::with_paths(root.path());
        assert!(backend.install(invalid.to_str().unwrap()).is_err());
        assert!(backend.list_installed().unwrap().is_empty());
    }

    #[test]
    fn rejects_plain_elf_without_an_appimage_payload() {
        let root = tempfile::tempdir().unwrap();
        let invalid = root.path().join("plain-binary.AppImage");
        fs::write(&invalid, b"\x7fELF this is not an AppImage").unwrap();
        let backend = AppImageBackend::with_paths(root.path());
        assert!(backend.install(invalid.to_str().unwrap()).is_err());
        assert!(backend.list_installed().unwrap().is_empty());
    }

    #[test]
    fn installs_valid_elf_creates_menu_icon_and_registry() {
        let root = tempfile::tempdir().unwrap();
        let input = root.path().join("Cute App.AppImage");
        let mut image = vec![0_u8; 32768 + 2048 * 20];
        image[..4].copy_from_slice(b"\x7fELF");
        let descriptor = &mut image[32768..32768 + 2048];
        descriptor[0] = 1;
        descriptor[1..6].copy_from_slice(b"CD001");
        descriptor[6] = 1;
        descriptor[80..84].copy_from_slice(&20_u32.to_le_bytes());
        descriptor[84..88].copy_from_slice(&20_u32.to_be_bytes());
        descriptor[128..130].copy_from_slice(&2048_u16.to_le_bytes());
        descriptor[130..132].copy_from_slice(&2048_u16.to_be_bytes());
        fs::write(&input, image).unwrap();
        let backend = AppImageBackend::with_paths(root.path());
        backend.install(input.to_str().unwrap()).unwrap();
        assert_eq!(backend.list_installed().unwrap().len(), 1);
        assert!(root.path().join("applications/cute-app.desktop").exists());
        assert!(root.path().join("icons/cute-app.svg").exists());
        assert!(backend.remove("cute-app").is_ok());
        assert!(backend.list_installed().unwrap().is_empty());
    }

    #[test]
    fn update_metadata_is_discovered_without_running_the_image() {
        let root = tempfile::tempdir().unwrap();
        let image = root.path().join("image.AppImage");
        let mut bytes = vec![0_u8; 32768 + 883 + 512];
        bytes[..4].copy_from_slice(b"\x7fELF");
        let update_info = b"zsync|https://example.invalid/app.zsync";
        bytes[32768 + 883..32768 + 883 + update_info.len()].copy_from_slice(update_info);
        fs::write(&image, bytes).unwrap();
        assert_eq!(
            read_update_info(&image).unwrap().as_deref(),
            Some("zsync|https://example.invalid/app.zsync")
        );
    }
}
