# Research: Sprachassistent

**Spec**: [spec.md](./spec.md) | **Plan**: [plan.md](./plan.md) | **Stand**: 2026-10-28

Jede Entscheidung mit Begründung und verworfenen Alternativen. Versionen und Lizenzen sind auf dem
Stand vom 2026-10-28 und werden im Spike (PR B, T005–T009, T130) auf allen Zielen bestätigt, bevor
Code darauf baut.

## Ausgangslage im Code

- **Aufnahme**: `src-tauri/src/audio/mod.rs` nimmt mit `cpal` 0.18.2 (Feature `voice`) den ganzen
  Puffer auf und rechnet ihn beim Stoppen einmal auf 16 kHz Mono um (`stt::CanonicalPcm`). Kein
  Streaming, keine Pausenerkennung, auf Android derselbe Weg (AAudio, `minSdk` 26).
- **Erkennung**: Parakeet über sherpa-onnx, warm gehalten in `VoiceState`;
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

**Result T007 (2026-11-09)**: The wake word models run in **tract** (`tract-onnx` 0.22.5, pure
Rust, MIT OR Apache-2.0), not in `ort`. There is no second ONNX runtime, so nothing collides with
the statically linked one of sherpa, and nothing native has to ship on Android or iOS. A spike
against values from onnxruntime in the training pipeline matched within 2·10⁻⁵ for the embeddings,
and the classifier scores were identical. On the reference desktop the embedding costs 11.9 ms per
80 ms step and the classifier 0.07 ms; the detector runs only while the pause detection hears
speech. tract 0.22 rejects a Pad node whose optional constant value is left out, so the shipped
`speech_embedding.onnx` has the value 0 written in, with unchanged output; the pipeline's export
does this and checks it. `ort` stays out of the dependencies.

**Stand nach T001 (2026-10-29)**:

- **Linken**: Ohne Features linkt `sherpa-onnx-sys` 2.0.1 auf dem Desktop statisch, die
  ONNX-Runtime steckt mit drin; neben dem Binary muss nichts ausgeliefert werden. Android und iOS
  gibt es nur als geteilte Bibliothek; für Android kopiert das Build-Skript die `.so`-Dateien
  selbst nach `jniLibs`, für iOS das xcframework. Damit entfällt `shared` auf dem Desktop. Braucht
  das Wake-Word-Modell `ort` (T007), kollidieren zwei statische Runtimes; dann wird neu entschieden.
- **Herkunft der Bibliotheken**: Das Build-Skript lädt die Archive ohne Prüfsumme von GitHub. haex
  übergibt sie über `SHERPA_ONNX_ARCHIVE_DIR` (`.cargo/config.toml`), nachdem
  `scripts/fetch-sherpa-onnx.ts` jede gegen die SHA-256 aus den GitHub-Release-Angaben geprüft hat
  (`scripts/lib/sherpaOnnxArchives.ts`). `pnpm install` lädt sie im `postinstall`, die CI vor jedem
  Build mit `voice`. Archivgrößen: Linux x64 22,7 MB, macOS 20–21 MB, Windows x64 123 MB, Android
  (alle ABIs) 45 MB.
- **Neue Build-Abhängigkeiten** (`bzip2`, `tar`, `zip`, `zstd` …): MIT, Apache-2.0, BSD-3, CC0,
  bzip2-1.0.6; keine GPL.
- **Fehlerverhalten**: Ein kaputtes Modell liefert einen Fehler (NULL), der Prozess läuft weiter;
  sherpa schreibt dabei eine Zeile mit dem Modellpfad nach stderr.
- **Linux**: Pausenerkennung (`assistant::vad`) mit Silero VAD int8 auf `de_sample.wav` grün.
  Windows, macOS und Android prüft die Plattform-Probe (Schritt `speech-detection`).
- **iOS**: Das Repository hat noch kein iOS-Projekt (`src-tauri/gen/` enthält nur `android`). Der
  iOS-Teil von T005 wartet, bis es eines gibt.

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

**Result T058 (2026-11-08)**: openWakeWord's own `melspectrogram.onnx` and `embedding_model.onnx`
are not used. Its README puts "all of the included pre-trained models" under CC BY-NC-SA 4.0, and
five issues asking whether that covers the two feature models (#313, #338, #346, #348, #357) have no
answer from the author. The embedding itself is Google's `speech_embedding/1` (Apache-2.0, Lin et
al. 2020, arXiv:2002.01322). haex uses the Apache-2.0 ONNX conversion made directly from Google's
weights: Hugging Face `androiddrew/google-speech-embedding-onnx`, revision
`e38c9f3880a56ba46e5094af7b69e78d3d2c4b7b`, file `model.onnx`, 2,183,225 bytes, SHA-256
`bfef7f0ace055206df8aeab960bb0fbe6e44898a8549ef79ae6a1ed6591169d3`. Its NOTICE names the original
Google archive (SHA-256 `0f626c42…a27f`) and the modifications. The included export script is stated
to reproduce the file byte for byte (not re-run here). The log-mel front end is inside the graph
(16 kHz mono, 11,120 leading zeros, 400-sample Hann window, hop 160, 512-point FFT, 32 mel bins,
`ln(mel + 0.001)`), so no separate melspectrogram model is needed. The output is one 96-value
embedding every 80 ms (1,280 samples), the backbone openWakeWord trains on. Only the small
classifier for "Haex" and "Hey Haex" is trained by haex (T059/T060). The model card notes that
features computed over a long recording differ from those of single clips: the classifier is trained
on the same streaming windows it sees at run time.

