# Phase 6: QUIC/iroh als P2P Transport-Layer für lokale Spaces

> Datum: 2026-03-30
> Status: Design validiert, Implementierung ausstehend

## Überblick

Lokale Spaces nutzen das Admin-Gerät als **Full Local Server**. Es übernimmt drei Rollen des Sync-Servers:
1. **MLS Delivery Service** (KeyPackages, Message Ordering, Welcome Delivery)
2. **CRDT Sync Hub** (Push/Pull von Changes)
3. **Realtime Notifications** (Push über QUIC Stream)

Transport: iroh/QUIC — das bestehende `peer_storage` Protokoll wird um neue Message-Types erweitert.

### Abgrenzung zu peer_storage

| Feature | peer_storage | space_delivery |
|---------|-------------|----------------|
| Zweck | Dateien streamen | Daten synchronisieren |
| Daten | Große Dateien (Fotos, Videos) | Kleine CRDT Changes + MLS Messages |
| Verschlüsselung | Transport-Level (QUIC) | MLS Epoch Keys |
| Persistenz | Kein Buffering | Leader buffert für offline Peers |

Beide teilen sich den gleichen iroh Endpoint.

## Design-Entscheidungen

### 1. Full Local Server (nicht Thin Relay)

Admin-Gerät speichert KeyPackages, buffert Messages, hält CRDT-Changes vor. Peers können offline gewesene Änderungen nachholen wenn sie sich mit dem Admin verbinden. MLS braucht Message Ordering — das geht nur zentral.

### 2. Priorisierte Leader Election

- User vergibt jedem Gerät eine Prioritätsstufe (1 = höchste, Default: 10)
- Online-Gerät mit niedrigster Priorität wird Leader
- Bei gleicher Priorität: deterministisch nach EndpointId
- Höher priorisiertes Gerät kommt online → Graceful Handoff nach ~10s Cooldown
- Laufende MLS-Operationen werden abgeschlossen bevor Leadership übergeht

### 3. Kein Buffer-Transfer bei Leader-Wechsel

Bei ungraceful Ausfall (Laptop zu, Akku leer, Netz weg):
- Neuer Leader startet mit leerem Buffer
- System heilt sich selbst:
  - Pending Commits werden bei Bedarf neu erzeugt
  - KeyPackages neu uploaded
  - Invites bei Bedarf wiederholt
- MLS Messages die nicht zugestellt wurden: Peers die online waren haben sie bereits, offline Peers haben sie sowieso verpasst

### 4. Pending Commit Pattern (Crash Recovery)

MLS Commits sind mehrstufig (erstellen → senden → Peers verarbeiten). Bei Crash:
- Leader speichert Commits als "pending" vor dem Senden
- Beim nächsten Start: Pending Commits erneut senden
- Peers verarbeiten Duplikate idempotent (gleiche Epoch + gleicher Commit = ignorieren)

### 5. Discovery: CRDT + mDNS kombiniert

- Leader-Zuordnung (Prioritäten) steht im CRDT (`haexSpaceDevices`)
- Tatsächliche Erreichbarkeit per mDNS (iroh built-in)
- Peer schaut in DB: "Gerät X mit Prio 1 soll Leader sein" → sucht per mDNS
- Fallback: nächstes Gerät aus Prio-Liste das per mDNS sichtbar ist

### 6. Invite Flow

- Admin erstellt Invite → Invite enthält EndpointId des Leaders + spaceId + inviteToken
- Invitee verbindet sich direkt zum Leader per iroh
- Ab dann identischer Flow wie Server-Spaces (Pending → Accept → KeyPackage Upload → Finalize → Welcome)
- Kein separater Discovery-Schritt für Invitee nötig

### 7. Selective Identity Disclosure

Bei Verbindung zum Leader:
- **Pflicht:** DID + EndpointId (technisch notwendig für Auth)
- **Optional:** Label + beliebige Claims aus `haex_identity_claims`
- Peer konfiguriert pro Space welche Claims er teilen möchte
- Privacy by Design: Peer kontrolliert was er preisgibt

### 8. Space-Typ immutable + Vault-Space geschützt

- Space-Typ (local/shared) kann nach Erstellung nicht geändert werden
- Vault-Space (`type: 'vault'`):
  - Wird aus Spaces-Liste gefiltert
  - Keine Invites möglich
  - Nicht löschbar über Spaces-UI
  - Vault-Daten werden über Sync-Settings verwaltet

