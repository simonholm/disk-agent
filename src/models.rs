use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DirectoryUsage {
    pub path: String,
    pub bytes: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FilesystemUsage {
    pub filesystem: String,
    pub mountpoint: String,
    pub total_bytes: i64,
    pub used_bytes: i64,
    pub available_bytes: i64,
    pub used_percent: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PodmanUsage {
    #[serde(default)]
    pub available: bool,
    #[serde(default)]
    pub images_bytes: Option<i64>,
    #[serde(default)]
    pub containers_bytes: Option<i64>,
    #[serde(default)]
    pub volumes_bytes: Option<i64>,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snapshot {
    pub timestamp: String,
    pub filesystem: FilesystemUsage,
    #[serde(default)]
    pub home_usage: Vec<DirectoryUsage>,
    #[serde(default)]
    pub local_share_usage: Vec<DirectoryUsage>,
    #[serde(default)]
    pub copilot_usage: Vec<DirectoryUsage>,
    #[serde(default)]
    pub podman: PodmanUsage,
    #[serde(default)]
    pub largest_directories: Vec<DirectoryUsage>,
    #[serde(default)]
    pub warnings: Vec<String>,
    #[serde(default = "schema_version_one")]
    pub schema_version: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsageChange {
    pub path: String,
    pub bytes: i64,
}

pub fn schema_version_one() -> u32 {
    1
}
