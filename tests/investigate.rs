use disk_agent::cargo::CargoTargetDiagnostic;
use disk_agent::classify::classify_path;
use disk_agent::codex::{CodexRelease, CodexStandalone, DAEMON_STORE, STANDALONE_STORE};
use disk_agent::investigate::{
    assess, render_investigation, render_investigation_with_codex,
    render_investigation_with_codex_packages, render_investigation_with_diagnostics,
    render_investigation_with_release_stores,
};
use disk_agent::models::{
    DirectoryUsage, FilesystemUsage, PodmanContainerUsage, PodmanUsage, Snapshot, UsageChange,
};
use disk_agent::release_store::{ReleaseEntry, ReleaseStore};
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
    assert!(output.contains("950M ~/.codex (Application data)"));
    assert!(!output.contains("Recently active areas"));
    assert!(output.contains("Changes since today's snapshot"));
    assert!(output.contains("Podman status"));
    assert!(output.contains("+838M ~/.codex/packages"));
    assert!(output.contains("Codex release/package growth"));
    assert!(output.contains("Risk: Low"));
    assert!(output.contains("Assessment"));
    assert!(output.contains("Healthy"));
    assert!(output.contains("Recommendations"));
    assert!(output.contains("codex-cache report"));
    assert!(output.contains("codex-cache clean --dry-run --keep current,previous"));
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
fn investigation_omits_codex_section_for_one_standalone_release() {
    let codex = CodexStandalone {
        current_release: Some("0.144.6".to_string()),
        releases: vec![CodexRelease {
            name: "0.144.6".to_string(),
            bytes: 10,
        }],
    };

    let output = render_investigation_with_codex(None, &sample(19, 62, 0), Some(&codex));

    assert!(!output.contains("Codex\n"));
}

#[test]
fn investigation_reports_codex_package_generations_conservatively() {
    const MIB: i64 = 1024 * 1024;
    let stores = [STANDALONE_STORE, DAEMON_STORE]
        .into_iter()
        .map(|name| ReleaseStore {
            name: name.to_string(),
            entries: ["0.157.0", "0.157.1"]
                .into_iter()
                .map(|version| ReleaseEntry {
                    name: format!("{version}-x86_64-unknown-linux-musl"),
                    bytes: 374 * MIB,
                })
                .collect(),
            active_entry: None,
            current_pointer_configured: false,
        })
        .collect::<Vec<_>>();
    let snapshot = sample(19, 62, 0);
    let output =
        render_investigation_with_codex_packages(None, &snapshot, &stores, Some("0.157.1"));
    assert!(output.contains("Codex CLI packages\n\nInstalled CLI version: 0.157.1"));
    assert!(output.contains("standalone:\n  0.157.0-x86_64-unknown-linux-musl (374M)"));
    assert!(output.contains("app-server-daemon:\n  0.157.0-x86_64-unknown-linux-musl (374M)"));
    assert!(output.contains("Observed release storage: 1.5G"));
    assert!(output.contains("Older generations potentially reclaimable: 0B"));
    assert!(output.contains("Review package use and rollback needs before removing anything."));
    assert!(output.contains("preserves the current CLI generation and one previous generation"));
    let mut accumulated = stores.clone();
    for store in &mut accumulated {
        store
            .entries
            .extend(["0.156.0", "0.156.1"].map(|version| ReleaseEntry {
                name: format!("{version}-x86_64-unknown-linux-musl"),
                bytes: 374 * MIB,
            }));
    }
    let output =
        render_investigation_with_codex_packages(None, &snapshot, &accumulated, Some("0.157.1"));
    assert!(output.contains("Older generations potentially reclaimable: 1.5G"));
    assert_eq!(assessment_line(&output), "Investigation recommended");
    assert!(!output.contains("No action required."));

    let mut mismatch = stores.clone();
    mismatch[1].entries[1].name = "0.157.2-x86_64-unknown-linux-musl".into();
    let output =
        render_investigation_with_codex_packages(None, &snapshot, &mismatch, Some("0.157.1"));
    assert!(output.contains("0.157.2-x86_64-unknown-linux-musl (374M)"));
    assert!(output.contains("Package versions do not confirm an older generation"));
    assert!(!output.contains("potentially reclaimable"));

    let mut single = stores.clone();
    for store in &mut single {
        store.entries.remove(0);
    }
    let output =
        render_investigation_with_codex_packages(None, &snapshot, &single, Some("0.157.1"));
    assert!(!output.contains("Codex CLI packages"));

    let mut notable = stores;
    notable[0]
        .entries
        .extend(["0.156.0", "0.156.1"].map(|version| ReleaseEntry {
            name: version.to_string(),
            bytes: 374 * MIB,
        }));
    notable[0].active_entry = Some("0.157.1-x86_64-unknown-linux-musl".to_string());
    let output =
        render_investigation_with_codex_packages(None, &snapshot, &notable, Some("0.157.1"));
    assert!(output.contains("Codex CLI packages"));
    assert!(!output.contains("Retained application versions"));
}

