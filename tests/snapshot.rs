use disk_agent_rs::json::load_snapshot;
use disk_agent_rs::models::{FilesystemUsage, Snapshot};
use disk_agent_rs::snapshot::save_snapshot;

#[test]
fn saves_pretty_json_snapshot_with_daily_name_and_reads_it_back() {
    let directory = tempfile::tempdir().unwrap();
    let snapshot = Snapshot {
        timestamp: "2026-06-19T10:50:00+00:00".to_string(),
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
        warnings: Vec::new(),
        schema_version: 1,
    };

    let path = save_snapshot(&snapshot, directory.path()).unwrap();

    assert_eq!(path.file_name().unwrap(), "2026-06-19.json");
    assert_eq!(load_snapshot(&path).unwrap(), snapshot);
}
