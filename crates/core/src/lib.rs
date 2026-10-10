//! Exodus core: all launcher logic, free of any user interface.
//!
//! Surfaces (the CLI, later the desktop app) talk to the core through
//! [`Launcher`]. Long operations take an [`OpContext`], report progress as
//! typed [`Event`]s to an [`EventSink`], and stop when their
//! [`CancellationToken`] fires. Errors implement [`Localize`], so a surface
//! turns them into text only when it shows them.

pub mod build_info;
pub mod config;
pub mod error;
pub mod event;
pub mod i18n;
pub mod launcher;

pub use build_info::{BuildInfo, build_info};
pub use config::{Endpoints, LauncherConfig};
pub use error::{Cancelled, ErrorView, Localize};
pub use event::{ChannelSink, Event, EventSink, NoopSink, OperationId, ProgressReporter};
pub use i18n::{MessageArgs, MessageValue};
pub use launcher::{Launcher, LauncherError, OpContext};
pub use tokio_util::sync::CancellationToken;
