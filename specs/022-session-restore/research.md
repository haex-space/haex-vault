# Research: Sitzung wiederherstellen (wählbar)

Stand: 2026-09-26. Grundlage: der Code auf `022-session-restore` (aufgesetzt auf
`020-tab-navigation` nach der Umbenennung Shell → Window Manager, `b4abfb6`) und
haex-crdt an der von haex-vault gepinnten Revision
`ed230d2c3f58c1b10710b6025ea0ce6c20b8d009` (`src-tauri/Cargo.toml`). Zitate mit
`crdt:` beziehen sich auf haex-crdt an dieser Revision.

## R1 Wo die Sitzung liegt: eine gerätelokale `_no_sync`-Tabelle

**Entscheidung**: Die Sitzung liegt in einer neuen Tabelle
`wm_sessions_no_sync` mit genau einer Zeile je Gerät
(`vault_device_uuid` als Primärschlüssel, `session_json`, `updated_at`).

**Begründung**:

- haex-crdt nimmt Tabellen mit der Endung `_no_sync` vollständig von der
  Synchronisierung aus: keine CRDT-Spalten (crdt:`src/crdt/transformer/mod.rs:105-131`,
  `:311-321`), keine Trigger (crdt:`src/db/init.rs:3-8`, `:39-52`), nie im
  Delete-Log `haex_deleted_rows`, nie in Sync-Paketen; eingehende Änderungen für
  so eine Tabelle werden verworfen (crdt:`src/crdt/apply/engine.rs:206-211`). Ein
  `DELETE` ist dort ein echtes lokales Löschen. Damit ist FR-010 erfüllt.
- haex-vault hat das Muster schon einmal genutzt (`device_downloaded_models_no_sync`,
  Migration 0007, entfernt in 0012).
- Die Zeile trägt die Gerätekennung, obwohl die Tabelle nicht synchronisiert
  wird: Eine Vault-Datei kann von mehreren Geräten benutzt werden (portabler
  Modus, Spec 014), und jedes darf nur seine eigene Sitzung sehen (FR-002,
  ADR-0001).

**Verworfen**:

- _Die drei Tabellen aus 0019 behalten und nur umbenennen_ (`…_no_sync`): Das
  relationale Modell (Fremdschlüssel, Kaskaden, eindeutige Kennungen, die
  Vermeidung von Löschen-und-neu-Einfügen wegen des HLC-Gleichstands,
  `wm_windows.rs:9-15`) existierte nur wegen der Synchronisierung. Ohne sie kostet
  es rund 2 000 Zeilen Rust für einen Zustand, den das Frontend ohnehin als
  Ganzes kennt.
- _Fremdschlüssel auf `known_devices` mit `ON DELETE CASCADE`_: Löscht ein anderes
  Gerät eine `known_devices`-Zeile, wendet haex-crdt das mit abgeschalteten
  Fremdschlüsseln an (crdt:`engine.rs:103`, `delete_propagation.rs:213`); die
  Kaskade feuert nicht. Der Schlüssel brächte also nur Scheinsicherheit. Eine
  verwaiste Zeile schadet nicht, weil nur die eigene Gerätekennung gelesen wird.

## R2 Ganze Sitzung als Momentaufnahme statt einzelner Fenster

**Entscheidung**: Das Frontend schreibt die ganze Sitzung als ein JSON-Dokument
(`WmSession`, siehe data-model.md), entprellt um 400 ms und sofort bei
strukturellen Änderungen, wie bisher. Rust prüft nur Größe (≤ 4 MiB) und
gültiges JSON; die inhaltliche Prüfung beim Laden übernimmt `hydrate` in
`src/lib/wm/layoutState.ts`, das schon heute unbekannte Apps und doppelte
Einzelinstanzen verwirft und Geometrie einpasst (Spec 015 FR-025, FR-026).

**Begründung**: Ein Schreibvorgang ist eine Zeile; Löschen ist eine Zeile.
Die Arbeitsbereichs-Kennungen erzeugt künftig das Frontend
(`crypto.randomUUID()`), der Umweg über `wm_create_workspace` entfällt. Die
Sitzung enthält seit der Klärung vom 2026-09-26 auch die Vor-/Zurück-Historie
jedes Tabs (bis 50 Einträge). Typische Sitzungen haben trotzdem wenige
Kilobyte; für Extremfälle gilt die Obergrenze von 4 MiB mit dem Rückfall
„ohne Historien speichern“ (data-model.md). Weil Historien in der Sitzung
liegen, sind sie nur im Frontend vorhanden: Das bestätigt die Momentaufnahme
statt eines relationalen Modells.

