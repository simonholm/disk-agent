use disk_agent::classify::classify_path;
use disk_agent::rules::load_rules;

#[test]
fn classifier_maps_common_paths_to_categories() {
    let rules = load_rules();

    let cases = [
        ("~/.cache/pip", "Cache"),
        ("~/.cargo/registry", "Rust"),
        ("~/.npm/_cacache", "Node"),
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
