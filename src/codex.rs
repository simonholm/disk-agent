use std::fs;
use std::path::Path;

use anyhow::Result;

use crate::command::{CommandRunner, SystemCommandRunner};
use crate::paths;
use crate::release_store::{version_key, ReleaseStore};

pub const STANDALONE_STORE: &str = "Codex standalone";
pub const DAEMON_STORE: &str = "Codex app-server-daemon";

pub fn installed_cli_version() -> Result<Option<String>> {
    let output = SystemCommandRunner.run(&["codex", "--version"])?;
    Ok((output.status == 0)
        .then(|| parse_cli_version(&output.stdout))
        .flatten())
}

pub fn package_store_name(store: &ReleaseStore) -> Option<&'static str> {
    match store.name.as_str() {
        STANDALONE_STORE => Some("standalone"),
        DAEMON_STORE => Some("app-server-daemon"),
        _ => None,
    }
}

pub fn has_retained_versions(stores: &[&ReleaseStore]) -> bool {
    stores.iter().any(|store| {
        store
            .entries
            .iter()
            .filter(|entry| package_release_version(&entry.name).is_some())
            .take(2)
            .count()
            > 1
    })
}

pub fn older_generation_bytes(stores: &[&ReleaseStore], installed: Option<&str>) -> Option<i64> {
    let installed_key = version_key(installed?)?;
    if stores.len() != 2
        || !stores.iter().any(|store| store.name == STANDALONE_STORE)
        || !stores.iter().any(|store| store.name == DAEMON_STORE)
        || !has_retained_versions(stores)
    {
        return None;
    }
    let mut older_bytes = 0;
    for store in stores {
        if store.current_pointer_configured && store.active_entry.is_none() {
            return None;
        }
        let suffix = store
            .entries
            .first()?
            .name
            .split_once('-')
            .map(|(_, suffix)| suffix);
        if suffix.is_some_and(|suffix| {
            !matches!(
                suffix,
                "x86_64-unknown-linux-musl"
                    | "aarch64-unknown-linux-musl"
                    | "x86_64-unknown-linux-gnu"
                    | "aarch64-unknown-linux-gnu"
            )
        }) || store
            .entries
            .iter()
            .any(|entry| entry.name.split_once('-').map(|(_, suffix)| suffix) != suffix)
        {
            return None;
        }
        let versions = store
            .entries
            .iter()
            .map(|entry| package_release_version(&entry.name))
            .collect::<Option<Vec<_>>>()?;
        if versions.iter().max() != Some(&installed_key) {
            return None;
        }
        if store
            .active_entry
            .as_deref()
            .is_some_and(|active| package_release_version(active) != Some(installed_key))
        {
            return None;
        }
        // Ambiguous platform variants must not be treated as removable generations.
        if versions
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            != versions.len()
        {
            return None;
        }
        let previous = versions
            .iter()
            .filter(|version| **version < installed_key)
            .max()
            .copied();
        older_bytes += store
            .entries
            .iter()
            .zip(versions)
            .filter(|(_, version)| previous.is_some_and(|previous| *version < previous))
            .map(|(entry, _)| entry.bytes)
            .sum::<i64>();
    }
    Some(older_bytes)
}

fn parse_cli_version(output: &str) -> Option<String> {
    let version = output.trim().strip_prefix("codex-cli ")?;
    version_key(version).map(|_| version.to_string())
}

