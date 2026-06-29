use disk_agent_rs::classify::classify_path;
use disk_agent_rs::investigate::{assess, render_investigation};
use disk_agent_rs::models::{DirectoryUsage, FilesystemUsage, Snapshot, UsageChange};
use disk_agent_rs::rules::load_rules;

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
fn investigation_reads_like_operational_report() {
    let mut before = sample(18, 60, 0);
    before.home_usage.push(DirectoryUsage {
        path: "~/.codex".to_string(),
        bytes: 100 * 1024 * 1024,
    });
    let mut after = sample(19, 62, 0);
    after.home_usage.extend([
        DirectoryUsage {
            path: "~/.codex".to_string(),
            bytes: 950 * 1024 * 1024,
        },
        DirectoryUsage {
            path: "~/.codex/packages".to_string(),
            bytes: 838 * 1024 * 1024,
        },
    ]);
    after.largest_directories.extend([
        DirectoryUsage {
            path: "~/.codex/packages".to_string(),
            bytes: 838 * 1024 * 1024,
        },
        DirectoryUsage {
            path: "~/.codex/packages/0.142.0".to_string(),
            bytes: 250 * 1024 * 1024,
        },
        DirectoryUsage {
            path: "~/.codex/packages/0.142.2".to_string(),
            bytes: 280 * 1024 * 1024,
        },
        DirectoryUsage {
            path: "~/.codex/packages/0.142.3".to_string(),
            bytes: 308 * 1024 * 1024,
        },
    ]);

    let output = render_investigation(&before, &after, None);

    assert!(output.contains("Filesystem usage: 62%"));
    assert!(output.contains("+838M ~/.codex/packages"));
    assert!(output.contains("Application releases"));
    assert!(output.contains("3 retained Codex releases"));
    assert!(output.contains("Risk: Low"));
    assert!(output.contains("Assessment"));
    assert!(output.contains("Healthy"));
    assert!(output.contains("Review retained Codex releases."));
}

#[test]
fn assessment_escalates_unknown_large_growth() {
    let before = sample(18, 60, 0);
    let mut after = sample(19, 61, 0);
    after.filesystem.used_bytes = before.filesystem.used_bytes + 2 * 1024_i64.pow(3);
    let growth = vec![UsageChange {
        path: "~/unknown".to_string(),
        bytes: 2 * 1024_i64.pow(3),
    }];
    let rules = load_rules();
    let classifications = [(
        "~/unknown".to_string(),
        classify_path("~/unknown", Some(&rules)),
    )]
    .into_iter()
    .collect();

    assert_eq!(
        assess(
            &before,
            &after,
            &growth,
            &classifications,
            &Default::default()
        ),
        "Attention Recommended"
    );
}
