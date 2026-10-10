# exodus-cli

## Overview

The `exodus` binary, the Phase 1 surface over `exodus-core`. A thin shell: it parses arguments with clap, starts the tokio runtime and logging, wires Ctrl+C to a `CancellationToken`, runs one command, and maps the result to an exit code.

## Key files

| File | Owns |
|---|---|
| `src/main.rs` | Entry point, runtime, log guard, returns `ExitCode` |
| `src/cli.rs` | clap definitions (`Cli`, `Command`) |
| `src/commands/` | One module per subcommand, dispatched in `commands::run` |
| `src/failure.rs` | `Failure`, error output and exit codes |
| `src/logging.rs` | tracing subscriber: daily log file plus stderr through indicatif |
| `src/signal.rs` | First Ctrl+C cancels the token, second exits at once |

## Commands

```bash
cargo run -p exodus-cli -- info
cargo run -p exodus-cli -- dev-wait --verbose   # debug builds only: tests Ctrl+C and logging
```

## Conventions

- Exit codes: `0` success, `1` failure, `2` misuse (clap), `130` cancelled (decided via `is_cancelled()`, never per module checks).
- Every text the CLI prints comes from `crates/core/locales/en/cli.ftl` via `exodus_core::i18n::message`. Only clap's own help, version line and misuse errors stay English in code (Phase 1).
- Core errors become `Failure::core(&error)`; `anyhow` is for CLI only problems.
- Logs: `<data_dir>/logs/exodus.log.<date>`, daily rotation, at most 7 files; filter via `EXODUS_LOG` (default `info` in the file, `warn` on stderr without `--verbose`). If the folder can't be created, warn once and continue on stderr.
- stderr output goes through indicatif's `MultiProgress` so progress bars don't tear.
- A command that writes no log file (like `info`) must not create folders and makes no network calls.

## Gotchas

- `main` returns `ExitCode`; never call `std::process::exit`, or the log `WorkerGuard` drops late and the last lines are lost.

## Related specs

- [0001 Stack & Architektur](../../docs/specs/0001-stack-architektur/index.md)

_Drafted by /audit from the repo, worth a quick human pass. Edit freely: once a line stops matching this draft, later runs treat it as curated and will flag rather than overwrite it._
