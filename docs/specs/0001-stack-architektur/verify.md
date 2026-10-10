# Verify: Stack & Architektur · spec 0001 · updated 2026-10-10
_Steps derived from spec 0001 acceptance criteria. `/check verify` runs these; `/test` locks the durable ones._

## UI / manual
- [ ] Run `cargo run -p exodus-cli -- dev-wait`, press Ctrl+C once → prints `Cancelled.`, exit code 130 → AC-6
- [ ] Run `dev-wait` again, press Ctrl+C twice quickly → process ends at once with exit code 130 → AC-6
- [ ] Run `cargo run -p exodus-cli -- dev-wait --verbose` → log lines appear above the spinner without tearing it → AC-6
- [ ] Make `<data_dir>/logs` unusable (for example create a file named `logs` in the data folder), run `dev-wait` → one warning `Could not use the log folder …`, the command still runs → AC-6

## Commands
- [ ] CI run of `.github/workflows/ci.yml` is green on Windows x64, Linux x64 and macOS Apple Silicon, including the `x86_64-apple-darwin` build step → AC-1
- [ ] `cargo build --workspace` and `cargo test --workspace` pass locally → AC-1
- [ ] `exodus --help`, `exodus --version`, `exodus info` exit 0 on all three systems (CI smoke step) → AC-2
- [ ] `exodus bogus` exits with 2 → AC-2 (exit codes)
- [ ] Every text `exodus info` prints matches an entry in `crates/core/locales/en/cli.ftl` → AC-2
- [ ] `cargo tree -p exodus-core -e normal,build` lists none of `tauri`, `clap`, `indicatif`, `anyhow` (CI step) → AC-3
- [ ] Build without `EXODUS_MSA_CLIENT_ID` (and with it set to empty or spaces) → `info` says `not set`; build with a value → `info` says `configured` and never prints the value → AC-4
- [ ] Put the ID only in a root `.env` (after `cargo clean -p exodus-core`) → `info` says `configured`; set the environment variable too → environment wins → AC-4
- [ ] `cargo test -p exodus-core` covers: event through `ChannelSink`, error to English text, `ProgressReporter` throttling with last state, every error key present in `.ftl` → AC-5
- [ ] `exodus info` creates no folders (data folder absent before and after) → AC-2, spec `exodus info`
- [ ] `dev-wait` writes `<data_dir>/logs/exodus.log.<date>` → AC-6
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --check` are clean; every crate has `[lints] workspace = true` → AC-7

## Acceptance-criteria coverage
- AC-1 … CI matrix + local build/test · AC-2 … help/version/info smoke, texts from `.ftl` · AC-3 … cargo tree check · AC-4 … client ID builds · AC-5 … core unit tests · AC-6 … dev-wait manual steps · AC-7 … clippy, fmt, workspace lints
