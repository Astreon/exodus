//! The error model shared by every module.
//!
//! Each module has its own `thiserror` enum that implements [`Localize`].
//! Text is produced only when a surface shows the error, from the Fluent
//! message named by [`Localize::key`]. Every key needs an entry in
//! `locales/en/<module>.ftl`, and every variant a sample in the module's
//! `error_samples()`, which the key test in `i18n` checks.

use std::error::Error;

use serde::Serialize;

use crate::{MessageArgs, i18n};

/// An error a surface can show to a person.
pub trait Localize: Error + Send + Sync + 'static {
    /// The Fluent message ID. Stable: surfaces may match on it.
    fn key(&self) -> &'static str;

    /// The variables the message needs.
    fn args(&self) -> MessageArgs {
        MessageArgs::new()
    }

    /// True when the operation stopped because it was cancelled.
    fn is_cancelled(&self) -> bool {
        false
    }
}

/// A serializable snapshot of an error, for events and surfaces.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ErrorView {
    pub key: String,
    pub args: MessageArgs,
    /// Technical text (the error and its causes) for logs and bug reports.
    pub detail: String,
}

impl ErrorView {
    /// The English text for this error.
    pub fn message(&self) -> String {
        i18n::message(&self.key, &self.args)
    }
}

impl<E: Localize + ?Sized> From<&E> for ErrorView {
    fn from(error: &E) -> Self {
        Self {
            key: error.key().to_owned(),
            args: error.args(),
            detail: detail(error),
        }
    }
}

/// `error: cause: cause of cause`.
fn detail(error: &(impl Error + ?Sized)) -> String {
    let mut text = error.to_string();
    let mut source = error.source();
    while let Some(cause) = source {
        text.push_str(": ");
        text.push_str(&cause.to_string());
        source = cause.source();
    }
    text
}

/// An operation stopped because its cancellation token fired.
///
/// Module errors wrap it (`Cancelled(#[from] Cancelled)`) and forward
/// [`Localize::is_cancelled`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("the operation was cancelled")]
pub struct Cancelled;

impl Localize for Cancelled {
    fn key(&self) -> &'static str {
        "error-cancelled"
    }

    fn is_cancelled(&self) -> bool {
        true
    }
}

#[cfg(test)]
pub(crate) fn error_samples() -> Vec<Box<dyn Localize>> {
    vec![Box::new(Cancelled)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, thiserror::Error)]
    #[error("outer")]
    struct Outer(#[source] std::io::Error);

    impl Localize for Outer {
        fn key(&self) -> &'static str {
            "error-cancelled"
        }
    }

    #[test]
    fn detail_contains_the_cause_chain() {
        let error = Outer(std::io::Error::other("inner"));
        assert_eq!(ErrorView::from(&error).detail, "outer: inner");
    }

    #[test]
    fn cancelled_reports_itself_as_cancelled() {
        assert!(Cancelled.is_cancelled());
        assert_eq!(ErrorView::from(&Cancelled).key, "error-cancelled");
    }
}