**Result T059–T061 (2026-11-09)**:

- **Word:** said [hɛks], like "Hexe" without the e (operator 2026-11-08). The synthesizers get "Hex"
  and "Hey Hex", not the brand spelling "Haex". Both forms stay wake words (operator 2026-11-08),
  although the lone one-syllable "Hex" is recognized clearly worse.
- **Pipeline:** https://github.com/haex-space/haex-wakeword (Apache-2.0).
- **Positives:** Piper voices trained on CC0, CC-BY, public-domain or Apache-2.0 data, run as a
  separate process because Piper is GPL-3.0. Piper's MLS voices babbled and were dropped. Pocket
  TTS speaks with Kyutai's CC0 voice donations.
- **Negatives:** about 500 h of Multilingual LibriSpeech and FLEURS (CC-BY-4.0). Common Voice
  needs an account and is not used. Confusables were synthesized: "-ks" words and what the first
  models fired on, such as "he said" or "he went".
- **Published:** `haex-space/haex-wakeword` on Hugging Face, revision
  `d0d2dc6225c3bb1103756ebaa15a4e1d16cd77b8`. It holds `speech_embedding.onnx` (2,183,273 bytes)
  and `hex.onnx` (857,934 bytes).

Measured on 37 h of held-out speech, from speakers the training never heard, and on held-out
synthetic speakers. haex fires when a number of consecutive windows reach a threshold, then stays
quiet for 2 s:

| `assistant.sensitivity` | Rule             | "Hey Hex" | "Hex" alone | False accepts per hour |
| ----------------------- | ---------------- | --------- | ----------- | ---------------------- |
| low                     | 3 windows ≥ 0.9  | 88.7 %    | 47.1 %      | 0.00                   |
| medium (default)        | 2 windows ≥ 0.98 | 90.5 %    | 49.6 %      | 0.05                   |
| high                    | 2 windows ≥ 0.5  | 94.2 %    | 72.3 %      | 0.48                   |

"medium" meets SC-004. These figures come from synthesized voices. SC-003, with real voices at up
to 3 m, is measured in the app (T065).

The lone "Hex" stayed near 50 % at "medium" in every attempt: more natural speech, targeted
confusables, and requiring a pause after the word. Recordings of real users are the next step for
it.

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
Parakeet-Transkripten, Schwelle aus den Aufnahmen) und nimmt das bessere Verfahren. Das Ergebnis kommt in diese
Datei.

**Alternativen**: EfficientWord-Net — studentisches Projekt in Python, mehr Fehlauslösungen.
Training auf dem Gerät — zu schwer für Telefone.

## R6 — Ende der Äußerung

**Entscheidung**: Silero VAD (über sherpa) mit einstellbarer Pause (Standard 0,8 s, Spanne
0,5–2,0 s), Mindestlänge 250 ms und Vorlauf von 1,5 s aus dem Ringpuffer, damit „Hey Haex, wie
viele …“ in einem Atemzug ganz ankommt. Höchstdauer einer Runde 60 s wie in 008.

**Stand nach PR B Teil 3 (2026-10-30)**: `assistant::listen` läuft auf dem Strom
(`audio::stream`). sherpa schneidet bei 60 s, behält aber vor dem erkannten Sprechbeginn einen
Vorlauf von etwa 0,5 s; eine geschnittene Äußerung ist deshalb rund 60,5 s lang.

**Stand nach PR B Teil 4 (2026-10-31)**: Das Silero-Modell ist 212 KB groß und MIT-lizenziert; es
liegt im Binary (`assistant/models/`) statt im Download-Katalog und wird beim ersten Gebrauch nach
`models/silero-vad/` geschrieben. Die Pausenerkennung ist damit immer bereit.

**Update after T161 (2026-11-02)**: a thinking pause of 0.8 s already ended what was said
(operator). The default is now 1.5 s and the range 0.5–3.0 s (data-model.md); the pause is set
with a slider on the assistant's settings page.

**Alternativen**: Smart Turn v3 (BSD-2, erkennt Satzende auch bei Denkpausen) — später, falls feste
Pausen stören; vermerkt als `ponytail:`-Kommentar. TEN VAD — Lizenz mit Wettbewerbsklausel. WebRTC
VAD — zu ungenau.

## R7 — Erkennung pro Äußerung: Parakeet

**Entscheidung**: Nach dem Ende einer Äußerung geht ihr Puffer an den gewählten `SttAdapter`. Kein
Streaming-STT. Stopp-Wörter (`stt::interrupt::match_interrupt`) prüft der Assistent zuerst und bricht
ohne Modell ab (FR-017).

