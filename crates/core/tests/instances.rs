//! The data folder as a surface uses it: instances, settings, trust, the
//! locks and deletion, against a real temporary folder (spec 0002).

use std::{
    error::Error,
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, Mutex},
};

use exodus_core::{
    InstanceEntry, InstanceError, Launcher, LauncherConfig, Localize, NewInstance, StorageError,
    Trash, settings::GameSettings,
};
use serde_json::{Value, json};

/// A trash that records what it was given, or fails on purpose.
#[derive(Debug, Default)]
struct FakeTrash {
    fail: bool,
    trashed: Mutex<Vec<PathBuf>>,
}

impl Trash for FakeTrash {
    fn trash(&self, path: &Path) -> Result<(), Box<dyn Error + Send + Sync>> {
        if self.fail {
            return Err("no trash on this drive".into());
        }
        // Like a real trash: the folder leaves its place.
        let moved = path.with_file_name(".trashed");
        fs::rename(path, &moved)?;
        self.trashed.lock().unwrap().push(path.to_owned());
        Ok(())
    }
}

struct Setup {
    dir: tempfile::TempDir,
    launcher: Launcher,
}

fn setup() -> Setup {
    setup_with(FakeTrash::default())
}

fn setup_with(trash: FakeTrash) -> Setup {
    let dir = tempfile::tempdir().unwrap();
    let launcher = Launcher::with_trash(LauncherConfig::new(dir.path()), Arc::new(trash)).unwrap();
    Setup { dir, launcher }
}

fn read_json(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}

fn game(value: Value) -> GameSettings {
    serde_json::from_value(value).unwrap()
}

// covers: AC-3, AC-4, AC-5, AC-6, AC-7, AC-12
#[tokio::test]
async fn create_rename_list_resolve_and_delete() {
    let Setup { dir, launcher } = setup();
    let layout = launcher.layout();

    let (id, created) = launcher
        .create_instance(NewInstance::new("Survival 1.21", "1.21.4"), None)
        .await
        .unwrap();
    assert_eq!(id.as_str(), "survival-1-21");
    assert!(layout.instance_game_dir("survival-1-21").is_dir());
    assert!(!layout.instance_content_file("survival-1-21").exists());
    assert_eq!(
        read_json(&layout.layout_file()),
        json!({ "layout_version": 1 })
    );
    let raw = read_json(&layout.instance_file("survival-1-21"));
    assert!(raw.get("id").is_none(), "the ID is the folder name only");
    assert_eq!(raw["name"], "Survival 1.21");

    let renamed = launcher
        .update_instance("survival-1-21", |mut instance| {
            instance.name = "  Hardcore ".into();
            instance.settings.window =
                Some(game(json!({ "window": { "width": 1600 } })).window.unwrap());
            Ok(instance)
        })
        .await
        .unwrap();
    assert_eq!(renamed.name, "Hardcore");
    assert_eq!(renamed.created_at, created.created_at);
    assert!(
        layout.instance_dir("survival-1-21").is_dir(),
        "the folder keeps its name"
    );

    launcher
        .update_global_settings(|mut global| {
            global.game.memory_max_mb = Some(8192);
            Ok(global)
        })
        .await
        .unwrap();
    let effective = launcher.effective_settings("survival-1-21").await.unwrap();
    assert_eq!(effective.memory_max_mb, 8192);
    assert_eq!(effective.width, 1600);
    assert_eq!(effective.height, 480);

    let listed = launcher.list_instances().await.unwrap();
    assert!(
        matches!(&listed[..], [InstanceEntry::Ok { instance, .. }] if instance.name == "Hardcore")
    );

    launcher
        .delete_instance("survival-1-21", false)
        .await
        .unwrap();
    assert!(!layout.instance_dir("survival-1-21").exists());
    assert!(launcher.list_instances().await.unwrap().is_empty());
    drop(dir);
}

