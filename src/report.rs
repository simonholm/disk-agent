use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};

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
        "Top consumers:".to_string(),
        String::new(),
    ];

    let consumers = top_consumers(snapshot, 5);
    if consumers.is_empty() {
        lines.push("No directory data available.".to_string());
    } else {
        lines.extend(
            consumers
                .iter()
                .map(|item| format!("{} {}", format_bytes(Some(item.bytes), false), item.path)),
        );
    }

    lines.extend([String::new(), "Podman:".to_string(), String::new()]);
    if snapshot.podman.available {
        lines.extend([
            format!(
                "Images: {}",
                format_bytes(snapshot.podman.images_bytes, false)
            ),
            format!(
                "Containers: {}",
                format_bytes(snapshot.podman.containers_bytes, false)
            ),
            format!(
                "Volumes: {}",
                format_bytes(snapshot.podman.volumes_bytes, false)
            ),
        ]);
    } else {
        lines.push(format!(
            "Unavailable ({}).",
            snapshot.podman.error.as_deref().unwrap_or("unknown error")
        ));
    }

    lines.extend([
        String::new(),
        "Largest directories:".to_string(),
        String::new(),
    ]);
    lines.extend(
        snapshot
            .largest_directories
            .iter()
            .take(10)
            .map(|item| format!("{} {}", format_bytes(Some(item.bytes), false), item.path)),
    );
    lines.extend([String::new(), "Assessment:".to_string()]);
    if fs.used_percent >= 90 {
        lines.push("Disk usage is critical.".to_string());
    } else if fs.used_percent >= 80 {
        lines.push("Disk usage is elevated.".to_string());
    } else {
        lines.push("No action required.".to_string());
    }

    lines.join("\n")
}

pub fn report_command(refresh: bool) -> Result<String> {
    let directory = paths::snapshot_dir()?;
    if refresh {
        let snapshot = crate::snapshot::collect_snapshot()?;
        let path = crate::snapshot::save_snapshot(&snapshot, &directory)?;
        return Ok(render_report_with_metadata(&snapshot, &path));
    }

    let loaded = latest_snapshot_with_path_from(&directory)?;
    Ok(render_report_with_metadata(&loaded.snapshot, &loaded.path))
}

pub fn render_report_with_metadata(snapshot: &Snapshot, path: &Path) -> String {
    format!(
        "Snapshot: saved {}\nSource: {}\n\n{}",
        snapshot.timestamp,
        path.display(),
        render_report(snapshot)
    )
}

fn top_consumers(snapshot: &Snapshot, limit: usize) -> Vec<&DirectoryUsage> {
    let mut values = snapshot
        .home_usage
        .iter()
        .filter(|item| item.path != "~" && item.path.matches('/').count() == 1)
        .collect::<Vec<_>>();
    values.sort_by(|left, right| right.bytes.cmp(&left.bytes));
    values.truncate(limit);
    values
}

fn is_snapshot_name(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    let bytes = name.as_bytes();
    bytes.len() == 15
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && &bytes[10..] == b".json"
        && bytes[..4].iter().all(u8::is_ascii_digit)
        && bytes[5..7].iter().all(u8::is_ascii_digit)
        && bytes[8..10].iter().all(u8::is_ascii_digit)
}
