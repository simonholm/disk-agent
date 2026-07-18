use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};

use crate::classify::{classify_path, is_child_path};
use crate::diff::compare_snapshots;
use crate::json::load_snapshot;
use crate::models::{DirectoryUsage, Snapshot, UsageChange};
use crate::paths;
use crate::rules::{load_rules, Classification};
use crate::snapshot::{collect_snapshot, save_snapshot};

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

pub fn render_investigation(today_baseline: Option<&Snapshot>, current: &Snapshot) -> String {
    let rules = load_rules();
    let recent_changes = today_baseline
        .map(|baseline| compare_snapshots(baseline, current))
        .unwrap_or_default();
    let recent_increases = non_overlapping(
        recent_changes
            .into_iter()
            .filter(|change| change.bytes > 0)
            .collect(),
    );

    let fs = &current.filesystem;
    let mut lines = vec![
        "Current filesystem usage".to_string(),
        String::new(),
        format!(
            "{} mounted at {}: {}% used ({} of {}, {} available)",
            fs.filesystem,
            fs.mountpoint,
            fs.used_percent,
            crate::output::format_bytes(Some(fs.used_bytes), false),
            crate::output::format_bytes(Some(fs.total_bytes), false),
            crate::output::format_bytes(Some(fs.available_bytes), false)
        ),
        String::new(),
        "Largest consumers".to_string(),
        String::new(),
    ];

    let largest = largest_consumers(current, 10);
    if largest.is_empty() {
        lines.push("No directory usage data collected.".to_string());
    }
    for usage in &largest {
        let classification = classify_path(&usage.path, Some(&rules));
        lines.push(format!(
            "{} {} ({})",
            crate::output::format_bytes(Some(usage.bytes), false),
            usage.path,
            classification.category
        ));
    }

    if today_baseline.is_some() {
        lines.push(String::new());
        lines.push("Changes since today's snapshot".to_string());
        lines.push(String::new());
        if recent_increases.is_empty() {
            lines.push("No significant same-day increases detected.".to_string());
        } else {
            for change in &recent_increases {
                let classification = classify_path(&change.path, Some(&rules));
                lines.extend([
                    format!(
                        "{} {}",
                        crate::output::format_bytes(Some(change.bytes), true),
                        change.path
                    ),
                    format!("Classification: {}", classification.classification),
                    format!("Risk: {}", classification.risk),
                ]);
                lines.push(String::new());
            }
        }
    }

    lines.push(String::new());
    lines.push("Podman status".to_string());
    lines.push(String::new());
    lines.extend(podman_status(today_baseline, current));

    if !current.warnings.is_empty() {
        lines.extend([
            String::new(),
            "Collection warnings".to_string(),
            String::new(),
        ]);
        lines.extend(
            current
                .warnings
                .iter()
                .map(|warning| format!("- {warning}")),
        );
    }

    let classifications = recent_increases
        .iter()
        .map(|change| {
            (
                change.path.clone(),
                classify_path(&change.path, Some(&rules)),
            )
        })
        .collect::<std::collections::HashMap<_, _>>();
    let largest_classifications = largest
        .iter()
        .map(|usage| (usage.path.clone(), classify_path(&usage.path, Some(&rules))))
        .collect::<std::collections::HashMap<_, _>>();
    let podman_increasing = podman_increased(today_baseline, current);
    let assessment = assess(
        current,
        &recent_increases,
        &classifications,
        &largest,
        &largest_classifications,
        podman_increasing,
    );
    lines.extend([
        String::new(),
        "Assessment".to_string(),
        String::new(),
        assessment.clone(),
        String::new(),
        "Recommendations".to_string(),
        String::new(),
    ]);
    lines.extend(recommendations(&assessment, &recent_increases, &largest));
    lines.join("\n").trim_end().to_string()
}

pub fn investigate_command() -> Result<String> {
    let directory = paths::snapshot_dir()?;
    let current = collect_snapshot()?;
    let today_baseline = today_snapshot(&directory, &current)?;
    let saved = save_if_new_day(&current, &directory)?;

    let report = render_investigation(today_baseline.as_ref(), &current);
    Ok(match saved {
        None => format!(
            "{report}\n\nFresh scan completed in memory; today's snapshot file already exists."
        ),
        Some(path) => format!("{report}\n\nSnapshot stored: {}", path.display()),
    })
}

