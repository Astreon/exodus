//! Game settings: built in defaults, global settings and per instance
//! overrides, merged field by field (spec 0002).
//!
//! Fields that can run code (`java`, `jvm_args`, `env`, `hooks`) only count
//! from an instance when the user trusted exactly those values.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::{Localize, MessageArgs, instance::InstanceId, storage::Versioned};

/// Current version of `settings.json`.
pub const SETTINGS_FORMAT: u32 = 1;

/// Built in minimum memory in MB.
pub const DEFAULT_MEMORY_MIN_MB: u32 = 512;
/// Built in maximum memory in MB.
pub const DEFAULT_MEMORY_MAX_MB: u32 = 4096;
/// Built in window width.
pub const DEFAULT_WINDOW_WIDTH: u32 = 854;
/// Built in window height.
pub const DEFAULT_WINDOW_HEIGHT: u32 = 480;
/// The smallest maximum memory a layer may set.
pub const MIN_MEMORY_MAX_MB: u32 = 256;

/// One layer of game settings. Every field is optional: a missing field
/// inherits from the layer below, a set field replaces it completely.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct GameSettings {
    /// Which Java runs the game. Runs code.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub java: Option<Java>,
    /// `-Xms` in MB.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_min_mb: Option<u32>,
    /// `-Xmx` in MB.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_max_mb: Option<u32>,
    /// Extra JVM arguments. Runs code.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jvm_args: Option<Vec<String>>,
    /// Window size; each part inherits on its own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window: Option<WindowSettings>,
    /// Extra environment variables for the game. Runs code.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub env: Option<BTreeMap<String, String>>,
    /// Commands around the game; each part inherits on its own. Runs code.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hooks: Option<Hooks>,
    /// Fields this version does not know, kept as they are.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl GameSettings {
    /// Whether this layer sets any field that can run code.
    pub fn sets_code_fields(&self) -> bool {
        self.java.is_some() || self.jvm_args.is_some() || self.env.is_some() || self.hooks.is_some()
    }

    /// This layer without the fields that can run code.
    fn without_code_fields(&self) -> Self {
        Self {
            java: None,
            jvm_args: None,
            env: None,
            hooks: None,
            ..self.clone()
        }
    }
}

/// Which Java runs the game: `"auto"` or `{ "path": "…" }`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Java {
    /// Picked by Exodus. Until feature 10 that is `java` from `PATH`.
    #[default]
    Auto,
    /// This Java executable.
    Path(String),
}

/// Window settings of one layer.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct WindowSettings {
    /// Width in pixels.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    /// Height in pixels.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
    /// Start in fullscreen.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fullscreen: Option<bool>,
    /// Fields this version does not know, kept as they are.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// Commands run around the game, each an argv list (no shell).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Hooks {
    /// Runs before the game starts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pre_launch: Option<Vec<String>>,
    /// Runs after the game exits.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub post_exit: Option<Vec<String>>,
    /// Wraps the Java command.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wrapper: Option<Vec<String>>,
    /// Fields this version does not know, kept as they are.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// `settings.json`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GlobalSettings {
    /// Always [`SETTINGS_FORMAT`] once loaded.
    pub format_version: u32,
    /// Settings every instance inherits.
    #[serde(default)]
    pub game: GameSettings,
    /// Instances whose code fields the user trusted, with the hash of exactly
    /// those values. At most one entry per instance.
    #[serde(default)]
    pub trusted_overrides: Vec<TrustedOverride>,
    /// Fields this version does not know, kept as they are.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Default for GlobalSettings {
    fn default() -> Self {
        Self {
            format_version: SETTINGS_FORMAT,
            game: GameSettings::default(),
            trusted_overrides: Vec::new(),
            extra: Map::new(),
        }
    }
}

impl Versioned for GlobalSettings {
    const CURRENT: u32 = SETTINGS_FORMAT;
}

impl GlobalSettings {
    /// Whether `hash` is the trusted hash of instance `id`.
    pub fn is_trusted(&self, id: &InstanceId, hash: &str) -> bool {
        self.trusted_overrides
            .iter()
            .any(|entry| entry.instance == *id && entry.sha256 == hash)
    }

    /// Records `hash` as the trusted hash of `id`, or removes the entry when
    /// `hash` is `None`. Returns whether anything changed.
    pub(crate) fn set_trusted(&mut self, id: &InstanceId, hash: Option<String>) -> bool {
        let before = self.trusted_overrides.clone();
        self.trusted_overrides.retain(|entry| entry.instance != *id);
        if let Some(sha256) = hash {
            self.trusted_overrides.push(TrustedOverride {
                instance: id.clone(),
                sha256,
                extra: Map::new(),
            });
        }
        before != self.trusted_overrides
    }
}

