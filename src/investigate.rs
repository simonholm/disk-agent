use anyhow::{anyhow, Result};

use crate::classify::{classify_path, is_child_path};
use crate::diff::compare_snapshots;
use crate::models::{Snapshot, UsageChange};
use crate::rules::{load_rules, Classification};

pub const LARGE_CACHE_BYTES: i64 = 2 * 1024_i64.pow(3);
pub const LARGE_GROWTH_BYTES: i64 = 5 * 1024_i64.pow(3);
pub const MODERATE_GROWTH_BYTES: i64 = 1024_i64.pow(3);

pub fn non_overlapping(changes: Vec<UsageChange>) -> Vec<UsageChange> {
    let rules = load_rules();
    let mut changes = changes;
    changes.sort_by(|left, right| right.bytes.abs().cmp(&left.bytes.abs()));
    let mut result = Vec::new();

    for change in &changes {
        let known_children = changes.iter().any(|child| {
            is_child_path(&child.path, &change.path)
                && child.bytes > 0
                && classify_path(&child.path, Some(&rules)).known
                && child.bytes >= change.bytes.abs() / 2
        });
        if !classify_path(&change.path, Some(&rules)).known && known_children {
            continue;
        }
        if result
            .iter()
            .any(|kept: &UsageChange| is_child_path(&change.path, &kept.path))
        {
            continue;
        }
        result.push(change.clone());
    }
    result
}

pub fn cause(change: &UsageChange, classification: &Classification, snapshot: &Snapshot) -> String {
    if change.path == "~/.codex/packages" || change.path.starts_with("~/.codex/packages/") {
        let versions = child_names(snapshot, "~/.codex/packages");
        if !versions.is_empty() {
            let noun = if versions.len() == 1 {
                "release"
            } else {
                "releases"
            };
            return format!(
                "{} retained Codex {noun}: {}.",
                versions.len(),
                versions.join(", ")
            );
        }
    }
    classification.explanation.clone()
}

pub fn render_investigation(
    before: &Snapshot,
    after: &Snapshot,
    previous_before: Option<&Snapshot>,
) -> String {
    let rules = load_rules();
    let all_changes = compare_snapshots(before, after);
    let growth = non_overlapping(
        all_changes
            .into_iter()
            .filter(|change| change.bytes > 0)
            .collect(),
    );
    let previous_changes = previous_before
        .map(|previous| compare_snapshots(previous, before))
        .unwrap_or_default();
    let repeated = repeated_growth(&growth, &previous_changes);

    let fs = &after.filesystem;
    let mut lines = vec![
        format!(
            "Filesystem usage: {}% ({} of {})",
            fs.used_percent,
            crate::output::format_bytes(Some(fs.used_bytes), false),
            crate::output::format_bytes(Some(fs.total_bytes), false)
        ),
        format!(
            "Snapshot interval: {} to {}",
            &before.timestamp[..10],
            &after.timestamp[..10]
        ),
        String::new(),
        "Growth:".to_string(),
        String::new(),
    ];

    if growth.is_empty() {
        lines.push("No significant growth.".to_string());
    }
    for change in &growth {
        let classification = classify_path(&change.path, Some(&rules));
        lines.extend([
            format!(
                "{} {}",
                crate::output::format_bytes(Some(change.bytes), true),
                change.path
            ),
            classification.classification.clone(),
            cause(change, &classification, after),
            format!("Risk: {}", classification.risk),
        ]);
        if repeated.contains(&change.path) {
            lines.push("Repeated growth: yes".to_string());
        }
        lines.push(String::new());
    }

    lines.push("Podman:".to_string());
    match (podman_total(before), podman_total(after)) {
        (Some(old), Some(new)) => lines.push(format!(
            "Changed by {}.",
            crate::output::format_bytes(Some(new - old), true)
        )),
        _ => lines.push("Comparison unavailable.".to_string()),
    }

    let classifications = growth
        .iter()
        .map(|change| {
            (
                change.path.clone(),
                classify_path(&change.path, Some(&rules)),
            )
        })
        .collect::<std::collections::HashMap<_, _>>();
    let assessment = assess(before, after, &growth, &classifications, &repeated);
    lines.extend([
        String::new(),
        "Assessment".to_string(),
        String::new(),
        assessment.clone(),
        String::new(),
        "Recommendations".to_string(),
        String::new(),
    ]);
    lines.extend(recommendations(&assessment, &growth, &classifications));
    lines.join("\n").trim_end().to_string()
}

