use anyhow::{anyhow, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutput {
    pub stdout: String,
    pub stderr: String,
    pub status: i32,
}

pub trait CommandRunner {
    fn run(&self, command: &[&str]) -> Result<CommandOutput>;
}

#[derive(Debug, Default)]
pub struct UnsupportedCommandRunner;

impl CommandRunner for UnsupportedCommandRunner {
    fn run(&self, _command: &[&str]) -> Result<CommandOutput> {
        Err(anyhow!(
            "command execution is not implemented in Rust phase 1"
        ))
    }
}
