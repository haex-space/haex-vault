# Research: Dateibrowser und Viewer (044)

Stand 2026-10-07. Quellen: Code von holzi auf `origin/main` (`0486fb9d` und folgende), haex-vault
(Repository `https://github.com/haex-space/haex-vault`, Revision
`fc4e84b61a050576ba42e0dc832d04064a8605a3`), crates.io/npm am 2026-10-07, developer.android.com.
„Probe“ heißt: `cargo check` für `aarch64-linux-android` und `x86_64-pc-windows-gnu`; das beweist
Kompilieren, nicht Verhalten zur Laufzeit.

## R1 Dateiaktionen der Agents laufen in Rust (FR-034)

- **Decision**: Die Definitionen der `files.*`-Aktionen bleiben im TS-Katalog (`src/lib/actions/`),
  bekommen aber das neue Feld `runner: 'native'`. `register_agent_actions`
  (`chat/action_commands.rs:41-82`) registriert für solche Definitionen statt `ActionTool` ein
  `NativeActionTool`, das direkt den `FilesService` aufruft. Quelle bleibt `action`: `find_actions`
  findet die Aktionen, die Prüfung „wurde angeboten“ (`tool_round.rs:173-178`) gilt, Risikoklasse kommt
  wie bei `ActionTool` aus `effect`. Den Zugriff auf `AppState` bekommt das Tool über einen Setter in
  `setup()`, wie `action_bridge` seinen Emitter (`action_bridge.rs:134`).
- **Rationale**: Ein Katalog bleibt (ADR 0006: „keine zweite Kopie des Katalogs in Rust“), die Prüfungen
  in `check-agent-actions.ts` greifen weiter, und die Aktionen laufen ohne Fenster, sobald die
  Definitionen einmal je Vault-Sitzung übergeben wurden. Rust-Tools mit eigener Quelle (wie `run_command`)
  kämen in jedes Angebot (`offer.rs:27-36`); neun davon sprengen das Kernbudget von höchstens zehn
  Werkzeugen.
- **Alternatives**: Eigene Quelle `files` mit Rust-Definitionen (verworfen: Budget, zweiter Katalog,
  `toolSource`-Typen im Frontend); Ausführung über das Fenster wie bisher (verworfen: FR-034).
- **Folge**: ADR 0011 „Native Ausführung von Katalog-Aktionen“. `check-agent-actions.ts` braucht für
  native `read`-Aktionen einen Stub im Harness statt `catalogRunner`.
- **Ausnahme**: `files.show` (FR-032a) öffnet ein Fenster und läuft deshalb als gewöhnliche
  Frontend-Aktion; ihr Handler lässt Rust vorher Berechtigung und Sperre prüfen (`files_agent_check`).

## R2 Bilder und Dokumente in Werkzeug-Ergebnissen (FR-035a)

- **Befund**: `ToolResult { content: String, is_error }` (`chat/tools/mod.rs:57`),
  `ChatRole::ToolResult` nur Text (`adapters/types.rs:34`), Anthropic schickt `tool_result` mit
  String-Inhalt (`adapters/request.rs:162-183`), lokal (mistralrs) nur Text
  (`llm/local/stream.rs:179-187`). Ob ein Modell Bilder annimmt, steht in
  `ModelCapabilities.accepted_attachment_kinds` (`model_capabilities.rs:33`), erreichbar über
  `ChatRequest.capabilities`, aber nicht in `Tool::execute`.
- **Decision**: `ToolResult` bekommt `images: Vec<ToolImage>` (Vorgabe leer; `media_type`, Base64).
  Der Turn-Runner entscheidet mit `request.capabilities`: nimmt das Modell Bilder an, gehen sie als
  Bildblöcke in den `tool_result` (Anthropic erlaubt dort ein Array aus Text- und Bildblöcken); sonst
  ersetzt er sie durch einen Hinweis. Gespeichert wird nur der Text mit einem Platzhalter
  („Bild <Name>, dem Modell gezeigt“); spätere Runden schicken das Bild nicht erneut.
