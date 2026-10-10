# 0001. Rust Core Bibliothek mit dünner CLI und Tauri 2 Oberfläche

**Date**: 2026-10-10
**Status**: Accepted

## Summary

Exodus besteht aus einer Rust Bibliothek (`exodus-core`), die die gesamte Launcher Logik enthält, und dünnen Oberflächen darauf: zuerst eine CLI, ab Phase 3 eine Tauri 2 Desktop App mit React Frontend. Der Core kennt keine Oberfläche; er meldet Fortschritt über typisierte Events, Fehler über Enums mit stabilen Schlüsseln, und lässt sich über ein Token abbrechen. So lässt sich der Core ohne UI testen, und die spätere Desktop App ist nur eine Hülle. Für den Bau heißt das: das erste Gerüst enthält nur `crates/core`, `crates/cli` und eine minimale CI, der Workspace ist aber schon auf `apps/desktop` vorbereitet.

## Requirements

**User stories**:
- Als Entwickler will ich einen Workspace, in dem der Core ohne UI baut und testbar ist, damit jede spätere Oberfläche nur eine dünne Hülle ist.
- Als Mitwirkender will ich das Projekt ohne eigene Azure App bauen können, damit der Einstieg nichts kostet.

**Acceptance criteria** (für das Gerüst, das dieser Spec festlegt):
- **AC-1**: `cargo build --workspace` und `cargo test --workspace` laufen fehlerfrei auf Windows x64 (MSVC), Linux x64 und macOS Apple Silicon; zusätzlich gelingt `cargo build --workspace --target x86_64-apple-darwin` auf dem Apple Silicon Runner (nur Build, keine Tests). Geprüft durch `.github/workflows/ci.yml` im Gerüst.
- **AC-2**: `exodus --help`, `exodus --version` und `exodus info` laufen auf allen drei Systemen mit Exit Code 0. Alle Texte, die Exodus selbst ausgibt, kommen aus `crates/core/locales/en/*.ftl`. Ausgenommen in Phase 1: Hilfe, Versionszeile und Bedienfehler, die clap selbst erzeugt (englisch im Code).
- **AC-3**: `exodus-core` hängt weder direkt noch transitiv (über weitere Abhängigkeiten) von `tauri`, `clap`, `indicatif` oder `anyhow` ab; ein CI Schritt prüft das automatisch über `cargo tree -p exodus-core -e normal,build`.
- **AC-4**: Ohne `EXODUS_MSA_CLIENT_ID` (oder mit leerem Wert) baut alles; `exodus info` meldet dann klar, dass diese Build keine Microsoft Client ID hat. Mit gesetztem, nicht leerem Wert meldet es „configured“, ohne die ID auszugeben.
- **AC-5**: Der Core exportiert `Launcher`, `LauncherConfig`, `OpContext`, `Event`, `EventSink`, `ChannelSink`, `NoopSink`, `ProgressReporter`, `Localize`, `ErrorView`, `build_info` und `i18n::message`. Tests zeigen: ein Event kommt durch `ChannelSink` an; ein Core Fehler wird über Fluent zu englischem Text; `ProgressReporter` drosselt, liefert aber den letzten Stand immer; jeder Fehlerschlüssel existiert in den `.ftl` Dateien.
- **AC-6**: Die CLI schreibt Diagnoselogs in eine täglich rotierende Datei unter `<data_dir>/logs/` (höchstens 7 Dateien); lässt sich der Ordner nicht anlegen, läuft sie nur mit stderr weiter. `--verbose` spiegelt Logs nach stderr, ohne Fortschrittsbalken zu zerstören. Strg+C bricht über das Token ab und endet mit Exit Code 130; ein zweites Strg+C beendet sofort mit 130.
- **AC-7**: Jedes Mitglied erbt die Workspace Lints (`unsafe_code = "forbid"`), und `cargo clippy --workspace --all-targets -- -D warnings` sowie `cargo fmt --check` sind sauber.

## Decision

**Chosen option**: Option 1: Rust Core Bibliothek im Prozess, dünne CLI, Tauri 2 Desktop App

Die gesamte Logik lebt in einer UI freien Bibliothek `exodus-core`, die CLI und Desktop App direkt im selben Prozess nutzen; alle übrigen Werkzeuge stehen in der Tabelle unten.

