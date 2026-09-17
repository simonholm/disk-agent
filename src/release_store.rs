use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::paths;
use crate::rules::Rule;

pub const MIN_RETAINED_BYTES: i64 = 512 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseEntry {
    pub name: String,
    pub bytes: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseStore {
    pub name: String,
    pub entries: Vec<ReleaseEntry>,
    pub active_entry: Option<String>,
}

impl ReleaseStore {
    pub fn total_storage_bytes(&self) -> i64 {
        self.entries.iter().map(|entry| entry.bytes).sum()
    }

    pub fn inactive_entry_count(&self) -> Option<usize> {
        self.active_entry.as_ref().map(|active| {
            self.entries
                .iter()
                .filter(|entry| entry.name != *active)
                .count()
        })
    }

    pub fn inactive_storage_bytes(&self) -> Option<i64> {
        self.active_entry.as_ref().map(|active| {
            self.entries
                .iter()
                .filter(|entry| entry.name != *active)
                .map(|entry| entry.bytes)
                .sum()
        })
    }

    pub fn is_notable(&self) -> bool {
        match (self.inactive_entry_count(), self.inactive_storage_bytes()) {
            (Some(count), Some(bytes)) => count >= 3 && bytes >= MIN_RETAINED_BYTES,
            _ => self.entries.len() >= 4 && self.total_storage_bytes() >= MIN_RETAINED_BYTES,
        }
    }
}

pub fn detect_release_stores() -> Result<Vec<ReleaseStore>> {
    let home = paths::home_dir()?;
    detect_release_stores_at(&home, &crate::rules::load_rules())
}

pub fn detect_release_stores_at(home: &Path, rules: &[Rule]) -> Result<Vec<ReleaseStore>> {
    rules
        .iter()
        .filter(|rule| rule.version_store)
        .filter_map(|rule| detect_release_store_at(home, rule).transpose())
        .collect()
}

fn detect_release_store_at(home: &Path, rule: &Rule) -> Result<Option<ReleaseStore>> {
    let Some(entries_kind) = rule.version_entries.as_deref() else {
        return Ok(None);
    };
    let store = expand_home(home, &rule.pattern);
    if !store.is_dir() {
        return Ok(None);
    }

    let mut entries = Vec::new();
    let mut resolved_entries = Vec::new();
    for entry in fs::read_dir(&store)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let matches_kind = match entries_kind {
            "directories" => file_type.is_dir(),
            "files" => file_type.is_file(),
            _ => false,
        };
        if !matches_kind {
            continue;
        }
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        entries.push(ReleaseEntry {
            name: name.clone(),
            bytes: entry_size(&path)?,
        });
        resolved_entries.push((name, fs::canonicalize(path).ok()));
    }
    if entries.is_empty() {
        return Ok(None);
    }
    entries.sort_by(|left, right| left.name.cmp(&right.name));

    let active_entry = rule
        .current_pointer
        .as_deref()
        .and_then(|pointer| active_entry(&expand_home(home, pointer), &resolved_entries));
    Ok(Some(ReleaseStore {
        name: rule
            .store_name
            .clone()
            .unwrap_or_else(|| rule.pattern.clone()),
        entries,
        active_entry,
    }))
}

fn active_entry(pointer: &Path, entries: &[(String, Option<PathBuf>)]) -> Option<String> {
    if !fs::symlink_metadata(pointer).ok()?.file_type().is_symlink() {
        return None;
    }
    let target = fs::canonicalize(pointer).ok()?;
    entries.iter().find_map(|(name, entry)| {
        entry
            .as_ref()
            .filter(|entry| **entry == target)
            .map(|_| name.clone())
    })
}

fn expand_home(home: &Path, path: &str) -> PathBuf {
    path.strip_prefix("~/")
        .map(|suffix| home.join(suffix))
        .unwrap_or_else(|| PathBuf::from(path))
}

