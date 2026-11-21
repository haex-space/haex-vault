# Vertrag: SQL von Erweiterungen

Begründung: [research.md](../research.md) R6–R8. Gilt für `extension_database_query`, `_execute`,
`_transaction` und für Migrationen.

## Begriffe

- **Präfix** einer Erweiterung: `<publicKey>__<name>__`. Ein Tabellenname wird exakt in genau drei Teile an
  `__` zerlegt (ohne Rücksicht auf Groß-/Kleinschreibung); Name der Erweiterung und Tabelle enthalten kein `__`.
- **Eigene Tabelle**: Präfix = Präfix der aufrufenden Erweiterung.
- **Fremde Tabelle**: Präfix einer anderen installierten Erweiterung. Ein wohlgeformtes Präfix
  (`<64 Hex>__<name>__`) einer nicht installierten Erweiterung bekommt dieselbe Antwort wie eine fremde Tabelle
  ohne Berechtigung (FR-062); die Anfrage an den Nutzer zeigt dann „nicht installiert“ und bietet nur
  „Verweigern“. Hat die aufrufende Erweiterung eine Berechtigung auf das Präfix einer nicht installierten
  Erweiterung (auch nach „Entfernen, Daten behalten“), antwortet haex-vault wie bei einer nicht vorhandenen Tabelle
  (US6 Szenario 5): 2000 `no such table: <name>`.
- **Kerntabelle**: alles andere, einschließlich `sqlite_*`, `haex_*`, `pragma_*`-Funktionen.

## Laufzeit

### Vorprüfung (haex-vault)

| Regel                                                                                                                             | Ergebnis bei Verstoß              |
| --------------------------------------------------------------------------------------------------------------------------------- | --------------------------------- |
| genau eine Anweisung (`parse_sql_statements(...).len() == 1`)                                                                     | 1000                              |
| Art ∈ {Query, Insert, Update, Delete}                                                                                             | 1000                              |
| keine Sync-Spalten (`haex_*`) gesetzt oder gelesen per Name                                                                       | 1000                              |
| Qualifizierer nur `main` oder keiner                                                                                              | 1000                              |
| Bezeichner nur ASCII                                                                                                              | 1000                              |
| kein `WITH`-Name gleich einem echten Tabellennamen oder in der Form einer Erweiterungstabelle (ob es sie gibt oder nicht, FR-062) | 1000                              |
| SQL-Länge ≤ `max_sql_bytes`                                                                                                       | 7000                              |
| jede Tabelle aus `visit_relations` (ohne eigene `WITH`-Namen) ist eigen oder fremd                                                | Kerntabelle → 1000 ohne Rückfrage |
| fremde Tabelle: Leseberechtigung (Query) bzw. Lesen und Schreiben (DML)                                                           | fehlt → 1004 (Anfrage) bzw. 1002  |

### Authorizer (haex-crdt `SqlGuard`, entscheidet)

Oberste Ebene (`accessor = None`):

| Aktion                       | erlaubt, wenn                                                                                    |
| ---------------------------- | ------------------------------------------------------------------------------------------------ |
| `Read`, `Select`             | `database = main` und Tabelle eigen oder einer installierten Erweiterung mit Leseberechtigung    |
| `Insert`, `Update`, `Delete` | `database = main` und Tabelle eigen oder einer installierten Erweiterung mit Lesen und Schreiben |
| `Function`                   | Funktion in der Erlaubtliste (unten)                                                             |
| `Recursive`                  | immer (rekursive `WITH`)                                                                         |
| alles andere                 | nie                                                                                              |

Liest eine Anweisung keine Spalte einer Tabelle (`count(*)`, `SELECT 1`, `EXISTS`), meldet SQLite `Read` mit
leerem Spaltennamen und dem Schema, wie es in der Anweisung steht, ohne Qualifizierer also ohne Datenbank; das
zählt wie `main` (Erweiterungen erreichen kein anderes Schema), ein `WITH`-Name bleibt ein `WITH`-Name und
`json_each` und Verwandte bleiben erlaubt. Das gilt auch für Migrationen.

