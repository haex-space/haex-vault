# Vertrag: Befehle und Ereignisse des Fensters

Alle Befehle gelten nur für das eigene Fenster (`Caller::User`).

| Befehl              | Eingabe                 | Ausgabe           | Wirkung                                                                             |
| ------------------- | ----------------------- | ----------------- | ----------------------------------------------------------------------------------- |
| `opener_mail_check` | `{ link: string }`      | `MailRequestView` | liest den Link (R2) und gibt ihn zurück oder lehnt ab; speichert nichts             |
| `opener_mail_open`  | `{ extensionId, link }` | –                 | liest den Link neu, prüft die Anmeldung, parkt die Anfrage, sendet `opener-request` |

`MailRequestView` (ts-rs): `{ to: string[], cc: string[], bcc: string[], subject: string, body: string }`.
Das Fenster zeigt davon im Dialog für einen Link von außen nur Empfänger und Betreff als Beschriftung der
Wahl; nichts davon geht an eine Extension, bevor `opener_mail_open` aufgerufen wird.

Fehler (stabile Codes): `OpenerLinkInvalid`, `OpenerLinkTooLarge`, `OpenerNotAvailable` (Extension nicht
installiert, nicht aktiviert oder nicht angemeldet). Texte in `errors.openers.*` (de/en).

## Ereignisse an das Fenster

| Ereignis                 | Nutzlast          | Wann                                                                           |
| ------------------------ | ----------------- | ------------------------------------------------------------------------------ |
| `deep-link-received`     | URL als Text      | wie in Spec 060; jetzt auch `mailto:` (Desktop, ein anderer Prozess gab es ab) |
| `opener-request-expired` | `{ extensionId }` | die Extension holte die Anfrage nicht in 15 s ab                               |

## Ablauf im Fenster

**Mail-Knopf**: `useMailOpeners().openers` leer → `openUrl(link)` wie heute. Nicht leer → Menü; „Mit dem
System öffnen“ → `openUrl(link)`; Öffner → `opener_mail_open`, dann `openExtension(extensionId)`.

**Link von außen**: `useDeepLink` legt `mailto:`-Links in `mailInbox`; `[instance].vue` zeigt nach
`sessionRestored` den `MailLinkDialog`: `opener_mail_check`; Fehler → „Der Link kann nicht gelesen werden“;
sonst eine Zeile je Öffner und „Link kopieren“, ohne Öffner der Satz „Kein Öffner installiert“. Wahl eines
Öffners → `opener_mail_open` und `openExtension`; Schließen → nichts; beides leert `mailInbox`.

## Konfiguration

`tauri.conf.json`: `plugins.deep-link.desktop.schemes: ["haex-vault", "mailto"]`, `mobile` bekommt
`{ "scheme": ["mailto"] }` neben dem Eintrag für `haex-vault`. `register_for_development` registriert nur
`haex-vault`.
