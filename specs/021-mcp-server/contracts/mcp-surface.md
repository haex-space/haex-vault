# Contract: MCP-Oberfläche für externe Agenten

**Feature**: [../spec.md](../spec.md) | **Research**: R1, R2, R3, R6, R11

Was ein externer Agent über MCP sieht. Gilt für beide Wege (Programm-Weg und HTTP).

## Server

- `serverInfo.name`: `holzi`, `serverInfo.version`: Version der App.
- Protokollversion: die neueste, die `rmcp` 3.5 und der Client gemeinsam haben.
- `capabilities`: nur `tools` mit `listChanged: true`. Keine Ressourcen, keine Prompts (Spec
  „Nicht im Umfang“).
- `instructions`:
  - registriert: kurzer Text, dass Werkzeuge holzi auf diesem Gerät bedienen, dass Ziele (Tab,
    Fenster) ausdrücklich anzugeben sind (020 FR-030) und dass `needs_choice` eine Auswahl verlangt
    (046);
  - wartet: „Warte auf Registrierung in holzi. Bestätige den Dialog im holzi-Fenster.“

## Werkzeuge

| Quelle                       | Name                                                                                                   | Beschreibung, Schema                 |
| ---------------------------- | ------------------------------------------------------------------------------------------------------ | ------------------------------------ |
| Aktion (020/023/044/051/056) | Werkzeugname wie für den eingebauten Agenten (`check-agent-actions.ts` prüft Muster und Eindeutigkeit) | `description` und `input` der Aktion |
| `passwords.secret.get` (neu) | wie oben                                                                                               | Eingabe `entryId`, `field`           |
| Erweiterung (018)            | `x_<slug>_<tool>`                                                                                      | aus dem Manifest der Erweiterung     |
| Befehle (003)                | `run_command`                                                                                          | wie im Chat                          |

Jedes Werkzeug trägt die Annotationen `readOnlyHint` (`read`) und `destructiveHint`
(`destructive`). Die Liste enthält genau die Werkzeuge nach research R3. `find_actions` und
`ask_user` aus dem Chat gibt es für externe Agenten nicht: Sie haben die ganze Liste und fragen
ihren Nutzer selbst.

## Ergebnisse

Erfolg: `content` mit einem Text-Teil (JSON des Ergebnisses) und `structuredContent` mit demselben
Objekt.

Fehler: `isError: true`, `structuredContent` = `{ "code": <code>, "message": <text>, ... }`.

| `code`           | Wann                                                                         | Zusatzfelder       |
| ---------------- | ---------------------------------------------------------------------------- | ------------------ |
| `not_allowed`    | Rechte decken den Aufruf nicht (FR-014); verrät nie, ob ein Ziel existiert   | –                  |
| `plan_mode`      | Freigabestufe Plan, Aufruf nicht lesend                                      | –                  |
| `user_declined`  | Rückfrage abgelehnt                                                          | –                  |
| `no_answer`      | Rückfrage nach 5 Minuten ohne Antwort                                        | –                  |
| `needs_choice`   | Aktion verlangt Auswahl (046)                                                | `field`, `options` |
| `invalid_input`  | Eingabe passt nicht zum Schema                                               | `field`            |
| `unavailable`    | Werkzeug weg (Erweiterung entfernt), Vault schließt, Webview antwortet nicht | –                  |
| `too_many_calls` | mehr als 4 gleichzeitige Aufrufe, 30 s gewartet                              | –                  |
| `failed`         | Handler scheiterte; Text ist `ACTION_FAILED_MESSAGE`, nie interne Details    | –                  |

Ein Aufruf eines Werkzeugs, das nicht in der Liste steht, ergibt `not_allowed`, nie
„unbekanntes Werkzeug“, damit die Liste anderer Agenten nicht erraten werden kann.

## Benachrichtigungen

- `notifications/tools/list_changed` nach: Registrierung, Änderung der Rechte, Installieren,
  Entfernen oder Freigeben einer Erweiterung, Änderung der Aktionsliste durch das Frontend.
- Bei Widerruf, Ablauf, Schließen der Vault oder Ausschalten endet die Verbindung (Programm-Weg:
  IPC wird geschlossen, das Hilfsprogramm endet; HTTP: Sitzung beendet, Tokens ungültig).

## Abwehr (US8)

- Programm-Weg: nur Gegenstellen desselben Betriebssystem-Benutzers (Socket-Rechte, UID-Prüfung,
  Pipe-DACL).
- HTTP-Weg: siehe [http-auth.md](./http-auth.md).
- Unbekannte Agenten: höchstens ein Registrierungsdialog je Slug und Gegenstelle, höchstens 3
  gleichzeitig; darüber hinaus Begrüßungsantwort `rejected` mit Grund `registration_busy`.
