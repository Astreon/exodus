# 0002. Datenablage im Mojang Layout mit versionierten JSON Dateien pro Instanz: Begründung

Entscheidungsprotokoll zu [index.md](index.md). `/develop` liest diese Datei nicht.

## Context

Spec 0001 hat die Leitplanken gesetzt: eine JSON Datei pro Instanz mit Versionsfeld, ein gemeinsamer Cache im Mojang Layout, der Datenordner über den `directories` Crate, überschreibbar und mit Portable Modus, Tokens im Schlüsselbund, nie im Klartext. Offen blieben der genaue Aufbau, das Instanzformat, die Versionierung, das Verhalten bei zwei gleichzeitig laufenden Prozessen und die Ablage der Konten.

Die Kräfte: Das Format ist der teuerste Teil zum späteren Umbauen, weil jede Instanz eines Nutzers davon abhängt. Es muss Modloader (Feature 13, 20), installierte Inhalte (Feature 14, 18, 23), mehrere Konten (Feature 17), Modpacks (22) und Import aus anderen Launchern (24) tragen, ohne dass eine Migration Daten verliert. Ohne Daemon können CLI und Desktop App gleichzeitig dieselben Dateien schreiben. Ein Absturz oder ein Stromausfall darf keine halbe JSON Datei hinterlassen. Der Windows Credential Manager erlaubt etwa 2,5 KB pro Eintrag, während MSA, Xbox und Minecraft Tokens zusammen und über mehrere Konten mehr ergeben können. Auf Linux ohne Secret Service (headless, minimale Fenstermanager) gibt es keinen Schlüsselbund, aber Klartext ist ausgeschlossen.

Dazu Nutzerfreundlichkeit: Einsteiger sollen ihre Welten im Dateimanager finden, Power User wollen Einstellungen global setzen und pro Instanz gezielt abweichen, und eine Umbenennung darf keinen Pfad brechen. Als Solo Projekt zählt, dass das Ganze ohne Datenbank oder Hintergrunddienst auskommt und sich von Hand reparieren lässt.

## Options considered

### Option 1: Versionierte JSON Dateien pro Ressource, Mojang Layout Cache, Dateisperren

Ein Datenordner; Cache wie im offiziellen Launcher; je Instanz ein Ordner mit `instance.json` und `minecraft/`; `settings.json`, `accounts.json` und verschlüsselte `accounts.secrets` getrennt; atomares Schreiben und Dateisperren (eine Datensperre, Laufsperren pro Instanz).

**Pros**:
- Jede Datei lesbar, von Hand reparierbar, Instanzen kopierbar.
- CLI und Desktop laufen parallel; ein laufendes Spiel sperrt nur seine eigene Instanz.
- Eine defekte Datei betrifft nur ihre Ressource.

**Cons**:
- Atomares Schreiben, Sperren und Migrationen sind eigener Code, den jede Datei nutzen muss.
- Auflisten liest jede `instance.json` einzeln (bei Hunderten Instanzen trotzdem schnell, aber kein Index).

### Option 2: SQLite für Metadaten, Mojang Layout für den Cache

Instanzen, Einstellungen und Konten liegen in einer SQLite Datei; Spielordner weiter pro Instanz.

**Pros**:
- Transaktionen und Nebenläufigkeit kommen fertig mit; Migrationen über ein bewährtes Werkzeug.
- Abfragen und Sortieren über viele Instanzen ohne eigenen Code.

**Cons**:
- Eine Instanz ist nicht mehr selbsttragend kopierbar; Import, Export und Reparatur brauchen Werkzeuge.
- Eine beschädigte Datenbank betrifft alle Instanzen auf einmal.
- Widerspricht der Leitplanke „JSON pro Instanz“ aus Spec 0001.

### Option 3: Vollständig eigenständige Instanzen (eigene Libraries und Assets pro Instanz)

Jede Instanz enthält alles, was sie zum Start braucht.

**Pros**:
- Keine geteilten Daten, also keine Sperren auf dem Cache und kein Aufräumen.
- Eine Instanz ist ein vollständiges Paket.

**Cons**:
- Mehrere hundert MB bis GB doppelt pro Instanz, Assets allein liegen bei mehreren hundert MB.
- Widerspricht der Leitplanke „gemeinsamer Cache“ aus Spec 0001.

### Option 4: Eine zentrale Datei für alle Instanzen

Eine `instances.json` mit allen Instanzen, Spielordner getrennt.

**Pros**:
- Ein Lesezugriff zum Auflisten, ein einziger Ort für Sortierung und Gruppen.

**Cons**:
- Jede Änderung an irgendeiner Instanz sperrt alle; eine defekte Datei legt alle lahm.
- Ein Instanzordner allein ist nicht mehr kopier oder importierbar.

## Rationale

