# Implementation Plan: holzi für Android

**Branch**: `043-android-build` | **Date**: 2026-10-07 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/043-android-build/spec.md`

## Summary

holzi läuft als Android-App auf echten Telefonen: derselbe Rust-Kern und dieselbe Oberfläche wie
am Desktop, dazu eine kleine lokale Plugin-Crate für alles, was nur Android hat. Der Kern
übersetzt schon fast vollständig für Android (nur `lettre` und zwei Desktop-Aufrufe in holzi
brechen); die Arbeit liegt in der Plattform-Anbindung, in Dateiflüssen ohne Pfade, im
Prozessmodell, in der e2e-Suite auf dem Emulator und in der CI.

Technischer Ansatz (Begründungen und verworfene Alternativen in [research.md](./research.md)):

- **Android-Projekt und Plugin-Crate** (R1): `src-tauri/gen/android` wird eingecheckt; Kotlin,
  Manifest-Berechtigungen, Keep-Regeln und Gradle-Abhängigkeiten liegen in
  `src-tauri/plugins/holzi-android/`, damit ein Neuerzeugen des Gerüsts nichts verliert.
- **Zertifikate** (R2): `rustls-platform-verifier` wird beim Start über `ndk-context` und `jni`
  initialisiert (System-Zertifikatsspeicher); `lettre` bekommt einen Patch (Fork mit fester
  Revision plus Upstream-PR), der auf Android `Verifier::new` statt `new_with_extra_roots`
  nimmt.
- **Gewählte Dateien** (R3): ein Baustein `files::picked` für Pfade (Desktop) und
  `content://`-Adressen (Android) über `tauri-plugin-fs`; alle Dateiflüsse (Anhänge, Import,
  Darstellung, Erweiterung installieren, Modell importieren, Dateidialoge der Erweiterungen,
  Tresordatei) laufen darüber, auf allen Plattformen gleich.
- **Prozessmodell** (R4, R5): Schließen beendet die App (`exit` → `finishAffinity` →
  `process::exit`), nie `process::restart` auf Android; Rückkehr in den Vordergrund und
  Netzwechsel stoßen den vorhandenen Wiederverbindungsweg des Syncs an.
- **Telefon-Oberfläche** (R6, R7, R9): Ränder und Tastaturhöhe als CSS-Variablen aus der
  Plugin-Crate; Bildschirmschutz (FLAG_SECURE) als Geräte-Einstellung, Standard an; eine
  Fähigkeitstabelle pro Plattform (`platform_capabilities`) ersetzt verstreute `cfg!`-Abfragen
  und steuert, was die Oberfläche anbietet.
- **Tresordatei übernehmen** (R8): neuer Command `import_instance`, auf Desktop und Android;
  Kopie unter `.pending`-Marker, Entsperren, Ablehnen einer Kopie eines schon vorhandenen
  Tresors, vollständiger Rückbau bei Fehlern.
- **Lokale KI** (R11): derselbe Lader (mistralrs, GGUF, CPU) und dieselbe Spracherkennung
  (candle Whisper) wie am Desktop; Telefon-Voreinstellungen aus `_meta.profiles`; ein
  gemeinsamer Bestätigungsschritt mit Größe und freiem Speicher vor jedem Modelldownload.
  ADR 0010 ersetzt den MLC-Teil von ADR 0002.
- **e2e auf Android** (R14): chromedriver direkt gegen die WebView, neue Plattform
  `scripts/e2e/lib/platform/android.ts` hinter dem vorhandenen `DeviceHost`, Dienste per
  `adb reverse`, lokales iroh-Relay, gemischte Gruppen (Gerät `phone` im Emulator). 57 von 60
  Szenarien laufen auf Android (95 %), drei sind begründet ausgenommen.
- **CI** (R15): Jobs `android-build` (APK-Artefakt) und `android-e2e` (3 Shards im Emulator mit
  KVM), zusammengefasst im Pflicht-Check `android`; Release-Workflow für signierte APKs bei Tags.

## Technical Context

