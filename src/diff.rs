use std::collections::{BTreeSet, HashMap};
use std::path::Path;

use anyhow::{anyhow, Result};

use crate::json::load_snapshot;
use crate::models::{Snapshot, UsageChange};
use crate::output::format_bytes;
use crate::paths;
use crate::report::snapshot_paths;

pub const SIGNIFICANT_BYTES: i64 = 50 * 1024 * 1024;

pub fn latest_two_from(directory: &Path) -> Result<(Snapshot, Snapshot)> {
    let paths = snapshot_paths(directory)?;
    if paths.len() < 2 {
        return Err(anyhow!(
            "two snapshots are required; snapshots are stored once per day"
        ));
    }
    let before = load_snapshot(&paths[paths.len() - 2])?;
    let after = load_snapshot(&paths[paths.len() - 1])?;
    Ok((before, after))
}

pub fn compare_snapshots(before: &Snapshot, after: &Snapshot) -> Vec<UsageChange> {
    let old = usage_map(before);
    let new = usage_map(after);
    let paths = old
        .keys()
        .chain(new.keys())
        .cloned()
        .collect::<BTreeSet<_>>();
    let mut changes = paths
        .into_iter()
        .map(|path| UsageChange {
            bytes: new.get(&path).copied().unwrap_or(0) - old.get(&path).copied().unwrap_or(0),
            path,
        })
        .filter(|change| change.bytes.abs() >= SIGNIFICANT_BYTES)
        .collect::<Vec<_>>();
    changes.sort_by(|left, right| right.bytes.abs().cmp(&left.bytes.abs()));
    changes
}

pub fn render_diff(before: &Snapshot, after: &Snapshot) -> String {
    let changes = compare_snapshots(before, after);
    let growth = changes
        .iter()
        .filter(|change| change.bytes > 0)
        .collect::<Vec<_>>();
    let shrinkage = changes
        .iter()
        .filter(|change| change.bytes < 0)
        .collect::<Vec<_>>();

    let mut lines = vec![
        "Compared:".to_string(),
        format!("{} → {}", &before.timestamp[..10], &after.timestamp[..10]),
        String::new(),
        "Growth:".to_string(),
        String::new(),
    ];
    if growth.is_empty() {
        lines.push("No significant growth.".to_string());
    } else {
        lines.extend(
            growth.iter().map(|change| {
                format!("{} {}", format_bytes(Some(change.bytes), true), change.path)
            }),
        );
    }

    lines.extend([String::new(), "Shrinkage:".to_string(), String::new()]);
    if shrinkage.is_empty() {
        lines.push("No significant shrinkage.".to_string());
    } else {
        lines.extend(
            shrinkage.iter().map(|change| {
                format!("{} {}", format_bytes(Some(change.bytes), true), change.path)
            }),
        );
    }
    lines.extend([String::new(), "No other significant changes.".to_string()]);
    lines.join("\n")
}

pub fn diff_command() -> Result<String> {
    let directory = paths::snapshot_dir()?;
    let (before, after) = latest_two_from(&directory)?;
    Ok(render_diff(&before, &after))
}

fn usage_map(snapshot: &Snapshot) -> HashMap<String, i64> {
    let mut result = HashMap::new();
    for items in [
        &snapshot.home_usage,
        &snapshot.local_share_usage,
        &snapshot.copilot_usage,
    ] {
        for item in items {
            result.insert(item.path.clone(), item.bytes);
        }
    }
    result
}
