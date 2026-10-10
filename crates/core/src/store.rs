//! The data folder operations on [`Launcher`]: instances, settings, trust,
//! deletion and the running lock (spec 0002).
//!
//! Each method runs completely inside one `spawn_blocking` call and holds
//! no lock across an `await`. Every change takes the data lock, reads the
//! file fresh, and writes it atomically.

use std::{
    error::Error,
    fmt,
    fs::{self, File, TryLockError},
    path::Path,
};

use crate::{
    Launcher,
    instance::{
        self, Instance, InstanceEntry, InstanceError, InstanceId, NewInstance, normalize,
        unique_slug,
    },
    settings::{self, EffectiveSettings, GlobalSettings, SettingsError, code_overrides_hash},
    storage::{
        self, DataLayout, DataLock, GAME_DIR, INSTANCE_FILE, StorageError, open_lock_file,
        random_hex, write_json_atomic,
    },
};

/// Moves folders to the trash of the system. Replaceable in tests.
pub trait Trash: Send + Sync + fmt::Debug {
    /// Moves `path` to the trash.
    fn trash(&self, path: &Path) -> Result<(), Box<dyn Error + Send + Sync>>;
}

/// The real trash of Windows, macOS or Linux (freedesktop).
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemTrash;

impl Trash for SystemTrash {
    fn trash(&self, path: &Path) -> Result<(), Box<dyn Error + Send + Sync>> {
        trash::delete(path).map_err(Into::into)
    }
}

/// Proof that this process runs the game of an instance. Other processes
/// cannot delete or start the instance until it is dropped.
#[derive(Debug)]
pub struct RunningGuard {
    id: InstanceId,
    _file: File,
}

impl RunningGuard {
    /// The running instance.
    pub fn id(&self) -> &InstanceId {
        &self.id
    }
}

impl Launcher {
    /// Creates an instance. Without `id`, the ID is a slug of the name that
    /// does not clash with an existing one; a given `id` must be valid and
    /// free. Code fields in `new.settings` are trusted automatically.
    pub async fn create_instance(
        &self,
        new: NewInstance,
        id: Option<String>,
    ) -> Result<(InstanceId, Instance), InstanceError> {
        let layout = self.layout();
        let created_at = jiff::Timestamp::now();
        blocking(move || create(&layout, new, id, created_at)).await
    }

    /// Every entry in `instances/`, readable or broken, sorted by ID.
    pub async fn list_instances(&self) -> Result<Vec<InstanceEntry>, StorageError> {
        let layout = self.layout();
        blocking(move || instance::list(&layout)).await
    }

    /// Reads one instance.
    pub async fn instance(&self, id: &str) -> Result<Instance, InstanceError> {
        let layout = self.layout();
        let id = InstanceId::parse(id)?;
        blocking(move || instance::read(&layout, &id)).await
    }

    /// Changes an instance: `change` gets the current file, read fresh under
    /// the data lock, and returns the new one. `created_at` and
    /// `format_version` stay as the core sets them; the ID never changes.
    /// Changed code fields are trusted only if the old ones were; an
    /// untrusted instance stays untrusted until `trust_instance_overrides`.
    pub async fn update_instance<F>(&self, id: &str, change: F) -> Result<Instance, InstanceError>
    where
        F: FnOnce(Instance) -> Result<Instance, InstanceError> + Send + 'static,
    {
        let layout = self.layout();
        let id = InstanceId::parse(id)?;
        blocking(move || update(&layout, &id, change)).await
    }

    /// Moves an instance folder to the trash, or deletes it for good when
    /// `permanent` (never following links). Refused while the game runs.
    /// The shared cache is never touched.
    pub async fn delete_instance(&self, id: &str, permanent: bool) -> Result<(), InstanceError> {
        let layout = self.layout();
        let trash = self.trash();
        let id = InstanceId::parse(id)?;
        blocking(move || delete(&layout, trash.as_ref(), &id, permanent)).await
    }

    /// Takes the running lock of an instance, without waiting.
    pub async fn acquire_running(&self, id: &str) -> Result<RunningGuard, InstanceError> {
        let layout = self.layout();
        let id = InstanceId::parse(id)?;
        blocking(move || {
            instance::read(&layout, &id)?;
            acquire_running(&layout, id)
        })
        .await
    }