#[test]
fn investigation_reports_notable_retained_application_versions_conservatively() {
    let stores = vec![
        ReleaseStore {
            name: "Codex".to_string(),
            entries: (0..4)
                .map(|version| ReleaseEntry {
                    name: version.to_string(),
                    bytes: 256 * 1024 * 1024,
                })
                .collect(),
            active_entry: Some("3".to_string()),
            current_pointer_configured: true,
        },
        ReleaseStore {
            name: "Claude".to_string(),
            entries: vec![
                ReleaseEntry {
                    name: "2.1.270".to_string(),
                    bytes: 128 * 1024 * 1024,
                };
                4
            ],
            active_entry: None,
            current_pointer_configured: false,
        },
    ];

    let output = render_investigation_with_release_stores(None, &sample(19, 62, 0), &stores);

    assert!(output.contains("Retained application versions"));
    assert!(output.contains("Codex\nActive version: 3"));
    assert!(output.contains("Retained versions: 3 (768M)"));
    assert!(output.contains("Claude\nActive version: unavailable"));
    assert!(output.contains("Review retained versions before removal; rollback or package-manager retention may be intentional."));
    assert!(!output.contains("safe to remove"));
    assert!(!output.contains("delete"));
}

#[test]
fn investigation_reports_multiple_codex_standalone_releases() {
    let codex = CodexStandalone {
        current_release: Some("0.144.6".to_string()),
        releases: vec![
            CodexRelease {
                name: "0.144.5".to_string(),
                bytes: 1024,
            },
            CodexRelease {
                name: "0.144.6".to_string(),
                bytes: 2048,
            },
        ],
    };

    let output = render_investigation_with_codex(None, &sample(19, 62, 0), Some(&codex));

    assert!(output.contains("Codex"));
    assert!(output.contains("Current release: 0.144.6"));
    assert!(output.contains("Installed releases: 2"));
    assert!(output.contains("Runtime storage: 3K"));
    assert!(output.contains("Old releases: 1K"));
    assert!(output.contains(
        "Retention policy: Unknown; upstream standalone updater currently does not prune releases."
    ));
    assert!(!output.contains("delete"));
    assert!(!output.contains("reclaim"));
    assert!(!output.contains("safe to remove"));
}

#[test]
fn investigation_reports_codex_runtime_update_without_generic_investigation_pressure() {
    let mut before = sample(19, 60, 0);
    before.home_usage.push(DirectoryUsage {
        path: "~/.codex/packages".to_string(),
        bytes: 300 * 1024 * 1024,
    });
    let mut after = sample(19, 86, 0);
    after.home_usage.push(DirectoryUsage {
        path: "~/.codex/packages".to_string(),
        bytes: 6 * 1024_i64.pow(3),
    });
    after.largest_directories.push(DirectoryUsage {
        path: "~/.codex/packages".to_string(),
        bytes: 6 * 1024_i64.pow(3),
    });
    let codex = CodexStandalone {
        current_release: Some("0.145.0".to_string()),
        releases: vec![
            CodexRelease {
                name: "0.144.6".to_string(),
                bytes: 2 * 1024_i64.pow(3),
            },
            CodexRelease {
                name: "0.145.0".to_string(),
                bytes: 3 * 1024_i64.pow(3),
            },
        ],
    };

    let output = render_investigation_with_codex(Some(&before), &after, Some(&codex));

    assert!(output.contains("+5.7G ~/.codex/packages"));
    assert!(output.contains("New runtime installed: 0.145.0"));
    assert!(output.contains("Current release: 0.145.0"));
    assert!(output.contains("Installed releases: 2"));
    assert!(output.contains("Runtime storage: 5G"));
    assert!(output.contains("Old releases: 2G"));
    assert_eq!(assessment_line(&output), "Healthy");
    assert!(output.contains("codex-cache report"));
    assert!(!output
        .contains("Review the listed directories to determine whether the growth is expected."));
}

