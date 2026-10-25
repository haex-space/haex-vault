# Contract: Änderung der Tauri-Befehle

Ändert [Spec 024 contracts/tauri-commands.md](../../024-own-device-sync/contracts/tauri-commands.md). Kein neuer
Befehl; `list_vault_devices` liefert mehr.

```ts
interface DeviceSchema {
  protocol: number // Sync-Protokoll
  holziMigration: number // Vault-Schema (Zahl der Migrationen von holzi)
  crdtTrigger: number // Änderungsverfolgung (Version der CRDT-Trigger)
}

interface VaultDevice {
  // … wie bisher (vaultDeviceUuid, devicePubkey, alias, role, isCurrent, online, lastSeen, problem)
  schema: DeviceSchema | null
}
```

- Bei `isCurrent` der Stand des laufenden holzi, nie `null`.
- Bei anderen Geräten der zuletzt im Handshake gesehene Stand; `null`, wenn dieses Gerät noch keinen gesehen hat.
- Ereignis `sync-devices-changed` wie bisher; es feuert zusätzlich, wenn sich die Werte eines Geräts ändern.
- Die Agenten-Aktion `settings.devices.list` (Spec 020, `settings.read`) liefert `schema` je Gerät mit
  `{ protocol, holziMigration, crdtTrigger }`, bei unbekanntem Stand ohne das Feld (FR-011). Die
  Erweiterungs-Schnittstelle (Spec 017) bekommt die Werte nicht.
