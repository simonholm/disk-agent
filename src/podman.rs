use anyhow::{anyhow, Result};

use crate::models::PodmanUsage;

pub fn collect_podman() -> Result<PodmanUsage> {
    Err(anyhow!(
        "podman collection is not implemented in Rust phase 1"
    ))
}
