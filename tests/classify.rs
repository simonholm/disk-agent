use disk_agent::classify::classify_path;
use disk_agent::rules::load_rules;

#[test]
fn classifier_maps_common_paths_to_categories() {
    let rules = load_rules();

    let cases = [
        ("~/.cache/pip", "Cache"),
        ("~/.cache/uv", "Cache"),
        ("~/.cargo/registry", "Rust"),
        ("~/.cargo-target", "Rust"),
        ("~/.cargo-target/release", "Rust"),
        ("~/.npm/_cacache", "Node"),
        ("~/.npm", "Node"),
        ("~/.nvm/versions/node/v24.4.0", "Node"),
        (
            "~/.local/share/claude/versions/0.1.0",
            "Application releases",
        ),
        ("~/.codex/packages/0.145.0", "Application releases"),
        ("~/.local/share/Trash/files", "Trash"),
        ("~/.local/share/containers/storage", "Podman"),
        ("~/Downloads/archive.iso", "Downloads"),
        ("~/Pictures/import", "Photos"),
        ("~/Videos/export", "Media"),
        ("~/labs/archive", "Development"),
        ("/var/log/journal", "System logs"),
    ];

    for (path, category) in cases {
        let classification = classify_path(path, Some(&rules));
        assert_eq!(classification.category, category);
        assert!(classification.known, "{path} should be classified");
    }
}

#[test]
fn classifier_reports_unclassified_locations_without_generic_rule_text() {
    let rules = load_rules();
    let classification = classify_path("~/mystery-growth", Some(&rules));

    assert_eq!(classification.category, "Unclassified");
    assert_eq!(
        classification.explanation,
        "Growth occurred in unclassified locations."
    );
    assert!(!classification.known);
}
