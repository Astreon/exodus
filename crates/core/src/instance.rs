//! Instances: one folder per instance with `instance.json` and the game
//! folder `minecraft/` (spec 0002).
//!
//! The ID is a slug of the first name, is the folder name, and never changes.
//! Renaming only changes `name`.

use std::{
    collections::HashSet,
    fmt, fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Map, Value};
use unicode_normalization::{UnicodeNormalization, char::is_combining_mark};

use crate::{
    ErrorView, Localize, MessageArgs,
    settings::{GameSettings, SettingsError},
    storage::{self, DataLayout, StorageError, Versioned},
};

/// Current version of `instance.json`.
pub const INSTANCE_FORMAT: u32 = 1;

/// Longest slug, including a collision suffix.
pub const MAX_ID_LEN: usize = 48;

/// Longest instance name, in characters, after trimming.
pub const MAX_NAME_LEN: usize = 64;

const FALLBACK_ID: &str = "instance";

const RESERVED: &[&str] = &[
    "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8",
    "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
];

/// The ID of an instance: a slug that is also its folder name.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct InstanceId(String);

impl InstanceId {
    /// Accepts `id` if it passes [`validate_id`].
    pub fn parse(id: &str) -> Result<Self, InstanceError> {
        if validate_id(id) {
            Ok(Self(id.to_owned()))
        } else {
            Err(InstanceError::InvalidId { id: id.to_owned() })
        }
    }

    /// The ID as text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for InstanceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for InstanceId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let id = String::deserialize(deserializer)?;
        Self::parse(&id).map_err(serde::de::Error::custom)
    }
}

/// The base slug of `name` (rules 1 to 4 of spec 0002), without collision
/// handling. Pure.
pub fn slugify(name: &str) -> String {
    let lowered = name
        .to_lowercase()
        .replace('ä', "ae")
        .replace('ö', "oe")
        .replace('ü', "ue")
        .replace('ß', "ss");
    let plain: String = lowered.nfkd().filter(|c| !is_combining_mark(*c)).collect();
    let dashed = plain
        .chars()
        .map(|c| {
            if c.is_ascii_lowercase() || c.is_ascii_digit() {
                c
            } else {
                '-'
            }
        })
        .collect::<String>();
    let collapsed = dashed
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    let truncated = truncate(&collapsed, MAX_ID_LEN);
    let slug = if truncated.is_empty() {
        FALLBACK_ID.to_owned()
    } else {
        truncated
    };
    if RESERVED.contains(&slug.as_str()) {
        format!("{slug}-1")
    } else {
        slug
    }
}

/// The first `max` characters of an ASCII slug, without a trailing `-`.
fn truncate(slug: &str, max: usize) -> String {
    slug.get(..max)
        .unwrap_or(slug)
        .trim_end_matches('-')
        .to_owned()
}

/// A slug of `name` that does not clash with `existing` (compared ignoring
/// case): adds `-2`, `-3` … and shortens the stem to stay within
/// [`MAX_ID_LEN`]. Pure.
pub fn unique_slug<'a>(name: &str, existing: impl IntoIterator<Item = &'a str>) -> String {
    let taken: HashSet<String> = existing.into_iter().map(str::to_lowercase).collect();
    let base = slugify(name);
    if !taken.contains(&base) {
        return base;
    }
    (2..)
        .map(|n| {
            let suffix = format!("-{n}");
            let stem = truncate(&base, MAX_ID_LEN - suffix.len());
            format!("{stem}{suffix}")
        })
        .find(|candidate| !taken.contains(candidate))
        .expect("some suffix is free")
}

/// True when `id` is a valid instance ID: its own slug and not a reserved
/// name. Rules out empty IDs, a leading `.` or `-`, capitals and separators.
pub fn validate_id(id: &str) -> bool {
    slugify(id) == id && !RESERVED.contains(&id)
}

