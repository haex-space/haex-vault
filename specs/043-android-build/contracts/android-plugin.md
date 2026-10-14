# Contract: Plugin-Crate `holzi-android`

**Anforderungen**: FR-006, FR-011, FR-011a, FR-012, FR-019, FR-023, FR-029, FR-017 (Gerätename)
| **Research**: R1, R4–R7, R10, R12

Lage: `src-tauri/plugins/holzi-android/` (Crate `tauri-plugin-holzi-android`, Pfad-Abhängigkeit
von `src-tauri`). Kotlin unter `android/src/main/java/space/haex/holzi/android/`. Am Desktop
übersetzt die Crate und meldet sich an, jede Funktion ist dort ein No-op bzw. liefert `None`.

## Rust-Schnittstelle (vom Kern benutzt, keine Commands für die Oberfläche)

```text
HolziAndroidExt::holzi_android(&self) -> &HolziAndroid
  set_secure(enabled: bool) -> Result<()>            // FLAG_SECURE + setRecentsScreenshotEnabled(!enabled) auf API 33+
  device_name() -> Result<Option<String>>            // Settings.Global.DEVICE_NAME, sonst Build.MODEL
  request_permission(p: Permission) -> Result<PermissionState>   // Permission::Microphone
  check_permission(p: Permission) -> Result<PermissionState>     // granted | denied | prompt
```

Benachrichtigungen fragt der Kern über den vorhandenen Fork von `tauri-plugin-notification` an
(`request_permission()`), nicht über diese Crate.

## Ereignisse (Kotlin → Kern)

| Ereignis          | Nutzlast                             | Auslöser                                             | Verbraucher                         |
| ----------------- | ------------------------------------ | ---------------------------------------------------- | ----------------------------------- |
| `network-changed` | `{ available: bool, metered: bool }` | `ConnectivityManager.registerDefaultNetworkCallback` | Sync: `network_change()`, Neuaufbau |

Rückkehr in den Vordergrund meldet Tauri selbst (`RunEvent::Resumed`); die Crate meldet sie
nicht doppelt.

## Umsetzung in Stufe 1c

- `display_name(uri) -> Option<String>` fragt den Anbieter einer gewählten Datei
  (`OpenableColumns.DISPLAY_NAME`, Vertrag [picked-file.md](./picked-file.md)).
- `watch_insets(on_change)` statt CSS-Variablen aus Kotlin: Die Crate meldet die Ränder über einen
  Kanal an den Kern (`platform/insets.rs`), der den letzten Wert hält, ihn als Ereignis
  `device-insets` sendet und mit `device_insets` auf Anfrage liefert. Die Seite schreibt daraus
  die Variablen unten (`plugins/deviceInsets.client.ts`). So fragt eine neu geladene Seite nach,
  statt Werte zu verlieren, die Kotlin vor dem Laden geschrieben hätte.

## Umsetzung in Stufe 2

- `device_name() -> Option<String>` (ohne `Result`): Der Kern übernimmt den Namen einmal beim
  Start (`hardware::hostname::use_platform_name`); „localhost“ zählt nie als Gerätename.
- `watch_network(on_change)` statt eines Ereignisses `network-changed`: ein Kanal wie bei den
  Rändern, ohne Nutzlast. Gemeldet wird nur ein Wechsel des Standardnetzes
  (`onAvailable` mit einem anderen Netz als zuletzt); das Netz beim Start ist kein Wechsel, ein
  Verlust allein auch nicht, denn ohne Netz gibt es nichts neu aufzubauen. `available` und
  `metered` braucht niemand (Datenvolumen ist eine eigene Idee in `plans/README.md`).
- Der Kern (`sync/resume.rs`) weckt auf `RunEvent::Resumed` und auf einen Netzwechsel die laufende
  Sync-Sitzung: Nostr-Relays neu verbinden und die eigene Anwesenheit sofort senden, den
  Wiederverbindungslauf anstoßen; beim Netzwechsel zusätzlich `Endpoint::network_change()`.
  Während der Tresor schließt, geschieht nichts.

## Umsetzung in Stufe 5

- `request_permission(Permission::Microphone)` und `check_permission` über die eigenen Befehle
  `requestPermissions`/`checkPermissions` des Tauri-Plugins: Die Crate erklärt `RECORD_AUDIO` im
  Manifest und in `@TauriPlugin(permissions = …)` unter dem Alias `microphone`. Die Anfrage
  blockiert, bis die Person antwortet; der Kern ruft sie vor jeder Aufnahme
  (`voice::ensure_microphone`), nach einer Entscheidung antwortet Android sofort. Am Desktop gilt
  die Berechtigung als erteilt. Eine Ablehnung ist `HolziError::MicrophoneDenied` (vorher das nie
  erzeugte `PermissionDenied`), und das Eingabefeld erklärt, wo man sie später erteilt.

## Seite (Kotlin → WebView)

In `load(webView)` setzt die Crate einen `OnApplyWindowInsetsListener` und schreibt bei jeder
Änderung per `evaluateJavascript` auf `document.documentElement`:

| CSS-Variable             | Inhalt                                                 |
| ------------------------ | ------------------------------------------------------ |
| `--holzi-inset-top`      | Statusleiste und Aussparung oben, in CSS-Pixeln (`px`) |
| `--holzi-inset-right`    | Systemleisten/Aussparung rechts                        |
| `--holzi-inset-bottom`   | Navigationsleiste unten (ohne Tastatur)                |
| `--holzi-inset-left`     | Systemleisten/Aussparung links                         |
| `--holzi-inset-keyboard` | Höhe der Bildschirmtastatur, `0px` wenn geschlossen    |

Die Seite benutzt `var(--holzi-inset-*, 0px)`; am Desktop fehlen die Variablen.

## Lebenszyklus

- Kein eigenes Beenden in der Crate, solange `RunEvent::Exit` nach `finishAffinity`
  zuverlässig kommt (R4); sonst ergänzt die Crate in `onDestroy` bei `isFinishing`
  `Process.killProcess(Process.myPid())`. Die Entscheidung fällt im ersten Gerätelauf und steht
  dann in research.md.

## Manifest und Build der Crate

- `AndroidManifest.xml`: `<uses-permission android:name="android.permission.RECORD_AUDIO"/>`,
  `ACCESS_NETWORK_STATE`.
- `@TauriPlugin(permissions = [Permission(strings = [RECORD_AUDIO], alias = "microphone")])`.
- `build.gradle.kts`: lokales Maven-Repo unter `android/maven/` mit dem versionierten AAR/POM
  von rustls-platform-verifier (die Upstream-Quelle ist in research R2 dokumentiert),
  Abhängigkeit `org.rustls:rustls-platform-verifier:<Version aus Cargo.lock>`, `androidx.core`.
- `proguard-rules.pro` (als `consumerProguardFiles`):
  `-keep, includedescriptorclasses class org.rustls.platformverifier.** { *; }`.

## Zertifikatsprüfung (nicht in der Crate)

Die Initialisierung von `rustls-platform-verifier` liegt im Kern (`src/tls/android.rs`): in
`setup` vor dem ersten Netzaufruf `ndk_context::android_context()` → `JavaVM` anhängen →
`rustls_platform_verifier::android::init_with_env(env, context)`. Die Crate liefert nur die
Gradle-Abhängigkeit und die Keep-Regel. Test: Der Android-e2e-Lauf macht HTTPS-Aufrufe
(Anbieter-Attrappe über HTTPS mit Test-CA ist **nicht** vertrauenswürdig → Fehlerfall; ein
öffentlicher Endpunkt → Erfolg) und `extension-isolation` ruft `example.org`.