/// One trusted instance.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TrustedOverride {
    /// The instance.
    pub instance: InstanceId,
    /// SHA 256 of its code fields as lowercase hex, see [`code_overrides_hash`].
    pub sha256: String,
    /// Fields this version does not know, kept as they are.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// The settings a game starts with, after merging every layer.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[non_exhaustive]
pub struct EffectiveSettings {
    /// Which Java runs the game.
    pub java: Java,
    /// `-Xms` in MB.
    pub memory_min_mb: u32,
    /// `-Xmx` in MB.
    pub memory_max_mb: u32,
    /// Extra JVM arguments.
    pub jvm_args: Vec<String>,
    /// Window width.
    pub width: u32,
    /// Window height.
    pub height: u32,
    /// Start in fullscreen.
    pub fullscreen: bool,
    /// Extra environment variables.
    pub env: BTreeMap<String, String>,
    /// Runs before the game starts.
    pub pre_launch: Option<Vec<String>>,
    /// Runs after the game exits.
    pub post_exit: Option<Vec<String>>,
    /// Wraps the Java command.
    pub wrapper: Option<Vec<String>>,
    /// The instance sets code fields the user has not trusted; they were
    /// ignored and the global values apply.
    pub untrusted_overrides: bool,
}

impl EffectiveSettings {
    /// The built in defaults.
    pub fn builtin() -> Self {
        Self {
            java: Java::Auto,
            memory_min_mb: DEFAULT_MEMORY_MIN_MB,
            memory_max_mb: DEFAULT_MEMORY_MAX_MB,
            jvm_args: Vec::new(),
            width: DEFAULT_WINDOW_WIDTH,
            height: DEFAULT_WINDOW_HEIGHT,
            fullscreen: false,
            env: BTreeMap::new(),
            pre_launch: None,
            post_exit: None,
            wrapper: None,
            untrusted_overrides: false,
        }
    }

    /// `layer` on top of `self`: every set field replaces the value below.
    fn apply(self, layer: &GameSettings) -> Self {
        let window = layer.window.clone().unwrap_or_default();
        let hooks = layer.hooks.clone().unwrap_or_default();
        Self {
            java: layer.java.clone().unwrap_or(self.java),
            memory_min_mb: layer.memory_min_mb.unwrap_or(self.memory_min_mb),
            memory_max_mb: layer.memory_max_mb.unwrap_or(self.memory_max_mb),
            jvm_args: layer.jvm_args.clone().unwrap_or(self.jvm_args),
            width: window.width.unwrap_or(self.width),
            height: window.height.unwrap_or(self.height),
            fullscreen: window.fullscreen.unwrap_or(self.fullscreen),
            env: layer.env.clone().unwrap_or(self.env),
            pre_launch: hooks.pre_launch.or(self.pre_launch),
            post_exit: hooks.post_exit.or(self.post_exit),
            wrapper: hooks.wrapper.or(self.wrapper),
            untrusted_overrides: self.untrusted_overrides,
        }
    }
}

/// Merges built in defaults, `global` and `instance`, field by field. The
/// instance's code fields only count when `code_trusted`; otherwise they are
/// ignored and the result reports `untrusted_overrides`. Pure.
pub fn resolve(
    global: &GameSettings,
    instance: &GameSettings,
    code_trusted: bool,
) -> Result<EffectiveSettings, SettingsError> {
    let untrusted = instance.sets_code_fields() && !code_trusted;
    let instance = if untrusted {
        instance.without_code_fields()
    } else {
        instance.clone()
    };
    let effective = EffectiveSettings {
        untrusted_overrides: untrusted,
        ..EffectiveSettings::builtin().apply(global).apply(&instance)
    };
    if effective.memory_min_mb > effective.memory_max_mb {
        return Err(SettingsError::MemoryRange {
            min: effective.memory_min_mb,
            max: effective.memory_max_mb,
        });
    }
    Ok(effective)
}

