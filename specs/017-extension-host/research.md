# Research: Erweiterungs-Host für haextensions

Phase 0 von [plan.md](./plan.md). Jede Entscheidung nennt Begründung und verworfene Alternativen.
Quellen (alle gepinnt):

- haex-vault 2.x (Legacy) @ `8dce379d94e18fcd42c3b73686a06f984ca3f574` (kurz **HV**). Vorlage für Verhalten; Prüfcode in
  Rust wird übernommen, wo er trägt, und dabei um die gefundenen Lücken korrigiert (R6). haex-vault 2.x selbst wird
  nicht geändert (Spec, Clarifications 2026-10-02).
- vault-sdk @ `502593e84b8d289b0986a2777754d6bd8f52da5e` (v3.7.0, kurz **SDK**).
- haextension @ `db48f9a948522c18a00331aac232718825cc9317`.
- haex-crdt @ `aeb26ebb67b3faba5e8c9b34c84d7291e44107a4` (Pin in `src-tauri/Cargo.toml:147`), tauri 2.12.1,
  wry 0.57.0, zip 7.2.0 (im `Cargo.lock`).

Wo „(vermutet)“ steht, ist das Verhalten nicht im Code nachgelesen, sondern abgeleitet; diese Stellen haben
eine Prüfaufgabe in tasks.md.

## R1 — Lieferungen und Reihenfolge

**Entscheidung**: Die Spec wird in sechs Lieferungen umgesetzt, jede ein eigener PR auf `main`:

