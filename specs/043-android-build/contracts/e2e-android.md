# Contract: e2e-Suite auf Android

**Anforderungen**: FR-033a, FR-033b, FR-033c, SC-007, FR-021 | **Research**: R14

## Aufruf

```text
pnpm test:e2e --platform android [--shard i/n] [--grep <muster>]
```

- `--platform linux` bleibt der Standard; die Szenarien ändern sich nicht.
- `--shard i/n` verteilt nach der zuletzt gemessenen Dauer (`artifacts/e2e-durations.json`,
  Fallback: alphabetisch reihum) und gilt für beide Plattformen.
- Voraussetzungen (Preflight): `adb` mit genau einem verbundenen Emulator oder
  `ANDROID_SERIAL`, installiertes Debug-APK (`com.haex.holzi`, debuggable), `chromedriver`
  passend zur WebView-Hauptversion, `iroh-relay`. Fehlt etwas, bricht der Preflight mit dem
  Namen des Fehlenden ab.

## Plattform `lib/platform/android.ts` (implementiert `DeviceHost`)

| Operation         | Umsetzung                                                                                                |
| ----------------- | -------------------------------------------------------------------------------------------------------- |
| `newData`         | `pm clear com.haex.holzi`; Bildschirmschutz aus (Geräte-Einstellung nach dem ersten Entsperren)          |
| `copyVaultFile`   | Quelle gestoppt; `adb exec-out run-as … tar c` bzw. Linux-Datei → `adb exec-in run-as … sh -c 'cat > …'` |
| `keep`            | App-Daten per `run-as` als Tar in die Fehlerunterlagen                                                   |
| `dispose`         | `pm clear`                                                                                               |
| `start`           | `am start -W -n com.haex.holzi/.MainActivity`, warten auf `webview_devtools_remote_<pid>`, Sitzung       |
| `stop`            | `input keyevent KEYCODE_HOME`, `am kill`, sonst `am force-stop`                                          |
| `kill`            | `am force-stop`                                                                                          |
| `alive`           | `adb shell pidof com.haex.holzi` (synchron)                                                              |
| `screenshot`      | chromedriver `GET /screenshot`                                                                           |
| Fenster schließen | Aufgabe entfernen (`am stack remove` bzw. `cmd activity` je nach API), Ende innerhalb der Frist prüfen   |

Sitzung: `POST /session` an chromedriver mit
`{ "goog:chromeOptions": { "androidPackage": "com.haex.holzi", "androidDeviceSerial": <serial>,
"androidUseRunningApp": true } }`. URLs: `tauri://localhost` → `http://tauri.localhost`,
`holzi-ext://localhost` → `http://holzi-ext.localhost`.

Dienste: Alle Geräte benutzen `127.0.0.1:P` für einen Dienst auf dem Testrechner. Auf einem
Emulator mit `su` (Google-APIs-Image) leitet seine Firewall `127.0.0.1:P` über das
Emulator-Netz an `10.0.2.2:P` weiter (eigene NAT-Ketten `holzi_e2e_out`/`holzi_e2e_post`, im
Preflight eingerichtet, `E2E_ANDROID_REDIRECT=1`); `adb reverse` trug den Sync-Verkehr über
dieselbe adb-Verbindung wie chromedriver, und ein großer Sync ließ adb den Emulator verlieren
(„device offline“). Vor den Szenarien prüft der Lauf bis 30 s, ob das Gerät einen Wegwerf-Server
des Testrechners so erreicht; kommt das Emulator-Netz nicht hoch, nimmt er `adb reverse` und sagt
es. Ein Telefon ohne `su` bekommt `adb reverse tcp:P tcp:P`. adbd bleibt der
shell-Benutzer (kein `adb root`: gepushte Dateien gehörten sonst root, die App könnte sie nicht
öffnen). Bricht adb kurz ab, warten `am force-stop` und `pm clear` beim Aufräumen bis 30 s auf
das Gerät und versuchen es noch einmal. iroh-Relays in Android-Läufen: `http://127.0.0.1:3340`
(`iroh-relay --dev`); Linux-Läufe bleiben beim geschlossenen Port.

