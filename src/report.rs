use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};

use crate::json::load_snapshot;
use crate::models::{DirectoryUsage, Snapshot};
use crate::output::format_bytes;
use crate::paths;

pub fn snapshot_paths(directory: &Path) -> Result<Vec<PathBuf>> {
    if !directory.exists() {
        return Ok(Vec::new());
    }

    let mut paths = Vec::new();
    for entry in fs::read_dir(directory)? {
        let path = entry?.path();
        if is_snapshot_name(&path) {
            paths.push(path);
        }
    }
    paths.sort();
    Ok(paths)
}

pub fn latest_snapshot_from(directory: &Path) -> Result<Snapshot> {
    Ok(latest_snapshot_with_path_from(directory)?.snapshot)
}

pub fn latest_snapshot_with_path_from(directory: &Path) -> Result<LoadedSnapshot> {
    let paths = snapshot_paths(directory)?;
    let Some(path) = paths.last() else {
        return Err(anyhow!(
            "no snapshots found; run 'disk-agent snapshot' first"
        ));
    };
    Ok(LoadedSnapshot {
        snapshot: load_snapshot(path)?,
        path: path.clone(),
    })
}

pub struct LoadedSnapshot {
    pub snapshot: Snapshot,
    pub path: PathBuf,
}

pub fn render_report(snapshot: &Snapshot) -> String {
    let fs = &snapshot.filesystem;
    let mut lines = vec![
        format!(
            "Filesystem usage: {}% ({} of {})",
            fs.used_percent,
            format_bytes(Some(fs.used_bytes), false),
            format_bytes(Some(fs.total_bytes), false)
        ),
        String::new(),
        "Directories (nested sizes overlap):".to_string(),
    ];

    let consumers = directory_overview(snapshot);
    if consumers.is_empty() {
        lines.push("No directory data available.".to_string());
    } else {
        lines.extend(consumers.iter().map(|item| {
            format!(
                "{:>8}  {}",
                format_bytes(Some(item.bytes), false),
                item.path
            )
        }));
    }

    lines.push(String::new());
    if snapshot.podman.available {
        lines.push(format!(
            "Podman: images {}; containers {}; volumes {}",
            format_bytes(snapshot.podman.images_bytes, false),
            format_bytes(snapshot.podman.containers_bytes, false),
            format_bytes(snapshot.podman.volumes_bytes, false),
        ));
    } else {
        lines.push(format!(
            "Podman: unavailable ({}).",
            snapshot.podman.error.as_deref().unwrap_or("unknown error")
        ));
    }

    lines.extend([String::new(), "Observations:".to_string()]);
    if fs.used_percent >= 90 {
        lines.push("Disk usage is critical.".to_string());
    } else if fs.used_percent >= 80 {
        lines.push("Disk usage is elevated.".to_string());
    } else {
        lines.push("Filesystem usage is below the elevated threshold (80%).".to_string());
    }

    let rules = crate::rules::load_rules();
    let mut stores = rules
        .iter()
        .filter(|rule| rule.version_store)
        .map(|rule| rule.pattern.as_str())
        .collect::<Vec<_>>();
    stores.push("~/.codex/packages");
    let large_stores = stores
        .iter()
        .filter_map(|path| {
            snapshot
                .home_usage
                .iter()
                .chain(&snapshot.local_share_usage)
                .chain(&snapshot.copilot_usage)
                .chain(&snapshot.largest_directories)
                .filter(|usage| usage.path == *path)
                .max_by_key(|usage| usage.bytes)
                .filter(|usage| usage.bytes >= crate::release_store::MIN_RETAINED_BYTES)
        })
        .collect::<Vec<_>>();
    if !large_stores.is_empty() {
        lines.push("Substantial version-store storage in snapshot:".to_string());
    }
    for usage in &large_stores {
        if large_stores
            .iter()
            .any(|parent| usage.path.starts_with(&format!("{}/", parent.path)))
        {
            continue;
        }
        lines.push(format!(
            "{:>8}  {}",
            format_bytes(Some(usage.bytes), false),
            usage.path
        ));
    }
    if !large_stores.is_empty() {
        lines.push("Reclaimability is unknown; this snapshot does not establish it.".into());
        lines.push("Check current retention: `disk-agent investigate` (live diagnostics).".into());
    }
    if !snapshot.warnings.is_empty() {
        lines.extend([String::new(), "Collection warnings:".into()]);
        lines.extend(
            snapshot
                .warnings
                .iter()
                .map(|warning| format!("  {warning}")),
        );
    }

    wrap_report(&lines.join("\n"))
}

