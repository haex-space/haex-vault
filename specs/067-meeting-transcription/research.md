# Research: Meeting-Mitschrift mit Sprecherzuordnung

**Spec**: [spec.md](./spec.md) | **Plan**: [plan.md](./plan.md) | **Stand**: 2026-10-28

Baut auf den Entscheidungen aus Spec 066 (Audio-Strom, sherpa-onnx 2.x, Pausenerkennung,
Echo-Unterdrückung) auf. Hier stehen nur die Entscheidungen, die 067 dazu braucht.

## Ausgangslage im Code

- **Verschlüsselte Binärdaten**: Der ganze Vault ist SQLCipher; ein BLOB in einer CRDT-Tabelle ist
  verschlüsselt und kommt auf alle eigenen Geräte. Muster: `passwords/binaries.rs` (Tabelle
  `haex_passwords_binaries`, Grenze 25 MiB, `orphaned_at` mit 7 Tagen Frist). Sync teilt große Zellen
  in Seiten (`sync/change.rs`), ein Schreibvorgang darf höchstens 100 MiB groß sein. Getestet ist bisher
  nur etwa 500 KB (`sync/large_preference_tests.rs`). Einen Datei-Sync zwischen eigenen Geräten (025)
  gibt es nicht; verschlüsselte Ordner (048) synchronisieren nur über Sync-Regeln (061).
- **Abspielen**: `files/media/server.rs` liefert über `StreamingSource` mit Range-Anfragen; Vorbild
  für einen Vault-BLOB ist `passwords_attachment_open` (`BytesSource`, ganz im Speicher).
- **Kontakte**: `haex_contacts` (051), fester Kontakt „Ich“ (Migration 0042). Keine allgemeine
  Rückverweis-Liste; `EntryPage.vue` hat feste Abschnitte (Vorbild `FileSharesSection.vue` über die
  Aktion `files.shares.ofContact`). Kein wiederverwendbarer Kontakt-Wähler; Bausteine
  `useContactsStore`, `filterContacts`, Muster `passwords/ReferencePicker.vue`.
- **Apps**: neue System-App über `WM_APPS` (`src/lib/wm/apps.ts`), Routen in `appRoutes.ts` mit eigenem
  Registry-Modul (Vorbild Kontakte).
- **Suche**: keine Volltextsuche (FTS5) im Projekt; 006 schließt sie aus. Kontakte und Passwörter
  suchen im Speicher mit gemeinsamer Faltung (`src/lib/search/fold.ts`,
  `passwords::ids::fold_for_search`).
- **Export**: `usePickedFile.pickSaveTargetAsync` + `files::picked::PickedFile` (auch Android).
- **Migrationen**: letzte auf `main` ist `0046_sync_rules_decrypt`; `0047` ist im Worktree `065-b`
  vergeben. Diese Spec nimmt die nächste freie Nummer bei Umsetzung.

## R1 — Ein Meeting ist eine Unterhaltung mit Mitschrift aus 066

**Entscheidung**: Ein Meeting ist eine Chat-Unterhaltung mit `chat_threads.origin = 'meeting'` (Spalte
aus 066). Ihre Mitschrift sind Zeilen in `voice_transcript_entries` (066), deren `speaker` eine
Teilnehmer-Id ist; die Nachrichten der Unterhaltung sind der Meeting-Chat (FR-020). Dazu kommen
eigene Tabellen für Meeting, Teilnehmer, Audio, Auswertung und Stimmprofil
([data-model.md](./data-model.md)).

**Begründung**: Eine Mitschrift-Tabelle für Gespräche und Meetings (eine Implementierung pro Regel);
Meeting-Chat und Assistent-Fragen im Meeting sind ohnehin Chat-Unterhaltungen.

**Alternative**: eigene Tabelle `meeting_entries` — verworfen, wäre eine zweite Mitschrift mit
denselben Feldern.

## R2 — Erkennung: Parakeet und Whisper aus 066

