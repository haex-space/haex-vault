# Research: Sync-Regeln

Die Fakten zum Code stammen aus einer Lesung des Stands `bc23034c` (2026-10-26). Pfade beziehen sich auf
`src-tauri/src/` (Rust) und `src/` (Frontend). Wo eine Zusage des Plans auf einer Annahme beruht, steht
sie als **Risiko** mit einer Aufgabe zum Klären am Anfang der Umsetzung.

## R1 — Orte: eine eigene dünne Schicht `Place`

**Befund.**

- Es gibt keine einzelne Schnittstelle über alle Arten von Quellen. `SourceRef` (`files/mod.rs`) kennt nur
  `Device` und `Storage { storage_id }`.
- Das Gerät läuft über freie Funktionen (`files/local/ops.rs`, `edit.rs`, `transfer/local.rs`).
- Speicher laufen über `StorageFiles` (`files/storage_source.rs`: `list`, `stat`, `rename`, `delete`,
  `read_start`, `create_folder`). Darunter liegt `ObjectStore` (`remote_storage/object_store.rs`) mit S3, WebDAV,
  SFTP, `Local` (`remote_storage/local.rs`, ein Ordner als Speicher, bisher nur für verschlüsselte Ordner auf dem
  Gerät) und `Peer` (`files/peer/store.rs`, andere eigene Geräte).
- Verschlüsselte Ordner sind in `StorageFiles` über `Route` eingehängt: Pfade laufen über entschlüsselte
  Namen.
- Ein Ort ist `(SourceRef oder Speicher-ID als String, Pfad)`. Die Speicher-ID eines anderen Geräts ist
  `peer:<Gerät>`; für Kontakt-Freigaben `share:<id>` (für Regeln ausgeschlossen, FR-002).
- Listen der Speicher sind nicht rekursiv; `list_dir` und `objects_under` liefern Ebenen oder Präfixe.

**Entscheidung.** Ein Merkmal `Place` mit zwei Ausprägungen im Modul `file_sync`:

- `DevicePlace { root: PathBuf }` für den lokalen Ordner (Dateisystem, `walkdir`, Papierkorb des Systems).
- `StoragePlace { files: StorageFiles, root: String }` für Speicher, andere Geräte und verschlüsselte Ordner.

`Place` bietet nur, was der Motor braucht: `list_tree` (rekursiv, Größe, Zeit, Art, `symlink`, `no_access`),
`stat`, `hash` (Streaming-SHA-256), `rename`/`move_within`, `delete_permanent`, `trash`, `read_text` und
`write_text` für `.haex-ignore`. Kopieren gehört nicht dazu (R2).

**Alternativen.**

- _Alles als `ObjectStore`, Geräteordner über `Protocol::Local`_: `put` nimmt den ganzen Inhalt als `Vec<u8>`
  (nur für kleine Objekte gedacht) und die Zeiten stammen aus dem synthetischen ETag. Verworfen.
- _`SourceRef` um Peer und Freigaben erweitern und ein Merkmal über alles ziehen_: ändert 044/056 für ein
  Feature, das nur lesen und kopieren will. Verworfen; `Place` kann später darauf aufsetzen.

**Geklärt (T003, 2026-10-26).** Ein Ort wird wie im Fenster adressiert: `SourceRef` plus Fensterpfad.
`files::browser_commands::place_of(app, state, files, source, path, want)` löst ihn auf: ein Speicher (auch ein
anderes Gerät über `peer:<Gerät>`) wird über `window_storage` mit dem `Scope` geöffnet, der Pfade durch
verschlüsselte Ordner nach ihren entschlüsselten Namen führt; ein Gerätepfad in einen verschlüsselten Ordner
auf dem Gerät (`DeviceRoot`) wird zum Speicher `device:<Wurzel>` mit Fensterpfad; jeder andere Gerätepfad bleibt
`Place::Device`. Der Motor speichert deshalb nur `SourceRef` und Fensterpfad und löst den Ort bei jedem Lauf mit
`place_of` neu auf (sichtbar machen als `pub(crate)`). Ein eigenes Format für verschlüsselte Orte entfällt.

## R2 — Kopieren über `files::transfer`

**Befund.**

