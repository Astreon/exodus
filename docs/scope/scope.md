# Scope: Exodus (Arbeitstitel)

Ein neuer, plattformübergreifender Minecraft Launcher für Windows, macOS und Linux, komplett neu geschrieben (kein Fork, kein übernommener Code aus Prism Launcher, MultiMC oder der Modrinth App). Er verbindet die Flexibilität von Prism mit dem Komfort der Modrinth App: Einsteiger erstellen ohne Anleitung eine Instanz und spielen, Power User können trotzdem alles einstellen (progressive disclosure statt Einstellungswüste). Open Source und kostenlos, Solo Projekt ohne Deadline.

**Build approach:** Tracer Bullet (zuerst ein dünner, echter Faden vom Login bis zum laufenden Spiel, dann jedes Segment einzeln verbreitern).
**Workflow:** Beta (nach `/develop` folgen `/check verify`, dann `/test`). Gilt für den Rust Core. UI Experimente in Phase 3 tragen `· Prototype`, bis das Design steht; danach auf Beta anheben. `/architect` ist der empfohlene erste Schritt für ein Feature mit einer echten Entscheidung, aber überspringbar, wenn du den Bau schon kennst.

**Rahmen für Phase 1:** nur CLI, nur Vanilla. Keine UI, keine Modloader, kein Modrinth oder CurseForge, keine Modpacks, kein Import, kein Auto Updater, kein Offline oder Cracked Modus. Plattformen: Windows x64, Linux x64, macOS Intel und Apple Silicon. Ausgaben auf Englisch, Texte zentral gehalten, damit später eine Übersetzungsplattform andocken kann. Keine Telemetrie. Phase 1 gilt als erreicht, wenn 1.8.9 und die aktuelle Release Version auf allen drei Systemen starten.

_Das sind Empfehlungen, damit dein Bau geordnet bleibt, keine Pflicht. Überspring, was nicht passt: wenn du schon weißt, wie du ein Feature baust, nimm direkt `/develop` und lass `/architect` weg. Du entscheidest, wann ein Feature `done` ist._

## At a glance

| # | Feature | Phase | Status |
|---|---------|-------|--------|
| 1 | Stack & Architektur | Foundation | in-progress |
| 2 | Coding Standards & Tooling | Foundation | planned |
| 3 | Azure App & Minecraft API Zugang | Foundation | planned |
| 4 | Datenablage & Instanzformat | Foundation | planned |
| 5 | Microsoft Login | Slice 1 | planned |
| 6 | Versionsliste & Vanilla Instanzen | Slice 1 | planned |
| 7 | Spieldateien herunterladen | Slice 1 | planned |
| 8 | Spiel starten & Log streamen | Slice 1 | planned |
| 9 | Parallele, wiederaufnehmbare Downloads | Slice 2 | planned |
| 10 | Automatische Java Runtime | Slice 3 | planned |
| 11 | Altes Startformat (1.8.9 und älter) | Slice 4 | planned |
| 12 | Alte Versionen auf Apple Silicon | Slice 5 | planned |
| 13 | Fabric & Quilt | Phase 2 | planned |
| 14 | Modrinth Suche & Installation | Phase 2 | planned |
| 15 | UI Stack & Designsystem | Phase 3 | planned |
| 16 | Instanzbibliothek & Spielen | Phase 3 | planned |
| 17 | Konten & Einstellungen | Phase 3 | planned |
| 18 | Content Browser | Phase 3 | planned |
| 19 | Instanzdetails | Phase 3 | planned |
| 20 | Forge & NeoForge | Phase 4 | planned |
| 21 | CurseForge Anbindung | Phase 4 | planned |
| 22 | Modpacks | Phase 4 | planned |
| 23 | Mod Updates mit Abhängigkeiten | Phase 4 | planned |
| 24 | Import aus anderen Launchern | Phase 4 | planned |
| 25 | Signierte Builds & Notarisierung | Phase 5 | planned |
| 26 | Auto Updater | Phase 5 | planned |

