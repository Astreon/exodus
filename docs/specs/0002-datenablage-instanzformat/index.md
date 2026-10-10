# 0002. Datenablage im Mojang Layout mit versionierten JSON Dateien pro Instanz

**Date**: 2026-10-10
**Status**: Accepted

## Summary

Dieser Spec legt fest, wo Exodus seine Daten ablegt und wie die Dateien aussehen. Alles liegt in einem Datenordner: ein gemeinsamer Cache im Mojang Layout für Versionen, Libraries, Assets und Runtimes, je Instanz ein Ordner mit `instance.json` und einem Spielordner `minecraft/`, dazu globale Einstellungen und Konten in eigenen Dateien. Jede Datei trägt eine `format_version`, bewahrt unbekannte Felder, wird atomar geschrieben (erst eine temporäre Datei, dann ein Umbenennen) und ist durch eine Dateisperre gegen zwei gleichzeitig laufende Exodus Prozesse geschützt. Tokens liegen verschlüsselt in einer Datei, deren Schlüssel im Schlüsselbund des Systems steckt (portabel: aus einer Passphrase). Feature 4 baut das Fundament, die Instanzen und die Einstellungen; den Kontenteil baut Feature 5 nach diesem Spec.

## Requirements

**User stories**:
- Als Spieler will ich eine Instanz umbenennen können, ohne dass Pfade, Welten oder Verknüpfungen brechen.
- Als Spieler will ich, dass ein Absturz oder ein zweites offenes Exodus Fenster nie eine halb geschriebene oder überschriebene Datei hinterlässt.
- Als Power User will ich Einstellungen global setzen und pro Instanz nur das überschreiben, was abweicht.
- Als Power User will ich den Datenordner verlegen oder Exodus portabel vom USB Stick starten.
- Als Spieler will ich, dass eine kopierte oder importierte Instanz keine Befehle auf meinem Rechner ausführt, ohne dass ich zustimme.
- Als Entwickler will ich, dass spätere Phasen (Modloader, Mods, mehrere Konten, Import) und zwei Exodus Versionen nebeneinander ohne brechende Migration in das Format passen.
- Als Spieler will ich, dass meine Anmeldung auf keinem System im Klartext auf der Platte liegt.

**Acceptance criteria** (Teil A baut Feature 4, Teil B baut Feature 5):

Teil A
- **AC-1**: Der Datenordner wird in dieser Reihenfolge bestimmt: CLI Flag `--data-dir`, dann Umgebungsvariable `EXODUS_DATA_DIR`, dann `portable.txt` neben der Binary (Daten in `<exe_dir>/data/`), sonst der Standard. Ein leerer Wert bei Flag oder Variable gilt als nicht gesetzt. Relative Pfade werden per `std::path::absolute` gegen das aktuelle Arbeitsverzeichnis absolut gemacht, ohne `..` aufzulösen. `exodus info` zeigt Pfad und Quelle (`flag`, `env`, `portable`, `default`) und legt weiterhin keine Ordner an.
- **AC-2**: Der Standard ist `ProjectDirs::from("", "", "Exodus").data_local_dir()`: Windows `%LOCALAPPDATA%\Exodus\data`, macOS `~/Library/Application Support/Exodus`, Linux `$XDG_DATA_HOME/exodus` (meist `~/.local/share/exodus`); gegen die gepinnte `directories` Version geprüft. Die Signatur von `default_data_dir()` bleibt gleich.
- **AC-3**: Alle Pfade entstehen aus einer reinen Funktion über die Wurzel und entsprechen exakt dem Layout im Abschnitt *Feature design*. Jede schreibende Operation läuft so: Datensperre nehmen, `exodus.json` lesen und prüfen, erst dann Ordner anlegen und schreiben. Fehlt `exodus.json`, gilt Layout 1, und die Datei wird mit `layout_version: 1` angelegt. Bei einer höheren `layout_version` verweigert jede schreibende Operation mit `storage-layout-too-new`; Lesen bleibt erlaubt.
- **AC-4**: `create_instance` erzeugt aus dem Namen einen Slug nach den *Slug Regeln* unten. Der Slug ist die ID, ist der Ordnername und steht nicht in `instance.json`. Ein automatisch erzeugter Slug zählt bei Kollision hoch (`-2`, `-3` …, Vergleich ohne Rücksicht auf Groß und Kleinschreibung); eine vom Aufrufer vorgegebene ID muss `validate_id` bestehen (sonst `instance-invalid-id`) und darf nicht existieren (sonst `instance-exists`). Angelegt werden `instances/<id>/instance.json` und `instances/<id>/minecraft/`, und zwar erst in `instances/.creating-<id>-<zufall>/` und dann per Umbenennen an den endgültigen Platz; `content.json` wird nicht angelegt.
- **AC-5**: Umbenennen ändert nur `name` in `instance.json`; Ordnername und ID bleiben unverändert. Ein Name muss nach dem Trimmen 1 bis 64 Zeichen haben, sonst `instance-invalid-name`.
- **AC-6**: `list_instances` liefert pro Eintrag in `instances/` (außer Namen, die mit `.` beginnen) entweder die Instanz oder `Broken { id, error }`. `Broken` sind: fehlende `instance.json`, ungültiges JSON, fehlendes Pflichtfeld, zu neue `format_version`, ungültiges `icon.file`, ein Name, der `validate_id` nicht besteht, eine Datei oder ein Symlink statt eines Ordners, ein Name, der kein gültiges UTF 8 ist (`id` dann verlustbehaftet umgewandelt). Eine defekte Datei verhindert nie das Auflisten der übrigen und wird nie automatisch verändert. Sortiert wird nach den Bytes der ID.
- **AC-7**: Effektive Einstellungen entstehen Feld für Feld aus eingebauten Standards, darüber globalen Einstellungen, darüber den Instanz Einstellungen. Ein gesetztes Feld ersetzt den Wert darunter vollständig (auch `jvm_args`, `env` und jeder Hook); ein fehlendes Feld erbt. Beim Speichern prüft jede Ebene nur ihre eigenen Felder (`settings-invalid` mit Feldname): Zahlen größer 0, `memory_max_mb` mindestens 256, `memory_min_mb` nicht über `memory_max_mb`, wenn beide in derselben Ebene stehen, kein leerer `env` Schlüssel und keiner mit `=`, keine leere argv Liste bei Hooks. Die Prüfung über die Ebenen hinweg (`memory_min_mb` nicht über `memory_max_mb` im Ergebnis) liefert `effective_settings` als Fehler `settings-memory-range`; das Speichern globaler Einstellungen prüft keine Instanzen.
- **AC-8**: Eine Datei mit höherer `format_version` als bekannt wird abgelehnt (`storage-too-new` mit Pfad, gefundener und unterstützter Version) und bleibt Byte für Byte unverändert. Eine ältere wird beim Laden im Speicher durch die Migrationskette gehoben; beim ersten Schreiben danach entsteht `<datei>.v<alt>.bak`, wenn es noch nicht existiert (ein vorhandenes Backup wird nie überschrieben), dann wird die aktuelle Version geschrieben. Unbekannte Felder bei gleicher oder älterer Version werden beim Schreiben unverändert erhalten.
- **AC-9**: Jede JSON Datei wird über `.<datei>.tmp-<pid>-<zufall>` im selben Ordner geschrieben, mit `sync_all`, dann per `rename` an ihren Platz. Scheitert ein Schritt, wird die temporäre Datei gelöscht und die alte Datei bleibt unverändert gültig. Liegengebliebene `.*.tmp-*` Dateien werden beim Lesen ignoriert und von der nächsten schreibenden Operation im selben Ordner gelöscht, wenn sie älter als eine Stunde sind.
- **AC-10**: Jede Änderung an einer JSON Datei (frisch lesen, ändern, schreiben) läuft unter der exklusiven Datensperre `locks/data.lock`. Zwei Prozesse, die gleichzeitig dieselbe oder verschiedene Dateien ändern, verlieren keine Änderung. Ist die Sperre nach 5 Sekunden nicht frei, endet die Operation mit `storage-busy`. Reine Lesezugriffe nehmen keine Sperre.
- **AC-11**: Hält ein Prozess die Laufsperre einer Instanz (das Spiel läuft), werden `delete_instance` und ein zweiter Start derselben Instanz sofort (ohne Warten) mit `instance-running` abgelehnt; Einstellungen lassen sich weiter ändern.
- **AC-12**: `delete_instance` verschiebt den Instanzordner in den Papierkorb des Systems und entfernt danach den Freigabe Eintrag der Instanz. Scheitert der Papierkorb (häufig auf Wechseldatenträgern und headless Linux), endet es mit `instance-trash-failed`, nachdem geprüft wurde, ob der Ordner noch da ist; erst ein ausdrücklicher Aufruf mit `permanent = true` löscht endgültig, ohne je einem Symlink zu folgen. Der gemeinsame Cache wird in keinem Fall angefasst.
- **AC-13**: Die Felder, die Code ausführen können (`java`, `jvm_args`, `env`, `hooks`), gelten aus `settings.json` immer. Aus `instance.json` gelten sie nur, wenn der Freigabe Hash dieser vier Felder der Instanz in `trusted_overrides` steht. Ändert Exodus selbst eines dieser Felder einer Instanz, trägt es die Freigabe mit ein. Von außen geänderte, kopierte oder importierte Werte werden für alle vier Felder ignoriert (es gelten die globalen), die übrigen Felder der Instanz wirken weiter, und das Ergebnis meldet `untrusted_overrides: true`. Eine Instanz, die keines der vier Felder setzt, ist nie „nicht freigegeben“.

