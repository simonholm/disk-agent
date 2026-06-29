use disk_agent_rs::diff::{compare_snapshots, render_diff, SIGNIFICANT_BYTES};
use disk_agent_rs::models::{DirectoryUsage, FilesystemUsage, Snapshot};

fn sample(day: u8, used_percent: i64, cache_bytes: i64) -> Snapshot {
    Snapshot {
        timestamp: format!("2026-06-{day:02}T10:50:00+00:00"),
        filesystem: FilesystemUsage {
            filesystem: "/dev/vda".to_string(),
            mountpoint: "/".to_string(),
            total_bytes: 1000,
            used_bytes: used_percent * 10,
            available_bytes: 1000 - used_percent * 10,
            used_percent,
        },
        home_usage: vec![
            DirectoryUsage {
                path: "~".to_string(),
                bytes: 500,
            },
            DirectoryUsage {
                path: "~/.cache".to_string(),
                bytes: cache_bytes,
            },
        ],
        local_share_usage: Vec::new(),
        copilot_usage: Vec::new(),
        podman: Default::default(),
        largest_directories: vec![DirectoryUsage {
            path: "~/.cache".to_string(),
            bytes: cache_bytes,
        }],
        warnings: Vec::new(),
        schema_version: 1,
    }
}

#[test]
fn diff_ignores_changes_under_50_mib() {
    let before = sample(18, 60, 10);
    let after = sample(19, 61, 49 * 1024 * 1024);

    assert_eq!(compare_snapshots(&before, &after), []);
}

#[test]
fn diff_includes_threshold_and_sorts_by_absolute_size() {
    let mut before = sample(18, 60, 0);
    before.home_usage.extend([
        DirectoryUsage {
            path: "~/.removed".to_string(),
            bytes: 200 * 1024 * 1024,
        },
        DirectoryUsage {
            path: "~/.exact".to_string(),
            bytes: 0,
        },
    ]);
    let mut after = sample(19, 61, 100 * 1024 * 1024);
    after.home_usage.push(DirectoryUsage {
        path: "~/.exact".to_string(),
        bytes: SIGNIFICANT_BYTES,
    });

    let changes = compare_snapshots(&before, &after);

    assert_eq!(
        changes
            .iter()
            .map(|change| (change.path.as_str(), change.bytes))
            .collect::<Vec<_>>(),
        vec![
            ("~/.removed", -200 * 1024 * 1024),
            ("~/.cache", 100 * 1024 * 1024),
            ("~/.exact", SIGNIFICANT_BYTES),
        ]
    );
}

#[test]
fn diff_renders_growth_and_shrinkage() {
    let mut before = sample(18, 60, 200 * 1024 * 1024);
    before.home_usage.push(DirectoryUsage {
        path: "~/.growing".to_string(),
        bytes: 0,
    });
    let mut after = sample(19, 61, 100 * 1024 * 1024);
    after.home_usage.push(DirectoryUsage {
        path: "~/.growing".to_string(),
        bytes: 75 * 1024 * 1024,
    });

    let output = render_diff(&before, &after);

    assert!(output.contains("2026-06-18 → 2026-06-19"));
    assert!(output.contains("Growth:"));
    assert!(output.contains("+75M ~/.growing"));
    assert!(output.contains("Shrinkage:"));
    assert!(output.contains("-100M ~/.cache"));
}
