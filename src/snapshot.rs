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

pub fn snapshot_command(verbose: bool) -> Result<String> {
    let snapshot = collect_snapshot()?;
    let path = save_snapshot(&snapshot, &paths::snapshot_dir()?)?;
    Ok(format_snapshot_message(&snapshot, &path, verbose))
}

fn format_snapshot_message(snapshot: &Snapshot, path: &Path, verbose: bool) -> String {
    let mut message = format!("Snapshot stored: {}", path.display());
    let warnings = if verbose {
        snapshot.warnings.iter().collect::<Vec<_>>()
    } else {
        snapshot
            .warnings
            .iter()
            .filter(|warning| !is_expected_warning(warning))
            .collect::<Vec<_>>()
    };
    if !warnings.is_empty() {
        message.push_str(&format!(
            "\nCompleted with {} ignored warning(s).",
            warnings.len()
        ));
        if verbose {
            message.push_str("\n\nIgnored warnings:");
            for warning in warnings {
                message.push_str(&format!("\n- {warning}"));
            }
        } else {
            message.push_str(
                "\nHint: re-run with `disk-agent snapshot --verbose` to view the ignored warnings.",
            );
        }
    }
    message
}

fn is_expected_warning(warning: &str) -> bool {
    warning.starts_with("du ") && warning.ends_with(": permission or read errors ignored")
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

#[cfg(test)]
mod tests {
    use super::format_snapshot_message;
    use crate::models::{FilesystemUsage, Snapshot};

    fn snapshot_with_warnings(warnings: Vec<&str>) -> Snapshot {
        Snapshot {
            timestamp: "2026-07-11T10:50:00+00:00".to_string(),
            filesystem: FilesystemUsage {
                filesystem: "/dev/vda".to_string(),
                mountpoint: "/".to_string(),
                total_bytes: 1000,
                used_bytes: 600,
                available_bytes: 400,
                used_percent: 60,
            },
            home_usage: Vec::new(),
            local_share_usage: Vec::new(),
            copilot_usage: Vec::new(),
            podman: Default::default(),
            largest_directories: Vec::new(),
            warnings: warnings.into_iter().map(str::to_string).collect(),
            schema_version: 1,
        }
    }

    #[test]
    fn default_snapshot_output_omits_warning_report_when_there_are_no_warnings() {
        let snapshot = snapshot_with_warnings(Vec::new());
        let output = format_snapshot_message(&snapshot, "/tmp/2026-07-11.json".as_ref(), false);

        assert_eq!(output, "Snapshot stored: /tmp/2026-07-11.json");
    }

    #[test]
    fn default_snapshot_output_omits_expected_warnings() {
        let snapshot = snapshot_with_warnings(vec![
            "du ~: permission or read errors ignored",
            "du ~/.local/share: permission or read errors ignored",
        ]);
        let output = format_snapshot_message(&snapshot, "/tmp/2026-07-11.json".as_ref(), false);

        assert_eq!(output, "Snapshot stored: /tmp/2026-07-11.json");
    }

    #[test]
    fn default_snapshot_output_reports_unexpected_warnings() {
        let snapshot = snapshot_with_warnings(vec![
            "du ~: permission or read errors ignored",
            "Path disappeared during scan: ~/gone",
        ]);
        let output = format_snapshot_message(&snapshot, "/tmp/2026-07-11.json".as_ref(), false);

        assert_eq!(
            output,
            "Snapshot stored: /tmp/2026-07-11.json\nCompleted with 1 ignored warning(s).\nHint: re-run with `disk-agent snapshot --verbose` to view the ignored warnings."
        );
    }

    #[test]
    fn verbose_snapshot_output_lists_ignored_warnings() {
        let snapshot = snapshot_with_warnings(vec![
            "du ~: permission or read errors ignored",
            "Path disappeared during scan: ~/gone",
        ]);
        let output = format_snapshot_message(&snapshot, "/tmp/2026-07-11.json".as_ref(), true);

        assert!(output.contains("Completed with 2 ignored warning(s)."));
        assert!(output.contains("Ignored warnings:\n- du ~: permission or read errors ignored"));
        assert!(output.contains("\n- Path disappeared during scan: ~/gone"));
    }
}
