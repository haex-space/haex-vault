# Research: Sprachassistent

**Spec**: [spec.md](./spec.md) | **Plan**: [plan.md](./plan.md) | **Stand**: 2026-10-28

Jede Entscheidung mit Begründung und verworfenen Alternativen. Versionen und Lizenzen sind auf dem
Stand vom 2026-10-28 und werden im Spike (PR B, T005–T009, T130) auf allen Zielen bestätigt, bevor
Code darauf baut.

## Ausgangslage im Code

- **Aufnahme**: `src-tauri/src/audio/mod.rs` nimmt mit `cpal` 0.18.2 (Feature `voice`) den ganzen
  Puffer auf und rechnet ihn beim Stoppen einmal auf 16 kHz Mono um (`stt::CanonicalPcm`). Kein
  Streaming, keine Pausenerkennung, auf Android derselbe Weg (AAudio, `minSdk` 26).
- **Erkennung**: `stt::local::LocalWhisperAdapter` (candle, CPU), warm gehalten in `VoiceState`;
  `SttAdapter::transcribe(&CanonicalPcm)` ohne Streaming. Katalog `stt/stt_catalog.json` mit
  festgelegter HF-Revision und Lizenz; Download über `models::download`.
- **Wiedergabe**: keine. Kein Ausgabestrom, keine Sprachausgabe, keine Töne.
- **Agent**: `chat::commands::send_message` speichert die Nachricht, baut die Anfrage und startet
  `turn::run_turn` mit einer `emit`-Closure (ohne `AppHandle`). Ereignisse `chat-token`,
  `tool-permission-request`, `chat-choice-request`, `chat-turn-complete` u. a. Die Zuhörer und die
  Freigabe-Oberfläche liegen nur in `ChatApp.vue`/`useChatTab`. Aktionen laufen über `ActionBridge`
  und `useAgentActions`, das im Arbeitsbereich immer läuft.
- **Freigaben**: `chat/tools/permission.rs` (`decide`, Modus `chat.permission_mode`, Vault-weit);
  Antwort über `respond_tool_permission`.
- **Geheimnisse**: `passwords/access.rs` verbietet `Caller::BuiltinAgent` jedes Geheimnis
  (ADR-0007); das einzige Agent-Werkzeug ist `passwords.items.search` (nur Kopfdaten).
- **Einstellungen**: Kategorie „KI“ mit Unterseite `ai.speech` (`SttModelSetting.vue`);
  gerätebezogene Einstellungen nach ADR-0001 über `storage::preferences` (`PrefScope::Device`).
- **WM**: Der Launcher hat mit dem Sperr-Knopf schon eine Kachel, die keine App ist; das Dock kennt
  `kind: 'control'`. Globale Ebene für Dialoge in `Desktop.vue`. Ein Eichhörnchen-Sprite gibt es
  nur als Ladeanimation (`src/assets/loader/squirrel-nut.webp`).
- **Fehlt**: Tray-Symbol (Tauri ohne `tray-icon`), Android-Vordergrund-Dienst, umbelegbare
  Tastenkürzel (`src/lib/wm/keybindings.ts`: „user rebinding is a later spec“), ein Mikrofon-Ersatz
  für e2e-Tests.
- **Schließen**: `instances/close.rs` ruft in `begin_close` `voice.cancel_for_close()` und in
  `finish_close` `voice.reset_for_close()`; `VaultGate` bricht alle darüber gestarteten Aufgaben ab.

## R1 — Gesprächssteuerung lebt in Rust

**Entscheidung**: Ein neues Modul `src-tauri/src/assistant/` steuert Zuhören, Gespräch, Runden,
Freigaben und Ausgabe. Es startet Runden über eine aus `send_message` herausgelöste Funktion
`chat::send::start_turn` (gleiche Speicherung, gleicher Anfrageaufbau, gleiche Sperre) und gibt
`run_turn` eine `emit`-Closure, die jedes Ereignis wie bisher an die Oberfläche weiterreicht und
zusätzlich an den Assistenten. Ein offenes Chatfenster mit derselben Unterhaltung aktualisiert sich
dadurch ohne Zusatzcode.

