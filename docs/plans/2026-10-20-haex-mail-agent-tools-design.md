# haex-mail: Werkzeuge für den Agenten, Regeln und Regel-Oberfläche (Spec 019)

Stand: 2026-10-20. Entwurf aus dem Brainstorming, noch keine Spec. Baut auf Spec 018
(Erweiterungswerkzeuge über MCP) auf, die Werkzeuge in den bestehenden haextensions ausdrücklich
Spec 019 zuweist.

## Ziel

Der eingebaute Agent kann in haex-mail Konten anlegen, Mails lesen, filtern, suchen, sortieren, in Ordner
einsortieren, Entwürfe erstellen und senden. Dazu kommen dauerhafte Regeln, die auch der Nutzer in
haex-mail von Hand anlegen, ändern und löschen kann. Anlass: „Wie viele ungelesene Mails habe ich?“ soll
ein Werkzeug beantworten statt eines geratenen `run_command`.

## Ausgangslage

- holzi-Seite von 018 ist fertig (`src-tauri/src/extensions/agent_tools/`, Manifest-Feld `tools`).
- haex-mail (`haextension/apps/haex-mail`) hat kein `tools`-Feld und keinen MCP-Baustein.
- Der MCP-Baustein im vault-sdk (`sdk.tools.register`) liegt nur auf dem Branch `feat/agent-tools`, ist
  nicht gemergt und nicht released (neuestes Tag v4.5.0, haex-mail hängt an `^4.1.0`).
- Der Host (`src-tauri/src/extensions/mail/commands.rs`) kennt `list_mailboxes`, `fetch_envelopes`,
  `fetch_message`, `fetch_attachment`, `set_flags`, `move_messages`, `append_message`, `send_message`,
  `build_rfc822`, Watch. Es fehlen IMAP-SEARCH, Ordner anlegen und endgültiges Löschen.
- haex-mail sucht heute nur clientseitig in der geladenen Liste und kennt weder Regeln noch Entwürfe.

## Entscheidungen

| Thema | Entscheidung |
| --- | --- |
| Konto anlegen | Nie mit Passwort durchs Modell. Entweder importiert der Agent einen Passwortmanager-Eintrag mit Tag `haex-mail` (`import_account`), oder `open_account_setup` öffnet den Assistenten, und der Nutzer tippt das Passwort selbst. |
| Sortieren | Verschieben in Ordner, Reihenfolge beim Auflisten und dauerhafte Regeln. |
| Senden | `create_draft` ist `change`; `send_mail` ist `destructive` und fragt je Aufruf mit Empfänger, Betreff und Text. |
| Host-Befehle | Alles in 019: SEARCH, CREATE und endgültiges Löschen kommen als neue Host-Befehle. |
| Regel-Aktionen | Verschieben, als gelesen markieren, markieren. Keine Weiterleitung (würde Prompt-Injection zu dauerhaftem Datenabfluss machen). |
| Regel-Bedingungen | Absender, Empfänger, Betreff enthält Text, hat Anhang; mit UND verknüpft. Kein ODER, keine Regex. |

## Werkzeugsatz (15, Manifest-Limit 32)

| Wirkung | Werkzeug | Zweck |
| --- | --- | --- |
| read | `list_accounts` | Konten mit Adresse und Anzeigename |
| read | `list_mailboxes` | Ordner mit Rolle, ungelesen/gesamt |
| read | `find_messages` | Filter (Konto, Ordner/Rolle, ungelesen, markiert, Absender, Empfänger, Zeitraum, Anhang), Freitext, Sortierung, `limit`/`offset`; liefert Umschläge ohne Text |
| read | `get_message` | Text und Anhangsliste; setzt nicht automatisch auf gelesen |
| read | `list_rules` | vorhandene Regeln |
| change | `mark_messages` | gelesen/ungelesen, markiert |
| change | `move_messages` | in Ordner oder Rolle |
| change | `create_mailbox` | Ordner anlegen |
| change | `create_draft` | Entwurf, auch als Antwort oder Weiterleitung |
| change | `remove_rule` | Regel entfernen |
| change | `import_account` | Konto aus Passwortmanager-Eintrag übernehmen |
| change | `open_account_setup` | Assistent öffnen, optional mit Host und Ports vorbelegt |
| destructive | `add_rule` | dauerhafte Regel anlegen; Abfrage zeigt die Regel im Klartext, auch im Modus „Auto“ (Spec 019, Clarification 2026-10-20) |
| destructive | `send_mail` | Entwurf senden |
| destructive | `delete_messages` | endgültig löschen |

Filtern, Suchen und Sortieren sind bewusst ein Werkzeug (`find_messages`): wenige klare Werkzeuge nutzen
kleine Modelle verlässlicher als viele ähnliche (vgl. 046 R16).

