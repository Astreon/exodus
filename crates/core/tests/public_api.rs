//! The core as a surface sees it: only the public API, from outside the crate.

use std::{sync::Arc, time::Duration};

use exodus_core::{
    BuildInfo, CancellationToken, Cancelled, ChannelSink, ErrorView, Event, EventSink, Launcher,
    LauncherConfig, Localize, MessageArgs, NoopSink, OpContext, ProgressReporter, build_info,
    i18n::message,
};
use tokio::sync::mpsc::UnboundedReceiver;

/// The error enum a core module would define, as a surface sees it.
#[derive(Debug, thiserror::Error)]
enum DemoError {
    #[error("disk is full")]
    DiskFull,
    #[error(transparent)]
    Cancelled(#[from] Cancelled),
}

impl Localize for DemoError {
    fn key(&self) -> &'static str {
        match self {
            // Reuses a real key, so the text renders.
            Self::DiskFull => "launcher-http-client",
            Self::Cancelled(inner) => inner.key(),
        }
    }

    fn args(&self) -> MessageArgs {
        match self {
            Self::DiskFull => MessageArgs::new().with("reason", "disk is full"),
            Self::Cancelled(inner) => inner.args(),
        }
    }

    fn is_cancelled(&self) -> bool {
        matches!(self, Self::Cancelled(_))
    }
}

fn launcher() -> Launcher {
    Launcher::new(LauncherConfig::new("data")).expect("launcher builds")
}

fn operation(cancel: CancellationToken) -> (OpContext, UnboundedReceiver<Event>) {
    let (sink, events) = ChannelSink::new();
    (launcher().op(Arc::new(sink), cancel), events)
}

/// Drops the context so the channel closes, then collects every event.
async fn all_events(ctx: OpContext, mut events: UnboundedReceiver<Event>) -> Vec<Event> {
    drop(ctx);
    let mut seen = Vec::new();
    while let Some(event) = events.recv().await {
        seen.push(event);
    }
    seen
}

// covers: AC-5 (every listed export is reachable and usable)
#[tokio::test]
async fn an_operation_reports_progress_logs_and_one_end_event() {
    let (ctx, events) = operation(CancellationToken::new());

    let result = ctx
        .run("demo", async {
            let mut reporter = ProgressReporter::new(&ctx);
            reporter.update(1, Some(2));
            ctx.log("halfway");
            reporter.update(2, Some(2));
            reporter.finish();
            Ok::<_, DemoError>(42)
        })
        .await;

    assert_eq!(result.unwrap(), 42);
    let op = ctx.id();
    assert_eq!(
        all_events(ctx, events).await,
        vec![
            Event::Started { op, kind: "demo" },
            Event::Progress {
                op,
                done: 1,
                total: Some(2)
            },
            Event::Log {
                op,
                line: "halfway".into()
            },
            Event::Progress {
                op,
                done: 2,
                total: Some(2)
            },
            Event::Finished { op },
        ]
    );
}

// covers: AC-5, AC-6 (cancelling through the token)
#[tokio::test]
async fn cancelling_while_running_ends_with_cancelled_and_never_finished() {
    let cancel = CancellationToken::new();
    let (ctx, events) = operation(cancel.clone());

    let canceller = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(20)).await;
        cancel.cancel();
    });
    let result: Result<(), DemoError> = ctx
        .run("demo", async {
            tokio::time::sleep(Duration::from_secs(60)).await;
            Ok(())
        })
        .await;
    canceller.await.unwrap();

    assert!(result.unwrap_err().is_cancelled());
    let events = all_events(ctx, events).await;
    assert!(matches!(
        events[..],
        [Event::Started { .. }, Event::Cancelled { .. }]
    ));
}

#[tokio::test]
async fn blocking_work_that_sees_the_token_ends_as_cancelled_not_failed() {
    let cancel = CancellationToken::new();
    let (ctx, events) = operation(cancel.clone());
    let worker = ctx.clone();

    let result = ctx
        .run("hash", async move {
            tokio::task::spawn_blocking(move || {
                cancel.cancel();
                worker.check_cancelled()?;
                Ok::<_, DemoError>(())
            })
            .await
            .expect("blocking task ran")
        })
        .await;

    assert!(result.unwrap_err().is_cancelled());
    let events = all_events(ctx, events).await;
    assert!(matches!(events.last(), Some(Event::Cancelled { .. })));
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, Event::Failed { .. } | Event::Finished { .. }))
    );
}

#[test]
fn check_cancelled_passes_until_the_token_fires() {
    let cancel = CancellationToken::new();
    let ctx = launcher().op(Arc::new(NoopSink), cancel.clone());
    assert!(ctx.check_cancelled().is_ok());

    cancel.cancel();

    assert!(ctx.is_cancelled());
    assert_eq!(ctx.check_cancelled(), Err(Cancelled));
}

// covers: AC-5 (a core error becomes English text through Fluent)
#[tokio::test]
async fn a_failure_reaches_the_surface_as_a_serializable_error_view() {
    let (ctx, events) = operation(CancellationToken::new());

    let result: Result<(), DemoError> = ctx.run("demo", async { Err(DemoError::DiskFull) }).await;

    assert!(!result.unwrap_err().is_cancelled());
    let events = all_events(ctx, events).await;
    let Some(Event::Failed { error, .. }) = events.last() else {
        panic!("expected Failed as the last event, got {events:?}");
    };
    assert_eq!(
        error.message(),
        "Could not set up the network client: disk is full"
    );
    assert_eq!(
        serde_json::to_value(error).unwrap(),
        serde_json::json!({
            "key": "launcher-http-client",
            "args": { "reason": "disk is full" },
            "detail": "disk is full",
        })
    );
}

#[test]
fn an_error_view_renders_through_the_shared_message_function() {
    let view = ErrorView::from(&Cancelled);
    assert_eq!(view.message(), message(&view.key, &view.args));
}

#[test]
fn a_sink_whose_surface_stopped_listening_drops_events_quietly() {
    let (sink, events) = ChannelSink::new();
    drop(events);
    let ctx = launcher().op(Arc::new(sink.clone()), CancellationToken::new());

    sink.emit(Event::Finished { op: ctx.id() });
    ctx.log("nobody hears this");
}

#[test]
fn every_clone_of_the_launcher_hands_out_new_operation_ids() {
    let launcher = launcher();
    let clone = launcher.clone();
    let ids: Vec<u64> = (0..4)
        .map(|n| {
            let source = if n % 2 == 0 { &launcher } else { &clone };
            source
                .op(Arc::new(NoopSink), CancellationToken::new())
                .id()
                .get()
        })
        .collect();
    let mut unique = ids.clone();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(unique.len(), ids.len(), "ids repeat: {ids:?}");
}

// covers: AC-4 (build facts never carry the ID itself)
#[test]
fn build_info_names_this_build_and_only_says_whether_an_id_exists() {
    let info: BuildInfo = build_info();
    assert_eq!(info.version, env!("CARGO_PKG_VERSION"));
    assert!(!info.target.is_empty());

    let json = serde_json::to_value(&info).unwrap();
    let fields: Vec<&str> = json
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(fields, ["msaClientIdConfigured", "target", "version"]);
    assert!(json["msaClientIdConfigured"].is_boolean());
}
