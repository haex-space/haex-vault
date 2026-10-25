# Implementation Plan: Versionsstände der Geräte in der Föderationsübersicht

**Branch**: `059-device-versions` | **Date**: 2026-10-25 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/059-device-versions/spec.md`

## Summary

Die Geräteliste in Einstellungen → Föderation → Geräte zeigt bei jedem Gerät Sync-Protokoll, Vault-Schema
und Änderungsverfolgung; Abweichungen vom eigenen Stand sind markiert und als kleiner oder größer lesbar. Dafür
nennen im Handshake beide Seiten ihren Stand vor der Entscheidung, und jedes Gerät hält den gesehenen Stand je
Gerät lokal fest.

Technischer Ansatz (Begründungen und verworfene Alternativen in [research.md](./research.md)):

- **Wire** (R1, R2): `Challenge` bekommt `schema: SchemaVersion` als letztes Feld; `PROTOCOL_VERSION` wird 2,
  die ALPN bleibt `holzi-sync/1`; das wählende Gerät prüft das Versionsbyte (nach dem Variantenkennzeichen) der `Challenge`, bevor es dekodiert.
- **Zuordnung** (R3): beide Seiten bestimmen das Gerät über den Endpunkt und die signierte Geräteliste
  (`problems::device_at_endpoint`), nie über Angaben der Gegenseite.
- **Speicher** (R4): drei Spalten in `device_presence_no_sync` (Migration `0040_device_peer_schema`), gerätelokal,
  keine CRDT-Tabelle, `HOLZI_TRIGGER_VERSION` bleibt 27.
- **Rückruf und Ereignis** (R5): der Handshake meldet jeden gesehenen Stand an den Knoten, der schreibt und bei
  Änderung `sync-devices-changed` auslöst.
- **Anzeige** (R6): `VaultDevice.schema`, reiner Vergleich `schemaDiff` in `deviceStatus.ts`, Anzeige in
  `DeviceRow.vue` mit Zeichen und Lesetext zusätzlich zur Farbe.
- **Meldung und Agent** (R6a): `problemMessage` nennt das zu aktualisierende Gerät, wenn die Richtung eindeutig
  ist (FR-012); `settings.devices.list` liefert die Werte mit (FR-011).

## Technical Context

**Language/Version**: Rust (Tauri 2, Edition wie `src-tauri/Cargo.toml`); TypeScript (strict), Vue 3, Nuxt 4 (SPA)

**Primary Dependencies**: keine neuen; vorhanden `postcard`, `serde`, `ts-rs`, `vue-i18n`, haex-ui

**Storage**: Vault-Datenbank (haex-crdt), gerätelokale Tabelle `device_presence_no_sync`, drei neue Spalten

**Testing**: `cargo test` gezielt (`-j 4`) für `sync::` und die betroffenen Testbinaries; `node --test` (`pnpm check:settings`, `scripts/check-settings.ts`) für
`deviceStatus`; der Föderations-E2E (tauri-driver, Szene `settings-federation`) für die Anzeige

**Target Platform**: Linux, macOS, Windows, Android, iOS (kein Plattformcode; reine Anzeige und Handshake)

**Project Type**: Tauri-Desktop-/Mobil-App (Rust-Backend, Nuxt-Frontend)

**Performance Goals**: ein zusätzliches Schreiben je Handshake; Anzeige ohne spürbare Verzögerung

**Constraints**: Sync gleicher Stände unverändert (SC-003); ältere Stände bekommen weiter eine Versionsmeldung
(R2); keine neue Berechtigung, keine Weitergabe an Erweiterungen oder MCP (FR-011)

**Scale/Scope**: eine Handvoll Geräte je Vault; drei Zahlen je Gerät

## Constitution Check

_GATE: vor Phase 0 und nach Phase 1 geprüft._

- I Keine Geheimnisse in Git, II keine lokalen Pfade, III–V Identität/Quellen: nicht berührt; die Spec nennt keine
  Pfade außerhalb des Repos.
- VII Relay-Ausfall blockiert lokale Arbeit nicht: die Werte kommen aus dem Handshake, die Anzeige liest
  lokale Daten; ohne Verbindung steht „Stand noch nicht bekannt“.
- Konventionelle Commits, PR nach `main`, kein Squash: Arbeit in `.worktrees/059-device-versions`, ein PR.
- Ein Entwurfsbeschluss, der einen Kernsatz verändert (ADR nötig)? Nein: die Abwägung ALPN gegen `v` ist in
  Spec 024 R14 und hier in R2 festgehalten; die ändernde Fassung von R14 steht im Anhang `contracts/`.
- Eine Umsetzung je Regel: die Zuordnung Endpunkt → Gerät bleibt bei `problems::device_at_endpoint`.
- Jedes Gerät und jedes System: kein Plattformcode, Test läuft auf Linux gegen In-Memory-Streams wie die
  bestehenden Handshake-Tests.

Nach Phase 1: unverändert bestanden; keine Komplexitätsabweichung.

## Project Structure

### Documentation (this feature)

```text
specs/059-device-versions/
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/
│   ├── sync-protocol.md      # Änderung von Spec 024 contracts/sync-protocol.md (Challenge, v, R14)
│   └── tauri-commands.md     # Änderung von Spec 024 contracts/tauri-commands.md (VaultDevice)
└── tasks.md                  # durch /speckit-tasks
```

### Source Code (repository root)

```text
src-tauri/src/
├── sync/
│   ├── wire.rs                    # Challenge.schema, PROTOCOL_VERSION = 2, Lesen der Challenge mit v-Prüfung
│   ├── handshake.rs               # Challenge schreiben, Rückruf on_peer_schema, v-Prüfung beim Wählen
│   ├── peer_schema.rs             # neu: Speichern/Lesen der drei Werte (device_presence_no_sync)
│   ├── endpoint.rs                # Rückruf verdrahten, Gerät über Endpunkt auflösen, Ereignis
│   ├── endpoint_devices.rs        # record_peer_schema (neben mark_seen/flag)
│   ├── device_view.rs             # VaultDevice.schema, DeviceSchema, Facts.schema
│   ├── handshake_schema_tests.rs  # neu: beide Seiten kennen die Werte (angenommen/abgelehnt/alt)
│   ├── peer_schema_tests.rs       # neu
│   └── device_view_tests.rs       # erweitert
└── identity/
    ├── migrations.rs              # 0040 registrieren
    ├── migrations_peer_schema.rs  # neu: PEER_SCHEMA_0040
    └── migrations_peer_schema_tests.rs  # neu (Muster: migrations_file_shares_tests.rs)

