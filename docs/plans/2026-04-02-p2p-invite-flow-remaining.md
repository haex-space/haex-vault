# P2P Invite Flow — Remaining Work

Offene Punkte nach der initialen Implementierung (v1.8.4).

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
| 1 | Outbox Capabilities laden | Klein | Hoch — ohne das werden falsche Capabilities gesendet |
| 2 | Contact EndpointId | Mittel | Hoch — Contact-Invite funktioniert ohne nicht |
| 4 | Push-Invite Event | Klein | Hoch — UI refresht nicht bei eingehenden Invites |
| 8 | Migration-Fix Vaults | Klein | Hoch — bestehende Vaults sind kaputt |
| 3 | contacts_only Policy | Klein | Mittel |
| 5 | CRDT SyncPush/Pull | Groß | Mittel — Kernfeature für Datensync in lokalen Spaces |
| 7 | Timer Refactoring | Klein | Mittel |
| 6 | Role → Capabilities | Groß | Niedrig — funktioniert mit Compat-Shim |
| 9 | QR-Scanner | Mittel | Niedrig — Link-Eingabe funktioniert |