**Begründung**: Der Assistent muss bei minimiertem Fenster und auf Android im Hintergrund laufen;
Audio, Erkennung und Ausgabe sind ohnehin in Rust. `run_turn` ist schon von `AppHandle` gelöst
(Tests nutzen einen Rekorder). Eine Steuerung im Webview hinge davon ab, dass WebKitGTK oder die
Android-WebView im Hintergrund nicht gedrosselt werden.

**Alternativen**: Steuerung im Frontend über `chat.message.send` — verworfen, die Aktion ist an den
Chat-Tab gebunden und würde ihn öffnen; Freigaben hingen an `useChatTab`. Ein zweiter
Turn-Ablauf nur für Sprache — verworfen (eine Implementierung pro Regel).

**Folge**: Aktionen brauchen weiter den Runner im Webview (`useAgentActions`). Er läuft, solange der
Arbeitsbereich geladen ist, also auch minimiert. Der Spike prüft, dass WebKitGTK, WebView2, WKWebView
und die Android-WebView Ereignisse im Hintergrund weiter verarbeiten (T009).

## R2 — Audio-Engine: ein Duplex-Strom in Rust

**Entscheidung**: `audio/` bekommt einen dauerhaften Aufnahmestrom mit Ringpuffer (16 kHz Mono,
Umrechnung blockweise mit Zustand) und einen Ausgabestrom mit Mischer für Sprachausgabe und Töne.
Beide laufen über `cpal` 0.18.2. Der Mischer liefert das abgespielte Signal als Referenz an die
Echo-Unterdrückung (R9). Die bisherige Push-to-Talk-Aufnahme aus 008 nutzt danach denselben
Aufnahmestrom.

**Begründung**: Nur wer Wiedergabe und Aufnahme selbst besitzt, hat eine exakte Referenz für die
Echo-Unterdrückung. `cpal` ist schon da und läuft auf allen Zielen (Android über AAudio).

**Alternativen**: Wiedergabe im Webview (`<audio>`) — verworfen: keine Referenz für die
Echo-Unterdrückung, die CSP erlaubt kein `blob:`, im Hintergrund unsicher. `rodio` — unnötig, der
Mischer ist klein.

## R3 — Sprach-Laufzeit: sherpa-onnx 2.x

**Entscheidung**: Die Crate `sherpa-onnx` (offiziell, Apache-2.0) für Pausenerkennung (Silero VAD),
offene Schlüsselwort-Erkennung (R5), Sprachausgabe (R8) und die zweite Spracherkennung (R7), mit `default-features = false` und
`shared`. Wo ein eigenes ONNX-Modell läuft (Standard-Wake-Word, R4), nutzt `ort` mit
`load-dynamic` dieselbe `libonnxruntime`. Die Version wird exakt gepinnt.

**Begründung**: Eine Abhängigkeit für vier Aufgaben auf Linux, macOS, Windows, Android und iOS.
Seit 2.0.0 (2026-10-23) ohne espeak-ng und piper-phonemize, also frei von GPL-Code.

**Risiken**: 2.0.x ist bei Planung wenige Tage alt; Rust-Crate 2.0.1 vom 2026-10-27. Zwei statisch
gelinkte ONNX-Runtimes kollidieren (doppelte Symbole), deshalb `shared` + `load-dynamic`. Ob die
Crate vorgebaute Bibliotheken für Android und iOS mitbringt, ist offen. Der Spike (T005–T007) baut
auf allen Zielen und misst die Größe; scheitert er, fällt die Pausenerkennung auf Silero über `ort`
zurück und die Sprachausgabe wird neu bewertet.

**Alternativen**: `sherpa-rs` — veraltet zugunsten der offiziellen Crate. Alles einzeln über `ort` —
Sprachausgabe müsste selbst nachgebaut werden. sherpa-onnx 1.x — bindet espeak-ng (GPL) statisch.

