# Sync Architecture Refactoring - Status

## Ziel
Komplette Umstellung der Sync-Architektur von append-only `haexCrdtChanges` auf direktes Tabellen-Scanning mit `haexCrdtDirtyTables`.

## Bereits implementiert ✅

### 1. Server-seitige Änderungen
**Dateien:**
- `haex-sync-server/src/db/schema.ts`
- `haex-sync-server/src/routes/sync.ts`
- `haex-sync-server/drizzle.config.ts`

**Änderungen:**
- `sync_changes` Tabelle neu strukturiert:
  - **Unverschlüsselt**: tableName, rowPks, columnName, operation, hlcTimestamp, deviceId
  - **Verschlüsselt**: encryptedValue, nonce
  - Composite unique index: (vaultId, tableName, rowPks, columnName)
  - Server-seitige Deduplizierung via `ON CONFLICT DO UPDATE`

- Push-Route (`POST /sync/push`):
  - Akzeptiert Changes mit unverschlüsselten Metadaten
  - `ON CONFLICT DO UPDATE` mit HLC-Vergleich
  - Nur Update wenn `incoming.hlcTimestamp > existing.hlcTimestamp`

- Pull-Route (`POST /sync/pull`):
  - Parameter: `afterUpdatedAt` (Server-Timestamp, nicht HLC!)
  - Gibt Metadaten + verschlüsselte Werte zurück

### 2. Client-seitige Schema-Änderungen
**Dateien:**
- `src/database/schemas/crdt.ts`
- `src/database/schemas/haex.ts`
- `src-tauri/database/migrations/0009_bouncy_galactus.sql`

**Entfernt:**
- ❌ `haexCrdtChanges` (value-less logging nicht mehr benötigt)
- ❌ `haexCrdtSnapshots` (nicht verwendet)
- ❌ `haexCrdtConfigs` (nicht verwendet)
- ❌ `haexCrdtSyncStatus` (ersetzt durch Backend-Timestamps)

**Neu/Geändert:**
- ✅ `haexCrdtDirtyTables`: Nur (table_name, last_modified) - markiert Tabellen mit Änderungen
- ✅ `haexSyncBackends`: Erweitert um `lastPushHlcTimestamp` und `lastPullTimestamp`

**Migration:** `0009_bouncy_galactus.sql` löscht alle alten CRDT-Tabellen

### 3. Trigger-System neu implementiert
**Datei:** `src-tauri/src/crdt/trigger.rs`

**Änderungen:**
- Trigger-Namen umbenannt: `z_dirty_{TABLE_NAME}_{insert|update|delete}`
- Nur noch `haexCrdtDirtyTables` befüllen (nicht mehr `haexCrdtChanges`)
- Viel einfacher: nur Tabellenname speichern, keine Spalten/Row-PKs

**Trigger-SQL-Beispiel:**
```sql
CREATE TRIGGER IF NOT EXISTS "z_dirty_haex_settings_insert"
    AFTER INSERT ON "haex_settings"
    FOR EACH ROW
    BEGIN
    INSERT OR REPLACE INTO haex_crdt_dirty_tables (table_name, last_modified)
    VALUES ('haex_settings', datetime('now'));
    END;
```

## WICHTIG: Column-Level HLC Timestamps! 🔴

### Problem entdeckt: Row-Level Timestamps reichen nicht!
**Szenario:**
```
Initial: Row {col_1: "old", col_2: "old", haex_timestamp: "2024-01-01"}

Device A (10:00): col_1 = "foo" → haex_timestamp: "2024-01-01T10:00:00-A"
Device B (10:01): col_2 = "bar" → haex_timestamp: "2024-01-01T10:01:00-B"

❌ Nur Row-Level Timestamp: B gewinnt komplett → col_1 Änderung von A geht verloren
✅ Column-Level Timestamps: col_1="foo" (von A) + col_2="bar" (von B) = Beide bleiben
```