src/
├── lib/sync/deviceStatus.ts       # schemaDiff, problemMessage (geprüft in scripts/check-settings.ts)
├── lib/actions/settingsActions.ts # Beschreibung von settings.devices.list nennt die Werte
├── stores/settingsActionHandlers.ts  # settings.devices.list liefert schema
├── components/settings/DeviceRow.vue
├── i18n/locales/{de,en}.json      # settings.federation.schema.*
└── types/bindings/                # DeviceSchema.ts, VaultDevice.ts (ts-rs)

specs/024-own-device-sync/         # Anhang: contracts/sync-protocol.md, contracts/tauri-commands.md,
                                   # data-model.md, research.md R14 verweisen auf 059
scripts/check-settings.ts          # schemaDiff
scripts/e2e/scenarios/settings-federation.test.ts  # „Dieses Gerät“ zeigt drei Zahlen
```

**Structure Decision**: Die Änderung bleibt im Sync-Modul und in der Geräteliste. `peer_schema.rs` kapselt den
Zugriff auf die drei Spalten, damit `problems.rs` (Problem) und `seen.rs` (Zeit) unberührt bleiben und jede
Spalte genau einen Schreiber-Pfad hat.

## Risiken und Reihenfolge

- Die Migration `0040` kann mit einer anderen offenen Spec kollidieren (Nummernvergabe): vor dem PR `origin/main`
  einmischen und bei Bedarf die Nummer anpassen; sie steht nur in `migrations.rs` und in der Datei des SQL.
- Die Migrationsanzahl steigt, daher synchronisieren bereits laufende Entwicklungsgeräte erst nach dem
  gemeinsamen Update; das ist Absicht und zeigt sich dann in der neuen Anzeige.
- R2 hängt an der Annahme, dass postcard am Ende überzählige Bytes erlaubt; die erste Aufgabe prüft das mit einem
  Test, bevor darauf gebaut wird.

## Complexity Tracking

Keine Abweichungen von der Constitution.
