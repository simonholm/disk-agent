use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::Deserialize;

use crate::command::{CommandRunner, SystemCommandRunner};
use crate::filesystem::display_path;
use crate::models::Snapshot;
use crate::paths;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CargoTargetDiagnostic {
    pub workspace: String,
    pub local_target: String,
    pub active_target: String,
}

#[derive(Debug, Deserialize)]
struct CargoMetadata {
    target_directory: PathBuf,
}

pub fn detect_stale_cargo_targets(snapshot: &Snapshot) -> Result<Vec<CargoTargetDiagnostic>> {
    detect_stale_cargo_targets_with_runner(snapshot, &SystemCommandRunner)
}

pub fn detect_stale_cargo_targets_with_runner(
    snapshot: &Snapshot,
    runner: &dyn CommandRunner,
) -> Result<Vec<CargoTargetDiagnostic>> {
    let home = paths::home_dir()?;
    detect_stale_cargo_targets_with_home_and_runner(snapshot, &home, runner)
}

fn detect_stale_cargo_targets_with_home_and_runner(
    snapshot: &Snapshot,
    home: &Path,
    runner: &dyn CommandRunner,
) -> Result<Vec<CargoTargetDiagnostic>> {
    let mut diagnostics = Vec::new();

    for workspace in candidate_workspaces(snapshot, &home) {
        let local_target = workspace.join("target");
        if !local_target.is_dir() {
            continue;
        }

        let Some(active_target) = active_target_directory(&workspace, runner)? else {
            continue;
        };
        if same_path(&local_target, &active_target) {
            continue;
        }

        diagnostics.push(CargoTargetDiagnostic {
            workspace: display_path(&workspace, home),
            local_target: display_path(&local_target, home),
            active_target: display_path(&active_target, home),
        });
    }

    Ok(diagnostics)
}

fn candidate_workspaces(snapshot: &Snapshot, home: &Path) -> Vec<PathBuf> {
    let mut candidates = BTreeSet::new();
    for usage in snapshot
        .largest_directories
        .iter()
        .chain(snapshot.home_usage.iter())
        .chain(snapshot.local_share_usage.iter())
        .chain(snapshot.copilot_usage.iter())
    {
        let Some(path) = expand_home_path(&usage.path, home) else {
            continue;
        };
        let workspace = if path.file_name().is_some_and(|name| name == "target") {
            path.parent().map(Path::to_path_buf)
        } else {
            Some(path)
        };
        let Some(workspace) = workspace else {
            continue;
        };
        if workspace.join("Cargo.toml").is_file() {
            candidates.insert(workspace);
        }
    }
    candidates.into_iter().collect()
}

fn active_target_directory(
    workspace: &Path,
    runner: &dyn CommandRunner,
) -> Result<Option<PathBuf>> {
    let output = runner.run_in(
        workspace,
        &["cargo", "metadata", "--format-version", "1", "--no-deps"],
    )?;
    if output.status != 0 {
        return Ok(None);
    }

    let Ok(metadata) = serde_json::from_str::<CargoMetadata>(&output.stdout) else {
        return Ok(None);
    };
    Ok(Some(metadata.target_directory))
}

fn expand_home_path(path: &str, home: &Path) -> Option<PathBuf> {
    if path == "~" {
        Some(home.to_path_buf())
    } else if let Some(rest) = path.strip_prefix("~/") {
        Some(home.join(rest))
    } else if path.starts_with('/') {
        Some(PathBuf::from(path))
    } else {
        None
    }
}