### Lösung: JSON-basierte Column-Level HLCs

**Schema-Struktur (SQLite):**
```sql
CREATE TABLE haex_settings (
  id TEXT PRIMARY KEY,
  key TEXT,
  value TEXT,
  haex_timestamp TEXT NOT NULL,      -- Max aller column HLCs (für Index/schnelle Scans)
  haex_column_hlcs TEXT NOT NULL     -- JSON: {"key": "2024-...-A", "value": "2024-...-B"}
)
```

**Warum JSON?**
- ✅ Keine Schema-Änderungen bei neuen Spalten
- ✅ Flexibel für dynamische Extension-Tabellen
- ✅ `haex_timestamp` als Index für `WHERE haex_timestamp > lastPushHlc`
- ✅ `haex_column_hlcs` für spaltenweise Konfliktauflösung beim Pull

**Änderungen an bestehenden Tabellen:**
- Alle CRDT-Tabellen brauchen die neue `haex_column_hlcs TEXT` Spalte
- Migration muss für bestehende Rows Default-Wert setzen (alle Spalten = haex_timestamp)

## Noch zu implementieren 🚧

### 4. Client Push-Logik (WICHTIG - nächster Schritt!)
**Datei:** `src/stores/sync/orchestrator.ts` (komplett refactoren)

**Pseudocode (mit Column-Level HLCs!):**
```typescript
async function scanAndPushChangesAsync(backendId: string, vaultId: string) {
  const backend = getBackend(backendId)
  const lastPushHlc = backend.lastPushHlcTimestamp

  // 1. Hole dirty tables
  const dirtyTables = await db.select().from(haexCrdtDirtyTables)

  const allChanges = []

  for (const { tableName } of dirtyTables) {
    // 2. Query für geänderte Rows (Row-Level HLC für Effizienz)
    const query = `
      SELECT * FROM ${tableName}
      WHERE haex_timestamp > ?
      OR ? IS NULL
    `
    const rows = await db.execute(query, [lastPushHlc, lastPushHlc])

    // 3. Für jede Row: spaltenweise Changes generieren
    for (const row of rows) {
      const columns = await getTableColumns(tableName) // PRAGMA table_info
      const pkColumns = columns.filter(c => c.is_pk)
      const dataColumns = columns.filter(c =>
        !c.is_pk &&
        c.name !== 'haex_timestamp' &&
        c.name !== 'haex_column_hlcs'
      )

      // Parse column-level HLCs
      const columnHlcs = JSON.parse(row.haex_column_hlcs)

      for (const col of dataColumns) {
        const columnHlc = columnHlcs[col.name]

        // Nur pushen wenn Column HLC > lastPushHlc (oder initial)
        if (!lastPushHlc || columnHlc > lastPushHlc) {
          const value = row[col.name]
          const { encryptedValue, nonce } = await encryptColumnValue(value, vaultKey)

          allChanges.push({
            tableName,
            rowPks: JSON.stringify(extractPks(row, pkColumns)),
            columnName: col.name,
            operation: 'UPDATE', // TODO: INSERT vs UPDATE detection
            hlcTimestamp: columnHlc, // ⚠️ Column-spezifischer HLC!
            deviceId: currentDeviceId,
            encryptedValue,
            nonce
          })
        }
      }
    }
  }

  // 4. Push an Server
  await pushChangesToServer(backendId, vaultId, allChanges)

  // 5. Update lastPushHlcTimestamp (Max aller Column HLCs)
  const maxHlc = Math.max(...allChanges.map(c => c.hlcTimestamp))
  await updateBackend(backendId, { lastPushHlcTimestamp: maxHlc })

  // 6. Cleanup dirty tables (wenn alle backends synced)
  await cleanupDirtyTablesIfAllSynced(dirtyTables)
}
```