## Protokoll-Design

Erweiterung des bestehenden `peer_storage` JSON-Protokolls (length-prefixed JSON über QUIC). Gleicher ALPN (`haex-peer/1`), gleicher Endpoint.

### Request/Response (einzelne QUIC-Streams)

```
MLS_UPLOAD_KEY_PACKAGES  { space_id, packages: [bytes] }             → { ok }
MLS_FETCH_KEY_PACKAGE    { space_id, target_did }                    → { package: bytes }
MLS_SEND_MESSAGE         { space_id, message: bytes, type }          → { message_id }
MLS_FETCH_MESSAGES       { space_id, after_id }                      → { messages: [...] }
MLS_SEND_WELCOME         { space_id, recipient_did, welcome: bytes } → { ok }
MLS_FETCH_WELCOMES       { space_id }                                → { welcomes: [...] }
SYNC_PUSH                { space_id, changes: [...] }                → { ok }
SYNC_PULL                { space_id, after_timestamp }               → { changes: [...] }
```

### Notifications (long-lived bidirektionaler QUIC-Stream)

Peer öffnet nach Verbindung einen Notification-Stream. Leader pushed:

```
NOTIFY_SYNC    { space_id, tables: [...] }
NOTIFY_MLS     { space_id, message_type }
NOTIFY_INVITE  { space_id, invite_id }
```

### Authentifizierung

Jeder Request enthält `sender_endpoint_id`. Leader prüft:
1. Ist Peer in `haexSpaceDevices` für diesen Space registriert?
2. Für Invitees: Gültiger Invite-Token vorhanden?

Gleiche UCAN-Verifikation wie Server-Spaces.

## Datenhaltung

### Neue Tabellen (alle `_no_sync`, existieren auf jedem Gerät)

```sql
haex_local_ds_messages_no_sync (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  space_id TEXT NOT NULL,
  sender_did TEXT NOT NULL,
  message_type TEXT NOT NULL,  -- commit, proposal, application
  message_blob BLOB NOT NULL,
  created_at TEXT DEFAULT CURRENT_TIMESTAMP
)

haex_local_ds_key_packages_no_sync (
  id TEXT PRIMARY KEY,
  space_id TEXT NOT NULL,
  target_did TEXT NOT NULL,
  package_blob BLOB NOT NULL,
  created_at TEXT DEFAULT CURRENT_TIMESTAMP
)

haex_local_ds_welcomes_no_sync (
  id TEXT PRIMARY KEY,
  space_id TEXT NOT NULL,
  recipient_did TEXT NOT NULL,
  welcome_blob BLOB NOT NULL,
  consumed INTEGER DEFAULT 0,
  created_at TEXT DEFAULT CURRENT_TIMESTAMP
)

haex_local_ds_pending_commits_no_sync (
  id TEXT PRIMARY KEY,
  space_id TEXT NOT NULL,
  commit_blob BLOB NOT NULL,
  delivered_to TEXT DEFAULT '[]',  -- JSON array of endpoint_ids
  created_at TEXT DEFAULT CURRENT_TIMESTAMP
)
```

### Schema-Erweiterung

`haexSpaceDevices`: Neue Spalte `leader_priority INTEGER DEFAULT 10`

### Housekeeping (konfigurierbar über Settings-UI)

| Daten | Default TTL | Löschregel |
|-------|-------------|------------|
| Messages | 7 Tage | TTL überschritten ODER alle Peers bestätigt |
| KeyPackages | 24 Stunden | TTL überschritten (Peer generiert neue) |
| Welcomes | 7 Tage | TTL überschritten (Invite wird wiederholt) |
| Pending Commits | 1 Stunde | TTL überschritten (wird neu erzeugt) |
| Cleanup-Intervall | 5 Minuten | - |

Settings-Keys in `haex_vault_settings`:
- `local_ds_message_ttl_days` (Default: 7)
- `local_ds_key_package_ttl_hours` (Default: 24)
- `local_ds_welcome_ttl_days` (Default: 7)
- `local_ds_pending_commit_ttl_hours` (Default: 1)
- `local_ds_cleanup_interval_minutes` (Default: 5)

Zusätzlich: Wenn ein Gerät aufhört Leader zu sein, löscht es sofort alle Buffer-Tabellen.

## Rust-Architektur