## Foundations

### 1. Stack & Architektur · in-progress
Rust Workspace so schneiden, dass der Core eine eigenständige Bibliothek ist und die CLI (Phase 1) wie später die Desktop UI (Phase 3) nur dünne Oberflächen darauf sind.
**Done when:** die Struktur (Core Bibliothek, CLI, Fehler und Fortschrittsmodell, Async Modell) steht in einem Spec, und das leere Gerüst baut und läuft auf Windows, macOS und Linux.
spec [0001](../specs/0001-stack-architektur/index.md) · code in `crates/`
- [x] Decide the stack (spec): `/architect stack & architektur`
- [x] Scaffold from the decision: `/develop stack & architektur`
- [x] Verify it: `/check verify stack & architektur`
- [x] Test it: `/test stack & architektur`

### 2. Coding Standards & Tooling
Konventionen und Tooling aus dem echten Gerüst festhalten, dann Formatierung, Linting und eine CI Matrix über alle drei Systeme einrichten.
**Done when:** root `AGENTS.md` beschreibt den echten Stack, eine Lizenzdatei liegt im Repo, Texte für Nutzer liegen zentral (bereit für eine spätere Übersetzungsplattform), und CI baut und testet auf Windows, macOS (Apple Silicon) und Linux.
- [ ] Capture conventions + tooling choices: `/audit`

### 3. Azure App & Minecraft API Zugang
Azure App registrieren und den Minecraft API Zugang bei Mojang beantragen. Die Freigabe kann Tage bis Wochen dauern, darum früh starten und parallel weiterbauen.
**Done when:** eine Client ID existiert, der Zugang ist freigegeben, und die Client ID ist so hinterlegt, wie Spec 1 es festlegt.
- [ ] App registrieren und Zugang beantragen (manueller Schritt, kein Befehl)
- [ ] Freigabe erhalten

### 4. Datenablage & Instanzformat · needs a decision
Wo und wie Exodus Daten ablegt: Datenordner pro System, Instanzformat, gemeinsamer Cache für Libraries, Assets und Runtimes, Kontendaten. Teuerster Teil zum späteren Umbauen, darum vor allen Slices.
**Done when:** das Format trägt spätere Phasen (Modloader, Mods, mehrere Konten, Import) ohne brechende Migration, und es ist versioniert.
- [ ] Design it (spec): `/architect datenablage & instanzformat`

## Slice 1: Erster Start (der dünne Faden)

Ziel dieses Slices: einmal echt durch alle Schichten. Login, aktuelle Release Version anlegen, herunterladen, starten, Log sehen. Auf deinem eigenen System, mit einem Java, auf das du zeigst. Breite kommt danach.

### 5. Microsoft Login · needs a decision
Anmeldung per Device Code Flow über die ganze Kette bis zum Minecraft Profil, Token sicher speichern und erneuern.
**Done when:** `login` zeigt Code und URL, nach Bestätigung sind Profilname und UUID bekannt, Tokens liegen nicht im Klartext auf der Platte, und ein abgelaufener Token wird ohne erneuten Login erneuert; Fehler (kein Spielbesitz, abgelehnte Freigabe, kein Xbox Profil) sind verständlich.
- [ ] Design it (spec): `/architect microsoft login`

### 6. Versionsliste & Vanilla Instanzen
Versionsliste von Mojang laden und Vanilla Instanzen über die CLI anlegen, auflisten und löschen. Baut auf Spec 4.
**Done when:** die CLI listet Releases (Snapshots per Schalter), legt eine Instanz für eine gewählte Version an, listet und löscht sie, und Löschen fasst den gemeinsamen Cache nicht an.
- [ ] Build it: `/develop versionsliste & vanilla instanzen`