**Wichtige Hilfsfunktionen zu erstellen:**
- `getTableColumns(tableName)`: `PRAGMA table_info(${tableName})`
- `extractPks(row, pkColumns)`: PKs aus Row extrahieren
- `encryptColumnValue(value, vaultKey)`: Einzelnen Wert verschlüsseln
- `cleanupDirtyTablesIfAllSynced(tables)`: Prüfen ob alle enabled backends synced

### 5. Client Pull-Logik (mit Column-Level HLC Vergleich!)
**Datei:** `src/stores/sync/engine.ts` (Funktion `pullChangesAsync` anpassen)

**Pseudocode:**
```typescript
async function pullAndApplyChangesAsync(backendId: string, vaultId: string) {
  const backend = getBackend(backendId)
  const lastPullTimestamp = backend.lastPullTimestamp

  // 1. Hole Changes vom Server
  const response = await fetch('/sync/pull', {
    body: JSON.stringify({
      vaultId,
      excludeDeviceId: currentDeviceId,
      afterUpdatedAt: lastPullTimestamp,
      limit: 100
    })
  })

  const { changes, hasMore } = await response.json()

  // 2. Gruppiere nach (tableName, rowPks)
  const changesByRow = groupBy(changes, c => `${c.tableName}:${c.rowPks}`)

  for (const [key, rowChanges] of changesByRow) {
    const { tableName, rowPks } = parseKey(key)

    // 3. Hole aktuellen Row aus lokaler DB
    const localRow = await getRow(tableName, JSON.parse(rowPks))

    if (!localRow) {
      // Row existiert nicht lokal → INSERT
      await insertRow(tableName, rowPks, rowChanges)
      continue
    }

    // Parse lokale column-level HLCs
    const localColumnHlcs = JSON.parse(localRow.haex_column_hlcs)

    for (const change of rowChanges) {
      // 4. Entschlüssele Wert
      const decryptedValue = await decryptColumnValue(
        change.encryptedValue,
        change.nonce,
        vaultKey
      )

      // 5. ⚠️ Column-Level HLC-Vergleich!
      const localColumnHlc = localColumnHlcs[change.columnName]

      if (!localColumnHlc || change.hlcTimestamp > localColumnHlc) {
        // Remote ist neuer → Update Spalte
        await updateColumn(
          tableName,
          rowPks,
          change.columnName,
          decryptedValue,
          change.hlcTimestamp // Update Column HLC in haex_column_hlcs JSON!
        )

        // 6. Update auch Row-Level haex_timestamp (max aller column HLCs)
        await updateRowTimestamp(tableName, rowPks)
      }
    }
  }

  // 7. Update lastPullTimestamp (Server timestamp, nicht HLC!)
  const maxUpdatedAt = Math.max(...changes.map(c => new Date(c.updatedAt).getTime()))
  await updateBackend(backendId, {
    lastPullTimestamp: new Date(maxUpdatedAt).toISOString()
  })

  // 8. Wenn hasMore: rekursiv weiter pullen
  if (hasMore) {
    await pullAndApplyChangesAsync(backendId, vaultId)
  }
}

// Hilfsfunktion: Update Column und dessen HLC
async function updateColumn(
  tableName: string,
  rowPks: object,
  columnName: string,
  value: any,
  hlcTimestamp: string
) {
  const pkWhere = Object.entries(rowPks)
    .map(([k, v]) => `${k} = '${v}'`)
    .join(' AND ')

  // 1. Update Spalten-Wert
  await db.execute(`
    UPDATE ${tableName}
    SET ${columnName} = ?
    WHERE ${pkWhere}
  `, [value])

  // 2. Update column HLC im JSON
  const row = await getRow(tableName, rowPks)
  const columnHlcs = JSON.parse(row.haex_column_hlcs)
  columnHlcs[columnName] = hlcTimestamp

  await db.execute(`
    UPDATE ${tableName}
    SET haex_column_hlcs = ?
    WHERE ${pkWhere}
  `, [JSON.stringify(columnHlcs)])
}

// Hilfsfunktion: Update Row-Level Timestamp (max aller Column HLCs)
async function updateRowTimestamp(tableName: string, rowPks: object) {
  const row = await getRow(tableName, rowPks)
  const columnHlcs = JSON.parse(row.haex_column_hlcs)
  const maxHlc = Math.max(...Object.values(columnHlcs))

  const pkWhere = Object.entries(rowPks)
    .map(([k, v]) => `${k} = '${v}'`)
    .join(' AND ')

  await db.execute(`
    UPDATE ${tableName}
    SET haex_timestamp = ?
    WHERE ${pkWhere}
  `, [maxHlc])
}
```