// covers: AC-4
#[tokio::test]
async fn ids_count_up_and_given_ids_are_checked() {
    let Setup { launcher, .. } = setup();
    let new = || NewInstance::new("Survival", "1.21.4");
    let (first, _) = launcher.create_instance(new(), None).await.unwrap();
    let (second, _) = launcher.create_instance(new(), None).await.unwrap();
    assert_eq!(
        (first.as_str(), second.as_str()),
        ("survival", "survival-2")
    );

    let taken = launcher
        .create_instance(new(), Some("survival".into()))
        .await
        .unwrap_err();
    assert_eq!(taken.key(), "instance-exists");
    let invalid = launcher
        .create_instance(new(), Some("Bad Id".into()))
        .await
        .unwrap_err();
    assert_eq!(invalid.key(), "instance-invalid-id");
    let (custom, _) = launcher
        .create_instance(new(), Some("my-pack".into()))
        .await
        .unwrap();
    assert_eq!(custom.as_str(), "my-pack");
}

// covers: AC-5
#[tokio::test]
async fn invalid_names_are_refused() {
    let Setup { launcher, .. } = setup();
    let error = launcher
        .create_instance(NewInstance::new("   ", "1.21.4"), None)
        .await
        .unwrap_err();
    assert_eq!(error.key(), "instance-invalid-name");
    launcher
        .create_instance(NewInstance::new("Ok", "1.21.4"), None)
        .await
        .unwrap();
    let error = launcher
        .update_instance("ok", |mut instance| {
            instance.name = "x".repeat(65);
            Ok(instance)
        })
        .await
        .unwrap_err();
    assert_eq!(error.key(), "instance-invalid-name");
    assert_eq!(launcher.instance("ok").await.unwrap().name, "Ok");
}

// covers: AC-6
#[tokio::test]
async fn broken_entries_are_listed_and_never_touched() {
    let Setup { launcher, .. } = setup();
    let layout = launcher.layout();
    launcher
        .create_instance(NewInstance::new("Good", "1.21.4"), None)
        .await
        .unwrap();
    let instances = layout.instances_dir();
    let valid = fs::read(layout.instance_file("good")).unwrap();
    let with = |id: &str, bytes: &[u8]| {
        fs::create_dir_all(instances.join(id)).unwrap();
        fs::write(instances.join(id).join("instance.json"), bytes).unwrap();
    };
    with("bad-json", b"{ nope");
    let mut missing: Value = serde_json::from_slice(&valid).unwrap();
    missing.as_object_mut().unwrap().remove("name");
    with("no-name", &serde_json::to_vec(&missing).unwrap());
    let mut newer: Value = serde_json::from_slice(&valid).unwrap();
    newer["format_version"] = json!(99);
    let newer = serde_json::to_vec(&newer).unwrap();
    with("too-new", &newer);
    let mut icon: Value = serde_json::from_slice(&valid).unwrap();
    icon["icon"] = json!({ "file": "../x" });
    with("bad-icon", &serde_json::to_vec(&icon).unwrap());
    fs::create_dir(instances.join("empty")).unwrap();
    fs::create_dir(instances.join("My Folder")).unwrap();
    fs::write(instances.join("foo"), b"file").unwrap();
    fs::create_dir(instances.join(".creating-x-1")).unwrap();

    let listed = launcher.list_instances().await.unwrap();
    let summary: Vec<(&str, Option<&str>)> = listed
        .iter()
        .map(|entry| match entry {
            InstanceEntry::Ok { id, .. } => (id.as_str(), None),
            InstanceEntry::Broken { id, error } => (id.as_str(), Some(error.key.as_str())),
        })
        .collect();
    assert_eq!(
        summary,
        [
            ("My Folder", Some("instance-invalid-id")),
            ("bad-icon", Some("storage-json")),
            ("bad-json", Some("storage-json")),
            ("empty", Some("instance-not-found")),
            ("foo", Some("storage-io")),
            ("good", None),
            ("no-name", Some("storage-json")),
            ("too-new", Some("storage-too-new")),
        ]
    );
    assert_eq!(
        fs::read(instances.join("too-new").join("instance.json")).unwrap(),
        newer
    );
    let error = launcher.update_instance("too-new", Ok).await.unwrap_err();
    assert_eq!(error.key(), "storage-too-new");
    assert_eq!(
        fs::read(instances.join("too-new").join("instance.json")).unwrap(),
        newer
    );
}

