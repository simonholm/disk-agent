use disk_agent::json::load_snapshot;
use disk_agent::report::render_report;

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