#[test]
fn investigation_recommends_codex_cache_for_both_package_stores() {
    for path in [
        "~/.codex/packages/standalone/releases",
        "~/.codex/packages/app-server-daemon/releases",
    ] {
        let mut before = sample(19, 62, 0);
        before.home_usage.push(DirectoryUsage {
            path: path.to_string(),
            bytes: 0,
        });
        let mut after = sample(19, 62, 0);
        after.home_usage.push(DirectoryUsage {
            path: path.to_string(),
            bytes: 1536 * 1024 * 1024,
        });

        let output = render_investigation(Some(&before), &after);

        assert!(output.contains(&format!("+1.5G {path}")));
        assert!(output.contains("Classification: Codex release/package growth"));
        assert!(output.contains("codex-cache report"));
        assert!(output.contains("codex-cache clean --dry-run --keep current,previous"));
    }
}

#[test]
fn investigation_classifies_copilot_package_growth_with_conservative_recommendation() {
    const MIB: i64 = 1024 * 1024;
    let mut before = sample(19, 62, 0);
    before.home_usage.push(DirectoryUsage {
        path: "~/.copilot".to_string(),
        bytes: 290 * MIB,
    });
    before.copilot_usage.extend([
        DirectoryUsage {
            path: "~/.copilot/pkg/linux-x64".to_string(),
            bytes: 290 * MIB,
        },
        DirectoryUsage {
            path: "~/.copilot/pkg/linux-x64/1.0.82".to_string(),
            bytes: 147 * MIB,
        },
        DirectoryUsage {
            path: "~/.copilot/pkg/linux-x64/1.0.83".to_string(),
            bytes: 143 * MIB,
        },
    ]);
    let mut after = sample(19, 62, 0);
    after.home_usage.push(DirectoryUsage {
        path: "~/.copilot".to_string(),
        bytes: 773 * MIB,
    });
    after.copilot_usage.extend([
        DirectoryUsage {
            path: "~/.copilot/pkg/linux-x64".to_string(),
            bytes: 773 * MIB,
        },
        DirectoryUsage {
            path: "~/.copilot/pkg/linux-x64/1.0.82".to_string(),
            bytes: 147 * MIB,
        },
        DirectoryUsage {
            path: "~/.copilot/pkg/linux-x64/1.0.83".to_string(),
            bytes: 143 * MIB,
        },
        DirectoryUsage {
            path: "~/.copilot/pkg/linux-x64/1.0.85".to_string(),
            bytes: 158 * MIB,
        },
        DirectoryUsage {
            path: "~/.copilot/pkg/linux-x64/1.0.86".to_string(),
            bytes: 159 * MIB,
        },
        DirectoryUsage {
            path: "~/.copilot/pkg/linux-x64/1.0.88".to_string(),
            bytes: 166 * MIB,
        },
    ]);

    let output = render_investigation(Some(&before), &after);

    assert!(output.contains("+483M ~/.copilot/pkg/linux-x64"));
    assert!(output.contains("Classification: Copilot CLI release/package growth"));
    assert!(output.contains("Observed package versions: 1.0.82, 1.0.83, 1.0.85, 1.0.86, 1.0.88"));
    assert!(output
        .contains("Inspect retained Copilot CLI package versions under ~/.copilot/pkg/linux-x64"));
    assert!(!output.contains("copilot clean"));
    assert!(!output.contains("Active version:"));
}

