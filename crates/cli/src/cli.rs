//! Command line arguments.
//!
//! Help and usage texts are written by clap in English and stay in code for
//! phase 1; everything Exodus prints itself comes from the `.ftl` files.

use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(
    name = "exodus",
    version,
    about = "A Minecraft launcher for Windows, macOS and Linux",
    propagate_version = true
)]
pub struct Cli {
    /// Mirror diagnostic logs to the terminal (stderr)
    #[arg(short, long, global = true)]
    pub verbose: bool,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Show version, target system and where Exodus keeps its files
    Info,

    /// Run a dummy operation until cancelled, to try logging and Ctrl+C
    #[cfg(debug_assertions)]
    #[command(hide = true)]
    DevWait,
}

impl Command {
    /// Whether the command writes the log file. `info` creates no folders.
    pub fn writes_log_file(&self) -> bool {
        match self {
            Self::Info => false,
            #[cfg(debug_assertions)]
            Self::DevWait => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory;

    use super::*;

    #[test]
    fn arguments_are_consistent() {
        Cli::command().debug_assert();
    }
}
