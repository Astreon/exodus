//! Every text Exodus shows, rendered from the Fluent files in `locales/`.
//!
//! Only English exists for now. Fluent types never appear in the public API;
//! callers pass [`MessageArgs`].

use std::{borrow::Cow, collections::BTreeMap, path::Path, sync::LazyLock};

use fluent::{FluentArgs, FluentValue};
use i18n_embed::{
    I18nAssets, LanguageLoader, fluent::FluentLanguageLoader, unic_langid::LanguageIdentifier,
};
use rust_embed::RustEmbed;
use serde::Serialize;

const FALLBACK_LANGUAGE: &str = "en";

#[derive(RustEmbed)]
#[folder = "locales/"]
struct Locales;

/// Serves every `.ftl` file of a language, not just `<domain>.ftl`, so the
/// texts can be split into one file per module.
struct AllFilesOfLanguage;

impl I18nAssets for AllFilesOfLanguage {
    fn get_files(&self, file_path: &str) -> Vec<Cow<'_, [u8]>> {
        let Some((language, _)) = file_path.split_once('/') else {
            return Vec::new();
        };
        let prefix = format!("{language}/");
        let mut names: Vec<_> = Locales::iter()
            .filter(|name| name.starts_with(&prefix) && name.ends_with(".ftl"))
            .collect();
        names.sort();
        names
            .iter()
            .filter_map(|name| Locales::get(name))
            .map(|file| file.data)
            .collect()
    }

    fn filenames_iter(&self) -> Box<dyn Iterator<Item = String> + '_> {
        Box::new(Locales::iter().map(|name| name.into_owned()))
    }
}

static LOADER: LazyLock<FluentLanguageLoader> = LazyLock::new(|| {
    let language: LanguageIdentifier = FALLBACK_LANGUAGE
        .parse()
        .expect("fallback language is a valid identifier");
    let loader = FluentLanguageLoader::new("exodus", language);
    loader
        .load_fallback_language(&AllFilesOfLanguage)
        .expect("embedded English texts load");
    // Isolation marks would show up as stray characters in terminals.
    loader.set_use_isolating(false);
    loader
});

/// Renders message `id` in English.
///
/// A missing message or variable never panics: it is logged, and the text
/// falls back to the ID or shows the variable name.
pub fn message(id: &str, args: &MessageArgs) -> String {
    match try_message(id, args) {
        Ok(text) => text,
        Err(MessageError::Missing { id }) => {
            tracing::warn!(id, "no text for message");
            id
        }
        Err(MessageError::Format { id, text, problems }) => {
            tracing::warn!(id, ?problems, "message rendered with problems");
            text
        }
    }
}

/// Renders message `id`, reporting a missing message or variable as an error.
pub fn try_message(id: &str, args: &MessageArgs) -> Result<String, MessageError> {
    let fluent_args = args.to_fluent();
    LOADER
        .with_fluent_message_and_bundle(id, |message, bundle| {
            let pattern = message.value()?;
            let mut errors = Vec::new();
            let text = bundle.format_pattern(pattern, fluent_args.as_ref(), &mut errors);
            Some((text.into_owned(), errors))
        })
        .flatten()
        .ok_or_else(|| MessageError::Missing { id: id.to_owned() })
        .and_then(|(text, errors)| {
            if errors.is_empty() {
                Ok(text)
            } else {
                Err(MessageError::Format {
                    id: id.to_owned(),
                    text,
                    problems: errors.iter().map(ToString::to_string).collect(),
                })
            }
        })
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum MessageError {
    #[error("no text for message `{id}`")]
    Missing { id: String },
    #[error("message `{id}` rendered with problems: {problems:?}")]
    Format {
        id: String,
        text: String,
        problems: Vec<String>,
    },
}

/// Variables of a message, by name.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(transparent)]
pub struct MessageArgs(BTreeMap<String, MessageValue>);

impl MessageArgs {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a variable, builder style.
    #[must_use]
    pub fn with(mut self, name: impl Into<String>, value: impl Into<MessageValue>) -> Self {
        self.insert(name, value);
        self
    }

    pub fn insert(&mut self, name: impl Into<String>, value: impl Into<MessageValue>) {
        self.0.insert(name.into(), value.into());
    }

    pub fn get(&self, name: &str) -> Option<&MessageValue> {
        self.0.get(name)
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &MessageValue)> {
        self.0.iter().map(|(name, value)| (name.as_str(), value))
    }

    fn to_fluent(&self) -> Option<FluentArgs<'_>> {
        if self.is_empty() {
            return None;
        }
        let mut args = FluentArgs::new();
        for (name, value) in self.iter() {
            let value = match value {
                MessageValue::Text(text) => FluentValue::from(text.as_str()),
                MessageValue::Number(number) => FluentValue::from(*number),
            };
            args.set(name, value);
        }
        Some(args)
    }
}