fn today_snapshot(directory: &Path, current: &Snapshot) -> Result<Option<Snapshot>> {
    let day = current
        .timestamp
        .get(..10)
        .ok_or_else(|| anyhow!("snapshot timestamp is too short"))?;
    let path = directory.join(format!("{day}.json"));
    if path.exists() {
        Ok(Some(load_snapshot(&path)?))
    } else {
        Ok(None)
    }
}

fn largest_consumers(snapshot: &Snapshot, limit: usize) -> Vec<DirectoryUsage> {
    let mut by_path = std::collections::HashMap::new();
    for usage in snapshot
        .largest_directories
        .iter()
        .chain(snapshot.home_usage.iter())
        .chain(snapshot.local_share_usage.iter())
        .chain(snapshot.copilot_usage.iter())
    {
        if usage.path != "~" {
            by_path.insert(usage.path.clone(), usage.clone());
        }
    }
    let mut consumers = by_path.into_values().collect::<Vec<_>>();
    consumers.sort_by(|left, right| {
        right
            .bytes
            .cmp(&left.bytes)
            .then_with(|| left.path.cmp(&right.path))
    });
    consumers.truncate(limit);
    consumers
}

fn podman_status(today_baseline: Option<&Snapshot>, current: &Snapshot) -> Vec<String> {
    let podman = &current.podman;
    if !podman.available {
        return vec![format!(
            "Unavailable: {}",
            podman
                .error
                .as_deref()
                .unwrap_or("Podman usage could not be collected")
        )];
    }

    let mut lines = vec![
        format!(
            "Images: {}",
            crate::output::format_bytes(podman.images_bytes, false)
        ),
        format!(
            "Containers: {}",
            crate::output::format_bytes(podman.containers_bytes, false)
        ),
        format!(
            "Volumes: {}",
            crate::output::format_bytes(podman.volumes_bytes, false)
        ),
        format!(
            "Total: {}",
            crate::output::format_bytes(podman_total(current), false)
        ),
    ];

    if let Some(baseline) = today_baseline {
        match (podman_total(baseline), podman_total(current)) {
            (Some(old), Some(new)) => lines.push(format!(
                "Change since today's snapshot: {}.",
                crate::output::format_bytes(Some(new - old), true)
            )),
            _ => lines.push("Change since today's snapshot: unavailable.".to_string()),
        }
    }

    lines
}

fn podman_increased(today_baseline: Option<&Snapshot>, current: &Snapshot) -> bool {
    match today_baseline.and_then(|baseline| podman_total(baseline).zip(podman_total(current))) {
        Some((old, new)) => new - old >= MODERATE_GROWTH_BYTES,
        None => false,
    }
}

pub fn assess(
    current: &Snapshot,
    recent_increases: &[UsageChange],
    classifications: &std::collections::HashMap<String, Classification>,
    largest: &[DirectoryUsage],
    largest_classifications: &std::collections::HashMap<String, Classification>,
    podman_increasing: bool,
) -> String {
    let evidence = scan_evidence(
        current,
        recent_increases,
        classifications,
        largest,
        largest_classifications,
        podman_increasing,
    );

    if evidence.large_unclassified_growth {
        "Large unclassified growth".to_string()
    } else if evidence.investigation_recommended {
        "Investigation recommended".to_string()
    } else if evidence.container_storage_increasing {
        "Container storage increasing".to_string()
    } else if evidence.build_artifacts_accumulating {
        "Build artifacts accumulating".to_string()
    } else if evidence.cache_growth_expected {
        "Cache growth expected".to_string()
    } else {
        "Healthy".to_string()
    }
}

struct ScanEvidence {
    large_unclassified_growth: bool,
    investigation_recommended: bool,
    container_storage_increasing: bool,
    build_artifacts_accumulating: bool,
    cache_growth_expected: bool,
}