    /// The global settings; built in defaults when `settings.json` is missing.
    pub async fn global_settings(&self) -> Result<GlobalSettings, SettingsError> {
        let layout = self.layout();
        blocking(move || Ok(load_global(&layout)?.value)).await
    }

    /// Changes the global settings. Checks only the global layer, never the
    /// instances.
    pub async fn update_global_settings<F>(
        &self,
        change: F,
    ) -> Result<GlobalSettings, SettingsError>
    where
        F: FnOnce(GlobalSettings) -> Result<GlobalSettings, SettingsError> + Send + 'static,
    {
        let layout = self.layout();
        blocking(move || {
            let _lock = DataLock::acquire(&layout)?;
            let loaded = load_global(&layout)?;
            let changed = change(loaded.value)?;
            settings::validate_layer(&changed.game)?;
            let changed = GlobalSettings {
                format_version: settings::SETTINGS_FORMAT,
                ..changed
            };
            storage::save(&layout.settings_file(), &changed, loaded.migrated_from)?;
            Ok(changed)
        })
        .await
    }

    /// The settings the game of an instance starts with.
    pub async fn effective_settings(&self, id: &str) -> Result<EffectiveSettings, InstanceError> {
        let layout = self.layout();
        let id = InstanceId::parse(id)?;
        blocking(move || {
            let instance = instance::read(&layout, &id)?;
            let global = load_global(&layout)?.value;
            let trusted = settings::code_trusted(&global, &id, &instance.settings);
            Ok(settings::resolve(
                &global.game,
                &instance.settings,
                trusted,
            )?)
        })
        .await
    }

    /// Trusts the current code fields of an instance. Meant for surfaces
    /// that showed those fields to the user first.
    pub async fn trust_instance_overrides(&self, id: &str) -> Result<(), InstanceError> {
        let layout = self.layout();
        let id = InstanceId::parse(id)?;
        blocking(move || {
            let _lock = DataLock::acquire(&layout)?;
            let instance = instance::read(&layout, &id)?;
            set_trust(&layout, &id, code_overrides_hash(&instance.settings))?;
            Ok(())
        })
        .await
    }
}

/// Runs blocking file work off the async threads. A panic inside is passed on.
async fn blocking<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> T {
    match tokio::task::spawn_blocking(work).await {
        Ok(value) => value,
        Err(error) => std::panic::resume_unwind(error.into_panic()),
    }
}

fn create(
    layout: &DataLayout,
    new: NewInstance,
    id: Option<String>,
    created_at: jiff::Timestamp,
) -> Result<(InstanceId, Instance), InstanceError> {
    let instance = new.build(created_at)?;
    let requested = id.as_deref().map(InstanceId::parse).transpose()?;

    let _lock = DataLock::acquire(layout)?;
    let instances = layout.instances_dir();
    fs::create_dir_all(&instances).map_err(|source| StorageError::io(&instances, source))?;
    instance::remove_stale_creating_dirs(layout);
    let existing = instance::existing_names(layout)?;
    let id = match requested {
        Some(id) => {
            if existing
                .iter()
                .any(|name| name.eq_ignore_ascii_case(id.as_str()))
            {
                return Err(InstanceError::Exists { id });
            }
            id
        }
        None => InstanceId::parse(&unique_slug(
            &instance.name,
            existing.iter().map(String::as_str),
        ))?,
    };

    let staging = layout.creating_dir(id.as_str(), &random_hex(4));
    let result = fill_staging(&staging, &instance)
        .and_then(|()| {
            Ok(set_trust(
                layout,
                &id,
                code_overrides_hash(&instance.settings),
            )?)
        })
        .and_then(|()| {
            let target = layout.instance_dir(id.as_str());
            fs::rename(&staging, &target).map_err(|source| StorageError::io(&target, source).into())
        });
    if result.is_err() {
        let _ = fs::remove_dir_all(&staging);
    }
    result.map(|()| (id, instance))
}

