//! Where Exodus keeps its files, and how it reads and writes them safely.
//!
//! Every JSON file carries a version, keeps fields it does not know, is
//! written through a temporary file plus `rename`, and is only changed while
//! holding the data lock `locks/data.lock` (spec 0002).

use std::{
    ffi::OsStr,
    fs::{self, File, TryLockError},
    io::{self, Write},
    path::{Path, PathBuf},
    thread,
    time::{Duration, SystemTime},
};

use ring::rand::{SecureRandom, SystemRandom};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Map, Value};

use crate::{Localize, MessageArgs};

/// Current version of the folder layout, stored in `exodus.json`.
pub const LAYOUT_VERSION: u32 = 1;

/// How long a change waits for the data lock before giving up.
pub const LOCK_TIMEOUT: Duration = Duration::from_secs(5);

const LOCK_POLL: Duration = Duration::from_millis(50);

/// Leftover temporary files and folders older than this get cleaned up.
pub(crate) const STALE_AFTER: Duration = Duration::from_secs(60 * 60);

const RENAME_ATTEMPTS: u32 = 5;
const RENAME_RETRY_DELAY: Duration = Duration::from_millis(100);

/// Every path inside a data folder. Pure: builds paths, touches nothing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DataLayout {
    root: PathBuf,
}

impl DataLayout {
    /// The layout of the data folder `root`.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// The data folder itself.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// `exodus.json`: the layout version.
    pub fn layout_file(&self) -> PathBuf {
        self.root.join("exodus.json")
    }

    /// `settings.json`: global settings.
    pub fn settings_file(&self) -> PathBuf {
        self.root.join("settings.json")
    }

    /// `accounts.json`: account metadata.
    pub fn accounts_file(&self) -> PathBuf {
        self.root.join("accounts.json")
    }

    /// `accounts.secrets`: the encrypted tokens.
    pub fn secrets_file(&self) -> PathBuf {
        self.root.join("accounts.secrets")
    }

    /// `logs/`: diagnostic logs.
    pub fn logs_dir(&self) -> PathBuf {
        crate::config::log_dir(&self.root)
    }

    /// `locks/`.
    pub fn locks_dir(&self) -> PathBuf {
        self.root.join("locks")
    }

    /// `locks/data.lock`: held for every change to a JSON file.
    pub fn data_lock(&self) -> PathBuf {
        self.locks_dir().join("data.lock")
    }

    /// `locks/instances/`.
    pub fn running_locks_dir(&self) -> PathBuf {
        self.locks_dir().join("instances")
    }

    /// `locks/instances/<id>.running.lock`: held while the game of an instance runs.
    pub fn running_lock(&self, id: &str) -> PathBuf {
        self.running_locks_dir().join(format!("{id}.running.lock"))
    }

    /// `versions/`.
    pub fn versions_dir(&self) -> PathBuf {
        self.root.join("versions")
    }

    /// `versions/<v>/<v>.json`.
    pub fn version_json(&self, version: &str) -> PathBuf {
        self.versions_dir()
            .join(version)
            .join(format!("{version}.json"))
    }

    /// `versions/<v>/<v>.jar`.
    pub fn version_jar(&self, version: &str) -> PathBuf {
        self.versions_dir()
            .join(version)
            .join(format!("{version}.jar"))
    }

    /// `libraries/`.
    pub fn libraries_dir(&self) -> PathBuf {
        self.root.join("libraries")
    }

    /// `libraries/<path>`, with `path` from `downloads.artifact.path` (`/` separated).
    pub fn library(&self, path: &str) -> PathBuf {
        path.split('/')
            .fold(self.libraries_dir(), |dir, part| dir.join(part))
    }

    /// `assets/`.
    pub fn assets_dir(&self) -> PathBuf {
        self.root.join("assets")
    }

    /// `assets/indexes/<id>.json`.
    pub fn asset_index(&self, id: &str) -> PathBuf {
        self.assets_dir().join("indexes").join(format!("{id}.json"))
    }

    /// `assets/objects/<first 2 hex>/<sha1>`.
    pub fn asset_object(&self, sha1: &str) -> PathBuf {
        let prefix = sha1.get(..2).unwrap_or(sha1);
        self.assets_dir().join("objects").join(prefix).join(sha1)
    }

