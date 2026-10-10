//! Diagnostic logs: a daily rotating file, optionally mirrored to stderr.

use std::{
    fs,
    io::{self, IsTerminal, Write},
    path::Path,
};

use exodus_core::{MessageArgs, i18n::message};
use indicatif::MultiProgress;
use tracing_appender::{
    non_blocking::WorkerGuard,
    rolling::{RollingFileAppender, Rotation},
};
use tracing_subscriber::{
    EnvFilter, Layer, fmt::MakeWriter, layer::SubscriberExt, util::SubscriberInitExt,
};

const FILTER_VAR: &str = "EXODUS_LOG";
const FILE_PREFIX: &str = "exodus.log";
const MAX_LOG_FILES: usize = 7;

/// Keeps the file writer alive; drop it only at the end of `main`.
pub struct LogGuard(#[expect(dead_code, reason = "held for its Drop")] Option<WorkerGuard>);

/// Sets up logging. Without `log_dir`, or when the folder is unusable,
/// logs go to stderr only.
pub fn init(verbose: bool, progress: &MultiProgress, log_dir: Option<&Path>) -> LogGuard {
    let stderr_filter = if verbose {
        configured_filter()
    } else {
        EnvFilter::new("warn")
    };
    let stderr_layer = tracing_subscriber::fmt::layer()
        .with_writer(ProgressAwareStderr(progress.clone()))
        .with_ansi(io::stderr().is_terminal())
        .with_target(false)
        .without_time()
        .with_filter(stderr_filter);

    let (file, problem) = match log_dir.map(open_log_file).transpose() {
        Ok(file) => (file, None),
        Err(problem) => (None, Some(problem)),
    };
    let (file_layer, guard) = match file {
        Some(appender) => {
            let (writer, guard) = tracing_appender::non_blocking(appender);
            let layer = tracing_subscriber::fmt::layer()
                .with_writer(writer)
                .with_ansi(false)
                .with_filter(configured_filter());
            (Some(layer), Some(guard))
        }
        None => (None, None),
    };

    tracing_subscriber::registry()
        .with(stderr_layer)
        .with(file_layer)
        .init();

    if let (Some(dir), Some(reason)) = (log_dir, problem) {
        let args = MessageArgs::new()
            .with("path", dir)
            .with("reason", reason.to_string());
        tracing::warn!("{}", message("cli-log-dir-unavailable", &args));
    }
    LogGuard(guard)
}

/// `EXODUS_LOG` (same syntax as `RUST_LOG`), else `info`.
fn configured_filter() -> EnvFilter {
    EnvFilter::builder()
        .with_default_directive(tracing::Level::INFO.into())
        .with_env_var(FILTER_VAR)
        .from_env_lossy()
}

fn open_log_file(dir: &Path) -> io::Result<RollingFileAppender> {
    fs::create_dir_all(dir)?;
    RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix(FILE_PREFIX)
        .max_log_files(MAX_LOG_FILES)
        .build(dir)
        .map_err(io::Error::other)
}

/// Writes each log line to stderr while progress bars are hidden, so the
/// bars do not tear.
#[derive(Clone)]
struct ProgressAwareStderr(MultiProgress);

impl<'a> MakeWriter<'a> for ProgressAwareStderr {
    type Writer = LineWriter;

    fn make_writer(&'a self) -> Self::Writer {
        LineWriter {
            progress: self.0.clone(),
            buffer: Vec::new(),
        }
    }
}

/// Collects one formatted event and prints it when dropped.
struct LineWriter {
    progress: MultiProgress,
    buffer: Vec<u8>,
}

impl Write for LineWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.buffer.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Drop for LineWriter {
    fn drop(&mut self) {
        if self.buffer.is_empty() {
            return;
        }
        self.progress.suspend(|| {
            let _ = io::stderr().write_all(&self.buffer);
        });
    }
}
