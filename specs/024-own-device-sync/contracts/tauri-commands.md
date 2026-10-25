# Contract: Tauri-Befehle, Ereignisse und Aktionen

Alle Befehle laufen durch den Wrapper des Vault-Gates (Spec 013); außer `link_join_*` brauchen sie
eine aktive Vault-Session. Fehler kommen als `HolziError` mit festen Codes. Typen für die
Oberfläche erzeugt `ts-rs` wie bisher.

## Befehle

| Befehl                                     | Eingabe                                                                                                                                                      | Ergebnis                                                                          | Wer                                           |
| ------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------ | --------------------------------------------------------------------------------- | --------------------------------------------- |
| `sync_status`                              | –                                                                                                                                                            | `SyncStatus`                                                                      | jedes Gerät                                   |
| `list_vault_devices` (erweitert, Spec 023) | –                                                                                                                                                            | `VaultDevice[]`                                                                   | jedes Gerät                                   |
| `vault_public_identity`                    | –                                                                                                                                                            | `{ npub: string, hex: string }`                                                   | jedes Gerät                                   |
| `link_code_create`                         | –                                                                                                                                                            | `{ code: string, qrSvg: string, expiresAt: number }`                              | nur Hauptgerät                                |
| `link_code_cancel`                         | –                                                                                                                                                            | –                                                                                 | nur Hauptgerät                                |
| `link_confirm`                             | `{ asMainDevice: boolean }`                                                                                                                                  | –                                                                                 | nur Hauptgerät, nur während eines Verknüpfens |
| `link_reject`                              | –                                                                                                                                                            | –                                                                                 | nur Hauptgerät                                |
| `link_join_start`                          | `{ code: string, vaultName: string, deviceName: string, passphrase: string, servers?: { nostrRelays: string[], irohRelays: string[], disabled: string[] } }` | `{ state: 'waiting_for_confirmation' }`                                           | Startseite, ohne offene Vault                 |
| `link_join_status`                         | –                                                                                                                                                            | `LinkJoinState`                                                                   | Startseite                                    |
| `link_join_cancel`                         | –                                                                                                                                                            | –                                                                                 | Startseite                                    |
| `admission_decide`                         | `{ devicePubkey: string, admit: boolean }`                                                                                                                   | –                                                                                 | nur Hauptgerät                                |
| `device_remove`                            | `{ devicePubkey: string }`                                                                                                                                   | –                                                                                 | nur Hauptgerät, nicht für sich selbst         |
| `sync_copy_notice_dismiss`                 | –                                                                                                                                                            | –                                                                                 | jedes Gerät                                   |
| `sync_servers_defaults`                    | –                                                                                                                                                            | `{ nostrRelays: string[], irohRelays: string[], disabled: [] }` (die eingebauten) | jedes Gerät, auch ohne offene Vault           |
| `sync_servers_get`                         | –                                                                                                                                                            | `{ nostrRelays: string[], irohRelays: string[], disabled: string[] }`             | jedes Gerät                                   |
| `sync_servers_set`                         | `{ nostrRelays: string[], irohRelays: string[], disabled: string[] }`                                                                                        | –                                                                                 | jedes Gerät                                   |

„Nur Hauptgerät“ prüft das Backend (`NotMainDevice`), nicht nur die Oberfläche. `passphrase` in
`link_join_start` wird wie beim Anlegen einer Vault behandelt (`Passphrase`, nie protokolliert).
`servers` nennt die Server, über die sich die Geräte der Vault finden, wenn es nicht nur die
eingebauten sind: `nostrRelays` und `irohRelays` sind die zusätzlichen, `disabled` die abgeschalteten
(eingebaute oder zusätzliche, beider Arten); fehlendes `servers` heißt nur die eingebauten. Benutzt
wird alles Aufgelistete, was nicht abgeschaltet ist. Eingebaute lassen sich abschalten, nicht löschen;
zusätzliche abschalten oder löschen. Sind alle Nostr-Server abgeschaltet, finden sich die Geräte nicht;
sind alle iroh-Server abgeschaltet, verbinden sie sich nur direkt. Eine neue Installation kann die
Server nicht aus einer Vault lesen, die sie noch nicht hat; ohne diese Angabe fände sie ein
Hauptgerät mit eigenen Servern nicht. Die Verknüpfen-Maske der Startseite bietet dafür die Nostr-Server
an (iroh-Server dort nicht; die E2E-Szenarien mit eigenem Nostr-Relay nutzen die Angabe per Befehl).
Die eingebauten Server stehen dort wie in den Einstellungen immer in der Liste (`sync_servers_defaults`), mit einem Kästchen zum Abschalten.

### Typen

