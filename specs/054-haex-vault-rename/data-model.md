# Data Model: Umbenennung von holzi in haex-vault

Die Umbenennung führt keine neuen Daten ein. Dieses Dokument beschreibt die **drei Listen**, auf
denen sie beruht, und die Daten, die der alte Vault trägt.

## Namensregel (NameRule)

Eine Regel macht aus einer Schreibweise des alten Namens die neue. Sie steht vollständig in
[contracts/naming-map.md](contracts/naming-map.md).

| Feld         | Bedeutung                                                                 |
| ------------ | ------------------------------------------------------------------------- |
| `family`     | Art der Fundstelle (Crate, Typ, Umgebungsvariable, Text, Schema, …)       |
| `old`        | Muster der alten Schreibweise (`holzi_…`, `HOLZI_…`, `Holzi…`, `holzi-…`) |
| `new`        | Ergebnis in der Konvention der Umgebung                                   |
| `exceptions` | benannte Sonderfälle, die die Regel nicht trifft                          |
| `stage`      | Umsetzungsstufe, in der die Regel angewendet wird (S2–S7)                 |

Regeln sind **vollständig und überschneidungsfrei**: Jede Fundstelle fällt in genau eine Familie
oder in die Ausnahmeliste. Eine Fundstelle, die in keine fällt, ist ein Fehler im Regelwerk und
wird dort ergänzt, nicht an Ort und Stelle entschieden.

## Persistierte Namen (PersistedName)

Namen, die **in Vault-Daten oder auf dem Netz** vorkommen. Nur sie können eine alte Datei oder ein
altes Gerät betreffen. Das Inventar ist aus den Quellen abgeleitet (research.md, R3) und wird in S1
durch den Testvault bestätigt.

| Familie         | Fundstelle                                                                                          | Wirkung auf die alte Datei                        |
| --------------- | --------------------------------------------------------------------------------------------------- | ------------------------------------------------- |
| Tabelle         | `holzi_maintenance_no_sync` (`identity/migrations.rs`, `storage/maintenance.rs`)                    | Abfrage nach neuem Namen scheitert, Tabelle fehlt |
| Kontaktart      | `holzi_vault`, `holzi_nostr_relay`, `holzi_iroh` (`contacts/kinds.rs`, `src/lib/contacts/kinds.ts`) | alte Zeilen tragen unbekannte Werte               |
| Herkunft        | `ContactOriginKind::Holzi`, serialisiert `holzi`                                                    | alte Zeilen tragen unbekannten Wert               |
| UUID            | `holzi:provider/local/chat`, `…/transcription` (`providers/local.rs`)                               | zusätzliche Provider-Zeilen                       |
| Signatur-Domäne | `holzi-device-auth/v1` u. a., acht Werte (`sync/signing.rs`)                                        | gespeicherte Signaturen prüfen nicht mehr         |
| Schlüssel-ID    | `holzi-key-id/v1` (`sync/content_keys.rs`)                                                          | gespeicherte IDs weichen ab                       |
| Ableitung       | Salz `holzi`, `holzi/vault-identity/v1` (`sync/keys.rs`)                                            | nur eine neue Ableitung weicht ab                 |
| Ableitung       | `holzi/device-name/v1`, `holzi/presence/v1`, `holzi/link/*`                                         | verschlüsselte Gerätenamen, Presence, Pairing     |
| Netz            | ALPN `holzi-sync/1`, `holzi-link/1` (`sync/wire.rs`)                                                | alte und neue Geräte verbinden sich nicht         |
| Präfix          | Visitenkarte `holzi:card:` (`contacts/card/format.rs`)                                              | alte Karten werden nicht mehr gelesen             |
| Präfix          | `holzi-hf-model-id-v1` (`models/huggingface.rs`)                                                    | codierte Modell-IDs in alten Zeilen               |

Jede Zeile ist Gegenstand genau eines Tests in S3 (feste Werte) oder S2 (Namen in Daten) und eines
Erwartungswerts im Test des Testvaults (S1).

## Ausnahmeliste (AllowEntry)

Orte, an denen der alte Name stehen darf. Sie ist **abschließend** (FR-004) und liegt als Datei im
Repo (Format: [contracts/old-name-gate.md](contracts/old-name-gate.md)).

| Feld      | Bedeutung                                                                                                   |
| --------- | ----------------------------------------------------------------------------------------------------------- |
| `kind`    | `path` (Pfadmuster), `pathname` (nur der Name des Pfads, nicht der Inhalt) oder `line` (Muster einer Zeile) |
| `pattern` | Glob oder Regex, je nach `kind`                                                                             |
| `reason`  | Pflichtfeld: Satz, warum der Name dort bleibt                                                               |
| `until`   | optional: Stufe, in der der Eintrag entfällt (S10 für die Constitution)                                     |

Anfangsinhalt (nach S8):

| Muster                                                      | kind     | Grund                                                | until |
| ----------------------------------------------------------- | -------- | ---------------------------------------------------- | ----- |
| `specs/054-haex-vault-rename/**`                            | path     | Die Spec beschreibt die Umbenennung.                 | –     |
| `specs/032-model-operates-holzi`                            | pathname | Verzeichnisname bleibt (FR-018).                     | –     |
| `.spaex/**`                                                 | path     | Vom spaex-Werkzeug aus den atoms erzeugt.            | S10   |
| `.specify/memory/constitution.md`                           | path     | Wie `.spaex`, vom Werkzeug erzeugt.                  | S10   |
| `scripts/ci/check-old-name.py`                              | path     | Gate-Implementierung enthält das Suchmuster.         | –     |
| `scripts/ci/test_check_old_name.py`                         | path     | Gate-Testfälle enthalten das Suchmuster.             | –     |
| `scripts/ci/old-name-allowlist.txt`                         | path     | Gate-Konfiguration enthält Suchmuster und Ausnahmen. | –     |
| README, `CONTEXT.md`: der eine Hinweissatz („früher holzi“) | line     | Name als Vergangenheit (FR-004).                     | –     |

Git-Historie und der Tag `holzi-final` sind keine verfolgten Dateien und liegen außerhalb der
Prüfung.

## Alte Vault-Datei (PreRenameVault)

Eine SQLCipher-Datei im Stand von `holzi-final`; Quelle der einzigen Übernahme.

| Teil                 | Inhalt                                                    | Rolle beim Import                                     |
| -------------------- | --------------------------------------------------------- | ----------------------------------------------------- |
| `<name>.db`          | Daten: Passwörter, Kontakte, Provider, Geräte, Sync-Daten | Quelle                                                |
| `<name>.db.vault-id` | Kennung des Vaults, von `vault_id::write`                 | vom Import neu geschrieben                            |
| `-wal`, `-shm`       | Journal                                                   | werden beim Schließen der App in die Datei übernommen |

Zwei Ausprägungen: der **Testvault** (`src-tauri/tests/fixtures/pre_rename_vault/`, erfundene
Inhalte, im Repo) und die **echte Datei des Betreibers** (nie im Repo; geprüft an einer Kopie,
Sicherung unverändert).

Zustände nach dem Öffnen in haex-vault 3:

```text
geöffnet → Passwörter lesbar → (Sync-Daten prüfbar | Sync-Daten nicht prüfbar, als Zustand angezeigt)
geöffnet → Abbruch   ⇒ Rückfall FR-014 (Passwort-Export vor dem Umzug)
```
