# Data Model: haex-vault für Android

**Spec**: [spec.md](./spec.md) | **Plan**: [plan.md](./plan.md)

Keine Migration und keine neue Tabelle. Neu sind eine Geräte-Einstellung in der vorhandenen
`preferences`-Tabelle und einige Typen an der Grenze zwischen Oberfläche und Kern.

## Geräte-Einstellung `privacy.screenCaptureProtection` (FR-011a)

| Feld      | Wert                                                                                |
| --------- | ----------------------------------------------------------------------------------- |
| Tabelle   | `preferences` (CRDT, vorhanden)                                                     |
| Bereich   | `PrefScope::Device(vault_device_uuid)` (ADR 0001); gilt nur für dieses Gerät        |
| Schlüssel | `privacy.screenCaptureProtection`                                                   |
| Wert      | `"true"` / `"false"` (über `parse_bool` wie `extensions/dev.rs`)                    |
| Standard  | fehlender Wert = an                                                                 |
| Sichtbar  | nur, wenn `platform_capabilities.screenCapture` wahr ist (Android)                  |
| Wirkung   | nach Entsperren/Anlegen und bei jeder Änderung: Plugin-Command `setSecure(enabled)` |

Die Zeile wird wie alle Geräte-Einstellungen synchronisiert, aber nur vom Gerät mit derselben
`vault_device_uuid` gelesen. Die Einstellungen-App speichert beim Umschalten sofort (kein
Speichern-Knopf).

## `PlatformCapabilities` (FR-016, FR-026; Vertrag [platform-capabilities.md](./contracts/platform-capabilities.md))

Statische Tabelle pro Zielplattform, kein gespeicherter Zustand. ts-rs-Typ:

| Feld              | Typ  | Desktop                   | Android   | Bedeutung                                      |
| ----------------- | ---- | ------------------------- | --------- | ---------------------------------------------- |
| `platform`        | enum | `linux`/`macos`/`windows` | `android` | Plattform                                      |
| `cliDelegates`    | bool | ja                        | nein      | Delegates claude/codex                         |
| `commandTool`     | bool | ja                        | nein      | Werkzeug `run_command` im Chat                 |
| `terminal`        | bool | ja                        | nein      | Terminal für Erweiterungen                     |
| `folderWatch`     | bool | ja                        | nein      | Ordner beobachten                              |
| `freePaths`       | bool | ja                        | nein      | freie Dateipfade für Erweiterungen             |
| `folderPick`      | bool | ja                        | nein      | Ordnerauswahl                                  |
| `gpuDetection`    | bool | ja                        | nein      | Grafikkarten-Erkennung                         |
| `relaunchOnClose` | bool | Release: ja               | nein      | Neustart zur Tresorauswahl nach dem Schließen  |
| `screenCapture`   | bool | nein                      | ja        | Bildschirmschutz einstellbar                   |
| `backGesture`     | bool | nein                      | ja        | System-Zurück-Geste vorhanden                  |
| `modelPresets`    | enum | `desktop`                 | `phone`   | Vorschläge der Modellkataloge (FR-027, FR-030) |

## Gewählte Datei (FR-015, FR-022, FR-002a; Vertrag [picked-file.md](./contracts/picked-file.md))

| Feld   | Typ                 | Regel                                                                                |
| ------ | ------------------- | ------------------------------------------------------------------------------------ |
| `file` | String (`FilePath`) | Desktop: absoluter Pfad aus dem Dialog; Android: `content://`-Adresse aus dem Dialog |
| `name` | String, optional    | Anzeigename; Android: aus dem Dialog, Desktop: Dateiname                             |

Regeln: Eine gewählte Datei kommt nur aus einem Dialog dieser Sitzung; der Kern liest und
schreibt sie ausschließlich über `files::picked` (`app.fs().open`). Auf Android lehnt der Kern
Pfad-Strings in diesen Commands ab (`invalid_input`). Kein Zugriff über die Sitzung hinaus.

## Übernahme einer Tresordatei (FR-002a; Vertrag [import-instance.md](./contracts/import-instance.md))