- `TransferManager` hängt an Oberflächenaufträgen (`Channel<TransferEvent>`, Fragen an den Nutzer), die
  Bibliotheksschicht darunter nicht: `local::run(job, totals, hooks)` (Gerät nach Gerät) und
  `remote::prepare_remote`/`remote::run_remote(job, plan, handle, retries, hooks)` mit
  `RemoteJob { op, from: Side, sources, to: Side, target }`.
- `Side` ist `Device` oder `Storage(Arc<StorageFiles>)`; damit sind alle Paarungen abgedeckt, auch verschlüsselte
  Ordner und Peers.
- Eingebaut sind Wiederholungen (1, 2, 4 s), Fortschritt (100 ms), Abbruch über `CancellationToken`, Teildatei
  `.<name>.holzi-part-<uuid>` beim Gerät, Mehrteil-Upload (8 MiB) bei S3; der Peer schreibt nur, wenn die
  Version noch passt (`Expected::Version/Absent`).
- Konfliktfragen laufen über `Hooks::ask` mit `Replace | KeepBoth | Skip`; „Replace“ schiebt den alten Eintrag
  zuerst beiseite.
- Nicht vorhanden: Fortsetzen nach Abbruch über Läufe hinweg, Setzen der Änderungszeit auf entfernten Zielen
  (nur lokal `set_modified`, `transfer/local.rs:375`), eine Prüfsumme je Datei.

**Entscheidung.** Der Motor ruft die Bibliotheksschicht je Datei oder je kleiner Gruppe auf, mit
`Hooks::ask = Replace` (der Plan hat schon entschieden) und eigenem Abbruch-Token. Dafür braucht
`files::transfer` einen schmalen, benannten Eintritt `copy_file(from: Side, path, to: Side, target, hooks)`; er
verpackt `RemoteJob` oder `local::run` und gibt Größe und die danach gemessene Zeit des Ziels zurück (für den
Stand).

**Geklärt (T001, 2026-10-26).** Kein eigener Ersetzungsmodus nötig; das Verhalten ist durch vorhandene Tests
festgehalten (`files/transfer/local_replace_tests.rs`, `remote_conflict_tests.rs`):

- Gerät: Replace schiebt das Ziel als `.<Name>.holzi-old-<uuid>` im selben Ordner beiseite, schreibt die neue
  Datei als `.<Name>.holzi-part-<uuid>` und benennt sie um; erst danach geht das Alte endgültig. Ein Fehler oder
  Abbruch legt das Alte zurück. Reste bleiben nur bei einem Absturz mitten darin; deshalb gehören
  `.*.holzi-old-*` zu den festen Ausnahmen. Die Änderungszeit der Quelle bleibt erhalten (`set_modified`).
- Speicher: Herunterladen in eine Teildatei mit Umbenennen; Hochladen bei S3 als ein `put` oder Mehrteil-Upload,
  bei SFTP als Teildatei mit `rename_over`, bei WebDAV als `PUT` einer temporären Datei mit `MOVE`. Unter dem
  Namen liegt nie eine halbe Datei. Entfernte Ziele setzen die Zeit selbst.
- Peer (056 FR-036): Hat sich das Ziel nach dem Plan geändert, endet der Upload mit `Conflict` und
  `run_remote` fragt ein zweites Mal (`after_conflict`); „Replace“ würde dann über die fremde Änderung
  schreiben. `copy_file` antwortet deshalb auf die erste Frage mit `Replace` (der Plan hat entschieden) und auf
  jede weitere Frage zur selben Datei mit `Skip`; die Datei gilt als „zwischendurch geändert“ und kommt in den
  nächsten Lauf.

**Risiko 2.** Fortsetzen nach Abbruch fehlt. FR-017 verlangt es nur für große Dateien innerhalb des Laufs; ein
neuer Lauf überträgt eine abgebrochene Datei neu (Teildatei wird verworfen). Die Spec (SC-005) verlangt, dass
vollständig kopierte Dateien nicht erneut übertragen werden, nicht das Fortsetzen halber.

## R3 — Der Plan ist reine Logik

