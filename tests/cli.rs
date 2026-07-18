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