/// Trims `name` and checks it has 1 to [`MAX_NAME_LEN`] characters.
pub fn validate_name(name: &str) -> Result<String, InstanceError> {
    let trimmed = name.trim();
    let length = trimmed.chars().count();
    if (1..=MAX_NAME_LEN).contains(&length) {
        Ok(trimmed.to_owned())
    } else {
        Err(InstanceError::InvalidName)
    }
}

/// `instances/<id>/instance.json`. The ID is the folder name, not a field.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Instance {
    /// Always [`INSTANCE_FORMAT`] once loaded; set by the core.
    pub format_version: u32,
    /// Display name, trimmed, 1 to 64 characters.
    pub name: String,
    /// Mojang version ID, such as `1.21.4`.
    pub minecraft_version: String,
    /// Mod loader, `None` for vanilla.
    #[serde(deserialize_with = "required")]
    pub loader: Option<Loader>,
    /// Overrides of the global settings.
    pub settings: GameSettings,
    /// Icon, `None` for the surface's default icon.
    #[serde(deserialize_with = "required")]
    pub icon: Option<Icon>,
    /// One group, trimmed and not empty (a blank group is stored as none).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    /// A fixed account; `None` uses the active account.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account: Option<String>,
    /// When the instance was created. Never changes.
    pub created_at: jiff::Timestamp,
    /// When the game last started.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_played_at: Option<jiff::Timestamp>,
    /// Fields this version does not know, kept as they are.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Versioned for Instance {
    const CURRENT: u32 = INSTANCE_FORMAT;
}

/// A nullable field that must still be present.
fn required<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::deserialize(deserializer)
}

/// A mod loader and its version.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Loader {
    /// `fabric`, `quilt`, `forge`, `neoforge`, or a kind a newer Exodus knows.
    pub kind: String,
    /// Loader version.
    pub version: String,
    /// Fields this version does not know, kept as they are.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// An instance icon: `{ "builtin": "…" }` or `{ "file": "…" }`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Icon {
    /// One of the icons that ship with Exodus.
    Builtin(String),
    /// A file in the instance folder.
    File(IconFile),
}

/// A plain file name in the instance folder: no separators, not `.` or `..`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct IconFile(String);

impl IconFile {
    /// The file name.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for IconFile {
    type Error = String;

    fn try_from(name: String) -> Result<Self, Self::Error> {
        let plain = !name.is_empty()
            && name != "."
            && name != ".."
            && !name.contains(['/', '\\', ':', '\0'])
            && Path::new(&name).file_name() == Some(name.as_ref());
        if plain {
            Ok(Self(name))
        } else {
            Err(format!("icon file `{name}` is not a plain file name"))
        }
    }
}

impl From<IconFile> for String {
    fn from(file: IconFile) -> Self {
        file.0
    }
}

/// What a caller gives to create an instance.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct NewInstance {
    /// Display name.
    pub name: String,
    /// Mojang version ID.
    pub minecraft_version: String,
    /// Mod loader, `None` for vanilla.
    pub loader: Option<Loader>,
    /// Overrides of the global settings. Code fields set here are trusted.
    pub settings: GameSettings,
    /// Icon.
    pub icon: Option<Icon>,
    /// Group.
    pub group: Option<String>,
    /// Fixed account.
    pub account: Option<String>,
}

impl NewInstance {
    /// A vanilla instance with no overrides.
    pub fn new(name: impl Into<String>, minecraft_version: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            minecraft_version: minecraft_version.into(),
            loader: None,
            settings: GameSettings::default(),
            icon: None,
            group: None,
            account: None,
        }
    }

    /// The instance file for this request, created at `created_at`. Pure.
    pub(crate) fn build(self, created_at: jiff::Timestamp) -> Result<Instance, InstanceError> {
        let instance = Instance {
            format_version: INSTANCE_FORMAT,
            name: self.name,
            minecraft_version: self.minecraft_version,
            loader: self.loader,
            settings: self.settings,
            icon: self.icon,
            group: self.group,
            account: self.account,
            created_at,
            last_played_at: None,
            extra: Map::new(),
        };
        normalize(instance)
    }
}

