# Data Model: Meeting-Mitschrift

**Spec**: [spec.md](./spec.md) | **Research**: [research.md](./research.md)

Alle Tabellen sind CRDT-Tabellen (Vault-Daten, synchronisiert, FR-027), ohne UNIQUE-Bedingungen;
Ids nach dem Muster abgeleiteter Ids (`contacts/ids.rs`). Eine Migration mit der nächsten freien Nummer;
`HOLZI_TRIGGER_VERSION` +1.

## Wiederverwendet aus 066

- `chat_threads.origin = 'meeting'`: das Meeting als Unterhaltung; ihre Nachrichten sind der
  Meeting-Chat.
- `voice_transcript_entries`: die Mitschrift. Für Meetings gilt:
  - `speaker` = Id einer Zeile in `haex_meeting_participants` oder `'haex'`.
  - `kind` zusätzlich `'speech'` (Äußerung im Meeting), `'pause'`, `'gap'` (Lücke durch Ausfall).
  - Neue Spalten (gleiche Migration, nullable, betreffen 066 nicht):

```sql
ALTER TABLE voice_transcript_entries ADD COLUMN ended_at INTEGER;          -- Unix-ms
ALTER TABLE voice_transcript_entries ADD COLUMN channel TEXT;              -- 'mic' | 'device'
ALTER TABLE voice_transcript_entries ADD COLUMN overlapping INTEGER;       -- 0/1
ALTER TABLE voice_transcript_entries ADD COLUMN assignment TEXT;           -- 'live' | 'final' | 'user'
ALTER TABLE voice_transcript_entries ADD COLUMN original_text TEXT;        -- gesetzt, wenn bearbeitet
ALTER TABLE voice_transcript_entries ADD COLUMN embedding BLOB;            -- f32 LE, nur bis Meeting-Ende
```

`embedding` wird nach der endgültigen Zuordnung geleert (keine Stimmmerkmale fremder Personen über das
Meeting hinaus, FR-013). Einträge mit `assignment = 'user'` ändert der End-Durchgang nicht (FR-011).

## Neue Tabellen

```sql
CREATE TABLE haex_meetings (
  id TEXT PRIMARY KEY NOT NULL,
  thread_id TEXT NOT NULL REFERENCES chat_threads(id) ON DELETE CASCADE,
  title TEXT NOT NULL,                 -- Standard: Datum und Uhrzeit
  started_at INTEGER NOT NULL,
  ended_at INTEGER,
  vault_device_uuid TEXT NOT NULL,     -- aufnehmendes Gerät
  source TEXT NOT NULL,                -- 'mic' | 'mic+device'
  consent_at INTEGER NOT NULL,         -- Bestätigung der Einwilligung (FR-002)
  announced INTEGER NOT NULL,          -- 0/1 Ansage (FR-003)
  keep_audio INTEGER NOT NULL,         -- 0/1 (FR-014)
  answers TEXT NOT NULL,               -- 'speak' | 'show' (FR-018)
  state TEXT NOT NULL,                 -- 'recording'|'paused'|'ended'|'assigning'
  created_at INTEGER NOT NULL
);

CREATE TABLE haex_meeting_participants (
  id TEXT PRIMARY KEY NOT NULL,
  meeting_id TEXT NOT NULL REFERENCES haex_meetings(id) ON DELETE CASCADE,
  label TEXT NOT NULL,                 -- „Sprecher N“ oder Name
  number INTEGER NOT NULL,             -- N für „Sprecher N“
  contact_id TEXT,                     -- ohne FK; fehlender Kontakt = unverknüpft
  is_me INTEGER NOT NULL,              -- 0/1
  merged_into TEXT,                    -- Id des Teilnehmers nach Zusammenführen
  channel TEXT NOT NULL                -- 'mic' | 'device'
);

CREATE TABLE haex_meeting_audio_chunks (
  id TEXT PRIMARY KEY NOT NULL,
  meeting_id TEXT NOT NULL REFERENCES haex_meetings(id) ON DELETE CASCADE,
  channel TEXT NOT NULL,               -- 'mic' | 'device'
  seq INTEGER NOT NULL,
  start_ms INTEGER NOT NULL,           -- Zeit des ersten Rahmens relativ zum Meeting-Beginn
  data BLOB NOT NULL                   -- Ogg-Opus-Seiten, höchstens 1 MiB
);

CREATE TABLE haex_meeting_evaluations (
  id TEXT PRIMARY KEY NOT NULL,
  meeting_id TEXT NOT NULL REFERENCES haex_meetings(id) ON DELETE CASCADE,
  version INTEGER NOT NULL,            -- 1, 2, … ; höchste = aktuell (FR-019)
  created_at INTEGER NOT NULL,
  model TEXT NOT NULL,
  content TEXT NOT NULL                -- JSON `Evaluation` (unten)
);

CREATE TABLE haex_voice_profile (
  id TEXT PRIMARY KEY NOT NULL,        -- fest abgeleitet: höchstens eins pro Vault
  embedding BLOB NOT NULL,             -- f32 LE, normiert
  threshold REAL NOT NULL,
  created_at INTEGER NOT NULL
);
```

`Evaluation` (JSON):

```text
{
  summary: string,
  decisions: [{ text, refs: [entryId] }],
  tasks: [{ text, owner?: participantId | string, due?: string, refs: [entryId] }]
}
```

Validierung: `refs` zeigen auf Einträge dieses Meetings; unbekannte Ids werden beim Speichern
verworfen.

## Zustände eines Meetings

```text
recording ⇄ paused ──beenden──▶ assigning ──fertig──▶ ended
    │                                  ▲
    └── Vault schließt ──▶ ended (ohne End-Durchgang; beim nächsten Öffnen „Zuordnung verbessern“ anbieten)
```

## Nur im Speicher

- `MeetingSession`: laufendes Meeting, Mittelpunkte der Sprecher je Kanal, Warteschlange der
  Erkennung, Rückstand in Sekunden, Opus-Encoder je Kanal.
- Die Rohaufnahmen der Stimmprofil-Einrichtung, bis die Einbettung berechnet ist.

## Löschen

- Meeting löschen → `chat_threads`-Zeile löschen; alles hängt per `ON DELETE CASCADE` daran
  (Mitschrift über `thread_id`, Meeting, Teilnehmer, Audio, Auswertungen) (FR-025).
- Audio löschen → nur `haex_meeting_audio_chunks` des Meetings; `keep_audio` bleibt als Herkunft, die
  Oberfläche zeigt „Audio gelöscht“.