fn fill_staging(staging: &Path, instance: &Instance) -> Result<(), InstanceError> {
    fs::create_dir(staging).map_err(|source| StorageError::io(staging, source))?;
    write_json_atomic(&staging.join(INSTANCE_FILE), instance)?;
    let game = staging.join(GAME_DIR);
    fs::create_dir(&game).map_err(|source| StorageError::io(&game, source))?;
    Ok(())
}

fn update<F>(layout: &DataLayout, id: &InstanceId, change: F) -> Result<Instance, InstanceError>
where
    F: FnOnce(Instance) -> Result<Instance, InstanceError>,
{
    let _lock = DataLock::acquire(layout)?;
    let loaded = instance::read_loaded(layout, id)?;
    let created_at = loaded.value.created_at;
    let old_hash = code_overrides_hash(&loaded.value.settings);
    let was_trusted =
        settings::code_trusted(&load_global(layout)?.value, id, &loaded.value.settings);
    let changed = change(loaded.value)?;
    let changed = normalize(Instance {
        format_version: instance::INSTANCE_FORMAT,
        created_at,
        ..changed
    })?;
    let new_hash = code_overrides_hash(&changed.settings);
    // Trust first: a crash in between leaves a harmless trust entry for
    // values that do not exist yet. Only trusted values pass trust on;
    // otherwise this edit would bless code fields changed from outside.
    if was_trusted && new_hash != old_hash {
        set_trust(layout, id, new_hash)?;
    }
    storage::save(
        &layout.instance_file(id.as_str()),
        &changed,
        loaded.migrated_from,
    )?;
    Ok(changed)
}

fn delete(
    layout: &DataLayout,
    trash: &dyn Trash,
    id: &InstanceId,
    permanent: bool,
) -> Result<(), InstanceError> {
    let _lock = DataLock::acquire(layout)?;
    let dir = layout.instance_dir(id.as_str());
    if !fs::symlink_metadata(&dir).is_ok_and(|meta| meta.is_dir()) {
        return Err(InstanceError::NotFound { id: id.clone() });
    }
    // Held while deleting, so the game cannot start halfway through.
    let _running = acquire_running(layout, id.clone())?;
    if permanent {
        // `remove_dir_all` never follows links, it removes the link itself.
        fs::remove_dir_all(&dir).map_err(|source| StorageError::io(&dir, source))?;
    } else if let Err(source) = trash.trash(&dir) {
        if fs::symlink_metadata(&dir).is_ok() {
            return Err(InstanceError::TrashFailed {
                id: id.clone(),
                source,
            });
        }
        tracing::warn!(id = %id, error = %source, "trash reported an error, but the folder is gone");
    }
    set_trust(layout, id, None)?;
    Ok(())
}

fn acquire_running(layout: &DataLayout, id: InstanceId) -> Result<RunningGuard, InstanceError> {
    let dir = layout.running_locks_dir();
    fs::create_dir_all(&dir).map_err(|source| StorageError::io(&dir, source))?;
    let path = layout.running_lock(id.as_str());
    let file = open_lock_file(&path)?;
    match file.try_lock() {
        Ok(()) => Ok(RunningGuard { id, _file: file }),
        Err(TryLockError::WouldBlock) => Err(InstanceError::Running { id }),
        Err(TryLockError::Error(source)) => Err(StorageError::io(&path, source).into()),
    }
}

/// `settings.json`, or the defaults when it does not exist.
fn load_global(layout: &DataLayout) -> Result<storage::Loaded<GlobalSettings>, StorageError> {
    Ok(
        storage::load::<GlobalSettings>(&layout.settings_file())?.unwrap_or(storage::Loaded {
            value: GlobalSettings::default(),
            migrated_from: None,
        }),
    )
}

/// Sets or removes the trust entry of `id`. Call with the data lock held.
/// Writes `settings.json` only when something changes.
fn set_trust(
    layout: &DataLayout,
    id: &InstanceId,
    hash: Option<String>,
) -> Result<(), StorageError> {
    let loaded = load_global(layout)?;
    let mut global = loaded.value;
    if global.set_trusted(id, hash) {
        storage::save(&layout.settings_file(), &global, loaded.migrated_from)?;
    }
    Ok(())
}