**Language/Version**: Rust 1.98.1 (Tauri 2.12.1), Kotlin (Plugin-Crate, JVM 17), TypeScript 6
(strict), Vue 3.5, Nuxt 4.5.2 (SPA), Node 22.19 für Prüf- und e2e-Skripte

**Primary Dependencies**: vorhanden — tauri 2.12.1, tauri-plugin-dialog 2.8.1,
tauri-plugin-fs 2.6.0, tauri-plugin-notification (`tauri-apps/plugins-workspace@4638151`,
vorher Fork `haexmas/plugins-workspace@b38883e`),
wry (Fork `haexmas/wry@47037dd`), rustls-platform-verifier 0.7.1, reqwest 0.13.5,
haex-crdt `928d06a`, mistralrs 0.8.1, candle, cpal 0.18.2, sysinfo 0.39.6, iroh, nostr-sdk.
**Neu**: Rust `jni` 0.22 und `ndk-context` 0.1 (nur Android); `lettre` per Patch aus
`haexmas/lettre` (feste Revision); Gradle `org.rustls:rustls-platform-verifier` aus dem
eingecheckten Maven-Repo `src-tauri/plugins/holzi-android/android/maven/` (Version wie
`rustls-platform-verifier-android` im Lockfile), `androidx.core` (Insets, schon im Gerüst);
e2e: chromedriver (Chrome for Testing, passend zur WebView), `iroh-relay` (Testrechner).

**Storage**: Keine Migration. Neue Geräte-Einstellung `privacy.screenCaptureProtection` in der
vorhandenen `preferences`-Tabelle (`PrefScope::Device`). Tresore weiter unter
`<AppLocalData>/instances/`. Details in [data-model.md](./data-model.md).

**Testing**: `cargo test` (neue Module `files::picked`, `instances::import`, `platform`,
Fähigkeitstabelle, Modellvorschlag nach Profil), Vitest für die TS-Bausteine, e2e-Suite auf
Linux (wie bisher) und auf Android im Emulator (neu, Pflicht-Check), Handprüfung auf einem
echten Telefon nach [quickstart.md](./quickstart.md) für das, was der Emulator nicht kann
(Gesten, Aussparung, Netzwechsel, System-Dateiauswahl, Mikrofon, Benachrichtigungen).

**Target Platform**: Android 8.0+ (`minSdk` 26 wegen AAudio, research R1; `targetSdk` 37), `arm64-v8a` (Telefone) und
`x86_64` (Emulator); Linux, macOS und Windows unverändert.

**Project Type**: Desktop- und Mobil-App (Tauri) mit einer neuen lokalen Plugin-Crate.

**Performance Goals**: Tresorauswahl ≤ 3 s nach dem Antippen (SC-001); Entsperren ≤ 2 × Desktop,
≤ 10 s (SC-002); Sync-Änderung ≤ 10 s (SC-005); lokales Modell beginnt ≤ 15 s nach dem Senden
auf 6 GB RAM (SC-009). CI: Android-Pflicht-Check mit warmem Cache ≤ 45 min.

**Constraints**: Keine Geheimnisse im Repository (Signaturschlüssel nur als CI-Geheimnis);
keine Pfade über die Grenze zwischen Oberfläche und Kern auf Android (nur gewählte Dateien);
Schließen beendet die App; kein Hintergrund-Sync; jede Regel an einer Stelle (Fähigkeitstabelle,
Bestätigungsschritt für Downloads, Datei-Baustein).

**Scale/Scope**: 6 User Stories, 34 Anforderungen plus FR-002a, FR-011a, FR-033a–c; 60
e2e-Szenarien, davon 57 auf Android; etwa 7 neue Android-Fälle; 8 umzustellende Dateiflüsse.

## Constitution Check

_GATE: Must pass before Phase 0 research. Re-check after Phase 1 design._

