use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};
use chrono::{Local, SecondsFormat};

use crate::filesystem::{collect_du, collect_filesystem};
use crate::json::write_snapshot_pretty;
use crate::models::{DirectoryUsage, Snapshot};
use crate::paths;
use crate::podman::collect_podman;

pub const TOP_DIRECTORY_DEPTH: u8 = 3;

pub fn save_snapshot(snapshot: &Snapshot, directory: &Path) -> Result<PathBuf> {
    fs::create_dir_all(directory)?;
    let day = snapshot
        .timestamp
        .get(..10)
        .ok_or_else(|| anyhow!("snapshot timestamp is too short"))?;
    let destination = directory.join(format!("{day}.json"));
    let temporary = destination.with_extension("json.tmp");
    write_snapshot_pretty(&temporary, snapshot)?;
    fs::rename(&temporary, &destination)?;
    Ok(destination)
}

pub fn snapshot_command() -> Result<String> {
    let snapshot = collect_snapshot()?;
    let path = save_snapshot(&snapshot, &paths::snapshot_dir()?)?;
    let mut message = format!("Snapshot stored: {}", path.display());
    if !snapshot.warnings.is_empty() {
        message.push_str(&format!(
            "\nCompleted with {} ignored warning(s).",
            snapshot.warnings.len()
        ));
    }
    Ok(message)
}

pub fn collect_snapshot() -> Result<Snapshot> {
    let home = paths::home_dir()?;
    let (home_usage, mut warnings) = collect_du(&home, 2)?;
    let (local_share_usage, local_warnings) = collect_du(&home.join(".local").join("share"), 2)?;
    let (copilot_usage, copilot_warnings) = collect_du(&home.join(".copilot"), 2)?;
    let (top_usage, top_warnings) = collect_du(&home, TOP_DIRECTORY_DEPTH)?;
    warnings.extend(local_warnings);
    warnings.extend(copilot_warnings);
    warnings.extend(top_warnings);

    let largest_directories = largest_directories(&top_usage, &local_share_usage, &copilot_usage);

    Ok(Snapshot {
        timestamp: Local::now().to_rfc3339_opts(SecondsFormat::Secs, false),
        filesystem: collect_filesystem()?,
        home_usage,
        local_share_usage,
        copilot_usage,
        podman: collect_podman()?,
        largest_directories,
        warnings,
        schema_version: 1,
    })
}

fn largest_directories(
    top_usage: &[DirectoryUsage],
    local_share_usage: &[DirectoryUsage],
    copilot_usage: &[DirectoryUsage],
) -> Vec<DirectoryUsage> {
    let mut by_path = std::collections::HashMap::new();
    for usage in top_usage
        .iter()
        .chain(local_share_usage.iter())
        .chain(copilot_usage.iter())
    {
        by_path.insert(usage.path.clone(), usage.clone());
    }
    let mut largest = by_path.into_values().collect::<Vec<_>>();
    largest.sort_by(|left, right| right.bytes.cmp(&left.bytes));
    largest
        .into_iter()
        .filter(|usage| usage.path != "~")
        .take(100)
        .collect()
}