// covers: AC-6 (links count as broken, unix only: Windows needs rights for links)
#[cfg(unix)]
#[tokio::test]
async fn a_link_in_instances_is_broken() {
    let Setup { launcher, dir } = setup();
    let outside = dir.path().join("outside");
    fs::create_dir(&outside).unwrap();
    fs::create_dir_all(launcher.layout().instances_dir()).unwrap();
    std::os::unix::fs::symlink(&outside, launcher.layout().instance_dir("linked")).unwrap();
    let listed = launcher.list_instances().await.unwrap();
    assert!(matches!(&listed[..], [InstanceEntry::Broken { id, .. }] if id == "linked"));
}

// covers: AC-8
#[tokio::test]
async fn unknown_fields_survive_an_update() {
    let Setup { launcher, .. } = setup();
    let layout = launcher.layout();
    launcher
        .create_instance(NewInstance::new("Keep", "1.21.4"), None)
        .await
        .unwrap();
    let path = layout.instance_file("keep");
    let mut raw = read_json(&path);
    raw["from_the_future"] = json!({ "a": 1 });
    raw["settings"]["future_setting"] = json!(true);
    fs::write(&path, serde_json::to_vec(&raw).unwrap()).unwrap();
    launcher
        .update_instance("keep", |mut instance| {
            instance.name = "Kept".into();
            Ok(instance)
        })
        .await
        .unwrap();
    let raw = read_json(&path);
    assert_eq!(raw["from_the_future"], json!({ "a": 1 }));
    assert_eq!(raw["settings"]["future_setting"], json!(true));
    assert_eq!(raw["name"], "Kept");
}