Teil B (gebaut mit Feature 5)
- **AC-14**: Konto Metadaten liegen in `accounts.json`; Tokens nie im Klartext auf der Platte. Alle Tokens aller Konten liegen in `accounts.secrets`, verschlüsselt mit ChaCha20 Poly1305 (ring), frische Zufallsnonce bei jedem Schreiben. Im installierten Modus ist der 32 Byte Schlüssel (als Base64 Text) der einzige Eintrag im Schlüsselbund des Systems, also unabhängig von der Zahl der Konten unter der Grenze des Windows Credential Managers.
- **AC-15**: Ist kein Schlüsselbund erreichbar (Linux ohne Secret Service) oder läuft Exodus im Portable Modus, fragt die erste Anmeldung, ob eine Passphrase gesetzt werden soll; der Schlüssel entsteht dann per Argon2id aus ihr, und sie wird nie gespeichert. Ohne Passphrase werden Tokens nur im Speicher gehalten, und der Aufrufer bekommt `account-not-persisted`.
- **AC-16**: Meldet der Schlüsselbund „Eintrag fehlt“, oder passt die Passphrase nicht, gelten alle Konten als abgemeldet, `accounts.json` bleibt erhalten, und `accounts.secrets` wird erst bei der nächsten erfolgreichen Anmeldung durch einen Umschlag mit neuem Schlüssel ersetzt (der dann nur dieses Konto enthält). Jeder andere Fehler des Schlüsselbunds (gesperrt, Zugriff abgelehnt) endet mit `account-keyring-error`; dabei wird nie ein neuer Schlüssel erzeugt und nichts überschrieben.

## Decision

**Chosen option**: Option 1: Mojang Layout Cache plus versionierte JSON Dateien pro Ressource, mit einer Datensperre und Laufsperren pro Instanz

Ein Datenordner mit gemeinsamem Cache im Mojang Layout, einem Ordner pro Instanz mit `instance.json` und `minecraft/`, getrennten Dateien für Einstellungen und Konten, jede versioniert, unbekannte Felder bewahrend, atomar geschrieben und unter einer gemeinsamen Datensperre geändert; Tokens verschlüsselt in einer Datei mit Schlüssel im Schlüsselbund oder aus einer Passphrase.

**Implementation skills**: `rust-engineer` (`jeffallan/claude-skills`, `.claude/skills/rust-engineer/`) · `rust-async-patterns` (`wshobson/agents`, `.claude/skills/rust-async-patterns/`)

