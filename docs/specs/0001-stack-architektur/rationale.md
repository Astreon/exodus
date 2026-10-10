# 0001. Rust Core Bibliothek mit dünner CLI und Tauri 2 Oberfläche: Begründung

Entscheidungsprotokoll zu [index.md](index.md). `/develop` liest diese Datei nicht.

## Context

> ⚠️ Premise note: Dieser Spec bündelt bewusst Grundformen, die sonst in Spec 4 (Instanzen als JSON, gemeinsamer Cache, Datenordner) und Spec 5 (Token Speicher) fielen, und entscheidet die UI Technik vorweg, die Feature 15 als offen führt. Das ist vertretbar, weil diese Punkte die Workspace Struktur und die Core API prägen. Das Risiko: Spec 4 und 5 dürfen diese Leitplanken nicht still neu verhandeln, und Feature 15 muss im Scope zugeschnitten werden, sonst entscheidet es dieselbe Frage doppelt.

Exodus ist ein Launcher für Windows, macOS (Intel und Apple Silicon) und Linux, neu geschrieben, Open Source, Solo Projekt ohne Deadline. Phase 1 ist nur eine CLI für Vanilla; ab Phase 3 kommt eine Desktop Oberfläche, die Einsteigern genügt und Power Usern alles erlaubt. Die Logik (Login, Metadaten, Downloads, Java, Start) muss darum ohne UI existieren und später unverändert unter einer grafischen Oberfläche laufen.

Die harten Kräfte: hunderte parallele Asset Downloads mit Fortschritt, Abbruch und Fortsetzen; ein Kindprozess (das Spiel), dessen Log live erscheinen soll; Tokens, die nie im Klartext liegen dürfen; Texte, die später übersetzt werden; eine Azure Client ID, deren Freigabe bei Mojang an diesen Launcher gebunden ist und nicht im Repo stehen soll; Clean Room Pflicht gegenüber Prism Launcher, MultiMC und der Modrinth App. Als Solo Projekt zählt jede Stunde, die nicht in Infrastruktur fließt.

Ohne diese Entscheidung baut jedes Feature ab Spec 4 auf Vermutungen über Fehler, Fortschritt und Abbruch, und ein späterer Umbau zieht sich durch jedes Modul.

## Options considered

### Option 1: Rust Core im Prozess, dünne CLI, Tauri 2 mit Web Frontend

Eine UI freie Rust Bibliothek, CLI und Tauri App linken sie direkt; das Frontend ist React mit TypeScript.

**Pros**:
- Core vollständig testbar ohne UI, Oberflächen bleiben dünn (basis: Ports and Adapters, Logik getrennt von Oberflächen).
- Web Frontend erlaubt ein ausgereiftes Designsystem (shadcn/ui, Tailwind) für die Einsteiger Zielgruppe.
- Kleine Binaries, da Tauri die Webview des Systems nutzt.

**Cons**:
- Zwei Sprachen und zwei Toolchains ab Phase 3.
- Webviews unterscheiden sich je System (WebView2, WKWebView, webkit2gtk); Linux Darstellung braucht Tests.

### Option 2: Rust Core mit nativer Rust GUI (egui, iced oder Slint)

Wie Option 1, aber die Oberfläche ist ebenfalls Rust.

**Pros**:
- Eine Sprache, eine Toolchain, kein Web Stack.
- Keine Abhängigkeit von Webviews des Systems.

**Cons**:
- Weniger fertige Komponenten; ein poliertes Designsystem kostet deutlich mehr Eigenbau.
- Barrierefreiheit und Textdarstellung sind je nach Toolkit schwächer.

### Option 3: Rust Dienst im Hintergrund mit CLI und UI als Clients

Ein Daemon hält Zustand, Downloads und laufende Spiele; CLI und Desktop sprechen per IPC mit ihm.

**Pros**:
- Ein Zustand für alle Oberflächen; Downloads laufen weiter, wenn die UI schließt.
- Kein Konflikt zwischen zwei gleichzeitig laufenden Launchern.

**Cons**:
- Protokoll, Lebenszyklus und Start des Dienstes auf drei Systemen sind viel Arbeit vor dem ersten Feature.
- Fehlersuche über Prozessgrenzen ist für ein Solo Projekt teuer.

### Option 4: Electron mit Logik in TypeScript

Alles in TypeScript, Logik im Node Hauptprozess.

**Pros**:
- Eine Sprache für Logik und Oberfläche, riesiges Ökosystem.