### 7. Spieldateien herunterladen · needs a decision
Client JAR, Libraries, Natives und Assets für eine Instanz beziehen, mit Hashprüfung und den Regeln pro System. In diesem Slice reicht ein einfacher Ablauf; der Spec entscheidet aber das ganze Downloadmodell inklusive parallel und wiederaufnehmbar (Slice 2).
**Done when:** alle Dateien der aktuellen Release Version liegen geprüft im Cache, eine beschädigte Datei wird erkannt und neu geladen, und ein zweiter Lauf lädt nichts doppelt.
- [ ] Design it (spec): `/architect spieldateien herunterladen`

### 8. Spiel starten & Log streamen · needs a decision
Classpath und Argumente im neuen Format (`arguments`) bauen, das Spiel als Prozess starten und die Ausgabe live streamen.
**Done when:** die aktuelle Release Version startet mit dem angemeldeten Konto bis ins Hauptmenü und lässt sich spielen, das Log erscheint live in der CLI, und Exit Code sowie Absturz werden gemeldet.
- [ ] Design it (spec): `/architect spiel starten & log streamen`

## Slice 2: Robuste Downloads

### 9. Parallele, wiederaufnehmbare Downloads
Den Downloadpfad aus Feature 7 verbreitern: parallel, mit Fortschritt, nach Abbruch wiederaufnehmbar, mit Wiederholen bei Netzfehlern. Baut auf Spec von Feature 7.
**Done when:** ein Download nach Abbruch (Strg+C, Netz weg) setzt fort statt neu zu beginnen, viele kleine Assets laden spürbar schneller als sequenziell, und Fortschritt ist sichtbar.
- [ ] Build it: `/develop parallele, wiederaufnehmbare downloads`

## Slice 3: Java automatisch

### 10. Automatische Java Runtime · needs a decision
Die passende Java Runtime pro Minecraft Version selbst beziehen und verwalten, statt auf ein installiertes Java zu zeigen.
**Done when:** für 1.8.9 und die aktuelle Version wird jeweils die passende Runtime auf allen drei Systemen bezogen, geprüft und genutzt, und mehrere Instanzen teilen sich dieselbe Runtime.
- [ ] Design it (spec): `/architect automatische java runtime`

## Slice 4: Alte Versionen

### 11. Altes Startformat (1.8.9 und älter)
Das alte Argumentformat (`minecraftArguments`), das Entpacken der Natives und ältere Asset Indizes unterstützen. Baut auf Spec von Feature 8.
**Done when:** 1.8.9 startet unter Windows, Linux und macOS Intel bis ins Hauptmenü, mit Ton und Sprache korrekt.
- [ ] Build it: `/develop altes startformat`

## Slice 5: Apple Silicon für alte Versionen

### 12. Alte Versionen auf Apple Silicon · needs a decision
Für 1.8.9 auf Apple Silicon fehlen offizielle ARM Natives (LWJGL 2) und teils eine passende Java 8 Runtime von Mojang. Hier muss entschieden werden, woher beides kommt.
**Done when:** 1.8.9 und die aktuelle Release Version starten nativ auf einem Apple Silicon Mac (ohne Rosetta), womit das Ziel von Phase 1 auf allen drei Systemen erreicht ist.
- [ ] Design it (spec): `/architect alte versionen auf apple silicon`

## Phase 2: Fabric, Quilt und Modrinth (grob)

### 13. Fabric & Quilt · needs a decision
Modloader Fabric und Quilt in Instanzen installieren und starten; legt das Modell für vererbte Versionsprofile fest, auf dem später Forge und NeoForge aufbauen.
**Done when:** eine Fabric und eine Quilt Instanz starten mit einem einfachen Mod.
- [ ] Design it (spec): `/architect fabric & quilt`

