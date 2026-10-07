use disk_agent::codex::{CodexRelease, CodexStandalone};
use disk_agent::explain::{render_explanation, render_explanation_with_codex};
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
fn explain_attributes_shared_cargo_target_and_registry_growth_to_rust() {
    let before = sample(
        18,
        64,
        vec![
            ("~", 0),
            ("~/.cargo-target", 0),
            ("~/.cargo-target/release", 0),
            ("~/.cargo", 0),
            ("~/.cargo/registry", 0),
        ],
    );
    let after = sample(
        19,
        68,
        vec![
            ("~", 1680 * MIB),
            ("~/.cargo-target", 1400 * MIB),
            ("~/.cargo-target/release", 1400 * MIB),
            ("~/.cargo", 280 * MIB),
            ("~/.cargo/registry", 262 * MIB),
        ],
    );

    let diff = disk_agent::diff::render_diff(&before, &after);
    assert!(diff.contains("+1.4G ~/.cargo-target\n"));
    assert!(diff.contains("+1.4G ~/.cargo-target/release\n"));
    assert!(diff.contains("+280M ~/.cargo\n"));
    assert!(diff.contains("+262M ~/.cargo/registry\n"));

    let output = render_explanation(&before, &after);
    assert!(output.contains("  +1.4G ~/.cargo-target/release\n"));
    assert!(output.contains("  +280M ~/.cargo\n"));
    assert!(output.contains("Growth is primarily due to Rust (+1.6G)."));
    assert!(!output.contains("unclassified"));
    assert!(!output.contains("Unclassified growth"));
    assert!(output.contains("Risk:\nLow"));
}

#[test]
fn explain_reports_unknown_when_contributors_are_unclassified() {
    let before = sample(18, 66, vec![("~", 0), ("~/mystery", 0)]);
    let after = sample(19, 69, vec![("~", 300 * MIB), ("~/mystery", 300 * MIB)]);

    let output = render_explanation(&before, &after);

    assert!(output.contains("Growth occurred in unclassified locations."));
    assert!(output.contains("Unclassified growth (+300M):"));
    assert!(output.contains("  +300M ~/mystery"));
    assert!(output.contains("Risk:\nUnknown"));
    assert!(output.contains("Inspect unclassified locations before taking cleanup action."));
}

#[test]
fn explain_unclassified_total_matches_listed_paths() {
    let before = sample(
        18,
        66,
        vec![("~", 0), ("~/mystery", 0), ("~/.local/bin", 0)],
    );
    let after = sample(
        19,
        69,
        vec![
            ("~", 500 * MIB),
            ("~/mystery", 300 * MIB),
            ("~/.local/bin", 200 * MIB),
        ],
    );

    let output = render_explanation(&before, &after);

    assert!(output.contains("Unclassified growth (+500M):"));
    assert!(output.contains("  +300M ~/mystery"));
    assert!(output.contains("  +200M ~/.local/bin"));
}

#[test]
fn explain_category_totals_do_not_double_count_nested_paths() {
    let before = sample(
        18,
        66,
        vec![
            ("~", 0),
            ("~/labs", 0),
            ("~/labs/repos", 0),
            ("~/.cache/uv", 0),
        ],
    );
    let after = sample(
        19,
        69,
        vec![
            ("~", 1300 * MIB),
            ("~/labs", 1000 * MIB),
            ("~/labs/repos", 700 * MIB),
            ("~/.cache/uv", 300 * MIB),
        ],
    );

    let output = render_explanation(&before, &after);

    assert!(output.contains("Growth is primarily due to Development (+1000M) and Cache (+300M)."));
    assert!(!output.contains("Development (+1.7G)"));
    assert!(!output.contains("  +1300M ~"));
}

#[test]
fn explain_recommends_native_uv_cache_clean_for_uv_cache_growth() {
    let before = sample(18, 66, vec![("~", 0), ("~/.cache/uv", 0)]);
    let after = sample(19, 69, vec![("~", 1300 * MIB), ("~/.cache/uv", 1300 * MIB)]);

    let output = render_explanation(&before, &after);

    assert!(output.contains("  +1.3G ~/.cache/uv"));
    assert!(output.contains("Growth is primarily due to Cache (+1.3G)."));
    assert!(output.contains("Run `uv cache clean` if reclaiming uv cache storage is desired."));
}

#[test]
fn explain_does_not_treat_uv_local_share_as_cache() {
    let before = sample(18, 66, vec![("~", 0), ("~/.local/share/uv", 0)]);
    let after = sample(
        19,
        69,
        vec![("~", 1300 * MIB), ("~/.local/share/uv", 1300 * MIB)],
    );

    let output = render_explanation(&before, &after);

    assert!(output.contains("  +1.3G ~/.local/share/uv"));
    assert!(output.contains("Growth is primarily due to Application data (+1.3G)."));
    assert!(output.contains("this is not classified as disposable cache"));
    assert!(!output.contains("uv cache clean"));
}

