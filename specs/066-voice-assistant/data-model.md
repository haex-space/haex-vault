# Data Model: Sprachassistent

**Spec**: [spec.md](./spec.md) | **Research**: [research.md](./research.md)

## Gespeichert

### Geräteeinstellungen (`storage::preferences`, `PrefScope::Device`, ADR-0001)

Alle Schlüssel gelten nur für das Gerät, dessen Zeile sie sind (FR-002, FR-038). Gespeichert wird
bei Auswahl.

| Schlüssel                                            | Typ                                   | Standard                   | Spec           |
| ---------------------------------------------------- | ------------------------------------- | -------------------------- | -------------- |
| `assistant.enabled`                                  | bool                                  | `false`                    | FR-001, FR-002 |
| `assistant.muted`                                    | bool                                  | `false`                    | US9            |
| `assistant.wakeWord`                                 | `"default"` \| `"custom"`             | `"default"`                | FR-007, FR-008 |
| `assistant.wakeWord.custom`                          | JSON `CustomWakeWord` (unten)         | –                          | FR-008         |
| `assistant.sensitivity`                              | `"low"` \| `"medium"` \| `"high"`     | `"medium"`                 | FR-009         |
| `assistant.pauseMs`                                  | int 500–2000                          | `800`                      | FR-012         |
| `assistant.followUpSecs`                             | int 3–30                              | `8`                        | FR-013         |
| `assistant.continueMins`                             | int 0–60                              | `5`                        | FR-015         |
| `assistant.modelId`                                  | Modell-Id \| null                     | null = Standard-Chatmodell | FR-020         |
| `assistant.voice.<sprache>` (de, en, fr, es, it, pt) | `VoiceRef` (unten)                    | Standardstimme der Sprache | FR-029, FR-053 |
| `assistant.speechRate`                               | float 0.75–1.5                        | `1.0`                      | FR-030         |
| `assistant.ttsProvider`                              | Anbieter-Id \| null                   | null = lokal               | FR-031         |
| `assistant.tones.enabled`                            | bool                                  | `true`                     | FR-045         |
| `assistant.tones.volume`                             | int 0–100                             | `60`                       | FR-045         |
| `assistant.headphones`                               | `"auto"` \| `"yes"` \| `"no"`         | `"auto"`                   | FR-016, R9     |
| `assistant.bubble.position`                          | JSON `{x, y}` (Anteil 0–1 der Fläche) | rechts unten               | FR-041         |
| `assistant.noticeShown`                              | bool                                  | `false`                    | FR-037         |

`VoiceRef` (Text, unabhängig von der Technik, FR-029): `builtin:<id>` (mitgeliefert),
`clone:<id>` (`haex_tts_voices`), `model:<id>` oder `model:<id>#<sprecher>` (importiert). Zeigt er
auf etwas, das es nicht mehr gibt, gilt die Standardstimme der Sprache.

`CustomWakeWord`:

```text
{
  label: string            # vom Nutzer, z. B. „Hallo Eichhörnchen“, nur zur Anzeige
  method: "templates"      # Ergebnis des Spikes in PR I (R5); weitere Werte möglich
  templates: [base64]      # 4 Einbettungsfolgen (f32, little endian), keine Rohaufnahmen
  threshold: float         # aus den Aufnahmen kalibriert
  createdAt: int           # Unix-ms
}
```

Validierung: `templates` genau 4, jede höchstens 2 s Einbettung; `threshold` in (0, 1). Fehlt der
Eintrag oder ist er ungültig, gilt `"default"` und die Einstellungen zeigen einen Hinweis.

### Chat-Unterhaltung (Spec 006)

Migration mit der nächsten freien Nummer (bei Planung nach `0046_sync_rules_decrypt`):

```sql
ALTER TABLE chat_threads ADD COLUMN origin TEXT;  -- NULL = getippt, 'voice' = Sprachgespräch
```

CRDT-Trigger kennen die neue Spalte; `HOLZI_TRIGGER_VERSION` 29 → 30. `chat_messages` bleibt der
Verlauf, den das Modell sieht (Fragen und Antworten); das Werkzeug-Ergebnis eines Vorlesens ist dort
`{"spoken": true}`.

### Vollständige Mitschrift (FR-047, FR-048)