**Verworfen**: Die inkrementellen Befehle aus 015 (`wm_save_windows`,
`wm_close_windows`, `wm_create_workspace`, `wm_delete_workspace`,
`wm_set_active_workspace`) beizubehalten. Sie setzen das relationale Modell aus
R1 voraus.

## R3 Einstellung: Präferenz mit Gerät vor Vault

**Entscheidung**: Schlüssel `wm.session_restore`, Werte `'true'`/`'false'` in der
bestehenden, synchronisierten Tabelle `preferences`, einmal mit Vault-Scope und
optional mit Geräte-Scope. Geltender Wert: Gerät, sonst Vault, sonst `false`
(FR-002, FR-003). Die Auflösung wird als kleine Erweiterung von
`src-tauri/src/storage/preferences.rs` gebaut (`get_scoped_bool`), die die
vorhandenen Funktionen `get` nutzt.

**Begründung**: Die Einstellung selbst soll synchronisiert werden (der
Vault-Wert gilt für alle Geräte), die Sitzung nicht. graphify-Abfrage
„resolve preference device overrides vault boolean“ (Graph-Stand `055a411`,
im Worktree unverändert genutzt) nennt `preferences.rs` als Kandidaten; den
Resolver für das Standardmodell (`chat/default_model.rs`) nicht, weil er eine
modellspezifische Kette mit Ladbarkeitsprüfung ist. Das Boolesche Muster
`'true'`/`'false'` folgt `voice.auto_send`.

## R4 Befehle: vier statt sechs

**Entscheidung**: Die sechs Befehle aus 015 entfallen. Neu (Vertrag in
[contracts/wm-session.md](./contracts/wm-session.md)):

| Befehl                   | Zweck                                                                                                                           |
| ------------------------ | ------------------------------------------------------------------------------------------------------------------------------- |
| `wm_session_restore_get` | Gerätewert, Vault-Wert, geltender Wert                                                                                          |
| `wm_session_restore_set` | einen der beiden Werte setzen oder zurücksetzen; löscht die gespeicherte Sitzung, wenn danach nichts mehr gilt (FR-005, FR-007) |
| `wm_session_load`        | beim Start: Einstellung plus Sitzung; löscht eine vorhandene Sitzung, wenn nichts gilt (FR-008)                                 |
| `wm_session_save`        | Momentaufnahme schreiben; tut nichts, wenn nichts gilt (Schutz gegen Wettläufe)                                                 |

**Begründung**: Setzen und Aufräumen in einem Befehl ist atomar in einer
Transaktion: Zwischen „ausschalten“ und „löschen“ kann kein Speichern
dazwischenkommen. `wm_session_save` prüft die Einstellung selbst, damit ein
verspätetes entprelltes Speichern nach dem Ausschalten nichts mehr anlegt. Die
Gerätekennung ermittelt Rust wie bisher selbst (`current_device_uuid` in
`storage/wm_commands.rs`); das Frontend übergibt keine.

## R5 Altdaten: Tabellen per Migration löschen, nicht Zeilen

**Entscheidung**: Migration `0020_wm_session_no_sync` in
`src-tauri/src/identity/migrations.rs`:

1. `DROP TABLE shell_window_tabs;` `DROP TABLE shell_windows;`
   `DROP TABLE workspaces;` — Kinder zuerst.
2. `CREATE TABLE wm_sessions_no_sync (…)`.
3. `CREATE TABLE haex_maintenance_no_sync (task TEXT PRIMARY KEY)` und
   `INSERT … VALUES ('vacuum_after_legacy_wm_drop')` für R6.

**Begründung**:

- `DROP TABLE` auf eine CRDT-Tabelle ist in einer App-Migration erlaubt; SQLite
  entfernt die Trigger mit der Tabelle, und das implizite Löschen feuert keine
  Trigger, es entsteht also kein Löschvermerk (crdt:`transformer/mod.rs:285`,
  `:311-318`; Agent-Recherche 2026-09-25).