**Entscheidung**: Erkannt wird pro Äußerung, die die Pausenerkennung schneidet (066 R6, R7). Beide
Modelle kommen aus 066: das vorhandene candle-Whisper und NVIDIA Parakeet TDT 0.6B v3 über sherpa
(int8, CC-BY-4.0, 25 europäische Sprachen inkl. de/en, mit Satzzeichen und Wortzeiten). 067 bringt
kein eigenes Modell mit. Der Spike (T005) wiederholt den Vergleich aus 066 (`stt_compare_eval`) mit
den Meeting-Fixtures und 4 Stunden am Stück. Für Meetings gilt das Modell, das SC-005 (≤ 15 %
Wortfehler) erfüllt und 4 Stunden ohne Rückstand schafft (SC-002); erwartet wird Parakeet, weil die
Wortzeiten die Sprecherzuordnung (R3) genauer machen. Whisper bleibt wählbar, bis der Betreiber es
nach eigenen Tests entfernt (066 R7).

**Begründung**: Whisper base ist im Deutschen schwach, Whisper small auf Laptops und Telefonen
grenzwertig für 4 Stunden; Parakeet ist laut Berichten deutlich schneller als Whisper small auf der
CPU (nicht gemessen).

**Alternativen**: Canary-180m-flash (CC-BY-4.0, de/en) als Ersatz, falls Parakeet 4 Stunden auf dem
Telefon nicht schafft. Kroko (CC-BY-SA bzw. Lizenzserver) und ältere Moonshine-Modelle (nicht
kommerziell) — verworfen.

**Rückstand**: Die Erkennung läuft in einer Warteschlange; sie verwirft nie Äußerungen. Liegt sie
mehr als 30 s zurück, zeigt die Anzeige „holt auf“.

## R3 — Sprechertrennung: live schrittweise, am Ende neu

**Entscheidung**:

- **Einbettung**: WeSpeaker ResNet34-LM über den Sprecher-Extraktor von sherpa (Gewichte CC-BY-4.0,
  per Dateiname und Digest gepinnt, weil dieselben Release-Tags auch nicht kommerzielle Modelle
  tragen).
- **Live**: Pro Äußerung eine Einbettung; Vergleich per Kosinus mit den Mittelpunkten der bisherigen
  Sprecher; über der Schwelle zum nächsten, sonst neuer „Sprecher N“ (Vorgehen wie diart und
  sherpa-diarize). Höchstens 10 Sprecher, darüber „Unbekannt“.
- **Am Ende**: Neu-Clustering aller Einbettungen (agglomerativ, Sprecherzahl aus dem Live-Durchgang
  als Startwert), danach Abbildung der neuen Cluster auf die Live-Bezeichnungen per größter
  Überlappung, damit Namen erhalten bleiben. Einträge, die der Nutzer selbst zugeordnet hat, sind feste
  Vorgaben und ändern sich nicht (FR-011).
- **Überlappung**: Bei behaltenem Audio prüft der End-Durchgang Überlappung mit pyannote
  segmentation-3.0 (MIT; ungegateter Spiegel bei onnx-community); ohne Audio markiert haex nur
  Überlappungen zwischen Mikrofon und Geräteton.
- **Kanäle**: Mikrofon und Geräteton werden getrennt geclustert (FR-029); das senkt Überlappungen
  zwischen Raum und Online-Teilnehmern.

**Risiko für SC-003**: Die fertige Pipeline von sherpa (pyannote + Einbettung + einfaches Clustering)
erreichte in einem unabhängigen Test auf AMI 35,6 % DER bei unstabiler Sprecherzahl; pyannote
community-1 mit VBx-Clustering 16,3 %. SC-003 (85 % richtig) verlangt etwa das zweite Niveau. Der
Spike (T006) misst auf deutschen Testaufnahmen; reicht das einfache Clustering nicht, wird VBx-artiges
Clustering nachgebaut oder SC-003 mit dem Betreiber angepasst — nicht stillschweigend.