Neue CRDT-Tabelle in derselben Migration, synchronisiert wie der Chat (Vault-Daten):

```sql
CREATE TABLE voice_transcript_entries (
  id TEXT PRIMARY KEY NOT NULL,
  thread_id TEXT NOT NULL REFERENCES chat_threads(id) ON DELETE CASCADE,
  seq INTEGER NOT NULL,            -- Reihenfolge im Gespräch, lückenlos ab 1
  spoken_at INTEGER NOT NULL,      -- Unix-ms, Beginn der Äußerung bzw. Ausgabe
  speaker TEXT NOT NULL,           -- 'user' | 'haex'; 067 ergänzt Teilnehmer-Ids
  kind TEXT NOT NULL,              -- siehe unten
  text TEXT NOT NULL,              -- vollständiger Text; nie ein Geheimnis
  spoken_until INTEGER,            -- Zeichen-Offset, bis zu dem gesprochen wurde (unterbrochen)
  outcome TEXT,                    -- Freigaben: 'approved'|'declined'|'open'|'cancelled'
  ref_id TEXT,                     -- requestId der Freigabe/Rückfrage oder chat_messages.id
  created_at INTEGER NOT NULL
);
CREATE INDEX voice_transcript_entries_thread ON voice_transcript_entries (thread_id, seq);
```

`kind`: `utterance`, `answer`, `announcement`, `progress`, `report`, `approvalQuestion`,
`approvalAnswer`, `choiceQuestion`, `choiceAnswer`, `stop`, `closing`, `notUnderstood`,
`secretNote`. Ein Vorlese-Hinweis ist `kind = 'secretNote'` mit „Passwort von ‚<Titel>‘ vorgelesen“
(Feldname je nach Feld). Ein `outcome = 'open'` wird beim Beantworten auf das Ergebnis aktualisiert.
Gesucht wird in Rust mit der gemeinsamen Faltung (`passwords::ids::fold_for_search`) und Teilstring-Vergleich; einen Volltextindex gibt es im Projekt nicht (006 schließt ihn aus).

### Eigene Stimmen (eigene Migration in PR K, nächste freie Nummer, Trigger-Version +1)

```sql
CREATE TABLE haex_tts_voices (            -- CRDT, synchronisiert (FR-050)
  id TEXT PRIMARY KEY NOT NULL,
  name TEXT NOT NULL,
  language TEXT NOT NULL,                 -- 'de'|'en'|'fr'|'es'|'it'|'pt'
  engine TEXT NOT NULL,                   -- 'pocket'
  reference_hash TEXT NOT NULL,           -- → haex_tts_voices_binaries.hash
  sample_rate INTEGER NOT NULL,
  consent_confirmed_at INTEGER NOT NULL,  -- Unix-ms der bestätigten Einwilligung
  created_at INTEGER NOT NULL
);

CREATE TABLE haex_tts_voices_binaries (   -- Muster haex_passwords_binaries / haex_contacts_binaries
  hash TEXT PRIMARY KEY NOT NULL,         -- SHA-256 der Aufnahme
  data BLOB NOT NULL,                     -- WAV 16 bit Mono, 10–30 s Sprache, < 25 MiB
  orphaned_at TEXT
);

CREATE TABLE haex_tts_models_no_sync (    -- nur dieses Gerät (FR-051)
  id TEXT PRIMARY KEY NOT NULL,
  name TEXT NOT NULL,
  engine TEXT NOT NULL,                   -- 'pocket'|'kokoro'|'supertonic'|'matcha'|'kitten'|'vits'
  languages TEXT NOT NULL,                -- JSON-Array
  speakers TEXT NOT NULL,                 -- JSON-Array [{id, label}], leer = ein Sprecher
  license_note TEXT NOT NULL,
  source TEXT NOT NULL,                   -- JSON {kind:'folder'|'hf', path|repo, revision?}
  model_id TEXT NOT NULL,                 -- → device_downloaded_models_no_sync.id (Pfad, Größe, SHA-256)
  rights_confirmed_at INTEGER NOT NULL,
  imported_at INTEGER NOT NULL
);
```

### Kataloge (im Binary, `include_str!`)