### 6. Crypto-Funktionen anpassen
**Datei:** `src/utils/crypto/vaultKey.ts`

**Neue Funktionen:**
```typescript
// Statt ganzes CRDT-Objekt: nur Spalten-Wert verschlüsseln
export async function encryptColumnValueAsync(
  value: any,
  vaultKey: Uint8Array
): Promise<{ encryptedValue: string; nonce: string }> {
  // JSON.stringify(value) → encrypt → base64
}

export async function decryptColumnValueAsync(
  encryptedValue: string,
  nonce: string,
  vaultKey: Uint8Array
): Promise<any> {
  // base64 → decrypt → JSON.parse
}
```

**Alte Funktionen entfernen/deprecaten:**
- `encryptCrdtDataAsync()` (verschlüsselte ganzes Change-Objekt)
- `decryptCrdtDataAsync()`

### 7. Orchestrator komplett refactoren
**Datei:** `src/stores/sync/orchestrator.ts`

**Zu ändern:**
- Alle Imports von `haexCrdtChanges`, `haexCrdtSyncStatus` entfernen
- `loadSyncStatusAsync()` entfernen (nicht mehr benötigt)
- `getChangesToPushAsync()` durch `scanAndPushChangesAsync()` ersetzen
- `pullFromBackendAsync()` durch `pullAndApplyChangesAsync()` ersetzen
- Sync-Reihenfolge: **Pull → dann Push** (wichtig!)

### 8. DELETE-Handling (Designentscheidung!)
**Problem:** Wie erkennen wir gelöschte Rows beim Scan?

**Option A: Soft-Delete mit Tombstone (empfohlen)**
- Jede Tabelle bekommt `is_deleted` Boolean-Spalte
- DELETE wird zu `UPDATE SET is_deleted = 1`
- Beim Scan: `WHERE haex_timestamp > lastPush AND is_deleted = 1` → operation='DELETE'
- Cleanup: Alte Tombstones periodisch löschen

**Option B: Hard-Delete mit separatem Tracking**
- DELETE-Trigger schreibt in separate `haex_deleted_rows` Tabelle
- Beim Scan: zusätzlich deleted_rows prüfen
- Komplexer, aber echte Deletes

**Zu klären:** Welche Option bevorzugt?

## Zusätzliche Aufgabe: Schema-Migration für Column-Level HLCs

**Alle CRDT-Tabellen brauchen:**
```sql
ALTER TABLE haex_settings ADD COLUMN haex_column_hlcs TEXT NOT NULL DEFAULT '{}';
ALTER TABLE haex_extensions ADD COLUMN haex_column_hlcs TEXT NOT NULL DEFAULT '{}';
ALTER TABLE haex_notifications ADD COLUMN haex_column_hlcs TEXT NOT NULL DEFAULT '{}';
-- etc. für alle CRDT-Tabellen
```

**Migration für existierende Rows:**
```sql
-- Für jede Tabelle: Initialisiere column_hlcs mit aktuellem haex_timestamp für alle Spalten
UPDATE haex_settings
SET haex_column_hlcs = json_object(
  'key', haex_timestamp,
  'value', haex_timestamp,
  'type', haex_timestamp
)
WHERE haex_column_hlcs = '{}';
```

