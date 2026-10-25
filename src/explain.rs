use anyhow::Result;

use crate::attribution::{
    cause_summary, classify_contributors, recommendations_with_reassurance, risk_level,
    select_contributors, unclassified_growth_lines,
};
use crate::codex::{detect_codex_standalone, CodexStandalone};
use crate::diff::{latest_two_from, snapshot_changes, SIGNIFICANT_BYTES};
use crate::models::Snapshot;
use crate::models::UsageChange;
use crate::output::format_bytes;
use crate::paths;
use crate::rules::{load_rules, Classification, Rule};

pub fn render_explanation(before: &Snapshot, after: &Snapshot) -> String {
    render_explanation_with_codex(before, after, None)
}

pub fn render_explanation_with_codex(
    before: &Snapshot,
    after: &Snapshot,
    codex_standalone: Option<&CodexStandalone>,
) -> String {
    let old_percent = before.filesystem.used_percent;
    let new_percent = after.filesystem.used_percent;
    let comparable = comparable_measurements(before, after);
    let filesystem_delta =
        comparable.then(|| after.filesystem.used_bytes - before.filesystem.used_bytes);
    let home_delta = comparable
        .then(|| home_total(after)?.checked_sub(home_total(before)?))
        .flatten();
    let unexplained_delta = filesystem_delta
        .zip(home_delta)
        .and_then(|(filesystem, home)| filesystem.checked_sub(home));
    let verb = if new_percent > old_percent {
        "increased"
    } else if new_percent < old_percent {
        "decreased"
    } else {
        "remained"
    };
    let first = if !comparable {
        "Snapshot comparison unavailable: incompatible or invalid measurements.".to_string()
    } else if verb == "remained" {
        format!(
            "Disk usage {verb} at {new_percent}% (used space changed by {}).",
            format_bytes(filesystem_delta, true)
        )
    } else {
        format!("Disk usage {verb} from {old_percent}% to {new_percent}%.")
    };

    let rules = load_rules();
    let changes = snapshot_changes(before, after, SIGNIFICANT_BYTES)
        .into_iter()
        .filter(|change| change.bytes > 0)
        .collect::<Vec<_>>();
    let change_count = changes.len();
    let validated_changes = changes
        .into_iter()
        .filter(|change| {
            comparable
                && directory_total(after, &change.path)
                    .zip(directory_total(before, &change.path))
                    .is_some_and(|(new, old)| new - old == change.bytes)
        })
        .collect::<Vec<_>>();
    let rejected_comparisons = validated_changes.len() != change_count;
    let observed_contributors = select_contributors(&validated_changes, 5);
    let classifications =
        classify_contributors_with_codex(&observed_contributors, &rules, codex_standalone);
    let collection_warnings = !before.warnings.is_empty() || !after.warnings.is_empty();
    let incomplete_observations =
        collection_warnings || rejected_comparisons || home_delta.is_none();
    let uncertain_growth = incomplete_observations
        && (unexplained_delta.is_some_and(|bytes| bytes >= SIGNIFICANT_BYTES)
            || (home_delta.is_none()
                && filesystem_delta.is_some_and(|bytes| bytes >= SIGNIFICANT_BYTES)));
    let risk = if !comparable {
        "Unknown".to_string()
    } else {
        let risk = risk_level(after, &classifications);
        if uncertain_growth {
            let risk = if risk == "Low" { "Unknown" } else { &risk };
            format!("{risk} (incomplete observations; significant growth is unexplained).")
        } else {
            risk
        }
    };
    let allow_reassurance = risk == "Low" && !incomplete_observations;
    let mut lines = vec![
        first,
        format!(
            "Total filesystem change: {}",
            format_bytes(filesystem_delta, true)
        ),
        format!(
            "Observed home-directory change: {}",
            format_bytes(home_delta, true)
        ),
        format!(
            "Difference unexplained by recorded home usage: {}",
            format_bytes(unexplained_delta, true)
        ),
    ];
    if collection_warnings {
        lines.push("Collection warnings: home observations are incomplete; the difference does not establish where growth occurred.".to_string());
    }
    lines.extend([String::new(), "Top contributors:".to_string()]);

    if !comparable || rejected_comparisons {
        lines.push("Some directory growth comparisons are unavailable: missing, conflicting, or incompatible measurements.".to_string());
    }
    if observed_contributors.is_empty() && (!comparable || rejected_comparisons) {
        lines.push("No comparable significant directory growth.".to_string());
    } else if observed_contributors.is_empty() {
        lines.push("No significant directory growth.".to_string());
    } else {
        lines.extend(observed_contributors.iter().map(|change| {
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
        if observed_contributors.is_empty() && (!comparable || rejected_comparisons) {
            "Observed contributor summary unavailable.".to_string()
        } else {
            cause_summary(&observed_contributors, &classifications)
        },
    ]);
    lines.extend(unclassified_growth_lines(&classifications));
    lines.extend([
        String::new(),
        "Risk:".to_string(),
        risk,
        String::new(),
        "Recommendations:".to_string(),
    ]);
    if !comparable {
        lines.push(
            "Collect compatible snapshots before assessing growth or recommending cleanup."
                .to_string(),
        );
    } else {
        lines.extend(recommendations_with_reassurance(
            &classifications,
            allow_reassurance,
        ));
        if incomplete_observations {
            lines.push("Inspect filesystem usage and incomplete home observations before deciding whether cleanup is needed.".to_string());
        } else if new_percent >= 80 {
            lines.push("Inspect filesystem usage and accepted directory growth before deciding whether cleanup is needed.".to_string());
        }
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

    lines.join("\n")
}

fn comparable_measurements(before: &Snapshot, after: &Snapshot) -> bool {
    let old = &before.filesystem;
    let new = &after.filesystem;
    before.schema_version == 1
        && after.schema_version == 1
        && !old.filesystem.is_empty()
        && old.filesystem == new.filesystem
        && !old.mountpoint.is_empty()
        && old.mountpoint == new.mountpoint
        && old.total_bytes > 0
        && old.total_bytes == new.total_bytes
        && (0..=old.total_bytes).contains(&old.used_bytes)
        && (0..=new.total_bytes).contains(&new.used_bytes)
        && (0..=100).contains(&old.used_percent)
        && (0..=100).contains(&new.used_percent)
        && (0..=old.total_bytes - old.used_bytes).contains(&old.available_bytes)
        && (0..=new.total_bytes - new.used_bytes).contains(&new.available_bytes)
        && matches!(
            (
                chrono::DateTime::parse_from_rfc3339(&before.timestamp),
                chrono::DateTime::parse_from_rfc3339(&after.timestamp)
            ),
            (Ok(old), Ok(new)) if old < new
        )
}

fn home_total(snapshot: &Snapshot) -> Option<i64> {
    snapshot.home_usage.iter().find(|entry| entry.path == "~")?;
    directory_total(snapshot, "~")
}

fn directory_total(snapshot: &Snapshot, path: &str) -> Option<i64> {
    let mut values = snapshot
        .home_usage
        .iter()
        .chain(&snapshot.local_share_usage)
        .chain(&snapshot.copilot_usage)
        .filter(|entry| entry.path == path);
    let total = values.next()?.bytes;
    (total >= 0 && values.all(|entry| entry.bytes == total)).then_some(total)
}

pub fn explain_command() -> Result<String> {
    let directory = paths::snapshot_dir()?;
    let (before, after) = latest_two_from(&directory)?;
    let codex_standalone = detect_codex_standalone()?;
    Ok(render_explanation_with_codex(
        &before,
        &after,
        codex_standalone.as_ref(),
    ))
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

fn classify_contributors_with_codex(
    contributors: &[UsageChange],
    rules: &[Rule],
    codex_standalone: Option<&CodexStandalone>,
) -> Vec<(UsageChange, Classification)> {
    let mut classifications = classify_contributors(contributors, rules);
    if !codex_standalone.is_some_and(|codex| codex.releases.len() > 1) {
        return classifications;
    }

    let codex_classification = crate::classify::classify_path("~/.codex/packages", Some(rules));
    for (change, classification) in &mut classifications {
        if is_codex_runtime_change(change) {
            *classification = Classification {
                path: change.path.clone(),
                ..codex_classification.clone()
            };
        }
    }
    classifications
}

fn is_codex_runtime_change(change: &UsageChange) -> bool {
    change.path == "~/.codex/packages" || change.path.starts_with("~/.codex/packages/")
}
