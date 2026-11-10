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

Only spoken rounds carry it: `assistant::turn::voice_instruction` passes the word list row
`prompt.voice` in the conversation language (one of the six) as the round's given system prompt,
which `chat/tools/prompt.rs` puts first. Content: spoken conversation, everything written is read
aloud (the model can read aloud and never says it cannot speak, read aloud or hear); the user's
words arrive as a transcript with possible recognition errors; full sentences, no formatting,
tables, code or written-out links; short answers, except a longer text the user wants to hear,
which is read to its end without asking whether to go on (the user interrupts instead); long
results of a search or tool summarized with a pointer to the chat; on ambiguity a targeted question
with at most three options; at most one suggestion per round, on the current topic, only as a
question; never read passwords or other secrets aloud itself (`assistant.secret.speak` comes with
PR F). Der Text wird mit dem Tool-Eval aus 032
gemessen (Release-Build).

## Katalogaktionen (Spec 020)

Alle im Bereich `guardrails` (`agentCallable: false`): Mikrofon und Zuhören steuert nur der Nutzer,
weder der eingebaute noch ein externer Agent (021).

| Aktion             | Eingabe | Ergebnis    | Binding | `defaultKeys` | Spec   |
| ------------------ | ------- | ----------- | ------- | ------------- | ------ |
| `assistant.toggle` | –       | `{enabled}` | global  | –             | FR-001 |

`assistant.toggle` schaltet den Assistenten ein oder aus (Betreiber 2026-11-02). Weitere Aktionen
(Sprechen, Stumm, Gespräch zeigen oder beenden) und ein Kürzel gibt es nicht (FR-001); die Aktionen
`assistant.talk`, `assistant.mute.set`, `assistant.conversation.end` und `assistant.conversation.show`
aus T049 entfallen (T161).

Einstellungsaktionen für `assistant.*` folgen dem Helfer `setting({...})` in
`settingsActions.ts`, ebenfalls `guardrails`.

## Launcher und Dock

- Launcher: Kachel „Sprachassistent“ neben der Sperr-Kachel, Zustand an/aus, führt
  `assistant.toggle` aus.
- Dock: `DockControlId` `"assistant"`. Bis zum Wake Word (PR D) wird er nie in den synchronisierten
  `dock.items` gespeichert: Dock und Launcher zeigen den Schalter, solange die Vorschau
  (`assistant.preview`) auf diesem Gerät an ist oder der Assistent läuft, damit er sich immer
  ausschalten lässt. Sein Kontextmenü ist das Platzierungsmenü.