/// Trims and checks the fields a caller may change. Pure.
pub(crate) fn normalize(instance: Instance) -> Result<Instance, InstanceError> {
    let name = validate_name(&instance.name)?;
    // A blank group means no group.
    let group = instance
        .group
        .map(|group| group.trim().to_owned())
        .filter(|group| !group.is_empty());
    crate::settings::validate_layer(&instance.settings)?;
    Ok(Instance {
        name,
        group,
        ..instance
    })
}

/// One entry of [`Launcher::list_instances`](crate::Launcher::list_instances).
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum InstanceEntry {
    /// A readable instance.
    Ok {
        /// Its ID.
        id: InstanceId,
        /// Its file.
        instance: Box<Instance>,
    },
    /// A folder entry that is not a usable instance. Never changed automatically.
    Broken {
        /// The folder name (lossy if it was not UTF 8).
        id: String,
        /// Why it is broken.
        error: ErrorView,
    },
}

impl InstanceEntry {
    /// The ID or folder name.
    pub fn id(&self) -> &str {
        match self {
            Self::Ok { id, .. } => id.as_str(),
            Self::Broken { id, .. } => id,
        }
    }
}

/// Reads one instance without taking a lock.
pub(crate) fn read(layout: &DataLayout, id: &InstanceId) -> Result<Instance, InstanceError> {
    read_loaded(layout, id).map(|loaded| loaded.value)
}

/// Reads one instance, keeping whether it was migrated.
pub(crate) fn read_loaded(
    layout: &DataLayout,
    id: &InstanceId,
) -> Result<storage::Loaded<Instance>, InstanceError> {
    let dir = layout.instance_dir(id.as_str());
    let is_dir = fs::symlink_metadata(&dir).is_ok_and(|meta| meta.is_dir());
    if !is_dir {
        return Err(InstanceError::NotFound { id: id.clone() });
    }
    storage::load::<Instance>(&layout.instance_file(id.as_str()))?
        .ok_or_else(|| InstanceError::NotFound { id: id.clone() })
}

/// Lists `instances/`, skipping names that start with `.`. Sorted by the
/// bytes of the ID. Only an unreadable `instances/` itself is an error.
pub(crate) fn list(layout: &DataLayout) -> Result<Vec<InstanceEntry>, StorageError> {
    let dir = layout.instances_dir();
    let entries = match fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(source) => return Err(StorageError::io(&dir, source)),
    };
    let mut listed: Vec<InstanceEntry> = entries
        .map(|entry| entry.map_err(|source| StorageError::io(&dir, source)))
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .filter(|entry| !entry.file_name().to_string_lossy().starts_with('.'))
        .map(|entry| list_entry(layout, &entry.path(), entry.file_name()))
        .collect();
    listed.sort_by(|a, b| a.id().as_bytes().cmp(b.id().as_bytes()));
    Ok(listed)
}

fn list_entry(layout: &DataLayout, path: &Path, name: std::ffi::OsString) -> InstanceEntry {
    let broken = |id: String, error: &dyn Localize| InstanceEntry::Broken {
        id,
        error: ErrorView::from(error),
    };
    let Some(name) = name.to_str() else {
        let id = name.to_string_lossy().into_owned();
        return broken(id.clone(), &InstanceError::InvalidId { id });
    };
    let id = match InstanceId::parse(name) {
        Ok(id) => id,
        Err(error) => return broken(name.to_owned(), &error),
    };
    let is_dir = fs::symlink_metadata(path).is_ok_and(|meta| meta.is_dir());
    if !is_dir {
        let source = std::io::Error::new(
            std::io::ErrorKind::NotADirectory,
            "not a folder (a file or a link)",
        );
        return broken(name.to_owned(), &StorageError::io(path, source));
    }
    match read(layout, &id) {
        Ok(instance) => InstanceEntry::Ok {
            id,
            instance: Box::new(instance),
        },
        Err(error) => broken(name.to_owned(), &error),
    }
}

