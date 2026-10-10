//! Command line arguments.
//!
//! Help and usage texts are written by clap in English and stay in code for
//! phase 1; everything Exodus prints itself comes from the `.ftl` files.

use std::path::PathBuf;

use clap::{
    Parser, Subcommand,
    builder::{OsStringValueParser, TypedValueParser},
};

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

    /// Keep all data in this folder (default: EXODUS_DATA_DIR, portable.txt, then the system folder)
    // Accepts an empty value, which the core treats as unset.
    #[arg(
        long,
        global = true,
        value_name = "PATH",
        value_parser = OsStringValueParser::new().map(PathBuf::from)
    )]
    pub data_dir: Option<PathBuf>,

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

    #[test]
    fn data_dir_is_global_and_may_be_empty() {
        let cli = Cli::try_parse_from(["exodus", "info", "--data-dir", ""]).unwrap();
        assert_eq!(cli.data_dir, Some(PathBuf::new()));
        let cli = Cli::try_parse_from(["exodus", "--data-dir", "x", "info"]).unwrap();
        assert_eq!(cli.data_dir, Some(PathBuf::from("x")));
    }
}
