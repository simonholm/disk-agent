use std::process::ExitCode;

use clap::Parser;
use disk_agent::cli::{Cli, Command};

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli.command) {
        Ok(output) => {
            println!("{output}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("disk-agent: {error}");
            ExitCode::from(1)
        }
    }
}

fn run(command: Command) -> anyhow::Result<String> {
    match command {
        Command::Snapshot => disk_agent::snapshot::snapshot_command(),
        Command::Report => disk_agent::report::report_command(),
        Command::Diff => disk_agent::diff::diff_command(),
        Command::Explain => disk_agent::explain::explain_command(),
        Command::Investigate => disk_agent::investigate::investigate_command(),
    }
}
