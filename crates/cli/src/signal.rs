//! Ctrl+C handling.

use exodus_core::CancellationToken;

use crate::failure::EXIT_CANCELLED;

/// The first Ctrl+C cancels `cancel` so the running operation can clean up;
/// a second one ends the process at once.
pub fn cancel_on_ctrl_c(cancel: CancellationToken) {
    tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_err() {
            return;
        }
        tracing::info!("interrupted, cancelling");
        cancel.cancel();
        if tokio::signal::ctrl_c().await.is_ok() {
            std::process::exit(EXIT_CANCELLED.into());
        }
    });
}