## Feature design

**Datenordner Layout** (Wurzel `<data>` nach AC-1):

```
<data>/
  exodus.json                  { "layout_version": 1 }
  settings.json                GlobalSettings
  accounts.json                AccountsFile            (Teil B)
  accounts.secrets             SecretsEnvelope         (Teil B)
  logs/                        Diagnoselogs (Spec 0001)
  locks/
    data.lock                  Datensperre für jede JSON Änderung
    instances/<id>.running.lock  Laufsperre einer Instanz
  versions/<v>/<v>.json, <v>/<v>.jar
  libraries/<path aus downloads.artifact.path>
  assets/indexes/<index id>.json
  assets/objects/<erste 2 hex>/<sha1>
  runtimes/                    Innenaufbau: Spec 10
  instances/
    .creating-<id>-<zufall>/   nur während des Anlegens
    <id>/
      instance.json            Instance
      instance.json.v<n>.bak   nur nach einer Migration
      content.json             reserviert für installierte Inhalte, Phase 1 legt sie nicht an
      icon.<ext>               optional, nur wenn icon = {file}
      minecraft/               --gameDir: saves/, resourcepacks/, mods/, options.txt …
```

Im Instanzordner gehören alle Einträge außer `minecraft/` Exodus; neue Launcher Dateien kommen nur dorthin, nie nach `minecraft/`. Laufsperren liegen bewusst außerhalb der Instanz: unter Windows lässt sich ein Ordner mit einer offenen Datei nicht verschieben, und ein kopierter Instanzordner nimmt so keine alten Sperren mit. Sperrdateien werden nie gelöscht (ein gelöschtes und neu angelegtes Sperrfile wäre eine zweite, unabhängige Sperre). Sperrdateien werden nur gesperrt, nie gelesen oder beschrieben. Netzlaufwerke werden für den Datenordner nicht unterstützt, weil Dateisperren dort unzuverlässig sind.

**Slug Regeln** (`slugify`, rein):
1. Klein schreiben (`str::to_lowercase`), dann ä zu ae, ö zu oe, ü zu ue, ß zu ss, dann Unicode NFKD zerlegen und Akzentzeichen (kombinierende Zeichen) entfernen (é zu e).
2. Jedes Zeichen außer `a-z0-9` wird zu `-`; mehrere `-` zu einem zusammenfassen; `-` am Anfang und Ende entfernen.
3. Auf 48 Zeichen kürzen, danach erneut `-` am Ende entfernen. Ist das Ergebnis leer (etwa bei rein nicht lateinischer Schrift), lautet es `instance`.
4. Reservierte Windows Namen (`con`, `prn`, `aux`, `nul`, `com1` bis `com9`, `lpt1` bis `lpt9`) bekommen `-1` angehängt.
5. Kollision (Vergleich ohne Groß und Kleinschreibung mit allen vorhandenen Namen in `instances/`, auch `Broken`): Endung `-2`, `-3` …; die Endung zählt in die 48 Zeichen, der Stamm wird dafür gekürzt.

`validate_id(id)` gilt genau dann, wenn `slugify(id) == id` und `id` kein reservierter Name ist. Damit sind leere IDs, `.` am Anfang, `-` am Anfang, Großbuchstaben und Trennzeichen ausgeschlossen.

**Data model sketch** (JSON Felder in `snake_case`; „optional“ heißt: fehlt im JSON und wird beim Schreiben weggelassen; jedes Struct, das als Datei gespeichert wird, hat `#[serde(flatten)] extra: serde_json::Map` und schreibt unbekannte Felder unverändert zurück):

`Instance` (`instances/<id>/instance.json`, ID = Ordnername, nicht in der Datei)

| Feld | Typ | Pflicht | Bedeutung |
|---|---|---|---|
| `format_version` | u32 | ja | aktuell `1` |
| `name` | String | ja | Anzeigename, getrimmt, 1 bis 64 Zeichen |
| `minecraft_version` | String | ja | Mojang Versions ID, z. B. `1.21.4` (geprüft wird sie in Feature 6) |
| `loader` | `null` oder `{ "kind": String, "version": String }` | ja (`null` = Vanilla) | bekannte `kind` Werte: `fabric`, `quilt`, `forge`, `neoforge`; eine unbekannte Art lässt die Instanz laden, aber nicht starten (Spec 8 meldet das) |
| `settings` | GameSettings | ja (darf `{}` sein) | Instanz Überschreibungen |
| `icon` | `null` oder `{ "builtin": String }` oder `{ "file": String }` | ja | `file` ist ein reiner Dateiname im Instanzordner |
| `group` | String | optional | eine Gruppe, getrimmt, nicht leer |
| `account` | AccountId | optional | festes Konto; fehlt = globales aktives Konto |
| `created_at` | RFC 3339 UTC | ja | nach dem Anlegen unveränderlich |
| `last_played_at` | RFC 3339 UTC | optional | setzt Spec 8 beim Start |

`GameSettings` (eingebettet in `Instance.settings` und `GlobalSettings.game`; jedes Feld optional, fehlt = erben)

| Feld | Typ | Eingebauter Standard | Führt Code aus |
|---|---|---|---|
| `java` | `"auto"` oder `{ "path": String }` | `"auto"` (bis Feature 10 heißt das: `java` aus `PATH`) | ja |
| `memory_min_mb` | u32 | `512` | |
| `memory_max_mb` | u32 | `4096` | |
| `jvm_args` | [String] | `[]` | ja |
| `window` | `{ "width": u32?, "height": u32?, "fullscreen": bool? }`, jedes Unterfeld erbt einzeln | `854`, `480`, `false` | |
| `env` | { String: String } | `{}` | ja |
| `hooks` | `{ "pre_launch": [String]?, "post_exit": [String]?, "wrapper": [String]? }`, jedes Unterfeld erbt einzeln; Werte sind argv Listen, keine Shell | keine | ja |

