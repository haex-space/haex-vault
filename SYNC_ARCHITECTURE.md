# Sync Architecture - DELETE Handling

## Problem

Bei der Table-Scanning-Architektur für CRDT-Sync gibt es ein fundamentales Problem:

1. **INSERT vs UPDATE**: Durch Table-Scanning können wir nicht unterscheiden, ob eine Row neu eingefügt oder nur aktualisiert wurde
2. **DELETE**: Gelöschte Rows existieren nicht mehr in der Tabelle und können nicht gescannt werden

## Lösung: Soft-Delete mit Single PK ✅ (GEWÄHLT)

### Architektur-Entscheidung

Nach Abwägung haben wir uns für **Soft-Delete** entschieden:

1. Alle CRDT-Tabellen bekommen ein `haex_deleted INTEGER NOT NULL DEFAULT 0` Flag
2. **Regel**: Jede Tabelle MUSS genau einen Primary Key (UUID) haben - **KEINE Composite Keys**
3. Uniqueness-Constraints werden über `UNIQUE(...) WHERE haex_deleted = 0` abgebildet
4. Einheitliches "Dirty Table" Pattern für INSERT/UPDATE/DELETE

### Warum Soft-Delete statt DELETE Log?

**Vorteile von Soft-Delete:**
- ✅ Einheitliches Pattern: Nur Dirty Tables, keine separate Log-Tabelle
- ✅ Einfachere Architektur: Ein Mechanismus für alles
- ✅ Wiederherstellbarkeit: Gelöschte Daten können bei Sync-Konflikten wiederhergestellt werden
- ✅ Konsistenz: Alle Tabellen haben gleiche Struktur

**Akzeptierter Trade-off:**
- ⚠️ Entwickler müssen Regel "nur Single PK" einhalten
- ⚠️ UNIQUE Constraints benötigen `WHERE haex_deleted = 0` (Partial Indexes)
- ⚠️ Alle User-facing Queries brauchen `WHERE haex_deleted = 0`

### Warum kein vollständiger Change Log?

Vollständiger Change Log (alle INSERT/UPDATE/DELETE) wurde verworfen wegen:
- **Storage Overhead**: Alle Änderungen würden dupliziert gespeichert
- **Komplexität**: Trigger für INSERT/UPDATE/DELETE auf jeder Tabelle
- **Redundanz**: Dirty Tables existieren bereits für INSERT/UPDATE

## Implementierungsplan

### 1. Datenbank-Richtlinien für Entwickler

```markdown
# HaexVault Database Guidelines

## Primary Keys (WICHTIG!)
- **Regel**: Jede Tabelle MUSS genau einen Primary Key haben
- **Format**: UUID als TEXT (generiert mit `lower(hex(randomblob(16)))`)
- **Spaltenname**: Immer `id`
- **KEINE Composite Keys erlaubt!**

## Uniqueness Constraints
- Verwende **UNIQUE Constraints mit Partial Index** statt Composite Keys
- Bei CRDT-Tabellen: `UNIQUE(...) WHERE haex_deleted = 0`
- Beispiel:
  ```sql
  id TEXT PRIMARY KEY,
  user_id TEXT NOT NULL,
  email TEXT NOT NULL,
  haex_deleted INTEGER NOT NULL DEFAULT 0,
  UNIQUE(user_id, email) WHERE haex_deleted = 0
  ```

## Soft-Delete Flag
- Alle CRDT-Tabellen haben `haex_deleted INTEGER NOT NULL DEFAULT 0`
- 0 = aktiv, 1 = gelöscht
- User-facing Queries MÜSSEN `WHERE haex_deleted = 0` enthalten
```

### 2. Migration: haex_deleted Spalte hinzufügen

```sql
-- Für alle existierenden CRDT-Tabellen
ALTER TABLE haex_passwords ADD COLUMN haex_deleted INTEGER NOT NULL DEFAULT 0;
ALTER TABLE haex_vaults ADD COLUMN haex_deleted INTEGER NOT NULL DEFAULT 0;
-- ... weitere Tabellen

-- Index für effiziente Queries
CREATE INDEX idx_passwords_deleted ON haex_passwords(haex_deleted);
CREATE INDEX idx_vaults_deleted ON haex_vaults(haex_deleted);
-- ... weitere Indexes
```

