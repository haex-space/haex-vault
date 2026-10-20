# Datenmodell: haex-mail für den Agenten, Regeln und Regel-Oberfläche

Alle Änderungen liegen in der Erweiterungs-Datenbank von haex-mail (Repository haex-space/haextension,
`apps/haex-mail/app/database/schemas/index.ts`, Migration 0003). holzi bekommt keine neue Tabelle und keine
neue Spalte; die Bridge-Methoden (`contracts/host-mail-commands.md`) sind zustandslos.

Tabellennamen entstehen wie bei den vorhandenen Tabellen über `getTableName(manifest.publicKey,
packageJson.name, name)`. Die Erweiterungs-Datenbank wird über die Vault zwischen eigenen Geräten
synchronisiert (017); das gilt für die neue Tabelle wie für `accounts`.

## Neue Tabelle `rules`

| Spalte                     | Typ                                 | Bedeutung                                                   |
| -------------------------- | ----------------------------------- | ----------------------------------------------------------- |
| `id`                       | text, Primärschlüssel               | UUID                                                        |
| `name`                     | text, nicht leer                    | Anzeigename, 1–80 Zeichen                                   |
| `account_id`               | text, leer erlaubt                  | Konto oder leer für „alle Konten“                           |
| `enabled`                  | integer (Wahrheitswert), Standard 1 | An/Aus-Schalter                                             |
| `conditions_json`          | text (JSON)                         | Liste der Bedingungen, mindestens eine                      |
| `actions_json`             | text (JSON)                         | Liste der Aktionen, mindestens eine                         |
| `origin`                   | text                                | `user` oder `agent`                                         |
| `created_at`, `updated_at` | wie `timestamps` der Datei          | Reihenfolge der Regeln: `created_at` aufsteigend, dann `id` |

Index: keiner nötig (wenige Zeilen, je Abruf ein Lesen der aktiven Regeln).

### `conditions_json`

Liste von Objekten, alle müssen zutreffen (UND):

- `{ "field": "from" | "to" | "subject", "contains": string }`: `contains` nicht leer nach `trim`, höchstens
  200 Zeichen; Vergleich ohne Beachtung der Schreibweise als Teilzeichenkette. `to` prüft `to` und `cc`;
  `from` und `to` vergleichen den Text `Name <adresse>`.
- `{ "field": "attachment" }`: die Mail hat einen Anhang (`hasAttachments`).

Höchstens 8 Bedingungen je Regel.

### `actions_json`

Liste von Objekten, mindestens eine, höchstens 4, ohne doppelte Art:

- `{ "type": "move", "role": MailboxRole }` oder `{ "type": "move", "mailbox": string }` (Ordnername im
  Konto). Eine Regel für „alle Konten“ darf nur `role` verwenden.
- `{ "type": "markRead" }`
- `{ "type": "flag" }`

Es gibt keine Art, die Mails weiterleitet oder sendet (FR-020).

### Prüfregeln (`lib/rules.ts`, `validateRule`)

1. `name`, `conditions`, `actions` wie oben; Verstoß liefert eine Fehlermeldung mit Feld.
2. Ist `account_id` gesetzt, gehört es zu einem vorhandenen Konto.
3. `move` mit `mailbox` nur mit gesetztem Konto.
4. `origin` ist `user` für Einträge aus der Oberfläche und `agent` für Einträge aus `add_rule`; die
   Oberfläche überschreibt `origin` beim Bearbeiten nicht.

### Zustand „Zielordner fehlt“ (FR-024)

Wird nicht gespeichert, sondern bei Anzeige und Ausführung aus den Ordnern des Kontos berechnet:
`move` mit `role` ist erfüllbar, wenn das Konto einen Ordner mit dieser Rolle hat; `move` mit `mailbox`
ist erfüllbar, wenn der Ordner existiert. Nicht erfüllbar → Regel wird übersprungen und in der Liste
markiert; es werden auch die anderen Aktionen dieser Regel nicht ausgeführt (die Regel gilt als Ganzes).

## Geänderte Tabelle `mailboxes`

Neue Spalte `rules_uid_through` (integer, leer erlaubt, Standard leer): die höchste UID, bis zu der die
Regeln für dieses Postfach gelaufen sind. Nur für INBOX-Zeilen belegt.

Zustände:

| Zustand            | Wert | Übergang                                                                                                                    |
| ------------------ | ---- | --------------------------------------------------------------------------------------------------------------------------- |
| nicht eingerichtet | leer | Erster Abruf: setze auf höchste vorhandene UID, ohne Regeln auszuführen                                                     |
| bereit             | UID  | Abruf mit Mails oberhalb: Regeln laufen für diese UIDs, dann rückt der Wert auf die höchste                                 |
| zurückgesetzt      | leer | `UIDVALIDITY` ändert sich (`invalidateMailboxCacheAsync`): Zeile verliert den Wert, nächster Abruf wie „nicht eingerichtet“ |

## Ergebnisformen der Werkzeuge (kein Speicher)

Alle Ergebnisse sind JSON im Textblock (018 contracts/mcp-port.md), höchstens 64 KiB.

- **Konto**: `{ id, displayName, email }`; nie Server-Zugangsdaten. `importable`: `{ id, title,
username }[]`.
- **Ordner**: `{ account, name, role | null, unseen, total, cached? }`.
- **Umschlag**: `{ id, account, mailbox, from, subject, date (ISO), unread, flagged, hasAttachments }`,
  `from` als Text `Name <adresse>`.
- **Seite**: `{ messages, total, hasMore, window? }`.
- **Mail**: Umschlag plus `to`, `cc`, `text`, `truncated`, `attachments: { name, type, size }[]`.
- **Regel**: `{ id, name, account | null, enabled, conditions, actions, origin, targetMissing }`.

Alle `id`-Werte sind die vorhandenen Schlüssel (`accountId::mailbox::uid` für Mails und Entwürfe,
`id` der Tabellen für Konten und Regeln).