```ts
type DeviceRole = 'main' | 'linked'
type DeviceProblem = 'incompatible_version' | 'duplicate' | null

interface VaultDevice {
  vaultDeviceUuid: string
  devicePubkey: string
  alias: string | null
  role: DeviceRole
  isCurrent: boolean
  online: boolean
  lastSeen: number | null // ms; null = "noch nie online gesehen"
  problem: DeviceProblem
  // Spec 059: Versionsstand; bei isCurrent der des laufenden holzi, sonst der zuletzt im Handshake
  // gesehene, null solange dieses Gerät keinen sah.
  schema: DeviceSchema | null
}

interface DeviceSchema {
  protocol: number // Sync-Protokoll
  holziMigration: number // Vault-Schema (Zahl der holzi-Migrationen)
  crdtTrigger: number // Änderungsverfolgung (Version der CRDT-Trigger)
}

interface SyncStatus {
  thisDevice: 'main' | 'linked' | 'awaiting_admission' | 'removed'
  openAdmissions: { devicePubkey: string; name: string; requestedAt: number }[]
  linking: null | {
    stage: 'code_shown' | 'awaiting_confirmation'
    newDeviceName?: string
  }
  // Diese Vault-Datei ist die Kopie eines Hauptgeräts und dieses Gerät hat sich als Hauptgerät
  // eingetragen; die Nutzerin hat den Hinweis (FR-044) noch nicht gelesen.
  copyEnrolledAsMain: boolean
}

type LinkJoinState =
  | { state: 'searching' }
  | { state: 'waiting_for_confirmation' }
  | { state: 'transferring'; progress: number }
  | { state: 'done'; vaultName: string }
  | {
      state: 'failed'
      reason:
        | 'expired'
        | 'used'
        | 'wrong_code'
        | 'rejected'
        | 'incompatible'
        | 'connection_lost'
    }
```

## Ereignisse (Backend → Oberfläche)

| Ereignis                  | Nutzdaten               | Wann                                                                                                                                                                      |
| ------------------------- | ----------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `sync-devices-changed`    | –                       | Geräteliste, Online-Stand, Name, Problem oder Aufnahmeanfragen ändern sich (FR-034)                                                                                       |
| `vault-data-changed`      | `{ tables: string[] }`  | Tabellen des offenen Vaults wurden geschrieben, gleich von wem: lokal (ein Fenster, ein Agent, eine Erweiterung), von einem anderen Gerät empfangen, Neuabgleich (FR-032) |
| `link-host-state-changed` | `SyncStatus['linking']` | Fortschritt eines Verknüpfens auf dem Hauptgerät                                                                                                                          |
| `link-join-state-changed` | `LinkJoinState`         | Fortschritt eines Verknüpfens auf der neuen Installation                                                                                                                  |

`vault-data-changed` kommt aus `src-tauri/src/vault_events.rs`: haex-crdt meldet nach jedem
Commit die geänderten Tabellen (`Database::observe_committed_changes`), holzi fasst Meldungen
innerhalb von 50 ms zu einem Ereignis zusammen. Die Quelle einer Änderung spielt für die Ansicht
keine Rolle. Jede Ansicht, die Vault-Daten zeigt, meldet sich mit `onVaultTablesChanged(tabellen,
laden)` (`src/composables/useVaultData.ts`) für genau die Tabellen an, die sie zeigt, und lädt
still nach: ohne Ladeanzeige und ohne eine laufende Eingabe zu überschreiben. Wer neue Daten aus
dem Vault zeigt, meldet sich dort an; das Neuladen im Ablauf des Sync selbst ist nicht mehr nötig.

## Aktionen (Katalog, Spec 020)

| Aktion                              | Agenten            | Wirkung                                       |
| ----------------------------------- | ------------------ | --------------------------------------------- |
| `settings.devices.list` (erweitert) | ja (lesen, FR-036) | Geräteliste mit Rolle, online, zuletzt online |
| `settings.devices.identity`         | ja (lesen)         | öffentlicher Schlüssel der Vault-Identität    |
| `settings.devices.link`             | nein               | öffnet die Verknüpfen-Ansicht                 |
| `settings.devices.admit`            | nein               | Aufnahmeanfrage annehmen oder ablehnen        |
| `settings.devices.remove`           | nein               | Gerät entfernen (mit Rückfrage)               |
| `settings.sync.servers.set`         | nein               | Verbindungsserver ändern                      |

Keine Aktion gibt private Schlüssel, Verknüpfungscodes oder Umschläge heraus (FR-036).

## Orte (Spec 020, Einstellungen)

- `/federation` bleibt der Einstieg und zeigt die Unteransicht „Geräte“ (Spec 023 FR-022).
- `/federation/devices/link` — Code anzeigen, danach Bestätigung mit Rollenfrage.
- `/federation/devices/:devicePubkey/remove` — Rückfrage mit den Folgen (FR-026).