Zustände einer Übernahme (nur während des Commands, nichts wird dauerhaft gespeichert außer dem
Ergebnis):

```text
gewählt → Name bestimmt → Marker angelegt → kopiert → entsperrt → geprüft (Identität neu) → veröffentlicht
                                 │             │          │              │
                                 └─────────────┴──────────┴──────────────┴──→ zurückgebaut (alle Dateien weg)
```

| Datei unter `<AppLocalData>/instances/` | angelegt in     | beim Rückbau entfernt |
| --------------------------------------- | --------------- | --------------------- |
| `<name>.db.pending`                     | Marker angelegt | ja                    |
| `<name>.db.<uuid>.tmp`                  | kopiert         | ja                    |
| `<name>.db`                             | kopiert         | ja                    |
| `<name>.db.lock`, `-wal`, `-shm`        | entsperrt       | ja                    |
| `<name>.db.vault-id`                    | veröffentlicht  | ja                    |

`<name>.db.vault-id` (neu, für jeden Tresor): eine Zeile mit dem SHA-256 des öffentlichen
Schlüssels der Vault-Identität, hexadezimal. Geschrieben bei Anlegen, Öffnen und Übernehmen;
entfernt mit dem Tresor. Sie enthält kein Geheimnis und dient nur dazu, eine zweite Kopie
desselben Tresors auf diesem Gerät zu erkennen. `list.rs` überspringt sie wie die anderen
Begleitdateien.

Name: Dateiname ohne Endung, Zeichen außerhalb `[A-Za-z0-9_-]` zu `-`, am Anfang ein
Buchstabe oder eine Ziffer, höchstens 64 Zeichen; belegt → `-2`, `-3`, … Fehlerarten:
`WrongPassphrase`, `NotAVault`, `AlreadyOnThisDevice`, `NotEnoughSpace`, `Unreadable`.

## Ausnahmeliste der e2e-Suite (FR-033b, SC-007; Vertrag [e2e-android.md](./contracts/e2e-android.md))

`scripts/e2e/platform-exclusions.ts`, ein Eintrag pro Szenario und Plattform:

| Feld          | Typ                         | Regel                                                                 |
| ------------- | --------------------------- | --------------------------------------------------------------------- |
| `scenario`    | String                      | Dateiname ohne `.test.ts`; muss existieren                            |
| `platform`    | `'android'`                 | weitere Plattformen später                                            |
| `kind`        | `'excluded'` \| `'pending'` |                                                                       |
| `reason`      | String                      | bei `excluded` Pflicht, mit Verweis auf FR-006 oder FR-016            |
| `counterCase` | String                      | bei `excluded` Pflicht: Android-Szenario, das „nicht verfügbar“ prüft |
| `stage`       | String                      | bei `pending` Pflicht: Stufe aus dem Plan, die es behebt              |

`pnpm check:e2e-exclusions` prüft die Regeln und berechnet die Abdeckung
(`(Szenarien − excluded − pending) / Szenarien`); ab Stufe 5 MUSS `pending` leer und die
Abdeckung ≥ 80 % sein.

## Modell-Vorschlag nach Profil (FR-027, FR-030)

Vorhanden: `catalog/model_catalog.json` `_meta.profiles` mit `desktop`, `mobile`,
`mobile_low_memory`. Neu gelesen: Auf Mobilgeräten kommen nur Modelle der beiden mobilen Profile
in die Auswahl; zwischen ihnen entscheidet die vorhandene Passung nach Arbeitsspeicher
(`hardware/fit.rs`). Spracherkennung: mobil `whisper-tiny` als Vorschlag.

## Bestätigung vor Modelldownloads (FR-028)

| Feld        | Typ  | Herkunft                               |
| ----------- | ---- | -------------------------------------- |
| `sizeBytes` | u64  | Katalog bzw. HuggingFace-Metadaten     |
| `freeBytes` | u64  | freier Speicher des Datenordners       |
| `fits`      | bool | `freeBytes ≥ sizeBytes + 10 %` Reserve |

Ein Download startet erst nach Bestätigung; ist `fits` falsch, zeigt der Schritt eine Warnung
und der Knopf heißt „Trotzdem laden“.
