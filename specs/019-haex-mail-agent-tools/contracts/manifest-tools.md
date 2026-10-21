# Vertrag: Werkzeuge von haex-mail

Der Block `tools` im Manifest von haex-mail (`apps/haex-mail/haextension/manifest.json` im Repository
haex-space/haextension) nach [`018-extension-agent-tools/contracts/manifest-tools.md`](../../018-extension-agent-tools/contracts/manifest-tools.md):
Name `^[a-z][a-z0-9_]{0,31}$`, Titel de/en (1–80 Zeichen), Beschreibung (1–1024), `inputSchema` nur mit
`type`, `description`, `properties`, `required`, `items`, `enum` (holzi `schema_in_subset`), `effect`,
Beispielsätze de/en (1–10, je 1–200 Zeichen). Das Manifest enthält keine Gleitkommazahlen.

Die Schemas unten sind die Quelle; Obergrenzen wie `limit` stehen nicht im Schema (die Teilmenge kennt keine
Schranken), der Handler kürzt und sagt es im Ergebnis. Eingaben, die das Schema nicht erfüllt, weist der
Baustein des vault-sdk vor dem Handler ab. Beschreibungen sind englisch (Modellsprache, wie in 018).

## Übersicht

| Werkzeug             | `effect`        | Handler (`app/tools/`) |
| -------------------- | --------------- | ---------------------- |
| `list_accounts`      | read            | `accounts.ts`          |
| `list_mailboxes`     | read            | `mailboxes.ts`         |
| `find_messages`      | read            | `messages.ts`          |
| `get_message`        | read            | `readMessage.ts`       |
| `list_rules`         | read            | `rules.ts`             |
| `mark_messages`      | change          | `messageChanges.ts`    |
| `move_messages`      | change          | `messageChanges.ts`    |
| `create_mailbox`     | change          | `mailboxes.ts`         |
| `create_draft`       | change          | `drafts.ts`            |
| `remove_rule`        | change          | `rules.ts`             |
| `import_account`     | change          | `accounts.ts`          |
| `open_account_setup` | change          | `accounts.ts`          |
| `add_rule`           | **destructive** | `rules.ts`             |
| `send_mail`          | **destructive** | `sendMail.ts`          |
| `delete_messages`    | **destructive** | `deleteMessages.ts`    |

15 Werkzeuge (Grenze 32). Die drei zerstörenden fragen auch im Modus „Auto“ (032 FR-007, Clarification
2026-10-20); ein Test im Repository haex-mail prüft die Zuordnung (siehe unten).

## Eingabe und Ergebnis je Werkzeug

Gemeinsam: `account` ist die Konto-Kennung aus `list_accounts`; fehlt sie, gilt „alle Konten“, wo das sinnvoll
ist. `ids` sind Mail-Kennungen `accountId::mailbox::uid` aus `find_messages`. Fehler sind
`isError: true` mit kurzer Meldung ohne Zugangsdaten und ohne Serverdetails.

**`list_accounts`** — Eingabe: `{}`. Ergebnis: `{ accounts: Konto[], importable: Importierbar[] }`,
`importable` höchstens 20, je `{ id, title, username }`.

**`list_mailboxes`** — Eingabe: `{ account?: string }`. Ergebnis: `{ mailboxes: Ordner[] }`
(`unseen`, `total` live; `cached: true` mit `error` je Konto bei Fehler, R7).

**`find_messages`** — Eingabe:

```json
{
  "type": "object",
  "properties": {
    "account": {
      "type": "string",
      "description": "Account id; all accounts when omitted."
    },
    "mailbox": {
      "type": "string",
      "description": "Mailbox name or role (inbox, sent, drafts, trash, junk, archive); inbox when omitted."
    },
    "unread": { "type": "boolean" },
    "flagged": { "type": "boolean" },
    "from": { "type": "string" },
    "to": { "type": "string" },
    "subject": { "type": "string" },
    "text": {
      "type": "string",
      "description": "Free text searched in the whole mailbox on the server."
    },
    "since": { "type": "string", "description": "YYYY-MM-DD" },
    "before": { "type": "string", "description": "YYYY-MM-DD" },
    "hasAttachment": { "type": "boolean" },
    "sort": { "type": "string", "enum": ["date", "sender", "subject"] },
    "descending": {
      "type": "boolean",
      "description": "Default true (newest first)."
    },
    "limit": {
      "type": "integer",
      "description": "Page size, default 25, at most 50."
    },
    "offset": { "type": "integer" }
  }
}
```

