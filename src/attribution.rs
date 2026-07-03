use std::collections::{BTreeMap, BTreeSet};

use crate::classify::{classify_path, is_child_path};
use crate::diff::{snapshot_changes, SIGNIFICANT_BYTES};
use crate::models::{Snapshot, UsageChange};
use crate::output::format_bytes;
use crate::rules::{Classification, Rule};

const LARGE_CATEGORY_GROWTH_BYTES: i64 = 1024_i64.pow(3);
const ABNORMAL_CATEGORY_GROWTH_BYTES: i64 = 5 * 1024_i64.pow(3);

pub fn top_contributors(before: &Snapshot, after: &Snapshot, limit: usize) -> Vec<UsageChange> {
    let changes = snapshot_changes(before, after, SIGNIFICANT_BYTES)
        .into_iter()
        .filter(|change| change.bytes > 0)
        .collect::<Vec<_>>();
    let mut candidates = BTreeMap::<String, i64>::new();

    for change in &changes {
        let descendants = changed_descendants(change, &changes);
        if descendants.is_empty() {
            candidates
                .entry(change.path.clone())
                .and_modify(|bytes| *bytes = (*bytes).max(change.bytes))
                .or_insert(change.bytes);
        } else {
            for descendant in descendants {
                candidates
                    .entry(descendant.path.clone())
                    .and_modify(|bytes| *bytes = (*bytes).max(descendant.bytes))
                    .or_insert(descendant.bytes);
            }
        }
    }

    let mut candidates = candidates
        .into_iter()
        .map(|(path, bytes)| UsageChange { path, bytes })
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| {
        right
            .bytes
            .cmp(&left.bytes)
            .then_with(|| path_depth(&right.path).cmp(&path_depth(&left.path)))
            .then_with(|| left.path.cmp(&right.path))
    });

    let mut selected = Vec::new();
    for candidate in candidates {
        if selected.iter().any(|kept: &UsageChange| {
            is_child_path(&candidate.path, &kept.path) || is_child_path(&kept.path, &candidate.path)
        }) {
            continue;
        }
        selected.push(candidate);
        if selected.len() == limit {
            break;
        }
    }
    selected
}

pub fn cause_summary(
    contributors: &[UsageChange],
    classifications: &[(UsageChange, Classification)],
) -> String {
    let mut totals = BTreeMap::<String, i64>::new();
    let mut unclassified_total = 0;
    for (change, classification) in classifications {
        if classification.known {
            *totals.entry(classification.category.clone()).or_insert(0) += change.bytes;
        } else {
            unclassified_total += change.bytes;
        }
    }
    if totals.is_empty() && !contributors.is_empty() {
        return "Growth occurred in unclassified locations.".to_string();
    }
    if totals.is_empty() {
        return "No significant directory growth.".to_string();
    }

    let mut totals = totals.into_iter().collect::<Vec<_>>();
    if unclassified_total > 0 {
        totals.push(("unclassified locations".to_string(), unclassified_total));
    }
    totals.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));
    let parts = totals
        .into_iter()
        .take(2)
        .map(|(category, bytes)| format!("{category} ({})", format_bytes(Some(bytes), true)))
        .collect::<Vec<_>>();
    match parts.as_slice() {
        [one] => format!("Growth is primarily due to {one}."),
        [one, two] => format!("Growth is primarily due to {one} and {two}."),
        _ => "Growth occurred in unclassified locations.".to_string(),
    }
}

pub fn risk_level(after: &Snapshot, classifications: &[(UsageChange, Classification)]) -> String {
    if classifications.is_empty() {
        return "Low".to_string();
    }
    let known_bytes = classifications
        .iter()
        .filter(|(_, classification)| classification.known)
        .map(|(change, _)| change.bytes)
        .sum::<i64>();
    let unknown_bytes = classifications
        .iter()
        .filter(|(_, classification)| !classification.known)
        .map(|(change, _)| change.bytes)
        .sum::<i64>();
    if unknown_bytes > 0 && unknown_bytes >= known_bytes {
        return "Unknown".to_string();
    }

    let constrained = after.filesystem.used_percent >= 90;
    let approaching = after.filesystem.used_percent >= 80;
    let sensitive = classifications.iter().any(|(_, classification)| {
        matches!(
            classification.category.as_str(),
            "Cache" | "Podman" | "System logs"
        )
    });
    let abnormal_sensitive = classifications.iter().any(|(change, classification)| {
        matches!(classification.category.as_str(), "Podman" | "System logs")
            && change.bytes >= ABNORMAL_CATEGORY_GROWTH_BYTES
    });
    let fast_sensitive = classifications.iter().any(|(change, classification)| {
        matches!(
            classification.category.as_str(),
            "Cache" | "Podman" | "System logs"
        ) && change.bytes >= LARGE_CATEGORY_GROWTH_BYTES
    });

    if constrained || abnormal_sensitive {
        "High".to_string()
    } else if approaching || (sensitive && fast_sensitive) {
        "Medium".to_string()
    } else {
        "Low".to_string()
    }
}

pub fn recommendations(classifications: &[(UsageChange, Classification)]) -> Vec<String> {
    if classifications.is_empty() {
        return vec!["No cleanup is currently required.".to_string()];
    }

    let mut seen = BTreeSet::new();
    let mut items = Vec::new();
    for (_, classification) in classifications {
        let recommendation = recommendation_for_category(classification);
        if seen.insert(recommendation.clone()) {
            items.push(recommendation);
        }
    }
    if !items
        .iter()
        .any(|item| item == "No cleanup is currently required.")
    {
        items.push("No cleanup is currently required.".to_string());
    }
    items
}

pub fn classify_contributors(
    contributors: &[UsageChange],
    rules: &[Rule],
) -> Vec<(UsageChange, Classification)> {
    contributors
        .iter()
        .map(|change| (change.clone(), classify_path(&change.path, Some(rules))))
        .collect()
}

fn changed_descendants<'a>(
    change: &UsageChange,
    changes: &'a [UsageChange],
) -> Vec<&'a UsageChange> {
    changes
        .iter()
        .filter(|candidate| {
            candidate.path != change.path
                && is_child_path(&candidate.path, &change.path)
                && relative_depth(&candidate.path, &change.path)
                    .is_some_and(|depth| (1..=3).contains(&depth))
        })
        .collect()
}

fn recommendation_for_category(classification: &Classification) -> String {
    match classification.category.as_str() {
        "Downloads" => "Review ~/Downloads if the increase was unexpected.".to_string(),
        "Cache" => {
            "Cache growth is usually safe to inspect later; no cleanup is required now.".to_string()
        }
        "Development" => {
            "Review recent build artifacts or repository changes if unexpected.".to_string()
        }
        "System logs" => "Inspect logs before cleanup.".to_string(),
        "Podman" => "Review Podman images, containers, and volumes if this growth was unexpected; no pruning is recommended from this report alone.".to_string(),
        _ if classification.known => classification.recommendation.clone(),
        _ => "Inspect unclassified locations before taking cleanup action.".to_string(),
    }
}

fn relative_depth(path: &str, parent: &str) -> Option<usize> {
    let remainder = path.strip_prefix(&format!("{parent}/"))?;
    Some(remainder.split('/').filter(|part| !part.is_empty()).count())
}

fn path_depth(path: &str) -> usize {
    path.split('/').filter(|part| !part.is_empty()).count()
}