## R4 — Standard-Wake-Word „Haex“ / „Hey Haex“: eigenes trainiertes Modell

**Entscheidung**: Ein eigenes Modell in der Architektur von openWakeWord (Melspektrogramm,
Einbettungsmodell, kleiner Klassifikator; Code Apache-2.0), trainiert auf beide Formen. Positive
Beispiele erzeugt eine Sprachausgabe mit freien Lizenzen (Pocket TTS, CC-BY-4.0; Piper-Stimmen nur
mit CC0- oder CC-BY-Lizenz), ergänzt um echte Aufnahmen freiwilliger Sprecher; Negative aus Common
Voice (CC0) und eigenen Geräuschaufnahmen. Das Training liegt in einem eigenen Repo
`haex-space/haex-wakeword` (Python); das fertige Modell liegt mit gepinnter Revision auf Hugging
Face und kommt über den Katalog wie die STT-Modelle. Erkannt wird nur, wenn die Pausenerkennung
Sprache meldet.

**Begründung**: Der Betreiber will das Standardwort trainieren (Klärung 2026-10-28). Ein
spezifisches Modell erreicht laut openWakeWord < 5 % verpasste Erkennungen und < 0,5
Fehlauslösungen pro Stunde (SC-003, SC-004). Gekoppelt an die Pausenerkennung bleibt die Last im
Zustand „bereit“ im einstelligen Prozentbereich eines Kerns (SC-010).

**Lizenz**: Die vortrainierten openWakeWord-Modelle (CC BY-NC-SA) und die üblichen Negativdaten
(ACAV100M, CC BY-NC-SA) sind nicht verwendbar. Die Lizenz des gemeinsamen Einbettungsmodells wird
vor dem Training geprüft (T058); ist sie nicht frei, wird es mit trainiert.

**Alternativen**: microWakeWord — gutes Modell, aber TFLite ohne Rust-Laufzeit. Porcupine — braucht
API-Schlüssel und Online-Aktivierung, eigene Wörter kostenpflichtig. Offene Erkennung von sherpa für
das Standardwort — kein deutsches Modell, schwächer als ein trainiertes Modell; bleibt Ersatz,
falls das Training SC-003/SC-004 nicht erreicht.

## R5 — Eigenes Wake Word: Muster aus den Einsprech-Aufnahmen

**Entscheidung**: Der Nutzer spricht das Wort viermal. haex berechnet aus jeder Aufnahme die
Einbettungsfolge des Standardmodells (R4) und speichert die vier Folgen als Vorlagen samt einer
Schwelle. Erkannt wird über den Abstand des laufenden Einbettungsfensters zu den Vorlagen.
Eignungsprüfung: Mindestdauer und Silbenzahl, Ähnlichkeit der vier Aufnahmen untereinander,
Abstand zu einer mitgelieferten Liste häufiger Wörter. Danach Probephase (US7). Die Rohaufnahmen
werden sofort nach dem Berechnen verworfen.

**Begründung**: Sprachunabhängig (das sherpa-Modell kann kein Deutsch), kein Training auf dem
Gerät, nutzt die schon laufende Einbettung. Passt zum „eigenen Wort für die eigene Stimme“.

**Spike**: PR I vergleicht auf einem Testsatz Vorlagen gegen die offene sherpa-Erkennung (Text aus
Whisper, Schwelle aus den Aufnahmen) und nimmt das bessere Verfahren. Das Ergebnis kommt in diese
Datei.

**Alternativen**: EfficientWord-Net — studentisches Projekt in Python, mehr Fehlauslösungen.
Training auf dem Gerät — zu schwer für Telefone.

## R6 — Ende der Äußerung

**Entscheidung**: Silero VAD (über sherpa) mit einstellbarer Pause (Standard 0,8 s, Spanne
0,5–2,0 s), Mindestlänge 250 ms und Vorlauf von 1,5 s aus dem Ringpuffer, damit „Hey Haex, wie
viele …“ in einem Atemzug ganz ankommt. Höchstdauer einer Runde 60 s wie in 008.

