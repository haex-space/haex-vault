# Contract: Erweiterung von haex-crdt für mehrteilige CRDT-Schreibungen

Repository `https://github.com/haexmas/haex-crdt`. Umgesetzt in haexmas/haex-crdt#34 (Transaktionen),
#35 (typisierte Fehler) und #36 (Entfernen der `DbConnection`-Schicht). Dazu kommen #37 (BLOB-Werte
und BLOB-Schlüssel reisen typtreu) und #38 (Zeilen aus der Zeit vor dem HLC werden beim Öffnen
gestempelt); holzi pinnt den Merge von #38, `d35d43f9250a7d9a7d2dca3a0c8ee7ffabc750a7`
(Constitution IV). Die Änderung ist allgemein und
enthält nichts holzi-Eigenes. Rückwärtsverträglichkeit war nicht verlangt: holzi löst haex-vault 2.x (Legacy)
ab (Betreiber-Entscheidung). Begründung: research R19.

## E1 Transaktion mit mehreren CRDT-Anweisungen

```rust
impl Database {
    pub fn write<R>(&self, f: impl FnOnce(&mut CrdtTransaction<'_>) -> Result<R>) -> Result<R>;
    pub fn read<R>(&self, f: impl FnOnce(&ReadOnlyConnection<'_>) -> Result<R>) -> Result<R>;
    pub fn max_transaction_bytes(&self) -> usize;
}

impl CrdtTransaction<'_> {
    pub fn execute(&mut self, sql: &str, params: &[&dyn ToSql]) -> Result<usize>;
    pub fn query_map<T, F>(&mut self, sql: &str, params: &[&dyn ToSql], f: F) -> Result<Vec<T>>;
    pub fn query_row<T, F>(&mut self, sql: &str, params: &[&dyn ToSql], f: F) -> Result<Option<T>>;
}

impl ReadOnlyConnection<'_> {
    pub fn query_row<T, P, F>(&self, sql: &str, params: P, f: F) -> rusqlite::Result<T>;
    pub fn query_map<T, P, F>(&self, sql: &str, params: P, f: F) -> rusqlite::Result<Vec<T>>;
}
```

- `write` öffnet eine `IMMEDIATE`-Transaktion, ruft `f` auf und committet nur bei `Ok`; bei `Err`
  oder Panik Rollback. Alle Anweisungen darin teilen einen HLC.
- Jede Schreibung in `execute`, `query_map` und `query_row` läuft durch den `CrdtTransformer`,
  auch `ON CONFLICT … DO UPDATE` und `RETURNING`. Schreibungen auf CRDT-Metaspalten weist er ab
  (`CrdtMetaColumnWriteForbidden`). Tabellen mit Endung `_no_sync` lässt er am Namen unberührt.
- `read` hält während `f` `PRAGMA query_only` und einen Authorizer, der nur Lesen, `SELECT`,
  Funktionen und Rekursion erlaubt.
- Parameter sind `&[&dyn ToSql]`, BLOBs eingeschlossen.
- Die freien Funktionen auf `DbConnection` (`execute`, `execute_with_crdt`, `select`,
  `select_with_crdt` mit JSON-Parametern) und die `PostWriteHook`s sind entfernt.

## E2 Einstellbare Grenze je Transaktion

- `DatabaseConfig.max_transaction_bytes: usize`, Standard `MAX_CRDT_TRANSACTION_BYTES` (100 MiB).
- Die kanonische Byteabrechnung `serialized_parameter_bytes` (öffentlich) ist die Summe der
  gespeicherten Größen aller Bind-Parameter: Länge von Text und BLOB, 8 Byte für Ganzzahl und
  Gleitkommazahl, 0 für NULL. SQL-Text, Spaltennamen, HLC-Metadaten sowie Transaktions- und
  Seitenrahmen zählen nicht. Diese eine Abrechnung verwenden sowohl `CrdtTransaction` als auch der
  Sync für die zum Anwenden einer vollständigen Transaktionsgruppe gebildete Parameterliste;
  dadurch prüfen lokaler Schreibweg und Empfänger dieselbe Größe.
- `CrdtTransaction` bricht mit `DatabaseError::TransactionTooLarge` ab, sobald die Summe die
  Grenze übersteigt, bevor die überschreitende Anweisung schreibt.
- holzi nutzt `Database::max_transaction_bytes()` als Obergrenze für eine Transaktionsgruppe im
  Sync. Der Sync verwirft eine Gruppe, sobald ihre vollständige
  `serialized_parameter_bytes`-Summe diese Grenze übersteigt (contracts/sync-protocol.md).

## E3 Typisierte Fehler

- `Error::Database(DatabaseError)` behält die Variante der Datenbankschicht, etwa
  `VaultAlreadyOpenElsewhere` oder `TransactionTooLarge`, statt eines Fehlertexts.
- `Error::Consumer(Box<dyn Error + Send + Sync>)`, gebildet mit `Error::consumer(e)`, trägt einen
  eigenen Fehler aus einem Closure oder Hook heraus; der Aufrufer holt ihn per `downcast` zurück.
- `Error::sqlite_error()` und `DatabaseError::sqlite_error()` geben den SQLite-Fehler dahinter
  heraus, damit der Aufrufer dessen Code liest (etwa `SQLITE_NOTADB` bei falscher Passphrase).

## Tests in haex-crdt

- Zwei Schreibungen in einem `write` → dieselbe `haex_hlc_no_sync` in beiden Zeilen, eine
  Transaktionsgruppe im Scanner.
- `Err` oder Panik im Closure → keine Zeile, kein Eintrag im Lösch-Log.
- Schreibung auf eine `_no_sync`-Tabelle → geschrieben, ohne Stempel.
- Schreibung in `read` → abgewiesen.
- BLOB-Parameter kommen unverändert an.
- Summe über der eingestellten Grenze → `TransactionTooLarge`, nichts geschrieben.
- Die Tests der entfernten Schreibfunktion laufen gegen `Database::write`.
