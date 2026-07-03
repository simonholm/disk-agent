use anyhow::Result;

use crate::attribution::{
    cause_summary, classify_contributors, recommendations, risk_level, top_contributors,
};
use crate::diff::{latest_two_from, SIGNIFICANT_BYTES};
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
    let contributors = top_contributors(before, after, 5);
    let classifications = classify_contributors(&contributors, &rules);
    let mut lines = vec![first, String::new(), "Top contributors:".to_string()];

    if contributors.is_empty() {
        lines.push("No significant directory growth.".to_string());
    } else {
        lines.extend(contributors.iter().map(|change| {
            format!(
                "  {} {}",
                format_bytes(Some(change.bytes), true),
                change.path
            )
        }));
    }

    lines.extend([
        String::new(),
        "Cause:".to_string(),
        cause_summary(&contributors, &classifications),
        String::new(),
        "Risk:".to_string(),
        risk_level(after, &classifications),
        String::new(),
        "Recommendations:".to_string(),
    ]);
    lines.extend(recommendations(&classifications));

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
