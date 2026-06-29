use std::path::PathBuf;

use anyhow::{anyhow, Result};

pub fn home_dir() -> Result<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| anyhow!("HOME is not set"))
}

pub fn snapshot_dir() -> Result<PathBuf> {
    Ok(home_dir()?.join(".disk-agent").join("snapshots"))
}
