# Research: holzi für Android

**Spec**: [spec.md](./spec.md) | **Plan**: [plan.md](./plan.md) | **Stand**: 2026-10-07

Versionen aus `src-tauri/Cargo.lock` und dem Tauri-Gerüst: tauri 2.12.1, tauri-build 2.7.1,
wry 0.57.0 (Fork `haexmas/wry@47037dd`), tao 0.37.1, tauri-plugin-dialog 2.8.1,
tauri-plugin-fs 2.6.0, rustls-platform-verifier 0.7.1 (`-android` 0.2.0), lettre 0.11.23,
reqwest 0.13.5, sysinfo 0.39.6, cpal 0.18.2; Tauri-CLI 2.12.0. Das Android-Gerüst von
Tauri 2.12 setzt `compileSdk`/`targetSdk` 37, `minSdk` 24, NDK 28.2.13676358, AGP 9.3.1. holzi hebt
`minSdk` auf 26 (Android 8.0): cpal nimmt das Mikrofon über AAudio auf und bindet `libaaudio`
fest ein, die das NDK erst ab API 26 hat; mit 24 scheitert der Link-Schritt (2026-10-07).

Ausgangslage (`cargo check --target aarch64-linux-android`, 2026-10-07): Von gut 960 Crates
bricht nur `lettre` (R2); in holzi brechen `FileDialogBuilder::blocking_pick_folder`
(`src/extensions/fs/dialogs.rs:79`) und `PathResolver::desktop_dir`
(`src/extensions/fs/mod.rs:97`). SQLCipher (vendored OpenSSL), haex-crdt (bindgen), mistralrs,
candle, cpal, portable-pty, iroh und nostr übersetzen. Gelinkt und gestartet wurde noch nichts.

## R1 — Android-Projekt im Repository und eine lokale Plugin-Crate

**Entscheidung**: `src-tauri/gen/android` wird mit `tauri android init` erzeugt und
eingecheckt; `.gitignore` ignoriert künftig nur noch `src-tauri/gen/schemas/` (die Dateien,
die das Gerüst selbst ausschließt, regelt dessen eigene `.gitignore`). Aller Kotlin-Code,
alle Berechtigungen im Manifest, die ProGuard-Regeln und die Gradle-Abhängigkeit für die
Zertifikatsprüfung liegen in einer **lokalen Tauri-Plugin-Crate**
`src-tauri/plugins/holzi-android/` (`tauri_plugin::Builder::new(..).android_path("android")`).
`MainActivity.kt` bleibt das Gerüst; ergänzt werden nur `configChanges` um
`density|fontScale|layoutDirection`, damit auch diese Änderungen die Activity nicht neu
erzeugen (FR-009).

**Begründung**: `tauri android init` überschreibt Gerüstdateien, die von `tauri-build`
erzeugten Teile (`tauri.settings.gradle`, `app/tauri.build.gradle.kts`) werden aus den
Plugin-Pfaden neu aufgebaut (`tauri-build-2.7.1/src/mobile.rs:147-195`). Ein Plugin-Modul
überlebt das Neuerzeugen und bleibt in Rust testbar; das Gerüst bleibt fast unverändert und
lässt sich bei einer neuen Tauri-Version neu erzeugen und vergleichen.

**Verworfen**: Kotlin direkt in `MainActivity` (geht beim Neuerzeugen verloren); ein
Community-Plugin für FLAG_SECURE (es gibt kein gepflegtes); `gen/android` nicht einchecken
(CI und Release müssten das Gerüst jedes Mal neu erzeugen und die Anpassungen nachziehen).

Die Plugin-Crate bietet (Verträge in [contracts/android-plugin.md](./contracts/android-plugin.md)):
Bildschirmschutz an/aus (R7), Ränder des Systems und Tastatur an die Seite melden (R6),
Berechtigungen Mikrofon und Benachrichtigungen (R12), Gerätename (R10), App beenden (R4),
Netzwechsel melden (R10).

## R2 — Zertifikatsprüfung auf Android (FR-024)

**Entscheidung**:

1. `rustls-platform-verifier` wird beim Start auf Android initialisiert, rein in Rust:
   `ndk_context::android_context()` liefert VM und Application-Kontext (tao 0.37.1 setzt sie in
   `onCreate`, `tao-0.37.1/src/platform_impl/android/ndk_glue.rs:411`); der Thread wird an die
   VM gehängt und `rustls_platform_verifier::android::init_with_env` aufgerufen. Neue
   android-only-Abhängigkeiten: `jni` 0.22, `ndk-context` 0.1. Die Kotlin-Seite
   (`org.rustls:rustls-platform-verifier`, Version wie `rustls-platform-verifier-android` im
   Lockfile, Maven-Repo
   `https://github.com/rustls/rustls-platform-verifier/raw/maven-archive/android-release-support/maven/`)
   und die Keep-Regel `-keep, includedescriptorclasses class org.rustls.platformverifier.** { *; }`
   stehen in der Gradle-Datei der Plugin-Crate (R1).
   Nachtrag 2026-10-14: Das Maven-Repo auf GitHub antwortete in CI mit HTTP 500 und ließ den
   Android-Build scheitern. `.aar` und `.pom` aus diesem Repo liegen seitdem unter
   `src-tauri/plugins/holzi-android/android/maven/`, und Gradle lädt sie von dort (ab 0.2.0
   enthält das Crate `rustls-platform-verifier-android` den Maven-Ordner nicht mehr).
