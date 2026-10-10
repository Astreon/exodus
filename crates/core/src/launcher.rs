//! The single entry point to the core.

use std::{
    future::Future,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use crate::{
    CancellationToken, Cancelled, ErrorView, Event, EventSink, LauncherConfig, Localize,
    MessageArgs, OperationId,
};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// The launcher. Cheap to clone; every clone shares the same state, so a
/// desktop app can hold it as shared state.
#[derive(Clone)]
pub struct Launcher {
    inner: Arc<Inner>,
}

struct Inner {
    config: LauncherConfig,
    #[expect(dead_code, reason = "the first network feature reads it")]
    http: reqwest::Client,
    next_op: AtomicU64,
}

impl Launcher {
    pub fn new(config: LauncherConfig) -> Result<Self, LauncherError> {
        install_crypto_provider();
        // No overall timeout: downloads may take long.
        let http = reqwest::Client::builder()
            .user_agent(&config.user_agent)
            .connect_timeout(CONNECT_TIMEOUT)
            .build()
            .map_err(LauncherError::HttpClient)?;
        Ok(Self {
            inner: Arc::new(Inner {
                config,
                http,
                next_op: AtomicU64::new(1),
            }),
        })
    }

    pub fn config(&self) -> &LauncherConfig {
        &self.inner.config
    }

    /// Starts the context for one long operation.
    pub fn op(&self, sink: Arc<dyn EventSink>, cancel: CancellationToken) -> OpContext {
        let id = self.inner.next_op.fetch_add(1, Ordering::Relaxed);
        OpContext {
            id: OperationId::new(id),
            sink,
            cancel,
        }
    }
}

impl std::fmt::Debug for Launcher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Launcher")
            .field("config", &self.inner.config)
            .finish_non_exhaustive()
    }
}

/// Uses ring for TLS: it builds without NASM or CMake. Keeps any provider a
/// binary installed earlier.
fn install_crypto_provider() {
    if rustls::crypto::CryptoProvider::get_default().is_none() {
        // Losing a race to another thread installing one is fine.
        let _ = rustls::crypto::ring::default_provider().install_default();
    }
}

/// What every long operation receives: its ID, where its events go, and
/// the token that cancels it.
#[derive(Clone)]
pub struct OpContext {
    id: OperationId,
    sink: Arc<dyn EventSink>,
    cancel: CancellationToken,
}

impl OpContext {
    pub fn id(&self) -> OperationId {
        self.id
    }

    pub fn sink(&self) -> Arc<dyn EventSink> {
        Arc::clone(&self.sink)
    }

    pub fn cancel_token(&self) -> &CancellationToken {
        &self.cancel
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancel.is_cancelled()
    }

    /// For blocking work (hashing, unpacking): call between chunks, since
    /// blocking code cannot be stopped from outside.
    pub fn check_cancelled(&self) -> Result<(), Cancelled> {
        if self.is_cancelled() {
            Err(Cancelled)
        } else {
            Ok(())
        }
    }

    pub fn emit(&self, event: Event) {
        self.sink.emit(event);
    }

    pub fn log(&self, line: impl Into<String>) {
        self.emit(Event::Log {
            op: self.id,
            line: line.into(),
        });
    }