/// Checks the fields one layer sets, on their own. Pure.
pub fn validate_layer(layer: &GameSettings) -> Result<(), SettingsError> {
    let invalid = |field: &str| {
        Err(SettingsError::Invalid {
            field: field.to_owned(),
        })
    };
    let positive = [
        ("memory_min_mb", layer.memory_min_mb),
        ("memory_max_mb", layer.memory_max_mb),
        ("window.width", layer.window.as_ref().and_then(|w| w.width)),
        (
            "window.height",
            layer.window.as_ref().and_then(|w| w.height),
        ),
    ];
    if let Some((field, _)) = positive.iter().find(|(_, value)| *value == Some(0)) {
        return invalid(field);
    }
    if layer
        .memory_max_mb
        .is_some_and(|max| max < MIN_MEMORY_MAX_MB)
    {
        return invalid("memory_max_mb");
    }
    if let (Some(min), Some(max)) = (layer.memory_min_mb, layer.memory_max_mb)
        && min > max
    {
        return invalid("memory_min_mb");
    }
    if layer
        .env
        .as_ref()
        .is_some_and(|env| env.keys().any(|key| key.is_empty() || key.contains('=')))
    {
        return invalid("env");
    }
    if let Some(hooks) = &layer.hooks {
        let lists = [
            ("hooks.pre_launch", &hooks.pre_launch),
            ("hooks.post_exit", &hooks.post_exit),
            ("hooks.wrapper", &hooks.wrapper),
        ];
        if let Some((field, _)) = lists
            .iter()
            .find(|(_, list)| list.as_ref().is_some_and(Vec::is_empty))
        {
            return invalid(field);
        }
    }
    if let Some(Java::Path(path)) = &layer.java
        && path.trim().is_empty()
    {
        return invalid("java");
    }
    Ok(())
}

/// The fields that can run code, in a fixed order, as hashed.
#[derive(Serialize)]
struct CodeOverrides<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    java: Option<&'a Java>,
    #[serde(skip_serializing_if = "Option::is_none")]
    jvm_args: Option<&'a Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    env: Option<&'a BTreeMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    hooks: Option<&'a Hooks>,
}

/// SHA 256 (lowercase hex) of the raw code fields an instance sets, or
/// `None` when it sets none. Pure.
pub fn code_overrides_hash(instance: &GameSettings) -> Option<String> {
    if !instance.sets_code_fields() {
        return None;
    }
    let fields = CodeOverrides {
        java: instance.java.as_ref(),
        jvm_args: instance.jvm_args.as_ref(),
        env: instance.env.as_ref(),
        hooks: instance.hooks.as_ref(),
    };
    let bytes = serde_json::to_vec(&fields).expect("settings always serialize");
    let digest = ring::digest::digest(&ring::digest::SHA256, &bytes);
    Some(crate::storage::hex(digest.as_ref()))
}

/// Whether the code fields of `instance` count, given the global settings.
pub(crate) fn code_trusted(
    global: &GlobalSettings,
    id: &InstanceId,
    instance: &GameSettings,
) -> bool {
    code_overrides_hash(instance).is_none_or(|hash| global.is_trusted(id, &hash))
}

/// Errors in settings.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SettingsError {
    /// A layer sets a value that is not allowed.
    #[error("the setting `{field}` has an invalid value")]
    Invalid { field: String },
    /// After merging, the minimum memory is above the maximum.
    #[error("minimum memory {min} MB is above maximum memory {max} MB")]
    MemoryRange { min: u32, max: u32 },
    /// Reading or writing `settings.json` failed.
    #[error(transparent)]
    Storage(#[from] crate::storage::StorageError),
}

impl Localize for SettingsError {
    fn key(&self) -> &'static str {
        match self {
            Self::Invalid { .. } => "settings-invalid",
            Self::MemoryRange { .. } => "settings-memory-range",
            Self::Storage(inner) => inner.key(),
        }
    }

    fn args(&self) -> MessageArgs {
        match self {
            Self::Invalid { field } => MessageArgs::new().with("field", field.as_str()),
            Self::MemoryRange { min, max } => {
                MessageArgs::new().with("min", *min).with("max", *max)
            }
            Self::Storage(inner) => inner.args(),
        }
    }
}