fn entry_size(path: &Path) -> Result<i64> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.is_dir() {
        let mut total = 0;
        for entry in fs::read_dir(path)? {
            total += entry_size(&entry?.path())?;
        }
        Ok(total)
    } else {
        Ok(i64::try_from(metadata.len()).unwrap_or(i64::MAX))
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::os::unix::fs::symlink;

    use super::{detect_release_stores_at, ReleaseEntry, ReleaseStore, MIN_RETAINED_BYTES};
    use crate::rules::Rule;

    fn rule(pattern: &str, entries: &str, pointer: &str, name: &str) -> Rule {
        Rule {
            pattern: pattern.to_string(),
            classification: String::new(),
            category: String::new(),
            risk: String::new(),
            explanation: String::new(),
            recommendation: String::new(),
            version_store: true,
            version_entries: Some(entries.to_string()),
            current_pointer: Some(pointer.to_string()),
            store_name: Some(name.to_string()),
        }
    }

    #[test]
    fn detects_codex_directory_releases_and_current_symlink() {
        let directory = tempfile::tempdir().unwrap();
        let home = directory.path();
        let store = home.join(".codex/packages/standalone/releases");
        fs::create_dir_all(store.join("0.154.0")).unwrap();
        fs::write(store.join("0.154.0/payload"), b"current").unwrap();
        fs::create_dir_all(store.join("0.153.0")).unwrap();
        fs::write(store.join("0.153.0/payload"), b"old").unwrap();
        symlink(
            "releases/0.154.0",
            home.join(".codex/packages/standalone/current"),
        )
        .unwrap();

        let stores = detect_release_stores_at(
            home,
            &[rule(
                "~/.codex/packages/standalone/releases",
                "directories",
                "~/.codex/packages/standalone/current",
                "Codex",
            )],
        )
        .unwrap();

        assert_eq!(stores[0].active_entry.as_deref(), Some("0.154.0"));
        assert_eq!(stores[0].inactive_entry_count(), Some(1));
    }

    #[test]
    fn detects_claude_file_releases_and_external_current_symlink() {
        let directory = tempfile::tempdir().unwrap();
        let home = directory.path();
        let store = home.join(".local/share/claude/versions");
        fs::create_dir_all(&store).unwrap();
        fs::write(store.join("2.1.272"), b"old").unwrap();
        fs::write(store.join("2.1.273"), b"current").unwrap();
        fs::create_dir_all(home.join(".local/bin")).unwrap();
        symlink(
            "../share/claude/versions/2.1.273",
            home.join(".local/bin/claude"),
        )
        .unwrap();

        let stores = detect_release_stores_at(
            home,
            &[rule(
                "~/.local/share/claude/versions",
                "files",
                "~/.local/bin/claude",
                "Claude",
            )],
        )
        .unwrap();

        assert_eq!(stores[0].active_entry.as_deref(), Some("2.1.273"));
    }

    #[test]
    fn unresolved_pointer_leaves_active_version_unavailable() {
        let directory = tempfile::tempdir().unwrap();
        let home = directory.path();
        let store = home.join("versions");
        fs::create_dir_all(&store).unwrap();
        fs::write(store.join("2.1.273"), b"version").unwrap();
        fs::create_dir_all(home.join("bin")).unwrap();
        symlink("../other", home.join("bin/current")).unwrap();

        let stores = detect_release_stores_at(
            home,
            &[rule("~/versions", "files", "~/bin/current", "Example")],
        )
        .unwrap();

        assert_eq!(stores[0].active_entry, None);
    }

    #[test]
    fn mismatched_pointer_leaves_active_version_unavailable() {
        let directory = tempfile::tempdir().unwrap();
        let home = directory.path();
        let store = home.join("versions");
        fs::create_dir_all(&store).unwrap();
        fs::write(store.join("2.1.273"), b"version").unwrap();
        fs::create_dir_all(home.join("bin")).unwrap();
        fs::write(home.join("other"), b"not a stored version").unwrap();
        symlink("../other", home.join("bin/current")).unwrap();

        let stores = detect_release_stores_at(
            home,
            &[rule("~/versions", "files", "~/bin/current", "Example")],
        )
        .unwrap();

        assert_eq!(stores[0].active_entry, None);
    }

    #[test]
    fn thresholds_require_excessive_retention() {
        let entries = (0..4)
            .map(|index| ReleaseEntry {
                name: index.to_string(),
                bytes: 256 * 1024 * 1024,
            })
            .collect();
        let known = ReleaseStore {
            name: "Known".to_string(),
            entries,
            active_entry: Some("3".to_string()),
        };
        assert!(known.is_notable());

        let unknown = ReleaseStore {
            name: "Unknown".to_string(),
            entries: vec![
                ReleaseEntry {
                    name: "0".to_string(),
                    bytes: MIN_RETAINED_BYTES / 4,
                };
                4
            ],
            active_entry: None,
        };
        assert!(unknown.is_notable());
    }

    #[test]
    fn ordinary_two_version_retention_is_silent() {
        let store = ReleaseStore {
            name: "Codex".to_string(),
            entries: vec![
                ReleaseEntry {
                    name: "old".to_string(),
                    bytes: MIN_RETAINED_BYTES,
                },
                ReleaseEntry {
                    name: "current".to_string(),
                    bytes: MIN_RETAINED_BYTES,
                },
            ],
            active_entry: Some("current".to_string()),
        };
        assert!(!store.is_notable());
    }
}