- **Rationale**: Das Werkzeug muss die Fähigkeiten des Modells nicht kennen, die Entscheidung liegt dort,
  wo `capabilities` schon ankommt. Kein erneutes Senden hält Kosten und Größe der Anfragen klein.
- **Alternatives**: Kontext mit Fähigkeiten an `Tool::execute` (verworfen: Signatur aller Werkzeuge);
  Bilder dauerhaft speichern und in jeder Runde senden (verworfen: Größe der Vault und Kosten).
- **Grenzen**: lange Kante 1568 px, höchstens 5 MB nach dem Verkleinern (wie `chat/attachments.rs:23`).

## R3 Gemeinsamer Kern für Dateien des Geräts

- **Befund**: `extensions/fs/` hat Pfadauflösung (`resolve.rs`, lehnt kaputte Links und `..` ab), die
  Sperrliste der eigenen Orte von holzi (`FsEnvironment.denied`, `mod.rs:81-91`: kanonische
  `app_config_dir`, `app_data_dir`, `app_local_data_dir`, `app_cache_dir`, `app_log_dir`), bekannte Orte
  (`mod.rs:93-107`) und eine Beobachtung mit `notify-debouncer-full` (`watch.rs`, nur Desktop). Die
  Operationen in `ops.rs` hängen an `CallContext` und `BridgeError`.
- **Decision**: Die reinen Teile (Auflösen, Sperrliste, bekannte Orte, Beobachtung) ziehen nach
  `src-tauri/src/files/local/`; `extensions/fs` ruft sie über dünne Adapter auf. Die Operationen schreibt
  der Dateibrowser neu für seine Anforderungen (Transfers, Papierkorb, keine halben Dateien).
- **Rationale**: Eine Sperrliste für Erweiterungen (017 FR-049) und Agents (FR-033), ein Ort für die
  Pfadprüfung. Portabler Modus (014) ist noch nicht gebaut; er ändert später nur die Wurzeln der Liste.

## R4 Medien über einen lokalen HTTP-Server (FR-012, FR-013, FR-016)

- **Decision**: `files/media_server.rs` nach haex-vault `src-tauri/src/media_server/mod.rs` (eigener
  HTTP/1.1 auf `tokio::net::TcpStream`, 127.0.0.1, zufälliger Port, UUIDv4-Tokens, Range `bytes=N-M`,
  `N-` und `-N`, gestückelt mit flachem Speicher). Neu gegenüber haex-vault: `release(token)` und
  `release_tab(tab_id)`; Antwort auf `OPTIONS`; CORS-Kopfzeilen `Access-Control-Allow-Origin`,
  `Access-Control-Expose-Headers: Accept-Ranges, Content-Range, Content-Length` für pdf.js; der
  Accept-Loop endet mit `gate.token()`. Quellen hinter dem Trait `StreamingSource { size, read_range }`
  (haex-vault `remote_storage/streaming/source.rs`).
- **Rationale**: WebKitGTK spielt `<video>`/`<audio>` nicht aus eigenen Schemata, und `asset://` hat kein
  Range (haex-vault `docs/plans/2026-06-03-unified-media-playback-and-filebrowser-refactor-design.md`).
  Ein Prozess je Vault-Sitzung (ADR 0003) heißt: Sperren beendet den Prozess, alle Tokens verfallen.
- **CSP** (`tauri.conf.json`): `media-src 'self' http://127.0.0.1:*`, `http://127.0.0.1:*` in `img-src`
  und `connect-src`. Kein `useHttpsScheme` (bleibt `false`, sonst Mixed Content auf Windows/Android).
- **Android**: Release-Builds setzen `usesCleartextTraffic=false` (`gen/android/app/build.gradle.kts:21`).
  `res/xml/network_security_config.xml` mit `cleartextTrafficPermitted="true"` nur für `127.0.0.1` und
  `android:networkSecurityConfig` im Manifest.