Option 1 erfüllt die Leitplanken aus Spec 0001 und hält den Schaden jeder Störung lokal: eine kaputte oder zu neue Datei betrifft genau eine Ressource und wird als `Broken` gemeldet statt alles zu blockieren. Die zusätzlichen Bausteine (atomares Schreiben, Sperren, Migrationskette) sind klein, einmal im Modul `storage` gebaut und von jeder Datei geteilt. `File::lock` ist seit Rust 1.89 in std, also kommt dafür keine Abhängigkeit dazu. SQLite (Option 2) wäre der stärkste Gegenkandidat, gibt aber die Kopierbarkeit einer Instanz auf, die Import (Feature 24) und Modpacks (Feature 22) direkt nutzen.

Die Unterentscheidungen im Gespräch, jeweils mit dem Zweitplatzierten:

- **Instanz ID = eingefrorener Slug als Ordnername**, ID nicht in der Datei. Lesbar im Dateimanager und stabil beim Umbenennen; ein kopierter Ordner kann keine falsche ID tragen. Zweitplatziert: zufällige ULID als Ordner (garantiert eindeutig, aber unlesbar).
- **Spielordner `minecraft/`** statt Spieldateien direkt im Instanzordner: Mods oder das Spiel können keine Launcher Dateien überschreiben, Import kopiert 1:1 hinein. Zweitplatziert: `.minecraft/` (vertraut, aber versteckt).
- **Sperrdateien unter `locks/`** statt im Instanzordner: unter Windows verhindert eine offene Datei das Verschieben eines Ordners in den Papierkorb, und kopierte Ordner nehmen keine Sperren mit. Diese Änderung kam beim Schreiben nach der Bestätigung des Datenmodells hinzu.
- **Eine Datensperre statt Sperren pro Ressource** (nach der Gegenprüfung geändert): `update_instance` schreibt bei einer neuen Freigabe zwei Dateien, `delete_instance` ebenso; mit Sperren pro Ressource bräuchte jede Operation eine feste Reihenfolge, sonst droht ein Deadlock. Schreibvorgänge dauern Millisekunden, also kostet die gemeinsame Sperre praktisch nichts. Die Laufsperren pro Instanz bleiben getrennt, weil sie für die Dauer eines Spiels gehalten werden. Zweitplatziert: Sperren pro Ressource mit fester Reihenfolge `settings` vor Instanz.
- **Anlegen über `.creating-*` und Umbenennen** (beim Einarbeiten der Gegenprüfung entschieden): ein halb angelegter Ordner ist beim Auflisten nie sichtbar, und ein Absturz hinterlässt nur einen versteckten Rest, den das nächste Anlegen wegräumt. Zweitplatziert: Ordner per `create_dir` reservieren und halb angelegte als `Broken` oder „wird angelegt“ zeigen.
- **Standardordner `data_local_dir`**: unter Windows liegt `data_dir` in Roaming, das in Domänen Netzen synchronisiert wird, für GB große Caches ungeeignet. Alles bleibt in einem Ordner, damit Portable Modus und Sicherung eine Wurzel haben. Zweitplatziert: Cache in den Cache Ordner des Systems (saubere XDG Trennung, aber Systemreiniger löschen ihn gern).
- **Override Reihenfolge Flag, Variable, `portable.txt`, Standard**: explizit und für Tests bequem. Eine Markerdatei statt eines vorhandenen `data/` Ordners, damit der Modus nie zufällig umschaltet.
- **Migration im Speicher, Backup beim ersten Schreiben**: reines Auflisten schreibt nie, ein Downgrade bleibt möglich, solange nichts gespeichert wurde, und ein Fehler in einer Migration ist über das Backup umkehrbar. Zweitplatziert: sofort zurückschreiben (alle Dateien schnell aktuell, aber Schreibzugriffe beim Lesen).
- **`format_version` steigt nur bei brechenden Änderungen, unbekannte Felder werden bewahrt** (nach der Gegenprüfung geändert): der erste Entwurf erhöhte die Version bei jeder Änderung. Das hätte eine ältere CLI neben einer neueren Desktop App jede Instanz als `Broken` sehen lassen, sobald die neuere sie einmal gespeichert hat. Mit `#[serde(flatten)] extra` schreibt eine ältere Version neue Felder unverändert zurück. Eine höhere Version wird weiter abgelehnt, wie gefordert. Zweitplatziert: Sprung bei jeder Änderung (strenger, aber unverträglich mit zwei Versionen nebeneinander).
- **Vererbung „gesetzt ersetzt“** für alle Felder, auch `jvm_args` und `env`: eine einzige Regel, die sich in der Oberfläche als „erbt von global“ oder „eigener Wert“ zeigen lässt. Zweitplatziert: Anhängen bei JVM Argumenten (bequem, aber ein globales Flag ließe sich pro Instanz nicht mehr entfernen).
- **Alle vier Einstellungsgruppen ab v1** (Java, Fenster, Umgebungsvariablen, Hooks): kostet jetzt nur Felder und erspart Formatsprünge in Phase 3.
- **Hooks als argv Listen, keine Shell** (Entscheidung beim Schreiben): plattformgleich und ohne Fallen beim Quoting. Zweitplatziert: Shell Befehlszeile (vertrauter, aber je System anders interpretiert).
- **Code Felder pro Instanz nur mit Freigabe über SHA 256**: Modpacks und Importe bringen sonst beliebige Befehle auf den Rechner. Die Gegenprüfung zeigte, dass Hooks nicht der einzige Weg sind: `java.path` (beliebige Datei), `jvm_args` (`-javaagent`, `-XX:OnError`) und `env` (`LD_PRELOAD`, `JAVA_TOOL_OPTIONS`) starten ebenso Code. Darum deckt die Freigabe alle vier Felder zusammen ab; RAM, Fenster und übrige Felder wirken ohne Freigabe. Zweitplatziert: nur Hooks freigeben und die Grenze dokumentieren (weniger Reibung, aber das Schutzziel wäre falsch).
- **Icon als eingebauter Name oder Datei im Instanzordner**, eine Gruppe als Text, Zeiten als RFC 3339 über `jiff`, optionales Konto pro Instanz: jeweils das kleinste Format, das Phase 3 und Import ohne Formatsprung tragen.
- **Defekte Dateien als `Broken` listen**: Nutzer sehen, was fehlt und warum. Zweitplatziert: still überspringen (ruhig, aber Instanzen verschwinden scheinbar).
- **Löschen in den Papierkorb, Fallback nur ausdrücklich endgültig**: Welten sind sonst unwiederbringlich. Zweitplatziert: endgültig nach Rückfrage.
- **Laufsperre, nur Löschen und Zweitstart verweigert**: schützt Welten, ohne Einstellungen zu blockieren.
- **Tokens verschlüsselt in einer Datei, Schlüssel als einziger Schlüsselbund Eintrag**: umgeht die Grenze unter Windows sicher für beliebig viele Konten und hält das Minecraft Token gecacht; der Minecraft Login ist ratenbegrenzt, darum ist „nur Refresh Token speichern und bei jedem Start die ganze Kette laufen lassen“ schlechter. Zweitplatziert: nur Refresh Token im Schlüsselbund (weniger Code, aber mehr Netzaufrufe und keine Garantie für die Größe des Refresh Tokens).
- **ring AEAD (ChaCha20 Poly1305)**: ring ist schon als rustls Backend im Baum, also kein zweiter Krypto Stack. Bei wenigen Schreibvorgängen ist eine zufällige 96 Bit Nonce unkritisch. `key_id` und `format_version` laufen als Zusatzdaten mit, damit ein vertauschter Umschlag nicht entschlüsselt. Zweitplatziert: RustCrypto `chacha20poly1305` (bequemere API, aber zweiter Stack).
- **Linux ohne Secret Service: nur Sitzung, Passphrase auf Wunsch** (Argon2id, 64 MiB, 3 Durchläufe): nie Klartext, headless nutzbar. Zweitplatziert: Kernel Keyring (übersteht keinen Neustart).
- **Portable Modus nutzt Passphrase oder nur die Sitzung** (nach der Gegenprüfung entschieden): der Schlüssel im Schlüsselbund bliebe auf dem ersten Rechner. Auf jedem anderen Rechner wäre man abgemeldet, und eine neue Anmeldung dort würde den ersten abmelden. Mit Passphrase reist der Schlüssel mit dem Stick. Zweitplatziert: Schlüsselbund auch portabel (bequem an einem Rechner, Pingpong zwischen mehreren).
- **Nur „Eintrag fehlt“ gilt als verlorener Schlüssel** (nach der Gegenprüfung): ein gesperrter Schlüsselbund oder eine abgelehnte Abfrage unter macOS darf nie einen neuen Schlüssel erzeugen und damit alle Sitzungen zerstören.
- **Bauumfang**: Feature 4 baut Fundament, Instanzen und Einstellungen; Konten und Secrets entstehen mit Feature 5, wo echte Tokens die Krypto realistisch testen. So bleibt der Tracer Bullet Faden dünn, ohne dass sich das Format später ändert.
- **Speicheroperationen ohne `OpContext`** (Entscheidung beim Schreiben): sie dauern Millisekunden und sind nicht sinnvoll abbrechbar; der Standard in `crates/core/AGENTS.md` gilt für lange Operationen. Einzig endgültiges Löschen großer Ordner kann Sekunden dauern; das ist hinnehmbar.

Gesuchte, aber abgelehnte Agent Skills: für keyring, ring, argon2, jiff und trash gibt es keine spezifischen Skills; die gefundenen allgemeinen Treffer (`full-stack-skills/rust-skills@rust-web-security`, `@rust-concurrency`, `trailofbits/skills@constant-time-analysis`) wurden abgelehnt, weil `rust-engineer` und der docsrs MCP die Crates abdecken.
