//! Typed events that long operations send to the surface.

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use serde::Serialize;
use tokio::sync::mpsc;

use crate::{ErrorView, OpContext};

/// Identifies one operation, unique per [`Launcher`](crate::Launcher).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub struct OperationId(u64);

impl OperationId {
    pub(crate) fn new(id: u64) -> Self {
        Self(id)
    }

    pub fn get(self) -> u64 {
        self.0
    }
}

/// What an operation reports while it runs.
///
/// Each operation sends `Started` first and then exactly one of `Finished`,
/// `Failed` or `Cancelled`; [`OpContext::run`] takes care of both.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[non_exhaustive]
pub enum Event {
    Started {
        op: OperationId,
        /// Stable name of the operation, such as `download`.
        kind: &'static str,
    },
    Progress {
        op: OperationId,
        done: u64,
        /// `None` while the total is not known yet.
        total: Option<u64>,
    },
    Log {
        op: OperationId,
        line: String,
    },
    Finished {
        op: OperationId,
    },
    Failed {
        op: OperationId,
        error: ErrorView,
    },
    Cancelled {
        op: OperationId,
    },
}

impl Event {
    pub fn op(&self) -> OperationId {
        match self {
            Self::Started { op, .. }
            | Self::Progress { op, .. }
            | Self::Log { op, .. }
            | Self::Finished { op }
            | Self::Failed { op, .. }
            | Self::Cancelled { op } => *op,
        }
    }
}

/// Receives the events of an operation. Must never block.
pub trait EventSink: Send + Sync + 'static {
    fn emit(&self, event: Event);
}

/// Drops every event.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoopSink;

impl EventSink for NoopSink {
    fn emit(&self, _event: Event) {}
}

/// Forwards events into an unbounded tokio channel.
///
/// Unbounded is fine because progress is throttled where it is produced
/// ([`ProgressReporter`]).
#[derive(Clone, Debug)]
pub struct ChannelSink(mpsc::UnboundedSender<Event>);

impl ChannelSink {
    /// A sink plus the receiving end for the surface.
    pub fn new() -> (Self, mpsc::UnboundedReceiver<Event>) {
        let (sender, receiver) = mpsc::unbounded_channel();
        (Self(sender), receiver)
    }
}

impl From<mpsc::UnboundedSender<Event>> for ChannelSink {
    fn from(sender: mpsc::UnboundedSender<Event>) -> Self {
        Self(sender)
    }
}

impl EventSink for ChannelSink {
    fn emit(&self, event: Event) {
        // A closed channel means the surface stopped listening; nothing to do.
        let _ = self.0.send(event);
    }
}

/// A source of the current time, swappable in tests.
pub trait Clock: Send + Sync + 'static {
    fn now(&self) -> Instant;
}

/// The real clock.
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Instant {
        Instant::now()
    }
}

/// Sends `Progress` events at most about ten times per second, and always
/// delivers the last state, on [`finish`](Self::finish) or when dropped.
pub struct ProgressReporter {
    op: OperationId,
    sink: Arc<dyn EventSink>,
    clock: Box<dyn Clock>,
    last_sent: Option<Instant>,
    pending: Option<(u64, Option<u64>)>,
}

impl ProgressReporter {
    pub const MIN_INTERVAL: Duration = Duration::from_millis(100);

    pub fn new(ctx: &OpContext) -> Self {
        Self::with_clock(ctx, SystemClock)
    }

    pub fn with_clock(ctx: &OpContext, clock: impl Clock) -> Self {
        Self {
            op: ctx.id(),
            sink: ctx.sink(),
            clock: Box::new(clock),
            last_sent: None,
            pending: None,
        }
    }

    /// Records progress; sends it now if enough time passed or it is complete.
    pub fn update(&mut self, done: u64, total: Option<u64>) {
        let now = self.clock.now();
        let due = self
            .last_sent
            .is_none_or(|sent| now.duration_since(sent) >= Self::MIN_INTERVAL);
        let complete = total.is_some_and(|total| done >= total);
        if due || complete {
            self.send(now, done, total);
        } else {
            self.pending = Some((done, total));
        }
    }

