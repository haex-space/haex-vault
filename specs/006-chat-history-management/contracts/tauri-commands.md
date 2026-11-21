# Contracts: Chat-Historie verwalten

Alle neuen Commands liefern `Result<T, HaexError>`. Rust-Felder werden wie
bisher als `camelCase` serialisiert. Backend-Fehler liefern strukturierte,
sprachneutrale Codes; die sichtbaren Texte kommen aus der i18n-Schicht.

## Bestehendes Kommando

### `list_threads() -> ThreadPayload[]`

Der bestehende Payload muss weiterhin mindestens `id`, `title`, `createdAt`
und `updatedAt` liefern. `createdAt` ist die alleinige Quelle für die im
Frontend berechnete Duration und wird als Unix-Epoch in Millisekunden
übergeben. Die bestehende Sortierung bleibt erhalten.

Die Berechnung folgt der bestehenden JavaScript-Zeitbasis:

```typescript
const elapsedMs = Math.max(0, Date.now() - createdAt)
```

Danach wird `elapsedMs` in volle Minuten, Stunden oder Tage umgerechnet und
abgerundet. Fehlt `createdAt` oder ist der Wert unbrauchbar, wird die Duration
als `0min` dargestellt.

## Neues Kommando: Titel ändern

### `rename_thread(args) -> ThreadPayload`

```typescript
type RenameThreadArgs = {
  threadId: string
  title: string
}
```

Vertrag:

- `threadId` muss eine vorhandene Thread-ID sein.
- `title` wird an den äußeren Leerzeichen bereinigt und muss danach 1 bis 120
  sichtbare Zeichen enthalten.
- Bei ungültigen Eingaben wird `InvalidInput` geliefert; der bestehende Titel
  bleibt erhalten.
- Bei Persistenzfehlern wird ein Fehler geliefert; die Antwort darf keinen
  teilweise gespeicherten Titel behaupten.
- Der erfolgreiche Payload enthält den gespeicherten Titel und den
  unveränderten Eröffnungszeitpunkt.

## Neues Kommando: Thread löschen

### `delete_thread(args) -> void`

```typescript
type DeleteThreadArgs = {
  threadId: string
}
```

Vertrag:

- Die Bestätigungsentscheidung wird ausschließlich im Frontend vor dem
  Command getroffen; das Backend löscht ohne zusätzliche UI-Rückfrage nur die
  konkret übergebene Thread-ID.
- Bei einem aktiven laufenden Turn oder offenen Tool-Approval muss das
  Frontend vor diesem Command den Turn abbrechen und dessen terminales Ereignis
  abwarten. Der Lösch-Command darf bei fehlgeschlagenem Abbruch nicht gesendet
  werden.
- `threadId` muss eine vorhandene Thread-ID sein. Eine bereits nicht mehr
  vorhandene ID liefert einen stabilen `NotFound`-Fehler und darf keine andere
  Thread-ID betreffen.
- Ein Erfolg entfernt den Thread und alle zugehörigen Nachrichten als eine
  zusammengehörige persistente Aktion.
- Ein Fehler darf keine teilweise sichtbare Löschung veröffentlichen.

## Frontend-Wrapper

`useChat` ergänzt schmale, explizit awaitbare Aufrufe:

```typescript
async function renameThreadAsync(
  threadId: string,
  title: string,
): Promise<Thread>

async function deleteThreadAsync(threadId: string): Promise<void>
```

Die Chat-Seite aktualisiert die lokale Thread-Liste erst nach erfolgreichem
Command-Ergebnis. Laufende Streaming-/Approval-Zustände werden nicht durch
einen stillen lokalen Optimismus gelöscht.