#[test]
fn investigation_does_not_classify_unrelated_copilot_state_as_packages() {
    let mut before = sample(19, 62, 0);
    before.home_usage.push(DirectoryUsage {
        path: "~/.copilot".to_string(),
        bytes: 300 * 1024 * 1024,
    });
    before.copilot_usage.push(DirectoryUsage {
        path: "~/.copilot/pkg/linux-x64".to_string(),
        bytes: 300 * 1024 * 1024,
    });
    let mut after = sample(19, 62, 0);
    after.home_usage.push(DirectoryUsage {
        path: "~/.copilot".to_string(),
        bytes: 900 * 1024 * 1024,
    });
    after.copilot_usage.push(DirectoryUsage {
        path: "~/.copilot/pkg/linux-x64".to_string(),
        bytes: 300 * 1024 * 1024,
    });

    let output = render_investigation(Some(&before), &after);

    assert!(output.contains("Classification: GitHub Copilot runtime"));
    assert!(!output.contains("Classification: Copilot CLI release/package growth"));
    assert!(!output.contains("Inspect retained Copilot CLI package versions"));
}

#[test]
fn investigation_does_not_recommend_codex_cache_for_codex_state_growth() {
    let before = sample(19, 62, 0);
    let mut after = sample(19, 62, 0);
    after.home_usage.push(DirectoryUsage {
        path: "~/.codex".to_string(),
        bytes: 1536 * 1024 * 1024,
    });

    let output = render_investigation(Some(&before), &after);

    assert!(output.contains("Classification: Codex persistent state/history"));
    assert!(!output.contains("codex-cache"));
}

#[test]
fn investigation_omits_codex_cache_recommendation_below_growth_threshold() {
    let before = sample(19, 62, 0);
    let mut after = sample(19, 62, 0);
    after.home_usage.push(DirectoryUsage {
        path: "~/.codex/packages".to_string(),
        bytes: 49 * 1024 * 1024,
    });

    let output = render_investigation(Some(&before), &after);

    assert!(!output.contains("codex-cache"));
}

#[test]
fn investigation_reports_stale_repository_local_cargo_target_without_cleanup_claim() {
    let diagnostics = vec![CargoTargetDiagnostic {
        workspace: "~/labs/repos/recall".to_string(),
        local_target: "~/labs/repos/recall/target".to_string(),
        active_target: "~/.cargo-target".to_string(),
    }];

    let output =
        render_investigation_with_diagnostics(None, &sample(19, 62, 0), None, &diagnostics);

    assert!(output.contains("Cargo targets"));
    assert!(output.contains(
        "~/labs/repos/recall/target appears inactive/stale; Cargo reports active target directory ~/.cargo-target for ~/labs/repos/recall."
    ));
    assert!(!output.contains("safe to remove"));
    assert!(!output.contains("delete"));
    assert!(!output.contains("reclaim"));
}

#[test]
fn low_risk_classified_same_day_growth_does_not_force_investigation() {
    let before = sample(19, 84, 0);
    let mut after = sample(19, 86, 0);
    after.home_usage.push(DirectoryUsage {
        path: "~/labs".to_string(),
        bytes: 487 * 1024 * 1024,
    });
    after.largest_directories.push(DirectoryUsage {
        path: "~/labs".to_string(),
        bytes: 487 * 1024 * 1024,
    });

    let output = render_investigation(Some(&before), &after);

    assert!(output.contains("+487M ~/labs"));
    assert!(output.contains("Classification: Development"));
    assert!(output.contains("Risk: Low"));
    assert_eq!(assessment_line(&output), "Healthy");
    assert!(output.contains("No action required."));
    assert!(!output
        .contains("Review the listed directories to determine whether the growth is expected."));
}

#[test]
fn partial_du_warnings_do_not_force_investigation_when_disk_pressure_is_low() {
    let before = sample(19, 59, 0);
    let mut after = sample(19, 61, 0);
    after.home_usage.extend([
        DirectoryUsage {
            path: "~/.cargo-target".to_string(),
            bytes: 565 * 1024 * 1024,
        },
        DirectoryUsage {
            path: "~/.nvm/versions".to_string(),
            bytes: 350 * 1024 * 1024,
        },
        DirectoryUsage {
            path: "~/.npm".to_string(),
            bytes: 190 * 1024 * 1024,
        },
    ]);
    after.largest_directories.extend([
        DirectoryUsage {
            path: "~/.cargo-target".to_string(),
            bytes: 565 * 1024 * 1024,
        },
        DirectoryUsage {
            path: "~/.nvm/versions".to_string(),
            bytes: 350 * 1024 * 1024,
        },
    ]);
    after.warnings = vec![
        "du ~: permission or read errors ignored".to_string(),
        "du ~/.local/share: permission or read errors ignored".to_string(),
        "du ~: permission or read errors ignored".to_string(),
    ];

    let output = render_investigation(Some(&before), &after);

    assert!(output.contains("Some directory sizes may be partial"));
    assert_eq!(
        output
            .matches("- du ~: permission or read errors ignored")
            .count(),
        1
    );
    assert_eq!(assessment_line(&output), "Healthy");
    assert!(output.contains("No action required."));
    assert!(!output
        .contains("Review the listed directories to determine whether the growth is expected."));
}

