use std::process::ExitCode;

use clap::Parser;
use disk_agent_rs::cli::{Cli, Command};

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli.command) {
        Ok(output) => {
            println!("{output}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("disk-agent-rs: {error}");
            ExitCode::from(1)
        }
    }
}

fn run(command: Command) -> anyhow::Result<String> {
    match command {
        Command::Snapshot => disk_agent_rs::snapshot::snapshot_command(),
        Command::Report => disk_agent_rs::report::report_command(),
        Command::Diff => disk_agent_rs::diff::diff_command(),
        Command::Explain => disk_agent_rs::explain::explain_command(),
        Command::Investigate => disk_agent_rs::investigate::investigate_command(),
    }
}