In Triggern: erlaubt nur, wenn `accessor` `z_dirty_<T>_(insert|update|delete)` ist und T eine Tabelle, die
diese Anweisung schreiben darf.

**Erlaubte Funktionen**: Kernfunktionen von SQLite (`abs`, `coalesce`, `ifnull`, `iif`, `instr`, `length`,
`lower`, `upper`, `ltrim`, `rtrim`, `trim`, `max`, `min`, `nullif`, `printf`, `format`, `quote`, `random`,
`randomblob`, `replace`, `round`, `sign`, `substr`, `substring`, `typeof`, `unicode`, `char`, `hex`, `unhex`,
`zeroblob`, `glob`, `like`, `likely`, `unlikely`, `changes`, `last_insert_rowid`, `total_changes`),
Aggregate und Fensterfunktionen, Datum und Zeit (auch `CURRENT_DATE`, `CURRENT_TIME`, `CURRENT_TIMESTAMP`), Mathefunktionen, JSON-Funktionen einschließlich
`json_each`/`json_tree` (Tabellenwertfunktionen nur, wenn der Test zeigt, dass sie nicht über Kerntabellen
lesen können). Funktionen, die der CRDT-Transformer selbst in die Anweisung einsetzt, erlaubt der Authorizer
nur, wenn sie aus dem Transformer stammen (Prüfaufgabe: ob der Transformer HLC-Werte als Literal oder als
Funktionsaufruf einsetzt); eine Erweiterung, die sie selbst aufruft, wird in der Vorprüfung abgelehnt. **Nie**: `load_extension`, `fts3_tokenizer`, `sqlite_compileoption_*`,
`sqlite_offset`, `sqlcipher_*`, `pragma_*`, unbekannte UDFs.

Lehnt der Authorizer etwas ab, was die Vorprüfung durchgelassen hat: 1000 „Form nicht zuordenbar“.

### Ausführung

- Ergebnis-Anweisungen (`SELECT`, `RETURNING`) über `query_map`, sonst `execute`.
- Ergebnis `{rows, columns, rowsAffected, lastInsertId}`; Sync-Spalten entfernt.
- „Tabelle fehlt“ hat eine Schreibweise: `no such table: <name>`, klein und ohne `main.`, ob SQLite sie meldet
  oder haex-vault die Tabellen einer nicht installierten Erweiterung verbirgt.
- Werte: BLOB → Base64-Text; INTEGER → Zahl; REAL NaN/Inf → `null`. Parameter: Zahl, Text, `null`,
  Wahrheitswert → 0/1, `{"$bytes": b64}` → BLOB, Array/Objekt → JSON-Text.
- Grenzen: Zeilen und Bytes im Callback, Laufzeit über Fortschritts-Callback (`SQLITE_INTERRUPT`),
  gleichzeitige Anfragen per Semaphore. Kein einzelner Wert und keine Zeile darf größer als die Antwortgrenze
  werden (`SqlGuard::max_value_bytes` von haex-crdt, `SQLITE_LIMIT_LENGTH`), auch nicht in Migrationen; SQLite
  lehnt z. B. `zeroblob(1e9)` ab, bevor der Wert angelegt wird. Überschreitung → 7000, die Transaktion wird
  zurückgerollt.
- Transaktion: alle Anweisungen erst vorgeprüft, dann in einem `write_guarded`; `rowsAffected` = Summe.
- Schreibanweisungen laufen durch den CRDT-Transformer (Zeitstempel, Löschmarken, Größengrenze), FR-028.

## Migrationen

Je Anweisung gelten die Tabellenregeln der Laufzeit (Qualifizierer, ASCII, `WITH`-Namen, Sync-Spalten,
eigene/fremde/Kerntabelle) ohne die Regel zur Art der Anweisung; `__new_<eigenes Präfix>…` zählt als eigene
Tabelle. Fremde Tabellen sind in Migrationen nie erlaubt. Dazu:

| erlaubt                                             | Bedingung                                                                                                                                |
| --------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------- |
| `CREATE TABLE`                                      | eigener Name oder `__new_<eigenes Präfix>…`; kein `AS SELECT`; keine `TEMP`; keine Spalte `haex_*`; `REFERENCES` nur auf eigene Tabellen |
| `CREATE [UNIQUE] INDEX`                             | auf eigener Tabelle                                                                                                                      |
| `ALTER TABLE … ADD/RENAME/DROP COLUMN`, `RENAME TO` | eigene Tabelle; kein Umbenennen über die Grenze `_no_sync`                                                                               |
| `DROP TABLE`, `DROP INDEX`                          | eigene Tabelle bzw. Index darauf                                                                                                         |
| `INSERT`/`UPDATE`/`DELETE`                          | nur eigene Tabellen (auch der Umbau `INSERT INTO __new_… SELECT … FROM …`)                                                               |
| `PRAGMA foreign_keys = OFF/ON`                      | nur als Steuerung des Schema-Modus, wird nicht ausgeführt                                                                                |

Verboten: `VIEW`, `TRIGGER`, `VIRTUAL`, `ATTACH`, `DETACH`, jedes andere `PRAGMA`, `VACUUM`, Transaktionssteuerung.

**Authorizer im Migrationsprofil** (oberste Ebene, `database = main`):

| Aktion                                                                                                            | erlaubt, wenn                                                                 |
| ----------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------- |
| `CreateTable`, `DropTable`, `AlterTable`                                                                          | Tabelle eigen oder `__new_<eigenes Präfix>…`                                  |
| `CreateIndex`, `DropIndex`                                                                                        | Index auf einer solchen Tabelle                                               |
| `Read`, `Select`, `Insert`, `Update`, `Delete`                                                                    | Tabelle eigen oder `__new_<eigenes Präfix>…`                                  |
| `Function`, `Recursive`                                                                                           | wie zur Laufzeit                                                              |
| Schreiben in `sqlite_master` durch eine erlaubte DDL                                                              | erlaubt (SQLite meldet es als `Insert`/`Update`/`Delete` auf `sqlite_master`) |
| alles andere (`CreateTemp*`, `CreateView`, `CreateTrigger`, `CreateVtable`, `Pragma`, `Attach`, `Transaction`, …) | nie                                                                           |

Die Trigger und der Umbau, die haex-crdt nach einer DDL selbst anlegt, laufen außerhalb des Authorizers (T005).

Ablauf je Migration in einem `write_guarded` im Schema-Modus (Fremdschlüssel aus, `foreign_key_check` vor dem
Commit): Anweisungen ausführen → für jede geänderte synchronisierte Tabelle Trigger neu anlegen → Umbau mit
unveränderten Sync-Spalten → Journalzeile schreiben. Scheitert etwas, wird alles zurückgerollt (FR-034).
Eine Migration hat ein eigenes Zeitlimit (60 s, Fortschritts-Callback wie zur Laufzeit), damit SQL einer
Erweiterung die Schreibsperre des Vaults nie unbegrenzt hält. Lesen der offenen und Anwenden laufen je Prozess
unter einer Sperre, damit zwei gleichzeitige Starts dieselbe Migration nicht zweimal anwenden.

## Umgehungssammlung (SC-002, `src-tauri/tests/extension_sql_bypass.rs`)

Jeder Fall dieser Liste ist eine verbotene Form und muss von der Vorprüfung **und** vom Authorizer jeweils
allein abgelehnt werden. Positive Fälle und Fälle mit eigener Regel je Schicht stehen darunter. Mindestens:

- `WITH x AS (SELECT * FROM chat_threads) SELECT * FROM x`; `WITH` mit eigenem Präfix als Name über eine
  Kerntabelle
- `EXISTS`, Unterabfragen in `SELECT`, `WHERE`, `HAVING`, `ORDER BY`, `LIMIT`, `VALUES`, `RETURNING`,
  `ON CONFLICT … DO UPDATE SET x = (SELECT …)`, `JOIN … ON (SELECT …)`, `CASE`, Funktionsargumente