## Regeln

- **Modell:** Name, Konto (oder alle), `enabled`, Bedingungen (UND), Aktionen, Herkunft (Nutzer/Agent).
- **Speicherung:** neue Tabelle `rules` in der Erweiterungs-DB (Migration 0003), über die Vault zwischen
  Geräten synchronisiert wie `accounts`.
- **Ausführung:** im Watch-Handler (`stores/mail/watch.ts`, INBOX, alle 90 s) und beim normalen Abruf läuft
  jede neue INBOX-Mail durch alle passenden Regeln; bei mehreren gewinnt `move` zuletzt.
  `add_rule` hat `applyToExisting`, das die Regel einmal auf den INBOX-Bestand anwendet.
- **Grenze:** Regeln laufen nur, solange haex-mail läuft. Kein Server-Dienst (Sieve). Mails, die bei
  ausgeschaltetem holzi ankommen, werden beim nächsten Abruf einsortiert. In die Spec als Einschränkung
  aufnehmen.
- **Ziel fehlt:** Zeigt eine Regel auf einen Ordner, der verschwunden ist, wird sie übersprungen und in
  der Liste markiert, statt Mails an einen falschen Ort zu schieben.
- **Gemeinsames Modul:** `lib/rules.ts` (rein, mit Tests) für Validierung, Zusammenfassung und
  Auswertung; Store, Dialog und Werkzeuge nutzen es.

## Oberfläche in haex-mail

- Neuer Abschnitt „Regeln“ in `pages/settings.vue` unter „Konten“, nach demselben Muster wie die Konten:
  Liste, Dialog zum Anlegen und Bearbeiten, Löschbestätigung. Keine eigene Seite.
- Zeile: Name, Klartext-Zusammenfassung, Schalter an/aus, Stift, Papierkorb, Kennzeichen „vom Agenten
  angelegt“.
- Dialog: Name, Konto, Bedingungszeilen (Feld, Text, +/−), Aktionsliste (verschieben nach, als gelesen
  markieren, markieren), Kontrollkästchen „Auf vorhandene Mails in INBOX anwenden“.
- Validierung: mindestens eine Bedingung und eine Aktion.

## Lieferung

Drei Etappen, jede mit eigenem PR, in dieser Reihenfolge:

1. **vault-sdk:** `feat/agent-tools` (MCP-Baustein) mergen und releasen.
2. **Host + SDK:** holzi bekommt `extension_mail_search`, `extension_mail_create_mailbox`,
   `extension_mail_delete_messages` (je mit Erlaubnisliste in `bridge/allowlist.rs`, Berechtigung
   `mail/fetch`, Tests in `*_tests.rs` gegen `test_server.rs`); das SDK bekommt die Methoden und
   Konstanten in `commands/mail.ts`. Zweites SDK-Release.
3. **haex-mail:** SDK anheben; Manifest-Block `tools`; Migration 0003 und `lib/rules.ts`; Werkzeug-
   Handler über `sdk.tools.register`; Regel-Oberfläche; Manifest neu signieren.

Spec 019 in holzi (speckit) deckt Etappe 2 und die Verträge für 3 ab; Etappe 1 und 3 laufen in den
jeweiligen Repos.

## Sicherheit

- Mailinhalte sind Daten, keine Anweisungen (018 FR-013). `get_message` und `find_messages` liefern sie
  so zurück; die Wirkungsart der Werkzeuge hängt nie vom Mailtext ab.
- Passwörter gehen nie durch Modell, Chat-Verlauf oder Zustimmungsabfrage.
- `send_mail` und `delete_messages` fragen je Aufruf; Regeln zeigen sich im Klartext.

## Tests

- `lib/rules.ts`: vitest (Bedingungen, Aktionen, Konflikte, fehlendes Ziel).
- Host-Befehle: Rust-Tests in `*_tests.rs` gegen den Test-IMAP-Server; SEARCH, CREATE und EXPUNGE dort
  nachrüsten.
- Werkzeuge: Eingabeschema und Handler je Werkzeug mit Test; Manifest gegen das `tools`-Format aus
  `specs/018-extension-agent-tools/contracts/manifest-tools.md`.
- Modellprüfung (018 FR-019): Sätze für die neuen Werkzeuge, vor allem „Wie viele ungelesene Mails habe
  ich?“ und „Schieb alle Newsletter ins Archiv“.

## Offen

- Wie stark `find_messages` bei sehr großen Ordnern paginiert (Obergrenze von `limit`).
- Ob `list_mailboxes` die Anzahl live per STATUS holt oder aus dem Cache liefert.
