use disk_agent::json::load_snapshot;
use disk_agent::report::{render_report, render_report_with_metadata};

#[test]
fn report_renders_loaded_snapshot_summary() {
    let snapshot = load_snapshot("tests/fixtures/snapshot_full.json".as_ref()).unwrap();
    let output = render_report(&snapshot);

    assert!(output.contains("Filesystem usage: 60% (600B of 1000B)"));
    assert!(output.contains("Directories (nested sizes overlap):"));
    assert!(output.contains("200M  ~/labs"));
    assert!(output.contains("100M  ~/.cache"));
    assert!(output.contains("Podman:"));
    assert!(output.contains("images 100B"));
    assert!(output.contains("containers 200B"));
    assert!(output.contains("volumes 300B"));
    assert!(!output.contains("Largest directories:"));
    assert_eq!(output.matches("~/labs").count(), 1);
    assert!(output.contains("Observations:"));
    assert!(output.contains("Filesystem usage is below the elevated threshold (80%)."));
    assert!(!output.contains("No action required."));
}

#[test]
fn report_identifies_snapshot_metadata() {
    let snapshot = load_snapshot("tests/fixtures/snapshot_full.json".as_ref()).unwrap();
    let output = render_report_with_metadata(
        &snapshot,
        "~/.disk-agent/snapshots/2026-06-19.json".as_ref(),
    );

    assert!(output.starts_with("Filesystem usage: 60% (600B of 1000B)\nSnapshot age:"));
    assert!(output.contains("Stored snapshot — current usage was not measured"));
    assert!(output.contains("Saved: 2026-06-19T10:50:00+00:00\n"));
    assert!(output.contains("Snapshot age:"));
    assert!(output.contains("disk-agent report --refresh"));
    assert!(output.contains("disk-agent investigate"));
    assert!(output.contains("Source: ~/.disk-agent/snapshots/2026-06-19.json"));
    assert!(output.contains("Filesystem usage: 60% (600B of 1000B)"));
}

#[test]
fn report_age_handles_elapsed_invalid_and_future_timestamps() {
    use disk_agent::report::render_report_with_metadata_at;
    let mut snapshot = load_snapshot("tests/fixtures/snapshot_full.json".as_ref()).unwrap();
    let now = chrono::DateTime::parse_from_rfc3339("2026-06-21T13:54:00+00:00")
        .unwrap()
        .with_timezone(&chrono::Utc);
    let render = |snapshot: &disk_agent::models::Snapshot| {
        render_report_with_metadata_at(snapshot, "saved.json".as_ref(), now)
    };
    assert!(render(&snapshot).contains("Snapshot age: 2d 3h 4m; current usage may differ."));
    snapshot.timestamp = "invalid".into();
    assert!(render(&snapshot).contains("unknown (invalid snapshot timestamp)"));
    snapshot.timestamp = "2026-06-22T10:50:00+00:00".into();
    assert!(render(&snapshot).contains("unknown (snapshot timestamp is in the future)"));
}

#[test]
fn report_flags_large_stored_version_sizes_without_claiming_reclaimability() {
    use disk_agent::models::DirectoryUsage;
    let mut snapshot = load_snapshot("tests/fixtures/snapshot_full.json".as_ref()).unwrap();
    snapshot.filesystem.used_percent = 79;
    snapshot.home_usage.extend([
        DirectoryUsage {
            path: "~/.codex".into(),
            bytes: 6_000_000_000,
        },
        DirectoryUsage {
            path: "~/.codex/packages".into(),
            bytes: 5_500_000_000,
        },
    ]);
    snapshot.largest_directories.push(DirectoryUsage {
        path: "~/.codex/packages/standalone/releases".into(),
        bytes: 2_000_000_000,
    });
    snapshot.local_share_usage.push(DirectoryUsage {
        path: "~/.local/share/claude/versions".into(),
        bytes: 1_000_000_000,
    });
    snapshot.copilot_usage.push(DirectoryUsage {
        path: "~/.copilot/pkg/linux-x64".into(),
        bytes: 1_000_000_000,
    });
    let output = render_report(&snapshot);
    assert!(output.contains("Filesystem usage is below the elevated threshold"));
    assert_eq!(
        output
            .matches("Substantial version-store storage in snapshot:")
            .count(),
        1
    );
    for path in [
        "~/.codex/packages",
        "~/.local/share/claude/versions",
        "~/.copilot/pkg/linux-x64",
    ] {
        assert!(output.contains(path));
    }
    assert!(output.contains("Reclaimability is unknown"));
    assert!(output.contains("disk-agent investigate"));
    assert!(!output.contains("potentially reclaimable"));
    assert!(!output.contains("No action required"));
    for percent in [80, 90] {
        snapshot.filesystem.used_percent = percent;
        assert!(render_report(&snapshot).contains(if percent == 80 {
            "Disk usage is elevated."
        } else {
            "Disk usage is critical."
        }));
    }
}

#[test]
fn large_persistent_state_alone_is_not_treated_as_version_storage() {
    let mut snapshot = load_snapshot("tests/fixtures/snapshot_full.json".as_ref()).unwrap();
    snapshot
        .home_usage
        .push(disk_agent::models::DirectoryUsage {
            path: "~/.codex".into(),
            bytes: 6_000_000_000,
        });
    assert!(!render_report(&snapshot).contains("Substantial version-store"));
}

#[test]
fn report_keeps_collection_warnings_and_unavailable_measurements_at_80_columns() {
    let mut snapshot = load_snapshot("tests/fixtures/snapshot_full.json".as_ref()).unwrap();
    snapshot.warnings = vec![format!(
        "du: {}: permission denied",
        "long directory name ".repeat(8)
    )];
    snapshot.podman.images_bytes = None;
    let output = render_report(&snapshot);
    assert!(output.contains("images unavailable"));
    assert!(output.contains("Collection warnings:"));
    assert!(output.contains("permission denied"));
    assert!(output.lines().all(|line| line.chars().count() <= 80));
    snapshot.podman.available = false;
    snapshot.podman.error = Some("permission denied".into());
    snapshot.home_usage.clear();
    snapshot.largest_directories.clear();
    let output = render_report(&snapshot);
    assert!(output.contains("Podman: unavailable (permission denied)"));
    assert!(output.contains("No directory data available."));
}