**Implementation skills**: `rust-engineer` (`jeffallan/claude-skills`, `.claude/skills/rust-engineer/`) · `rust-async-patterns` (`wshobson/agents`, `.claude/skills/rust-async-patterns/`) · `github-actions-templates` (`wshobson/agents`, `.claude/skills/github-actions-templates/`) · ab Phase 3: `tauri-v2` (`nodnarbnitram/claude-code-extensions`, `.claude/skills/tauri-v2/`) · `shadcn` (`shadcn-ui/ui`, `.claude/skills/shadcn/`) · `vercel-react-best-practices` (`vercel-labs/agent-skills`, `.claude/skills/vercel-react-best-practices/`) · `tailwind-design-system` (`wshobson/agents`, `.claude/skills/tailwind-design-system/`)

## Proposed stack

| Layer | Choice | Reason |
|---|---|---|
| Sprache | Rust, Edition 2024, neueste Stable zum Zeitpunkt des Gerüsts, gepinnt in `rust-toolchain.toml` (mit `components = ["rustfmt", "clippy"]`); `rust-version` = dieselbe Version; Windows Ziel `x86_64-pc-windows-msvc` | Alle bauen gleich; neue Clippy Lints kommen nur bei bewusstem Anheben. |
| Prozessmodell | Core als Bibliothek im Prozess, kein Daemon | Einfachste Form, passt zu CLI und Tauri, keine IPC zu pflegen. |
| Workspace | Cargo Workspace (`resolver = "3"`), Mitglieder `crates/*`, ab Phase 3 `apps/desktop`; Bun für das Frontend erst ab Phase 3 | Ein Repo, gemeinsame Versionen über `[workspace.dependencies]`, gemeinsame Lints über `[workspace.lints]`. |
| Core | `crates/core`, Paketname `exodus-core` (Bibliothek `exodus_core`), ein Crate mit Modulen | Der Name `core` ist als Paketname unbrauchbar (kollidiert mit Rusts eingebautem `core`); Module lassen sich später zu Crates herauslösen. |
| CLI | `crates/cli`, Paketname `exodus-cli`, Binary `exodus`; clap (derive) + indicatif | Standard für Rust CLIs; indicatif zeichnet Download Fortschritt. |
| Desktop (Phase 3) | Tauri 2 in `apps/desktop`, Frontend in `apps/desktop/ui`: React, TypeScript, Tailwind, shadcn/ui, Vite, Bun als Paketmanager (`bun.lock`) | Vom Engineer entschieden (Bun statt pnpm, weil lokal schon im Einsatz); dünne Hülle, die `Launcher` als Tauri State hält und Events weiterleitet. |
| Async | tokio | Standard Runtime; reqwest und Prozesse bauen darauf, `tauri::async_runtime` läuft ebenfalls auf tokio. |
| HTTP | reqwest ohne Standard Features, mit rustls, Plattform Zertifikatsprüfung, JSON, Streaming und gzip; Verbindungs Timeout 10 s, kein Gesamt Timeout | Keine OpenSSL Abhängigkeit unter Linux, trotzdem funktionieren Firmen Proxys mit eigenem Zertifikat; Downloads dürfen lange dauern. |
| Kryptografie Backend | `ring` für rustls (nicht aws-lc-rs) | Baut ohne NASM oder CMake unter Windows und problemlos als Intel Cross Build auf macOS. |
| Serialisierung | serde, serde_json | Für Mojang Metadaten, Instanzdateien und die Weitergabe von Events an Tauri. |
| Fehler | thiserror im Core (ein Enum pro Modul, alle mit `Localize`), anyhow nur in `exodus-cli` und später `apps/desktop` | Oberflächen können Fehler unterscheiden; Text entsteht erst beim Anzeigen. |
| Events | Typisiertes `Event` Enum an eine `EventSink`, die pro Operation über `OpContext` mitgegeben wird | Core bleibt UI frei; jede Oberfläche und jedes Fenster bekommt eigene Events. |
| Abbruch | `tokio_util::sync::CancellationToken` im `OpContext` jeder langen Operation | Sauberes Aufräumen und Fortsetzen statt hartem Abbruch. |
| Texte | Fluent (`.ftl`), nur Englisch, in `crates/core/locales/en/`; i18n-embed (Konfig in `crates/core/i18n.toml`, Rückfallsprache `en`); Schlüssel per Test geprüft | Eine Quelle für alle Oberflächen; Weblate und Crowdin können Fluent. |
| Logging | tracing, tracing-subscriber, tracing-appender (tägliche Datei, 7 Dateien), Ausgabe auf stderr über indicatif geleitet | Strukturierte Logs, Datei für Bug Reports; keine Telemetrie. |
| Tokens | keyring Crate (Schlüsselbund des Systems), nie Klartext auf der Platte; nicht im Gerüst | Spec 5 fügt es mit expliziten Features pro System hinzu und regelt den Fall ohne Schlüsselbund. |
| Datenordner | directories Crate, überschreibbar per Umgebungsvariable, dazu ein Portable Modus | Im Gerüst nur der Standardpfad; Variablenname, Portable Modus und Unterordner legt Spec 4 fest. |
| Instanzen | Eine JSON Datei pro Instanz mit Versionsfeld | Instanzordner sind kopierbar und reparierbar; Format im Detail in Spec 4. |
| Download Cache | Gemeinsamer Speicher für alle Instanzen im Mojang Layout (`libraries/` nach Maven Pfad, `assets/objects/` nach Hash), jede Datei per SHA1 geprüft | Startargumente erwarten genau diese Struktur; keine doppelten GB. Details in Spec 4 und 7. |
| Client ID | `build.rs` liest `EXODUS_MSA_CLIENT_ID` (Umgebung oder gitignorte `.env` im Repo Wurzel), optional | Repo bleibt frei von der ID; Mitwirkende bauen ohne eigene Azure App. |
| Tests | `cargo test`; HTTP gegen lokalen Mock Server (wiremock); echte Netztests mit `#[ignore]` | CI bleibt stabil und offline fähig; echte Server nur auf Wunsch. |
| Qualität | `cargo fmt --check`, `cargo clippy -D warnings`, Tests und cargo-deny sind Pflicht vor jedem Merge | cargo-deny prüft Lizenzen gegen eine GPL verträgliche Erlaubnisliste und RustSec Hinweise. Einrichtung von cargo-deny in Feature 2. |
| CI und Repo | GitHub, GitHub Actions. Gerüst: minimale `ci.yml` (Build, Test, fmt, clippy, AC-3 Prüfung auf Windows, Linux, macOS Apple Silicon plus Intel Cross Build) | Macht AC-1 prüfbar; Feature 2 baut sie aus (cargo-deny, Caching, weitere Schritte). |
| Lizenz | `GPL-3.0-or-later` für das ganze Repo | Forks müssen offen bleiben; alle geplanten Crates sind verträglich. |

