use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};

use crate::json::write_snapshot_pretty;
use crate::models::Snapshot;

pub fn save_snapshot(snapshot: &Snapshot, directory: &Path) -> Result<PathBuf> {
    fs::create_dir_all(directory)?;
    let day = snapshot
        .timestamp
        .get(..10)
        .ok_or_else(|| anyhow!("snapshot timestamp is too short"))?;
    let destination = directory.join(format!("{day}.json"));
    let temporary = destination.with_extension("json.tmp");
    write_snapshot_pretty(&temporary, snapshot)?;
    fs::rename(&temporary, &destination)?;
    Ok(destination)
}

pub fn snapshot_command() -> Result<String> {
    Err(anyhow!(
        "snapshot collection is not implemented in the Rust phase 1 binary"
    ))
}