**Alternativen**: NVIDIA Streaming Sortformer (echtes Streaming, aber höchstens 4 Sprecher, eigene
ONNX-Runtime, nicht-OSI-Lizenz). Nemotron-3-Diarization (AMI 20,3 %, höchstens 8 Sprecher; Aufnahme in
sherpa offen) — Kandidat für den End-Durchgang, sobald in sherpa.

## R4 — Eigenes Stimmprofil

**Entscheidung**: Dieselbe Einbettung. Einrichten: sechs kurze Sätze vorlesen (zusammen etwa 30 s);
Profil = Mittel der normierten Einbettungen; Schwelle per Kosinus, im Spike kalibriert (SC-004).
Rohaufnahmen danach verworfen. Gespeichert als eine Zeile pro Vault in einer CRDT-Tabelle
(synchronisiert, FR-013). Bei getrenntem Geräteton schlägt haex Mikrofon-Sprache eines
Kopfhörer-Nutzers auch ohne Profil als „Ich“ vor (FR-029).

**Begründung**: WeSpeaker ResNet34 erreicht 0,7–0,9 % EER auf VoxCeleb; für kurze Äußerungen und
Raummikrofone ist mit einigen Prozent zu rechnen, deshalb Messung statt Annahme.

## R5 — Geräteton je Betriebssystem

**Entscheidung**: `cpal` 0.18.2 deckt alle Desktop-Ziele ab:

- **Windows**: Eingabestrom auf dem Ausgabegerät (WASAPI-Loopback); Stille selbst auffüllen, weil ohne
  Wiedergabe keine Pakete kommen; bei Gerätewechsel neu öffnen.
- **macOS ≥ 14.6**: Core-Audio-Prozess-Tap von cpal; `NSAudioCaptureUsageDescription` in Info.plist,
  Berechtigung „Systemaudio-Aufnahme“. Verweigert sie der Nutzer, liefert der Strom nur Nullen ohne
  Fehler; haex prüft das und zeigt einen Hinweis. Unter 14.6 nur Mikrofon.
- **Linux**: PipeWire- bzw. PulseAudio-Host von cpal, Monitor des Standard-Ausgangs; Ersatz die Crate
  `pipewire` mit `stream.capture.sink=true`, falls cpal den Monitor nicht anbietet (T007 prüft das).
- **Android/iOS**: nur Mikrofon. Android könnte Medienton aufnehmen, aber keine Gesprächs-Apps
  (`USAGE_VOICE_COMMUNICATION` ist ausgeschlossen); für Meetings also wertlos.

## R6 — Doppelte Sätze vermeiden

**Entscheidung**: Der Geräteton ist die Referenz der Echo-Unterdrückung (Trait `EchoCanceller` aus 066
PR G) für den Mikrofon-Kanal. Lücken in der Referenz mit Nullen füllen, verspätete Referenz-Blöcke
verwerfen, Referenz etwa 30 ms früher einspeisen, einige Sekunden Einschwingzeit. Als Netz: eine
Mikrofon-Äußerung, deren Text und Zeit einer Geräteton-Äußerung gleichen, wird verworfen (SC-010).

## R7 — Audio behalten: Opus in Stücken im Vault

**Entscheidung**: Ogg Opus, mono, 16 kbit/s (VOIP, 20-ms-Rahmen), je Kanal ein Strom: etwa 7,2 MB pro
Stunde und Kanal (SC-007: ≤ 30 MB/h). Gespeichert in 1-MiB-Stücken in einer CRDT-Tabelle
`haex_meeting_audio_chunks`, dazu ein Index Zeit → Stück für sofortiges Springen. Abspielen über eine
eigene `StreamingSource`, die Stücke nach Bedarf liest (Range-Anfragen des Medienservers). Crate
`opus` (libopus, BSD-3) plus `ogg` (BSD-3); reine Rust-Encoder (2026 erschienen) sind noch zu jung.