NVIDIA Parakeet TDT 0.6B v3 über sherpa (`OfflineRecognizer`, `nemo_transducer`, int8, CC-BY-4.0)
ist der einzige lokale STT-Katalogeintrag. `VoiceState` hält den Adapter warm; Push-to-Talk (008)
und Assistent teilen ihn. Parakeet kann 25 europäische Sprachen, darunter Deutsch und Englisch.
Whisper und die Candle-Laufzeit sind entfernt; alte Katalog-IDs werden bei der Migration
bestehender Vaults bereinigt.

**Historische Vergleichsmessung** (T130, 2026-10-30): Die damaligen Whisper/Parakeet-Messwerte
stehen unten als Entscheidungsgrundlage. Der Whisper-Adapter und `tests/stt_compare_eval.rs` wurden
entfernt; künftige STT-Prüfungen beziehen sich auf Parakeet.

**Begründung**: Whisper base ist im Deutschen schwach, Whisper small auf der CPU langsam (008:
whisper-small braucht in `tauri dev` für 11 s Audio spürbar lange). Parakeet ist laut Berichten
deutlich schneller als Whisper small, liefert Satzzeichen und Wortzeiten (die 067 braucht) und
erfindet bei Stille kaum Text (nicht gemessen). sherpa ist durch R3 ohnehin im Build.

**Ergebnis T130 (2026-10-30)**: `tests/stt_compare_eval.rs` (Release) auf je 25 Sätzen aus Google
FLEURS (dev, CC BY 4.0, vorgelesen, ≈ 11 s pro Satz), Referenz-Desktop (8 Kerne, ohne GPU).
Wortfehlerrate in %, darunter die mittlere Rechenzeit pro Satz:

| Modell                                     | de   | en   | fr   | es   | it   | pt   | Zeit pro Satz |
| ------------------------------------------ | ---- | ---- | ---- | ---- | ---- | ---- | ------------- |
| Parakeet TDT 0.6B v3 (k2-fsa, int8)        | 14,4 | 3,4  | 6,8  | 6,3  | 4,5  | 5,5  | 0,6 s         |
| parakeet-primeline (deutsch nachtrainiert) | 6,5  | 4,4  | 9,1  | 4,5  | 4,1  | 5,3  | 0,6 s         |
| Whisper tiny                               | 36,3 | 10,4 | 48,8 | 19,5 | 35,7 | 25   | 1,0–2,1 s     |
| Whisper base                               | 24,7 | 7,6  | 28,3 | 11,4 | 22,7 | 12,6 | 2,3–3,9 s     |
| Whisper small                              | 11,6 | 5,8  | 14,0 | 6,9  | 11,9 | 8,3  | 5,7–9,9 s     |

- Parakeet war in diesem Vergleich 5- bis 15-mal schneller als Whisper und lag als einziges unter 1 s pro Satz
  (Echtzeitfaktor ≈ 0,05). Das Ziel „Text ≤ 1 s nach Sprechende“ hält nur Parakeet.
- `parakeet-primeline` (primeline, CC BY 4.0; sherpa-Export `flozen1981/parakeet-primeline-onnx`,
  Revision `d548e25b…`) halbiert die deutsche Fehlerrate bei gleicher Geschwindigkeit; en und fr
  verlieren 1–2 Punkte, es/it/pt werden etwas besser. Im Mittel das genaueste Modell (5,7 %).
  Es wird der Parakeet-Eintrag im Katalog (Betreiber, 2026-10-30).
- Whisper mit sechs statt zwei Kandidatensprachen erkennt de und en gleich gut; nur Whisper tiny
  wird auf Deutsch langsamer (Ausreißer bis 19 s).
- Parakeet über sherpa meldet keine Sprache (`OfflineRecognizerResult` hat kein Feld dafür). Für die
  Gesprächssprache (R22) kommt sherpas `SpokenLanguageIdentification` als Alternative zur Zeile
  `detect` der Wortliste in Frage.
- 25 Sätze je Sprache zeigen die Richtung, keine Nachkommastellen. Telefon und kurze
  Assistent-Äußerungen fehlen noch.
- **Standard** (Betreiber, 2026-11-06): `parakeet-primeline` ist auf Desktop und Telefon der
  einzige lokale STT-Eintrag, die Voreinstellung und die Onboarding-Empfehlung. Die Regel steht in
  `stt::catalog::default_entry`; die Oberfläche liest sie als `isDefault` aus der Katalogliste.

**Alternativen**: candle-Whisper und Whisper zusätzlich über sherpa — verworfen; Parakeet deckt die
benötigten Sprachen ab und ist auf allen unterstützten Presets der lokale STT-Pfad. Canary-180m-flash
(CC-BY-4.0, nur de/en/es/fr, also ohne Italienisch und Portugiesisch) — als Ersatz, falls Parakeet
auf dem Telefon zu langsam ist.

## R8 — Lokale Sprachausgabe: Kyutai Pocket TTS über sherpa, Stimmen als Referenzaufnahmen

