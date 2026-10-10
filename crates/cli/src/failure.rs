//! How a failed command ends: a message on stderr and an exit code.

use std::process::ExitCode;

use exodus_core::{ErrorView, Localize, MessageArgs, i18n::message};

/// Exit code for a cancelled command (128 + SIGINT).
pub const EXIT_CANCELLED: u8 = 130;

#[derive(Debug)]
pub enum Failure {
    /// An error from the core, shown with its localized text.
    Core { view: ErrorView, cancelled: bool },
    /// A problem in the CLI itself, such as stdout being closed.
    Cli(anyhow::Error),
}

impl Failure {
    pub fn core(error: &impl Localize) -> Self {
        Self::Core {
            view: ErrorView::from(error),
            cancelled: error.is_cancelled(),
        }
    }

    /// Prints the failure and returns the exit code: 130 when cancelled, else 1.
    pub fn report(self) -> ExitCode {
        match self {
            Self::Core {
                cancelled: true, ..
            } => {
                tracing::info!("cancelled");
                eprintln!("{}", message("cli-cancelled", &MessageArgs::new()));
                ExitCode::from(EXIT_CANCELLED)
            }
            Self::Core { view, .. } => {
                tracing::error!(key = %view.key, detail = %view.detail, "command failed");
                print_error(&view.message());
                ExitCode::FAILURE
            }
            Self::Cli(error) => {
                tracing::error!(detail = format!("{error:#}"), "command failed");
                print_error(&format!("{error:#}"));
                ExitCode::FAILURE
            }
        }
    }
}

impl From<anyhow::Error> for Failure {
    fn from(error: anyhow::Error) -> Self {
        Self::Cli(error)
    }
}

impl From<std::io::Error> for Failure {
    fn from(error: std::io::Error) -> Self {
        Self::Cli(error.into())
    }
}

fn print_error(text: &str) {
    let args = MessageArgs::new().with("message", text);
    eprintln!("{}", message("cli-error", &args));
}
