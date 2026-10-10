//! `exodus dev-wait` (debug builds only): a dummy long operation that runs
//! until cancelled, to try the log file, `--verbose` with a progress bar,
//! and Ctrl+C.

use std::{sync::Arc, time::Duration};

use exodus_core::{
    Cancelled, Launcher, LauncherConfig, MessageArgs, NoopSink, ProgressReporter, i18n::message,
};
use indicatif::{ProgressBar, ProgressStyle};

use super::Context;
use crate::failure::Failure;

const SECONDS: u64 = 600;

pub async fn run(context: Context) -> Result<(), Failure> {
    let data_dir = context.data_dir.map_err(|error| Failure::core(&error))?;
    let launcher =
        Launcher::new(LauncherConfig::new(data_dir)).map_err(|error| Failure::core(&error))?;
    let ctx = launcher.op(Arc::new(NoopSink), context.cancel);

    let bar = context.progress.add(ProgressBar::new(SECONDS));
    bar.set_style(ProgressStyle::with_template("{spinner} {msg} {pos}s").expect("valid template"));
    bar.set_message(message("dev-wait-message", &MessageArgs::new()));
    bar.enable_steady_tick(Duration::from_millis(100));

    let result = ctx
        .run("dev-wait", async {
            let mut reporter = ProgressReporter::new(&ctx);
            for second in 1..=SECONDS {
                tokio::time::sleep(Duration::from_secs(1)).await;
                bar.set_position(second);
                reporter.update(second, Some(SECONDS));
                tracing::info!(second, "still waiting");
            }
            Ok::<_, Cancelled>(())
        })
        .await;
    bar.finish_and_clear();
    result.map_err(|error| Failure::core(&error))
}