- **Risiko** [Guessing]: „Local Network Access“ in Chromium 142+ könnte Anfragen von
  `http://tauri.localhost` an `127.0.0.1` in WebView2/Android WebView sperren. Erste Aufgabe der
  Umsetzung ist eine selbstprüfende Plattform-Probe in der CI auf Windows, macOS und Android (Feature
  `platform-probe`, tasks.md T007; Android im Emulator wie haex-vault mit
  `reactivecircus/android-emulator-runner`); Ausweichweg: WebView2-Argument, das LNA abschaltet.
- **Ergebnis der Plattform-Probe** (T007, PR #328, 2026-10-08): `fetch` aus der Webview an
  `127.0.0.1` gelingt unter Windows (WebView2, Edge 153, also nach Einführung von LNA in Chromium 142) und macOS (WKWebView). Im Android-Emulator (Android 15, WebView 124, Debug-APK mit erlaubtem
  Cleartext) scheitert er mit `TypeError: Failed to fetch`. LNA scheidet dort aus (WebView zu alt);
  offen sind CSP und Private Network Access. Bis das geklärt ist, ruht der Medienserver (T014 ff.).
  **Gelöst (2026-10-08, PR #332):** Mit `res/xml/network_security_config.xml` (Release: Cleartext nur
  für `127.0.0.1`; Debug: überall, für `tauri android dev`) und `android:networkSecurityConfig` im
  Manifest kommt der `fetch` durch; der Server sah ihn. Vorher kam keine Verbindung an, obwohl der
  Debug-Build `usesCleartextTraffic=true` setzte; die Webview richtet sich offenbar nach der
  Netzwerk-Konfiguration [Likely]. Weder CSP-Verstöße noch ein unsicherer Kontext (`isSecureContext`
  war `true`) spielten eine Rolle.
- **Alternatives**: eigenes URI-Schema (verworfen: WebKitGTK), `asset://` (verworfen: kein Range, Scope
  über das ganze Dateisystem), vorsignierte S3-URLs (verworfen: CSP für beliebige Endpunkte, Zugangsdaten
  indirekt in der Webview, Grundsatz aus 038).

## R5 S3-Client erweitern

- **Befund**: `RemoteStore` (`remote_storage/mod.rs:239-275`) kennt `put(body: Vec<u8>)`,
  `get(max_bytes)`, `list(prefix, max)` (flach, ohne Delimiter), `delete`; `s3.rs` puffert ganze Körper
  (`read_body`, 148). Kein Range, kein HEAD, kein Multipart, kein Kopieren.
- **Decision**: `RemoteStore` bekommt `head`, `get_range` (Range-Kopfzeile auf die vorsignierte URL),
  `get_to_writer` (gestreamt), `list_dir` (Delimiter `/`, liefert Präfixe und Objekte, seitenweise),
  Multipart (`create`, `upload_part`, `complete`, `abort`) und `copy` (serverseitig innerhalb einer
  Verbindung). Uploads unter 8 MiB gehen weiter als ein `put`. `FakeStore`
  (`remote_storage/test_support.rs`) lernt dieselben Methoden.
- **Rationale**: rusty-s3 0.10 signiert die nötigen Aktionen (HeadObject, GetObject,
  ListObjectsV2 mit Delimiter, Create/UploadPart/Complete/AbortMultipartUpload, CopyObject) [Likely,
  beim Bau gegen RustFS zu bestätigen]. DNS-Pinning und „keine Weiterleitungen“ aus 038 bleiben.
- **Zugriff**: holzi-intern wie Spec 038: `StorageService::access_of(storage_id)`, Zugangsdaten als
  `Caller::Internal { feature: "storage" }`; der Dateibrowser sieht sie nie (FR-038).

## R6 Suche (FR-027 bis FR-030)

- **Decision**: `walkdir` 2.5 mit `same_file_system(true)` und `follow_links(false)` auf einem
  Blocking-Thread; unscharfer Vergleich mit `frizbee` 0.13 (`max_typos` 1 bei bis zu 5 Zeichen, sonst
  2); Treffer gehen über einen `tauri::ipc::Channel` ans Fenster, je Suche ein `CancellationToken`.
  Für Agents dieselbe Suche mit Grenzen 30 s und 500 Treffer, danach `truncated: true`. Auf S3 geht die
  Suche Präfix für Präfix über `list_dir` und meldet Fortschritt.
- **Rationale**: `same_file_system` vergleicht unter Unix `st_dev`, unter Windows die
  Seriennummer des Volumes; ab `/` fallen damit `/proc`, `/sys`, `/dev`, `/run` von selbst weg. `frizbee`
  ist tippfehlertolerant (Smith-Waterman mit `max_typos`), ohne Pflicht-Abhängigkeiten, SIMD auch auf
  NEON.
- **Bekannte Grenze**: btrfs-Subvolumes haben eigene `st_dev`; auf Fedora fällt `/home` aus einer Suche
  ab `/` heraus [Likely]. Das deckt sich mit „bleibt auf seinem Laufwerk“ und steht in der Hilfe.
- **Alternatives**: `nucleo-matcher` (keine Tippfehler), `fuzzy-matcher` (nicht gepflegt), `strsim`
  (nur Abstände, keine Teilstrings), Fuse.js im Fenster (verworfen: Agents brauchen die Suche in Rust).

## R7 Vorschaubilder (FR-005)

- **Decision**: In Rust mit `image` 0.25 (`default-features = false`, Features `jpeg`, `png`, `webp`,
  `gif`) und `fast_image_resize` 6.1; EXIF-Drehung über `decoder.orientation()` und
  `apply_orientation`; `Limits` gegen riesige Bilder. Kante 320 px wie in 036, gespeichert als JPEG unter
  `<AppCache>/files-thumbnails/<sha256(Quelle, Pfad, Größe, Änderungszeit)>.jpg`; ein Merkzeichen für
  „beschädigt“ verhindert Wiederholung. Ausgeliefert als Bytes über IPC (`tauri::ipc::Response`, wie
  `passwords_attachment_preview`), im Fenster als Object-URL; die Warteschlange aus
  `src/lib/passwords/thumbnails.ts` (LRU, zwei gleichzeitig) wird geteilt.
- **Rationale**: Ordner mit tausenden Fotos würden im Fenster jedes Mal neu dekodiert (036 macht
  Vorschaubilder im Fenster und nur im Speicher). Der Cache liegt im Cache-Verzeichnis von holzi und ist
  damit für Agents gesperrt und nie synchronisiert (ADR 0001).
- **Probe**: `image` und `fast_image_resize` kompilieren für Android und Windows.

## R8 Text aus PDF und Office für Agents (FR-035a)

- **Decision**: PDF mit `pdf-extract` 0.12.1 (baut auf lopdf 0.42, entschlüsselt Dateien ohne Passwort
  selbst), jeder Aufruf in `catch_unwind` auf einem Blocking-Thread, weil fehlerhafte PDFs paniken
  können. „Keine Textschicht“: Seite fast ohne Text, keine Fonts, aber Bilder (Heuristik). xlsx/ods mit
  `calamine` 0.36 (`open_workbook_auto`, Blattnamen). docx/odt selbst aus ZIP und XML (`zip` 8,
  `quick-xml` 0.41, beide kommen mit calamine ohne Zusatzkosten): `w:t`/`w:p` bzw. `text:p`/`text:h`.
  Grenzen: Eingabe bis 50 MB, Ausgabe bis 200 000 Zeichen mit Kennzeichen „abgeschnitten“.
- **Probe**: `pdf-extract`, `calamine`, `zip` 8, `quick-xml` 0.41 kompilieren für Android und Windows,
  kein ring/aws-lc-rs im Baum.
- **Befund bei der Umsetzung**: pdf-extract 0.12 kennt die Textoperatoren `'` und `"` nicht (nur `Tj`,
  `TJ`, `T*`); Ghostscript schreibt damit jede Zeile nach der ersten. holzi schreibt sie vor dem Lesen
  mit lopdf in `T*` + `Tj` um.
- **Alternatives**: `lopdf` direkt (schlechtere Wortabstände), `oxidize-pdf` (viele Abhängigkeiten,
  jung), `mupdf` (C, AGPL), `dotext` (seit 2017 tot), `docx-rust` (mehr Abhängigkeiten).

## R9 Papierkorb (FR-023)

- **Decision**: `trash` 5.2 nur für `cfg(not(target_os = "android"))` (kompiliert auf Android nicht,
  Probe). Android und S3 löschen endgültig nach Bestätigung (Spec). Aufruf auf einem Blocking-Thread.
- **Bekannte Grenze**: Unter Linux kopiert `trash` bei schreibgeschütztem Wurzelverzeichnis eines anderen
  Laufwerks in den Papierkorb im Home und löscht dann; das kann bei großen Ordnern dauern. Der Transfer
  zeigt dafür Fortschritt nur als „läuft“.

## R10 Transfers ohne halbe Dateien (FR-018 bis FR-026)

- **Decision**: Ein `TransferManager` in `files/transfer/` mit Id, Zustand und Fortschritt je Transfer
  (Events über einen `Channel`), gestartet mit `gate.spawn`, abbrechbar über einen eigenen
  `CancellationToken`, der am `gate.token()` hängt. Lokal: Schreiben in `.<Name>.holzi-part-<uuid>`
  daneben, am Ende `rename`; S3: Multipart, bei Abbruch `abort`. Wiederholung bei S3-Netzfehlern bis
  dreimal mit 1 s, 2 s, 4 s. Freier Platz lokal vor dem Start über eine kleine eigene Funktion
  (`statvfs` unter Unix, `GetDiskFreeSpaceExW` unter Windows), keine neue Abhängigkeit.
- **Rationale**: Beim Schließen der Vault wartet der Gate-Drain auf oder bricht die getrackten Aufgaben
  ab; Teil-Dateien werden im Abbruchpfad gelöscht.

## R11 Android: Zugriff auf alle Dateien (FR-039)

- **Befund**: Das Plugin `plugins/holzi-android` ist leer (`COMMANDS: &[]`); das Manifest hat nur
  `INTERNET`. `minSdk = 26`, `targetSdk = 37`.
- **Decision**: Manifest `MANAGE_EXTERNAL_STORAGE`; für API 26–29 zusätzlich
  `READ_EXTERNAL_STORAGE`/`WRITE_EXTERNAL_STORAGE` (`maxSdkVersion="29"`) als Laufzeitberechtigung.
  Plugin-Befehle `all_files_access_status` (`Environment.isExternalStorageManager()` ab API 30) und
  `request_all_files_access` (Intent `ACTION_MANAGE_APP_ALL_FILES_ACCESS_PERMISSION` mit
  `package:`-URI, bei `ActivityNotFoundException` `ACTION_MANAGE_ALL_FILES_ACCESS_PERMISSION`); neu
  prüfen in `onResume` bzw. beim Fokus des Fensters. Danach echte Pfade unter `/storage/emulated/0` mit
  `std::fs`; `Android/data` anderer Apps bleibt unerreichbar.
- **Quelle**: developer.android.com/training/data-storage/manage-all-files (Berechtigung,
  `isExternalStorageManager()`, `ACTION_MANAGE_ALL_FILES_ACCESS_PERMISSION`, Ausnahme `Android/data`,
  Play-Prüfung) [Certain]; die app-spezifische Action mit `package:`-URI [Likely].
- **Beobachtung auf Android**: `notify` 8 unterstützt Android über inotify (`Cargo.toml` von notify
  8.2: `inotify` für `target_os = "android"`) [Certain]. holzi hat `notify` bisher nur für den Desktop
  eingebunden, weil Erweiterungen auf Mobilgeräten nach 017 nur Dialog-Pfade bekommen; das ist eine
  Produktentscheidung für Erweiterungen, keine Grenze der Plattform. Der Dateibrowser bindet `notify`
  deshalb auch für Android ein (nicht iOS) und beobachtet nur den offenen Ordner, nicht rekursiv; die
  Erweiterungen bleiben auf Mobilgeräten bei „nicht verfügbar“. Weil Ereignisse über FUSE unter
  `/storage/emulated/0` nicht in jedem Fall ankommen müssen [Likely], lädt der Dateibrowser beim
  Zurückkehren in den Tab zusätzlich neu.

## R12 PDF-Viewer im Fenster (FR-010)

- **Decision**: `pdfjs-dist` 6.4 (Apache-2.0), nur bei Bedarf geladen; Worker über
  `pdfjs-dist/build/pdf.worker.min.mjs?url`; `cmaps/` und `standard_fonts/` als statische Assets. Kein
  `'wasm-unsafe-eval'` in `script-src`: pdf.js nutzt dann seine Nicht-Wasm-Ersatzwege (JPX/JBIG2
  langsamer) [Likely]. Laden über die Freigabe-URL mit Range (`rangeChunkSize` 1 MiB); dafür braucht der
  Server die CORS-Kopfzeilen aus R4. Vorlage: haex-vault `src/components/haex/system/files/PdfViewer.vue`
  (dort `pdfjs-dist ^6.2`).

## R13 Einbindung ins Fenster

- **Decision**: App `system.files` in `src/lib/wm/apps.ts` (`multiInstance: true`, Icon
  `lucide:folder`), Orte in `src/lib/files/registry.ts`, Routen in `src/components/wm/appRoutes.ts`.
  Ort = Adresse des Tabs, damit die Sitzung (022) ihn wiederherstellt: `/device?p=<Pfad>` und
  `/storage/:id?p=<Präfix>`, offene Datei als `&open=<Name>`. Ansicht, Sortierung, versteckte Dateien als
  Gerätepräferenzen `files.view`, `files.sort`, `files.hidden` (`set_pref` mit Scope Gerät, ADR 0001).
  Virtuelle Liste mit `useVirtualList` aus `@vueuse/core`; Bildansicht mit photoswipe (wie 036);
  Kontextmenü mit `ShadcnContextMenu`; Drag & Drop aus dem System wie `Attachments.vue:108-140`
  (`onDragDropEvent`).
- **Hinweis**: Der Pfad steht im Klartext in `wm_sessions_no_sync` (gerätelokal). Das ist für den
  eigenen Rechner hinnehmbar und gleich wie bei anderen Apps.

## R14 Berechtigungen der Agents (FR-031 bis FR-031b)

- **Befund**: Die Berechtigungen der Erweiterungen hängen per Fremdschlüssel an `extensions`
  (`identity/migrations_extensions.rs:63-73`); externe Agents (021) gibt es im Code noch nicht.
- **Decision**: Neue synchronisierte Tabelle `agent_file_permissions` (siehe data-model.md) mit
  `agent_id` (`builtin` oder später die Id aus 021), Art `device` oder `storage`, Ziel (leer oder
  Storage-Id) und Status (`granted`, `readWrite`, `denied`). Der eingebaute Agent hat `device` ab Werk
  (keine Zeile = erteilt); Widerruf ist eine Zeile `denied`. Speicher ab Werk ohne Zugriff. Beim Entfernen
  eines Speichers löscht `remove_storage` auch diese Zeilen (wie `store.rs:232` für Erweiterungen).
  Die Rückfrage bei fehlender Speicher-Berechtigung läuft über eine Brücke Rust → Fenster mit Antwort
  (Muster `action_bridge`); ohne Fenster lehnt holzi ab.
- **Rationale**: Die Typen `Permission`/`evaluate` aus den Erweiterungen sind auf Pfad-Ziele und
  Fremdschlüssel zugeschnitten; eine kleine eigene Tabelle ist klarer als ein Umbau.

## Offene Punkte für die Umsetzung

- LNA-Probe auf Windows und Android vor dem übrigen Medien-Code (R4).
- rusty-s3-Unterstützung für Range und CopyObject gegen RustFS bestätigen (R5).