**Begründung**: Nur CRDT-BLOBs sind heute verschlüsselt, synchronisiert und löschbar. Stücke halten
jede Zelle klein (Sync teilt sonst eine 30-MB-Zelle in viele Seiten einer Zeile). Sync großer Daten ist
ungetestet; T008 prüft 30 MB zwischen zwei Test-Vaults.

**Alternativen**: Eine BLOB-Zeile pro Meeting — zu groß für eine Zelle. Verschlüsselter Ordner (048) —
synchronisiert nicht von selbst. Datei-Sync aus 025 — gibt es noch nicht.

## R8 — Suche ohne Volltextindex

**Entscheidung**: Eine Rust-Suche über Titel, Teilnehmer, Mitschrift-Text und Auswertung mit der
vorhandenen Faltung (`passwords::ids::fold_for_search`) und `contains`, seitenweise. Ein Volltextindex
(FTS5, gerätelokal) ist ein `ponytail:`-Kandidat, falls die Suche bei vielen Stunden Mitschrift zu
langsam wird (Ziel: 100 Stunden Mitschrift in unter 1 s auf dem Referenz-Desktop, in T009 gemessen).

## R9 — Agent im und nach dem Meeting

**Entscheidung**:

- **Live** (FR-017): Ein Assistent-Gespräch aus 066 während eines Meetings bekommt die letzten
  10 Minuten der Mitschrift im Systemtext und das Lese-Werkzeug `meetings.transcript.read {meetingId,
from?, to?, speaker?, query?}`; das Werkzeug wird nur in Meeting-Kontexten angeboten (FR-021) und ist
  rein lesend. Die Aufzeichnung läuft weiter; die eigene Ausgabe ist über die Echo-Unterdrückung aus
  dem Mikrofon-Kanal entfernt und wird als `haex`-Eintrag geführt.
- **Danach** (FR-019): Die Auswertung ist eine Aufgabe mit festem Ausgabeschema (Zusammenfassung,
  Beschlüsse, Aufgaben mit Verweisen auf Eintrags-Ids). Mit 068 ordnet der Orchestrator sie einer Stufe
  zu; ohne 068 läuft sie über den eingebauten Agenten mit dem Standardmodell. Lange Mitschriften werden
  in Abschnitte geteilt und schrittweise zusammengefasst, wenn sie das Kontextfenster überschreiten.
- **Meeting-Chat** (FR-020): die Unterhaltung des Meetings, mit `meetings.transcript.read` im Angebot.

## R10 — Meetings-App und Einstieg

**Entscheidung**: System-App `system.meetings` (Liste, Detail mit Mitschrift, Teilnehmer-Leiste,
Player, Auswertung, Meeting-Chat als Tab). Einstieg „Meeting aufzeichnen“: Launcher-Kachel und
Dock-Steuer-Eintrag wie der Sprachassistent (066); die laufende Aufnahme zeigt die schwebende Anzeige
aus 066 im Zustand „Meeting“.

## R11 — Kontakte

**Entscheidung**: `contact_id` beim Teilnehmer ohne Fremdschlüssel; fehlt der Kontakt, gilt der
Teilnehmer als unverknüpft und behält seinen Namen (Spec, Edge Cases). Auf der Kontaktseite ein neuer
Abschnitt „Meetings“ nach dem Muster `FileSharesSection.vue` über die Aktion `meetings.ofContact`.
Kontakt-Wähler als neue, wiederverwendbare Komponente `contacts/ContactPicker.vue` (Muster
`ReferencePicker.vue`), die `ShareDialog.vue` später übernehmen kann.

## R12 — Testaufnahmen

**Entscheidung**: Testmeetings werden aus verschiedenen Stimmen der Sprachausgabe (066 R8) und
freiwilligen Aufnahmen zusammengesetzt (CC0), mit bekannter Zuordnung. Für Online-Meetings ein
Testaufbau mit WAV-Wiedergabe über den Geräteton (e2e-Feature `e2e-audio` aus 066).
