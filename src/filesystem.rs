use anyhow::{anyhow, Result};

use crate::models::{DirectoryUsage, FilesystemUsage};

pub fn collect_filesystem() -> Result<FilesystemUsage> {
    Err(anyhow!(
        "filesystem collection is not implemented in Rust phase 1"
    ))
}

pub fn collect_du(
    _path: &std::path::Path,
    _max_depth: u8,
) -> Result<(Vec<DirectoryUsage>, Vec<String>)> {
    Err(anyhow!("du collection is not implemented in Rust phase 1"))
}
