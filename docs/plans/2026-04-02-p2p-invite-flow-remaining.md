# P2P Invite Flow — Remaining Work

Offene Punkte nach der initialen Implementierung (v1.8.5).

---

## 0. Auto-Identity beim Vault-Start

**Problem:** Der Default-Space wird beim Vault-Start angelegt, aber zu dem Zeitpunkt existiert möglicherweise noch keine Identity. Ohne Identity kann keine Root-UCAN erstellt werden → Invites schlagen fehl mit "No admin UCAN found".

**Entscheidung:** Beim Vault-Start automatisch eine Identity erstellen falls keine existiert. Label = Gerätename, keine Claims, kein Avatar. Der Nutzer kann das später anpassen. Damit ist die kryptographische Grundlage (Ed25519 Keypair, DID) immer da wenn Spaces erstellt werden.

**Dateien:**
- `src/stores/vault/index.ts` (openVaultAsync — Identity vor Space-Erstellung sicherstellen)
- `src/stores/identity.ts` (ensureDefaultIdentityAsync)

**Schritte:**
1. In `identity.ts` eine `ensureDefaultIdentityAsync()` Funktion anlegen
2. Diese generiert ein Ed25519-Keypair, leitet den DID ab, speichert in DB
3. In `vault/index.ts` vor `ensureDefaultSpaceAsync()` aufrufen
4. Alle `if (!identity)` Guards in Space-Flows können dann entfallen

---

## 0b. Cascade-Delete: Identity → Spaces

**Problem:** Wenn eine Identity gelöscht wird, bleiben die zugehörigen Spaces verwaist zurück. Ohne Identity kann man im Space nichts tun (keine UCAN-Signierung, kein DID-Auth, kein MLS).

**Lösung:** Beim Löschen einer Identity alle Spaces mit-löschen die dieser Identity gehören. Das betrifft:
- Spaces wo die Identity Admin ist (lokale Spaces, eigene Online-Spaces)
- UCAN-Tokens die von dieser Identity ausgestellt wurden
- Space Devices die dieser Identity zugeordnet sind
- Outbox-Einträge für diese Spaces

**Dateien:**
- `src/stores/identity.ts` (deleteIdentityAsync)
- Oder: DB-Level CASCADE über FK von `haex_ucan_tokens.issuer_did` → `haex_identities.did`

**Offene Frage:** Sollen Spaces gelöscht werden bei denen man nur Member (nicht Admin) ist? Vermutlich ja — man verlässt den Space automatisch. Andere Members bemerken das über MLS.

**UX-Warnung:** Der Lösch-Dialog muss prominent anzeigen welche Spaces betroffen sind, z.B.:
- "Diese Identität ist Admin von 3 Spaces: **Projekt X**, **Team Y**, **Personal**"
- "Alle diese Spaces werden unwiderruflich gelöscht. Andere Mitglieder verlieren den Zugang."
- Zweistufige Bestätigung oder Eingabe des Space-Namens bei Admin-Spaces

---

## 1. Outbox: Capabilities und History aus Invite-Token laden

**Problem:** Der Outbox-Processor sendet aktuell hardcodierte Werte (`capabilities: ['space/read']`, `includeHistory: false`) statt die echten Werte aus dem Invite-Token zu laden.

**Dateien:**
- `src/composables/useInviteOutbox.ts:114-115`

**Lösung:** Vor dem PushInvite die `haex_invite_tokens`-Tabelle nach `tokenId` abfragen und `capabilities` + `include_history` von dort verwenden.

---

## 2. Contact EndpointId Auflösung

**Problem:** Beim Contact-Invite wird `contact.publicKey` als `contactEndpointId` übergeben — das ist falsch. Kontakte haben keine EndpointId im aktuellen Schema.

**Dateien:**
- `src/components/haex/system/settings/spaces/SpaceInviteDialog.vue:329`

**Lösung:** Entweder:
- EndpointId zum Contact-Schema hinzufügen (`haex_contacts`)
- Oder: EndpointId über `haex_space_devices` auflösen (falls der Kontakt in einem gemeinsamen Space ist)
- Oder: EndpointId über Discovery (iroh) auflösen

---

## 3. Invite-Policy: contacts_only implementieren

**Problem:** Die `contacts_only`-Policy im Rust PushInvite-Handler akzeptiert aktuell alle Invites, weil DID→publicKey-Auflösung fehlt.

**Dateien:**
- `src-tauri/src/space_delivery/local/push_invite.rs:143`

**Lösung:** DID aus dem PushInvite gegen `haex_contacts.publicKey` matchen. Erfordert DID→publicKey-Konvertierung (did:key → raw public key → Base64 SPKI).

---

## 4. Push-Invite Event an Frontend emittieren

**Problem:** Der Rust PushInvite-Handler erstellt den Pending Invite in der DB, aber emittiert kein Tauri-Event ans Frontend. Das Frontend hat zwar einen Listener (`push-invite-received`), aber die Rust-Seite hat keinen Zugang zum `AppHandle`.

