# ADR: Kryptographisch signierte Sync-Changes

**Status:** In Arbeit (aktualisiert 2026-04-05)
**Datum:** 2026-04-04
**Kontext:** Capability-Enforcement bei CRDT-Sync

## Problem

CRDT-Changes werden über den Sync-Server übertragen. Dabei signiert der Sender jeden Change mit Ed25519, und der Server verifiziert die Signatur beim Push. **Allerdings verifiziert der Empfänger (Pull-Seite) die Signaturen nicht.** Ein kompromittierter Server könnte daher manipulierte Changes mit gefälschten Signaturen an Clients liefern.

### Angriffsvektor

Kompromittierter Sync-Server kann beim Pull:
- Changes mit gefälschten Signaturen injizieren
- Bestehende Changes manipulieren (encryptedValue austauschen)
- Changes von nicht-autorisierten DIDs einspeisen

### Kein Problem: QUIC (Local Delivery)

QUIC-Verbindungen sind direkter Peer-to-Peer-Transport mit:
- TLS 1.3 Authentifizierung im QUIC-Handshake
- Capability-Check in `leader.rs` (`check_write_capability()`)
- Kein Mittelsmann → kein Manipulationsrisiko

**Entscheidung:** QUIC braucht keine zusätzliche Signaturschicht.

## Ist-Zustand (was bereits existiert)

| Komponente | Status | Details |
|-----------|--------|---------|
| `signRecordAsync` (vault-sdk) | ✅ Fertig | Ed25519, Kanonisierung via `\0`-Separator |
| `verifyRecordSignatureAsync` (vault-sdk) | ✅ Fertig | Gegenstück zu sign, gleiche Kanonisierung |
| Push signiert Changes | ✅ Fertig | `push.ts:275-291`, signiert über `{tableName, rowPks, columnName, encryptedValue, hlcTimestamp}` |
| Server-Schema `signature`/`signedBy` Spalten | ✅ Fertig | `sync_changes` Tabelle hat beide Spalten |
| Server verifiziert beim Push | ✅ Fertig | `sync.helpers.ts` → `verifyRecordSignatureAsync()` |
| Pull-Response enthält Signaturen | ✅ Fertig | `signature`, `signedBy`, `recordOwner` für Space-Syncs |
| **Client-seitige Verifikation beim Pull** | ❌ Fehlt | TODO in `pull.ts:159-162` |
| **E2E Tests für Signatur-Integrität** | ❌ Fehlt | `evil-scenarios.spec.ts` testet keine Signaturen |

## Verbleibende Arbeit

### 1. Client-seitige Signatur-Verifikation beim Pull

**Datei:** `src/stores/sync/orchestrator/pull.ts`

**Wo:** Nach dem Empfang der Changes vom Server, **vor** der Entschlüsselung und Anwendung.

**Ablauf:**
1. Für jeden Change mit `signature` + `signedBy`:
   - `verifyRecordSignatureAsync(record, signature, signedBy)` aufrufen
   - Record = `{tableName, rowPks, columnName, encryptedValue, hlcTimestamp}` (verschlüsselter Wert, wie er vom Server kommt)
2. Public Key des Signers gegen bekannte Space-Member prüfen (siehe Punkt 2)
3. Bei fehlgeschlagener Verifikation: **gesamten Pull-Batch verwerfen** und Error loggen

**Warum ganzen Batch verwerfen?** Ein teilweise angewendeter Batch mit manipulierten Changes könnte inkonsistente CRDT-State erzeugen. Lieber den gesamten Pull ablehnen und den User/Log informieren.

**Grace Period:** Changes ohne `signature` (von vor der Signatur-Einführung) werden akzeptiert. Nur Changes MIT Signatur werden verifiziert — eine ungültige Signatur ist schlimmer als keine.

### 2. Member-Public-Key-Auflösung

**Voraussetzung:** [Space Members Tabelle](2026-04-05-space-members-table.md) — separiert Space-Member vom Kontaktbuch.

**Problem:** `signedBy` im Pull-Response ist ein Raw-Public-Key (Base64 SPKI). Der Client muss verifizieren, dass dieser Key zu einem bekannten Space-Member gehört.

**Lösung:** Gegen `haex_space_members_no_sync` Tabelle prüfen:
- `signedBy` muss einem `memberPublicKey`-Eintrag für den betreffenden Space entsprechen
- Lookup: `WHERE spaceId = X AND memberPublicKey = signedBy`
- Index auf `(spaceId, memberPublicKey)` macht das effizient

**Cache:** Pro Pull-Batch einmal die bekannten Public Keys für den Space laden und im Memory cachen. Die Tabelle ist lokal und klein.

**Unbekannte Signer:** Wenn `signedBy` keinem bekannten Member entspricht → Change verwerfen. Das deckt den Fall ab, dass ein kompromittierter Server Changes von "Phantom-Membern" injiziert.