pub fn report_command(refresh: bool) -> Result<String> {
    let directory = paths::snapshot_dir()?;
    if refresh {
        let snapshot = crate::snapshot::collect_snapshot()?;
        let path = crate::snapshot::save_snapshot(&snapshot, &directory)?;
        let report = render_report(&snapshot);
        let (usage, details) = report.split_once('\n').unwrap_or((&report, ""));
        return Ok(wrap_report(&format!(
            "{usage}\nFresh measurements collected: {}\n{details}\n\nSnapshot saved: {}",
            snapshot.timestamp,
            path.display(),
        )));
    }

    let loaded = latest_snapshot_with_path_from(&directory)?;
    Ok(render_report_with_metadata(&loaded.snapshot, &loaded.path))
}

pub fn render_report_with_metadata(snapshot: &Snapshot, path: &Path) -> String {
    render_report_with_metadata_at(snapshot, path, Utc::now())
}

pub fn render_report_with_metadata_at(
    snapshot: &Snapshot,
    path: &Path,
    now: DateTime<Utc>,
) -> String {
    let age = match DateTime::parse_from_rfc3339(&snapshot.timestamp) {
        Ok(timestamp) => {
            let seconds = now.signed_duration_since(timestamp).num_seconds();
            if seconds < 0 {
                "unknown (snapshot timestamp is in the future)".to_string()
            } else {
                format!(
                    "{}d {}h {}m",
                    seconds / 86400,
                    seconds % 86400 / 3600,
                    seconds % 3600 / 60
                )
            }
        }
        Err(_) => "unknown (invalid snapshot timestamp)".to_string(),
    };
    let report = render_report(snapshot);
    let (usage, details) = report.split_once('\n').unwrap_or((&report, ""));
    wrap_report(&format!(
        "{usage}\nSnapshot age: {age}; current usage may differ.\nStored snapshot — current usage was not measured\nSaved: {}\n{details}\n\nRefresh: `disk-agent report --refresh` (collect and save).\nLive diagnostics: `disk-agent investigate`.\nSource: {}",
        snapshot.timestamp, path.display()
    ))
}

fn directory_overview(snapshot: &Snapshot) -> Vec<&DirectoryUsage> {
    // Merge the two former report lists without repeating an identical observation.
    let mut values = snapshot
        .home_usage
        .iter()
        .filter(|item| item.path != "~" && item.path.matches('/').count() == 1)
        .chain(snapshot.largest_directories.iter().take(10))
        .collect::<Vec<_>>();
    values.sort_by(|left, right| {
        right
            .bytes
            .cmp(&left.bytes)
            .then(left.path.cmp(&right.path))
    });
    values.dedup_by(|left, right| left.path == right.path && left.bytes == right.bytes);
    values.truncate(10);
    values
}

fn wrap_report(report: &str) -> String {
    let mut lines = Vec::new();
    for line in report.lines() {
        let mut remaining = line;
        while remaining.chars().count() > 80 {
            let end = remaining
                .char_indices()
                .nth(80)
                .map(|(index, _)| index)
                .unwrap();
            let split = remaining[..end]
                .rfind(' ')
                .filter(|&index| index > 0)
                .unwrap_or(end);
            lines.push(remaining[..split].to_string());
            remaining = remaining[split..].trim_start();
        }
        lines.push(remaining.to_string());
    }
    lines.join("\n")
}

fn is_snapshot_name(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    let bytes = name.as_bytes();
    let date_is_valid = bytes.len() >= 10
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes[..4].iter().all(u8::is_ascii_digit)
        && bytes[5..7].iter().all(u8::is_ascii_digit)
        && bytes[8..10].iter().all(u8::is_ascii_digit);

    date_is_valid
        && ((bytes.len() == 15 && &bytes[10..] == b".json")
            || (bytes.len() == 24
                && bytes[10] == b'_'
                && bytes[13] == b'-'
                && bytes[16] == b'-'
                && &bytes[19..] == b".json"
                && bytes[11..13].iter().all(u8::is_ascii_digit)
                && bytes[14..16].iter().all(u8::is_ascii_digit)
                && bytes[17..19].iter().all(u8::is_ascii_digit)))
}