#[test]
fn unexpected_collection_warnings_still_recommend_investigation() {
    let mut after = sample(19, 61, 0);
    after.warnings = vec!["podman system df failed: permission denied".to_string()];

    let output = render_investigation(None, &after);

    assert_eq!(assessment_line(&output), "Investigation recommended");
    assert!(output
        .contains("Review the listed directories to determine whether current usage is expected."));
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
        containers: Vec::new(),
        error: None,
    };
    let mut after = sample(19, 62, 0);
    after.podman = PodmanUsage {
        available: true,
        images_bytes: Some(2 * 1024_i64.pow(3)),
        containers_bytes: Some(0),
        volumes_bytes: Some(0),
        containers: Vec::new(),
        error: None,
    };

    let output = render_investigation(Some(&before), &after);

    assert_eq!(assessment_line(&output), "Container storage increasing");
    assert!(output.contains("Review Podman images and containers if the growth is unexpected."));
    assert!(!output.contains("No action required."));
}

#[test]
fn podman_status_attributes_significant_container_storage_without_cleanup_claim() {
    let mut after = sample(19, 62, 0);
    after.podman = PodmanUsage {
        available: true,
        images_bytes: Some(100 * 1024 * 1024),
        containers_bytes: Some(2_330_000_000),
        volumes_bytes: Some(0),
        containers: vec![
            PodmanContainerUsage {
                name: "archbox".to_string(),
                bytes: 2_290_000_000,
            },
            PodmanContainerUsage {
                name: "small".to_string(),
                bytes: 10 * 1024 * 1024,
            },
        ],
        error: None,
    };

    let output = render_investigation(None, &after);

    assert!(output.contains("Containers: 2.2G"));
    assert!(output.contains("Notable containers:"));
    assert!(output.contains("archbox: 2.1G writable layer"));
    assert!(!output.contains("small:"));
    assert!(!output.contains("reclaim"));
    assert!(!output.contains("safe to remove"));
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

#[test]
fn accumulated_claude_and_copilot_versions_need_review_without_growth() {
    for name in ["Claude", "GitHub Copilot CLI"] {
        let mut store = ReleaseStore {
            name: name.into(),
            entries: ["2.1.8", "2.1.9", "2.1.10", "2.1.11"]
                .map(|name| ReleaseEntry {
                    name: name.into(),
                    bytes: 256 * 1024 * 1024,
                })
                .to_vec(),
            active_entry: Some("2.1.11".into()),
            current_pointer_configured: true,
        };
        let snapshot = sample(19, 79, 0);
        let output = render_investigation_with_release_stores(None, &snapshot, &[store.clone()]);
        assert!(output.contains(
            "Older versions potentially reclaimable (preserving current and one previous): 512M"
        ));
        assert_eq!(assessment_line(&output), "Investigation recommended");
        assert!(!output.contains("No action required."));
        store.active_entry = None;
        let output = render_investigation_with_release_stores(None, &snapshot, &[store]);
        assert!(output.contains("reclaimability is unknown"));
        assert!(!output.contains("potentially reclaimable"));
        assert_eq!(assessment_line(&output), "Investigation recommended");
    }
}

#[test]
fn current_and_previous_versions_do_not_trigger_accumulation_review() {
    let store = ReleaseStore {
        name: "Claude".into(),
        entries: ["2.1.10", "2.1.11"]
            .map(|name| ReleaseEntry {
                name: name.into(),
                bytes: 1024 * 1024 * 1024,
            })
            .to_vec(),
        active_entry: Some("2.1.11".into()),
        current_pointer_configured: true,
    };
    let output = render_investigation_with_release_stores(None, &sample(19, 62, 0), &[store]);
    assert!(!output.contains("Retained application versions"));
    assert!(!output.contains("unusually large accumulated"));
    assert_eq!(assessment_line(&output), "Healthy");
}
