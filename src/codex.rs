use std::fs;
use std::path::Path;

use anyhow::Result;

use crate::paths;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodexStandalone {
    pub current_release: Option<String>,
    pub releases: Vec<CodexRelease>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodexRelease {
    pub name: String,
    pub bytes: i64,
}

impl CodexStandalone {
    pub fn inactive_release_count(&self) -> usize {
        self.releases
            .iter()
            .filter(|release| Some(release.name.as_str()) != self.current_release.as_deref())
            .count()
    }

    pub fn total_storage_bytes(&self) -> i64 {
        self.releases.iter().map(|release| release.bytes).sum()
    }

    pub fn inactive_storage_bytes(&self) -> i64 {
        self.releases
            .iter()
            .filter(|release| Some(release.name.as_str()) != self.current_release.as_deref())
            .map(|release| release.bytes)
            .sum()
    }
}

pub fn detect_codex_standalone() -> Result<Option<CodexStandalone>> {
    let root = paths::home_dir()?
        .join(".codex")
        .join("packages")
        .join("standalone");
    detect_codex_standalone_at(&root)
}

pub fn detect_codex_standalone_at(root: &Path) -> Result<Option<CodexStandalone>> {
    if !root.exists() {
        return Ok(None);
    }

    let releases_dir = root.join("releases");
    if !releases_dir.is_dir() {
        return Ok(None);
    }

    let mut releases = Vec::new();
    for entry in fs::read_dir(&releases_dir)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some(version) = release_version(&name) else {
            continue;
        };
        releases.push(CodexRelease {
            name: version,
            bytes: directory_size(&entry.path())?,
        });
    }

    if releases.is_empty() {
        return Ok(None);
    }

    releases.sort_by(|left, right| left.name.cmp(&right.name));

    Ok(Some(CodexStandalone {
        current_release: current_release(root, &releases_dir),
        releases,
    }))
}

fn current_release(root: &Path, releases_dir: &Path) -> Option<String> {
    let current = root.join("current");
    let target = fs::read_link(&current).ok()?;
    let target = if target.is_absolute() {
        target
    } else {
        current
            .parent()
            .map(|parent| parent.join(&target))
            .unwrap_or(target)
    };
    let release_path = if target.starts_with(releases_dir) {
        target
    } else {
        releases_dir.join(target.file_name()?)
    };
    let name = release_path.file_name()?.to_string_lossy();
    release_version(&name)
}

fn release_version(name: &str) -> Option<String> {
    let version = name.split_once('-').map_or(name, |(version, _)| version);
    let mut parts = version.split('.');
    let Some(major) = parts.next() else {
        return None;
    };
    let Some(minor) = parts.next() else {
        return None;
    };
    let Some(patch) = parts.next() else {
        return None;
    };
    (parts.next().is_none()
        && [major, minor, patch]
            .iter()
            .all(|part| !part.is_empty() && part.chars().all(|ch| ch.is_ascii_digit())))
    .then(|| version.to_string())
}

fn directory_size(path: &Path) -> Result<i64> {
    let mut total = 0;
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let metadata = fs::symlink_metadata(entry.path())?;
        if metadata.is_dir() {
            total += directory_size(&entry.path())?;
        } else {
            total += i64::try_from(metadata.len()).unwrap_or(i64::MAX);
        }
    }
    Ok(total)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::os::unix::fs::symlink;
    use std::path::Path;

    use super::detect_codex_standalone_at;

    #[test]
    fn no_installation_returns_none() {
        let directory = tempfile::tempdir().unwrap();

        let detected = detect_codex_standalone_at(&directory.path().join("standalone")).unwrap();

        assert_eq!(detected, None);
    }

    #[test]
    fn one_release_is_detected_without_inactive_storage() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("standalone");
        write_release(&root, "0.144.6", 12);
        symlink("releases/0.144.6", root.join("current")).unwrap();

        let detected = detect_codex_standalone_at(&root).unwrap().unwrap();

        assert_eq!(detected.current_release.as_deref(), Some("0.144.6"));
        assert_eq!(detected.releases.len(), 1);
        assert_eq!(detected.inactive_release_count(), 0);
        assert_eq!(detected.total_storage_bytes(), 12);
        assert_eq!(detected.inactive_storage_bytes(), 0);
    }

    #[test]
    fn multiple_releases_report_inactive_count_and_storage() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("standalone");
        write_release(&root, "0.144.4-x86_64-unknown-linux-musl", 10);
        write_release(&root, "0.144.5-x86_64-unknown-linux-musl", 20);
        write_release(&root, "0.144.6-x86_64-unknown-linux-musl", 30);
        write_release(&root, "latest", 40);
        symlink(
            "releases/0.144.6-x86_64-unknown-linux-musl",
            root.join("current"),
        )
        .unwrap();

        let detected = detect_codex_standalone_at(&root).unwrap().unwrap();

        assert_eq!(detected.current_release.as_deref(), Some("0.144.6"));
        assert_eq!(detected.releases.len(), 3);
        assert_eq!(detected.inactive_release_count(), 2);
        assert_eq!(detected.total_storage_bytes(), 60);
        assert_eq!(detected.inactive_storage_bytes(), 30);
    }

    #[test]
    fn broken_current_symlink_does_not_panic() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("standalone");
        write_release(&root, "0.144.5", 20);
        write_release(&root, "0.144.6", 30);
        symlink("releases/9.999.9", root.join("current")).unwrap();

        let detected = detect_codex_standalone_at(&root).unwrap().unwrap();

        assert_eq!(detected.current_release.as_deref(), Some("9.999.9"));
        assert_eq!(detected.releases.len(), 2);
        assert_eq!(detected.inactive_release_count(), 2);
        assert_eq!(detected.inactive_storage_bytes(), 50);
    }

    fn write_release(root: &Path, name: &str, bytes: usize) {
        let release = root.join("releases").join(name);
        fs::create_dir_all(&release).unwrap();
        fs::write(release.join("payload"), vec![b'x'; bytes]).unwrap();
    }
}