**Entscheidung**: Kyutai Pocket TTS (≈100 M Parameter, Code MIT, Gewichte CC-BY-4.0, erste Ausgabe
≈200 ms auf 2 Kernen eines M4) für sechs Sprachen: Deutsch, Englisch, Französisch, Spanisch,
Italienisch, Portugiesisch. Ein Modell pro Sprache im neuen Katalog `tts/tts_catalog.json`
(Revision, Größe, Lizenz, Eignung), geladen erst, wenn eine Stimme der Sprache gewählt wird. Für die
fünf Sprachen außer Englisch gibt es zusätzlich die größere Variante mit 24 Schichten als
„höhere Qualität“.

Pocket TTS hat in sherpa-onnx keine eingebauten Stimmen: Jeder Aufruf bekommt eine kurze
Referenzaufnahme (`GenerationConfig.reference_audio`), aus der das Modell die Stimme nachbildet; die
daraus berechnete Einbettung hält sherpa im Zwischenspeicher (`voice_embedding_cache_capacity`). Eine
mitgelieferte Stimme ist darum nur eine Referenzaufnahme von einigen hundert KB, die im Binary liegt
(`tts/voices/`, `include_bytes!`). Pro Sprache zwei bis drei Stimmen.

**Herkunft der Stimmen**: Die vorgefertigten Kyutai-Stimmen stammen fast alle aus englischen
Aufnahmen und tragen deren Akzent in andere Sprachen (kyutai-labs/pocket-tts#166). Deshalb nur
Aufnahmen von Muttersprachlern, und nur mit freier Lizenz: CC0 (Common Voice, Kyutai
`voice-donations`) oder CC-BY (VCTK für Englisch, CML-TTS für Französisch), sonst eigene Aufnahmen von
Freiwilligen mit schriftlicher Einwilligung als CC0. Expresso und EARS sind nur nicht-kommerziell
nutzbar und scheiden aus. Herkunft, Lizenz und Namensnennung je Stimme stehen in
`tts/voices/voices.json` und in den Lizenzangaben der App.

**Bezug der Modelle**: Das Kyutai-Repository auf Hugging Face ist zugangsbeschränkt (Anmeldung und
Zustimmung). Geladen wird darum der sherpa-Export (k2-fsa, CC-BY-4.0 erlaubt Weitergabe) mit
festgelegter Revision; ob sherpa 2.0 für alle sechs Sprachen fertige Exporte anbietet (laut
Release-Notes „PocketTTS (multilingual)“), klärt der Spike T008. Fehlt eine Sprache, exportiert der
Betreiber sie selbst und hängt sie an ein eigenes Release.

**Ausgabe**: satzweise. Der Assistent schneidet die Antwort an Satzgrenzen und gibt jeden Satz
einzeln an die Sprachausgabe (FR-022).

**Spike** (T008): Qualität, Akzent und Echtzeitfaktor je Sprache auf Referenz-Desktop und -Telefon,
normal und mit 24 Schichten. Reicht Deutsch nicht, ist der Ersatz Piper `de_DE-thorsten` mit eigener
freier Aussprache-Tabelle (gruut, MIT); die Herkunft der Stimme wird vorher rechtlich geprüft.

**Ergebnis T008 (2026-10-30)**, `tests/tts_spike_eval.rs`, fünf Assistent-Sätze je Sprache,
Stimme nachgebildet aus je einem FLEURS-Satz (nur zum Test):

- k2-fsa bietet Pocket TTS nur auf Englisch an (`sherpa-onnx-pocket-tts-int8-2026-01-26`). Die
  übrigen Sprachen gibt es als ONNX nur im Export von KevinAHM (`KevinAHM/pocket-tts-onnx`, CC BY 4.0
  seit 2026-01-28; der Hinweis „non-commercial“ im README des sherpa-Pakets stammt von davor).
- Diese Modelle erwarten vor der Stimm-Einbettung ein gelerntes Startsignal (`bos_before_voice.npy`,
  `insert_bos_before_voice` in `bundle.json`). sherpa-onnx 2.0.1 kennt das nicht: Ohne Startsignal
  findet das Modell kein Satzende und erzeugt 40–80 s Rauschen.
- **Umgehung**: Das Startsignal wird als `Concat` vor dem Ausgang in den Mimi-Encoder eingebaut
  (eigenes Skript, `onnx`-Bibliothek). Danach sprechen alle sechs Sprachen über das unveränderte
  sherpa. Sauberer wäre ein Upstream-PR an sherpa-onnx, der `bos_before_voice` unterstützt; bis dahin
  führt haex die angepassten Encoder als eigene, gepinnte Modelldateien (CC BY verlangt Namensnennung
  und einen Hinweis auf die Änderung).
- Erster Ton pro Satz 0,7–1,6 s (de, es, it, pt, en), Echtzeitfaktor 0,4–1,0; Französisch nur als
  24-Schichten-Variante, dort ≈ 2 s pro Sekunde Audio. sherpa ruft den Fortschritt erst am Satzende
  auf, deshalb schneidet haex Antworten selbst in Sätze (FR-022).
- Verständlichkeit, gemessen als Wortfehlerrate von Parakeet auf der erzeugten Sprache: meist 0–30 %,
  Fehler vor allem bei Eigennamen und Zahlen. Kurze Satzanfänge („Done.“, „Devo“) gehen öfter verloren.
  Pocket würfelt (Temperatur 0,7); zwei Läufe unterscheiden sich spürbar.
- Der Betreiber hat Deutsch und Englisch angehört: klingt gut (2026-10-30).
- Text mit Nullzeichen lässt `generate_with_config` panicken (`CString::new(..).unwrap()`); haex
  entfernt solche Zeichen vorher.

**Stand nach PR B Teil 4 (2026-10-31)**:

- Die Modelle liegen als eigenes Repo `haex-space/pocket-tts-sherpa-onnx` auf Hugging Face (CC BY 4.0,
  Revision `d8f5915ed8bd94e1aa3dbd5a219360b3bb966190`), ein Ordner je Sprache mit den Dateinamen von
  sherpa. Geändert ist nur `encoder.onnx` von de, es, fr, it und pt (Startsignal eingebaut); das
  Skript liegt im Repo unter `tools/` und erzeugt die Dateien byte-gleich. Englisch ist das Paket von
  k2-fsa. Das Skript ist als k2-fsa/sherpa-onnx#4148 vorgeschlagen.
- Pocket über sherpa beachtet `GenerationConfig.speed` nicht: Tempo 0,75, 1,0 und 1,5 ergaben 2,43 s,
  2,44 s und 2,59 s für denselben Satz. haex streckt die Ausgabe darum selbst (`audio::stretch`,
  WSOLA, eigener Code): Die Länge ändert sich, die Tonhöhe nicht.
- `tts::local::Speaker` erzeugt Satz für Satz auf einem eigenen Thread unter `VaultGate` und schiebt
  das Ergebnis in den Mischer; `stop()` verwirft, was der Mischer hält, die Warteschlange und den Satz,
  der gerade entsteht. Ein Schließen des Vaults beendet den Thread.

**Alternativen**: Piper-Engine — seit 1.3.0 GPL-3.0 (espeak-ng). Kokoro-82M — kein Deutsch.
Supertonic 3 — Repo archiviert, OpenRAIL-M. Die vorgefertigten Kyutai-Stimmen — englischer Akzent und
teils nicht-kommerzielle Quellen.

## R9 — Echo-Unterdrückung und Dazwischenreden

**Entscheidung**: AEC3 in der App hinter dem Trait `EchoCanceller`. Zuerst `sonora` (reines Rust,
BSD-3, besteht die C++-Tests von WebRTC, baut für Android und iOS), auf eine Commit-SHA gepinnt;
Ersatz `webrtc-audio-processing` 2.x (BSD-3, C++). Zusätzlich während eigener Ausgabe: höhere
Schwelle für Dazwischenreden, Mindestlänge 250 ms, Abgleich der Erkennung mit dem gerade
gesprochenen Text. Pro Plattform misst ein Test mit 50 Antworten (SC-005); wo er scheitert, gilt
dort über Lautsprecher nur Wake Word und Stopp-Wort (FR-016). Welche Plattform welches Verhalten
hat, steht in `docs/voice-assistant-platforms.md` und als Hinweis in den Einstellungen.

**Built 2026-11-07 (PR G)**: `sonora` 0.2.0 from crates.io instead of a git SHA (0.3.0 was published
the same day; 0.2.0 is two releases old and pinned with `=`). It runs at 16 kHz mono in the stream
worker, 10 ms frames, with only the echo canceller on (no noise suppression or gain control, which
would change what speech recognition hears). Measured in echo_tests.rs on a synthetic room (30 ms
delay, two reflections): ≥ 20 dB less echo after 3 s, the user's voice alone and over haex stays.
Barge-in: Silero speech (≥ 250 ms) plus a minimum loudness of the cancelled signal over the last
250 ms (`BARGE_IN_MIN_RMS` 0.005); the per-platform measurement (T103) still decides the defaults.

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
eine Tabelle im Chat gezeigt“), ISO-Daten und Uhrzeiten in gesprochene Form (sechs Sprachen, R22). Dazu die
Anweisung an das Modell (FR-021) als zusätzlicher Teil von `chat/tools/prompt.rs`, nur in
Sprachgesprächen.

