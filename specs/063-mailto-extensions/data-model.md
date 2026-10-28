# Datenmodell: Extensions als Öffner für E-Mail-Links

Es gibt keine Tabelle und keine Migration. Alles lebt im Manifest der Bündel oder im Speicher des Prozesses.

## 1. Öffner-Anmeldung (Manifest)

Block `openers` in `haextension/manifest.json`, Vertrag in `contracts/manifest-openers.md`.

| Feld      | Typ        | Regel                                                                                                          |
| --------- | ---------- | -------------------------------------------------------------------------------------------------------------- |
| `openers` | `string[]` | optional; höchstens 8; je Eintrag `^[a-z][a-z0-9]*(\.[a-z][a-z0-9]*)+$`, höchstens 32 Zeichen; keine Doppelten |

Rust-Sicht (`extensions/bundle/manifest.rs`):

```text
Manifest.openers: Vec<String>          // so, wie das Manifest sie schreibt, in Reihenfolge
OpenerKind::MailCompose                // "mail.compose", die einzige bekannte Art
fn Manifest::opens(&self, kind) -> bool
```

`ExtensionSummary.openers: Vec<String>` (ts-rs, für das Fenster) enthält nur bekannte Arten, nur für
installierte und aktivierte Extensions.

## 2. Öffnen-Anfrage (`MailRequest`, Rust, `openers/mail.rs`)

Entsteht aus einem Link, wird nie gespeichert.

| Feld      | Typ        | Regel                                                             |
| --------- | ---------- | ----------------------------------------------------------------- |
| `to`      | `string[]` | höchstens 50; je Adresse nach R2 geprüft; leer erlaubt            |
| `cc`      | `string[]` | wie `to`                                                          |
| `bcc`     | `string[]` | wie `to`                                                          |
| `subject` | `string`   | höchstens 998 Zeichen; kein `\r`/`\n`; leer, wenn nicht angegeben |
| `body`    | `string`   | höchstens 16 KiB; `\n` als Zeilenende; leer, wenn nicht angegeben |

`parse(link: &str) -> Result<MailRequest, HolziError>`; Fehler: `OpenerLinkInvalid { reason }` (Form,
Kodierung, Zeilenumbruch, Adresse) und `OpenerLinkTooLarge` (Grenzen).

## 3. Wartende Anfrage (`OpenRequests`, Rust, `openers/requests.rs`)

Zustand im Speicher, ein Eintrag je Extension.

```text
OpenRequests: Mutex<HashMap<Uuid /* extension_id */, Parked>>
Parked { request: MailRequest, parked_at: Instant }
TAKE_TIMEOUT = 15 s
```

| Übergang                        | Wirkung                                                                       |
| ------------------------------- | ----------------------------------------------------------------------------- |
| `park(ext, request)`            | ersetzt eine vorige Anfrage derselben Extension; sendet `opener-request`      |
| `take(ext)`                     | gibt die Anfrage zurück und löscht sie, sonst `None`                          |
| nach `TAKE_TIMEOUT` ohne `take` | löscht sie und sendet `opener-request-expired { extensionId }` an das Fenster |
| Ende des Prozesses              | alles weg (FR-017)                                                            |

## 4. Wartender Link von außen (`mailInbox`, Pinia)

| Feld   | Typ              | Regel                                                                             |
| ------ | ---------------- | --------------------------------------------------------------------------------- |
| `link` | `string \| null` | höchstens einer; ein neuer ersetzt den wartenden; nur im Speicher; `take()` leert |

## 5. Zustände der Auswahl (nur Oberfläche)

`closed → open(link, rows) → (opener chosen | system chosen | dismissed) → closed`. „System gewählt“ gibt es nur
für Links vom Mail-Knopf (R7). Ein Dialog für einen Link von außen hat die Zeilen der Öffner und „Link
kopieren“.

## 6. Regeln

| Regel                                           | Quelle            | Stelle                                                                           |
| ----------------------------------------------- | ----------------- | -------------------------------------------------------------------------------- |
| Ohne Öffner bleibt der Mail-Knopf wie bisher    | FR-001, SC-002    | `ValueActions.vue`, `lib/mail/openers.ts`                                        |
| Nichts an eine Extension ohne Wahl              | FR-005, SC-003    | `opener_mail_open` (nur auf Befehl des Fensters)                                 |
| Felder als Text, nie roher Link oder Kopfzeilen | FR-007            | `MailRequest`, `extension_opener_take`                                           |
| Grenzen und Zeilenumbrüche                      | FR-006 bis FR-008 | `openers/mail.rs`                                                                |
| Anmeldung ist keine Berechtigung                | FR-010            | `extension_opener_take` prüft nur die Anmeldung und die Anfrage dieser Extension |
| Sichtbar nur installiert, aktiviert, angemeldet | FR-012            | `ExtensionSummary.openers`                                                       |
| Link von außen nie mit „System öffnen“          | FR-018            | `MailLinkDialog.vue`                                                             |
| Höchstens ein wartender Link von außen          | FR-017            | `mailInbox`                                                                      |
