# Data Model: Versionsstände der Geräte

## `device_presence_no_sync` (Spec 024, gerätelokal) — drei Spalten neu

Migration `0040_device_peer_schema` (gleichzeitig der Migrationsstand 40 im Vergleich beim Handshake):

| Spalte           | Typ          | Regeln                                                                       |
| ---------------- | ------------ | ---------------------------------------------------------------------------- |
| `peer_protocol`  | INTEGER NULL | `SchemaVersion.protocol` des Geräts, wie es sich zuletzt im Handshake nannte |
| `peer_migration` | INTEGER NULL | `SchemaVersion.holzi_migration`                                              |
| `peer_trigger`   | INTEGER NULL | `SchemaVersion.crdt_trigger`                                                 |

```sql
ALTER TABLE device_presence_no_sync ADD COLUMN peer_protocol INTEGER;
--> statement-breakpoint
ALTER TABLE device_presence_no_sync ADD COLUMN peer_migration INTEGER;
--> statement-breakpoint
ALTER TABLE device_presence_no_sync ADD COLUMN peer_trigger INTEGER;
```

Regeln:

- Alle drei `NULL` heißt „Stand unbekannt“. Ein Schreiben setzt alle drei zugleich; steht nur ein Teil da
  (Fehler), liest die Anzeige „unbekannt“.
- Geschrieben wird bei jedem Handshake mit dem Gerät, das die geltende Liste für den Endpunkt nennt (R3), und
  nur dann; ein unbekannter Endpunkt legt keine Zeile an.
- Das Schreiben ersetzt nur diese drei Spalten; `last_seen`, `endpoint_addr` und `problem` bleiben.
- Die Zeile eines entfernten Geräts wird wie bisher gelöscht (`removal.rs`).
- Keine CRDT-Tabelle, kein Eintrag in der Änderungsverfolgung, `HOLZI_TRIGGER_VERSION` bleibt 27.

## `SchemaVersion` (Wire, `wire.rs`)

Unverändert: `{ protocol: u32, holzi_migration: u32, crdt_trigger: u32 }`. `protocol` kommt aus
`PROTOCOL_VERSION`, die jetzt 2 ist.

## `VaultDevice` (Befehl `list_vault_devices`, `device_view.rs`)

Neu: `schema: DeviceSchema | null`.

```text
DeviceSchema { protocol: number, holziMigration: number, crdtTrigger: number }
```

- Bei `isCurrent` der Stand des laufenden holzi (`local_schema()`), nie `null`.
- Bei anderen Geräten der gespeicherte Stand oder `null`.
- Die TS-Typen entstehen über ts-rs nach `src/types/bindings/` (`DeviceSchema.ts`, `VaultDevice.ts`).

## Zustände (Anzeige, rein, `deviceStatus.ts`)

`schemaDiff(own, other)` → je Wert `equal | lower | higher` oder `unknown`, wenn `other` fehlt. „Lower“ heißt: der
Wert des anderen Geräts ist kleiner als der dieses Geräts.
