use disk_agent::json::load_snapshot;
use disk_agent::report::{render_report, render_report_with_metadata};

#[test]
fn report_renders_loaded_snapshot_summary() {
    let snapshot = load_snapshot("tests/fixtures/snapshot_full.json".as_ref()).unwrap();
    let output = render_report(&snapshot);

    assert!(output.contains("Filesystem usage: 60% (600B of 1000B)"));
    assert!(output.contains("Top consumers:"));
    assert!(output.contains("200M ~/labs"));
    assert!(output.contains("100M ~/.cache"));
    assert!(output.contains("Podman:"));
    assert!(output.contains("Images: 100B"));
    assert!(output.contains("Containers: 200B"));
    assert!(output.contains("Volumes: 300B"));
    assert!(output.contains("Largest directories:"));
    assert!(output.contains("Assessment:"));
    assert!(output.contains("No action required."));
}

#[test]
fn report_identifies_snapshot_metadata() {
    let snapshot = load_snapshot("tests/fixtures/snapshot_full.json".as_ref()).unwrap();
    let output = render_report_with_metadata(
        &snapshot,
        "~/.disk-agent/snapshots/2026-06-19.json".as_ref(),
    );

    assert!(output.starts_with("Snapshot: saved 2026-06-19T10:50:00+00:00\n"));
    assert!(output.contains("Source: ~/.disk-agent/snapshots/2026-06-19.json"));
    assert!(output.contains("Filesystem usage: 60% (600B of 1000B)"));
}
