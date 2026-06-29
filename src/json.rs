use std::fs::File;
use std::io::{BufReader, Write};
use std::path::Path;

use anyhow::{Context, Result};

use crate::models::Snapshot;

pub fn load_snapshot(path: &Path) -> Result<Snapshot> {
    let file = File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let reader = BufReader::new(file);
    serde_json::from_reader(reader).with_context(|| format!("failed to parse {}", path.display()))
}

pub fn write_snapshot_pretty(path: &Path, snapshot: &Snapshot) -> Result<()> {
    let mut file =
        File::create(path).with_context(|| format!("failed to create {}", path.display()))?;
    serde_json::to_writer_pretty(&mut file, snapshot)
        .with_context(|| format!("failed to encode {}", path.display()))?;
    file.write_all(b"\n")
        .with_context(|| format!("failed to finish {}", path.display()))?;
    Ok(())
}