**Entscheidung.** `plan::build(rule, a: &Listing, b: &Listing, state: &[StateRow], exclusions: &Exclusions, hashes: &Hashes) -> Plan`
ohne Ein- und Ausgabe. `Facts` sind Pfad → (Größe, Zeit, Art, Flags). `Plan` ist eine Liste von Schritten:
`Copy { from, path }`, `Move { side, from, to }`, `Delete { side, path, mode }`, `Conflict { path, keep, aside }`,
`Adopt { path }` (gleiche Prüfsumme, nur Stand), `Skip { path, reason }`, `Report { path, what }` (Abweichung am
Zielort bei einseitig). Die Tabelle aus der Spec ist die Testtabelle der Planlogik, eine Zeile je Fall.

Reihenfolge der Schritte: Ordner anlegen, Verschiebungen, Kopien und Konfliktkopien, zuletzt Löschungen (damit
ein Abbruch keine Daten kostet, bevor die Kopie sitzt).

Ordner: Ein Ordner gilt nicht als eigener Eintrag des Stands; er entsteht durch seine Dateien. Leere Ordner
werden angelegt (`create_folder`), aber nicht gelöscht, solange sie nicht ausdrücklich verschwunden sind
(beide Seiten leer nach Löschung der Dateien: Ordner bleibt; ein Ordner, der auf einer Seite gelöscht wurde
und auf der anderen unverändert leer ist, wird nach dem Löschmodus behandelt wie eine Datei).

## R4 — Stand: Größe und Zeit je Seite, Fortschreiben je Schritt

**Entscheidung.** Tabelle mit Primärschlüssel `(rule_id, path)`: `a_size`, `a_mtime_ms`, `b_size`,
`b_mtime_ms`, `kept`. Nach einem Kopieren misst der Motor das Ziel neu (`stat`) und speichert diese Werte, nicht
die der Quelle, weil entfernte Ziele die Zeit selbst setzen und nur Werte derselben Seite verglichen werden
(FR-008, Annahme der Spec). Eine Verschiebung benennt die Zeile um. Ein Schritt schreibt seine Zeile in einer
Transaktion, nachdem er gelungen ist (FR-015).

Ein Wechsel der Orte oder der Richtung verwirft den Stand (Spec Story 4, Szenario 6) und löst FR-012a aus.

**Verschiebungen** (FR-009). Auf jeder Seite: Pfad im Stand, aber nicht mehr in der Liste, und neuer Pfad mit
gleicher Größe und gleicher Zeit wie der verschwundene. Bei genau einem Kandidatenpaar gilt es als Verschiebung;
bei mehreren gleichen Paaren entscheidet die Prüfsumme (R5), sonst Kopie plus Löschung.

## R5 — Prüfsumme nur, wo sie nötig ist

**Befund.** Es gibt keine Prüfsumme je Quelle. SHA-256 liegt nur in den Sidecar-Dateien verschlüsselter Ordner
(`files/encrypted/format/content.rs`, `sidecar.rs: FileFacts.sha256`). S3-/WebDAV-ETags sind nicht
verlässlich ein Hash.

**Entscheidung.** `Place::hash(path)` liest streamend und bildet SHA-256 (`sha2`); bei verschlüsselten Orten
liefert es den Wert der Sidecar, ohne Lesen. Gebraucht in drei Fällen: FR-012a (erster Abgleich, gleichnamig und
gleich groß), mehrdeutige Verschiebung (FR-009), Kontrolle nach dem Kopieren (FR-016). Die Kontrolle nach
dem Kopieren vergleicht zuerst die Größe des Ziels; die Prüfsumme nur bei verschlüsselten Zielen und kleinen
Dateien (bis 8 MiB), sonst nicht (ein Lesen großer Dateien über das Netz wäre die ganze Datei noch einmal). Die
Vorschau nennt die Zahl der Dateien, die für FR-012a gelesen werden müssten (FR-012a, letzter Satz).

## R6 — Ausnahmen mit dem Crate `ignore`

**Befund.** Kein Crate für `.gitignore`-Muster in `Cargo.toml` (`globset`, `ignore`, `wax` fehlen).

**Entscheidung.** `ignore::gitignore::GitignoreBuilder`, nur Auswertung von Zeilen (`add_line`), kein Dateisystem-
Lauf des Crates. Zwei Matcher (Quellort, Zielort), ausgenommen ist, was einer von beiden trifft (FR-039); dazu die
festen Ausnahmen als dritte Liste. Unterordner-Dateien werden nicht gelesen. Ein Pfad ist ausgenommen, wenn er
oder ein Elternordner trifft (`matched_path_or_any_parents`).