**Alternativen**: Smart Turn v3 (BSD-2, erkennt Satzende auch bei Denkpausen) — später, falls feste
Pausen stören; vermerkt als `ponytail:`-Kommentar. TEN VAD — Lizenz mit Wettbewerbsklausel. WebRTC
VAD — zu ungenau.

## R7 — Erkennung pro Äußerung: Whisper und Parakeet nebeneinander

**Entscheidung**: Nach dem Ende einer Äußerung geht ihr Puffer an den gewählten `SttAdapter`. Kein
Streaming-STT. Stopp-Wörter (`stt::interrupt::match_interrupt`) prüft der Assistent zuerst und bricht
ohne Modell ab (FR-017).

Neben dem vorhandenen candle-Whisper kommt NVIDIA Parakeet TDT 0.6B v3 über sherpa
(`OfflineRecognizer`, `nemo_transducer`, int8, CC-BY-4.0) als zweite Wahl in den STT-Katalog. Beide
laufen ganz lokal hinter `SttAdapter`; `VoiceState` hält den gewählten Adapter warm, Push-to-Talk
(008) und Assistent teilen ihn. Parakeet kann 25 europäische Sprachen, darunter Deutsch und Englisch;
das reicht für den Anfang (Entscheidung des Betreibers, 2026-10-28). Eine Whisper-Variante über
sherpa ist nicht geplant.

**Whisper bleibt**: Der Betreiber testet beide Modelle ausgiebig, bevor Whisper entfernt wird. Das
Entfernen von `stt::local` und `candle-transformers` ist nicht Teil von 066, sondern ein eigener PR
nach seiner Freigabe. Bis dahin entscheidet die Messung (T130) nur den Standard für neue
Installationen; bereits gewählte Modelle bleiben.

**Messung** (T130, später `tests/stt_compare_eval.rs`): Wortfehlerrate und Echtzeitfaktor beider
Modelle auf deutschen und englischen Äußerungen (Fixtures und eigene Aufnahmen, 2–15 s) auf
Referenz-Desktop und -Telefon, dazu Zeit vom Sprechende bis zum Text und Speicherbedarf. Ziel:
Text ≤ 1 s nach Sprechende auf dem Referenz-Desktop, damit SC-001 und SC-002 halten.

**Begründung**: Whisper base ist im Deutschen schwach, Whisper small auf der CPU langsam (008:
whisper-small braucht in `tauri dev` für 11 s Audio spürbar lange). Parakeet ist laut Berichten
deutlich schneller als Whisper small, liefert Satzzeichen und Wortzeiten (die 067 braucht) und
erfindet bei Stille kaum Text (nicht gemessen). sherpa ist durch R3 ohnehin im Build.

**Alternativen**: Sofort ganz auf sherpa wechseln und candle-Whisper entfernen — verworfen, bis die
Tests des Betreibers und der Spike auf allen Zielen (T005) bestanden sind; candle läuft heute überall.
Whisper zusätzlich über sherpa — vorerst nicht nötig, weil 25 Sprachen reichen. Canary-180m-flash
(CC-BY-4.0, nur de/en/es/fr) — als Ersatz, falls Parakeet auf dem Telefon zu langsam ist.

## R8 — Lokale Sprachausgabe: Kyutai Pocket TTS über sherpa

**Entscheidung**: Standardstimmen Deutsch und Englisch aus Kyutai Pocket TTS (≈100 M Parameter,
Code MIT, Gewichte CC-BY-4.0, Streaming, erste Ausgabe ≈200 ms auf 2 Kernen eines M4). Ein neuer
Katalog `tts/tts_catalog.json` im Format von `stt_catalog.json` (Revision, Größe, Lizenz, Sprache,
Eignung). Ausgabe satzweise: Der Assistent schneidet die Antwort an Satzgrenzen und gibt jeden Satz
einzeln an die Sprachausgabe (FR-022). Die Namensnennung für CC-BY steht in den Lizenzangaben der
App.