    /// `runtimes/`.
    pub fn runtimes_dir(&self) -> PathBuf {
        self.root.join("runtimes")
    }

    /// `instances/`.
    pub fn instances_dir(&self) -> PathBuf {
        self.root.join("instances")
    }

    /// `instances/<id>/`.
    pub fn instance_dir(&self, id: &str) -> PathBuf {
        self.instances_dir().join(id)
    }

    /// `instances/<id>/instance.json`.
    pub fn instance_file(&self, id: &str) -> PathBuf {
        self.instance_dir(id).join(INSTANCE_FILE)
    }

    /// `instances/<id>/content.json` (reserved, not created in phase 1).
    pub fn instance_content_file(&self, id: &str) -> PathBuf {
        self.instance_dir(id).join("content.json")
    }

    /// `instances/<id>/minecraft/`: the game directory (`--gameDir`).
    pub fn instance_game_dir(&self, id: &str) -> PathBuf {
        self.instance_dir(id).join(GAME_DIR)
    }

    /// `instances/<id>/<file>`: the icon file of an instance.
    pub fn instance_icon(&self, id: &str, file: &str) -> PathBuf {
        self.instance_dir(id).join(file)
    }

    /// `instances/.creating-<id>-<suffix>/`: an instance while it is being created.
    pub fn creating_dir(&self, id: &str, suffix: &str) -> PathBuf {
        self.instances_dir()
            .join(format!("{CREATING_PREFIX}{id}-{suffix}"))
    }
}

pub(crate) const INSTANCE_FILE: &str = "instance.json";
pub(crate) const GAME_DIR: &str = "minecraft";
pub(crate) const CREATING_PREFIX: &str = ".creating-";

/// `exodus.json`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LayoutFile {
    /// Version of the folder layout.
    pub layout_version: u32,
    /// Fields this version does not know, kept as they are.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// One step of a migration chain: turns a file of version `n` into version
/// `n + 1`, where `n` is the step's index in the chain.
pub type MigrationStep = fn(Value) -> Result<Value, String>;

/// A JSON file type with a version field.
pub(crate) trait Versioned: Serialize + DeserializeOwned {
    /// Name of the version field.
    const VERSION_FIELD: &'static str = "format_version";
    /// The version this build reads and writes.
    const CURRENT: u32;
    /// Steps from older versions; index `n` lifts version `n` to `n + 1`.
    const MIGRATIONS: &'static [MigrationStep] = &[];
}

impl Versioned for LayoutFile {
    const VERSION_FIELD: &'static str = "layout_version";
    const CURRENT: u32 = LAYOUT_VERSION;
}

/// Lifts `value` from version `from` to `target` through `chain`, setting
/// `version_field` after each step. Pure.
pub fn migrate(
    value: Value,
    version_field: &str,
    from: u32,
    target: u32,
    chain: &[MigrationStep],
) -> Result<Value, MigrateError> {
    if from > target {
        return Err(MigrateError::TooNew {
            found: from,
            supported: target,
        });
    }
    (from..target).try_fold(value, |value, version| {
        let step = usize::try_from(version)
            .ok()
            .and_then(|index| chain.get(index))
            .ok_or(MigrateError::Missing { from: version })?;
        let mut value = step(value).map_err(|reason| MigrateError::Step {
            from: version,
            reason,
        })?;
        let object = value.as_object_mut().ok_or(MigrateError::Step {
            from: version,
            reason: "the step did not return an object".into(),
        })?;
        object.insert(version_field.to_owned(), Value::from(version + 1));
        Ok(value)
    })
}

/// Why [`migrate`] failed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum MigrateError {
    /// The file is newer than this build.
    #[error("version {found} is newer than the supported version {supported}")]
    TooNew { found: u32, supported: u32 },
    /// No step exists for this version.
    #[error("no migration step from version {from}")]
    Missing { from: u32 },
    /// A step failed.
    #[error("migration from version {from} failed: {reason}")]
    Step { from: u32, reason: String },
}