- Ein `DELETE FROM` auf eine CRDT-Tabelle in einer Migration würde das Öffnen
  der Vault abbrechen: Migrationen laufen vor der HLC-Initialisierung
  (crdt:`src/database/mod.rs:123` vs. `:140`), der Delete-Trigger ruft
  `current_hlc()` auf und scheitert (crdt:`src/crdt/hlc.rs:257-259`). Deshalb
  die Reihenfolge Kinder vor Eltern: So löst `DROP TABLE workspaces` keine
  Kaskade in noch vorhandene CRDT-Kindtabellen aus.
- Andere Geräte haben die Tabellen, bis sie selbst aktualisieren. Schicken sie
  vorher Änderungen daran, überspringt haex-crdt sie als `MissingTable`, ohne den
  Sync abzubrechen (crdt:`engine.rs:201-205`).
- Löschvermerke, die 015 früher für geschlossene Fenster geschrieben hat,
  bleiben im Delete-Log. Sie tragen nur Tabellenname und Primärschlüssel
  (Gerätekennung plus zufällige Fenster-, Tab- oder Arbeitsbereichskennung) und
  sind nach FR-011 zulässig. Die neuen Tabellennamen unterscheiden sich von den
  alten, damit alte Vermerke nie neue Zeilen verdecken (crdt:`row.rs:135-143`).
- Kein Anheben von `HAEX_TRIGGER_VERSION`: Weder eine `_no_sync`-Tabelle noch
  das Löschen einer CRDT-Tabelle braucht Trigger-Neuinstallation
  (crdt:`db/init.rs:66-113`).

**Zusätzlich zur Laufzeit** (nach dem Öffnen, mit initialisiertem HLC): Der alte
Schlüssel `shell.active_workspace_id` in `preferences` wird für alle Geräte
gelöscht. Das erzeugt synchronisierte Löschvermerke mit Gerätekennung und
Schlüsselname, keinen Inhalt (FR-011). Der Schritt ist idempotent und läuft bei
jedem Öffnen, was FR-012 (erneuter Versuch) ohne Zusatzlogik erfüllt.

## R6 Keine Reste in der Datei: `secure_delete` und einmaliges `VACUUM`

**Entscheidung**:

- Nach jedem Öffnen der Vault setzt haex-vault auf der einen Verbindung
  (crdt:`src/database/mod.rs:79`, `Mutex<Connection>`) `PRAGMA secure_delete = ON`.
  Gelöschte Sitzungen werden dadurch beim Löschen mit Nullen überschrieben.
- Steht `vacuum_after_legacy_wm_drop` in `haex_maintenance_no_sync`, führt haex-vault
  einmal `VACUUM` und `PRAGMA wal_checkpoint(TRUNCATE)` aus und löscht den
  Eintrag danach. Schlägt es fehl, bleibt der Eintrag stehen und der nächste
  Start versucht es erneut (FR-012).

**Begründung**: Weder haex-crdt noch haex-vault setzen `secure_delete` oder
`VACUUM` (crdt:`src/db/core/init.rs:54-104`); gelöschte Inhalte bleiben sonst
in freien Seiten und im WAL, bis sie überschrieben werden. Die Datei ist mit
SQLCipher verschlüsselt, mit der Passphrase wären die Reste aber lesbar. SC-003
verlangt, dass eine Untersuchung der Datei keine Inhalte mehr findet. Die
Tabellen aus 0019 wurden in der Migration gelöscht, bevor `secure_delete`
greifen kann; deshalb das einmalige `VACUUM`. Bei typischen Vault-Größen
(einige MB) bleibt es unter der Sekunde aus SC-006; es läuft nur einmal.

**Verworfen**: `VACUUM` bei jedem Start (unnötige Last) und `auto_vacuum`
(müsste vor Anlage der Datei gesetzt sein).

## R7 Frontend: Speichern nur, wenn die Einstellung gilt

**Entscheidung**:

- `src/composables/useWmLayout.ts` wird zu `useWmSession.ts`: dieselbe
  serialisierte Warteschlange mit Entprellung (400 ms) und `flushAsync`, aber
  mit einem einzigen Befehl `wm_session_save` und der ganzen Momentaufnahme.
- Die Momentaufnahme liest die Historie jedes Tabs aus derselben Map. Auch
  Vor, Zurück und jede Navigation im Tab lösen `saveSessionSoon` aus, damit die
  gespeicherte Historie aktuell bleibt.