Ergebnis: `Seite` (`messages`, `total`, `hasMore`; `window`, `fallback`/`note`, `errors` nur, wenn sie
zutreffen). Bei mehreren Konten liefert jedes die Umschläge für `offset + limit` Treffer; `total` ist die
Summe, `hasMore` gilt bei `total > offset + limit`. Ohne jedes Kriterium wird alles seit 1900-01-01
gesucht (der Host verlangt mindestens eins). `offset + limit` darf bei neueste-zuerst höchstens 1000 sein.
`sort` ≠ `date` und `hasAttachment` brauchen die Umschläge selbst: das Werkzeug sieht die neuesten 200
Treffer je Ordner an (`window: 200` sagt es, wenn es mehr gab), `total` zählt dann die Treffer im Fenster.
Älteste zuerst (`descending: false`) holt alle Treffer (bis 1000), `window: 1000` bei mehr. Verweigert der
Server einen Suchtext mit Nicht-ASCII-Zeichen, prüft das Werkzeug die Umschläge der neuesten 1000 Mails selbst
(Absender, Empfänger, Betreff; `fallback: "local"` mit `note`). Ein Konto, das nicht antwortet, steht in
`errors`; antwortet keines, ist es ein Fehler. `mailbox` als Rolle oder
Name; ohne `account` und mit Ordnername, den ein Konto nicht hat, überspringt das Werkzeug dieses Konto.

**`get_message`** — Eingabe: `{ id: string }` (Pflicht). Ergebnis: `Mail`. Setzt nichts auf gelesen (R8).

**`list_rules`** — Eingabe: `{}`. Ergebnis: `{ rules: Regel[] }`.

**`mark_messages`** — Eingabe: `{ ids: string[], read?: boolean, flagged?: boolean }` (`ids` Pflicht, mindestens
eines von `read`/`flagged`; höchstens 100 `ids`, der Handler lehnt mehr ab). Ergebnis:
`{ done: string[], failed: { id, reason }[] }`.

**`move_messages`** — Eingabe: `{ ids: string[], to: string }` (`to` Rolle oder Ordnername des Kontos der Mail;
höchstens 100 `ids`). Ergebnis wie `mark_messages` (Fehler je Mail, FR-010).

**`create_mailbox`** — Eingabe: `{ account: string, name: string }`. Ergebnis: `{ created: boolean, name }`. Der Name
ist der, den der Nutzer schreibt (Umlaute eingeschlossen); an den Server geht er in IMAPs modifiziertem UTF-7. Danach
wird die Ordnerliste von haex-mail neu gelesen.

**`create_draft`** — Eingabe: `{ account: string, to?: string[], cc?: string[], subject?: string, body: string,
replyTo?: string, mode?: "reply" | "reply-all" | "forward" }`. `replyTo` ist die Kennung der Vorlage;
mit `mode` übernimmt der Handler Empfänger, Betreff und Bezug nach `buildReplyContextAsync`; `to`, `cc`,
`subject` überschreiben. Anhänge der Vorlage werden nicht übernommen. Ergebnis: `{ draftId, to, subject,
body }`, `to` mit allen Empfängern (to und cc). Legt nur ab, sendet nie.

**`send_mail`** — Eingabe: `{ draftId: string, to: string[], subject: string, body: string }` (alle
Pflicht; `to` nennt alle Empfänger des Entwurfs, cc eingeschlossen, damit die Abfrage alle zeigt). Ergebnis: `{ sent: true, to, subject }`. Die Zustimmungsabfrage zeigt nur die Eingabe (018
FR-011); damit sie Empfänger, Betreff und Text zeigt (FR-015), gehören diese Felder in die Eingabe. Der
Handler liest den Entwurf und vergleicht Empfänger, Betreff und Text (ohne Unterschiede in Leerraum und
Zeilenenden). Weicht etwas ab, sendet er nichts und antwortet mit Fehler und den Werten des Entwurfs, damit
das Modell es neu versuchen kann. Gesendet wird immer der Entwurf, nie die Eingabe: Was die Abfrage zeigt, ist
damit das, was geht. `create_draft` liefert dem Modell `to`, `subject` und `body` des Entwurfs zurück, damit es
sie übernehmen kann. Ein Entwurf mit Anhängen, ohne Empfänger oder außerhalb eines
Entwurfsordners wird nicht gesendet (Fehler); schlägt die Kopie nach „Gesendet“ oder das Entfernen des Entwurfs
nach dem Senden fehl, steht es im Ergebnis (`copySaved`, `draftRemoved`).

