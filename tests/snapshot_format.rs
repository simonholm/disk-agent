use disk_agent::json::load_snapshot;

#[test]
fn reads_snapshot_with_all_current_fields() {
    let snapshot = load_snapshot("tests/fixtures/snapshot_full.json".as_ref()).unwrap();

    assert_eq!(snapshot.timestamp, "2026-06-19T10:50:00+00:00");
    assert_eq!(snapshot.schema_version, 1);
    assert_eq!(snapshot.filesystem.used_percent, 60);
    assert_eq!(snapshot.home_usage.len(), 3);
    assert!(snapshot.podman.available);
    assert_eq!(snapshot.podman.images_bytes, Some(100));
}

#[test]
fn defaults_missing_optional_snapshot_fields() {
    let snapshot = load_snapshot("tests/fixtures/snapshot_minimal.json".as_ref()).unwrap();

    assert_eq!(snapshot.schema_version, 1);
    assert!(snapshot.home_usage.is_empty());
    assert!(snapshot.local_share_usage.is_empty());
    assert!(snapshot.copilot_usage.is_empty());
    assert!(snapshot.largest_directories.is_empty());
    assert!(snapshot.warnings.is_empty());
    assert!(!snapshot.podman.available);
    assert_eq!(snapshot.podman.images_bytes, None);
}