- `main.chat_threads`, `temp.x`, `"Chat_Threads"`, `[chat_threads]`, `` `chat_threads` ``
- Präfix-Kollision `a` gegen `a__b`; Tabelle einer nicht installierten Erweiterung
- `sqlite_master`, `sqlite_schema`, `pragma_table_info('chat_threads')`, `json_each((SELECT … FROM
chat_threads))`
- `load_extension(...)`, `sqlcipher_export(...)`, `fts3_tokenizer(...)`
- `BEGIN`, `COMMIT`, `SAVEPOINT`, `ATTACH`, Kommentar-Tricks
- in Migrationen: `CREATE VIEW`, `CREATE TRIGGER`, `CREATE VIRTUAL TABLE`, `CREATE TEMP TABLE`, Spalte
  `haex_hlc_no_sync` in einer `_no_sync`-Tabelle, `REFERENCES chat_threads`, `PRAGMA writable_schema`,
  `INSERT INTO chat_threads …`, `UPDATE haex_crdt_configs_no_sync …`

Positive Fälle, die beide Schichten zusammen und jede allein durchlassen müssen: Lesen, Einfügen, Ändern und
Löschen in eigenen Tabellen, `RETURNING`, `ON CONFLICT DO UPDATE`, rekursives `WITH` über eigene Tabellen,
`json_each` über ein Literal, `DELETE` in einer eigenen `_no_sync`-Tabelle (erzeugt keine Löschmarke, FR-029).

Fälle mit eigener Regel je Schicht:

- Trigger: Schreiben in eine eigene synchronisierte Tabelle löst `z_dirty_<T>_*` aus; der Authorizer lässt
  diesen Zugriff zu. Ein Trigger-Zugriff auf eine Tabelle, die die Anweisung nicht schreiben darf, lehnt er ab.
  Die Vorprüfung sieht Trigger nicht.
- Transformer: vom CRDT-Transformer eingesetzte Funktionen lässt der Authorizer zu. Ruft die Erweiterung
  dieselbe Funktion selbst auf, lehnt die Vorprüfung ab.
- Sync-Spalten: Schreiben in `haex_hlc_no_sync` und andere `haex_*`-Spalten lehnt die Vorprüfung ab (1000).
  Der Authorizer sieht bei `INSERT` keine Spalten, und der Transformer schreibt diese Spalten selbst; ohne
  Vorprüfung muss der gespeicherte Wert trotzdem vom Transformer stammen (er überschreibt oder verwirft Werte
  der Anweisung).
- `WITH` mit dem Namen einer Kerntabelle (`WITH chat_threads AS (SELECT 1) SELECT * FROM chat_threads`): die
  Vorprüfung lehnt ab (1000). Der Authorizer allein sieht nur die CTE und lässt sie zu; gelesen wird nichts
  Echtes, die Antwort ist die eigene Zeile der CTE.
- Tabellenfunktionen (`json_each`, `json_tree`, `jsonb_*`): SQLite meldet sie als `Read` auf die gleichnamige
  Tabelle in `main`; der Authorizer lässt diese zu, was ihre Argumente lesen, prüft er einzeln.
- CTEs: SQLite meldet den Körper einer CTE mit ihrem Namen als `accessor` und das Lesen einer CTE ohne
  Datenbank. Der Authorizer kennt die CTE-Namen der Anweisung und prüft beides wie SQL auf oberster Ebene; eine
  CTE mit dem Namen eines Änderungstriggers gewinnt nichts.
- Änderungstrigger (Ergebnis der Prüfaufgabe): der Transformer setzt den HLC als Literal in die Anweisung;
  die Trigger rufen `gen_uuid` und `current_hlc` auf, lesen `haex_crdt_configs_no_sync` und lösen beim Löschen
  den Trigger von `haex_deleted_rows` aus. Im Trigger erlaubt der Authorizer nur die eigene Tabelle und
  `haex_*`-Tabellen sowie diese beiden Funktionen.
- Zwei Anweisungen in einer Zeichenkette: die Vorprüfung lehnt ab (1000); ohne Vorprüfung lehnen
  `write_guarded`/`read_guarded` einen nicht leeren Rest nach der ersten Anweisung ab, statt ihn still zu
  verwerfen (T005).