#[test]
fn explain_prefers_claude_version_growth_over_its_unclassified_local_parent() {
    let mut before = sample(66, 66, vec![("~", 0), ("~/.local", 0)]);
    let mut after = sample(67, 67, vec![("~", 233_700_000), ("~/.local", 233_700_000)]);
    before.local_share_usage = vec![DirectoryUsage {
        path: "~/.local/share/claude/versions".to_string(),
        bytes: 645_840_896,
    }];
    after.local_share_usage = vec![DirectoryUsage {
        path: "~/.local/share/claude/versions".to_string(),
        bytes: 874_512_384,
    }];

    let output = render_explanation(&before, &after);

    assert!(output.contains("~/.local/share/claude/versions"));
    assert!(output.contains("Application releases"));
    assert!(!output.contains("Unclassified growth"));
}

#[test]
fn explain_handles_claude_growth_larger_than_its_local_parent_delta() {
    let mut before = sample(66, 66, vec![("~", 800_000_000), ("~/.local", 800_000_000)]);
    let mut after = sample(
        67,
        67,
        vec![("~", 1_000_000_000), ("~/.local", 1_000_000_000)],
    );
    before.local_share_usage = vec![DirectoryUsage {
        path: "~/.local/share/claude/versions".to_string(),
        bytes: 645_840_896,
    }];
    after.local_share_usage = vec![DirectoryUsage {
        path: "~/.local/share/claude/versions".to_string(),
        bytes: 874_512_384,
    }];

    let output = render_explanation(&before, &after);

    assert!(output.contains("~/.local/share/claude/versions"));
    assert!(!output.contains("Unclassified growth"));
}

fn assert_codex_package_growth_recommendation(path: &str) {
    let before = sample(18, 66, vec![("~", 0), (path, 0)]);
    let after = sample(19, 69, vec![("~", 1536 * MIB), (path, 1536 * MIB)]);

    let output = render_explanation(&before, &after);

    assert!(output.contains(&format!("+1.5G {path}")));
    assert!(output.contains("Codex release/package growth (+1.5G)"));
    assert!(output.contains("codex-cache report"));
    assert!(output.contains("codex-cache clean --dry-run --keep current,previous"));
}

#[test]
fn explain_recommends_codex_cache_for_standalone_release_growth() {
    assert_codex_package_growth_recommendation("~/.codex/packages/standalone/releases");
}

#[test]
fn explain_recommends_codex_cache_for_app_server_daemon_release_growth() {
    assert_codex_package_growth_recommendation("~/.codex/packages/app-server-daemon/releases");
}

#[test]
fn explain_attributes_codex_parent_growth_to_measured_package_growth() {
    let before = sample(
        18,
        66,
        vec![("~", 0), ("~/.codex", 0), ("~/.codex/packages", 0)],
    );
    let after = sample(
        19,
        69,
        vec![
            ("~", 1536 * MIB),
            ("~/.codex", 1536 * MIB),
            ("~/.codex/packages", 1024 * MIB),
        ],
    );

    let output = render_explanation(&before, &after);

    assert!(output.contains("+1G ~/.codex/packages"));
    assert!(output.contains("Codex release/package growth (+1G)"));
    assert!(output.contains("codex-cache report"));
    assert!(!output.contains("Codex release/package growth (+1.5G)"));
}

#[test]
fn explain_attributes_copilot_parent_growth_to_retained_package_versions() {
    let before = sample(
        18,
        66,
        vec![
            ("~", 290 * MIB),
            ("~/.copilot", 290 * MIB),
            ("~/.copilot/pkg/linux-x64", 290 * MIB),
            ("~/.copilot/pkg/linux-x64/1.0.82", 147 * MIB),
            ("~/.copilot/pkg/linux-x64/1.0.83", 143 * MIB),
        ],
    );
    let after = sample(
        19,
        69,
        vec![
            ("~", 773 * MIB),
            ("~/.copilot", 773 * MIB),
            ("~/.copilot/pkg/linux-x64", 773 * MIB),
            ("~/.copilot/pkg/linux-x64/1.0.82", 147 * MIB),
            ("~/.copilot/pkg/linux-x64/1.0.83", 143 * MIB),
            ("~/.copilot/pkg/linux-x64/1.0.85", 158 * MIB),
            ("~/.copilot/pkg/linux-x64/1.0.86", 159 * MIB),
            ("~/.copilot/pkg/linux-x64/1.0.88", 166 * MIB),
        ],
    );

    let output = render_explanation(&before, &after);

    assert!(output.contains("+483M ~/.copilot/pkg/linux-x64"));
    assert!(output.contains("Copilot CLI release/package growth (+483M)"));
    assert!(output
        .contains("Inspect retained Copilot CLI package versions under ~/.copilot/pkg/linux-x64"));
    assert!(!output.contains("Application runtime (+483M)"));
    assert!(!output.contains("copilot clean"));
}

