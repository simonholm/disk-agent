use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};

use crate::command::{CommandRunner, SystemCommandRunner};
use crate::models::{DirectoryUsage, FilesystemUsage};
use crate::paths;

pub fn collect_filesystem() -> Result<FilesystemUsage> {
    collect_filesystem_with_runner(&SystemCommandRunner)
}

pub fn collect_filesystem_with_runner(runner: &dyn CommandRunner) -> Result<FilesystemUsage> {
    let output = runner.run(&[
        "df",
        "-B1",
        "--output=source,size,used,avail,pcent,target",
        "/",
    ])?;
    let lines = output
        .stdout
        .lines()
        .skip(1)
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.split_whitespace().collect::<Vec<_>>())
        .collect::<Vec<_>>();
    let Some(parts) = lines.last() else {
        return Err(anyhow!("df failed: invalid output"));
    };
    if output.status != 0 || parts.len() < 6 {
        let stderr = output.stderr.trim();
        return Err(anyhow!(
            "df failed: {}",
            if stderr.is_empty() {
                "invalid output"
            } else {
                stderr
            }
        ));
    }
    Ok(FilesystemUsage {
        filesystem: parts[0].to_string(),
        total_bytes: parts[1].parse()?,
        used_bytes: parts[2].parse()?,
        available_bytes: parts[3].parse()?,
        used_percent: parts[4].trim_end_matches('%').parse()?,
        mountpoint: parts[5..].join(" "),
    })
}

pub fn collect_du(path: &Path, max_depth: u8) -> Result<(Vec<DirectoryUsage>, Vec<String>)> {
    collect_du_with_runner(path, max_depth, &SystemCommandRunner)
}

pub fn collect_du_with_runner(
    path: &Path,
    max_depth: u8,
    runner: &dyn CommandRunner,
) -> Result<(Vec<DirectoryUsage>, Vec<String>)> {
    let home = paths::home_dir()?;
    collect_du_with_home(path, max_depth, runner, &home)
}

pub fn collect_du_with_home(
    path: &Path,
    max_depth: u8,
    runner: &dyn CommandRunner,
    home: &Path,
) -> Result<(Vec<DirectoryUsage>, Vec<String>)> {
    if !path.exists() {
        return Ok((
            Vec::new(),
            vec![format!("path not found: {}", display_path(path, home))],
        ));
    }

    let depth = format!("--max-depth={max_depth}");
    let path_arg = path.to_string_lossy().into_owned();
    let output = runner.run(&["du", "-x", "-B1", &depth, &path_arg])?;

    let mut values = Vec::new();
    for line in output.stdout.lines() {
        let Some((size, raw_path)) = line.split_once('\t') else {
            continue;
        };
        let Ok(bytes) = size.parse() else {
            continue;
        };
        values.push(DirectoryUsage {
            path: display_path(&PathBuf::from(raw_path), home),
            bytes,
        });
    }

    let mut warnings = Vec::new();
    if output.status != 0 && !output.stderr.trim().is_empty() {
        warnings.push(format!(
            "du {}: permission or read errors ignored",
            display_path(path, home)
        ));
    }
    Ok((values, warnings))
}

pub fn display_path(path: &Path, home: &Path) -> String {
    let resolved_path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let resolved_home = home.canonicalize().unwrap_or_else(|_| home.to_path_buf());
    match resolved_path.strip_prefix(&resolved_home) {
        Ok(relative) if relative.as_os_str().is_empty() => "~".to_string(),
        Ok(relative) => format!("~/{}", relative.to_string_lossy()),
        Err(_) => path.to_string_lossy().into_owned(),
    }
}