`GlobalSettings` (`settings.json`): `format_version: u32` · `game: GameSettings` · `trusted_overrides: [{ "instance": InstanceId, "sha256": String }]` (höchstens ein Eintrag pro Instanz, `sha256` als 64 Zeichen Hex in Kleinbuchstaben). Fehlt die Datei, gilt sie als leer (alle Standards) und wird erst beim ersten Ändern angelegt.

**Freigabe Hash**: SHA 256 über `serde_json::to_vec` (ohne Leerraum) eines festen Structs `CodeOverrides { java, jvm_args, env, hooks }` in genau dieser Feldreihenfolge, mit den rohen Werten aus `instance.json` (nicht den effektiven), fehlende Felder weggelassen, `env` als `BTreeMap` (Schlüssel sortiert). Setzt die Instanz keines der vier Felder, gibt es keinen Hash und keine Prüfung.

`LayoutFile` (`exodus.json`): `layout_version: u32`, aktuell `1`. Steigt nur, wenn sich der Aufbau des Ordners selbst ändert.

`AccountsFile` (`accounts.json`, Teil B): `format_version` · `active: AccountId?` · `accounts: [Account]`.
`Account`: `id: AccountId` (32 Zeichen Hex aus 16 Zufallsbytes, `ring::rand::SystemRandom`) · `minecraft_uuid: String` (eindeutig in der Liste; eine zweite Anmeldung desselben Profils aktualisiert den Eintrag) · `profile_name: String` · `added_at` · `last_used_at?`.

`SecretsEnvelope` (`accounts.secrets`, Teil B, JSON): `format_version` · `key_source: "keyring" | "passphrase"` · `key_id: String` (32 Zeichen Hex aus 16 Zufallsbytes, auch bei Passphrase vorhanden) · `kdf: { "algorithm": "argon2id", "version": 19, "m_kib": 65536, "t": 3, "p": 1, "salt": base64 }` nur bei Passphrase (Salt 16 Byte, bei jedem Speichern neu, Ausgabe 32 Byte) · `nonce: base64` (12 Byte) · `ciphertext: base64`. Zusatzdaten der AEAD (mitgeprüft, nicht verschlüsselt): `format_version` als u32 Big Endian, direkt gefolgt von `key_id` als UTF 8. Klartext innen: `{ AccountId: Tokens }`; das Struct `Tokens` legt Spec 5 fest. Schlüsselbund Eintrag: Dienst `Exodus`, Konto `secrets-key-<key_id>`, Wert die 32 Schlüsselbytes als Base64 Text. Der Schlüssel liegt im Speicher in einem Typ, der ihn beim Drop überschreibt (`zeroize`).

Beziehungen: `Instance.account` N:1 `Account` (optional; zeigt es auf ein gelöschtes Konto, nimmt der Start das aktive Konto und warnt). `Instance.minecraft_version` N:1 Version im Cache, nur über den Namen, ohne Zähler. `Account` 1:1 Eintrag im Klartext von `accounts.secrets`. `trusted_overrides` N:1 `Instance` über die ID.

**State transitions**: Instanz `nicht vorhanden → wird angelegt (.creating-<id>-<zufall>/, unsichtbar beim Auflisten) → bereit (umbenannt nach <id>/) → läuft (Laufsperre gehalten) → bereit → im Papierkorb oder gelöscht`. Ein Absturz beim Anlegen hinterlässt nur einen `.creating-*` Ordner; das nächste `create_instance` löscht solche, die älter als eine Stunde sind.

Formatversion einer Datei: `älter → (Laden) im Speicher aktuell → (erstes Schreiben) Backup .v<alt>.bak, falls noch keins + aktuell auf der Platte`; `neuer → abgelehnt, unverändert`.

**Versionsregel**: `format_version` steigt nur bei einer brechenden Änderung (Feld umbenannt, entfernt, Typ oder Bedeutung geändert), immer mit Migrationsschritt. Ein neues optionales Feld braucht keinen Sprung, weil ältere Versionen es über `extra` erhalten.

**API surface** (Core, `exodus_core`; alle Methoden auf `Launcher` sind `async`, laufen komplett in einem `spawn_blocking` Block, halten keine Sperre über ein `await` und nehmen keinen `OpContext`, weil sie kurz sind):

| Funktion | Art | Key inputs | Key outputs | Key errors |
|---|---|---|---|---|
| `config::resolve_data_dir` | rein | `flag: Option<PathBuf>`, `env: Option<OsString>`, `portable_dir: Option<PathBuf>` (gesetzt, wenn `portable.txt` existiert), `cwd`, `default` | `(PathBuf, DataDirSource)` | keine |
| `config::locate_data_dir` | Rand | `flag: Option<PathBuf>` | `(PathBuf, DataDirSource)` | `ConfigError::NoHomeDir` |
| `storage::DataLayout` | rein | `root` | ein Pfad pro Eintrag im Layout | keine |
| `instance::slugify` / `instance::validate_id` | rein | Name und vorhandene Namen bzw. ID | ID | `InvalidId { reason }` |
| `settings::resolve` | rein | eingebaute Standards, `GameSettings` global, `GameSettings` Instanz, Code Felder freigegeben (bool) | `EffectiveSettings { …, untrusted_overrides }` | `MemoryRange` |
| `settings::validate_layer` | rein | `GameSettings` | `()` | `Invalid { field }` |
| `settings::code_overrides_hash` | rein | `GameSettings` der Instanz | `Option<String>` | keine |
| `storage::migrate` | rein | `serde_json::Value`, Kette `&[fn(Value) -> Result<Value, _>]`, Zielversion | `Value` | `TooNew`, `Migration` |
| `Launcher::create_instance` | Rand | `NewInstance { name, minecraft_version, loader, settings, icon, group, account }`, `id: Option<String>` | `(InstanceId, Instance)` | `instance-invalid-id`, `instance-invalid-name`, `instance-exists`, `settings-invalid`, `storage-*` |
| `Launcher::list_instances` | Rand | keine | `Vec<InstanceEntry>` (`Ok { id, instance }` oder `Broken { id, error: ErrorView }`) | `storage-io` nur wenn `instances/` selbst unlesbar ist |
| `Launcher::instance` | Rand | `id` | `Instance` | `instance-invalid-id`, `instance-not-found`, `storage-too-new`, `storage-json` |
| `Launcher::update_instance` | Rand | `id`, `FnOnce(Instance) -> Result<Instance, InstanceError>` | neue `Instance` | wie oben, `instance-invalid-name`, `settings-invalid`, `storage-busy` |
| `Launcher::delete_instance` | Rand | `id`, `permanent: bool` | `()` | `instance-not-found`, `instance-running`, `instance-trash-failed`, `storage-busy` |
| `Launcher::acquire_running` | Rand | `id` | `RunningGuard` (gibt die Sperre beim Drop frei) | `instance-running` |
| `Launcher::global_settings` / `update_global_settings` | Rand | `FnOnce(GlobalSettings) -> Result<GlobalSettings, SettingsError>` | `GlobalSettings` | `storage-*`, `settings-invalid` |
| `Launcher::effective_settings` | Rand | `id` | `EffectiveSettings` | wie `instance`, `settings-memory-range` |
| `Launcher::trust_instance_overrides` | Rand | `id` | `()` | `instance-not-found`, `storage-busy` |
| CLI: globales Flag `--data-dir <PATH>` | CLI | Pfad | wird an `locate_data_dir` gegeben | |
| CLI: `exodus info` | CLI | | zeigt zusätzlich die Quelle des Datenordners | |