**Dateien:**
- `src-tauri/src/space_delivery/local/push_invite.rs`
- `src-tauri/src/space_delivery/local/leader.rs` (LeaderState)

**Lösung:** `AppHandle` zum `LeaderState` hinzufügen (analog zu wie andere Tauri-Handler es machen). Nach dem DB-Insert `app_handle.emit("push-invite-received", ...)` aufrufen.

---

## 5. CRDT Sync über lokale Spaces (SyncPush/SyncPull)

**Problem:** Die `SyncPush` und `SyncPull` Handler im Leader sind noch TODO-Stubs.

**Dateien:**
- `src-tauri/src/space_delivery/local/leader.rs:345-375`

**Lösung:** Die CRDT-Sync-Logik aus dem Server-Sync (`pull.ts`/`push.ts`) adaptieren für den P2P-Fall. Der Leader agiert als "Mini-Server" für CRDT-Changes.

---

## 6. Role → Capabilities Migration

**Problem:** `haex_spaces.role` ist ein Legacy-Feld. Lokale Spaces nutzen Capabilities (UCAN), aber das UI prüft weiterhin `space.role === SpaceRoles.ADMIN` für Berechtigungen.

**Dateien:**
- `src/database/schemas/spaces.ts` (Schema)
- `src/stores/spaces.ts` (mapCapabilityToRole)
- `src/components/haex/system/settings/spaces/SpaceListItem.vue` (UI-Checks)
- `@haex-space/vault-sdk` (DecryptedSpace Interface)
- haex-sync-server (Server API)

**Schritte:**
1. `role` aus `haex_spaces`-Schema entfernen + Migration
2. SDK-Typ `DecryptedSpace` auf `capabilities: string[]` umstellen
3. Alle `space.role === SpaceRoles.X` Checks durch Capability-Checks ersetzen
4. Server-API anpassen

---

## 7. Orchestrator Timer Refactoring

**Problem:** `setInterval` für periodische Tasks (Fallback-Pull, Outbox-Processing) kann Aufrufe stapeln wenn ein Task länger als das Intervall dauert.

**Dateien:**
- `src/stores/sync/orchestrator/index.ts`

**Lösung:** Alle `setInterval`-Patterns durch rekursives `setTimeout` ersetzen:
```typescript
const runOutboxLoop = async () => {
  await processOutboxAsync()
  outboxTimeout = setTimeout(runOutboxLoop, INTERVAL_MS)
}
```

---

## 8. Migration-Fix für bestehende Vaults

**Problem:** Die Migration `0004_p2p_invite_flow.sql` ändert `type = 'shared'` → `'online'` für alle Spaces. Der "default" (Personal) Space war aber ein lokaler Space und sollte `'local'` sein, nicht `'online'`.

**Lösung:** Entweder:
- `ensureDefaultSpaceAsync` prüft und korrigiert den Typ beim Start
- Oder: Zusätzliches UPDATE in der Migration: `UPDATE haex_spaces SET type = 'local' WHERE id = 'default'`

---

## 9. QR-Scanner für Invite-Links

**Problem:** Der bestehende `ScanContactDialog` scannt QR-Codes für Kontakte. Für Invite-Links brauchen wir einen ähnlichen Scanner der `haexvault://invite/local?data=...` Links erkennt.

**Dateien:**
- `src/components/haex/system/settings/contacts/ScanContactDialog.vue` (Vorlage)
- `src/components/haex/system/settings/spaces.vue` (Join-Dialog)

**Lösung:** Entweder den Join-Dialog um einen QR-Tab erweitern oder eine separate Scanner-Komponente bauen die den erkannten Link direkt in den Join-Flow einspeist.

---

## Priorisierung

| # | Task | Aufwand | Priorität |
|---|------|---------|-----------|
| 0a | Auto-Identity beim Vault-Start | Klein | Kritisch — Voraussetzung für alles andere |
| 0b | Cascade-Delete Identity → Spaces | Klein | Kritisch — verwaiste Spaces ohne Identity sind nutzlos |
| 1 | Outbox Capabilities laden | Klein | Hoch — ohne das werden falsche Capabilities gesendet |
| 2 | Contact EndpointId | Mittel | Hoch — Contact-Invite funktioniert ohne nicht |
| 4 | Push-Invite Event | Klein | Hoch — UI refresht nicht bei eingehenden Invites |
| 8 | Migration-Fix Vaults | Klein | Erledigt in v1.8.5 |
| 3 | contacts_only Policy | Klein | Mittel |
| 5 | CRDT SyncPush/Pull | Groß | Mittel — Kernfeature für Datensync in lokalen Spaces |
| 7 | Timer Refactoring | Klein | Mittel |
| 6 | Role → Capabilities | Groß | Niedrig — funktioniert mit Compat-Shim |
| 9 | QR-Scanner | Mittel | Niedrig — Link-Eingabe funktioniert |
