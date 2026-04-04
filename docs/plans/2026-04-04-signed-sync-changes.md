# ADR: Kryptographisch signierte Sync-Changes

**Status:** Geplant
**Datum:** 2026-04-04
**Kontext:** Capability-Enforcement bei CRDT-Sync

## Problem

Aktuell werden CRDT-Changes unsigniert über den Sync-Server und QUIC übertragen. Es gibt keine Möglichkeit zu verifizieren:
1. **Wer** eine Änderung gemacht hat (Authentizität)
2. **Ob** die Änderung manipuliert wurde (Integrität)
3. **Ob** der Absender die nötige Berechtigung hatte (Autorisierung)

### Angriffsvektoren
- Kompromittierter Sync-Server kann Änderungen injizieren oder manipulieren
- Man-in-the-Middle auf dem Transportweg kann Changes verfälschen
- Read-Only Peer kann auf QUIC-Ebene Writes senden (gefixt in leader.rs, aber nicht end-to-end abgesichert)

## Entscheidung

**Jeder CRDT-Change wird vom Absender signiert.** Der Empfänger verifiziert die Signatur und prüft die UCAN-Capability bevor er die Änderung anwendet.

## Design

### 1. Change-Format erweitern

Aktuell (`RemoteColumnChange`):
```rust
pub struct RemoteColumnChange {
    pub table_name: String,
    pub row_pks: String,
    pub column_name: String,
    pub hlc_timestamp: String,
    pub batch_id: String,
    pub batch_seq: usize,
    pub batch_total: usize,
    pub decrypted_value: JsonValue,
}
```

Erweitert um:
```rust
pub struct RemoteColumnChange {
    // ... bestehende Felder ...
    pub author_did: String,        // DID des Absenders
    pub signature: String,         // Ed25519 Signatur über den Change-Hash
    pub content_hash: String,      // SHA-256 Hash über (table_name, row_pks, column_name, hlc_timestamp, value)
}
```

### 2. Signatur-Flow

**Sender (bei jedem CRDT-Write):**
1. Change-Daten serialisieren → deterministisches JSON (sortierte Keys)
2. SHA-256 Hash über das serialisierte JSON → `content_hash`
3. `content_hash` mit DID-Private-Key signieren → `signature`
4. `author_did` + `signature` + `content_hash` dem Change hinzufügen

**Empfänger (bei apply_remote_changes_to_db):**
1. `content_hash` aus den Change-Daten neu berechnen
2. Berechneten Hash mit mitgeliefertem `content_hash` vergleichen (Integrität)
3. `signature` mit `author_did`'s Public Key verifizieren (Authentizität)
4. UCAN-Capability für `author_did` + `space_id` prüfen (Autorisierung)
5. Nur bei Success: Change anwenden

### 3. Sync-Server Schema

`sync_changes` Tabelle erweitern:
```sql
ALTER TABLE sync_changes ADD COLUMN author_did TEXT;
ALTER TABLE sync_changes ADD COLUMN signature TEXT;
ALTER TABLE sync_changes ADD COLUMN content_hash TEXT;
```

Server prüft beim Push:
1. `author_did` muss zum authentifizierten User gehören
2. `signature` muss gültig sein
3. Capability wird wie bisher über `requireCapability` geprüft

### 4. Betroffene Komponenten

| Komponente | Änderung |
|-----------|----------|
| `crdt/commands.rs` | `execute_with_crdt` signiert jeden Change |
| `crdt/commands.rs` | `apply_remote_changes_to_db` verifiziert Signatur + Capability |
| `space_delivery/local/leader.rs` | SyncPush verifiziert Signaturen |
| `space_delivery/local/sync_loop.rs` | Pull verifiziert Signaturen |
| Frontend: Sync-Push TypeScript | `author_did` + `signature` mitsenden |
| Frontend: Sync-Pull TypeScript | Signatur verifizieren vor `apply_remote_changes_in_transaction` |
| haex-sync-server: `sync.ts` | `author_did`, `signature`, `content_hash` Spalten speichern/validieren |
| haex-sync-server: Schema | Migration für neue Spalten |

### 5. Migration / Backward Compatibility

- Changes ohne Signatur werden während der Migration akzeptiert (grace period)
- Nach der Migration: unsignierte Changes werden rejected
- Bestandsdaten in `sync_changes` werden nicht nachträglich signiert (nur neue Changes)

### 6. Performance-Überlegungen

- Ed25519 Signatur: ~60µs pro Signatur, ~30µs pro Verify → vernachlässigbar
- SHA-256 Hash: ~1µs pro Change → vernachlässigbar
- Zusätzliche DB-Queries für Capability-Check: 1 Query pro unique `author_did` im Batch
- Batch-Optimierung: Capability-Cache pro `author_did` innerhalb eines Batches

## Implementierungs-Reihenfolge

1. **Phase 1:** `content_hash` + `signature` Felder zum Change-Format hinzufügen (Rust + TS)
2. **Phase 2:** Sender signiert alle Changes
3. **Phase 3:** Empfänger verifiziert Signatur (Warnung bei fehlender Signatur)
4. **Phase 4:** Sync-Server speichert/validiert Signaturen
5. **Phase 5:** Fehlende Signatur wird rejected (kein Grace Period mehr)

## E2E Tests

Folgende Tests werden benötigt:
- Signierter Change wird akzeptiert
- Change mit ungültiger Signatur wird rejected
- Change mit gültiger Signatur aber ohne Capability wird rejected
- Change von unbekanntem DID wird rejected
- Manipulierter content_hash wird erkannt
- Batch mit gemischten Autoren (einige autorisiert, einige nicht) wird komplett rejected