Fehlerhafte Zeilen liefert `add_line` als Fehler je Zeile; der Motor sammelt sie für den Regelstatus und nimmt
die übrigen. Die Ausnahmedatei selbst wird abgeglichen wie jede Datei und ist nie ausgenommen.

**Alternativen.** _`globset` und eigene Negation_: baut `.gitignore` halb nach. _Eigenes Format_: der Betreiber
wünscht die gewohnte Schreibweise. Verworfen.

## R7 — Speicher: `*_no_sync` mit `vault_device_uuid`

**Befund.**

- Es gibt eine SQLite-Datenbank je Vault (haex-crdt). Geräteeigene Daten liegen dort; ADR 0001 verlangt dafür
  nachverfolgte Tabellen mit `vault_device_uuid` als Teil des Schlüssels und Fremdschlüssel auf
  `known_devices`, und reserviert `_no_sync` für flüchtigen Zustand, der Synclast wäre.
- `_no_sync` heißt nur „kein Sync-Paket“; die Zeilen reisen mit einer kopierten Datenbankdatei. Deshalb
  braucht jede Tabelle ohnehin `vault_device_uuid` (ADR 0001, letzter Punkt).
- Vorhandene Beispiele: `haex_file_device_access_no_sync`, `sync_progress_no_sync`, `device_presence_no_sync`.
- Nächste freie Migrationsnummer im Stand: `0042`; die Specs 060 (`0042_contacts_self`, `0043_contacts_cards_hash`)
  haben `0042` und `0043` reserviert, auch wenn ihre Umsetzung noch nicht gelandet ist.
  `HOLZI_TRIGGER_VERSION` bleibt unverändert, weil keine nachverfolgte Tabelle entsteht.

**Entscheidung.** Migration `0044_sync_rules` mit Tabellen `haex_sync_rules_no_sync`,
`haex_sync_rule_files_no_sync`, `haex_sync_rule_pending_no_sync`, `haex_sync_runs_no_sync`,
`haex_sync_run_items_no_sync`; alle mit `vault_device_uuid NOT NULL REFERENCES known_devices(...) ON DELETE
CASCADE`. Stand je Datei und Protokoll haben Hunderttausende Zeilen und würden jedes Gerät belasten; die Regeln
selbst dürfen laut Spec (FR-003) nicht auf andere Geräte. Die Abweichung von ADR 0001 hält **ADR 0018** fest.

**Geklärt (T002, 2026-10-26).** `0042_contacts_self` und `0043_contacts_cards_hash` sind auf `main`; `0044` ist
frei und wird `0044_sync_rules`.

**Einstellung Protokolltiefe.** `storage/preferences.rs` führt Einstellungen je Gerät (`PrefScope::Device`);
Schlüssel `syncRules.logDepth`, Wert ganze Zahl 0 bis 100, geprüft in `validate_value`. Die Vorgabe 10 gilt, solange
die Einstellung fehlt. Die Zeile ist wie jede Einstellung nachverfolgt und gilt nur für dieses Gerät (Spec
FR-044).

## R8 — Dienst, Zeitplan, Wächter

**Befund.**

- Sitzungsdienste starten an zwei Stellen: `instances/open.rs` (~148–170) und `instances/import.rs` (~204–222).
  Aufgaben laufen über `state.gate().spawn(...)` und enden mit dem Sperren oder Schließen der Vault
  (`gate.token()`). Zeitschleifen: `tokio::time::interval` mit `select!` auf `token.cancelled()`
  (`sync/service.rs`, `files/peer/serve/watch.rs`).
- Wächter: `files/local/watch.rs`, `watch_folder(root, recursive, on_changes) -> FolderWatch` (`notify` 8,
  Entprellen 500 ms, nicht auf iOS); Drop beendet ihn. Der Wächter für andere Geräte (056) ist ein Datenstrom
  der Peer-Verbindung.

**Entscheidung.** `file_sync::start_for_active_instance` an **beiden** Stellen. Eine Steuerungsaufgabe hält für
jede aktive Regel:

- einen Zeitgeber (Intervall der Regel, Standard 15 Minuten; bei lokalen Orten nur als Sicherheitsnetz,
  60 Minuten),