/// A file as loaded: its value and, if it was migrated in memory, the
/// version it had on disk.
#[derive(Debug)]
pub(crate) struct Loaded<T> {
    pub value: T,
    pub migrated_from: Option<u32>,
}

/// Reads and parses a versioned file. `Ok(None)` when it does not exist.
pub(crate) fn load<T: Versioned>(path: &Path) -> Result<Option<Loaded<T>>, StorageError> {
    load_with(path, T::VERSION_FIELD, T::CURRENT, T::MIGRATIONS)
}

fn load_with<T: DeserializeOwned>(
    path: &Path,
    version_field: &str,
    current: u32,
    chain: &[MigrationStep],
) -> Result<Option<Loaded<T>>, StorageError> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(source) => return Err(StorageError::io(path, source)),
    };
    let json_error = |source| StorageError::Json {
        path: path.to_owned(),
        source,
    };
    let value: Value = serde_json::from_slice(&bytes).map_err(json_error)?;
    let found = value
        .get(version_field)
        .and_then(Value::as_u64)
        .and_then(|version| u32::try_from(version).ok());
    let Some(found) = found else {
        // Let serde name the missing or malformed field.
        return Err(json_error(
            serde_json::from_value::<VersionOnly>(value)
                .err()
                .unwrap_or_else(|| serde::de::Error::missing_field("format_version")),
        ));
    };
    let value =
        migrate(value, version_field, found, current, chain).map_err(|error| match error {
            MigrateError::TooNew { found, supported } => StorageError::TooNew {
                path: path.to_owned(),
                found,
                supported,
            },
            other => StorageError::Migration {
                path: path.to_owned(),
                source: other,
            },
        })?;
    let value = serde_json::from_value(value).map_err(json_error)?;
    Ok(Some(Loaded {
        value,
        migrated_from: (found < current).then_some(found),
    }))
}

#[derive(Deserialize)]
#[expect(dead_code, reason = "only parsed to produce a precise error")]
struct VersionOnly {
    format_version: u32,
}

/// Writes `value` as JSON to `path` through a temporary file in the same
/// folder and `rename`. On failure the temporary file is removed and the old
/// file stays as it was.
pub(crate) fn write_json_atomic(path: &Path, value: &impl Serialize) -> Result<(), StorageError> {
    let mut bytes = serde_json::to_vec_pretty(value).map_err(|source| StorageError::Json {
        path: path.to_owned(),
        source,
    })?;
    bytes.push(b'\n');
    write_atomic(path, &bytes)
}

/// Writes `value` like [`write_json_atomic`]. If it was migrated from an
/// older version, first keeps the old file as `<file>.v<old>.bak`, unless
/// such a backup already exists.
pub(crate) fn save(
    path: &Path,
    value: &impl Serialize,
    migrated_from: Option<u32>,
) -> Result<(), StorageError> {
    if let Some(old) = migrated_from {
        let backup = backup_path(path, old);
        if !backup.exists() {
            let original = fs::read(path).map_err(|source| StorageError::io(path, source))?;
            write_atomic(&backup, &original)?;
        }
    }
    write_json_atomic(path, value)
}

/// `<file>.v<version>.bak`.
pub(crate) fn backup_path(path: &Path, version: u32) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(format!(".v{version}.bak"));
    path.with_file_name(name)
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), StorageError> {
    let dir = path.parent().unwrap_or(Path::new("."));
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    remove_stale_temp_files(dir);
    let temp = dir.join(format!(
        ".{name}{TEMP_MARKER}{}-{}",
        std::process::id(),
        random_hex(4)
    ));
    let result = write_and_sync(&temp, bytes)
        .and_then(|()| rename_with_retry(&temp, path))
        .map_err(|source| StorageError::io(path, source));
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result?;
    sync_dir(dir);
    Ok(())
}

const TEMP_MARKER: &str = ".tmp-";