- `src-tauri/src/stt/stt_catalog.json` (vorhanden, 008/010): neues Feld `engine`
  (`"candle-whisper"` für die vorhandenen Einträge, `"sherpa-parakeet"`) und `languages`
  (Parakeet: die 25 Sprachen des Modells). Parakeet als Eintrag `parakeet-tdt-0.6b-v3` (int8,
  CC-BY-4.0, Namensnennung). Die Whisper-Einträge bleiben unverändert (R7).
- `src-tauri/src/tts/tts_catalog.json`: Sprachmodelle, Format wie `stt_catalog.json` plus
  `language` (`"de"`, `"en"`, `"fr"`, `"es"`, `"it"`, `"pt"`), `engine` (`"pocket"` …), `variant`
  (`"standard"` oder `"24l"`), `sample_rate`. Gepinnte Revision, Größe, Lizenz, Namensnennung.
- `src-tauri/src/tts/voices/voices.json` mit den Referenzaufnahmen daneben (`include_bytes!`): je
  Sprache 2–3 mitgelieferte Stimmen mit `id`, `name`, `language`, `default`, Quelle, Lizenz,
  Namensnennung (R8).
- `src-tauri/src/assistant/assets.json`: feste Modelle ohne Wahl: Pausenerkennung (Silero VAD) und
  Standard-Wake-Word (`haex-space/haex-wakeword`), je mit Revision, Größe, Lizenz.

Dateien liegen wie die STT-Modelle unter `<AppLocalData>/models/<id>/`.

## Nur im Speicher

### Assistent-Zustand (`assistant::AssistantState`, ein Exemplar pro Prozess)

| Feld            | Inhalt                                                             |
| --------------- | ------------------------------------------------------------------ |
| `mode`          | `Off` \| `On { muted, paused }`                                    |
| `conversation`  | `Option<Conversation>`                                             |
| `last_ended`    | `Option<(thread_id, Instant)>` für die Fortsetzungszeit            |
| `spoken_secret` | `Option<Zeroizing<String>>`, nur bis Ende der Nachhörzeit (FR-026) |
| `echo_mode`     | `Full` \| `KeywordOnly` je Plattform und Ausgabeweg (FR-016)       |

`Conversation`: `thread_id`, `phase` (unten), `running_turn` (ja/nein), `open_questions`
(Freigabe-`requestId`s und Auswahl-Ids in Reihenfolge), `spoke_delegate_notice`,
`spoke_tts_fallback_notice`.

### Phasen (Anzeige-Zustand, Ereignis `assistant-state`)

```text
Off ──ein──▶ Ready ◀──────────────────────────────── Nachhörzeit abgelaufen,
               │                                       kein Turn, keine offene Frage
   Wake Word / „Sprechen“ / Kürzel
               ▼
           Listening ──Pause erkannt──▶ Thinking ──erste Wörter──▶ Speaking
               ▲                          │                           │
               │                    > N s ohne Antwort               fertig
               │                          ▼                           ▼
               │                       Working ──Antwort──▶ Speaking ─▶ FollowUp (Nachhörzeit)
               │                          │                                │
               │                Freigabe / Rückfrage                 Äußerung
               │                          ▼                                │
               └──────────────── AwaitingAnswer ◀──────────────────────────┘
Muted, Paused: Querzustände; aus jeder Phase erreichbar, zurück in die vorige.
Aktiv beenden: aus jeder Phase → Turn abbrechen (FR-039) → Ready.
Vault schließt: aus jeder Phase → Off (FR-004).
```

Regeln:

- Nachhörzeit läuft nur, wenn weder ein Turn läuft noch eine offene Frage besteht (FR-014).
- Dazwischenreden in `Speaking` → Ausgabe stoppen → `Listening` (FR-016; in `KeywordOnly` nur nach
  Wake Word oder Stopp-Wort).
- Stopp-Wort in `Thinking`/`Working`/`Speaking` → Turn abbrechen, Ton „Abgebrochen“ → `FollowUp`
  (FR-017).
- Turn fertig, während keine Runde gesprochen wird (Hintergrund) → Ton „Aufmerksamkeit“ → `Speaking`
  (FR-040).

### Einsprech-Sitzung (US7)

`Enrollment { label, samples: Vec<Embedding>, step }` — Rohaufnahmen nur bis zur Berechnung der
Einbettung, danach überschrieben; Abbruch verwirft alles (FR-008).
