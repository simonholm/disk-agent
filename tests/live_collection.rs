use std::path::Path;
use std::sync::Mutex;

use anyhow::Result;
use disk_agent_rs::command::{CommandOutput, CommandRunner};
use disk_agent_rs::filesystem::{collect_du_with_home, collect_filesystem_with_runner};
use disk_agent_rs::podman::{collect_podman_with_runner, parse_size};

#[derive(Debug, Default)]
struct FakeRunner {
    outputs: Mutex<Vec<(&'static str, CommandOutput)>>,
}

impl FakeRunner {
    fn new(outputs: Vec<(&'static str, CommandOutput)>) -> Self {
        Self {
            outputs: Mutex::new(outputs),
        }
    }
}

impl CommandRunner for FakeRunner {
    fn run(&self, command: &[&str]) -> Result<CommandOutput> {
        let joined = command.join(" ");
        let mut outputs = self.outputs.lock().unwrap();
        let Some(index) = outputs
            .iter()
            .position(|(prefix, _)| joined.starts_with(prefix))
        else {
            return Err(anyhow::anyhow!("unexpected command: {joined}"));
        };
        Ok(outputs.remove(index).1)
    }
}

fn output(stdout: &str, stderr: &str, status: i32) -> CommandOutput {
    CommandOutput {
        stdout: stdout.to_string(),
        stderr: stderr.to_string(),
        status,
    }
}

#[test]
fn filesystem_collection_parses_df_output() {
    let runner = FakeRunner::new(vec![(
        "df -B1 --output=source,size,used,avail,pcent,target /",
        output(
            "Filesystem     1B-blocks Used Available Use% Mounted on\n/dev/vda1 1000 610 390 61% /\n",
            "",
            0,
        ),
    )]);

    let usage = collect_filesystem_with_runner(&runner).unwrap();

    assert_eq!(usage.filesystem, "/dev/vda1");
    assert_eq!(usage.total_bytes, 1000);
    assert_eq!(usage.used_bytes, 610);
    assert_eq!(usage.available_bytes, 390);
    assert_eq!(usage.used_percent, 61);
    assert_eq!(usage.mountpoint, "/");
}

#[test]
fn du_collection_normalizes_home_paths_and_keeps_partial_data_on_warning() {
    let directory = tempfile::tempdir().unwrap();
    let home = directory.path();
    let cache = home.join(".cache");
    std::fs::create_dir(&cache).unwrap();
    let stdout = format!(
        "100\t{}\n250\t{}\n",
        cache.display(),
        Path::new(home).display()
    );
    let runner = FakeRunner::new(vec![(
        "du -x -B1 --max-depth=2",
        output(&stdout, "du: cannot read private\n", 1),
    )]);

    let (values, warnings) = collect_du_with_home(home, 2, &runner, home).unwrap();

    assert_eq!(values[0].path, "~/.cache");
    assert_eq!(values[0].bytes, 100);
    assert_eq!(values[1].path, "~");
    assert_eq!(values[1].bytes, 250);
    assert_eq!(
        warnings,
        vec!["du ~: permission or read errors ignored".to_string()]
    );
}

#[test]
fn podman_collection_parses_system_df_json_lines() {
    let runner = FakeRunner::new(vec![(
        "podman system df --format {{json .}}",
        output(
            "{\"Type\":\"Images\",\"Size\":\"1.5GB\"}\n{\"Type\":\"Containers\",\"Size\":\"20MB\"}\n{\"Type\":\"Local Volumes\",\"Size\":\"2kB\"}\n",
            "",
            0,
        ),
    )]);

    let usage = collect_podman_with_runner(&runner, Path::new("/unused"), true);

    assert!(usage.available);
    assert_eq!(usage.images_bytes, Some(1_500_000_000));
    assert_eq!(usage.containers_bytes, Some(20_000_000));
    assert_eq!(usage.volumes_bytes, Some(2_000));
    assert_eq!(usage.error, None);
}

#[test]
fn podman_collection_uses_rootless_storage_when_binary_is_absent() {
    let directory = tempfile::tempdir().unwrap();
    let storage = directory
        .path()
        .join(".local")
        .join("share")
        .join("containers")
        .join("storage");
    std::fs::create_dir_all(storage.join("overlay-images")).unwrap();
    std::fs::create_dir_all(storage.join("overlay-containers")).unwrap();
    std::fs::create_dir_all(storage.join("volumes")).unwrap();
    let runner = FakeRunner::new(vec![
        ("du -s -B1", output("100\timages\n", "", 0)),
        ("du -s -B1", output("200\tcontainers\n", "", 0)),
        ("du -s -B1", output("300\tvolumes\n", "", 0)),
    ]);

    let usage = collect_podman_with_runner(&runner, directory.path(), false);

    assert!(usage.available);
    assert_eq!(usage.images_bytes, Some(100));
    assert_eq!(usage.containers_bytes, Some(200));
    assert_eq!(usage.volumes_bytes, Some(300));
}

#[test]
fn podman_size_parser_matches_python_units() {
    assert_eq!(parse_size("0B"), 0);
    assert_eq!(parse_size("1.5GB"), 1_500_000_000);
    assert_eq!(parse_size("42MB"), 42_000_000);
    assert_eq!(parse_size("bad"), 0);
}