/// A message variable: text or a number.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(untagged)]
pub enum MessageValue {
    Text(String),
    Number(f64),
}

impl From<String> for MessageValue {
    fn from(text: String) -> Self {
        Self::Text(text)
    }
}

impl From<&str> for MessageValue {
    fn from(text: &str) -> Self {
        Self::Text(text.to_owned())
    }
}

impl From<&Path> for MessageValue {
    fn from(path: &Path) -> Self {
        Self::Text(path.display().to_string())
    }
}

impl From<f64> for MessageValue {
    fn from(number: f64) -> Self {
        Self::Number(number)
    }
}

impl From<u32> for MessageValue {
    fn from(number: u32) -> Self {
        Self::Number(number.into())
    }
}

impl From<u64> for MessageValue {
    fn from(number: u64) -> Self {
        // Fluent numbers are f64; display only, so precision loss is fine.
        Self::Number(number as f64)
    }
}

#[cfg(test)]
mod tests {
    use fluent::FluentResource;

    use super::*;
    use crate::{Localize, config, error, instance, launcher, settings, storage};

    fn all_error_samples() -> Vec<Box<dyn Localize>> {
        [
            error::error_samples(),
            config::error_samples(),
            launcher::error_samples(),
            storage::error_samples(),
            instance::error_samples(),
            settings::error_samples(),
        ]
        .into_iter()
        .flatten()
        .collect()
    }

    #[test]
    fn every_error_key_has_a_text_and_gets_all_its_variables() {
        let samples = all_error_samples();
        assert!(!samples.is_empty());
        for error in samples {
            let text = try_message(error.key(), &error.args())
                .unwrap_or_else(|problem| panic!("{error:?}: {problem}"));
            assert!(!text.trim().is_empty(), "{} renders empty", error.key());
        }
    }

    #[test]
    fn every_ftl_file_parses_cleanly() {
        let mut count = 0;
        for name in Locales::iter() {
            let data = Locales::get(&name).expect("listed file exists").data;
            let source = String::from_utf8(data.into_owned()).expect("ftl is UTF-8");
            if let Err((_, errors)) = FluentResource::try_new(source) {
                panic!("{name} has syntax errors: {errors:?}");
            }
            count += 1;
        }
        assert!(count >= 2, "texts from several files are embedded");
    }

    #[test]
    fn core_error_becomes_english_text() {
        let view = crate::ErrorView::from(&crate::Cancelled);
        assert_eq!(view.message(), "The operation was cancelled.");
    }

    #[test]
    fn messages_from_separate_files_are_all_loaded() {
        assert!(try_message("error-cancelled", &MessageArgs::new()).is_ok());
        assert!(try_message("config-no-home-dir", &MessageArgs::new()).is_ok());
        assert!(try_message("cli-cancelled", &MessageArgs::new()).is_ok());
    }

    #[test]
    fn variables_are_filled_in_without_isolation_marks() {
        let args = MessageArgs::new().with("reason", "boom");
        assert_eq!(
            try_message("launcher-http-client", &args).unwrap(),
            "Could not set up the network client: boom"
        );
    }

    #[test]
    fn missing_variable_is_reported() {
        let result = try_message("launcher-http-client", &MessageArgs::new());
        assert!(matches!(result, Err(MessageError::Format { .. })));
    }

    #[test]
    fn missing_message_falls_back_to_its_id() {
        assert_eq!(
            try_message("no-such-message", &MessageArgs::new()),
            Err(MessageError::Missing {
                id: "no-such-message".into()
            })
        );
        assert_eq!(
            message("no-such-message", &MessageArgs::new()),
            "no-such-message"
        );
    }

    #[test]
    fn args_serialize_as_a_plain_map() {
        let args = MessageArgs::new()
            .with("name", "Steve")
            .with("count", 3_u32);
        assert_eq!(
            serde_json::to_value(&args).unwrap(),
            serde_json::json!({ "count": 3.0, "name": "Steve" })
        );
    }
}