**Cons**:
- Keine echte CLI ohne Node Laufzeit; große Binaries und hoher Speicherbedarf.
- Widerspricht dem Scope (Rust Core).

## Rationale

Option 1, weil sie die zwei Hauptkräfte zusammenbringt: eine Logik, die in Phase 1 nur als CLI läuft und in Phase 3 unverändert unter einer UI, und eine Oberfläche, die Einsteiger ohne Anleitung bedienen. Option 2 spart den Web Stack, verlagert aber den Aufwand in ein selbst gebautes Designsystem, was der Einsteiger Leitlinie schadet. Option 3 löst ein Problem (zwei Launcher gleichzeitig), das Dateisperren in Spec 4 billiger lösen (basis: monolith first, keine Infrastruktur vor dem gemessenen Bedarf). Option 4 scheidet am Scope und an der CLI aus.

Innerhalb von Option 1 folgen die Detailwahlen derselben Linie: tokio und reqwest sind der langweilige Standard, den Tauri ohnehin mitbringt (basis: boring technology). rustls mit Plattform Zertifikaten vermeidet OpenSSL unter Linux und scheitert trotzdem nicht an TLS abfangenden Proxys. Typisierte Events an eine Senke statt Kanäle pro Aufruf halten jede Oberfläche gleich einfach und sind für Tauri serialisierbar. Fehler mit stabilen Schlüsseln statt fertiger Sätze machen die spätere Übersetzung zu reiner Datenarbeit (basis: Fluent, Projekt Fluent von Mozilla). Ein explizites Abbruch Token statt Drop der Future, weil Fortsetzen (Slice 2) sauberes Aufräumen braucht. Nach dem Gegenlesen durch ein zweites Modell kamen drei Schärfungen dazu: Senke und Token reisen pro Operation im `OpContext` statt einmal pro Launcher (mehrere Fenster und Befehle brauchen getrennte Events); `ring` statt aws-lc-rs, weil es ohne Zusatzwerkzeuge auf allen Zielen baut; und eine minimale CI schon im Gerüst, weil AC-1 sonst nicht prüfbar ist.

Zur Client ID: ein Pflichtwert beim Build würde jeden Mitwirkenden zu einer eigenen Azure App zwingen. Optional beim Build plus klare Meldung zur Laufzeit hält den Einstieg frei, und offizielle Builds bekommen die ID aus einem CI Secret. Ein Laufzeit Override wurde bewusst nicht gewählt, um keinen zweiten Pfad zu pflegen. GPL 3.0 schützt davor, dass geschlossene, umbenannte Kopien entstehen; die Kosten (der Core ist für geschlossene Projekte nicht wiederverwendbar) sind für dieses Projekt akzeptiert. pnpm aus der Vorlage des Engineers wurde durch Bun ersetzt, weil Bun lokal bereits im Einsatz ist und Tauri es unterstützt.

## References

**Project sources**:
- `docs/scope/scope.md`: Rahmen von Phase 1, Plattformen, Feature 1 bis 5 und 15, Build approach Tracer Bullet, Stufe Beta.
- Vorgaben des Engineers im Design Gespräch (Tauri 2, Workspace Layout, keyring, directories, Qualitätsregeln, Clean Room).
- Installierte Skills: `rust-engineer`, `rust-async-patterns`, `tauri-v2` (unter `.claude/skills/`).

**Practices & standards**:
- Ports and Adapters (Logik getrennt von Oberflächen).
- Monolith first, keine Infrastruktur vor gemessenem Bedarf.
- Boring technology.
- Fluent Lokalisierungsformat (Weblate und Crowdin unterstützen es).
- Inhaltsadressierte Ablage für Assets, wie Mojang sie vorgibt.
- Clean Room Entwicklung.

**Links** (beim Landscape Check im Gespräch abgerufen; Versionen bei Gerüstbau erneut prüfen):
- tokio: https://docs.rs/crate/tokio/latest
- reqwest: https://crates.io/crates/reqwest
- rustls: https://crates.io/crates/rustls
- thiserror: https://crates.io/crates/thiserror
- clap: https://crates.io/crates/clap
- tracing: https://crates.io/crates/tracing
- i18n-embed: https://docs.rs/crate/i18n-embed/latest
- tokio-util (CancellationToken): https://docs.rs/tokio-util/latest
- Rust 2024 Edition: https://blog.rust-lang.org/2025/01/22/rust-2024-beta/