2. `lettre` bekommt einen Patch nach dem Muster von reqwest: Ohne zusätzliche Wurzeln (und auf
   Android immer) `Verifier::new(provider)` statt `new_with_extra_roots`
   (`lettre-0.11.23/src/transport/smtp/client/tls.rs:540-551`; reqwest macht es in
   `async_impl/client.rs:758-780` schon so). Fork `haexmas/lettre`, eingebunden per
   `[patch.crates-io]` mit fester Revision, und ein Upstream-PR (ohne Agenten-Attribution); der
   Patch fällt weg, sobald lettre ihn veröffentlicht.

**Begründung**: Die Spec verlangt die vom System vertrauten Zertifizierungsstellen. Nur der
Plattform-Verifier fragt auf Android den `X509TrustManager` des Systems.
`rustls-native-certs` findet auf Android keine Zertifikate (`openssl-probe` sucht nur den
Termux-Pfad), `webpki-roots` ist die mitgelieferte Mozilla-Liste, nicht die des Systems. Ohne
Initialisierung bricht der erste TLS-Aufbau mit einer Panik ab
(„Expect rustls-platform-verifier to be initialized“).

**Betroffen**: reqwest (Anbieter, Modelldownloads, S3), `src/extensions/mail/connect.rs:77`
(IMAP), lettre (SMTP), Sync-Dienste über WebSocket (nostr). iroh benutzt eigene Schlüssel und
ist nicht betroffen.

**Verworfen**: lettre auf Android mit `webpki-roots` (verletzt FR-024, war nur der Umweg für
den ersten `cargo check`); Initialisierung per JNI-Export aus `MainActivity` (Kotlin im
Gerüst, R1); `webview.jni_handle().exec` (läuft erst, wenn eine Webview existiert; der
Verifier wird früher gebraucht).

## R3 — Gewählte Dateien statt Pfade (FR-015, FR-022, FR-002a)

**Entscheidung**: Ein Baustein `files::picked` in Rust nimmt eine **gewählte Datei** als
`tauri_plugin_fs::FilePath` entgegen (am Desktop ein Pfad, auf Android eine
`content://`-Adresse) und öffnet sie über `app.fs().open(file, options)`. Auf Android liefert
das Plugin über `getFileDescriptor` → `contentResolver.openAssetFileDescriptor` eine
`std::fs::File` (`tauri-plugin-fs-2.6.0/src/android.rs:37-80`), am Desktop öffnet es den Pfad.
Jeder Ablauf, der heute einen Pfad-String vom Frontend bekommt, nimmt künftig eine gewählte
Datei und liest oder schreibt über diesen Baustein, auf allen Plattformen gleich:

| Ablauf                                           | heute                                           |
| ------------------------------------------------ | ----------------------------------------------- |
| Anhänge im Passwortmanager                       | `components/passwords/Attachments.vue`          |
| Anhänge im Chat                                  | `components/chat/ComposerAttachments.vue:39`    |
| Import aus haex-vault (Spec 037)                 | `components/passwords/ImportWizard.vue:107`     |
| Darstellung/Hintergrund importieren, exportieren | `components/settings/AppearanceFileButtons.vue` |
| Erweiterung aus Datei installieren               | `components/extensions/InstallDialog.vue:74`    |
| Modell aus Datei importieren                     | `models/commands.rs:503`                        |
| Dateidialoge der Erweiterungen                   | `extensions/fs/dialogs.rs:71,79,92`             |
| Tresordatei übernehmen (neu, R8)                 | —                                               |

Speichern benutzt den Speicherdialog (`ACTION_CREATE_DOCUMENT`) und schreibt über denselben
Baustein (`wt`). Ordner wählen gibt es auf Android nicht
(`pick_folder` ist `#[cfg(desktop)]`, `tauri-plugin-dialog-2.8.1/src/lib.rs:618-760`): die
Stelle in `dialogs.rs` wird hinter `cfg(desktop)` gestellt und antwortet auf Android
`not_available` (FR-016). `desktop_dir` ebenso; die übrigen bekannten Ordner liefert Tauri
auf Android als app-eigene Ordner, die Filterung über `.ok()` in `extensions/fs/mod.rs:92-100`
bleibt.

**Begründung**: `FilePath::into_path()` scheitert für `content://` und ließ jede Auswahl wie
einen Abbruch aussehen (`dialogs.rs`). Ein Baustein für beide Plattformen statt
`cfg(android)`-Zweigen in jedem Ablauf; die Abläufe werden einmal umgestellt und am Desktop
von den vorhandenen e2e-Fällen weiter geprüft.

**Grenzen**: Der Zugriff gilt nur für die eine Auswahl (`ACTION_GET_CONTENT` gibt keine
dauerhafte Berechtigung). Nichts in dieser Spec braucht dauerhaften Zugriff; wenn eine spätere
Spec das braucht, kommt `ACTION_OPEN_DOCUMENT` mit `takePersistableUriPermission` in die
Plugin-Crate.