pub(crate) fn package_release_version(name: &str) -> Option<(u64, u64, u64)> {
    release_version(name).and_then(|version| version_key(&version))
}

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

    use super::{
        detect_codex_standalone_at, has_retained_versions, older_generation_bytes,
        package_store_name, parse_cli_version,
    };
    use crate::release_store::{detect_release_stores_at, ReleaseStore};
    use crate::rules::load_rules;

    const MIB: i64 = 1024 * 1024;

    #[test]
    fn two_matching_package_stores_estimate_older_generation() {
        let home = tempfile::tempdir().unwrap();
        for store in ["standalone", "app-server-daemon"] {
            let root = home.path().join(".codex/packages").join(store);
            write_release(
                &root,
                "0.157.0-x86_64-unknown-linux-musl",
                (374 * MIB) as usize,
            );
            write_release(
                &root,
                "0.157.1-x86_64-unknown-linux-musl",
                (374 * MIB) as usize,
            );
        }

        symlink(
            "releases/0.157.1-x86_64-unknown-linux-musl",
            home.path().join(".codex/packages/standalone/current"),
        )
        .unwrap();
        let mut detected = package_stores(home.path());
        let stores = detected.iter().collect::<Vec<_>>();
        assert_eq!(stores.len(), 2);
        assert!(has_retained_versions(&stores));
        assert_eq!(older_generation_bytes(&stores, Some("0.157.1")), Some(0));
        assert_eq!(older_generation_bytes(&stores, None), None);
        assert_eq!(older_generation_bytes(&stores[..1], Some("0.157.1")), None);
        assert_eq!(older_generation_bytes(&stores, Some("0.157.0")), None);
        assert_eq!(
            parse_cli_version("codex-cli 0.157.1\n"),
            Some("0.157.1".into())
        );

        for store in &mut detected {
            store.entries.push(crate::release_store::ReleaseEntry {
                name: "0.156.9-x86_64-unknown-linux-musl".into(),
                bytes: 100 * MIB,
            });
        }
        let stores = detected.iter().collect::<Vec<_>>();
        assert_eq!(
            older_generation_bytes(&stores, Some("0.157.1")),
            Some(200 * MIB)
        );

        detected[1].entries[1].name = "0.157.2-x86_64-unknown-linux-musl".into();
        let stores = detected.iter().collect::<Vec<_>>();
        assert_eq!(older_generation_bytes(&stores, Some("0.157.1")), None);

        detected[1].entries[1].name = "0.157.1-x86_64-unknown-linux-musl".into();
        detected[0].active_entry = Some("0.157.0-x86_64-unknown-linux-musl".into());
        let stores = detected.iter().collect::<Vec<_>>();
        assert_eq!(older_generation_bytes(&stores, Some("0.157.1")), None);
        detected[0].active_entry = Some("0.157.1-x86_64-unknown-linux-musl".into());
        detected[0].entries[2].name = "0.156.9-aarch64-unknown-linux-musl".into();
        let stores = detected.iter().collect::<Vec<_>>();
        assert_eq!(older_generation_bytes(&stores, Some("0.157.1")), None);
        detected[0].entries[2].name = "0.156.9-x86_64-unknown-linux-musl".into();

        for store in &mut detected {
            store.entries[0].name = "latest".into();
        }
        let stores = detected.iter().collect::<Vec<_>>();
        assert_eq!(older_generation_bytes(&stores, Some("0.157.1")), None);
    }

    #[test]
    fn configured_pointer_must_resolve_before_estimating_reclaimability() {
        for state in [
            "unconfigured",
            "resolved",
            "missing",
            "broken",
            "external",
            "file",
        ] {
            let home = tempfile::tempdir().unwrap();
            for store in ["standalone", "app-server-daemon"] {
                let root = home.path().join(".codex/packages").join(store);
                for version in ["0.156.9", "0.157.0", "0.157.1"] {
                    write_release(&root, &format!("{version}-x86_64-unknown-linux-musl"), 10);
                }
            }
            let pointer = home.path().join(".codex/packages/standalone/current");
            let mut rules = load_rules();
            match state {
                "unconfigured" => {
                    rules
                        .iter_mut()
                        .find(|rule| rule.store_name.as_deref() == Some(super::STANDALONE_STORE))
                        .unwrap()
                        .current_pointer = None;
                }
                "resolved" => {
                    symlink("releases/0.157.1-x86_64-unknown-linux-musl", &pointer).unwrap()
                }
                "broken" => symlink("releases/nonexistent", &pointer).unwrap(),
                "external" => {
                    let external = home.path().join("external-release");
                    fs::create_dir(&external).unwrap();
                    symlink(&external, &pointer).unwrap();
                }
                "file" => fs::write(&pointer, b"not a symlink").unwrap(),
                "missing" => {}
                _ => unreachable!(),
            }
            let detected = detect_release_stores_at(home.path(), &rules).unwrap();
            let standalone = detected
                .iter()
                .find(|store| store.name == super::STANDALONE_STORE)
                .unwrap();
            assert_eq!(
                standalone.current_pointer_configured,
                state != "unconfigured"
            );
            assert_eq!(standalone.active_entry.is_some(), state == "resolved");
            let stores = detected.iter().collect::<Vec<_>>();
            let confirmed = matches!(state, "unconfigured" | "resolved");
            assert_eq!(
                older_generation_bytes(&stores, Some("0.157.1")),
                confirmed.then_some(20),
                "{state}"
            );
            let snapshot =
                crate::json::load_snapshot("tests/fixtures/snapshot_full.json".as_ref()).unwrap();
            let output = crate::investigate::render_investigation_with_codex_packages(
                None,
                &snapshot,
                &detected,
                Some("0.157.1"),
            );
            assert_eq!(
                output.contains("potentially reclaimable"),
                confirmed,
                "{state}"
            );
            if !confirmed {
                assert!(
                    output.contains("Package versions do not confirm an older generation"),
                    "{state}"
                );
            }
        }
    }

    fn package_stores(home: &Path) -> Vec<ReleaseStore> {
        detect_release_stores_at(home, &load_rules())
            .unwrap()
            .into_iter()
            .filter(|store| package_store_name(store).is_some())
            .collect()
    }

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
        let payload = fs::File::create(release.join("payload")).unwrap();
        payload.set_len(bytes as u64).unwrap();
    }
}