`update_instance` übernimmt aus dem Ergebnis der Funktion alles außer `created_at` und `format_version` (beide setzt der Core). Ändert sich dabei der Freigabe Hash, schreibt es unter derselben Datensperre zuerst den neuen Eintrag in `settings.json`, dann `instance.json` (ein Absturz dazwischen lässt eine harmlose Freigabe für Werte zurück, die es noch nicht gibt). `trust_instance_overrides` ist für Oberflächen gedacht, die dem Nutzer vorher die vier Felder zeigen. Der Papierkorb läuft über einen kleinen Trait `Trash` (echte Umsetzung mit dem `trash` Crate, im Test ersetzbar). Teil B ergänzt mit Feature 5: `AccountStore` (lesen, hinzufügen, entfernen, aktiv setzen) und `SecretStore` (`load(passphrase: Option<&str>)`, `save`), Fehlerenum `AccountError`.

**Value sourcing**:

| Action | Value produced / displayed | Source |
|---|---|---|
| `locate_data_dir` | Datenordner und Quelle | Flag aus clap, `std::env::var_os("EXODUS_DATA_DIR")`, Ordner von `std::env::current_exe()` (nach Auflösen von Symlinks) plus Existenz von `portable.txt`, `std::env::current_dir()`, `default_data_dir()` |
| `create_instance` | ID | `slugify(name)` gegen die vorhandenen Namen in `instances/`, oder Aufrufer ID nach `validate_id` |
| `create_instance` | `created_at` | `jiff::Timestamp::now()` am Rand, an die reinen Bauschritte übergeben |
| `create_instance` | `minecraft_version`, `loader` | Eingabe des Aufrufers (Feature 6 prüft gegen das Manifest; Phase 1 immer `loader: null`) |
| `create_instance` | `settings`, `icon` | Eingabe, Standard `{}` und `null` (die Oberfläche zeigt dann ihr Standardicon); gesetzte Code Felder werden automatisch freigegeben |
| jede Ladeoperation | aktuelle `format_version` | Konstante pro Dateityp im Core (`INSTANCE_FORMAT`, `SETTINGS_FORMAT`, `ACCOUNTS_FORMAT`, `SECRETS_FORMAT`, `LAYOUT_VERSION`, alle `1`) |
| atomares Schreiben | Name der temporären Datei | `std::process::id()` plus 8 Zeichen Hex aus `SystemRandom` |
| `effective_settings` | jeder effektive Wert | eingebaute Standards (Konstanten im Core, Tabelle oben) < `settings.json` < `instance.json` |
| `effective_settings` | `untrusted_overrides` | `code_overrides_hash` der Instanz gegen ihren Eintrag in `trusted_overrides` |
| `delete_instance` | Papierkorb oder endgültig | `Trash` Trait; `permanent` vom Aufrufer (die CLI fragt in Feature 6) |
| Sperre | Wartezeit | Konstante `LOCK_TIMEOUT = 5 s`, Abfrage per `try_lock` alle 50 ms; Laufsperre nur ein `try_lock` |
| `exodus info` | Quelle des Datenordners | `DataDirSource`, Text aus `cli.ftl` |
| Teil B: Modus Schlüssel | Schlüsselbund oder Passphrase | `DataDirSource::Portable` oder Schlüsselbund nicht erreichbar ergibt Passphrase Frage; sonst Schlüsselbund; bei vorhandener Datei `key_source` |
| Teil B: Schlüssel | 32 Byte | `SystemRandom` beim ersten Speichern, danach Schlüsselbund; oder Argon2id aus Passphrase plus Salt aus der Datei |
| Teil B: Passphrase | Text | Parameter von `SecretStore::load`; woher die CLI sie holt, legt Spec 5 fest |

**Key invariants**:
- Eine Instanz ID ändert sich nach dem Anlegen nie; nur `name` und die übrigen Felder sind änderbar, `created_at` nie.
- Eine Datei mit zu neuer `format_version` wird nie geschrieben, migriert oder gelöscht.
- Unbekannte Felder werden nie verworfen; sie überleben jedes Lesen und Schreiben über `extra`.
- Keine JSON Datei wird je direkt überschrieben, immer temporäre Datei im selben Ordner plus `rename`. Unter Unix wird danach der Ordner per `sync_all` gesichert; unter Windows gibt es dafür keine Entsprechung, ein Stromausfall direkt nach dem Umbenennen kann dort die alte Version zurücklassen. Scheitert `rename` unter Windows mit Zugriff verweigert oder Freigabeverletzung (Virenscanner, Editor), bis zu 5 Versuche mit 100 ms Abstand. Gelesen wird nur über `std::fs` (öffnet unter Windows mit `FILE_SHARE_DELETE`), damit Leser das Umbenennen nicht blockieren.
- Jede Änderung liest die Datei innerhalb der Datensperre frisch, nie aus einem früheren Stand.
- Cache Dateien werden ebenfalls über temporäre Datei plus `rename` geschrieben und brauchen darum keine Sperre: laden zwei Prozesse dieselbe Datei, gewinnt die zweite identische Kopie. Geprüft wird jede gegen ihren SHA 1 aus den Mojang Metadaten (Ablauf in Spec 7).
- `icon.file` ist ein reiner Dateiname ohne Trennzeichen und ohne `..`; sonst gilt die Instanz als `Broken`.
- Löschen einer Instanz fasst nur `instances/<id>/` und ihren Eintrag in `trusted_overrides` an.
- Tokens erscheinen nie in einer Datei außer verschlüsselt in `accounts.secrets`, nie im Log und nie in einer `ErrorView`.

