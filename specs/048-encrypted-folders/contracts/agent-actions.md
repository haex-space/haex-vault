# Contract: Änderungen an den Dateiaktionen der Agents (048)

Grundlage sind die `files.*`-Aktionen aus [044](../../044-file-browser/contracts/agent-actions.md).
Neue Aktionen gibt es nicht; geändert wird, was Agents sehen und wann holzi fragt.

## Sichtbarkeit

- Ohne Freigabe für einen verschlüsselten Ordner erscheint er in `files.list` als
  `{ name: "verschlüsselter Ordner <n>", kind: "folder", encrypted: true, ref: "<opaque>" }`. `n`
  zählt je Antwort; `ref` ist eine Kennung, die nur in dieser Aufgabe des Agents gilt und mit der er
  den Ordner in weiteren Aktionen nennt. Name, Einträge, Anzahl und Objekte bleiben verborgen
  (FR-032).
- `path` dieses Eintrags ist der übergeordnete Ordner mit `ref` als letztem Teil; über diesen Pfad
  (und darunter) fragt holzi nach der Freigabe. Nennt der Agent den Ordner ohne Freigabe bei seinem
  echten Namen oder seinem Präfix beim Anbieter, ist er „nicht gefunden“, ohne Frage.
- `files.search` über einen Speicher lässt verschlüsselte Ordner ohne Freigabe aus.
- Kopieren oder Verschieben aus einem verschlüsselten Ordner in einen gewöhnlichen Ordner eines
  Speichers lehnt holzi für Agents ab (die Rückfrage aus FR-019 kann nur der Nutzer beantworten).
- Mit Freigabe sieht der Agent den Ordner mit seinem Namen und darin alles wie in einem gewöhnlichen
  Ordner.

## Frage an den Nutzer

Jede Aktion, die in einen verschlüsselten Ordner ohne passende Freigabe führt, löst die Frage aus
(Brücke aus 044 T076). Inhalt der Frage:

| Feld       | Inhalt                                                            |
| ---------- | ----------------------------------------------------------------- |
| Agent      | eingebauter Agent oder Name des externen Agents                   |
| Ordner     | Speicher und entschlüsselter Name                                 |
| Modell     | Name des Modells und „auf diesem Gerät“ oder „in der Cloud“       |
| Antworten  | Lesen erlauben, Lesen und Schreiben erlauben, Ablehnen            |
| Haken      | „Erlaubnis merken“ (ab Werk aus)                                  |
| Reichweite | nur mit Haken und Cloud-Modell sichtbar: „auch für Cloud-Modelle“ |

- Lokal: `ProviderKind::Local`. Cloud: `ApiKey`, `CliDelegate`, externe Agents.
- Ohne Haken gilt die Antwort bis zum Ende des Turns (eingebauter Agent) oder der MCP-Sitzung (externe
  Agents), höchstens bis zum Sperren der Vault.
- Eine gespeicherte Freigabe „nur lokal“ gilt nicht, wenn der Turn ein Cloud-Modell nutzt; dann fragt
  holzi neu.
- Antwortet niemand in 60 Sekunden oder ist kein Fenster offen: abgelehnt für diesen Aufruf, nichts
  gespeichert (wie 044).

## Ergebnisse

Fehler an den Agent: `notGranted` (abgelehnt), `readOnly` (Schreiben mit „Lesen“), `damaged`. Texte
an den Agent enthalten Namen aus dem Ordner nur, wenn die Freigabe besteht.