fn write_and_sync(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = File::options().write(true).create_new(true).open(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

/// Windows refuses a rename while a virus scanner or editor has the target
/// open; that usually passes within a moment.
fn rename_with_retry(from: &Path, to: &Path) -> io::Result<()> {
    let mut attempt = 1;
    loop {
        match fs::rename(from, to) {
            Err(error) if attempt < RENAME_ATTEMPTS && is_transient_rename_error(&error) => {
                attempt += 1;
                thread::sleep(RENAME_RETRY_DELAY);
            }
            result => return result,
        }
    }
}

#[cfg(windows)]
fn is_transient_rename_error(error: &io::Error) -> bool {
    const ERROR_ACCESS_DENIED: i32 = 5;
    const ERROR_SHARING_VIOLATION: i32 = 32;
    matches!(
        error.raw_os_error(),
        Some(ERROR_ACCESS_DENIED | ERROR_SHARING_VIOLATION)
    )
}

#[cfg(not(windows))]
fn is_transient_rename_error(_: &io::Error) -> bool {
    false
}

/// Makes the rename itself durable. Windows has no way to sync a folder.
#[cfg(unix)]
fn sync_dir(dir: &Path) {
    if let Err(error) = File::open(dir).and_then(|dir| dir.sync_all()) {
        tracing::debug!(dir = %dir.display(), %error, "could not sync folder");
    }
}

#[cfg(not(unix))]
fn sync_dir(_: &Path) {}

/// Whether `name` is a temporary file of [`write_json_atomic`].
pub(crate) fn is_temp_file(name: &OsStr) -> bool {
    let name = name.to_string_lossy();
    name.starts_with('.') && name.contains(TEMP_MARKER)
}

/// Removes temporary files in `dir` left behind by a crash, once they are
/// older than an hour. Best effort.
fn remove_stale_temp_files(dir: &Path) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        if is_temp_file(&entry.file_name()) && is_stale(&entry.path()) {
            let _ = fs::remove_file(entry.path());
        }
    }
}

/// True when `path` was last modified more than [`STALE_AFTER`] ago.
pub(crate) fn is_stale(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|modified| SystemTime::now().duration_since(modified).ok())
        .is_some_and(|age| age > STALE_AFTER)
}

/// `bytes` random bytes as lowercase hex.
pub(crate) fn random_hex(bytes: usize) -> String {
    let mut buffer = vec![0; bytes];
    SystemRandom::new()
        .fill(&mut buffer)
        .expect("the system random source works");
    hex(&buffer)
}

/// Lowercase hex of `bytes`.
pub(crate) fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::new(), |mut text, byte| {
        let _ = write!(text, "{byte:02x}");
        text
    })
}

/// The exclusive data lock, held for one read, change and write cycle.
/// Released when dropped.
#[derive(Debug)]
pub(crate) struct DataLock {
    _file: File,
}

impl DataLock {
    /// Takes the data lock of `layout`, waiting up to [`LOCK_TIMEOUT`], then
    /// checks `exodus.json` (creating it for a fresh folder). A newer layout
    /// refuses with [`StorageError::LayoutTooNew`].
    pub fn acquire(layout: &DataLayout) -> Result<Self, StorageError> {
        Self::acquire_within(layout, LOCK_TIMEOUT)
    }

    pub fn acquire_within(layout: &DataLayout, timeout: Duration) -> Result<Self, StorageError> {
        let locks = layout.locks_dir();
        fs::create_dir_all(&locks).map_err(|source| StorageError::io(&locks, source))?;
        let path = layout.data_lock();
        let file = open_lock_file(&path)?;
        let started = std::time::Instant::now();
        loop {
            match file.try_lock() {
                Ok(()) => break,
                Err(TryLockError::WouldBlock) if started.elapsed() < timeout => {
                    thread::sleep(LOCK_POLL);
                }
                Err(TryLockError::WouldBlock) => return Err(StorageError::Busy { path }),
                Err(TryLockError::Error(source)) => return Err(StorageError::io(&path, source)),
            }
        }
        let lock = Self { _file: file };
        check_layout(layout)?;
        Ok(lock)
    }
}

/// Opens (creating if needed) a lock file. Lock files are only locked,
/// never read, written or deleted.
pub(crate) fn open_lock_file(path: &Path) -> Result<File, StorageError> {
    File::options()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)
        .map_err(|source| StorageError::io(path, source))
}