| Lieferung | Inhalt                                                                                                                                                | Stories             | Voraussetzung                           |
| --------- | ----------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------- | --------------------------------------- |
| **L0**    | Vorarbeit in anderen Repositories: haex-crdt (R6, R8, R9), vault-sdk (R3, R13), wry für Android und Windows (R12, R25)                                | –                   | –                                       |
| **L1**    | Registry, Installation, Signatur v2, Rahmen und Brücke, SQL auf eigene Tabellen, Migrationen, Berechtigungen mit Anfrage und Einstellungen; ein Gerät | US1, US2, US3       | L0 gepinnt                              |
| **L2**    | Mehrere Geräte: Bundles als Vault-Daten, geparkte Sync-Gruppen, Lebenszyklus je Gerät                                                                 | US4                 | L1, 024 im Einsatz                      |
| **L3**    | Meldungen, Kontext, Schlüssel-Wert-Speicher, Protokolle, Tabellen anderer Erweiterungen, Update/Deaktivieren/Entfernen, Entwicklermodus               | US5, US6, US7, US12 | L1 (L2 für die Wirkung auf alle Geräte) |
| **L4**    | Netzwerk, Benachrichtigungen, Dateisystem                                                                                                             | US8, US9            | L1                                      |
| **L5**    | Passwörter (034 ist gemerged, PR #222), entfernter Speicher (nach 038), Mail, Shell                                                                   | US10, US11          | L1, 038                                 |

Jede Lieferung bringt ihre Fälle der Umgehungssammlung (SC-002) mit und gilt erst mit ihnen als fertig.

**Begründung**: Die P1-Stories ergeben erst zusammen etwas Nutzbares (eine Erweiterung ohne SQL oder ohne
Berechtigungen ist nicht prüfbar). Mehrere Geräte hängen an einer Änderung im Sync-Kern (R10) und werden
getrennt geprüft. L4/L5 sind unabhängig voneinander und warten teilweise auf andere Specs.

**Alternativen**: eine Lieferung (zu groß für ein Review); je Story eine Lieferung (US1 ohne US2/US3 ist
nicht testbar).

## R2 — Bundle-Format v2 und Signatur

**Entscheidung**: Ein Bundle (`.xt`, Zip) enthält die App-Dateien an der Wurzel, die Migrationen unter
`migrationsDir` und zwei Steuerdateien:

- `haextension/manifest.json` — ist **byte-genau** die kanonische JSON-Form nach RFC 8785 (JCS) von sich
  selbst; holzi kanonisiert nicht neu, sondern lehnt nicht kanonische Bytes ab. Kein Feld `signature`.
  Eingeschränktes JSON: nur ASCII-Schlüssel, nur ganze Zahlen im Bereich ±2^53, keine Gleitkommazahlen; damit
  ist die Kanonisierung auf beiden Seiten klein (rund 60 Zeilen) und ohne neue Abhängigkeit.
  (`serde_json` läuft in holzi mit `preserve_order`, `Cargo.toml:69`, sortiert also nicht.)
- `haextension/signature.json` (ebenfalls JCS):
  `{"files":[{"path","sha256","size"}…],"format":"haextension-bundle/2","publicKey","signature"}`. `files`
  listet jeden Eintrag des Archivs außer `signature.json`, nach UTF-8-Bytes sortiert.
- Signiert wird `"haextension-bundle/2\n"` + JCS von `signature.json` ohne `signature`; Ed25519, Prüfung mit
  `verify_strict`, schwache Schlüssel abgelehnt.

Prüfreihenfolge bei der Installation: Archiv nach R3 lesen → für jeden Eintrag Pfad, Größe und SHA-256
berechnen → genau gleich zur Liste `files` → `manifest.publicKey == signature.publicKey` → Signatur prüfen.
Ein Bundle mit `signature`-Feld im Manifest und ohne `signature.json` ist das alte Format: Meldung „mit dem
aktuellen Werkzeug `haex` neu signieren“ (FR-002).

**Begründung**: Die Liste bindet Pfad, Länge und Inhalt je Datei; das Manifest ist eine gelistete Datei.
Damit ist das Verschieben von Inhalt zwischen Dateien, Umbenennen, Ergänzen und Entfernen erkennbar (die Lücke
in HV `src-tauri/src/extension/crypto.rs:43-136`, das nur Inhalte aneinanderhängt). Eine eigene
Signaturdatei erspart das „Feld leeren und neu serialisieren“, das in HV von gleicher Ausgabe von JS und
serde abhängt. Die Fehlermeldung kann die geänderte Datei nennen.

**Umsetzung im Werkzeug**: haex-space/vault-sdk `c8588aadc7bac13f628cfb9c93e269edd5f7a81b` (#52), Signieren und
Prüfen in `src/bundle/` (`sign.ts`, `verify.ts`, `jcs.ts`, `zip.ts`), Testvektoren unter `test-vectors/bundles/`,
kopiert nach `src-tauri/tests/fixtures/extension_bundles/` (`SOURCE.md`). Seit haex-space/vault-sdk#54 (Release
4.0.0, `36bf6e98f36c2362d42aa2d92c85288a3d91e775`) gibt es eine einzige Umsetzung, das Rust-Crate
`crates/haex-bundle`: holzi bindet es per Git-SHA ein, `haex` nutzt seinen WebAssembly-Build; 46 Vektoren.

**Alternativen**: Signatur als Manifest-Feld (wieder das Leeren); Merkle-Wurzel (keine lesbaren Fehler);
abgesetzte `.sig`-Datei (geht beim Kopieren verloren); HV-Format (Spec-Entscheidung dagegen).

## R3 — Archiv lesen: Regeln und Grenzen

**Entscheidung**: Das Bundle-Format hat genau eine Umsetzung, das Rust-Crate `haex-bundle` im vault-sdk
(`crates/haex-bundle`, haex-space/vault-sdk#54). holzi bindet es per Git-Revision ein, das Werkzeug `haex` nutzt
seinen WebAssembly-Build. Es liest das Archiv mit einem eigenen Leser (nicht dem `zip`-Crate) in den Speicher,
ruft nie `extract` auf und prüft auf den rohen Namensbytes:

- gültiges UTF-8 in NFC (sonst ablehnen, nie stillschweigend normalisieren); nur `/`; kein führendes `/`, kein
  `\`, `:`, NUL oder Steuerzeichen; keine leeren, `.`- oder `..`-Teile; Teil ≤ 255 Bytes, Pfad ≤ 1024 Bytes;
- keine zwei Pfade, die nach NFC und Kleinschreibung gleich sind;
- nur reguläre Dateien (keine Symlinks, keine Verzeichniseinträge mit Daten, nichts Verschlüsseltes), nur
  `Stored` oder `Deflate`;
- **doppelte Namen**: das zentrale Verzeichnis wird roh gelesen, jeder Name einzeln geprüft (das `zip`-Crate
  fasst sie still zusammen, `zip-7.2.0/src/read.rs:71-74`);
- lokaler Kopf gleich dem Eintrag im zentralen Verzeichnis (Name, Methode, Verschlüsselung, CRC-32, Größen);
- Grenzen: `.xt` ≤ 64 MiB vor dem Öffnen; je Datei ≤ 25 MiB, gesamt ≤ 64 MiB entpackt, ≤ 2.000 Einträge,
  Verhältnis ≤ 200:1 je Eintrag; jeder Eintrag scheitert, sobald er mehr Bytes liefert als angegeben.

**Begründung**: Zip-Bomben, Pfad-Tricks und doppelte Einträge sind die üblichen Angriffe auf Installer; HV
ruft `archive.extract` ohne Grenzen auf (HV `installer.rs:84-90`). 25 MiB je Datei entspricht der bewährten
Grenze für Anhänge in 034 (R4 dort).

Ein Crate statt je einer Umsetzung in TypeScript (Werkzeug) und Rust (holzi): sonst müsste jede Regeländerung
an zwei Stellen nachgezogen werden (Entscheidung des Betreibers, 2026-10-03). Die Testvektoren erzeugt weiter
ein unabhängiges Node.js-Skript; sie prüfen das Crate nativ und über WebAssembly.

**Alternativen**: `zip`-Crate 7 oder 8 wie HV (Duplikate unsichtbar, kein Vergleich der lokalen Köpfe, eigene
Fehlerabbildung); zwei Umsetzungen mit gemeinsamen Testvektoren (doppelte Pflege); Entpacken in ein
Verzeichnis (unnötige Kopie im Klartext, siehe R4).

## R4 — Bundles als Vault-Daten und Ausliefern aus der Datenbank

**Entscheidung**: Je Datei ein inhaltsadressierter BLOB in `extension_blobs` (Schlüssel = SHA-256), jeder in
einem eigenen `write`; Dateiliste und Bundle-Zeile werden zuletzt geschrieben. Ausgeliefert wird direkt aus
der Datenbank, ohne entpackten Zwischenspeicher:

- bei jedem Start einer Erweiterung (FR-003): `signature.json` gegen die gespeicherten Bytes prüfen, dann jeden
  gelisteten BLOB lesen und seinen Hash prüfen;
- bei jeder Anfrage an das Protokoll: BLOB über den erwarteten Hash lesen und erneut prüfen (fängt einen
  BLOB, den ein anderes Gerät unter demselben Schlüssel mit anderem Inhalt geschrieben hat).

Verwaiste BLOBs räumt das Öffnen der Vault mit derselben Karenzzeit wie 034 (sieben Tage) auf.

**Begründung**: Jede Schreibgruppe bleibt weit unter der Grenze von 100 MiB je Transaktion
(`src-tauri/src/instances/vault_config.rs:36`); eine zu große Gruppe nimmt den direkten Weg (Spec 026). Gleiche
Dateien verschiedener Fassungen liegen nur einmal vor. Die Datenbank ist verschlüsselt; ein entpacktes
Verzeichnis könnte jeder lokale Prozess ändern und hinterließe Klartext (Spuren, Spec 013). Kosten je Start
grob 50 ms für 20 MiB (vermutet).

**Alternativen**: ein BLOB je Archiv (keine Deduplizierung, eine Zelle verdoppelt sich im Sync auf rund
48 MiB Grenze); eigene Chunk-Tabelle (in 034 aus denselben Gründen verworfen); entpacktes Verzeichnis wie HV
(prüft nur bei der Installation).

## R5 — Kennungen ohne UNIQUE-Constraints

**Entscheidung**: Alle synchronisierten Registry-Tabellen haben abgeleitete Kennungen (UUIDv5, ein Namensraum
je Tabelle) statt UNIQUE-Indizes: Erweiterung aus `publicKey ":" name`, Bundle aus dem SHA-256 der signierten
Nachricht (Zeilen sind dadurch unveränderlich), Migration aus `extId ":" name ":" sha256(sql)`, Berechtigung
aus `extId | Art | Aktion | Ziel | Geltungsbereich`. Details in [data-model.md](./data-model.md).

**Begründung**: Ein UNIQUE-Konflikt hält den Sync an (034 research R2); HV hat genau solche Indizes
(`core.ts:65-67, 246, 342`). Ein inhaltsabgeleitetes Bundle-Kennzeichen verhindert, dass Last-Writer-Wins je
Spalte zwei Bundles mischt.

**Alternativen**: Zufalls-UUIDs plus UNIQUE (hält den Sync an); Zufalls-UUIDs ohne Eindeutigkeit (Dubletten
bei gleichzeitiger Installation).

## R6 — SQL-Prüfung: zwei Schichten, der SQLite-Authorizer entscheidet

**Entscheidung**:

1. **Vorprüfung in Rust, portiert aus haex-vault 2.x** (`src-tauri/src/extensions/sql/`): Grundlage ist die
   vorhandene Prüfung von HV, rund 900 Zeilen und 1.180 Zeilen Tests — `database/core/parsing.rs` (eine
   Anweisung), `extension/database/planner.rs` (Art der Anweisung, Platzhalter), `database/core/extract.rs`
   (Tabellen aus dem AST), `extension/permissions/validator.rs` (Lesen oder Schreiben je Tabelle),
   `extension/permissions/checker.rs` und `manager/check/database.rs` (Abgleich mit Berechtigungen),
   `extension/database/helpers.rs` (Regeln für Migrationen, PRAGMA-Erlaubtliste) und die Tests unter
   `extension/database/tests/` (darunter `sql_injection_tests/`). Beim Portieren werden die Lücken geschlossen:
   Tabellen über `sqlparser::ast::visit_relations` statt der Handauswahl von `extract.rs:142-180` (erfasst auch
   `WITH`, `EXISTS`, Unterabfragen in jeder Klausel, `JOIN … ON`, `CASE`, Funktionsargumente); Namen aus `WITH`
   der Anweisung abgezogen, ein `WITH`-Name gleich einem echten Tabellennamen abgelehnt; nur Qualifizierer
   `main` oder keiner (HV prüft `main.haex_x` nicht, `checker.rs:224-226`); exakte Zerlegung des Präfixes statt
   `starts_with` (`utils.rs:202-210`); Erlaubtliste statt Sperrliste `haex_*`/`sqlite_*`; vorläufige
   Berechtigungen durchlaufen dieselbe Regel (HV `check/database.rs:66-74`); jede Migrationsanweisung durchläuft
   die volle Prüfung (HV prüft dort nur DDL, `helpers.rs:69-71, 176-189`); Funktionen über eine Erlaubtliste;
   genau eine Anweisung (haex-crdts `parse_single_statement` verwirft nachfolgende still, `parsing.rs:10-27`).
   Zweck: Anweisungen früh und mit klarer Meldung ablehnen und die nötigen Berechtigungen **vor** der Ausführung
   kennen, damit holzi fragen kann, ohne die Verbindung zu sperren.
2. **SQLite-Authorizer** (die eigentliche Durchsetzung), von haex-crdt nur um Vorbereiten und Ausführen der
   Anweisung der Erweiterung gesetzt:
   - oberste Ebene: `Read`/`Insert`/`Update`/`Delete` nur bei `database_name == "main"` und eigener oder
     freigegebener Tabelle (Präfix exakt zerlegt, ohne Rücksicht auf Groß-/Kleinschreibung); `Function` nur aus
     einer Erlaubtliste (Kern-, JSON-, Datums-, Mathefunktionen; nie `load_extension`, `fts3_tokenizer`,
     `sqlite_compileoption_*`, `sqlcipher_*`, unbekannte UDFs); alles andere (Pragma, Attach, Transaktion,
     Savepoint, Create/Drop/Alter, virtuelle Tabellen, Analyze, Reindex) abgelehnt;
   - in Triggern: nur, wenn der Auslöser `z_dirty_<T>_(insert|update|delete)` heißt und T eine Tabelle ist, die
     diese Anweisung schreiben darf (Erweiterungen legen nie Trigger an, also sind die Namen nicht fälschbar);
   - lehnt der Authorizer eine Tabelle ab, die die Vorprüfung nicht kannte, ist das „Form nicht zuordenbar“
     (FR-026), nie ein Durchlassen.
3. Grundregel beider Schichten: erlaubt sind nur eigene und freigegebene Tabellen von Erweiterungen; es gibt
   keine Sperrliste für `haex_*`. Berechtigungsziele werden beim Speichern geprüft (müssen Präfix einer
   Erweiterung sein). Namen mit `__` sind für Erweiterungen und Tabellen verboten (FR-004).

**Begründung**: Die Prüfung von HV ist eine gute, getestete Grundlage und wird deshalb übernommen. Ein
AST-Prüfer allein bleibt aber anfällig: Er muss jede Stelle kennen, an der SQL eine Tabelle nennen kann, und
sqlparser und SQLite können eine Anweisung verschieden verstehen (die Lücken in HV zeigen beides). Der
Authorizer ist ein Rückruf, den SQLite beim Vorbereiten **jeder** Anweisung für jeden Tabellen- und
Spaltenzugriff, jede Funktion und jedes Pragma aufruft, mit dem Namen, den SQLite selbst aufgelöst hat; er
kostet rund 150 Zeilen und fängt, was der Parser übersieht. Die Erlaubtliste schließt die Klasse „vorläufige
Berechtigung umgeht die Systemtabellen-Sperre“ (HV `check/database.rs:66-74`) und die Präfix-Kollision von HV
(`utils.rs:202-210`, `starts_with`) aus.

**Voraussetzung (haex-crdt, L0)**: `CrdtTransaction` hält die Transaktion privat (`database/write.rs:35-40`),
und `Database::read` setzt seinen eigenen Authorizer und danach `None` (`write.rs:171-183, 213-219`). holzi
kommt also nicht selbst an die Verbindung. Neue API in haex-crdt: `Database::write_guarded(&SqlGuard, f)` und
`read_guarded(&SqlGuard, f)` mit `SqlGuard { authorizer, progress }`; haex-crdt kombiniert ihn mit seiner
eigenen Nur-Lese-Sperre und setzt ihn nur um die Anweisungen der Erweiterung, nicht um seine eigenen
(`SELECT current_hlc()`, `persist_timestamp`, `db/core/execute/mod.rs:60-81`).

**Alternativen**: nur die portierte AST-Prüfung (eine übersehene Stelle genügt für einen Durchbruch);
nur der Authorizer (keine Berechtigungen vorab, also keine Anfrage; Fehler kämen erst bei der Ausführung);
dauerhafter Authorizer über `with_connection` (wird von `read()`
überschrieben und ist in holzi per Clippy gesperrt); SQL der Erweiterung roh auf der Verbindung (verletzt
FR-028).

## R7 — Ausführung, Ergebnisform, Grenzen

**Entscheidung**:

- Anweisungen mit Ergebnis (`SELECT`, `RETURNING`) laufen über `query_map`, andere über `execute`
  (rusqlite lehnt Ergebniszeilen in `execute` ab). `SELECT` über den Weg „ausführen“ funktioniert (FR-030).
- Ergebnis wie das SDK erwartet (`src/types.ts:211-216`): `{rows: unknown[][], columns, rowsAffected,
lastInsertId}`; Spaltennamen aus der ersten Zeile, bei null Zeilen über einen neuen Zugriff in haex-crdt
  (L0); `rowsAffected` bei `RETURNING` = Zeilenzahl; `lastInsertId` über `last_insert_rowid()` in derselben
  Transaktion (HV liefert immer `null`).
- Sync-Spalten (`haex_*_no_sync`) werden aus jedem Ergebnis entfernt (auch bei `SELECT *`).
- Werte wie HV, damit vorhandene Erweiterungen laufen: BLOB als Base64-Zeichenkette, ganze Zahlen als Zahl
  (oberhalb 2^53 ungenau, wie bisher), REAL NaN/Inf als `null`. Parameter: Zahl, Text, `null`, Wahrheitswert
  (0/1); `Uint8Array`/`ArrayBuffer` wandelt die Brücke im Frontend in ein markiertes Base64 um, das Rust als
  BLOB bindet; Arrays und Objekte als JSON-Text (wie HV).
- Grenzen je Erweiterung (FR-031), Standard wie HV: 10.000 Zeilen, 20 gleichzeitige Anfragen, 1 MB SQL,
  Laufzeit 5 s (HV speichert 30 s, setzt sie aber nie durch). Zeilen- und Byte-Grenze im `query_map`-Callback
  (Abbruch mit Fehler), Laufzeit über einen Fortschritts-Callback im `SqlGuard`, der nach der Frist
  `SQLITE_INTERRUPT` auslöst; gleichzeitige Anfragen über eine Semaphore je Erweiterung. Kurze Laufzeit, weil
  die Erweiterung die eine Verbindung mit Sync und Oberfläche teilt (SC-006).
- Eine Transaktion aus mehreren Anweisungen ist ein `write_guarded` mit einer Schleife.

**Begründung**: `max_transaction_bytes` zählt nur gebundene Parameter (`write.rs:245-273`), daher die Grenze
der SQL-Länge. Die Werte von HV sind die, mit denen die vorhandenen Erweiterungen gebaut sind.

**Alternativen**: `InterruptHandle` einmalig holen (unterbricht womöglich die nächste fremde Anweisung);
Sync-Spalten durchreichen (verrät Interna, ohne Nutzen).

## R8 — Migrationen von Erweiterungen

**Entscheidung**: Ein eigener Migrations-Lauf in holzi (`extensions/sql/migrate.rs`), der haex-crdt-Engine
**nicht** nutzt (feste Journaltabelle, bricht bei fremden Journaleinträgen ab und fällt still auf rohes SQL
zurück, `db/migrations/engine.rs:140, 234`).

- Quelle: die Migrationszeilen der Erweiterung in der Vault (FR-036), sortiert, getrennt an
  `--> statement-breakpoint`. Aus dem Bundle und aus `extension_database_register_migrations` (SDK) kommen sie
  in dieselbe Tabelle; zur Laufzeit registrierte Migrationen müssen in der Migrationsliste des installierten
  Bundles enthalten sein (gleicher Name, gleicher SHA-256), sonst abgelehnt.
- Regeln je Anweisung (FR-033): nur eigenes Präfix oder `__new_<eigenes Präfix>` (Tabellenumbau von
  Drizzle); keine `REFERENCES` auf fremde Tabellen; keine `VIEW`, `TRIGGER`, `VIRTUAL`, `TEMP`, `ATTACH`,
  `AS SELECT`; keine Spalten mit Präfix `haex_` (sonst bekäme eine `_no_sync`-Tabelle Trigger und ihre
  Löschungen gingen in den Sync); `PRAGMA foreign_keys=OFF/ON` nur als Steuerung des Schema-Modus; kein
  Umbenennen über die Grenze `_no_sync`. Dazu der Authorizer im Migrationsprofil.
- Ganze Migration plus Journalzeile in einem `write_guarded` (FR-034); Abweichung über SHA-256 erkannt.
- **Journal**: `extension_migrations_applied_no_sync` beschreibt den Zustand der Datei (wie
  `haex_app_migrations_no_sync` in haex-crdt). Eine Kopie der Vault-Datei trägt das Schema mit; ein
  Journal nach ADR-0001 (synchronisiert mit Gerätekennung) würde nach dem Kopieren die Tabellen erneut anlegen
  wollen und scheitern. Spec FR-032 wurde dazu angepasst (R22).

**Voraussetzung (haex-crdt, L0)**: (a) nach einer Schemaänderung `setup_triggers_for_table(tx, T,
recreate=true)` in derselben Transaktion, bei `RENAME` die alten Trigger entfernen (heute verwirft
`transform_write` das Kennzeichen, `execute/mod.rs:51`; Trigger kommen nur beim Öffnen,
`db/init.rs:89`); (b) Schema-Modus: Fremdschlüssel vor `BEGIN` aus, `foreign_key_check` vor dem Commit,
danach wieder an (in einer Transaktion wirkt `PRAGMA foreign_keys` nicht; sonst kaskadiert Drizzles
`DROP` beim Umbau in Kindtabellen, deren Trigger Löschmarken in den Sync schreiben); (c) Umbau, der die
Sync-Spalten unverändert kopiert und Trigger währenddessen aus hat (sonst stempelt `INSERT … SELECT` jede
Zeile neu und überschreibt gleichzeitige Änderungen anderer Geräte); (d) lokaler Modus ohne CRDT-Spalten für
den Entwicklermodus (R16).

**Alternativen**: Migration in `db.write` und danach `install_crdt(T, allow_reinstall)` plus Reparatur beim
Start (nicht atomar, kein Schema-Modus, kein metadatentreuer Umbau); haex-crdt-Engine (siehe oben).

## R9 — Änderungsmeldungen an Erweiterungen

**Entscheidung**: In `vault_events::run` geht jede gebündelte Meldung zusätzlich an einen
`broadcast::Sender<Arc<Vec<String>>>`. Der Host kürzt sie je offenem Rahmen mit derselben Funktion
`policy::can_read(ext, table)`, die der Authorizer nutzt (eigene Tabellen einschließlich `_no_sync` oder
Leseberechtigung, nie Kerntabellen), und schickt sie als `haextension:sync:tables-updated` (SDK-Name) an den
Rahmen. Nach Migrationen kommt dieselbe Meldung mit allen Tabellen, deren Schema sich geändert hat (das SDK hat keinen eigenen Typ dafür). Weil DDL keine Zeilen schreibt, sieht der Commit-Bericht nur die Journalzeile der Migration; dann vergleicht der Host die Tabellen der Erweiterungen mit dem letzten Stand von `sqlite_master`. Berechtigungen werden im Speicher
gehalten und bei Änderung verworfen.

**Begründung**: `observe_committed_changes` ersetzt einen früheren Beobachter (`database/mod.rs:174-188`); ein
zweiter Beobachter ist also nicht möglich. Der Beobachter erfasst lokale Schreibvorgänge, Sync und Resync (die
Lücke von HV, das nur nach einem Sync meldet).

**Alternativen**: Filtern im Frontend (dann kennt das Frontend-Ereignis die Kerntabellen und die Prüfstelle
läge außerhalb von Rust, gegen ADR-0004).

## R10 — Sync: Gruppen für fehlende Tabellen parken

**Entscheidung**: Vor dem Anwenden prüft der Empfang jede Gruppe (eine HLC, atomar): berührt sie eine Tabelle
mit Erweiterungspräfix (auch über das Ziel einer Löschmarke, `inbound.rs:290-299`), die es noch nicht gibt,
oder eine Spalte, die einer Erweiterungstabelle noch fehlt? Dann wird die **ganze Gruppe** in
`sync_parked_groups_no_sync` abgelegt, im selben `db.write` wie das Weiterrücken des Fortschritts; anderes
läuft weiter (FR-037). Unbekannte Tabellen ohne Präfix brechen weiter ab; `_no_sync`-Tabellen einer
Erweiterung auf der Leitung sind ein Protokollfehler. Der Lebenszyklus-Dienst (R11) wendet nach den Migrationen
die geparkten Gruppen dieses Präfixes in HLC-Reihenfolge an (`apply_remote_changes`) und löscht sie danach.
Grenze für geparkte Bytes je Erweiterung (256 MiB): an der Grenze wird keine Gruppe verworfen, sondern der Fortschritt des Ursprungsgeräts hält vor der nächsten Gruppe dieses Präfixes an, mit einer Statusmeldung (Fehler `parked_limit` am Gerätezustand dieser Erweiterung, solange sie dort noch übertragen wird; er endet, sobald sie dort startet oder mit „Daten löschen“ entfernt wird); „Daten löschen“ verwirft die geparkten Gruppen. Nach jedem Anwenden wird geprüft, dass
keine unbekannte Spalte übersprungen wurde.

**Gerät mit älterer Fassung**: Fehlt eine Tabelle oder Spalte, obwohl die Erweiterung hier `ready` oder
`disabled` ist, jede bekannte Migration angewendet ist und im laufenden Pull keine neue ankommt, hat eine
Migration sie entfernt oder umbenannt; geschrieben hat sie ein Gerät, das noch die ältere Fassung hat. Geparkt würde
diese Gruppe nie anwendbar und hielte alle späteren der Erweiterung fest. Sie wird daher nicht geparkt,
sondern wartet (`outdated_origin`): Der Fortschritt ihres Ursprungs bleibt unter ihr, spätere Gruppen
der Erweiterung im selben Pull warten mit, anderes wird übernommen. Der Absender liest beim Ausliefern
den aktuellen Stand seiner Tabellen; sobald er aktualisiert hat, kommt die Gruppe in der Form an, die
seine eigene Migration ihr gegeben hat (umbenannte Spalte unter neuem Namen, umgewandelte Daten mit
neuer HLC). Ist die Erweiterung hier nicht bereit (Übertragung, Signatur- oder Migrationsfehler), wird
wie oben geparkt. Eine falsche Einordnung kostet nur ein erneutes Abholen. Verworfen: eine
Spalten-Historie, die Zellen entfernter Spalten verwirft und umbenannte umleitet — sie verlöre Daten,
die die Migration des Absenders umgewandelt hätte.

**Überholte geparkte Gruppe**: Parkt ein Gerät eine Gruppe, solange es selbst zurückliegt, und wendet es
danach eine Migration an, die genau deren Spalte entfernt oder umbenennt, wird die Gruppe nie anwendbar.
Das Nachspielen erkennt das (`outdated_origin` bei einer jetzt aktuellen Erweiterung) und markiert sie
`refetch`. Jeder Pull fragt ihren Ursprung dann ab knapp vor ihr an (`Replica::pull_vector`); der
gespeicherte Fortschritt sinkt nicht, Ausliefern und Anwesenheit nutzen ihn weiter. Die frische Kopie
ersetzt die Markierung und wird wie jede Gruppe einsortiert. Die Markierung fällt, sobald ein Pull, der
von unter ihr gefragt hat, den Ursprung bis zu ihr bekommen hat und keine Gruppe davon wartet: Dann ist
die Kopie übernommen oder neu geparkt, oder spätere Änderungen haben die Gruppe ganz überschrieben. Ein
Pull, der vor der Markierung begann, lässt sie stehen; gleichzeitige Pulls verlieren sie so nicht.
Antwortet der Sender auf den abgesenkten Stand mit Resync, gilt dasselbe für den Snapshot. Verworfen:
den gespeicherten Fortschritt senken — ein gleichzeitiger Pull hebt ihn wieder über die Gruppe.

**Hinweis in den Einstellungen**: Jede Zeile der Geräteliste einer Erweiterung nennt die Fassung des
Bundles, das das Gerät dort hat (`extension_device_status.bundle_id`); ist sie älter als die hier
wirksame, heißt es „älter als hier: Daten von diesem Gerät kommen teils erst nach seinem Update an“. Der
Zustand dieses Geräts selbst steht schon darüber („wird übertragen“).

Regel für holzi: synchronisierte Zeilen von Kern- und Erweiterungstabellen werden nie in einer Schreibgruppe
geschrieben (sonst warten Kerndaten mit einer geparkten Gruppe). Lokale Journalzeilen (`_no_sync`) dürfen mit in
der Transaktion stehen.

**Begründung**: Heute bricht der Empfang bei jeder unbekannten Tabelle ab (`inbound.rs:178-189`, Test
`inbound_tests.rs:293-305`), und haex-crdt überspringt unbekannte Spalten still (`crdt/apply/row.rs:182-186`)
— beides verletzt FR-037. Spec 024 verlangt Atomarität je Gruppe, nicht je Paket; das Parken ganzer Gruppen
hält sie ein. Spätes Anwenden ist CRDT-sicher (Last-Writer-Wins je Spalte; Löschungen im Log unterdrücken
ältere Einfügungen, `delete_propagation.rs:108-118`).

**Alternativen**: das vorhandene `hold` (hält den Fortschritt unten; Daten einer Erweiterung, die nie startet,
kämen bei jedem Pull erneut); Migrationen direkt im Empfang (Signaturprüfung und DDL unter der Sperre des
Austauschs).

## R11 — Lebenszyklus über mehrere Geräte

**Entscheidung**:

- **Wirksame Fassung** = höchste Semver unter den nicht zurückgezogenen Bundles einer Erweiterung, bei
  Gleichstand die größere Bundle-Kennung; damit ergibt sich auf jedem Gerät dieselbe. Ein bestätigtes
  Downgrade zieht alle höheren Bundles zurück; eine gleichzeitige neuere Installation gewinnt trotzdem.
- Vor dem Wechsel prüft ein Gerät, dass die Migrationen der neuen Fassung eine Obermenge der angewendeten
  (Name, SHA-256) sind; sonst startet die Erweiterung dort nicht. Ausnahme bestätigtes Downgrade: die
  Migrationen der älteren Fassung müssen mit den angewendeten übereinstimmen, soweit sie dieselben Namen haben;
  angewendete, die die ältere Fassung nicht kennt, bleiben (nichts wird zurückgenommen, US7-3). Jedes Gerät
  führt nach einem Downgrade auch die Migrationen der zurückgezogenen höheren Bundles aus, die die ältere
  Fassung nicht kennt, sofern deren Bundle verifiziert; sonst blieben die Zeilen mit ihren Spalten dort
  geparkt.
- **Deaktivieren**: Last-Writer-Wins auf `extensions.enabled` (FR-039).
- **Entfernen**: Das auslösende Gerät setzt `state = removed`, `purge_data` und `purge_hlc` an der
  Erweiterungszeile (sie bleibt als Grabstein) und löscht seine Registry-Zeilen normal. Jedes Gerät räumt
  lokal in einem `write` auf: immer Schlüssel-Wert-Einträge und Protokolle; nur bei `purge_data` zusätzlich
  Tabellen mit Präfix, Journal und geparkte Gruppen. Bei „Daten behalten“ bleiben Tabellen, Journal und
  geparkte Gruppen, damit eine Neuinstallation die Migrationen nicht erneut ausführt. Schemaänderungen
  synchronisieren nicht (keine Zeilentrigger auf `DROP TABLE`), deshalb diese Marke.
- **Auslöser** ist `purge_hlc`, nicht `state`: `state` ist Last-Writer-Wins, und ein Gerät, das offline war,
  während entfernt und neu installiert wurde, sieht nur `installed`. Jedes Gerät merkt sich in
  `extension_purges_applied_no_sync` den zuletzt ausgeführten `purge_hlc` je Erweiterung und räumt auf,
  sobald ein neuerer ankommt, auch wenn `state` inzwischen wieder `installed` ist. Eine Neuinstallation lässt
  `purge_data` und `purge_hlc` stehen.
- **Empfang** (Ergänzung zu R10): Änderungen und Löschmarken an Tabellen einer mit `purge_data` entfernten
  Erweiterung mit HLC vor `purge_hlc` werden verworfen; bei „Daten behalten“ werden sie übernommen.
  Neuere Gruppen werden geparkt (`awaiting_purge`), solange dieses Gerät für diesen `purge_hlc` noch nicht
  aufgeräumt hat, auch wenn die Entfernung erst weiter vorn im selben Pull ankam; das Aufräumen verwirft
  nur geparkte Gruppen bis `purge_hlc`. Sonst würde ein Gerät, das während Entfernen und Neuinstallation
  offline war, die Zeilen der Neuinstallation mit den alten Tabellen löschen.
- **Neuinstallation** nach dem Entfernen nutzt dieselbe abgeleitete Kennung; neuere HLCs gewinnen über das
  Lösch-Log.
- **Behaltene Daten**: Bei „Daten behalten“ bleiben die Zeilen in `extension_migrations`. Jedes Gerät wendet die
  Migrationen einer Erweiterung, die bei ihm nicht läuft (deaktiviert oder mit behaltenen Daten entfernt), trotzdem
  an, auch ein später hinzugekommenes; so haben ihre synchronisierten Zeilen einen Platz und bleiben nicht geparkt.
  „Behaltene Daten löschen“ setzt `purge_data` und einen neuen `purge_hlc` und löscht die Migrationen; das Aufräumen
  läuft dann wie beim Entfernen mit „Daten löschen“. Die Liste zeigt eine entfernte Erweiterung nur, solange ihre
  Daten behalten werden, mit der Größe ihrer Tabellen auf diesem Gerät (`dbstat`).
- Gerätezustand („bereit“, „wird übertragen“, „Migration fehlgeschlagen“ mit Fehler) als synchronisierte,
  gerätebezogene Zeile für die Anzeige in den Einstellungen.

**Begründung**: kleinste Änderung am Sync-Kern, die FR-008, FR-037 bis FR-039 und den Randfall „zwei Geräte
installieren gleichzeitig“ erfüllt.

**Alternativen**: eigene `ApplyPolicy` in haex-crdt (034 R2 nennt sie als langfristigen Weg, größer);
Spalte `current_bundle_id` mit Last-Writer-Wins (verletzt „höhere Fassung gewinnt“).

## R12 — Ausliefern der Dateien, Ursprung und CSP

**Entscheidung**:

- Ein asynchrones Protokoll `holzi-ext` mit Pfad-Routing: `holzi-ext://localhost/<extId>/<datei>` (Linux,
  macOS) bzw. `http://holzi-ext.localhost/<extId>/<datei>` (Windows); `extId` ist die kurze Kennung aus der
  Registry. Der Handler wählt Dateien nur über den Pfad (nie über Origin, Referer oder einen „letzte
  Erweiterung“-Speicher wie HV `protocol.rs:21-24, 498-560`), liest abseits des Executors, antwortet mit
  `Access-Control-Allow-Origin: *` (Module unter undurchsichtigem Ursprung) und fällt für SPA-Routen auf
  `index.html` zurück.
- Abschottung über `sandbox="allow-scripts"` ohne `allow-same-origin`: jeder Rahmen hat einen eigenen
  undurchsichtigen Ursprung, getrennt von holzi, von anderen Erweiterungen und von eigenen anderen Tabs.
  Kein `allow-top-navigation`, `allow-popups`, `allow-forms`. Folge: Browser-Speicher, Cookies, IndexedDB und
  Service Worker gibt es im Rahmen nicht; das SDK ersetzt `localStorage` und Cookies schon durch Attrappen
  (`polyfills/localStorage.ts:15-28`), dauerhaft speichert FR-043.
- CSP **als Antwort-Header** (Tauri und wry reichen Header auf allen Desktop-Plattformen durch,
  `tauri-2.12.1/src/protocol/tauri.rs:181-184`, `webkitgtk/web_context.rs:235-239`,
  `wkwebview/class/url_scheme_handler.rs:138-139`):
  `default-src 'none'; script-src P 'wasm-unsafe-eval' 'sha256-…'; style-src P 'unsafe-inline'; img-src P data: blob:;
font-src P data:; media-src P data: blob:; connect-src P; worker-src P blob:; frame-src 'none';
object-src 'none'; base-uri 'none'; form-action 'none'; frame-ancestors <holzi>` mit P = Präfix der
  Erweiterung. Die Hashes der Inline-Skripte (Nuxt-Konfiguration, Import-Map, Polyfill des SDK) berechnet holzi
  nach der Signaturprüfung. Kein `'unsafe-eval'`. `'wasm-unsafe-eval'` für alle Erweiterungen, seit der erste
  Bedarf da ist (haex-doom: eine nach WebAssembly übersetzte Engine): es erlaubt nur das Übersetzen von
  WebAssembly, kein `eval` von JavaScript, und eine Berechtigung je Erweiterung brächte keine Abschottung. Keine expliziten
  `'self'` (Verhalten unter undurchsichtigem Ursprung uneinheitlich).
- holzis eigene CSP (`src-tauri/tauri.conf.json:23-44`) bekommt `frame-src holzi-ext://localhost
http://holzi-ext.localhost` (in `csp` und `devCsp`) und dasselbe in `img-src` für Symbole. `frame-src` der
  Elternseite regelt auch Selbstnavigationen des Rahmens und ist damit der Schutz gegen
  `location = 'https://…?daten'` (FR-010). `http://localhost:*` kommt **nicht** in die feste CSP (R16).
- **Windows**: wry spritzt die IPC-Skripte samt Invoke-Key in jeden Rahmen (`wry lib.rs:1029`,
  `webview2/mod.rs:506-508`). Das IPC-Protokoll lehnt `Origin: null` zwar ab
  (`tauri-2.12.1/src/ipc/protocol.rs:486-494`), und `ipc:`/`http://ipc.localhost` stehen nie in der CSP des
  Rahmens; das genügt FR-066 aber nicht, weil der Schlüssel im Rahmen läge. Deshalb korrigiert L0 wry auch für
  Windows: Skripte mit `for_main_frame_only` werden dort in `if (window === window.top) { … }` eingeschlossen
  (R25).

**Begründung**: ein CSP-fester Host auf allen Plattformen; das SDK erzwingt relative Pfade
(`src/nuxt.ts:115-116`). Ein Host je Erweiterung brächte unter dem undurchsichtigen Ursprung nichts und ließe
sich auf Windows nicht in `frame-src` festnageln (Platzhalter nur ganz links, DNS-Teil höchstens 63 Zeichen).

**Alternativen**: Host je Erweiterung; Base64-Host plus Speicher wie HV (unsicher); `'unsafe-inline'` für
Skripte (schwächer); CSP als `<meta>` (kann `frame-ancestors` nicht).

**Befunde der Sandbox-Prüfung (T045)** an den Apps von haextension `db48f9a948522c18a00331aac232718825cc9317`
(`apps/*/app`, `apps/*/src`; `haex-pass-browser` ist keine Erweiterung):

- `confirm()` vor dem Löschen in haex-notes (`app/pages/index.vue:48`) und haex-draw (`app/pages/index.vue:62`):
  ohne `allow-modals` gibt `confirm()` sofort `false` zurück, Löschen wäre unmöglich. **Entscheidung
  (Betreiber, 2026-10-03)**: kein `allow-modals`; das SDK bekommt `client.dialog.confirm`, holzi zeigt den
  Dialog über dem eigenen Tab (Brückenmethode `extension_dialog_confirm`), die Apps stellen um (T117–T119).
  Der Dialog nimmt beim Öffnen die Tastatur (der Bestätigen-Knopf bekommt den Fokus), der Rahmen ist
  solange `inert` und bekommt die Tastatur nach der Antwort zurück, wie nach `confirm()`; sonst bliebe sie im
  Rahmen, Escape bräche nicht ab und Enter auf dem Knopf der Erweiterung fragte erneut (7000).
- ics-Export in haex-calendar über `a.download` (`app/composables/useIcal.ts:400`): ohne `allow-downloads`
  passiert nichts. **Entscheidung**: kein `allow-downloads`; der Export läuft künftig über den
  Speichern-Dialog von holzi (L4, `extension_filesystem_*` mit Dialog-Auswahl), bis dahin geht er nicht.
- Link mit `target="_blank"` in haex-calendar (`app/components/calendar/EventPreview.vue:56`): öffnet nichts;
  Abhilfe ist `extension_web_open` (L4). `allow-popups` bleibt ausgeschlossen.
- `navigator.clipboard.writeText` in haex-code (`app/components/TerminalView.vue:159`): im Rahmen ohne
  Berechtigungs-Policy wirkungslos; kein L1-Thema (haex-code braucht ohnehin `shell`, L5).
- Formulare haben überall `@submit.prevent` (haex-files, haex-mail, haex-pass); Blob-URLs für Bilder
  (haex-image) sind nach `img-src … blob:` erlaubt; `alert(`, `prompt(`, `window.open(`, `localStorage` und
  `indexedDB` kommen nicht vor.

**Rest-Risiken**: WebRTC und DNS-Prefetch sind über CSP nicht sperrbar (Aufgabe: WebView2-Argument
`--force-webrtc-ip-handling-policy`, WebKitGTK hat WebRTC standardmäßig aus, vermutet). Keine
Prozesstrennung zwischen Rahmen (Spectre-artige Restrisiken).

## R13 — Kanal binden (FR-011)

**Entscheidung**:

- Jeder Rahmen bekommt beim Einhängen eine `frameInstanceId`; Rust mintet dafür ein **Start-Token**, gebunden
  an (Erweiterung, Tab, Rahmen), das in der URL des Rahmens steht (`?hf=<token>`). HTML-Dokumente liefert der
  Handler nur bei passendem Token für die `extId` des Pfads aus. So kann Rahmen A nicht zur Seite von B
  navigieren und B einen an A gebundenen Kanal erhalten.
- Bei **jedem neuen Dokument** (HV nutzt `{once:true}` und verpasst Neuladen, `broadcast.ts:155-193`): alten
  Port schließen, neuen `MessageChannel` anlegen, `PORT_INIT` alle 200 ms an `iframe.contentWindow` (Zielursprung
  `'*'` ist bei undurchsichtigem Ursprung unvermeidbar) bis `PORT_READY` auf **diesem** Port kommt (SDK-Frist
  10 s, `client/init.ts:25`). Zuordnung nur über `Map<port, {extId, tabId, frameInstanceId, generation}>`.
  Meldungen vor `READY` werden gepuffert (FR-042). Konsolenausgaben kommen über
  `window.parent.postMessage` und werden über `event.source === iframe.contentWindow` zugeordnet.
  Befund (T051): WebKitGTK feuert `load` am iframe auch für eine Hash-Navigation im Rahmen; das SDK nimmt
  `PORT_INIT` aber nur einmal je Dokument an. Ein neues Dokument meldet deshalb der Rahmen-Shim
  (`hello {fresh}`, contracts/bridge.md), nicht das `load` allein.
- **SDK-Änderung (L0)**: `waitForHostPortAsync` nimmt heute jedes `PORT_INIT` an
  (`client/init.ts:345-366`). Ein Geschwisterrahmen erreicht andere über `top.frames[i].postMessage` und könnte
  holzi zuvorkommen und einen falschen Port unterschieben. Fix im SDK: `if (event.source !== window.parent)
return` (klein, rückwärtskompatibel). Bis Erweiterungen neu gebaut sind: Kommt kein `READY` auf holzis Port,
  lädt holzi den Rahmen neu und protokolliert das.

**Begründung**: Host-Kontrolle allein verhindert das gefälschte `PORT_INIT` eines Geschwisters nicht.

**Alternativen**: nur Host-seitig (Lücke bleibt); Host je Rahmen (CSP-Problem, R12).

## R14 — Brücke im Frontend, Prüfstelle in Rust

**Entscheidung**: Das Frontend reicht jede Anfrage aus einem Port unverändert mit der `frameInstanceId` an
**einen** Tauri-Command `extension_bridge_call(frame, method, params)` weiter. Rust ordnet `frame` einer beim
Öffnen registrierten Rahmensitzung zu (Erweiterung, Tab, Gerät), prüft Aktivierung, Methode
(Erlaubtliste, sonst „nicht unterstützt“, FR-060) und Berechtigung und ruft die Funktion direkt auf. Die
Rahmensitzung legt das Frontend über `extension_frame_open(extId, tabId)` an (Antwort: `frame`, Start-Token,
URL); die Erweiterung kennt `frame` nie. Meldungen an Rahmen schickt Rust als ein Ereignis
`extension-frame-event {frame, type, data}` an das Hauptfenster, bereits gefiltert; das Frontend reicht sie an
den Port. Antworten folgen dem SDK-Format `{id, result | error{code, message, details}}`.

Fehlercodes wie HV, damit das SDK sie versteht (1002 verweigert, 1004 Anfrage nötig), ergänzt um holzi-eigene:
8000 „nicht unterstützt“, 8001 „nicht verfügbar“, 8002 „Erweiterung deaktiviert“ ([contracts/bridge.md](./contracts/bridge.md)).
Ein Vertragstest zählt alle Methoden der Erlaubtliste auf und scheitert, wenn eine davon Code unter `chat/`,
`llm/` oder `adapters/` erreicht (FR-009).

**Begründung**: ADR-0004 (eine Prüfstelle in Rust, Frontend nur Relais). Erweiterungen haben in holzi kein
eigenes Webview-Fenster, also stammt die Identität aus der Rahmensitzung, die holzis eigener Code anlegt — nie
aus Angaben der Erweiterung. Kein Tauri-ACL-Eintrag je Erweiterungs-Command nötig.

**Alternativen**: ein Tauri-Command je Host-Funktion (jede bräuchte eigene Identitätsprüfung); Prüfung im
Frontend (gegen ADR-0004).

## R15 — Berechtigungen: Modell, Anfrage, Einstellungen

**Entscheidung**:

- Reines Rust-Modul `extensions/permissions/` (Art, Aktion, Ziel, Zustand, Geltungsbereich vault-weit oder
  Gerät), Auswertung „verweigert vor erteilt vor fragen“, „Lesen und Schreiben“ deckt „Lesen“ (FR-017).
  Unbekannte Art, Aktion oder Ziel aus der Datenbank gelten als nicht vorhanden (FR-022; HV fällt auf Db/Read
  zurück, `crud.rs:246-255`). Schreibweisen `readWrite` und `read_write` werden beim Lesen des Manifests
  vereinheitlicht.
- Geltungsbereich über `vault_device_uuid` nach ADR-0001 (Nil-Kennung = vault-weit). Nur die Shell ist eine
  gerätebezogene Art und gilt immer auf dem Gerät; alles andere vault-weit, ohne Wahl (Clarification 2026-10-06,
  ersetzt die vom 2026-10-02).
- Vorläufige Berechtigungen nur im Speicher von Rust, bis die Vault schließt.
- Anfrage: Rust antwortet mit 1004 und schickt `extension-permission-request {requestId, ext, kind, action,
target, declared, deviceScoped}` an die Oberfläche; die Warteschlange im Frontend fasst gleiche Anfragen
  zusammen, zeigt sie nacheinander und meldet die Entscheidung an `extension_permission_resolve` (nur aus
  holzis Oberfläche aufrufbar, nicht über die Brücke). Rust schickt danach `extension:permission-resolved`
  an alle Rahmen der Erweiterung; das SDK wiederholt die Anfrage (`client/permissionRetry.ts`). Eine Anfrage,
  deren Rahmen alle geschlossen sind, verschwindet.
- Oberfläche: neuer Dialog nach dem Muster von `chat/PermissionPrompt.vue` (Schließen = abbrechen, nicht
  verweigern), Kategorie „Erweiterungen“ in `src/lib/settings/registry.ts` mit Liste, Details,
  Berechtigungen (mit Geltungsbereich), behaltenen Daten, Grenzen, Protokollen und Entwicklermodus; Auswahlen
  werden sofort gespeichert (023 FR-021), Bausteine `SettingsGroup`/`SettingsRow`/`SettingsOptionRow`.

**Begründung**: SDK-kompatibel (Fehler 1004 und Wiederholung), Prüfung bleibt in Rust; Erweiterungen können
Anfragen nicht selbst beantworten (der Fehler von HV vor dem Fix, `extension-commands.toml:93-99`).

**Alternativen**: Rust wartet auf die Entscheidung (blockiert die Anfrage lange und passt nicht zur
Wiederholungslogik des SDK).

## R16 — Entwicklermodus

**Entscheidung**: Schalter als gerätebezogene Einstellung (ADR-0001). Die Entwicklerin wählt den
Projektordner; holzi liest `haextension/manifest.json` und die Migrationen von dort. Host der Adresse nur
`localhost`, `127.0.0.1` oder `[::1]`. Registrierung und Berechtigungen in `dev_extensions_no_sync` und
`dev_extension_permissions_no_sync` mit Gerätekennung. Laden wird abgelehnt, wenn es für dasselbe
`(publicKey, name)` eine Zeile in `extensions` gibt (gleich welcher `state`, also auch nach „Daten behalten“)
oder eine Tabelle mit diesem Präfix besteht (FR-065); der Schlüssel im Manifest ist unsigniert und darf nicht
an fremde Daten kommen. Eine spätere Installation wird abgelehnt, solange Entwicklungs-Tabellen mit diesem
Präfix bestehen. Kommt eine Installation desselben Präfixes über den Sync von einem anderen Gerät, startet sie
auf diesem Gerät nicht (`migration_failed` mit Fehler `dev_prefix_conflict`), und Gruppen für dieses Präfix
werden weiter geparkt, bis die Entwicklungsfassung entladen und ihre Tabellen gelöscht sind. Entwicklungs-Tabellen entstehen im lokalen Modus von haex-crdt **ohne** CRDT-Spalten; die
Erkennung synchronisierter Tabellen und die Trigger lassen sie dadurch aus (`sync/replica.rs:113-124`,
`db/init.rs:39-52`). Die Adresse des Entwicklungsservers kommt nicht in die feste CSP. holzi setzt `frame-src`
für sie zur Laufzeit in die CSP des Hauptdokuments über `on_web_resource_request` (Muster
`tauri-2.12.1/src/webview/mod.rs:470-480`); weil die CSP beim Laden des Dokuments gilt, lädt das Einschalten
des Entwicklermodus das Hauptfenster neu (vermutet, Prüfaufgabe). Der Rahmen bekommt kein
`allow-same-origin` (gleiche Abschottung wie installiert). Die Antworten des Entwicklungsservers kontrolliert
holzi nicht; ihre CSP ist Sache der Entwicklerin, Netzsperre und Inline-Hashes gelten im Entwicklermodus also
nicht (Bewusste Grenze). Konsolenausgabe des Rahmens zeigt holzi in einem Bereich des Tabs.

**Umsetzung (L3)**: Die Migrationen kommen nicht von der Platte, sondern aus `extension_database_register_migrations`, mit dem die Erweiterung sie ohnehin meldet (wie HV im Entwicklermodus): die Regeln für das Lesen von Migrationen aus einem Bundle liegen privat in `haex-bundle`, ein zweiter Leser in holzi wäre eine zweite Implementierung. Sie laufen durch dieselbe Prüfung und im lokalen Modus. Erlaubt sind nur `localhost` und `127.0.0.1`, weil eine CSP-Quelle keine IPv6-Adresse nennen kann. Das Hauptfenster wird in `setup` gebaut (`create: false`), damit es den Haken `on_web_resource_request` bekommt; im `tauri dev` mit Entwicklungsserver ruft Tauri ihn nicht auf, dort lädt der Rahmen einer Entwicklungsfassung also nicht. Nach dem Entsperren mit eingeschaltetem Modus lädt das Fenster einmal neu (das Dokument stammt von vor dem Entsperren); ein Merker in `sessionStorage` verhindert eine Schleife. Geprüft unter Linux (WebKitGTK, E2E-Szene `extension-dev-mode`); macOS und Windows sind noch offen. `name` kommt wie im `readManifest` des SDK aus der `package.json` des Projekts (das SDK benennt die Tabellen damit, `haex init` schreibt keinen Namen ins Manifest); `version`, `author` und `homepage` von dort, wo das Manifest keine hat. `extension_get_info`, das Öffnen eines Rahmens und die Liste lesen das Projekt jeweils neu (ein Entwicklungsserver auf neuem Port wird gefunden), aber nur solange es dasselbe Präfix nennt wie die Registrierung; sonst `dev_project_changed`, und das Projekt wird neu geladen. Bewusste Grenze: `frame-src` gilt für das ganze Hauptdokument, solange der Modus an ist, darf also auch der Rahmen einer installierten Erweiterung zu einem Server auf `localhost`/`127.0.0.1` navigieren. Nur die Adressen der geladenen Projekte freizugeben, hieße das Fenster bei jedem Laden und jedem Portwechsel neu zu laden; der Modus ist eine bewusste Einstellung dieses Geräts.

**Nachtrag (Rahmen-Shim, 2026-10-05)**: Den Rahmen-Shim fügt holzi in die Dokumente ein, die es selbst ausliefert. Die Seite einer Entwicklungsfassung kommt vom Entwicklungsserver; ihr gibt holzi denselben Shim als Init-Skript des Hauptfensters für alle Rahmen (`initialization_script_for_all_frames`, Tauri 2.12). Es läuft wie das eingefügte vor den Skripten der Seite, aber nur in einem Rahmen (`window.top !== window`) mit einer Adresse `http://localhost:*` oder `http://127.0.0.1:*`, also nur dort, wo `frame-src` im Entwicklermodus Rahmen erlaubt. Ein ausgelieferter Rahmen (`holzi-ext`) hat eine andere Adresse und bekommt den Shim weiter nur eingefügt, nicht doppelt. Der Text des Shims bleibt eine Konstante, beide Wege setzen dieselbe ein. Damit meldet auch die Seite einer Entwicklungsfassung `hello {fresh}`, und holzi braucht keinen eigenen Handshake für sie. Grenze: Auf Android ohne `addDocumentStartJavaScript` (WebView-Funktion `DOCUMENT_START_SCRIPT`) setzt wry Init-Skripte nur in Antworten des eigenen Protokolls ein oder führt sie bei `onPageStarted` im Hauptdokument aus, nie in einem Rahmen (wry 0.57, `RustWebViewClient.kt`); die Seite einer Entwicklungsfassung bekommt dort keinen Shim, meldet kein `hello`, und ihr Tab zeigt nach der Frist den Fehler mit „Neu laden“. Alternativen: den Entwicklungsserver über das holzi-Protokoll spiegeln und den Shim einfügen (bricht den HMR-Websocket und die Ausnahme aus FR-064, dass holzi dort keine eigene Inhaltsrichtlinie setzt); den Shim ins SDK verlegen (eine zweite Implementierung neben holzis oder ein Umbau des SDK für alle Hosts).

**Begründung**: geräteeigen per Bauart; keine SDK-Änderung; derselbe Ablauf wie HV über den Projektordner,
aber ohne dessen synchronisierte Registrierung (HV `dev_server.rs:266, 301`).

**Alternativen**: Manifest vom Entwicklungsserver (das Vite-Plugin des SDK liefert es nicht aus,
`vite.ts:38-112`); `_no_sync`-Endung erzwingen (müsste das SQL der Erweiterung umschreiben).

## R17 — Window Manager, Navigation und Tastatur

**Entscheidung**:

- Apps: reaktive Liste `allApps()` = `WM_APPS` plus `extension.<extId>`-Definitionen (Titel als Text statt
  i18n-Schlüssel, Symbol aus dem Bundle, `multiInstance` aus dem Manifest). Alle Stellen, die heute `WM_APPS`
  direkt nutzen, bekommen die Liste (Launcher, „+“-Menü, Window-Manager-Store, Layout- und Aktionshandler,
  `titleForLocation`). Die Liste der Erweiterungen wird **vor** `wm.restoreSessionAsync()` geladen, und das
  Wiederherstellen bekommt die Apps als Getter, damit Tabs mit Erweiterungen nicht verworfen werden
  (`layoutState.ts:243`, `sessionSync.ts:64`).
- `TabPanel.vue` zeigt für `extension.*` einen `ExtensionFrame` statt `WmRouterView`; er nutzt `useWmTab()`
  (Titel, Aufmerksamkeit, Schließen-Wächter, Schließen) und `useTabRouter()`.
- Ein Tab, der in ein anderes Fenster wandert, und ein Wechsel des Arbeitsbereichs hängen den Rahmen neu ein,
  und WebKit lädt ihn dabei neu. In L1 akzeptiert: der aktuelle Ort kommt über den Hash der URL zurück. Eine
  dauerhafte Rahmenebene auf Ebene des Desktops ist eine spätere Verbesserung.
- **Rahmen-Shim**: Das SDK v3.7.0 hat keine Meldungen für Navigation, Titel, Tastatur oder Schließen
  (`messages.ts:9-34`; sein History-Polyfill sendet nichts, `polyfills/history.ts:25-84`). holzi fügt beim
  Ausliefern ein Inline-Skript in HTML ein (durch einen CSP-Hash gedeckt; die Signaturprüfung gilt für die
  gespeicherten Bytes), das auf ein eigenes `holzi:frame:init` mit zweitem Port hört (Prüfung `event.source
=== parent`) und Web-Standards abbildet: `hashchange`/`popstate` → Ort im Tab (`{path, query, replace}`),
  `document.title` → Tabtitel, registriertes `beforeunload` → Schließen-Wächter, `window.close()` → Tab
  schließen, `keydown` für die gebundenen Kürzel (holzi schickt die Liste) → Aktion, nur solange der Rahmen den Fokus hat und das Fenster aktiv ist (der Shim läuft im Code der Erweiterung, seine Nachrichten sind fälschbar). holzis Zurück schickt
  den Zielort, der Shim setzt den Hash. Aufmerksamkeit hat kein Web-Gegenstück; dafür bekommt das SDK in L0
  `client.tab.requestAttention(active)`, das die Brückenmethode `extension_tab_attention {active}` sendet. Rust
  prüft den Rahmen und meldet `extension-tab-attention {frame, active}` an die Oberfläche.

**Begründung**: FR-012 und Spec 020 FR-034/035 ohne Änderung am Code der Erweiterungen (FR-013).

**Alternativen**: nur SDK-Meldungen (Erweiterungen müssten neu gebaut werden); keine Brücke (verfehlt
FR-012); dauerhafte Rahmenebene sofort (Z-Reihenfolge und Fokus deutlich aufwendiger).

## R18 — Netzwerk

**Entscheidung**: Eigener `reqwest::Client` (vorhanden, rustls) mit `redirect::Policy::none()`; holzi folgt
höchstens zehn Weiterleitungen selbst und prüft jedes Ziel mit derselben Regel, einschließlich Methode
(FR-050); bei Ursprungswechsel fallen `Authorization` und `Cookie` weg. Nur http/https. Zeitlimit und
Größengrenze der Antwort aus den Grenzwerten. Muster: `*`, `schema://host/pfad*` mit `*.` für Subdomains,
blanke Domain genau oder als Suffix an einer Label-Grenze (`.example.org`; HV `manager/url.rs` prüft das ohne Punkt). Öffnen im Browser über
`tauri-plugin-opener` nach der Prüfung. Keine neue Abhängigkeit.

**Begründung**: HV folgt Weiterleitungen ungeprüft und prüft die Methode nie (`check/web.rs:16`); eine eigene
Schleife kann auch für das Weiterleitungsziel fragen.

**Umgesetzt (T098)**: Die Anfrage an den Nutzer nennt die Methode und den Ursprung `schema://host[:port]/*`, nicht
die volle Adresse; sonst fragte jede Symboldatei neu. Eine Weiterleitung auf ein Ziel ohne Berechtigung antwortet
1004 für dieses Ziel, das SDK wiederholt danach die ganze Anfrage. Das gilt nur, wenn die erste Methode wiederholbar ist
(GET, HEAD, OPTIONS, TRACE, PUT, DELETE, PROPFIND, REPORT); sonst, etwa bei POST oder PATCH, ist die Weiterleitung selbst
die Antwort (wie `fetch` mit `redirect: "manual"`), damit der erste Server die Anfrage nicht zweimal bekommt (Betreiber,
2026-10-05). Die Erweiterung folgt ihr mit einer eigenen Anfrage, für die gefragt werden darf. Ein verweigertes Ziel
bleibt 1002.

**Alternativen**: `Policy::custom` (synchron, kann nicht fragen); `tauri-plugin-http` (unnötig).

## R19 — Dateisystem

**Entscheidung**: Alles in Rust, auch Speichern und Öffnen (kein JS-Teil von plugin-fs).

1. Ziel mit `canonicalize` auflösen; bei noch nicht vorhandenem Ziel den nächsten vorhandenen Vorfahren
   auflösen und den Rest anhängen, `..` im Rest ablehnen.
2. Feste Sperrliste vor jeder Berechtigung: App-Daten, Verzeichnis der Vault, Konfiguration (FR-049).
3. Berechtigungen als Pfadpräfix mit Lesen oder Lesen und Schreiben, Geltungsbereich nach R15.
4. Was der Nutzer im Dialog wählt, wird eine an den Rahmen gebundene vorläufige Berechtigung (Datei genau,
   Ordner mit Unterordnern; Lesen beim Öffnen, Lesen und Schreiben beim Speichern), FR-048. `save_file` schreibt
   im selben Command.
5. `open_file` schreibt in ein holzi-eigenes Temp-Verzeichnis, nur mit `Path::file_name()`.
6. Beobachten über `notify` + `notify-debouncer-full` (neu), Schlüssel (Erweiterung, `ruleId`), Meldungen
   (`filesync:file-changed`, flache Form wie im SDK, jeder Pfad eines Bündels) nur an Rahmen der Erweiterung.

**Umsetzung (L4)**: `extensions/fs/` mit `resolve`, der Prüfung `authorize` (Sperrliste, Dialog-Auswahl des
Rahmens, dann Berechtigung mit Rückfrage 1004), `ops`, `dialogs` und `watch`. Gesperrt sind alle App-Ordner aus
Tauris Pfadauflöser (Konfiguration, Daten, lokale Daten mit den Vaults, Cache, Log), aufgelöst. Ein Aufruf, der
einen ganzen Baum erfasst (rekursives Entfernen, Umbenennen, Kopieren, Beobachten), ist auch gesperrt, wenn der
Baum einen dieser Orte enthält. Kopieren lässt symbolische Links innerhalb des Baums aus, weil ihr Ziel nie
geprüft wurde. Die Dialoge laufen über das Merkmal `FileDialogs` im Host (Tauri-Plugins im Programm, ein Fake in
den Tests), damit die Bridge keinen `AppHandle` braucht. `open_file` und `show_image` legen die Kopie in einen
Ordner im Cache von holzi, der selbst gesperrt ist; `show_image` nimmt nur PNG, JPEG, GIF, WebP und BMP.
Beobachtungen enden mit dem letzten Rahmen ihrer Erweiterung, beim Entfernen, Deaktivieren und Entladen der
Erweiterung, und eine nur durch die Dialog-Auswahl eines Rahmens erlaubte schon mit diesem Rahmen (FR-048). Sie
folgen keinen Links im Baum. `filesync:file-changed` trägt den Pfad relativ zum beobachteten Ordner; holzis Fenster
legt die Felder flacher Ereignisse neben `type`.

**Nachträge aus dem Review (L4)**: Ein Pfadteil, der da ist, sich aber nicht auflösen lässt (kaputter oder
kreisender Link), wird abgelehnt: Schreiben folgte ihm sonst an ein ungeprüftes Ziel. Kopieren schreibt nicht durch
einen Link, der im Ziel schon liegt. `read_file` liest nur reguläre Dateien und höchstens bis zur Antwortgröße.
`open_file` übergibt dem Standardprogramm nur Dokumente, Bilder und Medien (feste Liste von Endungen, ohne `:`,
Steuerzeichen und Punkt oder Leerzeichen am Ende): sonst startete etwa unter Windows eine `.bat` ohne jede
Berechtigung, und eine Seite (`.html`, `.svg`) liefe mit Zugriff auf lokale Dateien. Kopien im Cache, die älter als
ein Tag sind, entfernt holzi beim Start.

**Nachtrag Spec 044 (2026-10-07)**: Auflösen, Sperrliste der eigenen Orte von holzi (FR-049) und der Kern des
Beobachtens ziehen nach `src-tauri/src/files/local/`, weil der Dateibrowser und die Agents dieselben Regeln
brauchen; `extensions/fs` ruft sie über dünne Adapter auf, das Verhalten für Erweiterungen bleibt gleich. `notify`
wird mit 044 auch für Android gebaut; `watch` für Erweiterungen antwortet auf Android und iOS weiter „nicht
verfügbar“ (8001).

**Begründung**: FR-047/FR-049 verlangen das tatsächliche Ziel; HV prüft rein lexikalisch (Symlinks
entkommen), Dialog-Auswahlen erzeugen dort keine Berechtigung, `unwatch` prüft den Besitzer nicht.

**Alternativen**: `cap-std` ohne Folgen von Links (schließt das Zeitfenster zwischen Prüfen und Öffnen; später
als Härtung); lexikalischer Abgleich wie HV.

## R20 — Benachrichtigungen, Schlüssel-Wert-Speicher, Protokolle

**Entscheidung**:

- Benachrichtigungen über `tauri-plugin-notification` (neu). Klicks: Das Plugin meldet sie auf Desktops nicht
  (HV `notifications/mod.rs:12-18`). Unter Linux über `notify-rust` (XDG-Aktionen, `wait_for_action`,
  `close`; vermutet), auf macOS und Windows, wo das System es zulässt — Aufgabe mit Machbarkeitsprüfung; wenn
  es nicht geht, kommt der Punkt zur Spec zurück (FR-052). Symbol nur als `data:`-URL oder Datei aus dem Bundle.
- **Ergebnis der Machbarkeitsprüfung (T099, 2026-10-04/05)**: Unter Linux (COSMIC, `cosmic-notifications` 1.9,
  Fähigkeiten `actions`, `persistence`) meldet `notify-rust` 4.18.1 einen Klick auf die Benachrichtigung als `default`
  und ein Schließen als `Closed`; ohne Klick kommt nichts. `notify-rust` 4.18 meldet Klicks auch unter Windows
  (WinRT-Aktivierung) und macOS (`NSUserNotificationCenter`, blockiert bis zur Antwort). Betreiber: eine einheitliche
  Lösung, nicht je Betriebssystem. Deshalb nutzt holzi überall `tauri-plugin-notification` und hört Klicks über dessen
  `Notification::on_action`; der Fork `haexmas/plugins-workspace` (Zweig `feat/notification-desktop-actions`)
  ergänzt die Desktop-Seite (Aktionsarten als Knöpfe, `tap` für den Klick auf die Benachrichtigung,
  `remove_active` unter Linux und den BSDs, Listener-Befehle), Upstream-PR zu tauri-apps/plugins-workspace#2150.
  Schließen meldet das Plugin auf dem Desktop nicht; holzi begrenzt deshalb die offenen Benachrichtigungen je
  Erweiterung. Android und iOS melden Wegwischen als Aktion `dismiss`; holzi wertet nur `tap` und die Knöpfe als
  Klick. Unter Linux (COSMIC, Fork `3f9db5e`) geprüft: Ein Klick auf die Benachrichtigung kommt als `tap`, ein Knopf
  mit seiner Kennung, beide mit holzis Kennung der Benachrichtigung; das Klicksignal des Servers wurde dafür mit
  `gdbus emit` gesendet, nachdem der frühere Klick von Hand gezeigt hatte, dass COSMIC es sendet. Windows und macOS
  sind in der CI des Forks gebaut und geprüft (Clippy), aber nicht angeklickt; T099 bleibt bis dahin offen.
- **Nachtrag (2026-10-10)**: Upstream hat die Desktop-Seite als tauri-apps/plugins-workspace#3671 übernommen
  (Commit `46381516b7bd7b6915ded698d93e92760b4221d7` auf Zweig `v2`); holzi nimmt das Plugin seitdem von dort statt
  aus dem Fork, bis ein Release nach 2.5.1 es enthält. Abweichungen zum Fork: Unter Linux und den BSDs hört eine
  eigene D-Bus-Verbindung (`zbus`) auf die Signale aller Benachrichtigungen statt `notify-rust`s `handle_action` je
  Benachrichtigung, und Knöpfe erscheinen nur, solange ein Handler oder Listener existiert (holzi registriert
  `on_action` beim Start). Die Prüfung unter COSMIC oben galt dem Fork.

## R21 — Passwörter, entfernter Speicher, Mail, Shell (L5)

**Entscheidung**:

- **Passwörter**: dünner Adapter auf die Zugriffsprüfung von 034 (`Caller::Extension{id}`, `Grant{action,
scope}` aus `passwords/access.rs`), Berechtigungen der Art `passwords` → `Grant`. `list` →
  `list_headers`, `read` → `read_secret_item`, `create`/`update` → `create_item`/`update_item` (volle Eingabe
  des SDK wird zu einem Teil-Update), `delete` → Papierkorb. 034 ist seit PR #222 auf `main`; der Adapter braucht keinen Platzhalter mehr.
- **Entfernter Speicher**: Trait `ObjectStore`; Erweiterungen sehen nur Speicher, die der Nutzer für sie
  freigibt, mit einem Schlüsselpräfix je Erweiterung (029 FR-027 erlaubt in Space-Buckets nur
  inhaltsadressierte Objekte, also nie beliebige Schlüssel dort). Anlegen, Ändern, Prüfen und Entfernen immer
  über einen Dialog von holzi. Umgesetzt durch Spec 038 (Speicherverbindungen ohne Spaces): S3-Client
  `rusty-s3` über reqwest, eine Berechtigung `remoteStorage` je Speicher (`backendId`, ein Bucket) oder für
  `*`, Präfix `holzi-ext/<vault_id>/<extension_id>/` (Entwicklerversionen getrennt), Zugangsdaten gibt nur
  der Nutzer in holzi ein, sie liegen im Passwortmanager (Eintrag mit Eigentümer, Regel Z14 in 034) und
  erreichen keine Erweiterung (Review 2026-10-06: nicht mehr „je Verbindung“). Bis dahin „nicht verfügbar“.
- **Mail**: `async-imap` (tokio, `tokio-rustls`), `lettre` (rustls/ring), `mail-parser` (alle neu, reines
  Rust). Befehle wie im SDK; Berechtigung je Host **und** Port. Das Beobachten liest Zugangsdaten nur über 034
  mit der Erweiterung als Aufrufer (HV liest sie am Passwortmanager vorbei, `mail/poll.rs:164-208`). Beobachten
  endet mit dem letzten Rahmen, beim Deaktivieren und beim Schließen der Vault (FR-058).
- **Shell**: `portable-pty` 0.9 (vorhanden). Programm kanonisch auflösen und je Programm prüfen (HV prüft
  immer `"*"`, `shell/commands.rs:16-31`); Besitzprüfung je Sitzung; Prozessgruppe beenden bei letztem Rahmen,
  Deaktivieren, Schließen der Vault; UTF-8 strömend dekodieren; auf Nicht-Desktop „nicht verfügbar“
  (`cfg(desktop)`).
- **TLS** überall rustls mit ring (holzi lehnt aws-lc-rs ab).

## R22 — Änderungen an der Spec aus der Planung

Eingetragen als „(Planung)“ in den Clarifications der Spec:

1. Journal der angewendeten Migrationen als `_no_sync` (Zustand der Datei), nicht nach ADR-0001 (FR-032, R8).
2. Schlüssel-Wert-Speicher nach ADR-0001: gilt nur für das Gerät, liegt aber als gerätebezogene Zeile in der
   Vault (FR-043, R20).
3. Android und iOS: Erweiterungen laufen dort wie auf dem Desktop. Android braucht dafür eine Korrektur in wry
   (R25).
4. Beim Wechsel des Arbeitsbereichs und beim Verschieben eines Tabs lädt der Rahmen neu; der Ort bleibt
   (R17).

## R23 — Tests und CI

- Rust-Integrationstests unter `src-tauri/tests/`: `extension_bundle_format.rs` (Testvektoren aus dem
  vault-sdk, Kopf mit Repository, SHA und Pfad), `extension_sql_bypass.rs` (Umgehungssammlung SC-002,
  tabellengetrieben, dreimal: nur Vorprüfung, nur Authorizer per Test-Haken, beide; **jede** Schicht muss jeden
  verbotenen Fall allein ablehnen, positive Fälle und Fälle mit eigener Regel je Schicht nach sql-policy.md), `extension_migrations.rs`, `extension_sql_exec.rs` (Ergebnisform, Grenzen, Laufzeit),
  `sync_extension_parking.rs` und `extension_lifecycle_sync.rs` (mit `tests/common/sync_fixture.rs`),
  `extension_bridge_contract.rs` (FR-009), später `extension_fs.rs`, `extension_web.rs`.
- Einheitstests je Modul in `*_tests.rs` über `#[path]`.
- Frontend: `pnpm check:extensions` (Brücke, Warteschlange, App-Liste, Shim-Abbildung) nach dem Muster der
  vorhandenen Prüfskripte; `typecheck`, `lint`, `format:check`.
- End-to-End (Rahmen aus Spec 016/033): `extension-install-open`, `extension-permission-prompt`,
  `extension-isolation` (Rahmen kommt nicht an holzi, andere Rahmen, Netz), `extension-two-devices`,
  `extension-dialog` (Bestätigung nimmt die Tastatur, Escape und Enter antworten).
- Lokal nur gezielte Testbinärdateien mit `-j 4` (Erfahrung aus früheren Sitzungen); CI deckt den Rest.

## R24 — ADR

ADR-0008 „Erweiterungen: Bundle-Signatur v2 und SQLite-Authorizer als Prüfstelle für SQL“ hält die zwei
Entscheidungen mit der größten Tragweite fest (R2, R6). 0005 bleibt für Spec 021, 0007 für Spec 034
reserviert.

## R25 — Android und iOS

**Entscheidung**: Erweiterungen laufen auf Android und iOS mit demselben Modell (iframe, Brücke, Prüfstelle in
Rust). Die mobilen Builds von holzi selbst sind eigene Arbeit (eigene Spec, siehe plan.md); 017 baut alles so,
dass es dort läuft, und wird dort abgenommen, sobald die Ziele bestehen.

- **Android und Windows, IPC in Rahmen (L0, wry)**: wry spritzt die Init-Skripte von Tauri — darunter den IPC-Code mit dem
  `__TAURI_INVOKE_KEY__` — in **jeden** Rahmen: `WebViewCompat.addDocumentStartJavaScript(this, script,
setOf("*"))` (`wry-0.57.0/src/android/kotlin/RustWebView.kt:31`), ohne `for_main_frame_only` zu beachten.
  Die Java-Brücke `window.ipc` (`addJavascriptInterface`, `src/android/main_pipe.rs:308-311`) ist ohnehin in
  allen Rahmen sichtbar; Tauri lehnt aber jede Nachricht ohne den richtigen Invoke-Key ab
  (`tauri-2.12.1/src/error.rs:166-167`, `manager/mod.rs:688`). Windows hat dasselbe Problem (R12).
  **Korrektur** auf beiden Plattformen: Skripte mit `for_main_frame_only` in `if (window === window.top) { … }`
  einschließen. Auf Android wird zusätzlich der Rückfallweg ohne `addDocumentStartJavaScript` korrigiert: Dort
  fügte wry die Skripte als `<script>`-Element in jedes HTML des eigenen Protokolls ein, auch in Unterrahmen, deren
  Code den Text samt Invoke-Key aus dem DOM lesen könnte; jetzt bekommen Unterrahmen
  (`WebResourceRequest.isForMainFrame()`) nur Skripte, die nicht auf den Hauptrahmen beschränkt sind. Die
  Origin-Regeln bleiben `"*"` (Umsetzung, 2026-10-03): Sie filtern nach Ursprung, nicht nach Rahmen, würden
  legitime Hauptrahmen auf anderen Ursprüngen (Remote-Capabilities, Entwicklungsserver) die IPC nehmen und einen
  Unterrahmen mit gleichem Ursprung trotzdem nicht ausschließen; der Wächter ist der Schutz. Ein Hauptrahmen auf
  fremdem Ursprung (etwa nach einer Navigation) bekommt die Skripte auf jeder Plattform, auch unter Linux und
  macOS; ihn hält Tauris ACL ab: Anfragen von nicht lokalen Ursprüngen werden abgelehnt, solange keine Capability
  den Ursprung unter `remote` nennt (`tauri-2.12.1/src/webview/mod.rs:2075-2108`). holzi hat keine solche
  Capability, und `src-tauri/src/extensions/capabilities_tests.rs` lässt keine zu. Ein Rahmen einer
  Erweiterung bekommt den Schlüssel damit nie. Ein Block statt einer Funktion hält `var` und Funktionsdeklarationen
  global; Tauris Init-Skripte binden nichts mit `let`/`const` auf oberster Ebene (`tauri-2.12.1/scripts/*.js`). PR an `tauri-apps/wry`; bis zur
  Veröffentlichung nutzt holzi einen Fork über `[patch.crates-io]`, gepinnt auf den vollen SHA (Constitution
  IV). Abnahme: End-to-End-Szene auf dem Emulator prüft, dass `__TAURI_INTERNALS__` im Rahmen fehlt und ein
  Aufruf über `window.ipc` scheitert.
- **iOS**: wry setzt `WKUserScript … forMainFrameOnly` aus `for_main_frame_only` (`wkwebview/mod.rs:647, 784`),
  derselbe Code wie auf macOS. Erwartet: Rahmen bekommen die IPC nicht (vermutet, gleiche Abnahme-Szene im
  Simulator).
- **Protokoll und URL**: Android wie Windows `http://holzi-ext.localhost/<extId>/…`, iOS wie macOS
  `holzi-ext://localhost/<extId>/…` (`wry-0.57.0/src/custom_protocol_workaround.rs:13-40`); CSP-Header
  und `frame-src` wie in R12.
- **Plattformunterschiede der Host-Funktionen**: Dateien über die System-Auswahl (Android liefert
  Content-URIs; nur Dialog-Auswahl und App-eigene Orte, keine freien Pfade; HV nutzt dafür
  `tauri-plugin-android-fs`, `filesystem/commands/picker.rs:24-58`); Beobachten von Ordnern und Shell sind
  „nicht verfügbar“ (FR-059); Benachrichtigungen über `tauri-plugin-notification` (unterstützt beide);
  Tastenkürzel des Shims nur mit Hardware-Tastatur; Netz, SQL, Passwörter, Mail wie auf dem Desktop.
- **Entwicklermodus auf Mobilgeräten**: der Entwicklungsserver läuft auf dem Rechner und ist über
  `adb reverse` (Android) bzw. den Simulator (iOS) als `localhost` erreichbar; FR-064 bleibt damit gültig.
- **Speicher und Leistung**: Bundles werden aus der Datenbank gelesen wie auf dem Desktop; die Grenzen aus R3
  gelten unverändert.

**Begründung**: Der Betreiber verlangt Android- und iOS-Ziele mit Erweiterungen. Die Ursache auf Android ist
eine kleine, klar umrissene Stelle in wry; der Schutz durch den Invoke-Key greift, sobald der Schlüssel nicht
mehr in Rahmen landet.

**Alternativen**: eigenes Webview je Erweiterung (Tauri unterstützt mehrere Webviews auf Mobilgeräten nicht);
Erweiterungen auf Android sperren (vom Betreiber abgelehnt); die IPC im Rahmen per Skript entfernen (Rennen
mit dem injizierten Skript, nicht verlässlich).