## R4 — Prozessmodell und Lebenszyklus (FR-005 bis FR-009)

**Entscheidung**:

- `vault_gate::close_policy()` (`vault_gate/mod.rs:51-67`) gibt auf Android immer `Exit`
  zurück. `request_end` ruft `app.exit(0)`; Tauri beendet darauf die Activity
  (`finishAffinity`, `tauri-2.12.1/src/app.rs:1457-1493`). Weil Android den Prozess danach im
  Cache behalten würde, ruft holzi in `RunEvent::Exit` auf Android `std::process::exit(0)`, damit
  der nächste Start wirklich die Tresorauswahl in einem neuen Prozess zeigt (Spec 013, ADR 0003).
  `force_end` endet auf Android mit `std::process::exit`. `tauri::process::restart` wird auf
  Android nie aufgerufen (es startet `current_binary`, `process.rs:74-88`).
- Wegwischen aus der App-Übersicht und Beenden durch Android laufen ohne `ExitRequested`. Das
  ist für die Daten unkritisch, weil jede Änderung beim Speichern geschrieben ist; die Sperren
  (`presence.lock`, `<name>.db.lock`) sind Dateisperren des Betriebssystems und fallen mit dem
  Prozess (FR-007). `ProcessPresence` räumt beim nächsten Start auf, weil der Prozess dann allein
  ist (`instances/presence.rs:297-321`).
- Zweiter Start: Das Gerüst setzt `launchMode="singleTask"`; Android holt die laufende App nach
  vorn (FR-008).
- Zurück-Geste: holzi meldet sich für `back-button` an (`src/plugins/actions.client.ts`), damit
  Tauri nicht `onBackPressed()` aufruft, was ab Android 12 die App in den Hintergrund schiebt.
  Das Verhalten selbst steht in Spec 020 FR-019; die Erkennung über den User-Agent wird durch
  die Plattformangabe (R9) ersetzt.

**Verworfen**: Neustart in derselben App nach dem Schließen (die Person hat sich für „Schließen
beendet die App“ entschieden); Neustart per Intent mit `FLAG_ACTIVITY_CLEAR_TASK` (unnötig,
wenn die App endet).

**Geprüft im Emulator (2026-10-07, API 35)**: `RunEvent::Exit` kommt nach `finishAffinity`; nach
`close_instance` ist der Prozess innerhalb von Sekunden weg, ein Neustart zeigt die Tresorauswahl,
der Tresor lässt sich wieder entsperren. Kein Beenden in `onDestroy` nötig. Auf einem echten Telefon
noch nicht geprüft.

**Fund dabei**: SELinux verbietet Apps auf Android harte Links (`avc: denied { link }`). holzi legte
die Installationskennung über eine temporäre Datei und `fs::hard_link` an, sodass auf Android kein
Tresor angelegt werden konnte. Sie wird jetzt mit `tempfile::persist_noclobber` veröffentlicht, das
auf Linux und Android `renameat2(RENAME_NOREPLACE)` benutzt (gleiche Garantie: erscheint nur
vollständig, überschreibt nie). Sonst benutzt holzi keine harten Links.

## R5 — Hintergrund und Rückkehr (FR-018, FR-019)

**Entscheidung**: holzi hört auf `RunEvent::Resumed` (Tauri meldet die Lebenszyklus-Ereignisse
der Activity) und stößt dann denselben Weg an, den der Sync nach einer verlorenen Verbindung
geht: Verbindungen zu Sync-Diensten neu aufbauen, verpasste Änderungen holen. Netzwechsel
meldet die Plugin-Crate über `ConnectivityManager.registerDefaultNetworkCallback`; holzi ruft
darauf `Endpoint::network_change()` von iroh und baut WebSocket-Verbindungen neu auf. Im
Hintergrund tut holzi nichts Eigenes; was Android dort anhält, bleibt angehalten (FR-018: kein
Hintergrund-Sync).

