use disk_agent::explain::render_explanation;
use disk_agent::models::{DirectoryUsage, FilesystemUsage, Snapshot};

const MIB: i64 = 1024 * 1024;

fn sample(day: u8, used_percent: i64, entries: Vec<(&str, i64)>) -> Snapshot {
    Snapshot {
        timestamp: format!("2026-06-{day:02}T10:50:00+00:00"),
        filesystem: FilesystemUsage {
            filesystem: "/dev/vda".to_string(),
            mountpoint: "/".to_string(),
            total_bytes: 100 * 1024 * MIB,
            used_bytes: used_percent * 1024 * MIB,
            available_bytes: (100 - used_percent) * 1024 * MIB,
            used_percent,
        },
        home_usage: entries
            .into_iter()
            .map(|(path, bytes)| DirectoryUsage {
                path: path.to_string(),
                bytes,
            })
            .collect(),
        local_share_usage: Vec::new(),
        copilot_usage: Vec::new(),
        podman: Default::default(),
        largest_directories: Vec::new(),
        warnings: Vec::new(),
        schema_version: 1,
    }
}

#[test]
fn explain_expands_home_growth_into_classified_contributors() {
    let before = sample(
        18,
        66,
        vec![
            ("~", 0),
            ("~/labs/archive", 0),
            ("~/.cache/pip", 0),
            ("~/Downloads", 0),
        ],
    );
    let after = sample(
        19,
        69,
        vec![
            ("~", 844 * MIB),
            ("~/labs/archive", 430 * MIB),
            ("~/.cache/pip", 180 * MIB),
            ("~/Downloads", 110 * MIB),
        ],
    );

    let output = render_explanation(&before, &after);

    assert!(output.contains("Disk usage increased from 66% to 69%."));
    assert!(output.contains("Top contributors:"));
    assert!(output.contains("  +430M ~/labs/archive"));
    assert!(output.contains("  +180M ~/.cache/pip"));
    assert!(output.contains("  +110M ~/Downloads"));
    assert!(!output.contains("+844M ~"));
    assert!(output.contains("Growth is primarily due to Development (+430M) and Cache (+180M)."));
    assert!(output.contains("Risk:\nLow"));
    assert!(output.contains("Review recent build artifacts or repository changes if unexpected."));
    assert!(output
        .contains("Cache growth is usually safe to inspect later; no cleanup is required now."));
    assert!(!output.contains("No matching rule is available"));
}

#[test]
fn explain_reports_unknown_when_contributors_are_unclassified() {
    let before = sample(18, 66, vec![("~", 0), ("~/mystery", 0)]);
    let after = sample(19, 69, vec![("~", 300 * MIB), ("~/mystery", 300 * MIB)]);

    let output = render_explanation(&before, &after);

    assert!(output.contains("Growth occurred in unclassified locations."));
    assert!(output.contains("Risk:\nUnknown"));
    assert!(output.contains("Inspect unclassified locations before taking cleanup action."));
}
