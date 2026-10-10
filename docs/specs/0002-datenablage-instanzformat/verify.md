# Verify: Datenablage & Instanzformat · spec 0002 · updated 2026-10-11
_Steps derived from spec 0002 acceptance criteria (Teil A, feature 4). `/check verify` runs these; `/test` locks the durable ones._

## Commands
- [ ] `exodus info --data-dir ./x` → `Data folder: <cwd>\x (from --data-dir)`, and no folder `x` exists afterwards → AC-1
- [ ] `exodus --data-dir ./x info` (flag before the command) → same output as above → AC-1
- [ ] `EXODUS_DATA_DIR=<path> exodus info` → `<path> (from EXODUS_DATA_DIR)`; with `--data-dir y` as well, the flag wins → AC-1
- [ ] `EXODUS_DATA_DIR= exodus info --data-dir ""` → `(system default)`: empty values count as unset → AC-1
- [ ] `exodus info --data-dir ../a/./b` → path keeps `..` and drops `.` → AC-1
- [ ] Put `portable.txt` next to the `exodus` binary, run `exodus info` → `<exe_dir>/data (portable mode)`; remove the file again → AC-1
- [ ] `exodus info` on Windows → `%LOCALAPPDATA%\Exodus\data (system default)`; on Linux `~/.local/share/exodus`; on macOS `~/Library/Application Support/Exodus` → AC-2
- [ ] `cargo test -p exodus-core --test instances` → all pass (create, rename, list, broken entries, versions, lock, two processes, running lock, trash, trust) → AC-3 to AC-13
- [ ] `cargo test -p exodus-core --lib storage::` → layout paths, atomic write, stale temp cleanup, migration chain and `.v0.bak` backup pass → AC-3, AC-8, AC-9
- [ ] `cargo test -p exodus-core --lib settings::` → layer merge, per layer checks, hash → AC-7, AC-13
- [ ] `cargo test -p exodus-core --lib instance::` → slug table, `validate_id`, name limits, icon file names → AC-4, AC-5, AC-6

## Data folder checks (after the instance tests, or by hand with a small program on `Launcher`)
- [ ] After the first change, `<data>/exodus.json` is `{ "layout_version": 1 }` → AC-3
- [ ] `exodus.json` with `layout_version: 2` → creating or updating an instance fails with `storage-layout-too-new`; `instance` and `list_instances` still work → AC-3
- [ ] A new instance has `instances/<id>/instance.json` and `minecraft/`, no `content.json`, no `id` field in the JSON → AC-4
- [ ] Renaming keeps the folder name and `created_at`, and trims the name; 65 characters or only spaces fail with `instance-invalid-name` → AC-5
- [ ] `instance.json` with `format_version: 99` lists as broken with `storage-too-new` and stays byte for byte the same, even after an update attempt → AC-8
- [ ] An unknown field (top level and inside `settings`) survives an update → AC-8
- [ ] No `.*.tmp-*` file is left after writes; one older than an hour is removed by the next write in that folder → AC-9
- [ ] Holding `locks/data.lock` from outside makes a change fail with `storage-busy` after about 5 seconds; reads still work → AC-10
- [ ] With a `RunningGuard` held, `delete_instance` and a second `acquire_running` fail at once with `instance-running`; `update_instance` works → AC-11
- [ ] `delete_instance(id, false)` moves the folder to the system trash (check the recycle bin) → AC-12
- [ ] A failing trash gives `instance-trash-failed` and the folder is still there; `permanent = true` deletes it, leaves `versions/` and `libraries/` alone, and removes the instance's `trusted_overrides` entry → AC-12
- [ ] On Linux or macOS: a link inside `minecraft/` pointing outside survives `permanent` deletion with its target intact → AC-12

## Value sourcing
- [ ] Name `Survival 1.21` → ID `survival-1-21`; a second `Survival` → `survival-2`; `Über Café` → `ueber-cafe`; `CON` → `con-1`; `日本語` → `instance` → ID from `slugify` against existing names
- [ ] `created_at` is a UTC RFC 3339 time from the moment of creation and never changes on update → `Timestamp::now()` at the edge
- [ ] Global `memory_max_mb: 8192` plus instance `window.width: 1600` → effective 8192 MB, 1600 × 480 → defaults < `settings.json` < `instance.json`
- [ ] Global `memory_min_mb: 8192` and instance `memory_max_mb: 4096` both save; `effective_settings` fails with `settings-memory-range` → cross layer check
- [ ] `jvm_args` set through `create_instance` count; the same instance with `env` edited by hand gives global values for all four code fields, keeps its own window, and `untrusted_overrides: true`; after `trust_instance_overrides` they count again → hash against `trusted_overrides`
- [ ] Temp file names look like `.instance.json.tmp-<pid>-<8 hex>` → `process::id()` plus `SystemRandom`

## Acceptance-criteria coverage
- AC-1: commands 1 to 6 · AC-2: command 7 · AC-3: data folder checks 1, 2 · AC-4: check 3, slug value step · AC-5: check 4 · AC-6: instance tests (`broken_entries_are_listed_and_never_touched`) · AC-7: memory and layer value steps · AC-8: checks 5, 6 · AC-9: check 7 · AC-10: check 8, two process test · AC-11: check 9 · AC-12: checks 10 to 12 · AC-13: trust value step
- Teil B (AC-14 to AC-16) is built with feature 5, not covered here.