**Alternativen**: `pulldown-cmark` — wäre eine neue Abhängigkeit für wenige Regeln; ein
zeilenbasierter Umwandler mit Tests reicht, weil die Anweisung die meiste Formatierung schon
verhindert.

## R11 — Mündliche Freigaben und Rückfragen

**Entscheidung**: Der Assistent sieht `tool-permission-request` in seiner `emit`-Closure, spricht
Wirkung und Ziel (aus Aktionsbeschreibung, `riskClass` und Eingaben, Geheimnisse geschwärzt wie in
021 `redact.rs`) und wertet die Antwort lokal aus: feste Listen für Zustimmung und Ablehnung (sechs Sprachen, R22),
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

**Entscheidung**: `assistant/tones.rs` erzeugt alle 15 `ToneId`-Werte aus
`contracts/tauri-commands.md`: die 10 Anlässe aus FR-043 plus fünf weitere (`secret`, `modeOn`,
`modeOff`, `muted`, `unmuted`), deren Quelle in FR-043 nicht festgelegt ist, als kurze Sinus-/FM-Klänge mit
Hüllkurve in einer gemeinsamen Tonart (Familie), direkt in den Mischer. Eigener Code, keine
Fremd-Lizenz (FR-046: Herkunft = dieses Repo, dokumentiert in `docs/voice-assistant-platforms.md`).
„Arbeitet“ wiederholt sich alle 2 s leise, solange FR-023 gilt.

