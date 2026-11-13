# Contract: CI für Android

**Anforderungen**: FR-031, FR-032, FR-033, FR-033a, SC-007, SC-008 | **Research**: R15

## Jobs in `.github/workflows/ci.yml`

| Job             | läuft bei         | tut                                                                                                                                                                                                                   | Ergebnis                                     |
| --------------- | ----------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------- |
| `android-build` | PR, Push auf main | Java 17, SDK-Pakete per `sdkmanager`, Rust 1.98.1 + Ziele, `pnpm tauri android build --debug --apk --split-per-abi --target aarch64 --target x86_64`                                                                  | Artefakt `holzi-android-debug` (APK)         |
| `android-e2e`   | PR, Push auf main | Matrix `shard: [1, 2, 3]`; KVM freischalten, Emulator (API 35, `google_apis`, x86_64, Snapshot-Cache), APK installieren, Linux-App für gemischte Gruppen bauen, `pnpm test:e2e --platform android --shard ${shard}/3` | Fehlerunterlagen als Artefakt bei Fehlschlag |
| `android`       | PR, Push auf main | `needs: [android-build, android-e2e]`, scheitert, wenn einer scheitert                                                                                                                                                | **Pflicht-Check** im Branch-Schutz           |

Festgelegte Versionen (dieselben wie die devShell, geprüft von `pnpm check:android-versions`):
`platforms;android-36`, `platforms;android-37.0`, `build-tools;36.0.0`, `build-tools;37.0.0`,
`ndk;28.2.13676358`, Rust `1.98.1`, Ziele `aarch64-linux-android`, `x86_64-linux-android`.

Caches: `Swatinem/rust-cache` mit Schlüssel `android`, `gradle/actions/setup-gradle`, AVD-Snapshot.
Der Job `documentation` führt zusätzlich `pnpm check:e2e-exclusions` und
`pnpm check:android-versions` aus.

## Release `.github/workflows/android-release.yml`

- Auslöser: `workflow_call` aus `release.yml` (Tag `v*`; dort läuft vor dem Veröffentlichen die
  ganze CI, auch die Android-Jobs) und `workflow_dispatch` (zum Prüfen der Signatur ohne
  Release-Anhang).
- Schritte: wie `android-build`, aber `pnpm tauri android build --apk --split-per-abi --target
aarch64 --target x86_64` (Release); vorher den Keystore aus den Geheimnissen nach
  `$RUNNER_TEMP` und `src-tauri/gen/android/keystore.properties` (von git ignoriert) schreiben,
  nach dem Bau beide löschen; danach `apksigner verify --print-certs` gegen den erwarteten
  Fingerabdruck des Zertifikats (Variable `ANDROID_CERT_SHA256`, öffentlich): genau ein
  Unterzeichner, genau dieser Fingerabdruck.
- Geheimnisse: `ANDROID_KEY_BASE64` (Keystore, Base64), `ANDROID_KEY_ALIAS`,
  `ANDROID_KEY_PASSWORD` (PKCS12: ein Passwort für Keystore und Schlüssel). Fehlen sie oder die
  Variable, scheitert der Workflow im ersten Schritt; es entsteht nie ein unsigniertes oder mit
  Debug-Schlüssel signiertes Release.
- `versionCode` aus der Version in `tauri.conf.json` (Tauri-Standard: Major × 1 000 000 +
  Minor × 1 000 + Patch), damit jedes Release ein Update des vorigen ist. Ein Tag, der nicht
  `v<version>` heißt, lässt den Workflow scheitern.
- Ergebnis: je ABI ein APK `holzi-<version>-<abi>.apk` (arm64 für Telefone, x86_64 für
  Emulatoren) als Artefakt `holzi-android-release`; bei Tags hängt der Job `publish` von
  `release.yml` mit Schreibrecht sie an das Release des Tags, sobald CI und alle Builds grün sind
  (Signaturgeheimnisse und Schreib-Token nie im selben Job). Store-Veröffentlichung ist nicht
  Teil der Spec.
- Kein Cargo-Cache: Releases sind selten, und ihr Cache würde die Caches der CI aus den 10 GB
  des Repos verdrängen.

**Umsetzung in Stufe 1d**: getrennte APKs je ABI statt eines gemeinsamen (halbe Downloadgröße
auf dem Telefon); Prüfung Tag gegen Version und der eigene Job zum Anhängen kamen dazu.

## Aufgaben für die Person, die das Repo betreut (nicht im Code)

- Keystore einmalig erzeugen und sicher verwahren (Verlust = keine Updates mehr möglich).
- Die drei Geheimnisse und die Variable `ANDROID_CERT_SHA256` im Repo setzen.
- Den Pflicht-Check `android` im Branch-Schutz von `main` eintragen (ab Stufe 1b).