| Prinzip                                     | Prüfung                                                                                                                                                                                                                                                                            | Ergebnis |
| ------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------- |
| I. Keine Geheimnisse in Git                 | Signaturschlüssel und Passwörter nur als GitHub-Geheimnisse; `gen/android/keystore.properties` bleibt durch die `.gitignore` des Gerüsts ausgeschlossen und wird nur in der CI nach `$RUNNER_TEMP` geschrieben. Debug-Builds benutzen den Debug-Schlüssel des SDK (nicht im Repo). | erfüllt  |
| II. Keine lokalen absoluten Pfade           | `gen/android` enthält nur relative Pfade (Gerüst); `local.properties` (SDK-Pfad) ist durch die `.gitignore` des Gerüsts ausgeschlossen; SDK-Pfade kommen aus `ANDROID_HOME`.                                                                                                       | erfüllt  |
| III. Identität geräteunabhängig             | Tresor-Identität und Gerätekennung bleiben wie in Spec 024; die Tresordatei-Übernahme behält die Vault-Identität und erzeugt pro Installation eine neue Gerätekennung (R8).                                                                                                        | erfüllt  |
| IV. Feste Revisionen                        | `lettre`-Fork und wry-Fork mit voller Commit-SHA in `[patch.crates-io]`; Gradle-Artefakt mit fester Version; GitHub-Actions mit festen Versionen wie die vorhandenen Jobs.                                                                                                         | erfüllt  |
| V. Externe Quellen nur per Allowlist        | Keine neue Harness-Quelle; die Build-Umgebung kommt aus `haexmas/atoms` @ `b662c6205d6a1ebd7fac588291f82ea8e78c3d52`, Pfade `holzi/.devshell/packages.nix`, `holzi/.devshell/rust-toolchain.toml` und `holzi/nix-packages.json` (in holzi mit PR #308 ausgeliefert).               | erfüllt  |
| VI. Selbständernde Anweisungen per Review   | Keine Änderung an Constitution, Skills oder Berechtigungen; Branch-Schutz (neuer Pflicht-Check) ändert die Person, die das Repo betreut, selbst.                                                                                                                                   | erfüllt  |
| VII. Relay-Ausfall blockiert nie lokal      | Tresor, Passwortmanager und lokale Modelle laufen ohne Netz (Edge Case „ohne Netz“); e2e-Dienste laufen lokal.                                                                                                                                                                     | erfüllt  |
| VIII. Keine Verschleierung                  | nicht berührt                                                                                                                                                                                                                                                                      | erfüllt  |
| Workflow: ADR bei Grundsatzentscheidungen   | ADR 0010 hält die Android-Architektur und die Änderung an ADR 0002 (kein MLC-Backend) fest.                                                                                                                                                                                        | erfüllt  |
| Workflow: Phasen, PRs, Conventional Commits | Lieferung in Stufen, jede ein PR auf `main`, Merge ohne Squash.                                                                                                                                                                                                                    | erfüllt  |

Nach Phase 1 erneut geprüft: Die Verträge führen keine Pfade, Geheimnisse oder Branch-Verweise
ein; der Bildschirmschutz und die Fähigkeitstabelle sind gerätebezogen bzw. statisch. Keine
Verletzung, keine Einträge unter Complexity Tracking.

## Lieferung in Stufen (jede ein PR, jede auslieferbar)

| Stufe | Inhalt                                                                                                                                                                                                                                                                    | Stories               | Nachweis                                                                                      |
| ----- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------- | --------------------------------------------------------------------------------------------- |
| 1a    | Baut und startet: `gen/android`, Plugin-Crate (Grundgerüst, Beenden), Zertifikats-Initialisierung, lettre-Patch, Übersetzungsfehler, Prozessmodell, Fähigkeitstabelle (Grundlage), Entwicklungsmodus, ADR 0010, CI-Job `android-build` mit APK-Artefakt, Versionsabgleich | US1 (Teil), US2 (Bau) | APK aus der CI startet im Emulator und auf dem Telefon; Tresor anlegen, schließen, entsperren |
| 1b    | e2e auf Android: Plattform `android.ts`, chromedriver, `adb reverse`, iroh-Relay, gemischte Gruppen, Shards, Ausnahmeliste mit `excluded`/`pending`, Pflicht-Check `android`                                                                                              | US2 (Tests)           | Android-Lauf grün mit allen nicht-`pending` Szenarien                                         |
| 1c    | Telefon-Oberfläche: Ränder und Tastatur, Zurück-Geste, Bildschirmschutz und Einstellung, gewählte Dateien in allen Abläufen, Tresordatei übernehmen (Desktop und Android), „nicht verfügbar“ in der Oberfläche, neue Android-Fälle                                        | US1                   | Spec 036 T081 und Quickstart §3 auf dem Telefon; neue Android-Fälle grün                      |
| 1d    | Release: `android-release.yml`, Signatur aus Geheimnissen, `versionCode`                                                                                                                                                                                                  | US2 (Release)         | signiertes APK installiert sich über das vorige Release ohne Datenverlust                     |
| 2     | Sync: Rückkehr in den Vordergrund, Netzwechsel, Gerätename; Mehrgeräte-Szenarien aus `pending` holen                                                                                                                                                                      | US3                   | `sync-*` im Emulator grün; Quickstart §5 (WLAN ↔ Mobilfunk)                                   |
| 3     | Erweiterungen: CSP-Ursprung pro Plattform, Dateidialoge der Erweiterungen über gewählte Dateien, Benachrichtigungs-Berechtigung, endgültige Ausnahmen mit Gegenfällen; Spec 017 T112 (Android)                                                                            | US4                   | `extension-*` im Emulator grün; T112 abgehakt                                                 |
| 4     | Chat mit Online-Anbietern: Delegates ausgeblendet, `run_command` nicht angemeldet, Fall „ungültiges Zertifikat“                                                                                                                                                           | US5                   | Chat-Szenarien im Emulator grün; Quickstart §7                                                |
| 5     | Lokale KI und Sprache: Profile im Katalog, Bestätigungsschritt mit Größe und freiem Speicher (alle Plattformen), Mikrofon-Berechtigung, Messung SC-009; `pending` ist leer                                                                                                | US6                   | Quickstart §8 auf dem Telefon; SC-007 erfüllt                                                 |

Stufe 1a bis 1d bilden zusammen das MVP (P1). Ab Stufe 1b ist der Android-Lauf Pflicht-Check:
Szenarien, deren Stufe noch aussteht, stehen als `pending` mit Stufennummer in der Ausnahmeliste
und werden mit ihrer Stufe herausgenommen.

## Project Structure

### Documentation (this feature)

```text
specs/043-android-build/
├── plan.md              # dieser Plan
├── research.md          # Phase 0: R1–R17
├── data-model.md        # Phase 1: Einstellung, Fähigkeiten, gewählte Datei, Übernahme, Ausnahmeliste
├── quickstart.md        # Phase 1: Bauen, e2e auf Android, Handprüfung auf dem Telefon
├── contracts/
│   ├── platform-capabilities.md   # Command platform_capabilities
│   ├── import-instance.md         # Command import_instance
│   ├── picked-file.md             # gewählte Datei an der Grenze Oberfläche/Kern
│   ├── android-plugin.md          # Plugin-Crate: Commands, Ereignisse, Berechtigungen
│   ├── e2e-android.md             # DeviceHost android, Ausnahmeliste, CLI
│   └── ci.md                      # Jobs, Pflicht-Check, Geheimnisse
├── checklists/requirements.md
└── tasks.md             # Phase 2 (/speckit-tasks)
```

### Source Code (repository root)

```text
src-tauri/
├── Cargo.toml                         # jni, ndk-context (android); [patch.crates-io] lettre
├── gen/android/                       # neu eingecheckt (tauri android init), MainActivity unverändert
├── plugins/holzi-android/             # neu: lokale Plugin-Crate
│   ├── Cargo.toml, build.rs           # tauri_plugin::Builder … .android_path("android")
│   ├── src/lib.rs, src/mobile.rs      # Rust-Seite; Desktop: no-op
│   └── android/                       # Kotlin: HolziAndroidPlugin.kt, Manifest, proguard, build.gradle.kts
└── src/
    ├── lib.rs                         # Plugin anmelden, Verifier-Init, RunEvent::Exit/Resumed
    ├── platform/                      # neu: Fähigkeitstabelle, Command platform_capabilities
    ├── tls/android.rs                 # neu: rustls-platform-verifier initialisieren
    ├── files/picked.rs                # neu: gewählte Datei lesen/schreiben/kopieren
    ├── instances/import.rs            # neu: Tresordatei übernehmen
    ├── instances/{create,close_effects,paths}.rs  # Rückbau-Helfer, Android-Exit
    ├── vault_gate/mod.rs              # close_policy auf Android
    ├── extensions/fs/{dialogs,mod}.rs # cfg(desktop) Ordnerauswahl, gewählte Dateien
    ├── extensions/protocol/csp.rs     # Ursprung pro Plattform
    ├── adapters/cli_delegate/         # Fähigkeitsprüfung vor dem Start
    ├── chat/session.rs                # run_command nur mit commandTool
    ├── hardware/{hostname,mod}.rs     # Gerätename Android, keine GPU-Erkennung
    ├── catalog/, stt/catalog.rs       # Profile mobile/mobile_low_memory, whisper-tiny
    ├── models/download_check.rs       # neu: Größe und freier Speicher
    └── sync/                          # Resumed/Netzwechsel → Wiederverbindung
src/
├── composables/useDeviceCapabilities.ts # neu: Fähigkeiten aus dem Kern
├── composables/usePickedFile.ts       # neu: Dialog → gewählte Datei
├── plugins/actions.client.ts          # Zurück-Geste über Fähigkeiten statt User-Agent
├── components/wm/Desktop.vue          # Ränder über --holzi-inset-*
├── pages/index.vue + components/onboarding/ImportVaultSheet.vue  # Tresordatei öffnen
├── components/settings/ScreenCaptureSetting.vue                  # neu
└── components/{passwords,chat,extensions,settings,models}/…      # Dateiflüsse, Bestätigungsschritt
scripts/e2e/
├── lib/platform/android.ts            # neu
├── lib/{webdriver,instance,scenario,page,device}.ts  # Fähigkeiten pro Plattform, startInstance über DeviceHost
├── platform-exclusions.ts             # neu: excluded/pending mit Begründung bzw. Stufe
├── scenarios/android-*.test.ts        # neu: Telefon-Fälle
└── cli.ts                             # --platform, --shard
scripts/check-e2e-exclusions.ts, scripts/check-android-versions.ts  # neue Prüfskripte
.github/workflows/ci.yml               # android-build, android-e2e (3), android
.github/workflows/android-release.yml  # neu
docs/adr/0010-android-platform.md      # neu
nuxt.config.ts, tauri.conf.json        # TAURI_DEV_HOST, devCsp, Android-Bundle
```

**Structure Decision**: Ein Projekt wie bisher; neu sind nur die Plugin-Crate unter
`src-tauri/plugins/holzi-android/` (eigene Crate, weil Tauri Kotlin-Code nur über Plugins
sauber einbindet) und das eingecheckte Gerüst `src-tauri/gen/android/`. Alles andere sind
Module im vorhandenen Kern und der vorhandenen Oberfläche.

## Complexity Tracking

Keine Verletzungen der Constitution.

## Bewusste Grenzen (aus research.md)

- Kein dauerhafter Zugriff auf gewählte Dateien über einen Neustart hinaus (R3).
- Native Teile (System-Dateiauswahl, Android-Benachrichtigungen, echte Gesten) prüft die
  e2e-Suite nicht; das bleibt Handprüfung im Quickstart (R14).
- Lokale Modelle nur auf der CPU (R11); ein natives Backend wäre eine spätere Entscheidung mit
  eigenem ADR.
- iOS, Store-Veröffentlichung, Hintergrund-Sync, Entsperren per Biometrie: nicht in dieser Spec.

## Offene Punkte für den ersten Lauf auf dem Gerät

- Kommt `RunEvent::Exit` nach `finishAffinity` zuverlässig (R4)? Sonst Beenden in `onDestroy`
  der Plugin-Crate.
- Erkennt iroh Netzwechsel auf Android selbst (R5)? Der Anstoß aus der Plugin-Crate bleibt
  in jedem Fall.
- Bleiben chromedriver-Bildschirmfotos trotz FLAG_SECURE sichtbar (R7)? Die e2e-Plattform
  schaltet den Schutz sowieso aus.
- WebView-Version des Emulator-Images für chromedriver (R14).