### 3. UNIQUE Constraints aktualisieren

Alle existierenden UNIQUE Constraints müssen zu Partial Indexes werden:

```sql
-- Beispiel: Wenn haex_passwords einen UNIQUE Constraint auf (user_id, title) hatte
DROP INDEX IF EXISTS unique_passwords_user_title;
CREATE UNIQUE INDEX unique_passwords_user_title
  ON haex_passwords(user_id, title)
  WHERE haex_deleted = 0;
```

### 4. Push-Logik: Soft-Deleted Rows scannen

Der Table Scanner muss angepasst werden um AUCH soft-deleted Rows zu finden:

```typescript
// VORHER: Nur aktive Rows
SELECT * FROM haex_passwords WHERE haex_timestamp > ?

// NACHHER: Aktive UND gelöschte Rows
SELECT * FROM haex_passwords WHERE haex_timestamp > ?
-- kein WHERE haex_deleted = 0 Filter beim Scannen!
```

Soft-deleted Rows werden ganz normal als Column-Changes übertragen:
- Alle Spalten werden gesendet, inklusive `haex_deleted = 1`
- Backend erkennt DELETE am `haex_deleted` Flag
- Keine spezielle DELETE-Behandlung nötig

### 5. Datenformat für Server (unverändert)

```typescript
interface ColumnChange {
  tableName: string
  rowPks: string  // JSON: '{"id":"123"}'
  columnName: string  // z.B. "password" oder "haex_deleted"
  hlcTimestamp: string
  batchId: string
  batchSeq: number
  batchTotal: number
  encryptedValue: string  // Auch für haex_deleted (0 oder 1 verschlüsselt)
  nonce: string
}
```

**Wichtig**: `haex_deleted` wird wie jede andere Spalte behandelt - als Column-Change!

### 6. Pull-Logik: haex_deleted anwenden

Bei eingehenden Changes prüft das Backend:

```typescript
if (change.columnName === 'haex_deleted' && decryptedValue === 1) {
  // Row wurde auf anderem Gerät gelöscht
  // Setze haex_deleted = 1 lokal (Soft-Delete)
}
```

### 7. User-facing Queries anpassen

Alle Queries, die User-Daten liefern, MÜSSEN `WHERE haex_deleted = 0` haben:

```typescript
// ALLE SELECT Queries für User-facing Daten
const passwords = await db
  .select()
  .from(haexPasswords)
  .where(eq(haexPasswords.haexDeleted, 0))  // WICHTIG!
```

### 8. Cleanup-Strategie

Periodisches Cleanup (z.B. alle 7 Tage) löscht alte soft-deleted Rows:

```sql
-- Lösche soft-deleted Rows, die:
-- 1. Älter als 90 Tage sind
-- 2. Mit allen Backends synchronisiert wurden
DELETE FROM haex_passwords
WHERE haex_deleted = 1
  AND datetime(haex_timestamp) < datetime('now', '-90 days')
  AND haex_timestamp <= (
    SELECT MIN(last_pull_hlc_timestamp)
    FROM haex_sync_backends
    WHERE enabled = 1
  );
```

Alternative: Cleanup manuell durch User anstoßen (z.B. "Papierkorb leeren")

## DELETE Statements transformieren

**Wichtig**: Alle DELETE Statements müssen zu UPDATE Statements werden:

```typescript
// VORHER: DELETE
await db.delete(haexPasswords).where(eq(haexPasswords.id, passwordId))

// NACHHER: Soft-Delete (UPDATE)
await db
  .update(haexPasswords)
  .set({ haexDeleted: 1 })
  .where(eq(haexPasswords.id, passwordId))
```

**Keine DELETE Trigger nötig!** Das normale CRDT-UPDATE-System handled alles.

## Vorteile dieser Lösung

1. ✅ **Einheitliches Pattern**: Ein Mechanismus für INSERT/UPDATE/DELETE
2. ✅ **Wiederherstellbarkeit**: Gelöschte Daten können bei Sync-Konflikten wiederhergestellt werden
3. ✅ **Einfache Architektur**: Keine separate Log-Tabelle, keine Trigger
4. ✅ **Konsistenz**: Alle Tabellen haben gleiche Struktur
5. ✅ **Batch-Konsistenz**: Alle Changes (inkl. DELETE) im gleichen Batch
6. ✅ **CRDT-kompatibel**: DELETE ist einfach ein Column-Update wie jedes andere