fn check_layout(layout: &DataLayout) -> Result<(), StorageError> {
    let path = layout.layout_file();
    match load::<LayoutFile>(&path) {
        Ok(Some(_)) => Ok(()),
        Ok(None) => write_json_atomic(
            &path,
            &LayoutFile {
                layout_version: LAYOUT_VERSION,
                extra: Map::new(),
            },
        ),
        Err(StorageError::TooNew {
            path,
            found,
            supported,
        }) => Err(StorageError::LayoutTooNew {
            path,
            found,
            supported,
        }),
        Err(other) => Err(other),
    }
}

/// Errors while reading or writing the data folder.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum StorageError {
    /// Reading or writing a file failed.
    #[error("could not read or write {}", path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    /// A file is not valid JSON or misses a required field.
    #[error("{} is not valid", path.display())]
    Json {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    /// A file was written by a newer Exodus.
    #[error("{} has version {found}, this build supports up to {supported}", path.display())]
    TooNew {
        path: PathBuf,
        found: u32,
        supported: u32,
    },
    /// The data folder layout is newer than this build; it may only read.
    #[error("the data folder layout {found} is newer than the supported {supported}")]
    LayoutTooNew {
        path: PathBuf,
        found: u32,
        supported: u32,
    },
    /// Another Exodus process held the data lock for too long.
    #[error("the data lock {} stayed busy", path.display())]
    Busy { path: PathBuf },
    /// An older file could not be migrated.
    #[error("could not migrate {}", path.display())]
    Migration {
        path: PathBuf,
        #[source]
        source: MigrateError,
    },
}

impl StorageError {
    pub(crate) fn io(path: &Path, source: io::Error) -> Self {
        Self::Io {
            path: path.to_owned(),
            source,
        }
    }
}

impl Localize for StorageError {
    fn key(&self) -> &'static str {
        match self {
            Self::Io { .. } => "storage-io",
            Self::Json { .. } => "storage-json",
            Self::TooNew { .. } => "storage-too-new",
            Self::LayoutTooNew { .. } => "storage-layout-too-new",
            Self::Busy { .. } => "storage-busy",
            Self::Migration { .. } => "storage-migration",
        }
    }

    fn args(&self) -> MessageArgs {
        match self {
            Self::Io { path, source } => MessageArgs::new()
                .with("path", path.as_path())
                .with("reason", source.to_string()),
            Self::Json { path, source } => MessageArgs::new()
                .with("path", path.as_path())
                .with("reason", source.to_string()),
            Self::TooNew {
                path,
                found,
                supported,
            }
            | Self::LayoutTooNew {
                path,
                found,
                supported,
            } => MessageArgs::new()
                .with("path", path.as_path())
                .with("found", *found)
                .with("supported", *supported),
            Self::Busy { path } => MessageArgs::new().with("path", path.as_path()),
            Self::Migration { path, source } => MessageArgs::new()
                .with("path", path.as_path())
                .with("reason", source.to_string()),
        }
    }
}