**Spike**: deutsche Qualität und Echtzeitfaktor auf Referenz-Desktop und -Telefon (T008). Reicht es
nicht, ist der Ersatz Piper `de_DE-thorsten` mit eigener freier Aussprache-Tabelle (gruut, MIT);
die Herkunft der Stimme wird vorher rechtlich geprüft.

**Alternativen**: Piper-Engine — seit 1.3.0 GPL-3.0 (espeak-ng). Kokoro-82M — kein Deutsch.
Supertonic 3 — Repo archiviert, OpenRAIL-M.

## R9 — Echo-Unterdrückung und Dazwischenreden

**Entscheidung**: AEC3 in der App hinter dem Trait `EchoCanceller`. Zuerst `sonora` (reines Rust,
BSD-3, besteht die C++-Tests von WebRTC, baut für Android und iOS), auf eine Commit-SHA gepinnt;
Ersatz `webrtc-audio-processing` 2.x (BSD-3, C++). Zusätzlich während eigener Ausgabe: höhere
Schwelle für Dazwischenreden, Mindestlänge 250 ms, Abgleich der Erkennung mit dem gerade
gesprochenen Text. Pro Plattform misst ein Test mit 50 Antworten (SC-005); wo er scheitert, gilt
dort über Lautsprecher nur Wake Word und Stopp-Wort (FR-016). Welche Plattform welches Verhalten
hat, steht in `docs/voice-assistant-platforms.md` und als Hinweis in den Einstellungen.

**Kopfhörer**: Android meldet den Ausgabeweg (`AudioDeviceInfo`); auf dem Desktop schätzt haex ihn
aus dem Gerätenamen und bietet eine Einstellung „Ausgabe über Kopfhörer“ mit „automatisch“ als
Standard.

**Alternativen**: Echo-Unterdrückung der Plattform (Android `VOICE_COMMUNICATION`,
macOS VoiceProcessingIO, PipeWire-Modul, Windows-Kommunikationsmodus) — je Plattform eigener Code,
nicht über `cpal` erreichbar, auf Android geräteabhängig; bleibt Option, falls AEC3 auf einem Ziel
nicht reicht.

## R10 — Antworten zum Hören aufbereiten

**Entscheidung**: Ein eigenes kleines Modul `assistant/speakable.rs` wandelt Markdown in Sprechtext:
Formatierung weg, Tabellen, Links und Codeblöcke durch kurze Beschreibung ersetzt („Ich habe dir
eine Tabelle im Chat gezeigt“), ISO-Daten und Uhrzeiten in gesprochene Form (de/en). Dazu die
Anweisung an das Modell (FR-021) als zusätzlicher Teil von `chat/tools/prompt.rs`, nur in
Sprachgesprächen.

**Alternativen**: `pulldown-cmark` — wäre eine neue Abhängigkeit für wenige Regeln; ein
zeilenbasierter Umwandler mit Tests reicht, weil die Anweisung die meiste Formatierung schon
verhindert.

## R11 — Mündliche Freigaben und Rückfragen

**Entscheidung**: Der Assistent sieht `tool-permission-request` in seiner `emit`-Closure, spricht
Wirkung und Ziel (aus Aktionsbeschreibung, `riskClass` und Eingaben, Geheimnisse geschwärzt wie in
021 `redact.rs`) und wertet die Antwort lokal aus: feste Listen für Zustimmung und Ablehnung (de/en),
sonst „unklar“. Die Entscheidung geht über dieselbe Funktion wie `respond_tool_permission`.
`chat-choice-request` (`ask_user`) liest höchstens drei Möglichkeiten vor und ordnet Ordinalzahl,
Name oder Merkmal lokal zu; was nicht passt, geht als freie Antwort an den Agenten.

**Begründung**: Ob eine Aktion ausgeführt wird, entscheidet nie das Modell (SC-009). Freigaben im
offenen Chatfenster funktionieren weiter: Wer zuerst antwortet, gewinnt; die andere Seite schließt
ihre Frage.