**Umsetzung (PR B Teil 4)**: Alle Töne bestehen aus Noten der D-Dur-Pentatonik um D5 (587 Hz),
je ein Sinus mit leichtem FM-Anteil (Verhältnis 2, Index 0,6), 4 ms Anstieg und abklingender Hüllkurve;
ein bis drei Noten, höchstens 320 ms, Spitze 0,5. „Arbeitet“ ist eine leise einzelne Note mit
`WORKING_REPEAT` = 2 s; wann er läuft, steuert der Gesprächskern (PR C). „Hört zu“ und „Geheimnis“
klingen auch bei ausgeschalteten Tönen (FR-045). Jeder Ton geht zusätzlich als `assistant-tone` an die
Oberfläche.

**Neugestaltung (2026-11-02, Betreiber: „schwer unterscheidbar“)**: Dieselbe Klangfarbe, Tonlage und
Länge für alle Töne machten „Hört zu“ und „Verstanden“ fast gleich. Jetzt hat jede Note eine
Klangfarbe (`Timbre`: Glocke, Holz, weich, schnarrend, Tick) und darf zwischen zwei Stufen gleiten;
die Skala reicht über zweieinhalb Oktaven (D4 bis A6). Kein Paar von Tönen teilt Klangfarben-Folge und
Richtung (Test `every_tone_is_distinguishable`). „Hört zu“ gleitet als Glocke eine Oktave hinauf,
„Verstanden“ sind zwei kurze Holzschläge aufwärts, „Nicht verstanden“ zweimal derselbe schnarrende
Ton. Eigene Töne pro Anlass: FR-045a.

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

## R20 — Stimmen nachbilden

**Entscheidung**: Eine nachgebildete Stimme ist eine Referenzaufnahme wie die mitgelieferten (R8).
Aufnahme über den Duplex-Strom (R2) oder eine importierte Audiodatei (WAV, FLAC, Ogg, MP3 über die
vorhandene Dekodierung), umgerechnet auf die Abtastrate des Modells, beschnitten auf 10–30 s
Sprache (Pausenerkennung R6 entfernt Stille am Rand). Prüfung vor dem Speichern: Sprachanteil
≥ 10 s, Pegel, Übersteuerung (Anteil Samples nahe Vollausschlag), Rauschabstand grob aus den Pausen.
Danach eine Probe mit einem festen Satz der Sprache.

**Update 2026-11-03 (operator: own voices did not sound convincing)**: three losses on the way to the
model, all fixed. (1) The microphone path stored the 16 kHz signal of speech recognition, so
everything above 8 kHz was missing, and the linear conversion folded the rest back; the clone now
opens the device at its own rate (falling back to the assistant's 16 kHz stream only where the
device cannot be opened twice) and brings it down to 24 kHz through a windowed-sinc low-pass
(`audio::resample::resample_filtered`), as for files. (2) sherpa-onnx reads only the first 10 s of a
reference (`max_reference_audio_len`, default 10) and cuts mid-word; Kyutai's own code reads up to
30 s and lets the prompt end on a pause (`end_on_pause`). haex now passes 30 s, and a kept recording
is at most 30 s in total, cut after the last pause of at least 0.2 s before that limit. (3) The
built-in voices of 12–15 s were cut at 10 s as well and are now read whole. Kyutai: "the audio
quality of the sample is also reproduced", so a quiet room and a close microphone still matter most.

Gespeichert in der CRDT-Tabelle `haex_tts_voices`, die Aufnahme in `haex_tts_voices_binaries`
(Muster `haex_passwords_binaries`, unter 25 MiB: 30 s 16 bit Mono 24 kHz ≈ 1,4 MB) und dem Zeitpunkt der bestätigten
Einwilligung. Synchronisiert wie alle Vault-Daten; die Einbettung wird auf jedem Gerät neu berechnet
und nur im Speicher gehalten.

**Einwilligung**: Kyutai verbietet das Nachbilden ohne ausdrückliche und rechtmäßige Einwilligung.
haex kann sie nicht prüfen; der Nutzer bestätigt sie mit einem Kontrollkästchen, der Zeitpunkt wird
gespeichert (FR-050). Die Stimme verlässt das Gerät nur verschlüsselt im Vault; haex gibt
gesprochenes Audio nirgends als Datei aus.

**Alternativen**: Nur die Einbettung speichern statt der Aufnahme — kleiner, hängt aber an der
Modellversion; ein Modellwechsel würde alle Stimmen ungültig machen. Nur auf dem Gerät — verworfen
(Betreiber: über den Vault synchronisieren).

## R21 — Modelle importieren

