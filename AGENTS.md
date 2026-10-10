# Exodus

Cross platform Minecraft launcher (Windows, macOS, Linux), written from scratch in Rust. Working title.

## Stack

- **Language / Runtime**: Rust 1.99.0, edition 2024, pinned in `rust-toolchain.toml`; tokio, started only by the binaries
- **Workspace**: Cargo workspace (`resolver = "3"`), members `crates/*`: `exodus-core` (UI free library) and `exodus-cli` (binary `exodus`); from Phase 3 `apps/desktop` (Tauri 2, React, TypeScript, Tailwind, shadcn/ui, Vite)
- **Key dependencies**: reqwest (rustls on `ring`, no OpenSSL), serde, thiserror in core / anyhow in surfaces only, Fluent via i18n-embed, tracing
- **Package manager**: cargo, every version in `[workspace.dependencies]`, `Cargo.lock` committed; Bun for the frontend from Phase 3
- **CI**: GitHub Actions (`.github/workflows/ci.yml`): Windows x64, Linux x64, macOS Apple Silicon, plus a macOS Intel cross build
- Full decision: [spec 0001](docs/specs/0001-stack-architektur/index.md)

## Build approach

Tracer Bullet (zuerst ein dünner, echter Faden vom Login bis zum laufenden Spiel, dann jedes Segment einzeln verbreitern).

## Commands

```bash
cargo build --workspace
cargo run -p exodus-cli -- info          # run the CLI
cargo test --workspace
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
```

Optional build input `EXODUS_MSA_CLIENT_ID` (environment, else root `.env`, see `.env.example`). Empty means the build has no sign in.

## Specs

Stored in `docs/specs/`. Format: `docs/specs/NNNN-title/index.md`. Scope: `docs/scope/scope.md`.

## Rules

- Functional core, imperative shell: parsing, version resolution, planning and argument building are pure functions (data in, data out). I/O (HTTP, disk, processes) sits at the edges in async functions that call them.
- Immutable by default; `mut` only where it clearly helps. No global mutable state: statics are constants, shared state lives behind `Launcher`.
- Plain structs and enums plus small traits; composition over trait hierarchies; `Option` over sentinel values; iterator chains where they read better than loops.
- Expected failures are `Result` values with a `thiserror` enum per module; panics only for real invariants.
- `exodus-core` never depends on `tauri`, `clap`, `indicatif` or `anyhow` (CI checks it). Surfaces stay thin.
- Code, comments and commits in English. Clean room: never read or copy code from Prism Launcher, MultiMC or the Modrinth App; use public specs, official docs and Mojang metadata only.
- No telemetry. Never log tokens, auth codes or the client ID.
- Lints: `unsafe_code = "forbid"`, clippy `all` at warn, `-D warnings` in CI. Every crate sets `[lints] workspace = true` and `rust-version.workspace = true`.
- Every `pub` item in `exodus-core` has a doc comment. Naming: standard Rust; error enums `<Module>Error`; Fluent keys kebab case with an area prefix (`error-cancelled`, `info-version`).
- Commits follow Conventional Commits: `feat(core): …`, `fix(cli): …`, `docs: …`.
- Tests: unit tests in `#[cfg(test)]` next to the code, integration tests in `crates/*/tests/`; HTTP against wiremock; real network tests `#[ignore]`. Pure code needs no mocks.

## Tooling

- Pre commit hook `.githooks/pre-commit`: `cargo fmt --all --check` and clippy with `-D warnings`; enable once with `git config core.hooksPath .githooks`. Tests stay in CI.
- Before merge: fmt, clippy, tests, cargo-deny (GPL compatible licence allow list, RustSec advisories).
- Still to set up (`/develop tooling`, scope feature 2): the hook, `deny.toml`, `LICENSE` (GPL-3.0-or-later), CI caching and the cargo-deny step.

## Git

- integration: on
- branch prefix: feat/
- commit: per-milestone
- base branch: master

## Agent skills

- [rust-engineer](.claude/skills/rust-engineer/): `jeffallan/claude-skills`, idiomatic Rust, ownership, error handling, testing
- [rust-async-patterns](.claude/skills/rust-async-patterns/): `wshobson/agents`, tokio, cancellation, concurrency
- [github-actions-templates](.claude/skills/github-actions-templates/): `wshobson/agents`, CI workflows
- Phase 3, move to `apps/desktop/AGENTS.md` once it exists: [tauri-v2](.claude/skills/tauri-v2/) (`nodnarbnitram/claude-code-extensions`), [shadcn](.claude/skills/shadcn/) (`shadcn-ui/ui`), [vercel-react-best-practices](.claude/skills/vercel-react-best-practices/) (`vercel-labs/agent-skills`), [tailwind-design-system](.claude/skills/tailwind-design-system/) (`wshobson/agents`)

Declined: pnpm skill (Bun instead)
MCP servers: rust-docs via `docsrs-mcp` (configured in `.mcp.json`); from Phase 3 recommended: shadcn/ui MCP, Tauri MCP (`dirvine/tauri-mcp`)

## Context files

- [crates/core/AGENTS.md](crates/core/AGENTS.md): the UI free core library: operations, events, cancellation, errors and Fluent texts
- [crates/cli/AGENTS.md](crates/cli/AGENTS.md): the `exodus` binary: commands, exit codes, logging, Ctrl+C

_Drafted by /audit from the repo, worth a quick human pass. Edit freely: once a line stops matching this draft, later runs treat it as curated and will flag rather than overwrite it._