## R12 — Gesprächszustand, laufende Aufgaben, offene Fragen

**Entscheidung**: Ein Zustandsautomat `assistant/conversation.rs` mit den Zuständen der Anzeige
(bereit, hört zu, denkt nach, arbeitet, spricht, wartet auf Antwort, stumm, pausiert) und den
Zeitgebern Nachhörzeit, Fortsetzungszeit und Fortschrittsansage. Aktives Beenden ruft
`abort_turn`. Offene Fragen bleiben als `oneshot` in `pending_tool_approvals` bzw. der
Auswahl-Warteschlange; der Assistent merkt sich, welche zu seinem Gespräch gehören, und stellt sie
beim nächsten Wake Word zuerst. Die Uhr wird injiziert (deterministische Tests).

**Gleichzeitigkeit mit dem Chat**: Die Chat-Sperre (`ChatState::acquire_operation`) bleibt.
Läuft schon eine Antwort im Chatfenster, sagt haex „Ich bin gerade mit einer anderen Antwort
beschäftigt“ und wartet bis zu 10 s, bevor es aufgibt.

## R13 — Geheimnisse vorlesen (ADR-0020)

**Entscheidung**: Ein eigenes, natives Werkzeug `assistant.secret.speak {itemId, field}`, das nur in
Sprachgesprächen angeboten wird. Ablauf: Kopfdaten über `passwords.items.search` → Werkzeug-Aufruf
→ Assistent fragt immer mündlich nach (unabhängig von der Freigabestufe) → bei Zustimmung erzeugt
er einen `SpokenConsent`, den nur `assistant::approval` bauen kann → der Passwortdienst gibt das
Geheimnis über `reveal_for_speech(item, field, consent)` direkt an die lokale Sprachausgabe →
das Modell bekommt als Ergebnis nur „vorgelesen“, die Mitschrift einen Vorlese-Hinweis. Kein
Ereignis, kein Log, kein `Debug` trägt das Geheimnis. Buchstabieren in `assistant/spell.rs`.
Wiederholen innerhalb derselben Nachhörzeit nutzt einen nur im Speicher gehaltenen Wert, der danach
überschrieben wird.

**Begründung**: ADR-0007 bleibt gültig: Der eingebaute Agent bekommt nie ein Geheimnis. Der Zugriff
geschieht im Auftrag des Nutzers nach seiner mündlichen Bestätigung, wie „Anzeigen“ im Fenster.

**Alternativen**: Geheimnis an das Modell geben und vorlesen lassen — widerspricht ADR-0007 und
034/021. Externen Sprachdienst nutzen — ausgeschlossen (FR-027).

## R14 — Schwebende Anzeige und Tray

**Entscheidung**: `src/components/assistant/AssistantBubble.vue` in `Desktop.vue` über allen
Fenstern (eigene Ebene über dem Dock, unter Dialogen), Position als Geräteeinstellung. Zustände aus
dem Ereignis `assistant-state`. Darstellung: Eichhörnchen-Sprites pro Zustand nach dem bestehenden
Ablauf (Bildmodell, eine Zeile mit 4 Bildern, Magenta-Schlüssel); bis dahin Lucide-Symbole. Das
Tray-Symbol über das Tauri-Feature `tray-icon` mit Menü Ein/Aus, Stumm, Gespräch zeigen; unter Linux
braucht es `libayatana-appindicator` in der Nix-Umgebung und der Paketierung.

**Tastenkürzel**: Standardbelegung aus dem Aktionskatalog (`defaultKeys`), wirkt im Fenster. Umbelegen
kommt mit der späteren Kürzel-Spec; ein globales Kürzel bei minimiertem Fenster ist nicht Teil
dieser Spec. Die Spec wurde entsprechend angepasst (FR-011).

## R15 — Android: Vordergrund-Dienst