**Entscheidung**: Import aus einem lokalen Ordner (oder Archiv `.tar.bz2` wie in den sherpa-Releases)
oder aus einem Hugging-Face-Repository mit Revision (beim Import festgehalten). haex erkennt die
Modellart an den Pflichtdateien der sherpa-Konfigurationen, die 2.x anbietet: Pocket TTS (z. B.
Community-Modelle für Polnisch oder Tschechisch, dann mit Referenzaufnahme wie R8), Kokoro,
Supertonic, Matcha, Kitten, VITS. Welche davon ohne espeak-ng (seit 2.0 entfernt) wirklich sprechen,
klärt der Spike T137; nicht unterstützte Arten lehnt der Import mit Grund ab. Mehrsprecher-Modelle
zeigen ihre Sprecher-Ids zur Wahl.

Kopiert nach `<AppLocalData>/models/user-tts-<id>/` und wie Katalog-Downloads in
`device_downloaded_models_no_sync` eingetragen (Pfad, Größe, SHA-256); Name, Art, Sprachen und
Sprecher in der gerätebezogenen Tabelle `haex_tts_models_no_sync` (nicht synchronisiert, wegen der
Größe). Name, Sprachen und Lizenzangabe
gibt der Nutzer an.

**Sicherheit**: Es werden nur Modell-, Token- und Lexikon-Dateien übernommen; Bibliotheken
(`.so`, `.dll`, `.dylib`), Skripte und ONNX-Modelle mit benutzerdefinierten Operator-Domänen
lehnt der Import ab, damit keine fremde native Bibliothek geladen wird. Größe höchstens 2 GB.
Dateien werden vor dem ersten Laden nicht ausgeführt, nur von der ONNX-Runtime gelesen.

**Alternativen**: Freie Zuordnung von Dateien zu Feldern durch den Nutzer — fehleranfällig;
Erkennung über Dateinamen deckt die sherpa-Archive ab. Import über den externen Dienst (R19) bleibt
der Weg für Engines, die sherpa nicht kann (z. B. XTTS auf einem eigenen Server).

## R22 — Sechs Sprachen, eine Wortliste

**Entscheidung**: Alles, was haex selbst sagt oder lokal versteht, steht in **einer** Wortliste
`src-tauri/src/assistant/lexicon.json` (im Binary über `include_str!`): eine Zeile pro Bedeutung,
eine Spalte pro Sprache. Eine neue Sprache ist eine neue Spalte; eine fehlende Übersetzung ist eine
leere Zelle, die der Test sofort findet; Muttersprachler prüfen eine Datei statt verstreuter
Tabellen.

```text
{ "languages": ["de", "en", "fr", "es", "it", "pt"],
  "entries": [
    { "id": "hear.yes",  "match": "start",  "de": ["ja", "mach", "okay"], "en": ["yes", "go ahead"], … },
    { "id": "hear.stop", "match": "whole",  "de": ["stop", "stopp", "halt", "abbrechen"], "en": ["stop"], … },
    { "id": "say.notUnderstood", "de": "Das habe ich nicht verstanden.", "en": "…", … },
    { "id": "spell.#",   "de": "Raute", "en": "hash", … } ] }
```

- **Zeilenarten** am Präfix der Id: `hear.*` (Wörter, die haex lokal erkennt; Zelle ist eine Liste,
  `match` gilt für alle Sprachen: `whole` = ganze Äußerung, `start` = am Anfang, `contains`),
  `say.*` (Sätze, die haex spricht, mit Platzhaltern wie `{label}`), `spell.*` (Buchstabiernamen),
  `ordinal.*` (erste, zweite, letzte), `prompt.voice` (Gesprächsanweisung an das Modell, FR-021) und
  `detect` (häufige kurze Wörter je Sprache, siehe unten).
- **Inhalt**: Schlusswendungen, Stopp-Wörter, Bitte um Wiederholung, Zustimmung und Ablehnung
  (R11), Ordinalzahlen für Rückfragen, Buchstabiernamen (FR-026), alle gesprochenen Meldungen und die
  Gesprächsanweisung.
- **Schlusswendungen** (gebaut 2026-11-01): `hear.end` beendet ein Gespräch nur, wenn um die Wendung
  herum nichts steht außer kleinen Zusätzen aus `hear.endExtra` („danke“, „erstmal“, „okay“). So
  endet „Danke, das war's erstmal“, aber nicht „Das war's noch nicht, schreib Jonas eine Mail“ und
  nicht „danke“ allein. `hear.filler` („äh“, „hm“) macht eine Äußerung nicht verständlich (FR-019).
- **Nicht in der Liste**: gesprochene Daten, Uhrzeiten und Zahlen (R10). Das sind Regeln mit
  Grammatik (Kasus, Zahlwortbildung), keine Wörter; sie bleiben Code in `speakable.rs` mit einem
  Zweig pro Sprache.
- **Eine Quelle für die Sprachliste**: `languages` ist die Liste der Sprechsprachen. Die
  Stopp-Wörter aus 008 (`stt::interrupt::match_interrupt`, heute fest „stop“, „halt“,
  „abbrechen“) lesen künftig die Zeile `hear.stop`; das Wire-Format von `InterruptCommand` bleibt.
  Das frühere Whisper-Backend beschränkte die Spracherkennung auf `SUPPORTED_LANGUAGES = ["de", "en"]`
  (`stt/local.rs`, an die Oberflächensprachen gekoppelt) und nimmt künftig `languages` aus der
  Liste. Mehr Kandidaten konnten whisper-tiny bei kurzen Äußerungen öfter danebenliegen lassen; T130
  misst das.

