# Phase 4: Client-Integration + Realtime-Abstrahierung

## Kontext

Phase 3 hat den Server auf UCAN + DID-Auth umgestellt. Phase 4 bringt den Client dazu, die neuen Auth-Schemes zu nutzen, abstrahiert Supabase Realtime durch einen eigenen WebSocket, und refactored den Spaces Store.

Voraussetzungen: Phase 3 (Server-Enforcement), Ed25519-Umstellung (Phase 0), UCAN Library v0.1.1.

## Scope

1. Client Auth-Integration (UCAN/DID-Auth Header Builder)
2. Spaces Store Refactor (Key Grants → MLS + UCAN)
3. WebSocket Realtime (Supabase Realtime ersetzen)
4. Vault-seitige Validierung (Ebene 2, UCAN-Checks)
5. Invite-UI + Spam-Schutz

## Auth-Integration im Client

### DID-Auth Header Builder (`src/utils/auth/didAuth.ts`)

- `createDidAuthHeader(privateKey, did, action, body)` → `Authorization: DID <payload>.<signature>`
- Payload: `{ did, action, timestamp, bodyHash }` — base64url-encoded, Ed25519-signiert
- Verwendung: Space erstellen, Invite akzeptieren, Vault-Operationen, WebSocket-Connect

### UCAN Store (`src/utils/auth/ucanStore.ts`)

- UCANs in gesynkter Tabelle `haex_ucan_tokens`
- Admin: Self-signed Root-UCANs für eigene Spaces
- Eingeladene Member: UCAN kommt über Invite-Flow (Welcome Message)
- `getUcanForSpace(spaceId)` → UCAN aus lokaler DB, Expiry-Check
- `createUcanAuthHeader(spaceId)` → `Authorization: UCAN <token>`

### Identity Store Anpassung

- Login gibt nur noch DID-Identität zurück
- Supabase JWT intern nur noch für GoTrue Email-Flows
- Primäre Identität: `{ did, publicKey, privateKey, tier }`

## WebSocket Realtime

### Server: `src/routes/ws.ts`

- Hono WebSocket Route: `/ws?token=<DID-signed-payload>`
- DID-Auth beim Handshake (Timestamp ±30s, Signatur, action: "ws-connect")
- Server resolved DID → Identity → Spaces (über space_members)
- Server subscribed Client auf alle seine Spaces

### Events (Server → Client)

| Event | Trigger |
|-------|---------|
| `{ type: "sync", spaceId }` | Neue sync_changes für den Space |
| `{ type: "membership", spaceId }` | Membership geändert (invite accepted, member removed) |
| `{ type: "mls", spaceId }` | Neue MLS Messages/Welcomes |
| `{ type: "invite", spaceId, inviteId }` | Neuer Invite für den User |

### Client: `src/composables/useRealtime.ts`

- WebSocket-Verbindung nach Login
- Auto-Reconnect mit Exponential Backoff
- Dispatcht Events an jeweilige Stores
- Fallback: Polling-Interval wenn WebSocket nicht verbunden

### Supabase-Entkopplung

| Stelle | Vorher | Nachher |
|--------|--------|---------|
| Realtime | Supabase Channels | Eigener WebSocket |
| API Auth | Supabase JWT | UCAN / DID-Auth |
| Shadow-User | Bleibt | Quota, Tier, Abrechnung |
| GoTrue (Email/OTP) | Bleibt | Email-Verifizierung |
| PostgreSQL | Bleibt | Standard-PG, kein Lock-in |

## Spaces Store Refactor

### Space erstellen

```
User klickt "Neuer Space"
    → MLS Group erstellen (Tauri Command)
    → Self-signed Root-UCAN erstellen (space/admin)
    → UCAN in haex_ucan_tokens speichern
    → DID-Auth: POST /spaces
    → Lokale haex_spaces + haex_space_members befüllen
    → Sync propagiert an andere eigene Geräte
```

### Einladung senden (Admin/Owner)