**Begründung**: Der Sync kann sich schon von abgebrochenen Verbindungen erholen (Spec 033,
Wiederverbindungstakt aus PR #235); Android braucht nur den Anstoß zur richtigen Zeit, statt auf
das nächste Zeitfenster zu warten (SC-005: 10 s).

**Geklärt (Stufe 2)**: iroh 1.3 erkennt Netzwechsel auf Android nicht selbst; die Dokumentation
von `Endpoint::network_change` sagt, Android gebe diese Information nur an Java-Code. Der Anstoß
aus der Plugin-Crate ist also nötig. Bei der Rückkehr in den Vordergrund baut holzi außerdem die
Nostr-Verbindungen neu auf, weil Android die Sockets einer Hintergrund-App schließen kann, ohne
dass der Client es sofort merkt.

## R6 — Ränder, Aussparung und Tastatur (FR-011, FR-012)

**Entscheidung**: Die Plugin-Crate setzt in `load(webView)` einen
`ViewCompat.setOnApplyWindowInsetsListener` auf die Webview: Systemleisten und Aussparung
werden als CSS-Variablen `--holzi-inset-top|right|bottom|left` (in CSS-Pixeln) auf
`document.documentElement` geschrieben, die Höhe der Tastatur als `--holzi-inset-keyboard`. Der
Window Manager (wm) rückt seinen Wurzelbereich um diese Werte ein; das Gerüst setzt
`enableEdgeToEdge()`. Am Desktop sind die Variablen nicht gesetzt und fallen auf `0px` zurück.
Ein fokussiertes Feld wird nach einer Änderung der Tastaturhöhe mit `scrollIntoView` in den
sichtbaren Bereich geholt.

**Begründung**: Ab `targetSdk` 35 ist Edge-to-Edge erzwungen, `adjustResize` verkleinert das
Fenster dann nicht mehr, und wry und tao werten keine Ränder aus. `env(safe-area-inset-*)`
liefert erst ab neueren WebView-Versionen richtige Werte (laut Capawesome ab etwa Chromium 140,
nicht unabhängig bestätigt); eigene Variablen hängen nicht von der WebView-Version des Telefons
ab.

## R7 — Bildschirmschutz (FR-011a)

**Entscheidung**: Gerätebezogene Einstellung `privacy.screenCaptureProtection` als
`preferences`-Zeile mit `PrefScope::Device` (ADR 0001; Muster `extensions/dev.rs:46-79`),
Standard „an“ (fehlender Wert = an). Nach dem Entsperren oder Anlegen eines Tresors und bei jeder
Änderung der Einstellung setzt holzi über die Plugin-Crate
`window.addFlags/clearFlags(FLAG_SECURE)` auf dem UI-Thread; auf Android 13+ zusätzlich
`setRecentsScreenshotEnabled(false)`. Vor dem Entsperren ist der Schutz aus (es gibt nichts zu
schützen; das Passwortfeld zeigt Punkte). Die Einstellung steht in der Kategorie „Allgemein“
der Einstellungen-App (Spec 023), nur auf Android sichtbar, mit dem Hinweis „gilt nur für dieses
Gerät“ (Muster `SttModelSetting.vue:112-116`), ohne Speichern-Knopf.

**e2e**: FLAG_SECURE schwärzt `adb screencap`; Bildschirmfotos über chromedriver
(`Page.captureScreenshot`) entstehen im Compositor von Chromium und sind vermutlich nicht
betroffen (unbestätigt). Die Android-Plattform der e2e-Suite schaltet die Einstellung beim
Anlegen jedes Geräts aus; ein eigener Android-Fall prüft, dass sie an ist und sich ausschalten
lässt.

## R8 — Tresordatei übernehmen (FR-002a, Desktop und Android)

**Entscheidung**: `instances/import.rs` mit dem Command `import_instance(file, passphrase)`:

1. Namen aus dem Dateinamen bilden (`validate_instance_name` erlaubt
   `^[A-Za-z0-9][A-Za-z0-9_-]{0,63}$`, `paths.rs:325`): Endung weg, andere Zeichen zu `-`; bei
   einem belegten Namen `-2`, `-3`, … anhängen.
2. Wie beim Anlegen zuerst den `.pending`-Marker atomar anlegen (`create.rs:86-95`), dann die
   gewählte Datei über R3 nach `<name>.db` kopieren (Muster `models/import.rs:18`
   `copy_into_managed`: erst in eine `.tmp`, dann umbenennen).
3. Mit `open_existing_database` entsperren (eine Variante von `open_instance_core`, die den
   eigenen Marker zulässt, `open.rs:77`).
4. Prüfen, ob es die Vault-Identität schon in einem anderen Tresor dieses Geräts gibt; dann
   ablehnen („Diesen Tresor gibt es auf diesem Gerät schon“). Grund: dieselbe Installation fände
   in der Kopie ihre eigene `known_devices`-Zeile und ihre Schlüssel wieder
   (`identity/bootstrap.rs:138-201`, `sync/keys.rs:130-157`); zwei Dateien mit derselben
   Gerätekennung würden sich beim Sync als Doppel ausgeben (`sync/handshake.rs:353`).
5. Erfolg: Marker entfernen, als aktiven Tresor veröffentlichen (wie `create`),
   `instance-list-changed` senden. Fehlschlag: `.db`, `.db.pending`, `.db.lock`, `.db-wal`,
   `.db-shm` und die `.tmp` entfernen; die Originaldatei wird nur gelesen.

Eine Kopie auf einer **anderen** Installation ist der Fall aus Spec 024 (US7, FR-006): neue
Gerätekennung, neue Schlüssel, die Vault-Identität bleibt; die Kopie eines Hauptgeräts wird
Hauptgerät, die eines verknüpften Geräts wartet auf Aufnahme (`sync/genesis.rs:53-172`). Daran
ändert sich nichts; e2e `sync-copy` deckt es ab.

Oberfläche: In der Tresorauswahl (`src/pages/index.vue:94-125`) kommt neben „Neuer Tresor“ und
„Verknüpfen“ ein schwächer gewichteter Knopf „Tresordatei öffnen“; er öffnet die Dateiauswahl
und danach eine Passwort-Abfrage nach dem Muster von `OnboardingUnlockSheet` (FR-002:
Anlegen bleibt hervorgehoben).

**Verworfen**: Tresor an Ort und Stelle öffnen (von der Person verworfen; SQLite braucht
`-wal`/`-shm` neben der Datei, was `content://` nicht hergibt); Rest-Bereinigung erst beim
nächsten Start (`startup.rs:36-78` räumt nur `.pending` mit `.db` auf, nicht `.lock`, `-wal`,
`-shm`).

**Nebenbefund**: Der Rückbau beim Anlegen (`create.rs:107-130`) lässt `.db.lock` liegen. Der
Rückbau von `import` und `create` benutzt denselben Helfer, der alle Begleitdateien entfernt.

## R9 — Plattform und „nicht verfügbar“ (FR-016, FR-026)

**Entscheidung**: Ein Modul `platform` in Rust beschreibt, was dieses Gerät kann, als feste
Tabelle pro Zielplattform (`cfg(desktop)`/`cfg(mobile)`), und gibt sie über den Command
`platform_capabilities` an die Oberfläche (ts-rs-Typ). Eine Regel, eine Stelle
([contracts/platform-capabilities.md](./contracts/platform-capabilities.md)):

| Fähigkeit         | Android | Wirkung, wenn nicht verfügbar                                                                   |
| ----------------- | ------- | ----------------------------------------------------------------------------------------------- |
| `cliDelegates`    | nein    | Delegates claude/codex fehlen in Modellwahl und Einstellungen; Aufruf liefert „nicht verfügbar“ |
| `commandTool`     | nein    | Das Werkzeug `run_command` (`chat/tools/cli.rs`) wird nicht angemeldet                          |
| `terminal`        | nein    | wie heute `extensions/shell/mod.rs:198-214`                                                     |
| `folderWatch`     | nein    | wie heute `extensions/fs/watch.rs`                                                              |
| `freePaths`       | nein    | wie heute `extensions/fs/mod.rs:114`                                                            |
| `folderPick`      | nein    | Ordnerauswahl antwortet „nicht verfügbar“ (R3)                                                  |
| `gpuDetection`    | nein    | keine Grafikkarten-Erkennung; Empfehlungen nur nach Arbeitsspeicher                             |
| `screenCapture`   | ja      | Einstellung aus R7 sichtbar                                                                     |
| `relaunchOnClose` | nein    | R4                                                                                              |

Die vorhandenen Stellen (`desktop_only()`, `free_paths`, `watch.rs`) lesen künftig diese
Tabelle statt eigener `cfg!`-Abfragen. Delegates: Der Aufruf scheitert heute erst beim Start des
Programms als „nicht installiert“ (`adapters/cli_delegate/process.rs:15-31`); auf Android prüft
die Delegate-Schicht vorher die Tabelle. Per Sync übernommene Delegate-Einstellungen bleiben
gespeichert und werden nur nicht angeboten (FR-026). `run_command` würde auf Android
tatsächlich laufen (`/system/bin/sh` in der Sandbox der App), hat dort aber nichts Sinnvolles
zu tun und wird deshalb nicht angemeldet. Die Erkennung von Android im Frontend über den
User-Agent (`src/plugins/actions.client.ts:29`) wird durch die Tabelle ersetzt.

## R10 — Gerätename und Kopplung (FR-017)

**Entscheidung**: `hardware/hostname.rs` liest auf Android den Gerätenamen über die
Plugin-Crate (`Settings.Global.DEVICE_NAME`, sonst `Build.MODEL`), weil sysinfo dort meist
„localhost“ liefert. Der Name wird beim ersten Öffnen eines Tresors als Alias des Geräts
übernommen (`admission::adopt_computer_name`). Sonst ist die Kopplung dieselbe wie zwischen zwei
Desktops (Spec 024); die e2e-Fälle `sync-link`, `sync-two-devices` usw. laufen mit dem Gerät
`phone` im Emulator (R14).

## R11 — Lokale KI und Spracheingabe (FR-027 bis FR-030)

**Entscheidung**: Auf Android läuft **derselbe Lader wie am Desktop** (mistralrs 0.8.1 mit
GGUF, CPU) und dieselbe Spracherkennung (candle Whisper, `stt/local.rs`), mit Feature
`llm-cpu` und `voice` wie am Desktop. Ein neues ADR 0010 ersetzt den Teil von ADR 0002, der für
Mobilgeräte ein natives MLC-Backend vorsieht; die Modellfamilie und die Voreinstellungen
(Qwen3-1.7B, Qwen3-0.6B bei wenig Speicher, jeweils Q4_K_M) bleiben.

- Vorschlag: Der Katalog liest `_meta.profiles` aus `catalog/model_catalog.json` (heute von
  keinem Code gelesen) und wählt auf Mobilgeräten nur unter `mobile` und `mobile_low_memory`;
  die Passung nach Arbeitsspeicher (`hardware/fit.rs:54-79`, sysinfo liest auf Android
  `/proc/meminfo`) entscheidet zwischen beiden.
- Spracherkennung: Auf Mobilgeräten ist `whisper-tiny` der Vorschlag (FR-030); größere bleiben
  wählbar.
- Downloadgröße und freier Speicher: Ein gemeinsamer Bestätigungsschritt vor jedem
  Modelldownload, auf allen Plattformen (eine Regel, eine Stelle): Größe, freier Speicher im
  Datenordner (`sysinfo::Disks` bzw. `statvfs`), Warnung, wenn es nicht reicht. Heute zeigen nur
  `SttModelChoiceStep.vue:63` und der HuggingFace-Picker die Größe; `DownloadModels.vue`,
  `ModelChoiceStep.vue` und `SttModelSetting.vue` laden ohne Bestätigung.
- Mikrofon: Vor der ersten Aufnahme fragt holzi über die Plugin-Crate `RECORD_AUDIO` an (R12);
  cpal nimmt über AAudio auf.

**Begründung**: mistralrs, candle und cpal übersetzen für `aarch64-linux-android`; ein zweites
Backend wäre eine zweite Umsetzung derselben Regel (ein Lader, ein Tokenizer, ein
Werkzeugformat). Ein natives MLC-Backend bliebe als spätere Option, wenn die Messung auf dem
Telefon (SC-009) zu langsam ist.

**Risiko**: Nur CPU, keine GPU auf dem Telefon. Qwen3-1.7B Q4_K_M braucht etwa 1,3 GB
Arbeitsspeicher; auf einem aktuellen Mittelklasse-Telefon sind einige Token pro Sekunde zu
erwarten. SC-009 (Antwortbeginn in 15 s) wird in Stufe 5 gemessen; reicht es nicht, wird
zuerst die 0.6B-Voreinstellung vorgeschlagen und das Ergebnis im ADR festgehalten.

## R12 — Berechtigungen (FR-023, FR-029)

**Entscheidung**: Die Plugin-Crate erklärt `RECORD_AUDIO` (Alias `microphone`) in ihrem Manifest
und bietet die Standard-Commands `checkPermissions`/`requestPermissions` der
Tauri-Plugin-Basis. Benachrichtigungen: Der Fork von `tauri-plugin-notification`
(`haexmas/plugins-workspace@b38883e`) erklärt `POST_NOTIFICATIONS` schon; holzi ruft
`request_permission()` beim ersten Versuch einer Erweiterung, eine Benachrichtigung zu senden.
Abgelehnt: die Nachricht erscheint in holzi (vorhandene Anzeige aus Spec 017), und die
Einstellungen nennen den Weg zur Android-Einstellung.

**Umsetzung (Stufe 3)**: Eine Anzeige von Erweiterungs-Benachrichtigungen in holzi gab es aus
Spec 017 nicht. Bei abgelehnter Berechtigung zeigt holzi die Nachricht jetzt als Meldung im
Fenster (Titel, Text, „Öffnen“ bringt den Tab der Erweiterung nach vorn). Gefragt wird beim ersten
Senden einer Erweiterung, nicht beim Start. Der Hinweis auf die Android-Einstellung in holzis
Einstellungen fehlt noch.

## R13 — Entwicklungsmodus auf dem Telefon (FR-034)

**Entscheidung**: `nuxt.config.ts` liest `TAURI_DEV_HOST` (heute fälschlich `TAURI_ENV_HOST`,
das es nicht gibt): `devServer.host` wird `process.env.TAURI_DEV_HOST || 'localhost'`, die
eigene HMR-Port-Angabe (1421, aus dem Vite-Gerüst) fällt weg. `devCsp` in `tauri.conf.json`
erlaubt zusätzlich die Adresse des Entwicklungsrechners nur im Entwicklungsmodus (oder
`tauri android dev` läuft mit `--host` und `adb reverse`, dann bleibt `localhost`). Im Emulator
geht es heute schon (`adb reverse tcp:3030`), auf einem echten Telefon nicht.
`pnpm tauri:android:dev` in `package.json` startet es in der devShell.

## R14 — e2e-Suite auf Android (FR-033a bis FR-033c, SC-007)

**Entscheidung**: **chromedriver direkt**, ohne Appium. Die Suite spricht schon W3C
(`scripts/e2e/lib/webdriver.ts`); nur `newSession` bekommt Fähigkeiten pro Plattform:
`goog:chromeOptions: { androidPackage: 'com.haex.holzi', androidDeviceSerial,
androidUseRunningApp: true }` (`androidUseRunningApp` ist Pflicht, sonst löscht chromedriver
die App-Daten). Debug-Builds schalten das WebView-Debugging ein
(`wry-0.57.0/src/android/main_pipe.rs`, `setWebContentsDebuggingEnabled` unter
`debug_assertions`). chromedriver passend zur WebView-Version des Emulators
(`dumpsys package com.google.android.webview`, Chrome for Testing).

Neue Plattform `scripts/e2e/lib/platform/android.ts` nach `PLATFORMS.md`:

- `start`: `am start -W`, auf den DevTools-Socket warten, Sitzung anhängen. `stop`: Home, dann
  `am kill`. `kill`: `am force-stop`. `alive`: `adb shell pidof` (synchron). `dispose`:
  `pm clear`. `copyVaultFile`/`keep`: `run-as com.haex.holzi` über `adb exec-in/exec-out`.
- Dienste auf dem Testrechner (Nostr-Testrelay, RustFS, Anbieter-Attrappe, Dev-Server der
  Erweiterungen) per `adb reverse tcp:P tcp:P`; die Adressen bleiben `127.0.0.1` auf allen
  Geräten, was für synchronisierte Relay-Listen und die Regel „Klartext-Mail nur über Loopback“
  wichtig ist. iroh: ein lokales `iroh-relay --dev` (HTTP, Port 3340, per `adb reverse`), weil
  der Emulator mit `10.0.2.15` für andere Geräte nicht erreichbar ist und UDP nicht durch
  `adb reverse` geht.
- Gemischte Gruppen: Das Gerät `phone` einer Gruppe läuft im Emulator, die übrigen als
  Linux-Prozesse (17 von 18 Mehrgeräte-Fällen haben ein `phone`; `sync-indirect` nimmt das erste
  Gerät).
- Änderungen in `lib/` (keine Änderung an Szenarien): `ctx.startInstance` läuft künftig über
  `DeviceHost`; `navigate` übersetzt `tauri://localhost` in `http://tauri.localhost` und
  `holzi-ext://localhost` in `http://holzi-ext.localhost`; Dateien für `install`,
  `importExport`, `addAttachments` werden per `run-as` in die Sandbox gelegt und mit Gerätepfad
  übergeben; der Emulator bekommt `wm size 2560x1600` und `wm density 320` (etwa 1280×800 dp wie
  Xvfb), damit Desktop-Fälle dieselbe Darstellung sehen; Telefon-Breite prüfen eigene
  Android-Fälle.
- `cli.ts` bekommt `--platform android` und `--shard i/n` (Verteilung nach gemessener Dauer).

**Einordnung der 60 Szenarien**:

- **Laufen auf Android (57)**: alle bis auf die drei unten; davon brauchen `closing-page`,
  `settings-color-scheme` (Farbschema über `cmd uimode night`), `lock-twice` (Prozesszählung
  über `pidof`) und `window-close-while-streaming` (Fenster schließen = Aufgabe aus der
  App-Übersicht entfernen) eine Android-Abbildung in `lib/`.
- **Ausgenommen (3)**, Liste mit Begründung in `scripts/e2e/platform-exclusions.ts`:
  `extension-files` (freie Pfade, Ordner beobachten; FR-016; Gegenfall: Aufruf mit Pfad antwortet
  „nicht verfügbar“), `extension-dev-mode` (lädt aus einem Ordner; FR-016; Gegenfall:
  Entwicklungsmodus der Erweiterungen ist ausgeblendet und der Aufruf „nicht verfügbar“),
  `relaunch-after-lock` (Neustart nach dem Schließen gibt es nicht; FR-006; Gegenfall:
  `lock-twice` auf Android, die App endet).
- **Abdeckung**: 57 von 60 = 95 % (SC-007: mindestens 80 %, Ziel 90 %).
- **Neue Android-Fälle**: Telefonbreite 360 px und Ränder (FR-010, FR-011), Zurück-Geste über
  `KEYCODE_BACK` (FR-013), Bildschirmschutz (FR-011a), „nicht verfügbar“ für die Ausnahmen,
  Tresordatei übernehmen (FR-002a; Datei per `run-as` bereitgestellt, Auswahl über einen
  Test-Seam statt der System-Dateiauswahl, die chromedriver nicht bedienen kann).

**Während des Aufbaus**: Die Ausnahmeliste kennt zwei Arten: `excluded` (endgültig, mit
Begründung nach FR-033b) und `pending` (läuft auf Android noch nicht, mit der Stufe, die es
behebt). Ab Stufe 1b ist der Android-Lauf Pflicht-Check; `pending` muss mit der letzten Stufe
leer sein. `pnpm check:e2e-exclusions` prüft, dass jeder Eintrag eine Begründung bzw. eine Stufe
hat und dass kein Szenario fehlt.

**Verworfen**: Appium 2 mit UiAutomator2 (zusätzlicher Server, Server-APK, Kontextwechsel) —
erst nötig, wenn native Teile wie die System-Dateiauswahl oder Benachrichtigungen automatisch
geprüft werden sollen; das bleibt Handprüfung im Quickstart. `10.0.2.2` statt `adb reverse`
(andere Adressen auf Telefon und Desktop in synchronisierten Einstellungen).

## R15 — CI (FR-031 bis FR-033c, SC-007)

**Entscheidung**:

- **Kein Nix in der CI** (wie die vorhandenen Jobs): `actions/setup-java` (17), das SDK des
  Runners plus `sdkmanager` mit festen Versionen aus derselben Liste wie die devShell
  (`platforms;android-36`, `platforms;android-37.0`, `build-tools;36.0.0`, `build-tools;37.0.0`,
  `ndk;28.2.13676358`), `dtolnay/rust-toolchain` 1.98.1 mit den Android-Zielen,
  `Swatinem/rust-cache` mit eigenem Schlüssel `android`, `gradle/actions/setup-gradle`. Ein
  Prüfskript `pnpm check:android-versions` vergleicht die Versionen im Workflow mit
  `.devshell/packages.nix` und `.devshell/rust-toolchain.toml`.
- **Job `android-build`**: `pnpm tauri android build --debug --apk --split-per-abi --target aarch64
--target x86_64` (ein APK je Architektur; die Debug-Bibliothek wird gestrippt, sonst wäre sie
  1,9 GB statt etwa 210 MB je Architektur); lädt die APKs als Artefakt hoch (FR-032). Bricht der
  Bau, ist der PR rot (FR-031). Ein zweiter Job `android-lint` lässt clippy für Android mit
  beiden Feature-Sätzen laufen.
- **Jobs `android-e2e` (3 Shards)**: KVM über die udev-Regel freischalten,
  `reactivecircus/android-emulator-runner@v2` (API 35, `google_apis`, x86_64, Snapshot im
  Cache, ein Skript statt mehrzeiligem `script:`), APK aus `android-build`, Linux-App für die
  gemischten Gruppen mit warmem Cache bauen, `pnpm test:e2e --platform android --shard i/3`.
- **Job `android`** als einziger Pflicht-Check fasst Bau und Shards zusammen (Branch-Schutz
  von `main`: Eintrag ergänzen; das macht die Person, die das Repo betreut).
- **Dauer**: Desktop-e2e heute 15,8 min am Stück; im Emulator 1,5- bis 2,5-mal langsamer
  geschätzt, auf 3 Shards etwa 25 bis 30 min mit warmem Cache. Der erste Lauf ohne Cache dauert
  länger; die Caches werden auf `main` gefüllt. Grenze des Cache-Speichers (10 GB) beachten.
- **Release** (`.github/workflows/android-release.yml`, bei Tags `v*`):
  `pnpm tauri android build --apk --target aarch64 --target x86_64` mit
  `gen/android/keystore.properties`, das der Workflow aus den Geheimnissen
  `ANDROID_KEY_BASE64`, `ANDROID_KEY_ALIAS`, `ANDROID_KEY_PASSWORD` schreibt (Muster der
  Tauri-Doku); `build.gradle.kts` liest sie nur, wenn die Datei existiert, sonst bleibt der
  Release unsigniert und der Workflow schlägt fehl. `versionCode` aus der Version in
  `tauri.conf.json` (Tauri-Standard), damit Updates sich installieren lassen (FR-033, SC-008).

**Verworfen**: `nix develop` in der CI (kein vorhandener Job benutzt Nix; ohne Binär-Cache
dauert das SDK-Laden bei jedem Lauf lange); ARM-Emulator (zu langsam ohne Hardware).

## R16 — Erweiterungen auf Android (FR-020 bis FR-023)

- Der wry-Fork ist schon im Einsatz (`[patch.crates-io]`, T013/T014 aus Spec 017). Nachweis
  auf Android: `extension-isolation` im Emulator (R14) prüft, dass kein Rahmen
  `__TAURI_INTERNALS__` oder den Aufrufschlüssel sieht. Damit ist Spec 017 T112 für Android
  erledigt (iOS bleibt offen).
- `extensions/protocol/csp.rs` schreibt `tauri://localhost` fest; auf Android heißt der Ursprung
  `http://tauri.localhost` (wie heute schon unter Windows, `protocol/mod.rs:17`). Die Regel wird
  aus einer Quelle pro Plattform gebildet.
- Dateizugriff der Erweiterungen über R3: Die Dateidialoge geben der Erweiterung den Inhalt der
  gewählten Datei bzw. schreiben über die gewählte Adresse; die Erweiterung sieht nie einen Pfad.
- Benachrichtigungen: R12.

## R17 — ADR

ADR 0010 „Android: derselbe Kern, eine Plugin-Crate für die Plattform“ hält fest: gleicher Lader
für lokale Modelle auf Android (ersetzt den MLC-Teil von ADR 0002), Zertifikatsprüfung über den
Plattform-Verifier mit lettre-Patch, eine lokale Plugin-Crate für alles Plattformnahe,
eingechecktes `gen/android`, Schließen beendet die App (ADR 0003 gilt unverändert). Geschrieben als
[`docs/adr/0010-android-platform.md`](../../docs/adr/0010-android-platform.md); ADR 0002 trägt den
Hinweis „superseded in part“.

## R18 — Temporäre Dateien auf Android (gefunden in Stufe 1c)

**Befund**: `std::env::temp_dir()` fällt auf Android ohne `TMPDIR` auf `/data/local/tmp` zurück,
und dort darf eine App nicht schreiben. Das traf jede Stelle mit `tempfile::tempdir()` (etwa den
Import aus haex-vault, der die Vault-Datei vor dem Öffnen kopiert) und SQLite selbst, das seine
Zwischendateien ebenfalls über `TMPDIR` ablegt.

**Entscheidung**: `MainActivity.onCreate` setzt `TMPDIR` auf den Cache-Ordner der App
(`Os.setenv`), bevor `super.onCreate` die Rust-Seite startet. Zu diesem Zeitpunkt läuft noch kein
Rust-Thread, der die Umgebung liest; ein `set_var` im `setup` des Kerns wäre nicht mehr
threadsicher.

**Verworfen**: jedem `tempfile`-Aufruf einen Ordner mitgeben (übersieht SQLite und Abhängigkeiten).
