//! One module per subcommand.

#[cfg(debug_assertions)]
mod dev_wait;
mod info;

use std::path::PathBuf;

use exodus_core::{CancellationToken, config::ConfigError};
use indicatif::MultiProgress;

use crate::{cli::Command, failure::Failure};

/// What every command may use.
pub struct Context {
    pub data_dir: Result<PathBuf, ConfigError>,
    pub progress: MultiProgress,
    pub cancel: CancellationToken,
}

pub async fn run(command: Command, context: Context) -> Result<(), Failure> {
    match command {
        Command::Info => info::run(&context, &mut std::io::stdout().lock()),
        #[cfg(debug_assertions)]
        Command::DevWait => dev_wait::run(context).await,
    }
}