    /// Sends the last state if it was held back.
    pub fn finish(mut self) {
        self.flush();
    }

    fn flush(&mut self) {
        if let Some((done, total)) = self.pending.take() {
            let now = self.clock.now();
            self.send(now, done, total);
        }
    }

    fn send(&mut self, now: Instant, done: u64, total: Option<u64>) {
        self.pending = None;
        self.last_sent = Some(now);
        self.sink.emit(Event::Progress {
            op: self.op,
            done,
            total,
        });
    }
}

impl Drop for ProgressReporter {
    fn drop(&mut self) {
        self.flush();
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use crate::{CancellationToken, Launcher, LauncherConfig};

    /// A clock that only moves when the test says so.
    #[derive(Clone)]
    struct ManualClock(Arc<Mutex<Instant>>);

    impl ManualClock {
        fn new() -> Self {
            Self(Arc::new(Mutex::new(Instant::now())))
        }

        fn advance(&self, by: Duration) {
            *self.0.lock().unwrap() += by;
        }
    }

    impl Clock for ManualClock {
        fn now(&self) -> Instant {
            *self.0.lock().unwrap()
        }
    }

    fn context() -> (OpContext, mpsc::UnboundedReceiver<Event>) {
        let launcher = Launcher::new(LauncherConfig::new("data")).unwrap();
        let (sink, events) = ChannelSink::new();
        (
            launcher.op(Arc::new(sink), CancellationToken::new()),
            events,
        )
    }

    fn progress(events: &mut mpsc::UnboundedReceiver<Event>) -> Vec<(u64, Option<u64>)> {
        let mut seen = Vec::new();
        while let Ok(event) = events.try_recv() {
            if let Event::Progress { done, total, .. } = event {
                seen.push((done, total));
            }
        }
        seen
    }

    #[test]
    fn event_arrives_through_channel_sink() {
        let (ctx, mut events) = context();
        ctx.log("hello");
        assert_eq!(
            events.try_recv().unwrap(),
            Event::Log {
                op: ctx.id(),
                line: "hello".into()
            }
        );
    }

    #[test]
    fn progress_is_throttled_but_the_last_state_arrives() {
        let (ctx, mut events) = context();
        let clock = ManualClock::new();
        let mut reporter = ProgressReporter::with_clock(&ctx, clock.clone());

        reporter.update(1, Some(1000));
        for done in 2..=50 {
            clock.advance(Duration::from_millis(1));
            reporter.update(done, Some(1000));
        }
        clock.advance(ProgressReporter::MIN_INTERVAL);
        reporter.update(60, Some(1000));
        reporter.update(70, Some(1000));
        reporter.finish();

        assert_eq!(
            progress(&mut events),
            vec![(1, Some(1000)), (60, Some(1000)), (70, Some(1000))]
        );
    }

    #[test]
    fn completion_is_sent_immediately() {
        let (ctx, mut events) = context();
        let mut reporter = ProgressReporter::with_clock(&ctx, ManualClock::new());
        reporter.update(1, Some(2));
        reporter.update(2, Some(2));
        assert_eq!(progress(&mut events), vec![(1, Some(2)), (2, Some(2))]);
    }

    #[test]
    fn dropping_the_reporter_sends_the_held_back_state() {
        let (ctx, mut events) = context();
        let mut reporter = ProgressReporter::with_clock(&ctx, ManualClock::new());
        reporter.update(1, None);
        reporter.update(2, None);
        drop(reporter);
        assert_eq!(progress(&mut events), vec![(1, None), (2, None)]);
    }

    #[test]
    fn events_serialize_with_a_type_tag() {
        let event = Event::Progress {
            op: OperationId::new(7),
            done: 3,
            total: None,
        };
        assert_eq!(
            serde_json::to_value(&event).unwrap(),
            serde_json::json!({ "type": "progress", "op": 7, "done": 3, "total": null })
        );
    }
}