#[cfg(test)]
pub(crate) fn error_samples() -> Vec<Box<dyn Localize>> {
    vec![
        Box::new(SettingsError::Invalid {
            field: "memory_max_mb".into(),
        }),
        Box::new(SettingsError::MemoryRange {
            min: 8192,
            max: 4096,
        }),
    ]
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn layer(value: Value) -> GameSettings {
        serde_json::from_value(value).unwrap()
    }

    fn id(text: &str) -> InstanceId {
        InstanceId::parse(text).unwrap()
    }

    #[test]
    fn layers_merge_field_by_field() {
        let global = layer(
            json!({ "memory_max_mb": 8192, "jvm_args": ["-Dglobal"], "window": { "width": 1280 } }),
        );
        let instance = layer(json!({ "window": { "height": 720 }, "jvm_args": [] }));
        let effective = resolve(&global, &instance, true).unwrap();
        assert_eq!(effective.memory_min_mb, DEFAULT_MEMORY_MIN_MB);
        assert_eq!(effective.memory_max_mb, 8192);
        assert_eq!((effective.width, effective.height), (1280, 720));
        assert!(
            effective.jvm_args.is_empty(),
            "an empty list replaces the global one"
        );
        assert!(!effective.untrusted_overrides);
    }

    #[test]
    fn untrusted_code_fields_fall_back_to_global_values() {
        let global = layer(json!({ "env": { "A": "global" } }));
        let instance = layer(json!({ "env": { "A": "mine" }, "window": { "width": 1000 } }));
        let effective = resolve(&global, &instance, false).unwrap();
        assert_eq!(effective.env["A"], "global");
        assert_eq!(effective.width, 1000, "other fields still count");
        assert!(effective.untrusted_overrides);

        let plain = layer(json!({ "memory_max_mb": 2048 }));
        assert!(!resolve(&global, &plain, false).unwrap().untrusted_overrides);
    }

    #[test]
    fn hooks_inherit_each_part() {
        let global = layer(json!({ "hooks": { "pre_launch": ["a"], "wrapper": ["w"] } }));
        let instance = layer(json!({ "hooks": { "wrapper": ["mine"] } }));
        let effective = resolve(&global, &instance, true).unwrap();
        assert_eq!(effective.pre_launch, Some(vec!["a".to_owned()]));
        assert_eq!(effective.wrapper, Some(vec!["mine".to_owned()]));
        assert_eq!(effective.post_exit, None);
    }

    #[test]
    fn merged_memory_range_is_checked() {
        let global = layer(json!({ "memory_min_mb": 8192 }));
        let instance = layer(json!({ "memory_max_mb": 4096 }));
        assert!(validate_layer(&global).is_ok());
        assert!(validate_layer(&instance).is_ok());
        assert!(matches!(
            resolve(&global, &instance, true),
            Err(SettingsError::MemoryRange {
                min: 8192,
                max: 4096
            })
        ));
    }

    #[test]
    fn each_layer_checks_its_own_fields() {
        let cases = [
            (json!({ "memory_max_mb": 100 }), "memory_max_mb"),
            (json!({ "memory_min_mb": 0 }), "memory_min_mb"),
            (
                json!({ "memory_min_mb": 2048, "memory_max_mb": 1024 }),
                "memory_min_mb",
            ),
            (json!({ "window": { "width": 0 } }), "window.width"),
            (json!({ "env": { "": "x" } }), "env"),
            (json!({ "env": { "A=B": "x" } }), "env"),
            (json!({ "hooks": { "wrapper": [] } }), "hooks.wrapper"),
            (json!({ "java": { "path": " " } }), "java"),
        ];
        for (value, expected) in cases {
            match validate_layer(&layer(value.clone())) {
                Err(SettingsError::Invalid { field }) => assert_eq!(field, expected, "{value}"),
                other => panic!("{value}: {other:?}"),
            }
        }
        assert!(validate_layer(&layer(json!({ "memory_max_mb": 256, "java": "auto" }))).is_ok());
    }

    #[test]
    fn java_serializes_as_auto_or_path() {
        assert_eq!(serde_json::to_value(Java::Auto).unwrap(), json!("auto"));
        assert_eq!(
            serde_json::to_value(Java::Path("/j".into())).unwrap(),
            json!({ "path": "/j" })
        );
    }

    #[test]
    fn hash_covers_only_code_fields_and_is_order_independent() {
        assert_eq!(
            code_overrides_hash(&layer(json!({ "memory_max_mb": 2048 }))),
            None
        );
        let a = code_overrides_hash(&layer(json!({ "env": { "B": "2", "A": "1" } }))).unwrap();
        let b = code_overrides_hash(&layer(
            json!({ "env": { "A": "1", "B": "2" }, "memory_max_mb": 1024 }),
        ))
        .unwrap();
        assert_eq!(a, b);
        assert_eq!(a.len(), 64);
        assert!(
            a.chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
        );
        let c = code_overrides_hash(&layer(json!({ "env": { "A": "1", "B": "3" } }))).unwrap();
        assert_ne!(a, c);
    }

    #[test]
    fn hash_is_sha256_of_the_compact_json() {
        let hash = code_overrides_hash(&layer(json!({ "jvm_args": ["-X"] }))).unwrap();
        let expected = ring::digest::digest(&ring::digest::SHA256, br#"{"jvm_args":["-X"]}"#);
        assert_eq!(hash, crate::storage::hex(expected.as_ref()));
    }

    #[test]
    fn trust_entries_are_replaced_and_removed() {
        let mut global = GlobalSettings::default();
        let survival = id("survival");
        assert!(global.set_trusted(&survival, Some("aa".into())));
        assert!(global.set_trusted(&survival, Some("bb".into())));
        assert_eq!(global.trusted_overrides.len(), 1);
        assert!(global.is_trusted(&survival, "bb"));
        assert!(!global.set_trusted(&survival, Some("bb".into())));
        assert!(global.set_trusted(&survival, None));
        assert!(global.trusted_overrides.is_empty());
    }
}
