# Implementation Plans

Bookmark-Speicherung und -Sync für `haex-pass-browser`, gespeichert in der
Passwortdatenbank von `haex-vault`. Execute plans in the order below. Each
executor must read the complete plan, honor its STOP conditions, run every
verification gate, and update the status row when finished.

Vorgänger-Pläne (HTML-Blob-Ansatz) liegen in `archive/` und sind **verworfen** —
siehe „Findings considered and rejected".

## Execution order & status

| Plan | Title | Repo | Priority | Effort | Depends on | Status |
|---|---|---|---|---|---|---|
| 001 | Syncbare Bookmark-Sammlungen, Berechtigung und Bridge-Methoden | haex-vault (V) | P1 | M | — | DONE |
| 002 | Bookmark-Onboarding, Sammlungswahl und Gerätesync integrieren | haextension (H) | P1 | L | 001 | TODO |

Status values: TODO | IN PROGRESS | DONE | BLOCKED (with a one-line reason) |
REJECTED (with a one-line rationale)

## Produktanforderungen

- Bei **jeder Installation** von `haex-pass-browser` wird gefragt, ob Lesezeichen
  gespeichert werden sollen.
- Sind bereits Lesezeichen im Vault vorhanden, wird angeboten, sie zu **laden**.
- Beim **Anlegen** einer neuen Sammlung werden die vorhandenen Browser-Lesezeichen
  als Startinhalt übernommen (gespeichert, nichts geht verloren).
- **Mehrere Browser bleiben unabhängig**: der Nutzer legt **beliebig viele**
  benannte **Sammlungen** an (privat, beruflich, projektbezogen, …). Jeder Browser
  zeigt genau eine aktive Sammlung; verschiedene Sammlungen mischen nie. Es gibt
  keine feste Anzahl und keine vordefinierten Sammlungen.
- Der Nutzer kann **jederzeit zwischen Sammlungen umschalten** und zurück. Der
  native Browser-Baum ist eine Anzeige der aktiven Sammlung; beim Wechsel werden
  die angezeigten Lesezeichen (nach Warnung) ersetzt. **Kein Backup** — das ist
  kein Datenverlust, weil jede Sammlung vollständig im Vault liegt. Umschalten ist
  gesperrt, solange ungesicherte Änderungen bestehen und kein Sync möglich ist.

## Kernidee

Lesezeichen werden als **strukturierte Zeilen** in syncbaren Tabellen gespeichert
(`haex_bookmark_collections`, `haex_bookmarks`, `haex_bookmark_devices`) — nicht
als opake HTML-Datei. Eine **Sammlung** ist die Unabhängigkeitsgrenze: Browser
derselben Sammlung konvergieren (kanonische Roots Toolbar↔Toolbar usw.),
verschiedene Sammlungen bleiben vollständig getrennt. Damit übernimmt die
**bereits vorhandene CRDT-Sync-Schicht des Vaults** die komplette Konvergenz:

- Jede Tabelle, deren Name nicht auf `_no_sync` endet, bekommt automatisch die
  CRDT-Spalten (`haex_hlc`, `haex_column_hlcs`) und wird spaltengenau gemergt
  (Rust `CrdtTransformer`, siehe `src-tauri/src/crdt/transformer/mod.rs`).
- Hard-DELETEs schreiben automatisch Tombstones nach `haex_deleted_rows`
  (BEFORE-DELETE-Trigger), also Delete-wins ohne eigenen Code
  (`src/database/schemas/crdt.ts`).
- UNIQUE-Konflikte landen in `haex_crdt_conflicts`.

Die Browser-Erweiterung baut deshalb **keine** eigene Merge-Engine mehr. Sie
spiegelt nur den nativen Browser-Baum in Tabellenzeilen und wieder zurück.

## Dependency notes

- Plan 001 legt in V die drei syncbaren Tabellen, die `bookmarks`-Berechtigung und
  sechs dünne Bridge-Methoden (collections-list/collection-create/list/upsert/
  delete/device-upsert) an. Reine Datenschicht, kein Browser-Code.
- Plan 002 baut in H das Onboarding mit Sammlungswahl, den Backup-Move für
  bestehende lokale Lesezeichen, den nativen Adapter (kanonische Roots) und die
  Sync-Zustandsmaschine gegen die feste API aus 001. Es darf reale
  Browser-Lesezeichen erst verändern, wenn 001 abgeschlossen und getestet ist.

## Findings considered and rejected

- **Opake HTML-Replikdatei pro Gerät + selbstgebaute CRDT-Engine** (der
  ursprüngliche `archive/`-Ansatz): verworfen. Er baute Lamport-Clocks,
  Tombstones, Delete-wins, konvergenten Merge und kanonische Serialisierung im
  Browser-Add-on von Hand nach — genau das, was der Vault beim `CREATE TABLE`
  schon automatisch und feiner (spaltengenau) leistet. Die Blob-Form war nötig,
  weil ein opaker Anhang nicht zeilenweise gemergt werden kann; sie war die
  Ursache fast der gesamten Komplexität. Der Nutzer hat eine HTML-Datei nie
  gefordert.
- **Portables Netscape-HTML als Sync-Format**: verworfen. Wer eine HTML-Datei
  braucht, exportiert sie jederzeit direkt aus dem Browser. Ein optionaler
  Einweg-Export aus `haex-bookmarks` ist ein trivialer Follow-up, kein
  Sync-Fundament.
- **Lesezeichen in bestehende Passwort-Items packen** (statt eigener Tabelle):
  verworfen. Bookmarks sind eine eigene Datendomäne; sie würden das Passwort-UI,
  das Tag-/Gruppenmodell und die Berechtigung verwässern. Eine dedizierte Tabelle
  reitet auf denselben Sync-Schienen, bleibt aber sauber getrennt — inklusive
  eigener `bookmarks`-Berechtigung.
- **Automatische Deduplizierung nach URL/Titel**: verworfen. Identische URLs in
  verschiedenen Ordnern sind absichtlich möglich; Zusammenlegen zerstört lokale
  ID-Mappings.
- **Native System-Roots löschen/ersetzen**: verworfen. Chrome/Firefox lassen
  System-Roots nicht frei modifizieren. Innerhalb einer Sammlung konvergieren
  stattdessen **kanonische Roots** (Toolbar↔Toolbar usw.); nur die veränderbaren
  Nachfahren werden angefasst.
- **Backup-Ordner beim Laden/Umschalten**: verworfen (nach anfänglicher Planung).
  Weil jede Sammlung vollständig im Vault liegt, ist das Ersetzen der angezeigten
  Lesezeichen kein Datenverlust; der native Baum ist nur eine Anzeige der aktiven
  Sammlung. Statt Backup wird vor dem Ersetzen klar gewarnt. Umschalten ist
  gesperrt, solange die aktive Sammlung ungesicherte Änderungen hat.
- **Ein gemeinsamer Bookmark-Satz für alle Browser** (ohne Sammlungen):
  verworfen. Ein privater und ein beruflicher Browser würden sich vermischen.
  Sammlungen sind die Unabhängigkeitsgrenze; die Konvergenz-Schicht bleibt
  darunter dieselbe.
- **Neues WebSocket-Push-Protokoll**: verworfen. Die bestehende
  Request/Response-Bridge plus 5-Minuten-Alarm und Sync an Lebenszyklus-Grenzen
  reicht für das MVP.