#[test]
fn explain_keeps_unrelated_copilot_state_growth_as_application_runtime() {
    let before = sample(
        18,
        66,
        vec![
            ("~", 300 * MIB),
            ("~/.copilot", 300 * MIB),
            ("~/.copilot/pkg/linux-x64", 300 * MIB),
            ("~/.copilot/pkg/linux-x64/1.0.86", 150 * MIB),
            ("~/.copilot/pkg/linux-x64/1.0.88", 150 * MIB),
        ],
    );
    let after = sample(
        19,
        69,
        vec![
            ("~", 900 * MIB),
            ("~/.copilot", 900 * MIB),
            ("~/.copilot/pkg/linux-x64", 300 * MIB),
            ("~/.copilot/pkg/linux-x64/1.0.86", 150 * MIB),
            ("~/.copilot/pkg/linux-x64/1.0.88", 150 * MIB),
        ],
    );

    let output = render_explanation(&before, &after);

    assert!(output.contains("Application runtime (+600M)"));
    assert!(!output.contains("Copilot CLI release/package growth"));
    assert!(!output.contains("Inspect retained Copilot CLI package versions"));
}

#[test]
fn explain_omits_codex_cache_recommendation_below_growth_threshold() {
    let path = "~/.codex/packages/standalone/releases";
    let before = sample(18, 66, vec![("~", 0), (path, 0)]);
    let after = sample(19, 66, vec![("~", 49 * MIB), (path, 49 * MIB)]);

    let output = render_explanation(&before, &after);

    assert!(output.contains("No significant directory growth."));
    assert!(!output.contains("codex-cache"));
}

#[test]
fn explain_does_not_recommend_codex_cache_for_unrelated_application_growth() {
    let before = sample(
        18,
        66,
        vec![("~", 0), ("~/.local/share/claude/versions", 0)],
    );
    let after = sample(
        19,
        69,
        vec![
            ("~", 1536 * MIB),
            ("~/.local/share/claude/versions", 1536 * MIB),
        ],
    );

    let output = render_explanation(&before, &after);

    assert!(output.contains("Application releases (+1.5G)"));
    assert!(!output.contains("Codex release/package growth"));
    assert!(!output.contains("codex-cache"));
}

#[test]
fn explain_does_not_attribute_codex_state_growth_to_packages() {
    let before = sample(18, 66, vec![("~", 0), ("~/.codex", 0)]);
    let after = sample(
        19,
        69,
        vec![("~", 3 * 1024 * MIB), ("~/.codex", 3 * 1024 * MIB)],
    );
    let codex = CodexStandalone {
        current_release: Some("0.145.0".to_string()),
        releases: vec![
            CodexRelease {
                name: "0.144.6".to_string(),
                bytes: 1024,
            },
            CodexRelease {
                name: "0.145.0".to_string(),
                bytes: 2048,
            },
        ],
    };

    let output = render_explanation_with_codex(&before, &after, Some(&codex));

    assert!(output.contains("Growth is primarily due to Application data (+3G)."));
    assert!(output.contains("Risk:\nLow"));
    assert!(!output.contains("codex-cache"));
    assert!(!output.contains("Growth occurred in unclassified locations."));
    assert!(!output.contains("Inspect unclassified locations before taking cleanup action."));
}

#[test]
fn explain_omits_stale_unclassified_recommendation_for_small_residual_growth() {
    let before = sample(
        18,
        66,
        vec![
            ("~", 0),
            ("~/.copilot", 0),
            ("~/.codex/packages", 0),
            ("~/other", 0),
        ],
    );
    let after = sample(
        19,
        69,
        vec![
            ("~", 1050 * MIB),
            ("~/.copilot", 570 * MIB),
            ("~/.codex/packages", 358 * MIB),
            ("~/other", 122 * MIB),
        ],
    );
    let codex = CodexStandalone {
        current_release: Some("0.145.0".to_string()),
        releases: vec![
            CodexRelease {
                name: "0.144.6".to_string(),
                bytes: 1024,
            },
            CodexRelease {
                name: "0.145.0".to_string(),
                bytes: 2048,
            },
        ],
    };

    let output = render_explanation_with_codex(&before, &after, Some(&codex));

    assert!(output.contains(
        "Growth is primarily due to Application runtime (+570M) and Codex release/package growth (+358M)."
    ));
    assert!(output.contains("Review the Copilot installation only if the growth is unexpected."));
    assert!(output.contains("codex-cache report"));
    assert!(!output.contains("Inspect unclassified locations before taking cleanup action."));
}

#[test]
fn explain_reports_low_risk_for_classified_rust_and_node_growth() {
    let before = sample(
        18,
        59,
        vec![
            ("~", 0),
            ("~/.cargo-target", 0),
            ("~/.nvm/versions", 0),
            ("~/.npm", 0),
        ],
    );
    let after = sample(
        19,
        61,
        vec![
            ("~", 1105 * MIB),
            ("~/.cargo-target", 565 * MIB),
            ("~/.nvm/versions", 350 * MIB),
            ("~/.npm", 190 * MIB),
        ],
    );

    let output = render_explanation(&before, &after);

    assert!(output.contains("  +565M ~/.cargo-target"));
    assert!(output.contains("  +350M ~/.nvm/versions"));
    assert!(output.contains("  +190M ~/.npm"));
    assert!(output.contains("Growth is primarily due to Rust (+565M) and Node (+540M)."));
    assert!(output.contains("Risk:\nLow"));
    assert!(!output.contains("Risk:\nUnknown"));
    assert!(!output.contains("unclassified locations"));
}
