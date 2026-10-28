# Contract: Agent-Werkzeug und Katalogaktionen

## Werkzeug `assistant.secret.speak` (nur Sprachgespräche, ADR-0020)

Natives Werkzeug (ADR-0011), nur im Werkzeugangebot einer Runde, deren Unterhaltung
`origin = 'voice'` hat und die über den Assistenten läuft.

```text
input:  { itemId: string,                       # aus passwords.items.search
          field: "password"|"totp"|"pin"|"custom:<name>" }
result: { spoken: true }
      | { spoken: false, reason: "declined"|"noAnswer"|"notFound"|"noLocalVoice"|"noField" }
```

Ablauf:

1. haex sucht Titel und Feld der Zeile (Kopfdaten, kein Geheimnis).
2. Ton „Frage“, Ansage „Ich lese <Feld> von ‚<Titel>‘ laut vor. Soll ich?“ — immer, unabhängig von
   der Freigabestufe (FR-025).
3. Nur eindeutige Zustimmung → `SpokenConsent` (nur in `assistant::approval` konstruierbar),
   gebunden an `itemId`, `field`, Unterhaltung und Ablaufzeit der aktuellen Nachhörzeit →
   `PasswordsService::reveal_for_speech(item, field, consent)` → Ton „Geheimnis“ → buchstabierte
   Ausgabe über eine lokale Stimme (FR-026, FR-027).
4. Ergebnis an das Modell nur `{spoken: true}`; Vorlese-Hinweis in die Mitschrift.

`noLocalVoice`: Das Geheimnis erscheint stattdessen in der kompakten Mitschrift als maskiertes Feld
mit „Anzeigen“ (Aufrufer Nutzer, wie im Passwortmanager), haex sagt das. Wiederholtes Vorlesen ist
nur für dasselbe `itemId` und `field` innerhalb derselben Nachhörzeit erlaubt; ein anderes Ziel,
ein anderes Feld oder ein abgelaufenes Fenster erfordert eine neue mündliche Bestätigung. Ein
zwischengespeicherter Wert wird beim Ende des Fensters verworfen.

Invarianten (Vertragstest `assistant_secret_contract.rs`): Das Geheimnis erscheint in keinem
`emit`-Ereignis, keinem Log-Eintrag, keiner Modell-Eingabe, keinem `chat_messages`-Inhalt und keiner
Anfrage an einen externen Dienst.

## Anweisung an das Modell (FR-021)

Nur in Sprachgesprächen hängt `chat/tools/prompt.rs` einen Abschnitt an (de oder en nach Sprache des
Gesprächs): gesprochenes Gespräch; kurze Sätze, keine Formatierung, keine Tabellen, keine Links
ausschreiben; bei Mehrdeutigkeit `ask_user` mit höchstens drei Möglichkeiten; höchstens ein Vorschlag
pro Runde zum aktuellen Thema, nur als Frage; bei langen Ergebnissen eine Zusammenfassung und Verweis
auf den Chat; Passwörter nur über `assistant.secret.speak`. Der Text wird mit dem Tool-Eval aus 032
gemessen (Release-Build).

## Katalogaktionen (Spec 020)

Alle im Bereich `guardrails` (`agentCallable: false`): Mikrofon und Zuhören steuert nur der Nutzer,
weder der eingebaute noch ein externer Agent (021).

| Aktion                        | Eingabe     | Binding | `defaultKeys`                                | Spec   |
| ----------------------------- | ----------- | ------- | -------------------------------------------- | ------ |
| `assistant.mode.set`          | `{enabled}` | global  | –                                            | FR-001 |
| `assistant.mute.set`          | `{muted}`   | global  | –                                            | US9    |
| `assistant.talk`              | –           | global  | `Ctrl+Shift+Space` (macOS `Cmd+Shift+Space`) | FR-011 |
| `assistant.conversation.end`  | –           | global  | –                                            | FR-014 |
| `assistant.conversation.show` | –           | global  | –                                            | FR-042 |

Einstellungsaktionen für `assistant.*` folgen dem Helfer `setting({...})` in
`settingsActions.ts`, ebenfalls `guardrails`.

## Launcher und Dock

- Launcher: Kachel „Sprachassistent“ neben der Sperr-Kachel, Zustand an/aus, führt
  `assistant.mode.set` aus.
- Dock: neuer `DockControlId` `"assistant"`; anheftbar wie die anderen Steuer-Einträge (045).
