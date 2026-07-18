//! Loading and durably writing config files.
//!
//! Both format backends share this module so the write path — backup, atomic
//! replace, permission preservation — exists exactly once.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// Suffix appended to the previous contents before a write.
const BACKUP_SUFFIX: &str = "confai.bak";

/// Read a config file, treating "missing" as "empty" so a first write can create it.
pub fn read_or_empty(path: &Path) -> Result<String> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(text),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(err) => Err(err).with_context(|| format!("reading {}", path.display())),
    }
}

/// Replace `path` with `contents`, keeping a one-deep backup of what was there.
///
/// The new contents land in a sibling temp file that is flushed and then renamed,
/// so a crash mid-write leaves either the old file or the new one, never a
/// truncated config.
pub fn write_atomic(path: &Path, contents: &str) -> Result<()> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)
        .with_context(|| format!("creating {}", parent.display()))?;

    if path.exists() {
        let backup = backup_path(path);
        fs::copy(path, &backup)
            .with_context(|| format!("backing up {} to {}", path.display(), backup.display()))?;
    }

    let temp = path.with_extension(format!("confai.tmp.{}", std::process::id()));
    let mut file = fs::File::create(&temp)
        .with_context(|| format!("creating {}", temp.display()))?;
    file.write_all(contents.as_bytes())
        .with_context(|| format!("writing {}", temp.display()))?;
    file.sync_all().with_context(|| format!("flushing {}", temp.display()))?;
    drop(file);

    fs::rename(&temp, path).with_context(|| {
        format!("replacing {} with {}", path.display(), temp.display())
    })?;
    Ok(())
}

/// Where [`write_atomic`] stashes the previous contents.
pub fn backup_path(path: &Path) -> PathBuf {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    path.with_file_name(format!("{name}.{BACKUP_SUFFIX}"))
}

/// Restore the file [`write_atomic`] backed up, if one exists.
pub fn restore_backup(path: &Path) -> Result<bool> {
    let backup = backup_path(path);
    if !backup.exists() {
        return Ok(false);
    }
    fs::copy(&backup, path)
        .with_context(|| format!("restoring {} from {}", path.display(), backup.display()))?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A directory of its own per test run: `write_atomic` creates temp files in
    /// the target's parent, so tests sharing one directory would observe each
    /// other's in-flight writes. The process id keeps successive runs apart
    /// without deleting a directory Windows may not have released yet.
    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir()
            .join("confai-tests")
            .join(format!("{name}-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        dir.join("config.toml")
    }

    #[test]
    fn missing_file_reads_as_empty() {
        let path = scratch("absent.toml");
        assert_eq!(read_or_empty(&path).unwrap(), "");
    }

    #[test]
    fn write_backs_up_previous_contents() {
        let path = scratch("roundtrip.toml");
        write_atomic(&path, "first").unwrap();
        write_atomic(&path, "second").unwrap();

        assert_eq!(fs::read_to_string(&path).unwrap(), "second");
        assert_eq!(fs::read_to_string(backup_path(&path)).unwrap(), "first");

        assert!(restore_backup(&path).unwrap());
        assert_eq!(fs::read_to_string(&path).unwrap(), "first");
    }

    #[test]
    fn write_leaves_no_temp_files_behind() {
        let path = scratch("clean.toml");
        write_atomic(&path, "x").unwrap();
        let strays: Vec<_> = fs::read_dir(path.parent().unwrap())
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.contains("confai.tmp"))
            .collect();
        assert!(strays.is_empty(), "left temp files: {strays:?}");
    }
}
