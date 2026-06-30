use std::path::{Path, PathBuf};

use anyhow::Result;
use serde_json::Value;

use crate::command::{CommandRunner, SystemCommandRunner};
use crate::models::PodmanUsage;
use crate::paths;

pub fn collect_podman() -> Result<PodmanUsage> {
    let home = paths::home_dir()?;
    Ok(collect_podman_with_runner(
        &SystemCommandRunner,
        &home,
        command_exists("podman"),
    ))
}

pub fn collect_podman_with_runner(
    runner: &dyn CommandRunner,
    home: &Path,
    podman_available: bool,
) -> PodmanUsage {
    if !podman_available {
        return collect_podman_storage_with_runner(runner, home);
    }

    let Ok(output) = runner.run(&["podman", "system", "df", "--format", "{{json .}}"]) else {
        return PodmanUsage {
            error: Some("podman system df failed".to_string()),
            ..Default::default()
        };
    };
    if output.status != 0 {
        return PodmanUsage {
            error: Some(if output.stderr.trim().is_empty() {
                "podman system df failed".to_string()
            } else {
                output.stderr.trim().to_string()
            }),
            ..Default::default()
        };
    }

    let mut images = 0;
    let mut containers = 0;
    let mut volumes = 0;
    for line in output.stdout.lines() {
        let Ok(row) = serde_json::from_str::<Value>(line) else {
            return PodmanUsage {
                error: Some("could not parse podman system df output".to_string()),
                ..Default::default()
            };
        };
        let Some(kind) = row.get("Type").and_then(Value::as_str) else {
            continue;
        };
        let size = parse_size(row.get("Size").and_then(Value::as_str).unwrap_or("0B"));
        match kind {
            "Images" => images = size,
            "Containers" => containers = size,
            "Local Volumes" => volumes = size,
            _ => {}
        }
    }

    PodmanUsage {
        available: true,
        images_bytes: Some(images),
        containers_bytes: Some(containers),
        volumes_bytes: Some(volumes),
        error: None,
    }
}

fn collect_podman_storage_with_runner(runner: &dyn CommandRunner, home: &Path) -> PodmanUsage {
    let storage = home
        .join(".local")
        .join("share")
        .join("containers")
        .join("storage");
    if !storage.exists() {
        return PodmanUsage {
            error: Some("podman is not installed".to_string()),
            ..Default::default()
        };
    }

    let (images, images_error) = du_total(runner, &storage.join("overlay-images"));
    let (containers, containers_error) = du_total(runner, &storage.join("overlay-containers"));
    let (volumes, volumes_error) = du_total(runner, &storage.join("volumes"));
    let had_errors =
        images_error.is_some() || containers_error.is_some() || volumes_error.is_some();

    PodmanUsage {
        available: true,
        images_bytes: Some(images.unwrap_or(0)),
        containers_bytes: Some(containers.unwrap_or(0)),
        volumes_bytes: Some(volumes.unwrap_or(0)),
        error: had_errors.then(|| "podman storage had unreadable paths".to_string()),
    }
}

fn du_total(runner: &dyn CommandRunner, path: &Path) -> (Option<i64>, Option<String>) {
    if !path.exists() {
        return (None, None);
    }
    let path_arg = path.to_string_lossy().into_owned();
    let Ok(output) = runner.run(&["du", "-s", "-B1", &path_arg]) else {
        return (None, Some("could not read Podman storage".to_string()));
    };
    let line = output.stdout.lines().next().unwrap_or("");
    let Some((size, _)) = line.split_once('\t') else {
        return (
            None,
            Some(if output.stderr.trim().is_empty() {
                "could not read Podman storage".to_string()
            } else {
                output.stderr.trim().to_string()
            }),
        );
    };
    match size.parse() {
        Ok(value) => (
            Some(value),
            (output.status != 0 && !output.stderr.trim().is_empty())
                .then(|| output.stderr.trim().to_string()),
        ),
        Err(_) => (
            None,
            Some(if output.stderr.trim().is_empty() {
                "could not parse Podman storage usage".to_string()
            } else {
                output.stderr.trim().to_string()
            }),
        ),
    }
}

pub fn parse_size(value: &str) -> i64 {
    let value = value.trim();
    if value.is_empty() || value == "0B" {
        return 0;
    }
    let units = [
        ("kB", 1000_i64),
        ("KB", 1000_i64),
        ("MB", 1000_i64.pow(2)),
        ("GB", 1000_i64.pow(3)),
        ("TB", 1000_i64.pow(4)),
        ("B", 1_i64),
    ];
    for (suffix, multiplier) in units {
        let Some(number) = value.strip_suffix(suffix) else {
            continue;
        };
        let Ok(number) = number.parse::<f64>() else {
            return 0;
        };
        return (number * multiplier as f64) as i64;
    }
    0
}

fn command_exists(name: &str) -> bool {
    let Some(path) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&path).any(|directory| is_executable(directory.join(name)))
}

fn is_executable(path: PathBuf) -> bool {
    if !path.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        path.metadata()
            .map(|metadata| metadata.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        true
    }
}