**Security model**: Lokale Einzelnutzer Daten, keine Rollen. Schutzziele:
1. Tokens sind auf der Platte nur verschlüsselt; der Schlüssel liegt nie neben der Datei. Das schützt vor kopierten Dateien, Cloud Backups und einem gestohlenen Datenordner, nicht vor Schadsoftware unter demselben Benutzer, die den Schlüsselbund ebenso lesen kann. Das ist dieselbe Grenze wie beim Schlüsselbund allein.
2. Fremde Instanzen (kopiert, importiert, aus Modpacks) können ohne Zustimmung keine Befehle ausführen, weil alle Felder, die Code starten (`java`, `jvm_args`, `env`, `hooks`), pro Instanz eine Freigabe brauchen, die an ihren genauen Inhalt gebunden ist.
3. Kein Pfad aus einer Datei darf aus seinem Ordner ausbrechen (`icon.file`, IDs ohne Trennzeichen, Symlinks in `instances/` gelten als `Broken`).

Unter Unix legt Exodus `accounts.json`, `accounts.secrets` und deren temporäre Dateien schon beim Anlegen mit Rechten `0600` an (nicht erst nach dem Umbenennen). Unter Windows gelten die geerbten Rechte des Benutzerprofils. Keine regulierten Daten außer dem Profilnamen und der UUID; kein Audit Log nötig.

**Configuration required**:
- `EXODUS_DATA_DIR`: überschreibt den Datenordner (leer = nicht gesetzt).
- `portable.txt` neben der Binary: schaltet den Portable Modus ein (Inhalt egal). Innerhalb eines macOS `.app` Bundles wird der Portable Modus nicht unterstützt.
- Neue Crates in `[workspace.dependencies]`, jeweils aktuelle Version, Features gegen genau diese Version prüfen: `jiff` (Feature `serde`), `trash`, `unicode-normalization`, `ring` direkt (dieselbe Hauptversion wie rustls, kein zweites ring), mit Feature 5 zusätzlich `keyring` (native Backends pro System, kein Klartext Backend), `argon2` und `zeroize`. Alle mit GPL 3.0 verträglicher Lizenz. Keine eigene Crate für Sperren: `std::fs::File::lock` und `try_lock` sind seit Rust 1.89 stabil.
- Fluent Schlüssel in `locales/en/storage.ftl`, `instance.ftl`, `settings.ftl` (Teil B: `account.ftl`): `storage-io`, `storage-json`, `storage-too-new`, `storage-layout-too-new`, `storage-busy`, `storage-migration`, `instance-not-found`, `instance-exists`, `instance-invalid-id`, `instance-invalid-name`, `instance-running`, `instance-trash-failed`, `settings-invalid`, `settings-memory-range`; Teil B: `account-not-persisted`, `account-keyring-error`, `account-passphrase-wrong`, `account-key-missing`; dazu die `cli.ftl` Texte für die Quelle des Datenordners. Jede Variante bekommt einen Eintrag in `error_samples()`.

**Critical test scenarios**:
- Happy path: Instanz „Survival 1.21“ anlegen, umbenennen, auflisten, effektive Einstellungen mit globalem RAM und eigenem Fenster auflösen, löschen; Ordner `survival-1-21` bleibt bis zum Löschen gleich, verifies **AC-4**, **AC-5**, **AC-6**, **AC-7**, **AC-12**
- Datenordner: Tabelle aus Flag (auch leer), Variable (auch leer), Marker und Standard gegen `resolve_data_dir`, relative Pfade; `exodus info --data-dir x` zeigt `flag` und legt nichts an, verifies **AC-1**, **AC-2**
- Slugs: Tabelle mit `Über Café`, `CON`, `日本語`, 60 Zeichen Namen, Kollision mit `Survival` (Großschreibung) und Kollision an der 48 Zeichen Grenze; `validate_id` lehnt `-x`, `.x`, `A`, `a--b` ab, verifies **AC-4**
- Versionierung: `instance.json` mit `format_version: 99` liefert `Broken` mit `storage-too-new` und ist danach Byte gleich; eine Testkette v0 zu v1 hebt eine Datei, legt beim ersten Schreiben `.v0.bak` an und lässt ein vorhandenes `.v0.bak` unverändert; ein unbekanntes Feld überlebt ein Update; `exodus.json` mit `layout_version: 2` blockiert Schreiben, nicht Lesen, verifies **AC-3**, **AC-8**
- Kaputte Daten: ungültiges JSON, fehlendes Pflichtfeld, Ordner ohne `instance.json`, Ordner `My Folder`, Datei `foo` in `instances/`, Symlink, `.creating-x-1` Ordner und `icon.file = "../x"` erscheinen korrekt, die übrigen Instanzen trotzdem, verifies **AC-6**
- Abbruch beim Schreiben: liegengebliebene `.instance.json.tmp-*` beeinflusst das Laden nicht und wird nach einer Stunde beim nächsten Schreiben entfernt; ein Fehler vor dem Umbenennen hinterlässt keine temporäre Datei, verifies **AC-9**
- Nebenläufigkeit: zwei Prozesse (Test Binary) erhöhen je 50 Mal ein Feld derselben Instanz, Ergebnis 100; eine gehaltene Datensperre führt nach 5 s zu `storage-busy`, verifies **AC-10**
- Laufsperre: mit gehaltenem `RunningGuard` schlagen Löschen und zweites `acquire_running` sofort mit `instance-running` fehl, `update_instance` gelingt, verifies **AC-11**
- Papierkorb: ein `Trash` Fehler liefert `instance-trash-failed` und der Ordner ist noch da; `permanent` löscht, ohne einem Symlink in `minecraft/` zu folgen; Dateien unter `versions/` und `libraries/` bleiben, verifies **AC-12**
- Freigabe: über die API gesetzte `jvm_args` gelten; dieselbe Instanz mit von Hand geändertem `env` liefert globale Werte für alle vier Code Felder, eigene Fenstergröße weiter und `untrusted_overrides: true`; nach `trust_instance_overrides` gelten sie; eine Instanz ohne Code Felder meldet `false`, verifies **AC-13**
- Einstellungen: `memory_max_mb 100` wird beim Speichern abgelehnt; `memory_min_mb 8192` global und `memory_max_mb 4096` in der Instanz speichern beide, `effective_settings` liefert `settings-memory-range`; leeres `jvm_args` in der Instanz ersetzt globale Argumente, verifies **AC-7**
- Teil B: Speichern und Laden mit Schlüsselbund; zwei Konten mit großen Dummy Tokens (je 4 KB) liegen hinter einem einzigen Schlüsselbund Eintrag; Portable Modus fragt nach Passphrase, ohne Passphrase `account-not-persisted`; falsche Passphrase und „Eintrag fehlt“ ergeben abgemeldete Konten mit erhaltener `accounts.json`; ein simulierter Fehler „Zugriff abgelehnt“ erzeugt keinen neuen Schlüssel und lässt `accounts.secrets` Byte gleich; die Datei enthält keinen der Token Texte, verifies **AC-14**, **AC-15**, **AC-16**

