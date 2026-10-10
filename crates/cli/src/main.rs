//! `exodus`: the command line surface of the Exodus launcher.

mod cli;
mod commands;
mod failure;
mod logging;
mod signal;

use std::process::ExitCode;

use clap::Parser;
use exodus_core::{CancellationToken, config};
use indicatif::MultiProgress;

use crate::cli::Cli;

#[tokio::main]
async fn main() -> ExitCode {
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(error) => {
            // Help and version print to stdout and exit with 0; misuse exits with 2.
            let _ = error.print();
            return ExitCode::from(u8::try_from(error.exit_code()).unwrap_or(2));
        }
    };

    let progress = MultiProgress::new();
    let data_dir = config::locate_data_dir(cli.data_dir.clone());
    let log_dir = cli
        .command
        .writes_log_file()
        .then(|| data_dir.as_ref().ok().map(|(dir, _)| config::log_dir(dir)))
        .flatten();
    // Lives until the end of `main` so the last log lines reach the file.
    let _log_guard = logging::init(cli.verbose, &progress, log_dir.as_deref());

    let cancel = CancellationToken::new();
    signal::cancel_on_ctrl_c(cancel.clone());

    let context = commands::Context {
        data_dir,
        progress,
        cancel,
    };
    match commands::run(cli.command, context).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(failure) => failure.report(),
    }
}