// covers: AC-3
#[tokio::test]
async fn a_newer_layout_blocks_writes_but_not_reads() {
    let Setup { launcher, .. } = setup();
    let layout = launcher.layout();
    launcher
        .create_instance(NewInstance::new("Old", "1.21.4"), None)
        .await
        .unwrap();
    fs::write(layout.layout_file(), br#"{ "layout_version": 2 }"#).unwrap();
    assert_eq!(launcher.instance("old").await.unwrap().name, "Old");
    assert_eq!(launcher.list_instances().await.unwrap().len(), 1);
    let error = launcher
        .create_instance(NewInstance::new("New", "1.21.4"), None)
        .await
        .unwrap_err();
    assert_eq!(error.key(), "storage-layout-too-new");
    let error = launcher.update_instance("old", Ok).await.unwrap_err();
    assert_eq!(error.key(), "storage-layout-too-new");
}

// covers: AC-7
#[tokio::test]
async fn settings_are_checked_per_layer_and_merged() {
    let Setup { launcher, .. } = setup();
    let error = launcher
        .update_global_settings(|mut global| {
            global.game.memory_max_mb = Some(100);
            Ok(global)
        })
        .await
        .unwrap_err();
    assert_eq!(error.key(), "settings-invalid");
    assert!(
        !launcher.layout().settings_file().exists(),
        "nothing written"
    );

    launcher
        .update_global_settings(|mut global| {
            global.game.memory_min_mb = Some(8192);
            global.game.jvm_args = Some(vec!["-Dglobal".into()]);
            Ok(global)
        })
        .await
        .unwrap();
    let mut new = NewInstance::new("Small", "1.21.4");
    new.settings = game(json!({ "memory_max_mb": 4096, "jvm_args": [] }));
    launcher.create_instance(new, None).await.unwrap();
    let error = launcher.effective_settings("small").await.unwrap_err();
    assert_eq!(error.key(), "settings-memory-range");

    launcher
        .update_global_settings(|mut global| {
            global.game.memory_min_mb = None;
            Ok(global)
        })
        .await
        .unwrap();
    let effective = launcher.effective_settings("small").await.unwrap();
    assert!(
        effective.jvm_args.is_empty(),
        "the empty list replaces the global one"
    );
}

// covers: AC-13
#[tokio::test]
async fn code_fields_need_trust() {
    let Setup { launcher, .. } = setup();
    let layout = launcher.layout();
    launcher
        .update_global_settings(|mut global| {
            global.game.env = Some([("WHO".to_owned(), "global".to_owned())].into());
            Ok(global)
        })
        .await
        .unwrap();
    let mut new = NewInstance::new("Modded", "1.21.4");
    new.settings = game(json!({ "jvm_args": ["-Dmine"], "window": { "width": 1000 } }));
    launcher.create_instance(new, None).await.unwrap();
    let effective = launcher.effective_settings("modded").await.unwrap();
    assert_eq!(
        effective.jvm_args,
        ["-Dmine"],
        "set through the API, so trusted"
    );
    assert!(!effective.untrusted_overrides);

    // Edited by hand: every code field falls back, the rest still counts.
    let path = layout.instance_file("modded");
    let mut raw = read_json(&path);
    raw["settings"]["env"] = json!({ "WHO": "stranger" });
    fs::write(&path, serde_json::to_vec(&raw).unwrap()).unwrap();
    let effective = launcher.effective_settings("modded").await.unwrap();
    assert!(effective.untrusted_overrides);
    assert_eq!(effective.env["WHO"], "global");
    assert!(effective.jvm_args.is_empty());
    assert_eq!(effective.width, 1000);

    launcher.trust_instance_overrides("modded").await.unwrap();
    let effective = launcher.effective_settings("modded").await.unwrap();
    assert!(!effective.untrusted_overrides);
    assert_eq!(effective.env["WHO"], "stranger");

    launcher
        .create_instance(NewInstance::new("Plain", "1.21.4"), None)
        .await
        .unwrap();
    assert!(
        !launcher
            .effective_settings("plain")
            .await
            .unwrap()
            .untrusted_overrides
    );
    let trusted = read_json(&layout.settings_file())["trusted_overrides"].clone();
    assert_eq!(trusted.as_array().unwrap().len(), 1);
}

// covers: AC-13 (a changed code field through the API stays trusted)
#[tokio::test]
async fn updating_code_fields_trusts_the_new_values() {
    let Setup { launcher, .. } = setup();
    launcher
        .create_instance(NewInstance::new("Hooked", "1.21.4"), None)
        .await
        .unwrap();
    launcher
        .update_instance("hooked", |mut instance| {
            instance.settings.hooks =
                game(json!({ "hooks": { "wrapper": ["gamemoderun"] } })).hooks;
            Ok(instance)
        })
        .await
        .unwrap();
    let effective = launcher.effective_settings("hooked").await.unwrap();
    assert_eq!(effective.wrapper, Some(vec!["gamemoderun".to_owned()]));
    assert!(!effective.untrusted_overrides);
}

// covers: AC-11
#[tokio::test]
async fn a_running_instance_cannot_be_deleted_or_started_twice() {
    let Setup { launcher, .. } = setup();
    launcher
        .create_instance(NewInstance::new("Busy", "1.21.4"), None)
        .await
        .unwrap();
    let guard = launcher.acquire_running("busy").await.unwrap();
    assert_eq!(guard.id().as_str(), "busy");

    let started = std::time::Instant::now();
    let second = launcher.acquire_running("busy").await.unwrap_err();
    assert_eq!(second.key(), "instance-running");
    let delete = launcher.delete_instance("busy", true).await.unwrap_err();
    assert_eq!(delete.key(), "instance-running");
    assert!(started.elapsed().as_secs() < 2, "refused without waiting");

    launcher
        .update_instance("busy", |mut instance| {
            instance.settings.memory_max_mb = Some(2048);
            Ok(instance)
        })
        .await
        .unwrap();

    drop(guard);
    launcher.delete_instance("busy", true).await.unwrap();
}

// covers: AC-12
#[tokio::test]
async fn a_failing_trash_keeps_the_folder_and_permanent_deletes() {
    let Setup { launcher, .. } = setup_with(FakeTrash {
        fail: true,
        ..FakeTrash::default()
    });
    let layout = launcher.layout();
    let mut new = NewInstance::new("Gone", "1.21.4");
    new.settings = game(json!({ "jvm_args": ["-X"] }));
    launcher.create_instance(new, None).await.unwrap();
    let cached = layout.library("com/example/lib/1/lib-1.jar");
    fs::create_dir_all(cached.parent().unwrap()).unwrap();
    fs::write(&cached, b"jar").unwrap();
    let version = layout.version_json("1.21.4");
    fs::create_dir_all(version.parent().unwrap()).unwrap();
    fs::write(&version, b"{}").unwrap();

    let error = launcher.delete_instance("gone", false).await.unwrap_err();
    assert_eq!(error.key(), "instance-trash-failed");
    assert!(layout.instance_dir("gone").is_dir());

    launcher.delete_instance("gone", true).await.unwrap();
    assert!(!layout.instance_dir("gone").exists());
    assert!(cached.is_file() && version.is_file(), "the cache stays");
    let trusted = read_json(&layout.settings_file())["trusted_overrides"].clone();
    assert_eq!(trusted, json!([]), "the trust entry goes with the instance");

    let error = launcher.delete_instance("gone", true).await.unwrap_err();
    assert_eq!(error.key(), "instance-not-found");
}

// covers: AC-12 (permanent deletion removes a link, not what it points to)
#[cfg(unix)]
#[tokio::test]
async fn permanent_delete_does_not_follow_links() {
    let Setup { launcher, dir } = setup();
    launcher
        .create_instance(NewInstance::new("Linked", "1.21.4"), None)
        .await
        .unwrap();
    let outside = dir.path().join("precious");
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("world.dat"), b"keep me").unwrap();
    std::os::unix::fs::symlink(
        &outside,
        launcher.layout().instance_game_dir("linked").join("saves"),
    )
    .unwrap();
    launcher.delete_instance("linked", true).await.unwrap();
    assert!(outside.join("world.dat").is_file());
}