## Build plan

Tracer Bullet: zuerst ein dünner, echter Faden vom CLI Flag über den Datenordner bis zu einer geschriebenen und wieder gelesenen `instance.json`, danach jedes Segment verbreitern.

Meilenstein 1: Datenordner und dünner Faden
1. `default_data_dir()` auf `data_local_dir()` umstellen; reines `resolve_data_dir` mit `DataDirSource`, `locate_data_dir` am Rand; globales CLI Flag `--data-dir`; `exodus info` zeigt Quelle; Tests als Tabelle, satisfies **AC-1**, **AC-2**
2. Modul `storage`: `DataLayout` mit allen Pfaden des Layouts, atomares Schreiben (`write_json_atomic` mit Aufräumen der temporären Datei), Laden mit `format_version` Prüfung, `TooNew` und `extra` Feldern; `StorageError` mit Fluent Texten, satisfies **AC-3**, **AC-8**, **AC-9**
3. Modul `instance` dünn: Typ `Instance` v1 komplett (alle Felder, `loader: null`), `create_instance` mit einfachem Slug über `.creating-*` und Umbenennen, `instance(id)`; Integrationstest legt an und liest zurück, satisfies **AC-4**

Meilenstein 2: Instanzen vollständig und sicher
4. Datensperre über `File::lock` mit `LOCK_TIMEOUT`, `exodus.json` Prüfung in jeder schreibenden Operation, `update_instance` mit frischem Lesen unter Sperre und Existenzprüfung; Mehrprozess Test, satisfies **AC-3**, **AC-10**
5. Migrationskette (`storage::migrate`, reine Funktion über `Value`), Backup `.v<alt>.bak` ohne Überschreiben, Aufräumen alter temporärer und `.creating-*` Reste, getestet mit einer Testkette, satisfies **AC-8**, **AC-9**
6. Slug Regeln vollständig, `validate_id`, Namensprüfung und Umbenennen, `list_instances` mit allen `Broken` Fällen, Prüfung von `icon.file`, satisfies **AC-4**, **AC-5**, **AC-6**

Meilenstein 3: Einstellungen und Freigabe
7. Modul `settings`: `GameSettings`, `GlobalSettings`, eingebaute Standards, reines `resolve`, `validate_layer`, `update_global_settings`, `effective_settings` mit `settings-memory-range`, satisfies **AC-7**
8. Freigabe: `code_overrides_hash`, `trusted_overrides`, automatische Freigabe in `create_instance` und `update_instance` (Schreibreihenfolge `settings.json` vor `instance.json`), `trust_instance_overrides`, `untrusted_overrides` im Ergebnis, satisfies **AC-13**

Meilenstein 4: Löschen und Laufsperre
9. `acquire_running` mit `RunningGuard`; `Trash` Trait mit `trash` Umsetzung; `delete_instance` mit Prüfung nach Fehler, `permanent` ohne Symlinks zu folgen, Entfernen der Freigabe; Test, dass der Cache bleibt, satisfies **AC-11**, **AC-12**

Teil B (Feature 5, nicht in den Meilensteinen von Feature 4)
10. `accounts.json` mit `AccountStore`, Rechte `0600` ab der temporären Datei, satisfies **AC-14**
11. `accounts.secrets`: Umschlag, ring AEAD mit Zusatzdaten, Schlüssel als Base64 im Schlüsselbund unter `secrets-key-<key_id>`, `zeroize`, Unterscheidung „Eintrag fehlt“ gegen andere Fehler, satisfies **AC-14**, **AC-16**
12. Passphrase Modus über Argon2id für Portable Modus und fehlenden Schlüsselbund, sonst nur Sitzung mit `account-not-persisted`, satisfies **AC-15**, **AC-16**

## Consequences

