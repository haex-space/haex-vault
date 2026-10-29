# Datenmodell: Sync-Regeln

Alle Tabellen kommen mit der Migration `0044_sync_rules` (Nummer siehe research.md R7, Risiko 3), liegen in der
Vault-Datenbank, enden auf `_no_sync` (kein Sync-Paket, keine Trigger-Version) und tragen
`vault_device_uuid NOT NULL REFERENCES known_devices(vault_device_uuid) ON DELETE CASCADE`, damit eine kopierte
Datenbankdatei auf einem anderen Gerät keine fremden Regeln anwendet (ADR 0001, letzter Punkt; ADR 0018).

## `haex_sync_rules_no_sync` — Regel

| Spalte                             | Typ       | Hinweis                                                                                                                                                                |
| ---------------------------------- | --------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `id`                               | TEXT PK   | UUID v4                                                                                                                                                                |
| `vault_device_uuid`                | TEXT      | FK, Gerät, das die Regel führt                                                                                                                                         |
| `name`                             | TEXT      | nicht leer, höchstens 80 Zeichen                                                                                                                                       |
| `enabled`                          | INTEGER   | 1 aktiv, 0 pausiert                                                                                                                                                    |
| `place_a_source`, `place_b_source` | TEXT      | `SourceRef` wie im Fenster: `device` oder Speicher-ID (`<id>`, `peer:<Gerät>`); nie `share:<id>`. Ein verschlüsselter Ordner ist Quelle plus Fensterpfad (research R1) |
| `place_a_path`, `place_b_path`     | TEXT      | Ordnerpfad der Quelle (Gerät: absolut; Speicher: Fensterpfad wie `/Fotos`, Wurzel `/`)                                                                                 |
| `direction`                        | TEXT      | `aToB`, `both`                                                                                                                                                         |
| `delete_mode`                      | TEXT      | `mirror`, `keep`, `ask`                                                                                                                                                |
| `interval_secs`                    | INTEGER   | 60 bis 86 400; Vorgabe 900                                                                                                                                             |
| `created_at`, `updated_at`         | TEXT      | ISO 8601                                                                                                                                                               |
| `last_run_at`                      | TEXT NULL | Ende des letzten Laufs                                                                                                                                                 |
| `last_outcome`                     | TEXT NULL | `done`, `partial`, `waiting`, `error`, `cancelled`                                                                                                                     |
| `last_error`                       | TEXT NULL | Grund bei `error`/`waiting`                                                                                                                                            |

Validierung (`rules::validate`): Orte nicht gleich; keine Überlappung mit einer anderen Regel dieses Geräts (der
Pfad eines Orts liegt nicht in dem eines anderen Orts derselben Quelle, auch nicht über Kreuz A/B); Quelle ist
keine Freigabe (`share:`) und nicht der Datenbereich von holzi; Intervall im Bereich. `place_a` ist der Quellort,
`place_b` der Zielort, auch bei `both`; eine einseitige Regel (`aToB`) kopiert immer vom Quellort zum Zielort.

## `haex_sync_rule_files_no_sync` — Stand

| Spalte                 | Typ          | Hinweis                                                                                                                             |
| ---------------------- | ------------ | ----------------------------------------------------------------------------------------------------------------------------------- |
| `rule_id`              | TEXT         | FK auf Regel, `ON DELETE CASCADE`                                                                                                   |
| `vault_device_uuid`    | TEXT         | FK                                                                                                                                  |
| `path`                 | TEXT         | relativ zum Ort, `/` als Trenner, Namen so, wie der Ort sie meldet (keine Unicode-Normalisierung)                                   |
| `a_size`, `a_mtime_ms` | INTEGER NULL | Stand von Seite A nach dem letzten Erfolg; NULL = Datei dort nicht vorhanden                                                        |
| `b_size`, `b_mtime_ms` | INTEGER NULL | dito für Seite B                                                                                                                    |
| `kept`                 | INTEGER      | 1 = Löschmodus „Behalten“ hat sie bewusst stehen lassen                                                                             |
| `hash`                 | TEXT NULL    | SHA-256 des Inhalts (hex), sobald holzi ihn kennt (Kontrolle nach dem Kopieren, erster Abgleich); bestätigt Verschiebungen (FR-009) |

PK `(rule_id, path)`. Ein Wechsel von Orten oder Richtung löscht alle Zeilen der Regel.

## `haex_sync_rule_pending_no_sync` — anstehende Löschungen (Löschmodus „Nachfragen“)

| Spalte                         | Typ  | Hinweis                                           |
| ------------------------------ | ---- | ------------------------------------------------- |
| `rule_id`, `vault_device_uuid` | TEXT | FK                                                |
| `path`                         | TEXT | relativ                                           |
| `side`                         | TEXT | `a` oder `b`: Seite, auf der gelöscht werden soll |
| `found_run_id`                 | TEXT | Lauf, der sie gefunden hat                        |

