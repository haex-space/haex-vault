# Vertrag: drei neue Mail-Methoden der Bridge

Ergänzt die Zeile der Mail-Methoden in [`017-extension-host/contracts/bridge.md`](../../017-extension-host/contracts/bridge.md)
um `extension_mail_search`, `extension_mail_create_mailbox`, `extension_mail_delete_messages`. Es gelten
dieselben Regeln wie dort: Zugangsdaten kommen mit jedem Aufruf (`imap`), IMAP verlangt die Berechtigung
`mail`/`fetch` für `host:port`, `host` oder `*`, 60 s je Aufruf, Postfachnamen ohne Steuerzeichen
(`check_mailbox`), Fehler nennen weder Passwort noch Benutzername, jede Antwort über der Grenze der
Erweiterung ist 7000.

Implementiert in `src-tauri/src/extensions/mail/commands.rs` und dem neuen `imap_manage.rs`, eingetragen in
`src-tauri/src/extensions/bridge/allowlist.rs` (`module: mail::commands::MODULE`), im vault-sdk als
`MAIL_COMMANDS.search`, `.createMailbox`, `.deleteMessages` (`src/commands/mail.ts`).

## `extension_mail_search`

Parameter: `{ imap, mailbox, criteria, limit? }`

`criteria` (alle Felder optional, mindestens eines, sonst 3001):

| Feld                    | Typ                 | IMAP                                                     |
| ----------------------- | ------------------- | -------------------------------------------------------- |
| `text`                  | string              | `TEXT "<…>"` (Kopf und Text)                             |
| `from`, `to`, `subject` | string              | `FROM`, `TO`, `SUBJECT`                                  |
| `since`, `before`       | string `YYYY-MM-DD` | `SINCE`, `BEFORE` (Datum ohne Uhrzeit, Zone des Servers) |
| `unseen`                | boolean             | `UNSEEN` (true) oder `SEEN` (false)                      |
| `flagged`               | boolean             | `FLAGGED` (true) oder `UNFLAGGED` (false)                |

Alle Kriterien gelten zugleich (UND). Zeichenketten dürfen keine Steuerzeichen enthalten (3001) und sind
höchstens 200 Zeichen lang; der Host maskiert `"` und `\`. `since` und `before` müssen gültige Daten sein
(3001).

`limit`: höchstens `MAX_ENVELOPES` (1000), Standard 1000; größere Werte sind 3001.

Antwort: `{ uids: number[], total: number }`. `uids` sind die neuesten `limit` Treffer, absteigend nach UID;
`total` ist die Zahl aller Treffer.

Nicht-ASCII-Text: Der Host sendet `CHARSET UTF-8`. Kann er das nicht gültig übertragen (siehe research R2),
antwortet er mit 3001 „text not searchable on this connection“; die Erweiterung filtert dann lokal.

Nicht vorhandener Ordner: 1001.

## `extension_mail_create_mailbox`

Parameter: `{ imap, mailbox }`

Antwort: `{ created: boolean }`. Existiert der Ordner schon, ist `created` `false` und es ist kein Fehler.
Der Name darf das Trennzeichen des Servers enthalten (verschachtelter Ordner); ob der Server fehlende
Elternordner anlegt, bestimmt er. Server-Ablehnung (Name ungültig, Berechtigung) ist 2005 ohne
Serverdetails über das hinaus, was `MailError::Imap` heute nennt.

## `extension_mail_delete_messages`

Parameter: `{ imap, mailbox, uids }`

`uids`: 1 bis 1000 Einträge (leer → `null` ohne Serveraufruf wie bei `move_messages`; mehr als 1000 → 7000).

Ablauf: Anmelden, `CAPABILITY`; fehlt `UIDPLUS`, Fehler 2005 „the server cannot UID EXPUNGE“ **ohne**
irgendeine Mail zu markieren. Sonst `SELECT`, `UID STORE <uids> +FLAGS (\Deleted)`, `UID EXPUNGE <uids>`.

Antwort: `null`. Nicht vorhandene UIDs sind kein Fehler.

Eine Mail, die nur markiert wurde und deren `UID EXPUNGE` scheiterte, bleibt mit `\Deleted` im Ordner; der Host
meldet den Fehler (2005) und nimmt das Flag nicht zurück. Die Erweiterung zeigt solche Mails nicht als
gelöscht an.

## Tests (E2a)

`extensions/mail/mail_tests.rs`, gegen den erweiterten Testserver:

- Suche: jedes Kriterium einzeln und in Kombination; mehr Treffer als `limit` (`total` stimmt); nicht
  vorhandener Ordner; Steuerzeichen; Nicht-ASCII-Fall (Verhalten laut R2).
- Ordner anlegen: neu, vorhanden (`created: false`), ungültiger Name.
- Löschen: nur die genannten UIDs verschwinden, andere Mails mit `\Deleted` bleiben; Server ohne UIDPLUS
  verweigert und lässt alle Mails unverändert.
- Berechtigung: ohne `fetch` für Host und Port jeweils `PermissionDenied`; mit `fetch` für einen anderen Server
  ebenso (wie `each_server_and_port_needs_its_own_permission`).