### 3. E2E Tests

**Datei:** `haex-e2e-tests/tests/sync/signature-verification.spec.ts` (neu)

Die bestehende `evil-scenarios.spec.ts` testet Server-seitige Abwehr. Die neuen Tests prüfen **Client-seitige** Verifikation und den **End-to-End-Signatur-Flow**.

#### Testfälle

**Happy Path:**

| Test | Beschreibung |
|------|-------------|
| Signierter Change wird akzeptiert | Admin pushed signiert → Puller verifiziert erfolgreich |
| Cross-Device Sync mit Signaturen | Device A pushed → Device B pulled und verifiziert → Daten korrekt |
| Batch mit mehreren Autoren | Zwei autorisierte Member pushed → Puller akzeptiert beide Signaturen |

**Angriffsvektoren (Server-seitig — `evil-scenarios.spec.ts` erweitern):**

| Test | Beschreibung |
|------|-------------|
| Push ohne Signatur in Space wird rejected | Server muss `signature` + `signedBy` für Space-Syncs erzwingen |
| Push mit falscher Signatur wird rejected | Gültige Struktur, aber falscher Private Key → Server rejected |
| Push mit `signedBy` ≠ authentifizierter DID | Attacker signiert korrekt, gibt aber fremden Public Key an |
| recordOwner-Manipulation: Attacker überschreibt fremden Record | Non-collaborative Record von Victim → Attacker versucht Update |

**Angriffsvektoren (Client-seitig — `signature-verification.spec.ts`):**

| Test | Beschreibung |
|------|-------------|
| Manipulierter `encryptedValue` bei intakter Signatur | Direkt in DB ändern → Pull-Client muss Signatur-Mismatch erkennen |
| Manipulierter `hlcTimestamp` bei intakter Signatur | Timestamp in DB ändern → Verifikation schlägt fehl |
| Gefälschte Signatur (gültiges Format, falscher Key) | Change in DB mit neuer Signatur von fremdem Key → Client rejected |
| Change von unbekanntem Signer | `signedBy` zeigt auf Key der nicht in Space-Members ist → Client rejected |
| Replay-Angriff: gültige Signatur, falscher Kontext | Signierter Change von Space A in Space B replayed → Client erkennt Kontext-Mismatch |
| Batch mit gemischten validen/invaliden Signaturen | Ein manipulierter Change in Batch von 10 → gesamter Batch rejected |

#### Test-Infrastruktur

Die Tests nutzen die bestehenden Helpers:
- `signAndPushSpaceChanges()` für valide Pushes
- `pushChanges()` für Pushes ohne/mit manipulierter Signatur
- Direkte DB-Manipulation für Server-seitige Tampering-Tests (Supabase-Client oder SQL)
- `pullChanges()` + manuelle Verifikation mit `verifyRecordSignatureAsync`

Für Client-seitige Tests (die tatsächlich die Vault-App testen):
- `VaultAutomation` + `VaultBridgeClient` für App-Interaktion
- Change in Server-DB manipulieren → App pullt → prüfen dass Change nicht angewendet wurde

## Betroffene Dateien

| Datei | Änderung |
|-------|----------|
| `haex-vault/src/stores/sync/orchestrator/pull.ts` | Signatur-Verifikation vor Anwendung |
| `haex-e2e-tests/tests/sync/signature-verification.spec.ts` | Neue Testdatei |
| `haex-e2e-tests/tests/sync/evil-scenarios.spec.ts` | Erweitert um Signatur-Angriffsvektoren |

## Implementierungs-Reihenfolge

1. **[Space Members Tabelle](2026-04-05-space-members-table.md)** zuerst (Voraussetzung)
2. **Pull-Verifikation:** `verifyRecordSignatureAsync` in `pull.ts` einbauen, Lookup gegen `haex_space_members_no_sync`
3. **E2E Tests — Server-seitig:** `evil-scenarios.spec.ts` um Signatur-Tests erweitern
4. **E2E Tests — Client-seitig:** `signature-verification.spec.ts` für End-to-End-Flow
5. **Strict Mode:** Grace Period entfernen → unsignierte Changes rejected

## Nicht im Scope

- **QUIC-Signierung:** Nicht nötig (Peer-to-Peer mit TLS + Capability-Check)
- **Rust-seitige Verifikation:** `apply_remote_changes_to_db` bleibt wie es ist — die Verifikation passiert im TypeScript-Layer vor der Übergabe an Rust
- **Schema-Migration:** Keine neuen Spalten nötig — `signature`, `signedBy`, `recordOwner` existieren bereits
- **Nachträgliche Signierung:** Bestandsdaten werden nicht nachsigniert

## Performance

- Ed25519 Verify: ~30µs pro Change → bei 1000 Changes im Batch: ~30ms
- Public-Key-Lookup: 1 Query pro Pull-Batch (nicht pro Change)
- Kein messbarer Impact auf die Sync-Performance
