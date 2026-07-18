use disk_agent::classify::classify_path;
use disk_agent::investigate::{assess, render_investigation};
use disk_agent::models::{DirectoryUsage, FilesystemUsage, PodmanUsage, Snapshot, UsageChange};
use disk_agent::rules::load_rules;

const SUPPORTED_ASSESSMENTS: &[&str] = &[
    "Healthy",
    "Cache growth expected",
    "Build artifacts accumulating",
    "Container storage increasing",
    "Large unclassified growth",
    "Investigation recommended",
];

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
    let mut before = sample(19, 60, 0);
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

    let output = render_investigation(Some(&before), &after);

    assert!(output.contains("Current filesystem usage"));
    assert!(output.contains("/dev/vda mounted at /: 62% used"));
    assert!(output.contains("Largest consumers"));
    assert!(!output.contains("Recently active areas"));
    assert!(output.contains("Changes since today's snapshot"));
    assert!(output.contains("Podman status"));
    assert!(output.contains("+838M ~/.codex/packages"));
    assert!(output.contains("Application releases"));
    assert!(output.contains("Risk: Low"));
    assert!(output.contains("Assessment"));
    assert!(output.contains("Healthy"));
    assert!(output.contains("Recommendations"));
    assert!(output.contains("No action required."));
    assert!(SUPPORTED_ASSESSMENTS.contains(&assessment_line(&output)));
    assert!(!output.contains("Growth:"));
    assert!(!output.contains("Shrinkage:"));
    assert!(!output.contains("Snapshot interval"));
}

#[test]
fn investigation_omits_change_section_without_same_day_snapshot() {
    let output = render_investigation(None, &sample(19, 62, 0));

    assert!(!output.contains("Recently active areas"));
    assert!(!output.contains("Changes since today's snapshot"));
    assert!(output.contains("Current filesystem usage"));
    assert!(output.contains("Assessment"));
}

#[test]
fn cache_assessment_has_cache_recommendation() {
    let after = sample(19, 62, 3 * 1024_i64.pow(3));
    let output = render_investigation(None, &after);

    assert_eq!(assessment_line(&output), "Cache growth expected");
    assert!(output.contains("No cleanup required unless disk space becomes constrained."));
    assert!(!output.contains("No action required."));
}

#[test]
fn container_assessment_has_container_recommendation() {
    let mut before = sample(19, 62, 0);
    before.podman = PodmanUsage {
        available: true,
        images_bytes: Some(0),
        containers_bytes: Some(0),
        volumes_bytes: Some(0),
        error: None,
    };
    let mut after = sample(19, 62, 0);
    after.podman = PodmanUsage {
        available: true,
        images_bytes: Some(2 * 1024_i64.pow(3)),
        containers_bytes: Some(0),
        volumes_bytes: Some(0),
        error: None,
    };

    let output = render_investigation(Some(&before), &after);

    assert_eq!(assessment_line(&output), "Container storage increasing");
    assert!(output.contains("Review Podman images and containers if the growth is unexpected."));
    assert!(!output.contains("No action required."));
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
            &after,
            &growth,
            &classifications,
            &[],
            &Default::default(),
            false
        ),
        "Large unclassified growth"
    );
}

#[test]
fn investigation_recommended_has_investigation_recommendation() {
    let mut after = sample(19, 86, 0);
    after.largest_directories.push(DirectoryUsage {
        path: "~/data".to_string(),
        bytes: 700 * 1024 * 1024,
    });

    let output = render_investigation(None, &after);

    assert_eq!(assessment_line(&output), "Investigation recommended");
    assert!(output
        .contains("Review the listed directories to determine whether current usage is expected."));
    assert!(!output.contains("No action required."));
}

fn assessment_line(output: &str) -> &str {
    output
        .lines()
        .skip_while(|line| *line != "Assessment")
        .nth(2)
        .expect("assessment value follows the Assessment heading")
}
