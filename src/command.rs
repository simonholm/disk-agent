use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};

pub const COMMAND_TIMEOUT_SECONDS: u64 = 120;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutput {
    pub stdout: String,
    pub stderr: String,
    pub status: i32,
}

pub trait CommandRunner {
    fn run(&self, command: &[&str]) -> Result<CommandOutput>;

    fn run_in(&self, _cwd: &Path, command: &[&str]) -> Result<CommandOutput> {
        self.run(command)
    }
}

#[derive(Debug, Default)]
pub struct SystemCommandRunner;

impl CommandRunner for SystemCommandRunner {
    fn run(&self, command: &[&str]) -> Result<CommandOutput> {
        run_command(command, None)
    }

    fn run_in(&self, cwd: &Path, command: &[&str]) -> Result<CommandOutput> {
        run_command(command, Some(cwd))
    }
}

fn run_command(command: &[&str], cwd: Option<&Path>) -> Result<CommandOutput> {
    if command.is_empty() {
        return Err(anyhow!("empty command"));
    }

    let mut command_builder = Command::new(command[0]);
    command_builder
        .args(&command[1..])
        .env("LC_ALL", "C")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(cwd) = cwd {
        command_builder.current_dir(cwd);
    }
    let mut child = command_builder.spawn();

    let mut child = match child.as_mut() {
        Ok(_) => child?,
        Err(error) => {
            return Ok(CommandOutput {
                stdout: String::new(),
                stderr: error.to_string(),
                status: 127,
            });
        }
    };

    let deadline = Instant::now() + Duration::from_secs(COMMAND_TIMEOUT_SECONDS);
    loop {
        if child.try_wait()?.is_some() {
            let output = child.wait_with_output()?;
            return Ok(CommandOutput {
                stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
                stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
                status: output.status.code().unwrap_or(1),
            });
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let output = child.wait_with_output()?;
            return Ok(CommandOutput {
                stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
                stderr: format!("command timed out after {COMMAND_TIMEOUT_SECONDS}s"),
                status: 124,
            });
        }
        thread::sleep(Duration::from_millis(100));
    }
}
