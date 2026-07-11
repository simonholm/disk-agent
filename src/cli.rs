use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(
    name = "disk-agent",
    about = "Bounded, read-only disk usage observer.",
    disable_help_subcommand = true
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Clone, Copy, Subcommand)]
pub enum Command {
    /// Collect and store today's disk usage snapshot.
    Snapshot {
        /// Print each warning that was ignored during collection.
        #[arg(long)]
        verbose: bool,
    },
    /// Show the latest saved snapshot.
    Report {
        /// Collect and save a fresh snapshot before reporting.
        #[arg(long)]
        refresh: bool,
    },
    /// Compare the latest two daily snapshots.
    Diff,
    /// Explain the latest significant changes.
    Explain,
    /// Collect evidence and produce a bounded diagnostic report.
    Investigate,
}