fn same_path(left: &Path, right: &Path) -> bool {
    let left = left.canonicalize().unwrap_or_else(|_| left.to_path_buf());
    let right = right.canonicalize().unwrap_or_else(|_| right.to_path_buf());
    left == right
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::fs;
    use std::path::{Path, PathBuf};

    use tempfile::TempDir;

    use super::detect_stale_cargo_targets_with_home_and_runner;
    use crate::command::{CommandOutput, CommandRunner, SystemCommandRunner};
    use crate::models::{DirectoryUsage, FilesystemUsage, Snapshot};

    #[derive(Default)]
    struct FakeRunner {
        outputs: std::sync::Mutex<VecDeque<CommandOutput>>,
        calls: std::sync::Mutex<Vec<(PathBuf, Vec<String>)>>,
    }

    impl CommandRunner for FakeRunner {
        fn run(&self, command: &[&str]) -> anyhow::Result<CommandOutput> {
            self.run_in(Path::new("."), command)
        }

        fn run_in(&self, cwd: &Path, command: &[&str]) -> anyhow::Result<CommandOutput> {
            self.calls.lock().unwrap().push((
                cwd.to_path_buf(),
                command.iter().map(|part| part.to_string()).collect(),
            ));
            Ok(self.outputs.lock().unwrap().pop_front().unwrap())
        }
    }

    fn sample(path: &str) -> Snapshot {
        Snapshot {
            timestamp: "2026-08-29T10:50:00+00:00".to_string(),
            filesystem: FilesystemUsage {
                filesystem: "/dev/vda".to_string(),
                mountpoint: "/".to_string(),
                total_bytes: 1000,
                used_bytes: 600,
                available_bytes: 400,
                used_percent: 60,
            },
            home_usage: vec![DirectoryUsage {
                path: path.to_string(),
                bytes: 100,
            }],
            local_share_usage: Vec::new(),
            copilot_usage: Vec::new(),
            podman: Default::default(),
            largest_directories: Vec::new(),
            warnings: Vec::new(),
            schema_version: 1,
        }
    }

    #[test]
    fn external_active_target_reports_repo_local_target_as_stale() {
        let home = temp_home();
        let workspace = home.path().join("labs/repos/recall");
        fs::create_dir_all(workspace.join("target")).unwrap();
        fs::write(
            workspace.join("Cargo.toml"),
            "[package]\nname = \"recall\"\n",
        )
        .unwrap();
        let active = home.path().join(".cargo-target");
        fs::create_dir_all(&active).unwrap();
        let runner = runner_with(&format!(r#"{{"target_directory":"{}"}}"#, active.display()));

        let diagnostics = detect_stale_cargo_targets_with_home_and_runner(
            &sample("~/labs/repos/recall"),
            home.path(),
            &runner,
        )
        .unwrap();

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].workspace, "~/labs/repos/recall");
        assert_eq!(diagnostics[0].local_target, "~/labs/repos/recall/target");
        assert_eq!(diagnostics[0].active_target, "~/.cargo-target");
        let calls = runner.calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, workspace);
        assert_eq!(
            calls[0].1,
            ["cargo", "metadata", "--format-version", "1", "--no-deps"]
        );
    }

    #[test]
    fn normal_repo_local_target_produces_no_diagnostic() {
        let home = temp_home();
        let workspace = home.path().join("labs/repos/recall");
        fs::create_dir_all(workspace.join("target")).unwrap();
        fs::write(
            workspace.join("Cargo.toml"),
            "[package]\nname = \"recall\"\n",
        )
        .unwrap();
        let runner = runner_with(&format!(
            r#"{{"target_directory":"{}"}}"#,
            workspace.join("target").display()
        ));

        let diagnostics = detect_stale_cargo_targets_with_home_and_runner(
            &sample("~/labs/repos/recall"),
            home.path(),
            &runner,
        )
        .unwrap();

        assert!(diagnostics.is_empty());
    }

    #[test]
    fn missing_or_invalid_metadata_is_ignored() {
        let home = temp_home();
        let workspace = home.path().join("labs/repos/recall");
        fs::create_dir_all(workspace.join("target")).unwrap();
        fs::write(
            workspace.join("Cargo.toml"),
            "[package]\nname = \"recall\"\n",
        )
        .unwrap();

        let missing = FakeRunner {
            outputs: std::sync::Mutex::new(VecDeque::from([CommandOutput {
                stdout: String::new(),
                stderr: "cargo not found".to_string(),
                status: 127,
            }])),
            calls: std::sync::Mutex::new(Vec::new()),
        };
        let invalid = runner_with("not json");

        assert!(detect_stale_cargo_targets_with_home_and_runner(
            &sample("~/labs/repos/recall"),
            home.path(),
            &missing,
        )
        .unwrap()
        .is_empty());
        assert!(detect_stale_cargo_targets_with_home_and_runner(
            &sample("~/labs/repos/recall"),
            home.path(),
            &invalid,
        )
        .unwrap()
        .is_empty());
    }

    #[test]
    fn non_cargo_directories_are_ignored() {
        let home = temp_home();
        fs::create_dir_all(home.path().join("labs/repos/notes/target")).unwrap();
        let runner = FakeRunner::default();

        let diagnostics = detect_stale_cargo_targets_with_home_and_runner(
            &sample("~/labs/repos/notes"),
            home.path(),
            &runner,
        )
        .unwrap();

        assert!(diagnostics.is_empty());
        assert!(runner.calls.lock().unwrap().is_empty());
    }

    #[test]
    fn workspace_local_cargo_config_controls_active_target_directory() {
        let home = temp_home();
        let workspace = home.path().join("labs/repos/recall");
        fs::create_dir_all(workspace.join(".cargo")).unwrap();
        fs::create_dir_all(workspace.join("src")).unwrap();
        fs::create_dir_all(workspace.join("target")).unwrap();
        fs::write(
            workspace.join("Cargo.toml"),
            "[package]\nname = \"recall\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .unwrap();
        fs::write(workspace.join("src/main.rs"), "fn main() {}\n").unwrap();
        fs::write(
            workspace.join(".cargo/config.toml"),
            "[build]\ntarget-dir = \"configured-target\"\n",
        )
        .unwrap();

        let diagnostics = detect_stale_cargo_targets_with_home_and_runner(
            &sample("~/labs/repos/recall"),
            home.path(),
            &SystemCommandRunner,
        )
        .unwrap();

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            diagnostics[0].active_target,
            "~/labs/repos/recall/configured-target"
        );
    }

    fn runner_with(stdout: &str) -> FakeRunner {
        FakeRunner {
            outputs: std::sync::Mutex::new(VecDeque::from([CommandOutput {
                stdout: stdout.to_string(),
                stderr: String::new(),
                status: 0,
            }])),
            calls: std::sync::Mutex::new(Vec::new()),
        }
    }

    fn temp_home() -> TempDir {
        tempfile::tempdir().unwrap()
    }
}