**Gesprächssprache**: folgt der erkannten Sprache der Äußerung, sonst der Oberfläche. Parakeet
erkennt die Sprache selbst; die Zeile `detect` bleibt für mehrdeutige Fälle verfügbar (meiste Treffer
gewinnt, bei Gleichstand die vorige Gesprächssprache).

**Prüfung**: Ein Rust-Test verlangt für jede Zeile alle Sprachen aus `languages`, gleiche
Platzhalter in allen Spalten und keine Überschneidung von `hear.yes` und `hear.no` innerhalb einer
Sprache. Die Spalten fr, es, it, pt prüft vor der Freigabe je ein Muttersprachler (T151).

**Alternativen**: Eine Tabelle pro Sprache oder pro Modul — eine fehlende Übersetzung fällt erst zur
Laufzeit auf, neue Sprachen berühren viele Dateien (Vorschlag des Betreibers, 2026-10-28). Die
vue-i18n-Dateien der Oberfläche — die sind Frontend, nur de/en, und Rust darf sie nicht lesen.
Übersetzung zur Laufzeit durch das Modell — langsamer, und Zustimmung oder Ablehnung darf nie das
Modell auswerten (SC-009).

## Spec-Korrekturen aus der Planung

- **FR-002 / FR-008**: Gerätebezogene Einstellungen werden nach ADR-0001 als Gerätezeilen
  synchronisiert. Die Spec sagt jetzt „gilt nur für dieses Gerät; andere Geräte übernehmen ihn
  nicht“ statt „wird nicht synchronisiert“.
- **FR-011**: Tastenkürzel lassen sich heute noch nicht umbelegen; die Spec nennt eine
  Standardbelegung, die mit der späteren Kürzel-Spec umbelegbar wird.

## R23 — Text front end and other speech models (2026-11-04)

**Decision**: haex writes numbers, times, dates, amounts, units and abbreviations out itself
(FR-022b, `tts::spoken`), between the text and the speech model, so it holds for every chat model.
Pocket TTS has no text front end; complete TTS systems put such a step before their model (Piper
uses espeak-ng). None of the free models compared below documents German text normalization, so
the step stays necessary whichever model haex uses.

**Other models (research 2026-11-04, not listened to)**: Pocket TTS itself was retrained for
German in Kyutai's releases v3.3.0 and v4.0.0 (Sept/Oct 2026); haex's German export is probably
the April one, so a new export is the cheapest gain. Qwen3-TTS (Apache-2.0, all six languages,
cloning from about 3 s, llama.cpp support) sounds best but runs only about real time on a CPU.
Supertonic 3 (sherpa-onnx, fast) has OpenRAIL-M weights and no open cloning. Piper's code is
GPL-3.0 and sherpa-onnx 2.x has no phonemizer for it. F5-TTS, XTTS-v2 and others are
non-commercial. Proper names stay hard for every model; a pronunciation list is the usual answer.

## R24 — Only enrolled voices (2026-11-07)

**Decision**: speaker embeddings through sherpa-onnx 2.0.1's `SpeakerEmbeddingExtractor` (already in
the build, no new crate) with 3D-Speaker's CAM++ trained on VoxCeleb
(`3dspeaker_speech_campplus_sv_en_voxceleb_16k.onnx`, 29.6 MB, Apache-2.0 as the 3D-Speaker models),
downloaded on first use from `csukuangfj/speaker-embedding-models` at revision
`0743f301363dec56491a490f6d6cbc9d67f9a3bf`, SHA-256
`357a834f702b80161e5b981182c038e18553c1f2ca752ed6cec2052365d4129b`. VoxCeleb has speakers of many
languages, so it is not tied to English.

- Enrolment: three passages read aloud (the openings of the read-aloud texts, about 6–10 s each),
  recorded with the own-voice sessions; each becomes an embedding, the recording is dropped. The
  sentences must agree (each ≥ 0.45 cosine to their mean, else "inconsistent"); the stored print is
  the normalized mean, with the model id.
- Matching: cosine similarity ≥ 0.5 to any enrolled print (`MATCH_THRESHOLD`); a provisional value
  until the SC-018 measurement (T177). Prints of another model are never compared.
- In a conversation every utterance is checked before transcription; one of nobody enrolled is
  dropped silently (no request, stop word or answer). A barge-in waits for one second of speech
  before it is checked, so with enrolled voices it is slower than SC-005's 0.5 s; the operator
  accepted ignoring everyone else as the reason.
- Without anyone enrolled, or with the device setting off, everyone may speak as before. If
  enrolled prints exist and the setting is on, a missing or incompatible model prevents opening the
  assistant session so it never silently lets an unknown voice through.

**Alternatives**: ERes2Net (VoxCeleb, 26.5 MB, Apache-2.0) is similar; WeSpeaker ResNet34 and NeMo
TitaNet are larger or under other licences. A speaker check inside the wake word (PR D) would only
cover the start of a conversation, not stop words or barge-in.