#[cfg(test)]
pub(crate) fn error_samples() -> Vec<Box<dyn Localize>> {
    let path = PathBuf::from("instance.json");
    vec![
        Box::new(StorageError::io(&path, io::Error::other("disk"))),
        Box::new(StorageError::Json {
            path: path.clone(),
            source: serde_json::from_str::<Value>("{").unwrap_err(),
        }),
        Box::new(StorageError::TooNew {
            path: path.clone(),
            found: 9,
            supported: 1,
        }),
        Box::new(StorageError::LayoutTooNew {
            path: path.clone(),
            found: 2,
            supported: 1,
        }),
        Box::new(StorageError::Busy { path: path.clone() }),
        Box::new(StorageError::Migration {
            path,
            source: MigrateError::Missing { from: 0 },
        }),
    ]
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct Demo {
        format_version: u32,
        count: u32,
        #[serde(flatten)]
        extra: Map<String, Value>,
    }

    fn v0_to_v1(mut value: Value) -> Result<Value, String> {
        let old = value
            .as_object_mut()
            .and_then(|object| object.remove("counter"))
            .ok_or("no counter")?;
        value["count"] = old;
        Ok(value)
    }

    const CHAIN: &[MigrationStep] = &[v0_to_v1];

    fn load_demo(path: &Path) -> Result<Option<Loaded<Demo>>, StorageError> {
        load_with(path, "format_version", 1, CHAIN)
    }

    fn temp() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    #[test]
    fn layout_paths_match_the_spec() {
        let layout = DataLayout::new("d");
        let d = Path::new("d");
        assert_eq!(layout.layout_file(), d.join("exodus.json"));
        assert_eq!(layout.data_lock(), d.join("locks").join("data.lock"));
        assert_eq!(
            layout.running_lock("a"),
            d.join("locks").join("instances").join("a.running.lock")
        );
        assert_eq!(
            layout.version_jar("1.21.4"),
            d.join("versions").join("1.21.4").join("1.21.4.jar")
        );
        assert_eq!(
            layout.library("com/mojang/x/1/x-1.jar"),
            d.join("libraries/com/mojang/x/1/x-1.jar")
                .components()
                .collect::<PathBuf>()
        );
        assert_eq!(
            layout.asset_object("abcdef"),
            d.join("assets").join("objects").join("ab").join("abcdef")
        );
        assert_eq!(
            layout.asset_index("17"),
            d.join("assets").join("indexes").join("17.json")
        );
        assert_eq!(
            layout.instance_game_dir("a"),
            d.join("instances").join("a").join("minecraft")
        );
        assert_eq!(
            layout.creating_dir("a", "ff"),
            d.join("instances").join(".creating-a-ff")
        );
        assert_eq!(layout.logs_dir(), d.join("logs"));
    }

    #[test]
    fn migrate_runs_each_step_and_sets_the_version() {
        let value = json!({ "format_version": 0, "counter": 3 });
        let migrated = migrate(value, "format_version", 0, 1, CHAIN).unwrap();
        assert_eq!(migrated, json!({ "format_version": 1, "count": 3 }));
    }

    #[test]
    fn migrate_rejects_newer_and_missing_steps() {
        let value = json!({ "format_version": 5 });
        assert_eq!(
            migrate(value.clone(), "format_version", 5, 1, CHAIN),
            Err(MigrateError::TooNew {
                found: 5,
                supported: 1
            })
        );
        let old = json!({ "format_version": 0, "counter": 1 });
        assert_eq!(
            migrate(old, "format_version", 0, 2, CHAIN).unwrap_err(),
            MigrateError::Missing { from: 1 }
        );
    }

    #[test]
    fn a_newer_file_is_refused_and_left_alone() {
        let dir = temp();
        let path = dir.path().join("demo.json");
        let bytes = br#"{ "format_version": 99, "count": 1 }"#;
        fs::write(&path, bytes).unwrap();
        let error = load_demo(&path).unwrap_err();
        assert!(matches!(
            error,
            StorageError::TooNew {
                found: 99,
                supported: 1,
                ..
            }
        ));
        assert_eq!(fs::read(&path).unwrap(), bytes);
    }

    #[test]
    fn an_older_file_is_migrated_and_backed_up_once() {
        let dir = temp();
        let path = dir.path().join("demo.json");
        let original = br#"{ "format_version": 0, "counter": 7 }"#;
        fs::write(&path, original).unwrap();

        let loaded = load_demo(&path).unwrap().unwrap();
        assert_eq!(loaded.value.count, 7);
        assert_eq!(loaded.migrated_from, Some(0));
        let backup = backup_path(&path, 0);
        assert!(!backup.exists(), "loading alone writes nothing");

        let write = |demo: &Demo, from| save(&path, demo, from).unwrap();
        write(&loaded.value, loaded.migrated_from);
        assert_eq!(fs::read(&backup).unwrap(), original);
        assert_eq!(load_demo(&path).unwrap().unwrap().migrated_from, None);

        // A second migration never overwrites the existing backup.
        fs::write(&path, br#"{ "format_version": 0, "counter": 8 }"#).unwrap();
        let again = load_demo(&path).unwrap().unwrap();
        write(&again.value, again.migrated_from);
        assert_eq!(fs::read(&backup).unwrap(), original);
    }

    #[test]
    fn unknown_fields_survive_a_round_trip() {
        let dir = temp();
        let path = dir.path().join("demo.json");
        fs::write(
            &path,
            br#"{ "format_version": 1, "count": 1, "future": { "a": [1, 2] } }"#,
        )
        .unwrap();
        let mut demo = load_demo(&path).unwrap().unwrap().value;
        demo.count = 2;
        write_json_atomic(&path, &demo).unwrap();
        let raw: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(
            raw,
            json!({ "format_version": 1, "count": 2, "future": { "a": [1, 2] } })
        );
    }

    #[test]
    fn missing_file_loads_as_none_and_bad_json_is_reported() {
        let dir = temp();
        let path = dir.path().join("demo.json");
        assert!(load_demo(&path).unwrap().is_none());
        fs::write(&path, b"{ nope").unwrap();
        assert!(matches!(load_demo(&path), Err(StorageError::Json { .. })));
        fs::write(&path, br#"{ "count": 1 }"#).unwrap();
        let error = load_demo(&path).unwrap_err();
        assert!(error.to_string().contains("not valid"), "{error}");
    }

    #[test]
    fn writing_leaves_no_temporary_file_and_removes_stale_ones() {
        let dir = temp();
        let path = dir.path().join("demo.json");
        let stale = dir.path().join(".demo.json.tmp-1-deadbeef");
        let fresh = dir.path().join(".demo.json.tmp-2-cafebabe");
        fs::write(&stale, b"half").unwrap();
        fs::write(&fresh, b"half").unwrap();
        let old = SystemTime::now() - STALE_AFTER - Duration::from_secs(60);
        File::options()
            .write(true)
            .open(&stale)
            .unwrap()
            .set_modified(old)
            .unwrap();

        write_json_atomic(&path, &json!({ "format_version": 1, "count": 1 })).unwrap();

        let mut names: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        names.sort();
        assert_eq!(
            names,
            [fresh.file_name().unwrap(), path.file_name().unwrap()]
        );
        assert_eq!(load_demo(&path).unwrap().unwrap().value.count, 1);
    }

    #[test]
    fn a_failed_write_keeps_the_old_file_and_no_temporary_file() {
        let dir = temp();
        let path = dir.path().join("demo.json");
        fs::write(&path, br#"{ "format_version": 1, "count": 1 }"#).unwrap();
        // A folder in the target's place makes the rename fail.
        let blocked = dir.path().join("blocked");
        fs::create_dir(&blocked).unwrap();
        fs::write(blocked.join("child"), b"x").unwrap();
        assert!(write_json_atomic(&blocked, &json!({})).is_err());
        let names: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(names.len(), 2, "{names:?}");
        assert_eq!(load_demo(&path).unwrap().unwrap().value.count, 1);
    }

    #[test]
    fn the_lock_creates_the_layout_file_and_refuses_a_newer_layout() {
        let dir = temp();
        let layout = DataLayout::new(dir.path());
        drop(DataLock::acquire(&layout).unwrap());
        let raw: Value = serde_json::from_slice(&fs::read(layout.layout_file()).unwrap()).unwrap();
        assert_eq!(raw, json!({ "layout_version": 1 }));

        fs::write(layout.layout_file(), br#"{ "layout_version": 2 }"#).unwrap();
        assert!(matches!(
            DataLock::acquire(&layout),
            Err(StorageError::LayoutTooNew { found: 2, .. })
        ));
    }

    #[test]
    fn a_held_lock_makes_the_next_one_busy() {
        let dir = temp();
        let layout = DataLayout::new(dir.path());
        let held = DataLock::acquire(&layout).unwrap();
        let busy = DataLock::acquire_within(&layout, Duration::from_millis(120));
        assert!(matches!(busy, Err(StorageError::Busy { .. })));
        drop(held);
        assert!(DataLock::acquire(&layout).is_ok());
    }

    #[test]
    fn hex_is_lowercase_and_random_hex_has_the_right_length() {
        assert_eq!(hex(&[0, 15, 255]), "000fff");
        assert_eq!(random_hex(4).len(), 8);
    }
}