// covers: AC-10
#[tokio::test]
async fn a_held_data_lock_makes_changes_busy() {
    let Setup { launcher, .. } = setup();
    launcher
        .create_instance(NewInstance::new("Wait", "1.21.4"), None)
        .await
        .unwrap();
    let lock = fs::File::options()
        .write(true)
        .open(launcher.layout().data_lock())
        .unwrap();
    lock.lock().unwrap();
    let started = std::time::Instant::now();
    let error = launcher.update_instance("wait", Ok).await.unwrap_err();
    assert!(matches!(
        error,
        InstanceError::Storage(StorageError::Busy { .. })
    ));
    assert_eq!(error.key(), "storage-busy");
    assert!(started.elapsed().as_secs_f32() >= 4.9);
    // Reads take no lock.
    assert_eq!(launcher.instance("wait").await.unwrap().name, "Wait");
}

const CHILD_ENV: &str = "EXODUS_TEST_COUNTER_DIR";
const ROUNDS: u32 = 50;

/// Runs inside a child process started by `two_processes_lose_no_change`.
#[tokio::test]
async fn counter_child() {
    let Some(dir) = std::env::var_os(CHILD_ENV) else {
        return;
    };
    let launcher = Launcher::new(LauncherConfig::new(dir)).unwrap();
    for _ in 0..ROUNDS {
        launcher
            .update_instance("counter", |mut instance| {
                let count: u32 = instance.name.parse().unwrap();
                instance.name = (count + 1).to_string();
                Ok(instance)
            })
            .await
            .unwrap();
    }
}

// covers: AC-10
#[tokio::test]
async fn two_processes_lose_no_change() {
    let Setup { launcher, dir } = setup();
    launcher
        .create_instance(NewInstance::new("0", "1.21.4"), Some("counter".into()))
        .await
        .unwrap();
    let spawn = || {
        Command::new(std::env::current_exe().unwrap())
            .args(["counter_child", "--exact", "--nocapture"])
            .env(CHILD_ENV, dir.path())
            .spawn()
            .unwrap()
    };
    let children = [spawn(), spawn()];
    for mut child in children {
        assert!(child.wait().unwrap().success());
    }
    assert_eq!(
        launcher.instance("counter").await.unwrap().name,
        (2 * ROUNDS).to_string()
    );
}