- Der Store (`src/stores/windowManager.ts`) hält `sessionRestore`
  (geltender Wert). Die bisherigen Speicheraufrufe (`persistWindowNow`,
  `persistWindowDebounced`) werden zu `saveSessionNow`/`saveSessionSoon`, die
  nichts tun, solange `sessionRestore` falsch ist.
- `hydrateFromBackendAsync` wird zu `restoreSessionAsync`: `wm_session_load`,
  bei Sitzung `hydrate` und die Historien der Tabs in die Historien-Map des
  Navigations-Stores (`src/stores/wmNavigation.ts`) übernehmen, statt sie wie
  bisher zu leeren; sonst ein leerer Arbeitsbereich. Fehler werden
  protokolliert, haex-vault startet leer (FR-012); die Workspace-Seite fängt den Fehler
  ab, damit `?open=` trotzdem wirkt (FR-014).
- Nach `wm_session_restore_set` übernimmt der Store den neuen geltenden Wert;
  wird er wahr, speichert er sofort (FR-005).

**Begründung**: Die Aufrufstellen im Store bleiben, nur ihr Ziel ändert sich.
Das hält die Änderung am Store klein (er steht bei 485 Zeilen).

**Umsetzung (2026-09-26)**: Die Logik liegt im reinen Modul
`src/lib/wm/sessionSync.ts`, damit sie unter Node testbar ist. Der Store hält
keinen eigenen `sessionRestore`-Ref, sondern delegiert (`restoreSessionAsync`,
`setSessionRestore`, `getSessionRestore`). Einstellungsänderungen laufen über
dieselbe Warteschlange wie das Speichern, statt wie geplant über
`applySessionRestore` nachträglich übernommen zu werden. Eine zu große Sitzung
meldet Rust als `HaexError::SessionTooLarge`.

## R8 Einstellungsansicht und Aktionen

**Entscheidung**: Neue Komponente
`src/components/settings/SessionRestoreSetting.vue` (Auto-Import
`SettingsSessionRestoreSetting`) nach dem Gerätenamen in `SettingsApp.vue`.
Sie zeigt Gerätewert, Vault-Wert und geltenden Wert, hat eine Auswahl „dieses
Gerät / alle Geräte“ wie `DefaultModelSetting.vue`, einen Schalter und
„Zurücksetzen“. Aktionen im Katalog (`src/lib/actions/settingsActions.ts`):
`settings.sessionRestore.set` (`{ scope, enabled }`) und
`settings.sessionRestore.clear` (`{ scope }`), Bereich `settings.device`, von
Agenten aufrufbar (keine Leitplanke, Spec-Annahme). `settings.get` meldet die
drei Werte mit.

**Begründung**: Folgt dem vorhandenen Muster (Spec 002, Spec 020 FR-024).

**Umsetzung (2026-09-26)**: Statt Anzeige, Schalter und „Zurücksetzen“ hat die
Ansicht eine einzige Auswahl „Aus / Nur auf diesem Gerät / Auf allen Geräten
dieser Vault“, die beim Wählen speichert (Betreiber-Rückmeldung: keine Knöpfe
zum Übernehmen; „Aus“ heißt aus auf allen Geräten). Die Zuordnung auf Geräte-
und Vault-Wert steht in FR-004; die
Aktionen bleiben `set`/`clear` je Scope.

## R9 Tests

- Rust: neue Tests für die vier Befehle (Einstellung aus → `load` löscht,
  `save` tut nichts; an → Rundreise; Gerät vor Vault; Zurücksetzen; Größengrenze;
  fremde Gerätezeilen unberührt) und für Migration 0020 nach dem Muster
  `migration_source_before` in `migrations_tests.rs` (frische und
  aktualisierte Vault: alte Tabellen weg, neue da, Wartungseintrag gesetzt;
  nach dem Öffnen ist er abgearbeitet).
- `check:wm-state`: `check-wm-persistence.ts` wird auf `useWmSession`
  umgestellt (Warteschlange, Entprellung, kein Speichern bei `sessionRestore`
  falsch, Umschalten speichert sofort).
- Die bisherigen Rust-Tests für `wm_windows`/`wm_workspaces`/`wm_commands`
  entfallen mit ihrem Code.
- Manuell: [quickstart.md](./quickstart.md).