    /// Runs `work` as operation `kind`: sends `Started`, stops the work when
    /// the token fires, and sends exactly one end event.
    pub async fn run<T, E, F>(&self, kind: &'static str, work: F) -> Result<T, E>
    where
        F: Future<Output = Result<T, E>>,
        E: Localize + From<Cancelled>,
    {
        self.emit(Event::Started { op: self.id, kind });
        let result = tokio::select! {
            biased;
            () = self.cancel.cancelled() => Err(E::from(Cancelled)),
            result = work => result,
        };
        let end = match &result {
            Ok(_) => Event::Finished { op: self.id },
            Err(error) if error.is_cancelled() => Event::Cancelled { op: self.id },
            Err(error) => Event::Failed {
                op: self.id,
                error: ErrorView::from(error),
            },
        };
        self.emit(end);
        result
    }
}

impl std::fmt::Debug for OpContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpContext")
            .field("id", &self.id)
            .field("cancelled", &self.is_cancelled())
            .finish_non_exhaustive()
    }
}

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum LauncherError {
    #[error("failed to build the HTTP client")]
    HttpClient(#[source] reqwest::Error),
}

impl Localize for LauncherError {
    fn key(&self) -> &'static str {
        match self {
            Self::HttpClient(_) => "launcher-http-client",
        }
    }

    fn args(&self) -> MessageArgs {
        match self {
            Self::HttpClient(source) => MessageArgs::new().with("reason", source.to_string()),
        }
    }
}

#[cfg(test)]
pub(crate) fn error_samples() -> Vec<Box<dyn Localize>> {
    install_crypto_provider();
    let builder_error = reqwest::Client::new()
        .get("not a url")
        .build()
        .expect_err("an invalid URL fails to build");
    vec![Box::new(LauncherError::HttpClient(builder_error))]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ChannelSink;

    #[derive(Debug, thiserror::Error)]
    enum TestError {
        #[error("boom")]
        Boom,
        #[error(transparent)]
        Cancelled(#[from] Cancelled),
    }

    impl Localize for TestError {
        fn key(&self) -> &'static str {
            match self {
                Self::Boom => "launcher-http-client",
                Self::Cancelled(inner) => inner.key(),
            }
        }

        fn is_cancelled(&self) -> bool {
            matches!(self, Self::Cancelled(_))
        }
    }

    fn launcher() -> Launcher {
        Launcher::new(LauncherConfig::new("data")).unwrap()
    }

    async fn run_with(
        work: impl Future<Output = Result<(), TestError>>,
        cancel: CancellationToken,
    ) -> (Result<(), TestError>, Vec<Event>) {
        let (sink, mut events) = ChannelSink::new();
        let ctx = launcher().op(Arc::new(sink), cancel);
        let result = ctx.run("test", work).await;
        drop(ctx);
        let mut seen = Vec::new();
        while let Some(event) = events.recv().await {
            seen.push(event);
        }
        (result, seen)
    }

    fn assert_send_sync_clone<T: Send + Sync + Clone>() {}

    #[test]
    fn launcher_can_be_shared_state() {
        assert_send_sync_clone::<Launcher>();
    }

    #[test]
    fn operation_ids_are_unique() {
        let launcher = launcher();
        let a = launcher.op(Arc::new(crate::NoopSink), CancellationToken::new());
        let b = launcher
            .clone()
            .op(Arc::new(crate::NoopSink), CancellationToken::new());
        assert_ne!(a.id(), b.id());
    }

    #[tokio::test]
    async fn success_sends_started_then_finished() {
        let (result, events) = run_with(async { Ok(()) }, CancellationToken::new()).await;
        assert!(result.is_ok());
        assert!(matches!(
            events[..],
            [Event::Started { kind: "test", .. }, Event::Finished { .. }]
        ));
    }

    #[tokio::test]
    async fn failure_sends_failed_with_the_error() {
        let (result, events) =
            run_with(async { Err(TestError::Boom) }, CancellationToken::new()).await;
        assert!(matches!(result, Err(TestError::Boom)));
        match &events[..] {
            [Event::Started { .. }, Event::Failed { error, .. }] => {
                assert_eq!(error.detail, "boom")
            }
            other => panic!("unexpected events {other:?}"),
        }
    }

    #[tokio::test]
    async fn cancelling_stops_the_work_and_sends_cancelled() {
        let cancel = CancellationToken::new();
        cancel.cancel();
        let (result, events) = run_with(std::future::pending(), cancel).await;
        assert!(result.unwrap_err().is_cancelled());
        assert!(matches!(
            events[..],
            [Event::Started { .. }, Event::Cancelled { .. }]
        ));
    }
}