## Extension Schema Migration System

Extensions können dynamisch Tabellen erstellen, die ebenfalls synchronisiert werden müssen.

### Architektur

**Dynamische Tabellenerkennung:**
- `discover_crdt_tables()` in `src-tauri/src/database/init.rs` scannt alle Tabellen mit `haex_tombstone` Spalte
- Keine hardcodierte Tabellenliste mehr nötig
- CRDT-Trigger werden automatisch für alle erkannten Tabellen erstellt

**Extension Migrations Tabelle:**
```sql
CREATE TABLE haex_extension_migrations (
  id TEXT PRIMARY KEY,
  extension_id TEXT NOT NULL,          -- Foreign Key zu haex_extensions
  extension_version TEXT NOT NULL,     -- z.B. "0.1.15"
  migration_name TEXT NOT NULL,        -- z.B. "0000_illegal_wallflower"
  sql_statement TEXT NOT NULL,         -- Komplette .sql Datei
  applied_at TEXT DEFAULT (CURRENT_TIMESTAMP),

  -- CRDT Spalten (automatisch synchronisiert)
  haex_timestamp TEXT,
  haex_column_hlcs TEXT DEFAULT '{}',
  haex_tombstone INTEGER DEFAULT 0,

  FOREIGN KEY (extension_id) REFERENCES haex_extensions(id) ON DELETE CASCADE,
  UNIQUE (extension_id, migration_name) WHERE haex_tombstone = 0
)
```

**Migration Flow:**
1. Extension führt Migration lokal aus via SDK `runMigrationsAsync()`
2. SDK speichert Migration in `haex_extension_migrations`
3. Migration wird via CRDT zu anderen Devices synchronisiert
4. Device B beim Pull:
   - Prüft ob Extension in entsprechender Version vorhanden
   - Führt Migration aus wenn Extension verfügbar
   - Wartet wenn Extension noch nicht heruntergeladen
5. Backend erstellt automatisch CRDT-Trigger für neue Extension-Tabellen

**Tabellenpräfix:**
Extension-Tabellen nutzen Format: `{public_key_hash}__{extension_name}__{table_name}`

Beispiel: `b4401f13f65e576b8a30ff9fd83df82a8bb707e1994d40c99996fe88603cefca__haex-pass__haex_passwords_item_details`

## Status

**Basis-Sync-System:**
- [ ] Migration: haex_deleted Spalte zu allen CRDT-Tabellen hinzufügen
- [ ] Migration: UNIQUE Constraints mit `WHERE haex_deleted = 0` aktualisieren
- [ ] CREATE TABLE Statements mit partial UNIQUE Constraints anpassen
- [ ] TypeScript Schemas mit haex_deleted erweitern
- [ ] Table Scanner: Filter entfernen (soft-deleted Rows mitscannen)
- [ ] Alle DELETE Statements durch UPDATE mit haex_deleted = 1 ersetzen
- [ ] Alle user-facing SELECT Queries mit `WHERE haex_deleted = 0` erweitern
- [ ] Cleanup-Job für alte soft-deleted Rows implementieren
- [ ] Tests schreiben

**Extension Migration System:**
- [x] `haex_extension_migrations` Tabelle Schema erstellt
- [x] Dynamische Tabellenerkennung implementiert (`discover_crdt_tables()`)
- [x] Extension Download Folder existiert bereits (`app_local_data_dir()/extensions`)
- [x] SDK erweitert: `registerMigrationsAsync()` Methode hinzugefügt
- [x] Frontend Handler: `handleDatabaseMethodAsync` erweitert
- [x] Rust Command: `register_extension_migrations` implementiert
- [x] SQL-Validierung: Nur Extension-eigene Tabellen erlaubt
- [ ] Pull-Prozess: Extension Migrations vor CRDT-Changes anwenden
- [ ] Migration Execution: SQL-Statements ausführen wenn Extension verfügbar
- [ ] Validierung: Extension-Version muss mit Migration-Version übereinstimmen
