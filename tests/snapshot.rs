use disk_agent::diff::latest_two_from;
use disk_agent::json::load_snapshot;
use disk_agent::models::{FilesystemUsage, Snapshot};
use disk_agent::report::snapshot_paths;
use disk_agent::snapshot::save_snapshot;

#[test]
fn saves_pretty_json_snapshot_with_timestamped_name_and_reads_it_back() {
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

    assert_eq!(path.file_name().unwrap(), "2026-06-19_10-50-00.json");
    assert_eq!(load_snapshot(&path).unwrap(), snapshot);
}

#[test]
fn discovers_timestamped_snapshots_chronologically_for_diff() {
    let directory = tempfile::tempdir().unwrap();
    let mut first = sample_snapshot("2026-06-19T10:50:00+00:00");
    first.filesystem.used_percent = 60;
    let mut second = sample_snapshot("2026-06-19T12:50:00+00:00");
    second.filesystem.used_percent = 61;
    let third = sample_snapshot("2026-06-20T08:50:00+00:00");

    let first_path = save_snapshot(&first, directory.path()).unwrap();
    std::fs::rename(first_path, directory.path().join("2026-06-19.json")).unwrap();
    save_snapshot(&second, directory.path()).unwrap();
    save_snapshot(&third, directory.path()).unwrap();

    let names = snapshot_paths(directory.path())
        .unwrap()
        .into_iter()
        .map(|path| path.file_name().unwrap().to_owned())
        .collect::<Vec<_>>();
    assert_eq!(
        names,
        [
            "2026-06-19.json",
            "2026-06-19_12-50-00.json",
            "2026-06-20_08-50-00.json",
        ]
    );

    let (before, after) = latest_two_from(directory.path()).unwrap();
    assert_eq!(before.timestamp, second.timestamp);
    assert_eq!(after.timestamp, third.timestamp);
}

fn sample_snapshot(timestamp: &str) -> Snapshot {
    Snapshot {
        timestamp: timestamp.to_string(),
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
    }
}