PK `(rule_id, path)`. Die Zeile verschwindet bei Bestätigen (Löschen folgt im nächsten Lauf oder sofort), bei
Verwerfen (Datei bleibt, der Stand wird auf „behalten“ gesetzt oder die Datei zurückkopiert, je nach Wahl) oder
wenn die Datei wieder auftaucht.

## `haex_sync_runs_no_sync` — Läufe des Protokolls

| Spalte                                                         | Typ       | Hinweis                                  |
| -------------------------------------------------------------- | --------- | ---------------------------------------- |
| `id`                                                           | TEXT PK   | UUID                                     |
| `rule_id`, `vault_device_uuid`                                 | TEXT      | FK; `rule_id` mit `ON DELETE CASCADE`    |
| `started_ms`, `duration_ms`                                    | INTEGER   |                                          |
| `cause`                                                        | TEXT      | `interval`, `watch`, `manual`, `startup` |
| `outcome`                                                      | TEXT      | wie `last_outcome`                       |
| `error`                                                        | TEXT NULL |                                          |
| `copied`, `moved`, `deleted`, `conflicts`, `errors`, `skipped` | INTEGER   | Zähler                                   |

Nach jedem Lauf behält `run_log::trim(rule_id, depth)` die jüngsten `depth` Läufe; bei `depth = 0` wird nichts
geschrieben. Eine Verkleinerung der Tiefe führt `trim` für alle Regeln sofort aus (FR-044).

## `haex_sync_run_items_no_sync` — Dateien eines Laufs

| Spalte       | Typ       | Hinweis                                                            |
| ------------ | --------- | ------------------------------------------------------------------ |
| `run_id`     | TEXT      | FK `ON DELETE CASCADE`                                             |
| `seq`        | INTEGER   | Reihenfolge im Lauf                                                |
| `path`       | TEXT      | relativ                                                            |
| `action`     | TEXT      | `copy`, `move`, `delete`, `conflictCopy`, `adopt`, `skip`, `error` |
| `side`       | TEXT      | `a` oder `b`: Seite, auf der etwas geschah                         |
| `detail`     | TEXT NULL | Grund bei `skip`/`error`; Ziel bei `move`; Konfliktname            |
| `trash_kind` | TEXT NULL | `system` oder `rule` bei gespiegelter Löschung                     |
| `trash_path` | TEXT NULL | Pfad im Regel-Papierkorb (nur `rule`)                              |

PK `(run_id, seq)`. Für jede Dateiaktion wird ein Eintrag geschrieben, damit jede Datei eines noch gespeicherten
Laufs auffindbar bleibt. Der Speicherbedarf wächst daher mit Protokolltiefe mal Dateien je Regel; die Einstellung
der Protokolltiefe ist die Begrenzung für die Datenbankgröße. Das Protokoll enthält keine Dateiinhalte (FR-045).

## Laufzeitzustand (nicht gespeichert)

- `FileSyncState`: je Regel `Steuerung` (Zeitgeber, optional `FolderWatch`, „fällig“-Vermerk, Abbruch-Token des
  Laufs). Beim Sperren der Vault weg.
- Statusableitung (research.md R9) aus `last_outcome` und der Steuerung. Der Frontend-Zustand umfasst
  `active`, `running`, `waiting`, `error`, `paused` und `inactive` (letzteres bedeutet „holzi nicht aktiv").

## Einstellung

`storage/preferences.rs`, `PrefScope::Device`, Schlüssel `syncRules.logDepth`, ganze Zahl 0 bis 100 als Text,
Vorgabe 10 wenn fehlend.

## Typen für das Frontend (ts-rs, Präfix `SyncRule`)

`SyncRule`, `SyncRuleDraft`, `SyncRulePlace { source: SourceRef | { kind: 'storage', storageId }, path }`,
`SyncRuleDirection`, `SyncRuleDeleteMode`, `SyncRuleStatus`, `SyncRulePlan`, `SyncRulePlanStep`,
`SyncRulePending`, `SyncRuleRun`, `SyncRuleRunItem`, `SyncRuleIgnore`, `SyncRuleError`. Das Präfix verhindert
die stille Überschreibung fremder Bindings (Namensregel für `#[ts(export)]`).

## Zustandsübergänge einer Regel

```text
angelegt ──► aktiv ──► läuft ──► aktiv         (Ergebnis done/partial)
              │          ├────► wartet ──► aktiv  (Ort wieder erreichbar)
              │          └────► Fehler  ──► aktiv  (Regel geändert oder behoben)
              ├─► pausiert ◄──► aktiv
              ├─► holzi nicht aktiv ──► aktiv     (Vault/Anwendung wieder aktiv)
              └─► gelöscht (Zeilen aller Tabellen weg; Dateien beider Orte unberührt)
```