```
    → UCAN für Invitee erstellen (delegiert aus eigenem UCAN)
    → UCAN-Auth: POST /:spaceId/invites
    → Server speichert pending Invite
    → WebSocket pusht "invite"-Event an Invitee
```

### Einladung annehmen (Invitee)

```
    → Invite-Ansicht zeigt pending Invites
    → Prüfung gegen haex_blocked_dids und haex_invite_policy
    → User akzeptiert → DID-Auth: POST /:spaceId/invites/:id/accept + KeyPackages
    → Admin bekommt Event → holt KeyPackage → MLS Add Commit + Welcome
    → Admin sendet UCAN an Invitee (über MLS Welcome)
    → Invitee empfängt Welcome + UCAN → tritt MLS Group bei
    → Lokale haex_space_members wird aktualisiert → synct
```

### Membership-Änderungen

Membership-Änderungen brauchen den Delivery Service (Server oder Admin-Gerät bei lokalen Spaces). Die Flows:

**Server-Space:**
Admin-Vault schreibt lokale haex_space_members → synct als sync_change an Server → Server aktualisiert seine space_members (via Invite/Remove API Calls) → WebSocket pusht Event → alle Members syncen.

**Lokaler Space (Phase 6):**
Admin-Vault schreibt lokale haex_space_members → pusht über QUIC an verbundene Members.

In beiden Fällen: Der Server aktualisiert seine `space_members` basierend auf den expliziten API-Calls (Invite Accept, Member Remove), nicht aus sync_changes. Der Server kann sync_changes nicht lesen (verschlüsselt).

## Vault-seitige Validierung (Ebene 2)

Neues Composable: `src/composables/useChangeValidator.ts`

Validierungskette bei eingehendem sync_change:

1. Persönlicher Vault? → akzeptieren
2. MLS Decrypt → Sender-DID (Phase 5, wenn Verschlüsselung steht)
3. MLS Membership Check (Phase 5)
4. UCAN Capability Check (space/write für Datenänderungen)
5. Record-Ownership prüfen (eigener/collaborative Record)

In Phase 4: Schritte 4+5 (UCAN-Checks). Schritte 2+3 kommen in Phase 5 mit MLS-Verschlüsselung.

## Lokale Tabellen

### Gesynkt (CRDT)

| Tabelle | Inhalt |
|---------|--------|
| `haex_spaces` | Space-Metadaten (id, encryptedName, nameNonce, type, createdAt) |
| `haex_space_members` | Mitgliedschaften (spaceId, did, publicKey, role, label, invitedBy) |
| `haex_ucan_tokens` | UCANs (spaceId, token, capability, issuedAt, expiresAt) |
| `haex_pending_invites` | Eingehende Invites (spaceId, inviterDid, status, createdAt) |
| `haex_blocked_dids` | Blockierte DIDs für Spam-Schutz |
| `haex_invite_policy` | Einstellung: "alle", "nur Kontakte", "niemand" |

### Entfernen

| Tabelle/Code | Grund |
|--------------|-------|
| `haexSpaceKeys` | Ersetzt durch MLS Epoch Keys (Phase 5) |
| `spaceKeyCache`, `generateSpaceKey()` | Ersetzt durch MLS |
| `getSpaceKeysAsync()`, Key Grant Logik | Ersetzt durch MLS + UCAN |
| P-256 ECDH Encrypt/Decrypt für Key Grants | Ersetzt durch MLS |
| Supabase Realtime Channel-Subscriptions | Ersetzt durch WebSocket |
| Supabase JWT in Request-Headern | Ersetzt durch UCAN/DID |

## Invite-UI + Spam-Schutz

### Invite-Ansicht

Eigene Unterseite im Spaces View: Liste aller pending Invites mit Accept/Decline.

### Spam-Schutz

- **Invite-Policy** (`haex_invite_policy`): "alle" | "nur Kontakte" | "niemand"
- **Block-Liste** (`haex_blocked_dids`): Blockierte DIDs → Invites automatisch ignoriert
- **Kontakt-Check**: Bei Policy "nur Kontakte" → Lookup gegen `haexContacts` Tabelle
- Einstellung global in den Space-Settings konfigurierbar