**Entscheidung**: Ein Kotlin-Dienst `AssistantService` im Plugin `holzi-android` mit
`foregroundServiceType="microphone"`, Berechtigungen `FOREGROUND_SERVICE`,
`FOREGROUND_SERVICE_MICROPHONE`, `POST_NOTIFICATIONS`. Er startet nur aus der sichtbaren App: beim
Einschalten des Modus und bei jeder Rückkehr in die App, wenn der Modus an ist (Android 14+ verbietet
den Start aus dem Hintergrund). Die Aufnahme bleibt in Rust (`cpal`/AAudio); der Dienst hält nur den
Prozess am Leben und zeigt die Benachrichtigung mit „Ausschalten“ und „Stumm“. Telefonate und
exklusive Mikrofon-Nutzung erkennt haex über Audio-Fokus und Aufnahmefehler und pausiert (FR-036).

**iOS**: Kein Hintergrund-Zuhören (App-Store-Regel 2.5.4); beim Wechsel in den Hintergrund
stoppt der Assistent, im Vordergrund läuft er wieder.

## R16 — Töne zur Laufzeit erzeugt

**Entscheidung**: `assistant/tones.rs` erzeugt die 13 Töne aus FR-043 als kurze Sinus-/FM-Klänge mit
Hüllkurve in einer gemeinsamen Tonart (Familie), direkt in den Mischer. Eigener Code, keine
Fremd-Lizenz (FR-046: Herkunft = dieses Repo, dokumentiert in `docs/voice-assistant-platforms.md`).
„Arbeitet“ wiederholt sich alle 2 s leise, solange FR-023 gilt.

**Alternativen**: Kenney „Interface Sounds“ (CC0) — passt nicht als Familie, nur als Platzhalter.

## R17 — Einstellungen und Daten

**Entscheidung**: Geräteeinstellungen `assistant.*` nach ADR-0001 (siehe
[data-model.md](./data-model.md)). Das eigene Wake Word als Gerätezeile; es gilt nur auf diesem
Gerät, andere Geräte lesen es nicht (Spec FR-002/FR-008 entsprechend präzisiert). Sprachgespräche
bekommen in `chat_threads` die Spalte `origin` (`'voice'`). Das Ende des letzten Gesprächs für die
Fortsetzungszeit hält der Assistent nur im Speicher; nach einem Neustart beginnt immer ein neues.

## R18 — Testbarkeit ohne Mikrofon

**Entscheidung**: `audio::Source` und `audio::Sink` als Traits. Für Tests: eine Quelle aus
WAV-Dateien mit Pausen dazwischen und eine Senke, die Ausgabe mitschreibt. Im e2e-Build schaltet
`HOLZI_TEST_AUDIO_SCRIPT=<json>` (nur mit Feature `e2e-audio`) die Quelle um. Testaufnahmen liegen in
`src-tauri/src/assistant/fixtures/` (selbst erzeugt, CC0). Gesprächsabläufe testet ein
Integrationstest mit festem Modell-Skript, falscher Erkennung und falscher Sprachausgabe.

## R19 — Externer Sprachdienst

**Entscheidung**: Ein Anbieter-Typ „Sprachausgabe“ mit OpenAI-kompatiblem Endpunkt `/audio/speech`
(deckt OpenAI und lokale Server ab). Zugangsdaten wie bei den Chat-Anbietern. Fällt er aus, spricht
die lokale Stimme (FR-031). Geheimnisse nie über ihn (FR-027).

## Spec-Korrekturen aus der Planung

- **FR-002 / FR-008**: Gerätebezogene Einstellungen werden nach ADR-0001 als Gerätezeilen
  synchronisiert. Die Spec sagt jetzt „gilt nur für dieses Gerät; andere Geräte übernehmen ihn
  nicht“ statt „wird nicht synchronisiert“.
- **FR-011**: Tastenkürzel lassen sich heute noch nicht umbelegen; die Spec nennt eine
  Standardbelegung, die mit der späteren Kürzel-Spec umbelegbar wird.