- für lokale Orte einen `FolderWatch` (rekursiv, Ereignisse vereinen, 2 s Ruhe, dann „fällig“),
- „Jetzt abgleichen“ und Regeländerungen über einen Kanal.

Je Regel läuft höchstens ein Lauf; ist einer aktiv, wird „fällig“ nur vermerkt. Läufe verschiedener Regeln
laufen nebenläufig mit Obergrenze 2 (Speicher und Netz nicht überlasten). Beim Öffnen der Vault wird jede aktive
Regel einmal fällig (Nachholen).

Wächter eigener Geräte (Peer-Watch) sind in v1 nicht angeschlossen; entfernte Orte laufen im Intervall. Das
kann später ohne Änderung des Modells nachgerüstet werden.

**Sperre.** Der Lauf prüft das Abbruch-Token zwischen Schritten und reicht es an die Übertragung weiter
(FR-026); die Teildatei wird von der Übertragungsschicht verworfen.

## R9 — Fehler, Status und Ereignisse

**Entscheidung.** Statusableitung in `status.rs` aus dem letzten Lauf und dem Zustand der Steuerung: aktiv, läuft,
wartet (Ort nicht erreichbar: `StorageError::Network|Unreachable`, Gerät offline, Schalter aus), Fehler
(Zugangsdaten, Ort nicht mehr vorhanden, Stand beschädigt), pausiert, holzi nicht aktiv (Anzeigewert des
Frontends, wenn die Steuerung nicht läuft). Änderungen gehen als Tauri-Ereignis `sync-rules-changed` mit Regel-ID
an das Frontend; das Frontend hält einen Pinia-Speicher wie `syncDevices.ts` (Hören, Erzeugungszähler gegen
Rennen, zusammenfassendes Laden). Fehlermodell: Ortsfehler als `FilesError` (bestehende Codes), Regelfehler als
`SyncRuleError { code, message }` mit Codes `overlap`, `invalidPlace`, `invalidInterval`, `notFound`.

## R10 — Löschen und Papierkorb

**Befund.** `trash` 5.2 nur auf dem Desktop (`transfer/entries.rs`, `platform_removal()`); auf Android und in
Speichern ist Löschen endgültig.

**Entscheidung.** `Place::trash(path)`:

- `DevicePlace` auf dem Desktop: `trash::delete` (Papierkorb des Systems); Protokoll nennt „Papierkorb des
  Systems“.
- sonst: `Place::move_within` nach `<Ort>/.holzi-trash/<Lauf>/<Pfad>`; Protokoll nennt den Regel-Papierkorb mit
  dem Pfad, die Oberfläche führt dorthin.

`.holzi-trash` ist fest ausgenommen und wird nie abgeglichen, nie geleert. In einem Speicher ohne Rename (S3)
ist es kopieren und löschen; die Datei bleibt bis dahin vollständig.

## R11 — Agent

**Befund.** Aktionen stehen im Frontend (`src/lib/actions/*.ts`, z. B. `fileShareActions.ts`) und laufen über
einen `NativeExecutor` (`chat/tools/native_action.rs`); Vorbild `files/agent/shares.rs` und
`files/agent/share_confirm.rs` (`PromptPark`, eine Abfrage je Aufruf). Berechtigungen: `files/permissions.rs`
(`AgentFileKind`), Tabelle `agent_file_permissions` (`kind TEXT NOT NULL`, kein CHECK).

**Entscheidung.** `AgentFileKind::SyncRules` (Ziel leer, nur durch den Nutzer gesetzt), `AgentGrants.sync_rules:
bool`. Aktionen: `syncRules.list` und `syncRules.status` (Lesen), `syncRules.create`, `syncRules.update`,
`syncRules.delete` (zerstörerisch, eigene Abfrage mit Orten, Richtung und Löschmodus). Eigene Daten von holzi sind
als Ort schon wegen FR-002 ausgeschlossen. `check-agent-actions` und `check-files-agent-permissions` werden
erweitert.

## R12 — Oberfläche

