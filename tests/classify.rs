use disk_agent::classify::classify_path;
use disk_agent::rules::load_rules;

#[test]
fn classifier_maps_common_paths_to_categories() {
    let rules = load_rules();

    let cases = [
        ("~/.cache/pip", "Cache"),
        ("~/.cache/uv", "Cache"),
        ("~/.local/share/uv", "Application data"),
        ("~/.local/share/junie", "Application data"),
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
        ("~/.codex", "Application data"),
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
fn classifier_distinguishes_uv_cache_from_uv_managed_data() {
    let rules = load_rules();

    let cache = classify_path("~/.cache/uv", Some(&rules));
    assert_eq!(cache.classification, "uv cache");
    assert_eq!(cache.category, "Cache");
    assert!(cache.recommendation.contains("uv cache clean"));

    let data = classify_path("~/.local/share/uv", Some(&rules));
    assert_eq!(data.classification, "uv-managed tools/data");
    assert_eq!(data.category, "Application data");
    assert!(!data.explanation.contains("cache directory"));
    assert!(!data.recommendation.contains("uv cache clean"));
}

#[test]
fn classifier_marks_junie_and_codex_as_conservative_application_data() {
    let rules = load_rules();

    let junie = classify_path("~/.local/share/junie", Some(&rules));
    assert_eq!(junie.classification, "Junie application data");
    assert_eq!(junie.category, "Application data");
    assert!(junie.recommendation.contains("before removing anything"));

    let codex = classify_path("~/.codex", Some(&rules));
    assert_eq!(codex.classification, "Codex persistent state/history");
    assert_eq!(codex.category, "Application data");
    assert!(codex
        .recommendation
        .contains("Do not treat ~/.codex as disposable cache"));

    let codex_packages = classify_path("~/.codex/packages/0.145.0", Some(&rules));
    assert_eq!(codex_packages.category, "Application releases");
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