**`delete_messages`** — Eingabe: `{ messages: { id: string, subject: string }[] }` (höchstens 100). Die Abfrage
zeigt nur die Eingabe; mit Betreff statt nackter Kennung kann der Nutzer verständlich zustimmen (032
FR-008). Der Handler vergleicht den Betreff (ohne Unterschiede in Leerraum und Groß-/Kleinschreibung) mit
dem der Mail auf dem Server; weicht er ab, wird diese Mail nicht gelöscht und mit Grund gemeldet.
Ergebnis wie `mark_messages`.

**`add_rule`** — Eingabe:

```json
{
  "type": "object",
  "properties": {
    "name": { "type": "string" },
    "account": {
      "type": "string",
      "description": "Account id; all accounts when omitted."
    },
    "conditions": {
      "type": "array",
      "items": {
        "type": "object",
        "properties": {
          "field": {
            "type": "string",
            "enum": ["from", "to", "subject", "attachment"]
          },
          "contains": { "type": "string" }
        },
        "required": ["field"]
      }
    },
    "actions": {
      "type": "array",
      "items": {
        "type": "object",
        "properties": {
          "type": { "type": "string", "enum": ["move", "markRead", "flag"] },
          "to": {
            "type": "string",
            "description": "For move: mailbox role or name."
          }
        },
        "required": ["type"]
      }
    },
    "applyToExisting": { "type": "boolean" }
  },
  "required": ["name", "conditions", "actions"]
}
```

Ergebnis: `{ id, applied?: number, appliedLimit?: boolean }`. `applied` nur mit `applyToExisting`. Die Prüfregeln
aus `data-model.md` gelten; der Handler mappt `to` auf `role` (wenn es eine Rolle ist) oder `mailbox`.

**`remove_rule`** — Eingabe: `{ id?: string, name?: string }` (eines davon). Ergebnis: `{ removed: number }`.
Nicht gefunden → Fehler (FR-026); passt ein Name auf mehrere Regeln, entfernt es keine und nennt die
Kennungen.

**`import_account`** — Eingabe: `{ passwordItemId: string }`. Ergebnis: `{ account: Konto }`.

**`open_account_setup`** — Eingabe: `{ email?: string, imapHost?: string, imapPort?: integer, imapSecurity?:
"tls" | "starttls" | "none", smtpHost?: string, smtpPort?: integer, smtpSecurity?: "tls" | "starttls" | "none",
displayName?: string }`. Kein Feld für ein Passwort. Ergebnis: `{ prepared: true, hint: string }`, der Hinweis sagt,
dass der Nutzer das Fenster von haex-mail öffnen und das Passwort selbst eingeben muss.

## Beispielsätze (Auszug, je Werkzeug 4+ de und 4+ en im Manifest)

Die Sätze unterscheiden sich so, dass ähnliche Werkzeuge getrennt gefunden werden (018 R6, wortgewichtete
Überlappung). Beispiele je Werkzeug (die vollständige Liste legt die Aufgabe fest, die das Manifest schreibt):

| Werkzeug         | de                                                                                                             | en                                                                 |
| ---------------- | -------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------ |
| `list_mailboxes` | „Wie viele ungelesene Mails habe ich?“, „Habe ich neue Mails?“, „Welche Ordner gibt es?“                       | „How many unread mails do I have?“, „Do I have new mail?“          |
| `find_messages`  | „Zeig mir die Mails von Anna“, „Suche die Rechnung vom Elektriker“, „Was ist letzte Woche angekommen?“         | „Show me mails from Anna“, „Find the invoice from the electrician“ |
| `move_messages`  | „Verschieb diese Mails ins Archiv“, „Leg die Belege in den Ordner Steuer“                                      | „Move these mails to the archive“                                  |
| `add_rule`       | „Schieb Newsletter von shop.de künftig automatisch ins Archiv“, „Lege eine Regel für Mails von meiner Bank an“ | „Always archive newsletters from shop.de“                          |
| `send_mail`      | „Sende den Entwurf an Anna“, „Schick die Antwort ab“                                                           | „Send the draft to Anna“                                           |

## Tests (E3, haex-mail)

- Das Manifest erfüllt die Formatregeln 1–8 von 018 und die Schema-Teilmenge (ein Test, der `manifest.tools`
  prüft).
- `effect` ist `destructive` für genau `add_rule`, `send_mail`, `delete_messages`, `read` für genau die fünf
  Lesewerkzeuge und sonst `change`.
- Jedes Werkzeug im Manifest hat einen registrierten Handler und umgekehrt (`sdk.tools.register` wirft für
  nicht erklärte Namen).
- Kein Ergebnis und keine Fehlermeldung enthält ein Passwort (Test mit einem Konto, dessen Passwort ein
  erkennbarer Marker ist; alle Handler laufen, die Ergebnisse werden danach durchsucht).