### Architektur im Detail

**Inhalt des Gerüsts.** `exodus-core` enthält nur die Module `config`, `launcher`, `event`, `error`, `i18n` und `build_info`, dazu einen Re-Export von `CancellationToken`. Keine leeren Domänen Module (`auth`, `meta`, `download` usw.); die legt jedes Feature selbst an. Kein keyring, kein Netzwerkaufruf im Gerüst.

**Core API (Fassade).** `Launcher` ist der einzige Einstieg, gebaut über `Launcher::new(LauncherConfig)`. Er hält intern (hinter `Arc`) den gemeinsamen `reqwest::Client`, die Konfiguration und einen Zähler für Operationen. `Launcher` ist `Clone + Send + Sync`, damit Tauri ihn als State halten kann. Spätere Operationen sind `async` Methoden, gruppiert nach Modul, und geben je ihren eigenen Fehlertyp zurück.

**LauncherConfig.** Felder: `data_dir: PathBuf`, `endpoints: Endpoints` (alle externen Basis URLs, Standard = echte Endpunkte), `user_agent: String`. `LauncherConfig::new(data_dir)` füllt den Rest mit Standards. Den Standardordner liefert `exodus_core::config::default_data_dir()` (directories, `ProjectDirs::from("", "", "Exodus")`); Spec 4 ersetzt den Inhalt dieser Funktion, nicht ihre Signatur. Endpunkte lassen sich nur über die Config ändern (Tests), nie über Umgebungsvariablen, damit niemand Token Endpunkte von außen umleiten kann.

**OpContext und Events.** Jede lange Operation nimmt `ctx: &OpContext`, gebaut über `launcher.op(sink, cancel)`. `OpContext` trägt die `OperationId`, eine `Arc<dyn EventSink>` und das `CancellationToken`. `OperationId(u64)` kommt aus einem `AtomicU64` im Launcher und ist `Copy + Serialize`. `Event` ist ein `#[non_exhaustive]` Enum, `Clone + Serialize`, jede Variante trägt die `OperationId`. Grundmenge im Gerüst: `Started { op, kind }`, `Progress { op, done, total }`, `Log { op, line }`, `Finished { op }`, `Failed { op, error: ErrorView }`, `Cancelled { op }`. Die Operation selbst sendet `Started` zu Beginn und genau ein Endevent (`Finished`, `Failed` oder `Cancelled`).

