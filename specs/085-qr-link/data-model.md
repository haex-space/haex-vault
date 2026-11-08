# Data Model: Link a vault by scanning a QR code

Nothing is stored. The link text and its parsed form exist in memory only.

## LinkText (parsed, Rust `sync/link/text.rs`, exported to TS by ts-rs)

| Field            | Type                                                             | Rule                                                                                     |
| ---------------- | ---------------------------------------------------------------- | ---------------------------------------------------------------------------------------- |
| `code`           | link code (26 Base32 chars)                                      | required; parsed by `LinkCode::parse`; shown to the user as the display form with dashes |
| `vaultName`      | string, optional                                                 | at most 64 characters; a suggestion only                                                 |
| `servers`        | optional `SyncServers` (`nostrRelays`, `irohRelays`, `disabled`) | absent for a plain code; checked by `servers::validate`                                  |
| `droppedServers` | number                                                           | count of entries removed because they failed the server check or the limits; 0 normally  |

## LinkCodeInfo (host, existing, extended)

| Field               | Change                                                          |
| ------------------- | --------------------------------------------------------------- |
| `code`, `expiresAt` | unchanged                                                       |
| `qrSvg`             | now the QR of `text`                                            |
| `text`              | new: the `haex-vault://link?…` string, complete                 |
| `omittedServers`    | new: servers left out of the QR because of its size, 0 normally |

## Camera gate (Rust, in memory)

One boolean, set by `camera_scan_begin` / cleared by `camera_scan_end`, read by the permission handler. Cleared when the main
window closes. Not persisted.

## Device-local preference

`link.scan.cameraId` (string): the chosen camera of this device, kept in the device-local preferences store (never synced).

## State transitions (sheet)

`form` → (scan | paste | type) → `form` prefilled → submit → existing `LinkJoinState` machine (unchanged). The scanner is a
sub-state of `form`: `closed` → `starting` → `scanning` → `closed`.