/// Names of every entry in `instances/`, for collision checks.
pub(crate) fn existing_names(layout: &DataLayout) -> Result<Vec<String>, StorageError> {
    let dir = layout.instances_dir();
    match fs::read_dir(&dir) {
        Ok(entries) => entries
            .map(|entry| {
                entry
                    .map(|entry| entry.file_name().to_string_lossy().into_owned())
                    .map_err(|source| StorageError::io(&dir, source))
            })
            .collect(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(source) => Err(StorageError::io(&dir, source)),
    }
}

/// Removes `.creating-*` folders older than an hour, left by a crash.
pub(crate) fn remove_stale_creating_dirs(layout: &DataLayout) {
    let Ok(entries) = fs::read_dir(layout.instances_dir()) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let path: PathBuf = entry.path();
        if name.to_string_lossy().starts_with(storage::CREATING_PREFIX) && storage::is_stale(&path)
        {
            let _ = fs::remove_dir_all(&path);
        }
    }
}

/// Errors of instance operations.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum InstanceError {
    /// The ID is not a valid slug.
    #[error("`{id}` is not a valid instance ID")]
    InvalidId { id: String },
    /// The name is empty or longer than 64 characters.
    #[error("the instance name must have 1 to 64 characters")]
    InvalidName,
    /// An instance with this ID exists already.
    #[error("instance `{id}` exists already")]
    Exists { id: InstanceId },
    /// No instance with this ID.
    #[error("instance `{id}` was not found")]
    NotFound { id: InstanceId },
    /// The game of this instance is running.
    #[error("instance `{id}` is running")]
    Running { id: InstanceId },
    /// Moving the instance to the trash failed; it is still there.
    #[error("could not move instance `{id}` to the trash")]
    TrashFailed {
        id: InstanceId,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },
    /// Invalid settings.
    #[error(transparent)]
    Settings(#[from] SettingsError),
    /// Reading or writing the data folder failed.
    #[error(transparent)]
    Storage(#[from] StorageError),
}

impl Localize for InstanceError {
    fn key(&self) -> &'static str {
        match self {
            Self::InvalidId { .. } => "instance-invalid-id",
            Self::InvalidName => "instance-invalid-name",
            Self::Exists { .. } => "instance-exists",
            Self::NotFound { .. } => "instance-not-found",
            Self::Running { .. } => "instance-running",
            Self::TrashFailed { .. } => "instance-trash-failed",
            Self::Settings(inner) => inner.key(),
            Self::Storage(inner) => inner.key(),
        }
    }

    fn args(&self) -> MessageArgs {
        match self {
            Self::InvalidId { id } => MessageArgs::new().with("id", id.as_str()),
            Self::InvalidName => MessageArgs::new().with("max", MAX_NAME_LEN as u32),
            Self::Exists { id } | Self::NotFound { id } | Self::Running { id } => {
                MessageArgs::new().with("id", id.as_str())
            }
            Self::TrashFailed { id, source } => MessageArgs::new()
                .with("id", id.as_str())
                .with("reason", source.to_string()),
            Self::Settings(inner) => inner.args(),
            Self::Storage(inner) => inner.args(),
        }
    }
}