**EventSink.** Trait mit `fn emit(&self, event: Event)`, verlangt `Send + Sync + 'static`, darf nie blockieren. Der Core liefert `NoopSink` und `ChannelSink` (auf `tokio::sync::mpsc::UnboundedSender`). Unbegrenzt ist vertretbar, weil Fortschritt beim Erzeuger gedrosselt wird. `Started`, `Finished`, `Failed`, `Cancelled` und `Log` werden nie verworfen. Wie der Spiel Log Strom (viele Zeilen) gepuffert wird, regelt Spec 8.

**ProgressReporter.** Gemeinsamer Helfer im Gerüst, den jede Operation für Fortschritt nutzt: höchstens etwa zehn `Progress` Events pro Sekunde, der letzte Stand (z. B. 100 %) wird immer gesendet. Die Uhr ist für Tests austauschbar.

**Fehler und Texte.**
- `pub trait Localize: std::error::Error + Send + Sync + 'static` mit `fn key(&self) -> &'static str`, `fn args(&self) -> MessageArgs` und `fn is_cancelled(&self) -> bool` (Standard `false`). `MessageArgs` ist ein eigener einfacher Map Typ (Schlüssel zu Text oder Zahl); Fluent Typen erscheinen nie in der öffentlichen API.
- `ErrorView { key, args, detail }` ist `Clone + Serialize`, gebaut aus jedem `&E: Localize`; `detail` ist der technische Text (`Display` plus Ursachenkette) für Logs und Bug Reports.
- `exodus_core::i18n::message(id, &MessageArgs) -> String` rendert über einen einmal geladenen Fluent Loader (fest `en`). CLI Texte liegen in `crates/core/locales/en/cli.ftl`, Core Texte in eigenen Dateien je Modul.
- Ein Test zählt alle Fehlervarianten auf und prüft, dass jeder Schlüssel in den `.ftl` Dateien existiert und alle Variablen geliefert werden. `fl!` prüft nur wörtliche Schlüssel, darum ersetzt dieser Test die Prüfung beim Kompilieren.
- Schlüssel für eine fehlende Client ID ist reserviert: `auth-client-id-missing`. Die Fehlervariante dazu legt Spec 5 an.
- Die CLI ermittelt Exit Code 130 über `is_cancelled()`, nicht über Abfragen je Modul.

**Abbruch.** Das Token wird an jeder `await` Stelle per `tokio::select!` beachtet. Arbeit in `spawn_blocking` (Hashen, Entpacken) prüft `is_cancelled()` zwischen Stücken, weil sich blockierende Arbeit nicht von außen abbrechen lässt. Ein abgebrochener Lauf gibt immer `Err` mit `is_cancelled() == true` zurück und sendet `Cancelled`, nie `Finished`. Was mit Teildateien passiert, regelt Spec 7. Die CLI verbindet das erste Strg+C mit dem Token; ein zweites beendet sofort mit 130.

**Runtime.** Der Core startet keine Runtime; die Binaries tun das (`#[tokio::main]` in der CLI, `tauri::async_runtime` in der Desktop App). Blockierende oder rechenlastige Arbeit läuft über `tokio::task::spawn_blocking`.

**Logging.** Der Core schreibt nur `tracing` Ereignisse; den Subscriber richten die Binaries ein. Datei: `<data_dir>/logs/exodus.log.<Datum>`, tägliche Rotation, höchstens 7 Dateien. Scheitert das Anlegen (z. B. Ordner nur lesbar), läuft die CLI nur mit stderr weiter und warnt einmal. Der `WorkerGuard` des Datei Schreibers lebt bis zum Ende von `main`; `main` gibt den Exit Code zurück (`ExitCode`) statt `std::process::exit` aufzurufen, sonst gehen die letzten Zeilen verloren. Die stderr Ausgabe läuft über indicatif (`MultiProgress::suspend` oder tracing-indicatif), damit Balken nicht zerreißen. Filter über `EXODUS_LOG` (Syntax wie `RUST_LOG`). Tokens, Codes und die Client ID werden nie geloggt.