pub fn investigate_command() -> Result<String> {
    Err(anyhow!(
        "investigate is not implemented in the Rust phase 1 binary because live collection is not ported"
    ))
}

fn repeated_growth(
    current: &[UsageChange],
    previous: &[UsageChange],
) -> std::collections::HashSet<String> {
    let previous_growth = previous
        .iter()
        .filter(|change| change.bytes > 0)
        .map(|change| change.path.as_str())
        .collect::<std::collections::HashSet<_>>();
    current
        .iter()
        .filter(|change| change.bytes > 0 && previous_growth.contains(change.path.as_str()))
        .map(|change| change.path.clone())
        .collect()
}

pub fn assess(
    before: &Snapshot,
    after: &Snapshot,
    growth: &[UsageChange],
    classifications: &std::collections::HashMap<String, Classification>,
    repeated: &std::collections::HashSet<String>,
) -> String {
    let fs = &after.filesystem;
    let total_growth = after.filesystem.used_bytes - before.filesystem.used_bytes;
    let unknown_growth = growth
        .iter()
        .any(|change| !classifications[&change.path].known);
    let podman_growth = growth
        .iter()
        .any(|change| classifications[&change.path].classification == "Podman storage");
    let largest_growth = growth.iter().map(|change| change.bytes).max().unwrap_or(0);

    if fs.used_percent >= 90
        || total_growth >= LARGE_GROWTH_BYTES
        || largest_growth >= LARGE_GROWTH_BYTES
        || (unknown_growth && total_growth >= MODERATE_GROWTH_BYTES)
        || (podman_growth && total_growth >= MODERATE_GROWTH_BYTES)
    {
        "Attention Recommended".to_string()
    } else if fs.used_percent >= 80
        || !repeated.is_empty()
        || unknown_growth
        || large_known_cache(after)
    {
        "Monitor".to_string()
    } else {
        "Healthy".to_string()
    }
}

fn recommendations(
    assessment: &str,
    growth: &[UsageChange],
    classifications: &std::collections::HashMap<String, Classification>,
) -> Vec<String> {
    if assessment == "Healthy" && growth.is_empty() {
        return vec!["No action required.".to_string()];
    }
    let mut seen = std::collections::HashSet::new();
    let mut items = Vec::new();
    for change in growth {
        let recommendation = classifications[&change.path].recommendation.clone();
        if seen.insert(recommendation.clone()) {
            items.push(recommendation);
        }
    }
    if items.is_empty() {
        items.push("No action required.".to_string());
    }
    items
}

fn large_known_cache(snapshot: &Snapshot) -> bool {
    let patterns = [
        "~/.cargo/registry",
        "~/.npm/_cacache",
        "~/.local/share/containers",
    ];
    snapshot
        .largest_directories
        .iter()
        .chain(snapshot.home_usage.iter())
        .chain(snapshot.local_share_usage.iter())
        .any(|usage| {
            patterns.iter().any(|pattern| {
                (usage.path == *pattern || usage.path.starts_with(&format!("{pattern}/")))
                    && usage.bytes >= LARGE_CACHE_BYTES
            })
        })
}

fn podman_total(snapshot: &Snapshot) -> Option<i64> {
    if !snapshot.podman.available {
        return None;
    }
    Some(
        snapshot.podman.images_bytes.unwrap_or(0)
            + snapshot.podman.containers_bytes.unwrap_or(0)
            + snapshot.podman.volumes_bytes.unwrap_or(0),
    )
}

fn child_names(snapshot: &Snapshot, parent: &str) -> Vec<String> {
    let prefix = format!("{parent}/");
    let mut names = std::collections::BTreeSet::new();
    for usage in snapshot
        .largest_directories
        .iter()
        .chain(snapshot.home_usage.iter())
        .chain(snapshot.local_share_usage.iter())
        .chain(snapshot.copilot_usage.iter())
    {
        let Some(remainder) = usage.path.strip_prefix(&prefix) else {
            continue;
        };
        if !remainder.is_empty() && !remainder.contains('/') {
            names.insert(remainder.to_string());
        }
    }
    names.into_iter().collect()
}
