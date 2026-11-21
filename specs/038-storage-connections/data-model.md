# Data Model: Speicherverbindungen (S3)

**Feature**: [spec.md](./spec.md) | **Research**: [research.md](./research.md)

Migrationen (R3): `0027_passwords_owner` (`src-tauri/src/identity/migrations_passwords_owner.rs`, PR B) und
`0028_storage_connections` (`src-tauri/src/identity/migrations_storage.rs`, PR C). `HAEX_TRIGGER_VERSION`
wird jeweils erhöht (neue Spalte in einer CRDT-Tabelle, neue CRDT-Tabellen).

## haex_storage_connections (synchronisiert)

Eine Speicherverbindung (Spec: Begriffe, FR-001, FR-006).

| Spalte                | Typ  | Regel                                                                             |
| --------------------- | ---- | --------------------------------------------------------------------------------- |
| `id`                  | TEXT | PK, UUIDv4                                                                        |
| `provider_name`       | TEXT | Pflicht, 1–80 Zeichen, vom Nutzer (z. B. „Hetzner“); in der Liste der Erweiterung |
| `provider_kind`       | TEXT | `aws`, `rustfs`, `other` (Vorbelegung von Endpunkt und Adressierung)              |
| `endpoint`            | TEXT | `https://…` oder `http://…` nur für lokale Adressen (R8); bei `aws` leer erlaubt  |
| `endpoint_scope`      | TEXT | `public` oder `local`, beim Festlegen des Endpunkts (R8, FR-009b)                 |
| `region`              | TEXT | Pflicht, 1–64 Zeichen                                                             |
| `addressing`          | TEXT | `path` oder `virtual`                                                             |
| `credentials_item_id` | TEXT | Kennung des Eintrags im Passwortmanager (`owner = 'storage'`), Pflicht            |
| `created_at`          | TEXT | ISO 8601                                                                          |
| `updated_at`          | TEXT | ISO 8601                                                                          |

Kein Fremdschlüssel auf den Eintrag (anderer Bereich, Löschen über den Dienst von 034). Zustand der
Zugangsdaten (`ConnectionView.credentials`, Review 2026-10-06, statt `credentialsMissing: bool`):

- `present`: der Eintrag ist da.
- `missing` („neue Zugangsdaten nötig“): der Eintrag fehlt, und `haex_deleted_rows` hat einen
  Löschvermerk für ihn (vom Nutzer gelöscht).
- `syncing` („wird synchronisiert“, Edge Case): der Eintrag fehlt ohne Löschvermerk, ist also noch nicht
  angekommen. Ist ein Vermerk schon aufgeräumt (nach 90 Tagen, Spec 024 R20), zeigt haex-vault weiter
  `syncing`; der Hinweis bietet dort auch das Eingeben neuer Zugangsdaten an.

Ein Aufruf einer Erweiterung scheitert bei `syncing` mit 2002 `network` (Edge Case), bei `missing` mit
2002 `accessDenied`.

## haex_storages (synchronisiert)

Ein Speicher = ein Bucket auf einer Verbindung (FR-002). Ziel der Berechtigung `remoteStorage`.

| Spalte          | Typ  | Regel                                                                 |
| --------------- | ---- | --------------------------------------------------------------------- |
| `id`            | TEXT | PK, UUIDv4; ist die `backendId` des SDK und das Ziel der Berechtigung |
| `connection_id` | TEXT | → `haex_storage_connections.id`                                       |
| `name`          | TEXT | Pflicht, 1–80 Zeichen                                                 |
| `bucket`        | TEXT | Pflicht, Bucket-Name nach S3 (3–63, Kleinbuchstaben, Ziffern, `.-`)   |
| `created_at`    | TEXT | ISO 8601                                                              |
| `updated_at`    | TEXT | ISO 8601                                                              |

Entfernen eines Speichers: Zeile löschen und die Berechtigungen aller Erweiterungen mit diesem Ziel
(`extension_permissions`, Art `remoteStorage`, Ziel = `id`) im selben Schreibvorgang (FR-007). Entfernen
einer Verbindung: alle ihre Speicher wie eben, dann die Verbindung, dann den Eintrag der Zugangsdaten über
den Dienst von 034 (`Caller::Internal { feature: "storage" }`), endgültig (nicht Papierkorb), wenn keine
andere Verbindung denselben Eintrag nutzt.

## storage_tests_no_sync (geräteeigen)

Letztes Testergebnis je Speicher auf diesem Gerät (FR-004, Edge Case „neue Zugangsdaten nötig“).

| Spalte       | Typ  | Regel                                                                    |
| ------------ | ---- | ------------------------------------------------------------------------ |
| `storage_id` | TEXT | PK                                                                       |
| `tested_at`  | TEXT | ISO 8601                                                                 |
| `outcome`    | TEXT | `passed`, `accessDenied`, `unreachable`, `bucketMissing`, `missingRight` |

Wird auch von einem Aufruf einer Erweiterung mit `accessDenied` gesetzt, damit die Einstellungen den
Hinweis zeigen.

## haex_passwords_item_details.owner (neue Spalte, Migration 0027)

| Spalte  | Typ  | Regel                                                                          |
| ------- | ---- | ------------------------------------------------------------------------------ |
| `owner` | TEXT | `NULL` = Eintrag des Nutzers; sonst Name einer haex-vault-Funktion (`storage`) |

Regel Z14 (R2, Änderung an `specs/034-password-manager/contracts/access.md`): Ein Eintrag mit `owner` ist
für jeden Aufrufer außer `User` und `Internal { feature }` mit `feature == owner` `NotFound`, fehlt in
`list_headers` und den Agent-Kopfdaten und gilt beim Auflösen von Verweisen als nicht sichtbar.
`ItemState` bekommt dafür `owner: Option<&str>`. Nur haex-vault setzt `owner` (beim Anlegen über
`create_item` eines `Internal`-Aufrufers); kein Command und keine Bridge-Methode kann ihn setzen oder
ändern.

## Zugangsdaten-Eintrag

| Feld im Eintrag             | Inhalt                                       |
| --------------------------- | -------------------------------------------- |
| `title`                     | `S3: <provider_name>`                        |
| `username`                  | Zugangsschlüssel (Access Key ID)             |
| `password`                  | Geheimnis (Secret Access Key)                |
| eigenes Feld `sessionToken` | optional                                     |
| `url`                       | Endpunkt (für den Nutzer im Passwortmanager) |
| `owner`                     | `storage`                                    |

## Abgeleitet, nicht gespeichert

- **Bereich einer Erweiterung**: `haex-ext/<vault_id>/<extension_id>/`, für eine Entwicklerversion
  `haex-ext-dev/<vault_id>/<dev_extension_id>/`; `vault_id` aus `vault_identity.pubkey` (R4, Review
  2026-10-06).
- **Liste für eine Erweiterung** (FR-009a): `{ id, type: "s3", name, providerName, bucket }` je Speicher,
  den eine Leseberechtigung deckt.

## Zustände eines Speichers in den Einstellungen

`ungetestet` → (Test) → `bereit` | `Fehler <Grund>`; ein Aufruf mit `accessDenied` setzt `Fehler
accessDenied` (Hinweis „neue Zugangsdaten nötig“); neue Zugangsdaten → erneuter Test. Unabhängig davon
zeigt der Speicher den Zustand der Zugangsdaten seiner Verbindung (`syncing` oder `missing`, siehe oben).