Darstellung: `wm size 2560x1600`, `wm density 320` für Desktop-Szenarien; Android-Fälle für
die Telefonbreite setzen `wm size 1080x2400`, `wm density 480` (360 dp) und setzen danach zurück.

Gemischte Gruppen: Gerät `phone` → Emulator, alle anderen → Linux; ohne `phone` das erste
Gerät. Höchstens ein Android-Gerät pro Gruppe und Shard.

## Ausnahmeliste `scripts/e2e/platform-exclusions.ts`

Form in [data-model.md](../data-model.md#ausnahmeliste-der-e2e-suite-fr-033b-sc-007-vertrag-e2e-androidmd).
Anfangsstand (endgültig):

| Szenario                     | Grund                                                                   | Gegenfall                         |
| ---------------------------- | ----------------------------------------------------------------------- | --------------------------------- |
| `extension-files`            | freie Pfade und Ordner beobachten gibt es nicht (FR-016)                | `android-not-available`           |
| `extension-dev-mode`         | lädt eine Erweiterung aus einem Ordner (FR-016)                         | `android-not-available`           |
| `relaunch-after-lock`        | Schließen beendet die App, kein Neustart (FR-006)                       | `lock-twice` (Android: App endet) |
| `extension-encrypted-device` | nur über Dateizugriff auf den Ort, keine freien Pfade (FR-016, 048 US7) | `android-not-available`           |

`pending`-Einträge entstehen in Stufe 1b für alles, was erst eine spätere Stufe zum Laufen
bringt, jeweils mit Stufe. `pnpm check:e2e-exclusions` schlägt fehl, wenn ein Eintrag ohne
Begründung/Gegenfall/Stufe ist, ein Szenario nicht existiert, die Abdeckung unter 80 % fällt
oder (ab Stufe 5, Schalter in der Datei) `pending` nicht leer ist.

## Neue Android-Fälle (`scenarios/android-*.test.ts`)

| Fall                     | prüft                                                                                                   |
| ------------------------ | ------------------------------------------------------------------------------------------------------- |
| `android-phone-layout`   | 360 dp: kein waagrechtes Scrollen, Bedienelemente innerhalb der Ränder (FR-010, FR-011)                 |
| `android-back-gesture`   | `KEYCODE_BACK`: Tab zurück, Fensterübersicht, schließen, App bleibt (FR-013)                            |
| `android-screen-capture` | Einstellung standardmäßig an, ausschaltbar, Wert pro Gerät (FR-011a)                                    |
| `android-not-available`  | Ordnerauswahl, Terminal, Ordner beobachten, freie Pfade, Entwicklungsmodus → „nicht verfügbar“ (FR-016) |
| `android-close-ends-app` | Tresor schließen → Prozess weg; Neustart → Tresorauswahl (FR-006)                                       |
| `android-process-death`  | `am kill` im Hintergrund → Neustart ohne „anderswo geöffnet“ (FR-007)                                   |
| `vault-file-import`      | Desktop und Android: Tresordatei übernehmen (FR-002a)                                                   |

Diese Fälle laufen nur auf Android (außer `vault-file-import`); sie stehen nicht in der
Abdeckungsrechnung der Desktop-Fälle.

## Umsetzung in Stufe 1c

- Android-Fälle stehen unter `needs: { phone: true }` und werden ohne Telefon übersprungen. Was nur
  ein Telefon hat (Zurück-Geste, Prozessende im Hintergrund, Fensterflags), liefert die
  Plattformschicht als `instance.phone`; die Szenarien nennen so keine Plattform (Seam-Prüfung).
- Telefonbreite: `startInstance({ phoneScreen: true })` setzt `wm size 1080x2400` und
  `wm density 480` vor dem Start. Eine Änderung der Dichte zur Laufzeit übernimmt die WebView nicht
  (sie meldete 540 statt 360 CSS-Pixel).
- Fenster schließen: `cmd activity stack remove <taskId>` (wie Wegwischen aus den letzten Apps).
- Navigation: `location.replace` statt einer Navigation des Treibers. Die WebView von Android legt
  für eine Treiber-Navigation einen Verlaufseintrag an, den holzi selbst nie hat; `history.back()`
  hätte ihn in `tab-content-isolation` erreicht.
- Prozessende im Hintergrund: `kill -9` als die App selbst (`run-as`) nach `KEYCODE_HOME`;
  `am kill` beendet einen gerade erst in den Hintergrund gegangenen Prozess nicht.
- Der Bildschirmschutz stört die Screenshots von chromedriver nicht; er bleibt in jedem Fall an.

## Umsetzung in Stufe 2

- Gemischte Gruppen: `GroupDeps.phoneHost` ist das Telefon, `host` Linux. Auf das Telefon kommt das
  Gerät `phone`, sonst das erste Gerät einer Gruppe, die ohne `phone` angelegt wurde; ein später mit
  `addDevice('…', 'phone')` hinzugefügtes Gerät läuft dann auf Linux (`sync-link`: der Laptop auf
  dem Telefon verknüpft ein Linux-Gerät). Jedes Gerät merkt sich seinen Host.
- iroh-Relay: pro Szenario mit Gruppe ein `iroh-relay --dev` auf einem freien Port statt fest 3340
  (der Port steht in einer eigenen Konfiguration, Metriken aus), erreichbar wie die Dienste oben; alle Geräte
  bekommen `http://127.0.0.1:<port>` als einzigen iroh-Relay. Der Preflight verlangt `iroh-relay`
  (`E2E_IROH_RELAY` oder `PATH`).
- Linux-Teil: Ein Android-Lauf prüft zusätzlich die Linux-Werkzeuge und baut die Linux-App wie ein
  Linux-Lauf (`E2E_LINUX_APP`, `E2E_TOOLS`). Fehlen die Werkzeuge, bricht der Preflight mit dem ab,
  was fehlt (Review von #340: ein Lauf ohne die anderen Geräte wäre kein vollständiger Lauf).
- Dateien für ein Gerät (`onDevice`) liegen nur für das Gerät auf dem Telefon im App-Speicher, alle
  anderen bekommen den Pfad dieses Rechners.
- Tresordatei kopieren: vom Telefon auf ein Linux-Gerät (`run-as … cat`, Größe geprüft); umgekehrt
  braucht es noch kein Szenario.
- Der Preflight ersetzt auch eine App mit höherer Versionsnummer (ein Release-Build auf dem
  Emulator), wie schon eine mit fremdem Schlüssel.
- `sync-servers-off` schaltet alle Server ab, auch die iroh-Relays: Ein Gerät kennt den Relay des
  anderen von früher und erreichte es dort ohne Nostr-Server (Spec 033 FR-010 nennt nur die
  Nostr-Server; unter Linux zeigen die iroh-Relays ohnehin auf einen geschlossenen Port).
- Verknüpfen: Die erste Einladung des neuen Geräts kam ohne seinen Relay an, wenn er noch nicht
  verbunden war; das neue Gerät wartet jetzt bis zu 3 s darauf, und das Hauptgerät wählt nach 6 s
  ohne Verbindung die neueste Einladung (`sync/link/join_task.rs`, `host_task.rs`). Das betrifft
  ein echtes Telefon hinter NAT genauso.
- Stoppen eines Telefons in einer Gruppe schließt erst den Tresor (die App endet, FR-006): Nach
  einem erzwungenen Ende hielte das andere Gerät die Sitzung über den Relay bis zur Leerlaufzeit
  (etwa 45 s) und wiese den neuen Prozess so lange als Duplikat ab.
- Die Linux-Geräte eines Android-Laufs binden ihren Sync-Endpunkt nur an Loopback
  (`HOLZI_E2E_SYNC_LOOPBACK=1`, nur Debug-Builds, `sync::endpoint::app_bind_addr`): Das Netz des
  Emulators verlor nach einem Bündel großer UDP-Pakete den direkten Weg ganz (beobachtet beim
  Übertragen des Tresors in `sync-indirect`), und die Verbindung wich nicht auf den Relay aus. So
  läuft alles zwischen Telefon und Linux über den iroh-Relay des Szenarios.

## Umsetzung in Stufe 3

- Neuer Fall `android-notifications` (FR-023): Die Plattformschicht setzt die Antwort auf die
  Benachrichtigungsberechtigung vorab (`instance.phone.allowNotifications`: `pm grant`, oder als
  „abgelehnt, nicht mehr fragen“ markiert, weil ein Entzug den Prozess beendet) und liest die
  gezeigten Benachrichtigungen (`notificationTitles`, aus `dumpsys notification`). Abgelehnt:
  Meldung in holzi, keine Systembenachrichtigung; erlaubt: Systembenachrichtigung, keine Meldung.
- `android-not-available` prüft zusätzlich über die Probe-Erweiterung: freier Pfad, Ordnerdialog und
  Ordnerbeobachtung antworten 8001 (Gegenfall von `extension-files`).
- `sync-relay-return` hält während der Pause auch den iroh-Relay der Gruppe an (`g.irohRelay`, nur in
  einem Lauf mit Telefon): Über ihn fand ein Gerät das andere an der früher gelernten Adresse
  ohne Nostr-Relay, wie bei `sync-servers-off`. Der Relay kommt auf derselben Adresse zurück.

## Umsetzung in Stufe 4

- Der Ersatz-Anbieter kann HTTPS mit einem selbst signierten Zertifikat sprechen
  (`selfSignedTls`, Zertifikat pro Lauf von `openssl`, nichts im Repository). Ohne Befehl zum
  Ändern der Basisadresse eines Anbieters prüft `chat-provider-untrusted-cert` das Anlegen eines
  Anbieters an diesem Server; das Senden mit ungültigem Zertifikat prüfen die Rust-Tests.
- `chat-model-search` prüft plattformneutral, dass die Delegates genau dort angeboten werden, wo
  `platform_capabilities.cliDelegates` gilt.
- `sync-link` war in der CI auf `main` einmal rot: Das neue Linux-Gerät sendete seine Anwesenheit,
  bevor sein Relay verbunden war, also nur mit Loopback-Adressen; die nächste Meldung wäre erst nach
  60 s gekommen. Die Anwesenheit geht jetzt sofort neu hinaus, wenn sich der eigene Relay ändert
  (`presence_loop`, `SyncNode::addr_changes`). Das betrifft echte Geräte hinter NAT genauso.
- Die Fehlerunterlagen des Telefons enthalten zusätzlich `logcat.txt` (die letzten 20 000 Zeilen):
  Die Logdatei der App bricht auf dem Gerät manchmal früh ab.

## Umsetzung in Stufe 5

- `models-download-confirm` (Desktop und Telefon) braucht kein echtes Modell: Der Debug-Befehl
  `e2e_stand_in_models` legt Ersatz-Einträge neben den Katalog und schickt HuggingFace-Anfragen an
  einen Server auf dem Gerät (`models/stand_in.rs`; ein Release-Build lehnt ab, nur `127.0.0.1`
  und `localhost`). Als Server dient der Ersatz-Anbieter, der jede Anfrage festhält. Geprüft wird:
  Größe und freier Speicher im Bestätigungsschritt, vor der Bestätigung keine Anfrage, bei einem
  Eintrag von einem Petabyte die Warnung und „Trotzdem laden“, nach der Bestätigung die Anfrage.
- `android-microphone`: Die Telefon-Steuerung `refuseMicrophone()` legt die Ablehnung fest wie ein
  zweites „Nicht zulassen“ (`pm set-permission-flags … user-set user-fixed`); der Diktierknopf
  zeigt dann die Erklärung, und `start_voice_recording` antwortet mit `MicrophoneDenied`. Die
  Frage des Systems selbst lässt sich nicht bedienen.
- Texte prüfen die Szenarien sprachunabhängig über `lib/translations.ts` (der Text eines
  Schlüssels in allen Sprachen); Szenarien selbst lesen keine Dateien (SC-005).