#[cfg(test)]
pub(crate) fn error_samples() -> Vec<Box<dyn Localize>> {
    let id = InstanceId::parse("survival").unwrap();
    vec![
        Box::new(InstanceError::InvalidId { id: "Bad".into() }),
        Box::new(InstanceError::InvalidName),
        Box::new(InstanceError::Exists { id: id.clone() }),
        Box::new(InstanceError::NotFound { id: id.clone() }),
        Box::new(InstanceError::Running { id: id.clone() }),
        Box::new(InstanceError::TrashFailed {
            id,
            source: "no trash here".into(),
        }),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs_follow_the_rules() {
        let cases = [
            ("Survival 1.21", "survival-1-21"),
            ("Über Café", "ueber-cafe"),
            ("Straße", "strasse"),
            ("CON", "con-1"),
            ("lpt9", "lpt9-1"),
            ("日本語", "instance"),
            ("  --a__b--  ", "a-b"),
            ("ﬁsh", "fish"),
        ];
        for (name, slug) in cases {
            assert_eq!(slugify(name), slug, "{name}");
        }
    }

    #[test]
    fn long_names_are_cut_at_48_without_a_trailing_dash() {
        let name = "a".repeat(47) + " b" + &"c".repeat(20);
        let slug = slugify(&name);
        assert_eq!(slug, "a".repeat(47));
        assert_eq!(slugify(&"x".repeat(60)).len(), MAX_ID_LEN);
    }

    #[test]
    fn collisions_count_up_ignoring_case() {
        assert_eq!(unique_slug("Survival", ["Survival"]), "survival-2");
        assert_eq!(
            unique_slug("Survival", ["survival", "survival-2"]),
            "survival-3"
        );
        assert_eq!(unique_slug("Survival", ["other"]), "survival");
    }

    #[test]
    fn collision_suffix_fits_into_48_characters() {
        let long = "y".repeat(48);
        let slug = unique_slug(&long, [long.as_str()]);
        assert_eq!(slug, "y".repeat(46) + "-2");
        assert_eq!(slug.len(), MAX_ID_LEN);
    }

    #[test]
    fn validate_id_accepts_only_own_slugs() {
        for good in ["survival", "a-b", "x1", "con-1"] {
            assert!(validate_id(good), "{good}");
        }
        for bad in [
            "",
            "-x",
            ".x",
            "A",
            "a--b",
            "a b",
            "con",
            "x-",
            &"z".repeat(49),
        ] {
            assert!(!validate_id(bad), "{bad}");
        }
    }

    #[test]
    fn names_are_trimmed_and_limited() {
        assert_eq!(validate_name("  Hardcore ").unwrap(), "Hardcore");
        assert!(validate_name("   ").is_err());
        assert!(validate_name(&"ä".repeat(64)).is_ok());
        assert!(validate_name(&"ä".repeat(65)).is_err());
    }

    #[test]
    fn icon_files_must_be_plain_names() {
        for good in ["icon.png", ".icon"] {
            assert!(IconFile::try_from(good.to_owned()).is_ok(), "{good}");
        }
        for bad in ["", ".", "..", "../x", "a/b", r"a\b", "C:x"] {
            assert!(IconFile::try_from(bad.to_owned()).is_err(), "{bad}");
        }
    }

    #[test]
    fn instance_json_has_the_spec_shape() {
        let instance = NewInstance::new(" Survival ", "1.21.4")
            .build("2026-10-11T08:00:00Z".parse().unwrap())
            .unwrap();
        assert_eq!(
            serde_json::to_value(&instance).unwrap(),
            serde_json::json!({
                "format_version": 1,
                "name": "Survival",
                "minecraft_version": "1.21.4",
                "loader": null,
                "settings": {},
                "icon": null,
                "created_at": "2026-10-11T08:00:00Z"
            })
        );
    }

    #[test]
    fn loader_and_icon_must_be_present_even_when_null() {
        let mut value = serde_json::json!({
            "format_version": 1,
            "name": "x",
            "minecraft_version": "1.21.4",
            "loader": null,
            "settings": {},
            "icon": { "file": "icon.png" },
            "created_at": "2026-10-11T08:00:00Z"
        });
        assert!(serde_json::from_value::<Instance>(value.clone()).is_ok());
        value.as_object_mut().unwrap().remove("loader");
        assert!(serde_json::from_value::<Instance>(value).is_err());
    }
}
