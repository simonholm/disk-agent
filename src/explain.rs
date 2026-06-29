use anyhow::Result;

use crate::classify::classify_path;
use crate::diff::{compare_snapshots, latest_two_from, SIGNIFICANT_BYTES};
use crate::investigate::{cause, non_overlapping};
use crate::models::Snapshot;
use crate::output::format_bytes;
use crate::paths;
use crate::rules::load_rules;

pub fn render_explanation(before: &Snapshot, after: &Snapshot) -> String {
    let old_percent = before.filesystem.used_percent;
    let new_percent = after.filesystem.used_percent;
    let filesystem_delta = after.filesystem.used_bytes - before.filesystem.used_bytes;
    let verb = if new_percent > old_percent {
        "increased"
    } else if new_percent < old_percent {
        "decreased"
    } else {
        "remained"
    };
    let first = if verb == "remained" {
        format!(
            "Disk usage {verb} at {new_percent}% (used space changed by {}).",
            format_bytes(Some(filesystem_delta), true)
        )
    } else {
        format!("Disk usage {verb} from {old_percent}% to {new_percent}%.")
    };

    let rules = load_rules();
    let growth = non_overlapping(
        compare_snapshots(before, after)
            .into_iter()
            .filter(|change| change.bytes > 0)
            .collect(),
    );
    let mut lines = vec![
        first,
        String::new(),
        "Largest contributors:".to_string(),
        String::new(),
    ];

    for change in growth.iter().take(5) {
        let classification = classify_path(&change.path, Some(&rules));
        lines.extend([
            format!("{} {}", format_bytes(Some(change.bytes), true), change.path),
            String::new(),
            "Cause:".to_string(),
            cause(change, &classification, after),
            String::new(),
            "Risk:".to_string(),
            classification.risk,
            String::new(),
            "Action:".to_string(),
            classification.recommendation,
            String::new(),
        ]);
    }

    if growth.is_empty() {
        lines.push("No significant directory growth.".to_string());
    }

    let old_podman = podman_total(before);
    let new_podman = podman_total(after);
    lines.push(String::new());
    match (old_podman, new_podman) {
        (Some(old), Some(new)) if (new - old).abs() < SIGNIFICANT_BYTES => {
            lines.push("Podman unchanged.".to_string());
        }
        (Some(old), Some(new)) => {
            lines.push(format!(
                "Podman changed by {}.",
                format_bytes(Some(new - old), true)
            ));
        }
        _ => lines.push("Podman comparison unavailable.".to_string()),
    }

    let largest_growth = growth.first().map(|change| change.bytes).unwrap_or(0);
    let unusual = new_percent - old_percent >= 5
        || filesystem_delta >= 5 * 1024_i64.pow(3)
        || largest_growth >= 5 * 1024_i64.pow(3);
    lines.extend([String::new(), "Assessment:".to_string()]);
    let (assessment, recommendation) = if new_percent >= 90 {
        (
            "Growth appears unusual because disk usage is critical.",
            "Review the largest contributor and decide manually whether its contents are still needed.",
        )
    } else if unusual {
        (
            "Growth appears unusual because the change is large for one snapshot interval.",
            "Review the largest contributor to confirm the growth is expected.",
        )
    } else if !growth.is_empty() {
        (
            "Growth appears normal and available capacity is not currently constrained.",
            "No action required.",
        )
    } else {
        ("No unusual growth was detected.", "No action required.")
    };
    lines.extend([
        assessment.to_string(),
        String::new(),
        "Recommendation:".to_string(),
        recommendation.to_string(),
    ]);
    lines.join("\n")
}

pub fn explain_command() -> Result<String> {
    let directory = paths::snapshot_dir()?;
    let (before, after) = latest_two_from(&directory)?;
    Ok(render_explanation(&before, &after))
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