**Positive**:
- Der Cache im Mojang Layout passt direkt zu den Startargumenten und spart pro Instanz mehrere hundert MB bis GB.
- Ordner pro Instanz mit `minecraft/` lassen sich kopieren, sichern und aus anderen Launchern 1:1 übernehmen; lesbare Slugs findet man im Dateimanager.
- `loader`, `account`, `content.json`, `format_version` und bewahrte unbekannte Felder sind von Anfang an da; Modloader, mehrere Konten, Mods und zwei Exodus Versionen nebeneinander kommen ohne brechende Migration.
- Atomares Schreiben und eine Datensperre machen CLI und Desktop App parallel sicher, ohne Daemon und ohne Fragen nach der Reihenfolge von Sperren.
- Ein einziger Schlüsselbund Eintrag umgeht die Größengrenze unter Windows für beliebig viele Konten.
- Kopierte und importierte Instanzen können keinen Code ohne Zustimmung starten.

**Negative / tradeoffs**:
- Ein Slug, der nach dem Umbenennen nicht mehr zum Namen passt („survival-1-21“ heißt jetzt „Hardcore“), wirkt im Dateimanager verwirrend. Bewusst in Kauf genommen für stabile Pfade.
- Eine Datensperre für alle JSON Dateien serialisiert jedes Schreiben über den ganzen Ordner. Bei Schreibzeiten im Millisekundenbereich ist das unkritisch, langsame Operationen dürfen die Sperre darum nie halten.
- Eigene Krypto Schicht (AEAD Umschlag, Schlüsselverwaltung) statt reinem Keyring: mehr Code, den Feature 5 sorgfältig testen muss.
- Ohne Secret Service oder im Portable Modus und ohne Passphrase müssen sich Nutzer bei jedem Start neu anmelden.
- Unter macOS kann der Schlüsselbund bei unsignierten Builds oder nach einem Update wiederholt nach Erlaubnis fragen; wird der Zugriff abgelehnt, bleibt Exodus abgemeldet, ohne Daten zu verlieren.
- Die Laufsperre hält der startende Prozess. Wird die CLI hart beendet, läuft das Spiel weiter, aber der Schutz fällt weg.
- Freigaben gelten pro Instanz für alle vier Code Felder zusammen; wer eine Instanz von Hand bearbeitet, muss sie danach erneut freigeben.
- `File::lock` ist unter Windows eine Pflichtsperre, unter Unix nur beratend; Programme außer Exodus (ein Texteditor) ignorieren sie. Atomares Schreiben schützt trotzdem vor halben Dateien. Netzlaufwerke werden nicht unterstützt.
- Auf Wechseldatenträgern und headless Linux gibt es oft keinen Papierkorb; dort ist Löschen immer der zweistufige Weg über `permanent`.
- Sperrdateien unter `locks/instances/` bleiben nach dem Löschen einer Instanz liegen (wenige Bytes), bis das Aufräumen kommt.
- Die Gerüst Version legte Logs unter Windows im Roaming Ordner ab; diese alten Logs bleiben dort liegen (nur Diagnose, keine Migration nötig).

**Neutral**:
- `exodus.json` ist ein zusätzlicher Marker im Datenordner, nötig, damit eine künftige Änderung des Layouts erkennbar ist.
- Die Hauptversion von ring muss mit der von rustls übereinstimmen, sonst landen zwei Kopien im Binary.
- Diese Spec übernimmt die Speicherform der Tokens aus dem Auftrag an Spec 5 (Spec 0001, Follow-up). Spec 5 legt nur noch Login Ablauf, Token Struct, CLI Abfrage der Passphrase und die keyring Features fest.

## Follow-up

- [ ] Scope Feature 5: vermerken, dass es Teil B dieses Specs (Build Schritte 10 bis 12, AC-14 bis AC-16) mitbaut.
- [ ] Spec 5: Struct `Tokens`, wie die CLI die Passphrase abfragt (Prompt, kein Umgebungswert), keyring Features pro System und die genaue Zuordnung der keyring Fehler zu „Eintrag fehlt“, „nicht erreichbar“ und „anderer Fehler“.
- [ ] Spec 6: Ablage des gecachten Versionsmanifests (Vorschlag `<data>/meta/`) und Prüfung von `minecraft_version` beim Anlegen; CLI Befehle `instance create/list/delete` mit Rückfrage für `--permanent`.
- [ ] Spec 7: Namensschema für Teil und temporäre Dateien beim Download im selben Ordner wie das Ziel, SHA 1 Prüfung; lange Pfade unter Windows (über 260 Zeichen, etwa tiefe Library Pfade) per `longPathAware` Manifest oder `\\?\` Präfix.
- [ ] Spec 8: hält `RunningGuard` für die Dauer des Spiels, setzt `last_played_at`, startet nur mit der effektiven Auflösung und meldet `untrusted_overrides` und eine unbekannte Loader Art; Ablage eigener Launcher Logs pro Instanz (dann unter `instances/<id>/`, nie in `minecraft/`); lange Pfade in `minecraft/` bei Mods.
- [ ] Spec 10: Innenaufbau von `runtimes/`; bis dahin bedeutet `java: "auto"` das `java` aus `PATH`.
- [ ] Spec 11: Ort für entpackte Natives.
- [ ] Feature 16: Liste der eingebauten Icons; Ansicht zum Freigeben der Code Felder einer Instanz.
- [ ] Feature 24: Import kopiert in `minecraft/`; importierte Code Felder bleiben ohne Freigabe.
- [ ] Vormerken, nicht Phase 1: Aufräumen des gemeinsamen Caches (nicht mehr referenzierte Versionen, Libraries, Assets, Runtimes), verwaister Sperrdateien und verwaister `trusted_overrides` Einträge (von Hand gelöschte Ordner), mit eigener Sperre `locks/cache.lock`, damit kein laufender Download gelöscht wird.
- [ ] `AGENTS.md` (`/sync`): unter `Declined:` die in dieser Runde gefundenen und abgelehnten Skills vermerken (`full-stack-skills/rust-skills@rust-web-security`, `@rust-concurrency`, `trailofbits/skills@constant-time-analysis`); eine Zeile in `crates/core/AGENTS.md` zu Speicherregeln (atomar schreiben, Datensperre, frisch lesen, `extra` Felder, Versionsregel) ergänzen, sobald gebaut.

## Rationale

Begründung und Optionen: siehe [rationale.md](rationale.md).
