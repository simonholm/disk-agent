use std::process::Command;

fn disk_agent(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_disk-agent"))
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn long_version_flag_reports_package_version() {
    let output = disk_agent(&["--version"]);

    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        format!("disk-agent {}\n", env!("CARGO_PKG_VERSION"))
    );
}

#[test]
fn short_version_flag_reports_package_version() {
    let output = disk_agent(&["-V"]);

    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        format!("disk-agent {}\n", env!("CARGO_PKG_VERSION"))
    );
}

#[test]
fn help_flag_still_reports_normal_cli_help() {
    let output = disk_agent(&["--help"]);

    assert!(output.status.success());
    assert!(output.stderr.is_empty());

    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("Bounded, read-only disk usage observer."));
    assert!(stdout.contains("Usage: disk-agent <COMMAND>"));
    assert!(stdout.contains("Commands:"));
    assert!(stdout.contains("snapshot"));
    assert!(stdout.contains("investigate"));
    assert!(stdout.contains("-h, --help"));
}

#[test]
fn saved_report_is_explicit_and_does_not_collect_current_state() {
    let home = tempfile::tempdir().unwrap();
    let snapshots = home.path().join(".disk-agent/snapshots");
    std::fs::create_dir_all(&snapshots).unwrap();
    std::fs::copy(
        "tests/fixtures/snapshot_full.json",
        snapshots.join("2026-06-19.json"),
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_disk-agent"))
        .arg("report")
        .env("HOME", home.path())
        .env("PATH", "")
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("Stored snapshot — current usage was not measured"));
    assert!(stdout.contains("Snapshot age:"));
    assert!(stdout.contains("Filesystem usage: 60%"));
    assert!(stdout.contains("disk-agent report --refresh"));
    assert_eq!(std::fs::read_dir(snapshots).unwrap().count(), 1);
}

#[test]
fn refreshed_report_collects_and_saves_fresh_measurements() {
    use std::os::unix::fs::PermissionsExt;
    let home = tempfile::tempdir().unwrap();
    let bin = home.path().join("bin");
    std::fs::create_dir(&bin).unwrap();
    for (name, script) in [
        ("df", "#!/bin/sh\nprintf 'Filesystem Size Used Avail Use%% Target\n/dev/test 1000 640 360 64%% /\n'\n"),
        ("du", "#!/bin/sh\nexit 0\n"),
        ("podman", "#!/bin/sh\nexit 1\n"),
    ] {
        let path = bin.join(name);
        std::fs::write(&path, script).unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let output = Command::new(env!("CARGO_BIN_EXE_disk-agent"))
        .args(["report", "--refresh"])
        .env("HOME", home.path())
        .env("PATH", &bin)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.starts_with("Fresh measurements collected:"));
    assert!(stdout.contains("Filesystem usage: 64%"));
    assert!(!stdout.contains("current usage was not measured"));
    let snapshots =
        disk_agent::report::snapshot_paths(&home.path().join(".disk-agent/snapshots")).unwrap();
    assert_eq!(snapshots.len(), 1);
    let snapshot = disk_agent::json::load_snapshot(&snapshots[0]).unwrap();
    assert_eq!(snapshot.filesystem.used_percent, 64);
}