### 14. Modrinth Suche & Installation · needs a decision
Mods auf Modrinth suchen und passend zu Version und Loader in eine Instanz installieren, inklusive Abhängigkeiten.
**Done when:** Suche filtert nach Version und Loader, Installation löst Pflichtabhängigkeiten auf und meldet Konflikte.
- [ ] Design it (spec): `/architect modrinth suche & installation`

## Phase 3: Desktop UI (grob)

### 15. UI Stack & Designsystem · needs a decision · Prototype
UI Technik festlegen und eine Designsprache mit Basiskomponenten aufbauen, auf dem fertigen Core. Leitlinie: progressive disclosure, Einsteiger zuerst.
**Done when:** `design.md` beschreibt Typografie, Farben, Abstände und Komponenten, und eine Basisansicht ruft echten Core Code auf.
- [ ] Design it (spec): `/architect ui stack & designsystem`

### 16. Instanzbibliothek & Spielen · needs a decision · Prototype
Startansicht mit Instanzen, neue Instanz erstellen und mit einem Klick spielen.
**Done when:** ein Einsteiger erstellt ohne Anleitung eine Instanz und landet im Spiel.
- [ ] Design it (spec): `/architect instanzbibliothek & spielen`

### 17. Konten & Einstellungen · needs a decision · Prototype
Mehrere Microsoft Konten verwalten und wechseln, Einstellungen mit einfachen Standards und aufklappbaren Details.
**Done when:** mehrere Konten lassen sich hinzufügen und wechseln, und Einstellungen sind global sowie pro Instanz setzbar.
- [ ] Design it (spec): `/architect konten & einstellungen`

### 18. Content Browser · needs a decision · Prototype
Mods, Resource Packs und Shader im Launcher finden und installieren, zuerst über Modrinth.
**Done when:** Inhalte lassen sich durchsuchen, filtern und mit einem Klick in eine Instanz installieren.
- [ ] Design it (spec): `/architect content browser`

### 19. Instanzdetails · needs a decision · Prototype
Pro Instanz: Log Ansicht, Screenshots, Welten und Serverliste.
**Done when:** alle vier Bereiche zeigen die echten Daten der Instanz.
- [ ] Design it (spec): `/architect instanzdetails`

## Phase 4: Mehr Loader, Plattformen und Modpacks (grob)

### 20. Forge & NeoForge · needs a decision
Forge und NeoForge inklusive ihrer Installer Logik unterstützen.
**Done when:** je eine Forge und NeoForge Instanz startet mit einem Mod, für mindestens eine alte und eine aktuelle Version.
- [ ] Design it (spec): `/architect forge & neoforge`

### 21. CurseForge Anbindung · needs a decision
CurseForge als zweite Quelle im Content Browser. Bei Open Source darf der API Key nicht offen im Code stehen.
**Done when:** Suche und Installation funktionieren wie bei Modrinth, und der API Key ist nicht im Quellcode.
- [ ] Design it (spec): `/architect curseforge anbindung`

### 22. Modpacks · needs a decision
Modpacks installieren und aktualisieren (`.mrpack` und CurseForge Packs).
**Done when:** ein Modpack beider Formate wird installiert und startet, und ein Update behält Nutzerdaten (Welten, Einstellungen).
- [ ] Design it (spec): `/architect modpacks`

### 23. Mod Updates mit Abhängigkeiten · needs a decision
Updates für installierte Mods finden und anwenden, mit Auflösung der Abhängigkeiten.
**Done when:** verfügbare Updates werden angezeigt, und ein Update hält die Instanz startfähig oder meldet den Konflikt vorher.
- [ ] Design it (spec): `/architect mod updates mit abhängigkeiten`

### 24. Import aus anderen Launchern · needs a decision
Instanzen aus Prism Launcher, MultiMC und der Modrinth App übernehmen.
**Done when:** eine Instanz aus jedem der drei Launcher wird übernommen und startet.
- [ ] Design it (spec): `/architect import aus anderen launchern`

## Phase 5: Release Reife (grob)