fn scan_evidence(
    current: &Snapshot,
    recent_increases: &[UsageChange],
    classifications: &std::collections::HashMap<String, Classification>,
    largest: &[DirectoryUsage],
    largest_classifications: &std::collections::HashMap<String, Classification>,
    podman_increasing: bool,
) -> ScanEvidence {
    let large_unclassified_growth = recent_increases.iter().any(|change| {
        !classifications[&change.path].known && change.bytes >= MODERATE_GROWTH_BYTES
    });
    let largest_recent = recent_increases
        .iter()
        .map(|change| change.bytes)
        .max()
        .unwrap_or(0);
    let has_unclassified_change = recent_increases
        .iter()
        .any(|change| !classifications[&change.path].known);
    let container_storage_increasing = podman_increasing
        || recent_increases
            .iter()
            .any(|change| classifications[&change.path].category == "Podman");
    let build_recent = recent_increases.iter().any(|change| {
        matches!(
            classifications[&change.path].category.as_str(),
            "Development" | "Rust" | "Node"
        )
    });
    let build_artifacts = largest.iter().any(|usage| {
        matches!(
            largest_classifications[&usage.path].category.as_str(),
            "Development" | "Rust" | "Node"
        ) && usage.bytes >= LARGE_CACHE_BYTES
    });
    let cache_recent = recent_increases
        .iter()
        .any(|change| classifications[&change.path].category == "Cache");
    let cache_current = large_known_cache(current)
        || largest.iter().any(|usage| {
            largest_classifications[&usage.path].category == "Cache"
                && usage.bytes >= LARGE_CACHE_BYTES
        });

    ScanEvidence {
        large_unclassified_growth,
        investigation_recommended: current.filesystem.used_percent >= 85
            || largest_recent >= LARGE_GROWTH_BYTES
            || has_unclassified_change
            || !current.warnings.is_empty(),
        container_storage_increasing,
        build_artifacts_accumulating: build_recent || build_artifacts,
        cache_growth_expected: cache_recent || cache_current,
    }
}

fn recommendations(
    assessment: &str,
    recent_increases: &[UsageChange],
    largest: &[DirectoryUsage],
) -> Vec<String> {
    let mut items = Vec::new();
    match assessment {
        "Large unclassified growth" => {
            if let Some(change) = recent_increases.first() {
                items.push(format!(
                    "Inspect {} because it changed by {} and is not classified by current rules.",
                    change.path,
                    crate::output::format_bytes(Some(change.bytes), true)
                ));
            } else if let Some(usage) = largest.first() {
                items.push(format!(
                    "Inspect {} because it is currently the largest observed consumer.",
                    usage.path
                ));
            }
        }
        "Investigation recommended" => {
            if recent_increases.is_empty() {
                items.push(
                    "Review the listed directories to determine whether current usage is expected."
                        .to_string(),
                );
            } else {
                items.push(
                    "Review the listed directories to determine whether the growth is expected."
                        .to_string(),
                );
            }
        }
        "Container storage increasing" => {
            items.push(
                "Review Podman images and containers if the growth is unexpected.".to_string(),
            );
        }
        "Build artifacts accumulating" => {
            items.push(
                "Consider cleaning build artifacts if they are no longer needed.".to_string(),
            );
        }
        "Cache growth expected" => {
            items.push("No cleanup required unless disk space becomes constrained.".to_string());
        }
        "Healthy" => {}
        _ => {}
    }

    if items.is_empty() {
        items.push("No action required.".to_string());
    }

    dedupe(items)
}

fn dedupe(items: Vec<String>) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    items
        .into_iter()
        .filter(|item| seen.insert(item.clone()))
        .collect()
}

fn save_if_new_day(snapshot: &Snapshot, directory: &Path) -> Result<Option<PathBuf>> {
    let day = snapshot
        .timestamp
        .get(..10)
        .ok_or_else(|| anyhow!("snapshot timestamp is too short"))?;
    let destination = directory.join(format!("{day}.json"));
    if destination.exists() {
        return Ok(None);
    }
    save_snapshot(snapshot, directory).map(Some)
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
