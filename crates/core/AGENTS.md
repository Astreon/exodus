# exodus-core

## Overview

All launcher logic, free of any user interface. Surfaces (the CLI now, the Tauri app from Phase 3) call it in process through the `Launcher` facade, get progress as typed `Event`s and errors as `Localize` values, and turn them into text only when they show them.

## Key files

| File | Owns |
|---|---|
| `src/lib.rs` | Public API and re-exports (including `CancellationToken`) |
| `src/launcher.rs` | `Launcher` facade, `OpContext`, `OpContext::run` |
| `src/event.rs` | `Event`, `EventSink`, `ChannelSink`, `NoopSink`, `ProgressReporter` |
| `src/error.rs` | `Localize`, `ErrorView`, `Cancelled` |
| `src/i18n.rs` | Fluent loader, `message`, `MessageArgs`, the error key test |
| `src/config.rs` | `LauncherConfig`, `Endpoints`, `default_data_dir`, `log_dir`, data folder lookup (`resolve_data_dir`, `locate_data_dir`) |
| `src/storage.rs` | `DataLayout`, versioned JSON, atomic writes, data lock `locks/data.lock`, migrations and backups |
| `src/instance.rs` | `Instance`, `InstanceId` (slug, never changes), `instance.json` format |
| `src/settings.rs` | Global settings, per instance overrides, merging, trust hashes for code fields |
| `src/store.rs` | The data folder methods on `Launcher`: instances, settings, trust, delete, running lock |
| `build.rs` | Reads `EXODUS_MSA_CLIENT_ID` and the target into the build |
| `locales/en/*.ftl` | Every text Exodus prints, one file per module plus `cli.ftl` |

## Conventions

- One module per domain (`auth`, `meta`, `download`, …), created by the feature that needs it. No empty placeholder modules.
- Every long operation is an `async` method on `Launcher` taking `ctx: &OpContext`, and wraps its work in `ctx.run(kind, …)`, which sends `Started` and exactly one end event (`Finished`, `Failed` or `Cancelled`).
- Progress goes through `ProgressReporter` (about ten events per second, last state always sent). `EventSink::emit` never blocks.
- Cancellation: respect the token at each `await` (`tokio::select!`); blocking work in `spawn_blocking` calls `ctx.check_cancelled()` between chunks. A cancelled run returns `Err` with `is_cancelled() == true`.
- Errors: one `thiserror` enum per module implementing `Localize`, wrapping `Cancelled(#[from] Cancelled)` and forwarding `is_cancelled`. Each new key needs an entry in `locales/en/<module>.ftl`, and each variant a sample in the module's `error_samples()`, registered in `all_error_samples()` in `i18n.rs`.
- Fluent types never appear in the public API; use `MessageArgs`.
- Public types a surface can see (events, `ErrorView`, later instances) derive `Serialize`; `Event` and error enums are `#[non_exhaustive]`.
- The core never starts a runtime or a tracing subscriber; it only emits `tracing` events.
- External base URLs change only through `LauncherConfig.endpoints` (tests), never through environment variables.
- Data folder files: every change takes the data lock, reads the file fresh and writes it through `storage` (temp file plus `rename`); each `store` method runs in one `spawn_blocking` and holds no lock across an `await`. JSON files carry a format version and keep unknown fields.
- Instance fields that can run code (`java`, `jvm_args`, `env`, `hooks`) only take effect when the user trusted exactly those values (trust hash in the global settings).
- Lock files are never deleted, not even when an instance is deleted (spec 0002).

## Gotchas

- `fl!` checks only literal keys. The key test in `i18n.rs` is the real guard, so a variant missing from `error_samples()` slips through.
- After creating `.env` for the first time, run `cargo clean -p exodus-core` once; later changes are picked up.
- An empty `EXODUS_MSA_CLIENT_ID` counts as unset (fork PRs in CI). The ID ends up in the binary; it is not a secret.
- Reserved Fluent key: `auth-client-id-missing` (its variant arrives with spec 5).
- If a derive macro trips `unsafe_code = "forbid"`, lower it to `deny` in that crate only, with a comment.

## Related specs

- [0001 Stack & Architektur](../../docs/specs/0001-stack-architektur/index.md)
- [0002 Datenablage & Instanzformat](../../docs/specs/0002-datenablage-instanzformat/index.md)

_Drafted by /audit from the repo, worth a quick human pass. Edit freely: once a line stops matching this draft, later runs treat it as curated and will flag rather than overwrite it._