**Befund.** Dateibrowser: `src/components/apps/FilesApp.vue`, Routen in `src/lib/files/registry.ts`
(`FILES_ROUTE_CHILDREN`), Seitenleiste `src/components/files/Sidebar.vue` (Abschnitte als `<section>`, „Meine
Freigaben“ als Schaltfläche). Kein wiederverwendbarer Ordnerwähler für Quelle plus Pfad; vorhandene Wähler sind
spezifisch (Zugangsdaten, verschlüsselter Ordner, Systemdialog). Einstellungen: `src/lib/settings/registry.ts`
(`subView('general.dock', 'general/dock', 'general', …)`) plus `appRoutes.ts` und `scripts/check-settings.ts`.
haex-ui hat `UiTextarea`, `UiInput type=number`, `UiDrawerModal`, `ShadcnBadge`, `ShadcnSwitch`, `ShadcnTabs`; keine
Liste, keinen Code-Editor.

**Entscheidung.**

- Seitenleiste: neuer Abschnitt „Sync-Regeln“ (Schaltfläche wie „Meine Freigaben“) öffnet die Route
  `sync-rules`; `data-testid="files-sync-rules"`.
- `FilesFolderPicker`: lokal im Repo (er hängt an `files_sources` und `files_list`, kein allgemeiner Baustein
  für haex-ui); Ergebnis `{ source: SourceRef | storageId, path }`. Zeigt keine Orte fremder Vaults.
- Ausnahmen-Editor: `UiTextarea` mit Zeilenzähler und der Zahl der ausgenommenen Dateien; feste Ausnahmen
  darunter, nicht änderbar.
- Einstellungen: Dialog `SyncRuleSettings.vue` im Sync-Regeln-Bereich (Schaltfläche neben „Neue Regel“; vorher `general.syncLog`) mit `UiInput type=number` und Muster von `ExtensionLimitsView.vue` (Speichern bei
  Blur oder Enter, ungültige Eingabe fällt auf den gespeicherten Wert zurück).
- Pinia-Speicher: jedes Feld beim Neumounten klassifizieren (offene Dialoge zurücksetzen, laufende Läufe bleiben).
- Neue reine Helfer in `src/lib/files/syncRules.ts` ziehen keine Bindings mit verschachtelten Importen nach
  (`typecheck:scripts`).

## R13 — Plattformen

Auf Android sind Orte gewöhnliche Pfade (`/storage/emulated/0/...`); SAF betrifft nur gewählte Einzeldateien
(Spec 043). Der Wächter (inotify) funktioniert; Einschränkungen des Scoped Storage für Ordner fremder Apps sind
nicht gesondert behandelt, die Regel meldet dann `noAccess` je Datei. Lösch-Papierkorb: siehe R10. Regeln
pausieren bei geschlossener App (FR-028); der Hintergrunddienst ist eine eigene Spec.

## R14 — Dokumente mit Verweisen auf 025

Beim Löschen von `specs/025-own-device-file-sync/` müssen folgende Stellen mitgezogen werden, damit
`scripts/ci/check-docs.py` (Link-Prüfung) grün bleibt: `specs/056-file-access-sharing/spec.md` (Zeilen ~67, ~583),
`specs/044-file-browser/spec.md` (~73), `docs/plans/2026-10-07-file-browser-design.md` (~16),
`docs/plans/2026-10-21-file-access-and-sharing-design.md` (~8, ~125), `docs/adr/0013-encrypted-folder-format.md`
(~10), `docs/plans/2026-09-28-sync-architecture.md` (Tabelle ~637),
`docs/plans/2026-10-26-sync-rules-design.md` (Zeile 5), `specs/061-sync-rules/spec.md` (Zeile ~61). Zeilen
von Hand prüfen, nicht nur per Suche (`specs/026-blind-relay`, `specs/028-data-shares` erwähnen 025
möglicherweise nur mit der bloßen Nummer).

## R15 — Messung SC-007

`src-tauri/src/file_sync/perf_tests.rs` (T068): 10 000 Dateien in 100 Ordnern auf dem Gerät, der erste
Lauf kopiert sie, der zweite findet nichts zu tun. Gemessen am 2026-10-31 im Debug-Build auf einem
i7-12800HX mit ext4: **0,25 s** für den zweiten Lauf (SC-007 erlaubt 5 s). Der Test ist `#[ignore]` und
läuft nur von Hand (`cargo test --lib -- --ignored file_sync::run::tests::perf --nocapture`).