**Oder dynamisch mit Trigger:**
Neuer Trigger-Code muss `haex_column_hlcs` JSON bei INSERT/UPDATE befüllen!

## Offene Fragen

1. **DELETE-Handling:** Tombstone oder separate Tracking-Tabelle?
   - **Empfehlung:** Soft-delete mit `is_deleted` Boolean (einfacher, konsistent)

2. **Initial Sync:** Beim ersten Backend (lastPushHlcTimestamp === null):
   - Alle Tabellen als dirty markieren?
   - Oder automatisch alles hochladen ohne dirty-check?
   - **Empfehlung:** Automatisch alles hochladen (alle Rows in allen Tabellen scannen)

3. **Dirty Table Cleanup:** Wann aus `haexCrdtDirtyTables` löschen?
   - Nach Sync zu ALLEN enabled backends?
   - Oder pro-backend tracking in extra Spalte?
   - **Empfehlung:** Nach Sync zu ALLEN enabled backends löschen (einfacher)

4. **Realtime-Subscriptions:** Wie passen Supabase Realtime subscriptions in neue Architektur?
   - Wahrscheinlich: Bei Server-Change-Event → sofort Pull-Zyklus starten
   - **Zu klären:** Subscription auf `sync_changes` Tabelle für `updatedAt` Changes

5. **Error Handling:** Was passiert wenn Push fehlschlägt?
   - Dirty table bleibt erhalten → retry beim nächsten Sync
   - Backend timestamps nicht updaten
   - **Gut so!** ✅

6. **Column HLC Initialisierung:** Wie generieren wir HLCs beim lokalen INSERT/UPDATE?
   - Bestehender HLC-Generator verwenden
   - Alle geänderten Spalten bekommen neuen HLC
   - Row-Level `haex_timestamp` = max aller column HLCs
   - **Trigger müssen angepasst werden!** 🔴

## Dateien die noch angepasst werden müssen

1. `src/stores/sync/orchestrator.ts` - Hauptarbeit!
2. `src/stores/sync/engine.ts` - Push/Pull-Funktionen
3. `src/utils/crypto/vaultKey.ts` - Neue encrypt/decrypt Funktionen
4. `src/stores/sync/backends.ts` - Eventuell Backend-Queries anpassen
5. Alle Komponenten die `haexCrdtChanges` importieren (suchen mit grep)

## Test-Plan

1. **Manuelle Tests:**
   - [ ] Neue Vault erstellen, Backend hinzufügen → Initial Sync
   - [ ] Settings ändern → Dirty table markiert → Push funktioniert
   - [ ] Von zweitem Gerät ändern → Pull funktioniert → HLC-Konflikt korrekt gelöst
   - [ ] Row löschen → DELETE richtig gesynct
   - [ ] Mehrere Backends → Alle bekommen Updates

2. **Edge Cases:**
   - [ ] Offline-Änderungen → Reconnect → Sync
   - [ ] Konflikt: Beide Geräte ändern selbe Spalte
   - [ ] Backend löschen während Sync läuft
   - [ ] Netzwerk-Fehler während Push/Pull

## Nächster konkreter Schritt

**Datei:** `src/stores/sync/orchestrator.ts`

1. Imports aufräumen (alte CRDT-Tabellen entfernen)
2. Hilfsfunktion `getTableColumnsAsync()` implementieren
3. Funktion `scanDirtyTablesForChangesAsync()` implementieren
4. Neue `pushToBackendAsync()` implementieren

**Start-Code:**
```typescript
// In orchestrator.ts - neue Hilfsfunktion
async function getTableColumnsAsync(db: Database, tableName: string) {
  const columns = await db.select(`PRAGMA table_info(${tableName})`)
  return columns.map(c => ({
    name: c.name,
    type: c.type,
    isPk: c.pk > 0
  }))
}
```