**build_info und `exodus info`.** `exodus_core::build_info() -> BuildInfo { version, target, msa_client_id_configured: bool }`. `exodus info` gibt als reinen Text (kein `--json` in Phase 1) aus: Version, Zielsystem, Status der Client ID, Datenordner und Log Ordner. Kein Netzwerk, und es legt keine Ordner an; Exit Code 0.

**Client ID.** `crates/core/build.rs` liest `EXODUS_MSA_CLIENT_ID` aus der Umgebung; fehlt sie dort, aus `.env` im Repo Wurzel (Pfad über `CARGO_MANIFEST_DIR`, also `../../.env`). Regeln: Umgebung gewinnt; Zeilen `KEY=VALUE`, Kommentare mit `#` ignoriert, Anführungszeichen und Leerzeichen entfernt, kein dotenv Crate; ein leerer Wert gilt als nicht gesetzt (wichtig für Fork PRs in CI, wo das Secret leer ist). Weitergabe per `cargo:rustc-env`, mit `rerun-if-env-changed=EXODUS_MSA_CLIENT_ID` und `rerun-if-changed=../../.env`. Der Core liest sie mit `option_env!`. Offizielle Builds setzen sie als GitHub Actions Secret. Hinweis: die ID steckt im fertigen Binary und ist dort auslesbar; sie ist kein Geheimnis, das Ziel ist nur, dass Forks nicht unter der freigegebenen ID ausgeliefert werden.

**Konfigurationswerte und ihre Quelle**:

| Wert | Quelle |
|---|---|
| Microsoft Client ID | Build Zeit: `EXODUS_MSA_CLIENT_ID` (Umgebung, sonst `.env`), leer = nicht gesetzt |
| Datenordner | `config::default_data_dir()` (directories); Override und Portable Modus folgen in Spec 4 |
| Log Ordner | `<data_dir>/logs/` |
| Log Filter | `EXODUS_LOG`, Standard `info` in der Datei, `warn` auf stderr ohne `--verbose` |
| Externe Basis URLs | `LauncherConfig.endpoints`, Standard = echte Endpunkte, nur per Config änderbar |
| User Agent | `Exodus/<CARGO_PKG_VERSION> (+<Repo URL>)`, die Repo URL als Konstante im Core |
| Zielsystem in `info` | Build Zeit Zielsystem (über `build.rs` aus `TARGET` weitergegeben) |
| Versionen der Crates | aktuelle Version zum Zeitpunkt des Gerüsts, zentral in `[workspace.dependencies]`; reqwest Feature Namen gegen genau diese Version prüfen, weil sie sich zwischen Versionen geändert haben |

**Regeln für das Repo.**
- Code, Kommentare und Commits auf Englisch.
- Clean Room: kein Code aus Prism Launcher, MultiMC oder der Modrinth App lesen oder übernehmen. Grundlage sind nur öffentliche Spezifikationen, offizielle Dokumentation und Mojangs Metadaten.
- `[workspace.lints]`: `unsafe_code = "forbid"`, Clippy Gruppe `all` auf Warnung (in CI per `-D warnings` zum Fehler). Jedes Mitglied setzt `[lints] workspace = true` und `rust-version.workspace = true`. Löst ein Derive Makro (z. B. von rust-embed) `forbid` aus, nur in diesem Crate auf `deny` senken, mit Kommentar.
- `Cargo.lock` wird eingecheckt (das Repo baut Binaries). `.gitignore` enthält `target/` und `.env`.
- Eine gemeinsame Workspace Version (SemVer, `0.x` bis zum ersten öffentlichen Release).
- Alle öffentlichen Core Typen, die eine Oberfläche sehen kann (Events, `ErrorView`, später Instanzen), leiten `Serialize` ab. Wie sie nach TypeScript kommen, entscheidet Spec 15.

**Exit Codes der CLI**: `0` Erfolg, `1` Fehler, `2` falsche Bedienung (clap), `130` abgebrochen.

## Consequences

**Positive**:
- Der Core ist ohne UI testbar; jede Oberfläche bleibt dünn und austauschbar.
- Fehler und Events sind typisiert und serialisierbar, die Desktop App kann sie direkt an das Frontend geben.
- Übersetzungen lassen sich später ohne Umbau anbinden, weil alle eigenen Texte schon in Fluent liegen.
- Mitwirkende bauen ohne Azure App, ohne OpenSSL und ohne NASM oder CMake.