```
src-tauri/src/
├── peer_storage/              # Bestehend: File-Sharing über iroh
├── space_delivery/            # NEU
│   ├── mod.rs                 # Gemeinsame Types, Traits
│   ├── local/
│   │   ├── mod.rs
│   │   ├── commands.rs        # Tauri Commands
│   │   ├── types.rs           # Request/Response Types
│   │   ├── protocol.rs        # 11 Message-Types + Notifications
│   │   ├── leader.rs          # Buffering, Message Ordering
│   │   ├── peer.rs            # Connect to Leader, Send/Receive
│   │   ├── discovery.rs       # mDNS + CRDT-basierte Priorität
│   │   ├── election.rs        # Leader Election + Graceful Handoff
│   │   └── housekeeping.rs    # Cleanup mit konfigurierbaren TTLs
│   └── remote/                # Perspektivisch: Server-Kommunikation nach Rust migrieren
│       └── ...
```

### Tauri Commands

```
local_ds_start          → Leader-Modus starten für Space
local_ds_stop           → Leader-Modus stoppen
local_ds_status         → { is_leader, connected_peers, buffered_messages }
local_ds_connect        → Als Peer zum Leader verbinden
local_ds_get_leader     → Aktuellen Leader für Space ermitteln
```

### Geteilter iroh Endpoint

`space_delivery` und `peer_storage` teilen sich denselben iroh Endpoint. Der ALPN bleibt `haex-peer/1`. Das `type`-Feld in der JSON-Message unterscheidet File-Requests von Delivery-Requests.

## Frontend-Integration

### useMlsDelivery.ts

Bestehendes Factory-Pattern:
```typescript
createDeliveryService(space) → ServerDeliveryService | LocalDeliveryService
```

`LocalDeliveryService` implementiert gleiches Interface, ruft Tauri Commands auf die über iroh/QUIC kommunizieren.

### useRealtime.ts

Lokales Gegenstück:
- Server-Space: WebSocket zum Server
- Lokaler Space: QUIC Notification-Stream zum Leader
- Gleiche `on(type, handler)` API

### Sync-Orchestrator

Erkennt am Space-Typ (`local`) dass iroh-Transport genutzt wird. Findet Leader per mDNS + CRDT-Prioritäten. Kein Eintrag in `haexSyncBackends` nötig.

### Space-Erstell-Dialog

User wählt einmalig "Lokal" oder "Server". Typ ist danach immutable.

### Settings-View

- Geräteprioritäten pro Space
- Housekeeping-TTL-Konfiguration

### Admin-Übersicht

Admin sieht welche Identitäten aktuell verbunden sind:
- DID + EndpointId (immer sichtbar)
- Label + Claims (nur wenn Peer sie freigegeben hat)

## Implementierungsreihenfolge

### 6.1 — Grundlagen
- Neue `_no_sync` Tabellen (Drizzle Schema + Migration)
- `leader_priority` Spalte auf `haexSpaceDevices`
- Housekeeping-Settings Defaults
- Vault-Space aus Spaces-Liste filtern, Invite/Delete blockieren

### 6.2 — Rust: `space_delivery` Modul
- Protokoll-Types (11 Message-Types + Notifications)
- Leader-Logik (Buffering, Message Ordering, Housekeeping)
- Peer-Logik (Connect, Send, Receive)
- Geteilter iroh Endpoint mit `peer_storage`
- Tauri Commands

### 6.3 — Leader Election + Discovery
- mDNS Discovery Integration
- Prioritätsbasierte Leader Election
- Graceful Handoff mit Cooldown
- Pending Commit Pattern

### 6.4 — Frontend Integration
- `LocalDeliveryService` in `useMlsDelivery.ts`
- Lokales Realtime-Gegenstück
- Sync-Orchestrator erkennt lokale Spaces
- Space-Erstell-Dialog: "Lokal" oder "Server" (immutable)
- Settings-View: Geräteprioritäten + Housekeeping-TTLs
- Admin-Übersicht: verbundene Peers mit Selective Disclosure

### 6.5 — Invite Flow für lokale Spaces
- Invite-Link mit EndpointId statt serverUrl
- Leader akzeptiert Invite-Requests von Nicht-Mitgliedern
- Pending → Accept → Finalize Flow (identisch zu Server)

## Perspektivisch

- `space_delivery/remote/` — Server-Kommunikation von TypeScript nach Rust migrieren (symmetrisch zu local)