### 25. Signierte Builds & Notarisierung · needs a decision
Signierte Builds für alle drei Systeme, Notarisierung unter macOS.
**Done when:** Installer starten unter Windows und macOS ohne Sicherheitswarnung, und Linux Pakete sind signiert.
- [ ] Design it (spec): `/architect signierte builds & notarisierung`

### 26. Auto Updater · needs a decision · GA
Der Launcher aktualisiert sich selbst. Liefert Code auf fremde Rechner aus, darum eine Stufe höher als der Rest.
**Done when:** Updates werden vor dem Einspielen auf ihre Signatur geprüft, ein fehlgeschlagenes Update lässt die alte Version lauffähig.
- [ ] Design it (spec): `/architect auto updater`

## Deferred
Außerhalb der geplanten Phasen, festgehalten, damit der Plan ehrlich bleibt.
- **Übersetzungen**: Anbindung an eine Übersetzungsplattform (z. B. Crowdin oder Weblate), Deutsch als erste weitere Sprache · needs a decision
- **Opt in Absturzberichte**: nur mit Zustimmung, vor dem öffentlichen Release · needs a decision
- **Weitere Architekturen**: Linux ARM64 und Windows ARM64 · needs a decision

## Legend

**The decision box.** Every feature carries exactly one, the sub-task whose label ends with `(spec)`. Its wording varies (`Design it (spec)` normally, `Decide the stack (spec)` on Stack & Architektur), so skills locate it by that `(spec)` suffix, never by an exact label. Every other box is an execution box and `/architect` never ticks one.

**Feature lifecycle**: the scope updates as a feature moves; each row is what it shows and who sets it:

| State | Set by | The feature shows |
|---|---|---|
| `planned` · needs a decision | `/scope` | one box: `Design it (spec): /architect <feature>` |
| `in-progress` (designed) | **`/architect` at spec capture** | `Design it` ticked; spec linked; `Build it: /develop <feature>` + **2 to 5 milestones**; the tier's closing boxes (`Verify it` Alpha+, `Test it` Beta+, `Review it` + `Document it` GA); any surfaced follow-up enrolled |
| `in-progress` (building) | `/develop` | milestone sub-boxes tick one by one; code pointer filled |
| `in-progress` (verified) | `/check verify` | `Build it` + milestones ticked; `Verify it` ticked |
| `done` | **you, when you decide it is** (any skill sets it when you say so); `/sync` reconciles | boxes you ran ticked, skipped ones marked skipped; the tier's last stage (`Prototype` → after `/develop`; `Alpha` → after `/check verify`; `Beta`/`GA` → after `/test`) is the suggested point to call it done; `/sync` captures conventions |

- **Next step** = the first unticked box (always a command or a tracked milestone).
- **needs a decision** = run `/architect` first; otherwise straight to `/develop` (or `/audit` for standards & tooling). The tag drops once the spec is captured.
- **Atomic build tasks live in the spec's `## Build plan`, not here**: the scope carries only the milestone rollup.
- **Status** `planned` → `in-progress` → `done`, plus `existing` (pre-workflow) and `dropped` (de-scoped, kept for history).
- **Approach tag** beside a heading (e.g. `· Facade`) overrides the project default for that feature; no tag = inherits it.
- **Workflow tier tag** beside a heading (e.g. `· GA`, `· Prototype`) sets that one feature's rigor above or below the project default; no tag inherits the default. It decides the feature's check boxes and each skill's next suggestion.
- **Workflow** (header line) is the project default, what runs after `/develop`: **Prototype** = nothing (trust develop's own build time self check); **Alpha** = `/check verify`; **Beta** = `/check verify` then `/test`; **GA** = adds a fresh model `/check review` then `/document`. A feature built on an unratified decision (an `Assumed` spec) stays flagged, but that never blocks `done`.
- **Pointer line** (`spec <n> · code in <path>`): the spec link added by `/architect`, the code path by `/develop`.