**Negative / tradeoffs**:
- Zwei Sprachen und zwei Paketmanager ab Phase 3 (Cargo und Bun); CI wird dann schwerer (Bun, WebView2, webkit2gtk).
- Ohne Daemon sehen zwei gleichzeitig laufende Launcher (CLI und Desktop) einander nicht; Spec 4 braucht Dateisperren für gemeinsame Daten.
- Fehlertexte über Schlüssel sind beim Schreiben umständlicher als `format!`; jeder neue Fehler braucht einen Fluent Eintrag und taucht im Schlüssel Test auf.
- `OpContext`, Drosselung über `ProgressReporter`, genau ein Endevent und stückweises Prüfen des Tokens sind Regeln, die jede neue Operation einhalten muss.
- Die Plattform Zertifikatsprüfung scheitert auf minimalen Linux Systemen ohne installierte Zertifikate; Spec 7 sollte TLS Fehler als eigene Fehlerart mit klarer Meldung führen.
- Die Hilfe von clap bleibt in Phase 1 englisch im Code und muss vor einer Übersetzung nachgezogen werden.
- Ohne Client ID gebaute Versionen können sich nicht anmelden; das muss die Meldung klar sagen.

**Neutral**:
- Diese Spec legt Grundformen fest, die eigentlich zu Spec 4 (JSON pro Instanz, gemeinsamer Cache, directories, Portable Modus) und Spec 5 (keyring) gehören. Diese Specs verfeinern innerhalb dieser Leitplanken.
- Die minimale CI entsteht schon hier; Feature 2 erweitert sie statt sie neu anzulegen.
- Die UI Technik (Tauri 2, React, Tailwind, shadcn/ui) ist hiermit entschieden; Feature 15 behält nur Designsystem und TS Typ Brücke.

## Follow-up

- [ ] Scope Feature 15 umbenennen oder zuschneiden: die UI Technik ist durch diesen Spec entschieden, offen bleiben Designsystem, `design.md` und die Rust nach TypeScript Typ Brücke (tauri-specta, ts-rs oder anderes).
- [ ] Spec 4 übernimmt: Name der Umgebungsvariable für den Datenordner, Erkennung des Portable Modus, Dateisperren gegen zwei gleichzeitige Launcher, genaues Instanz JSON und Cache Layout, Inhalt von `default_data_dir()`.
- [ ] Spec 5 übernimmt: keyring mit expliziten Features pro System, Verhalten ohne Schlüsselbund (Linux ohne Secret Service) innerhalb der Leitplanke „nie Klartext“, Fehlervariante für den reservierten Schlüssel `auth-client-id-missing`.
- [ ] Spec 7 übernimmt: TLS Fehler als eigene Fehlerart; Umgang mit Teildateien bei Abbruch.
- [ ] Spec 8 übernimmt: Pufferung und Verwerfregel für den Spiel Log Strom.
- [ ] Feature 2 (`/audit`): `AGENTS.md` aus dem echten Gerüst schreiben, `LICENSE` (GPL 3.0) anlegen, cargo-deny Konfiguration ergänzen und die minimale `ci.yml` ausbauen.
- [ ] Endgültigen Namen vor dem ersten öffentlichen Release festlegen: „Exodus“ steckt in Binary, Paketnamen, Umgebungsvariablen, Datenordner und Schlüsselbund Eintrag; danach kostet eine Umbenennung eine Datenmigration.
- [ ] Installierte Skills gehören in den Abschnitt `## Agent skills` von `AGENTS.md` (`/audit` oder `/sync` schreibt ihn): `rust-engineer`, `rust-async-patterns`, `github-actions-templates` in die Wurzel; `tauri-v2`, `shadcn`, `vercel-react-best-practices`, `tailwind-design-system` in ein eigenes `apps/desktop/AGENTS.md`, sobald es existiert. Abgelehnt: `pnpm` Skill (Bun statt pnpm).
- [ ] MCP Server, die du in deinen MCP Einstellungen verbinden willst: Rust Docs MCP (mcp-docsrs, Community) schon für Phase 1; shadcn/ui MCP (offiziell) und Tauri MCP (dirvine/tauri-mcp, Community) ab Phase 3. Danach in `AGENTS.md` unter `MCP servers:` vermerken.

## Rationale

Begründung und Optionen: siehe [rationale.md](rationale.md).
