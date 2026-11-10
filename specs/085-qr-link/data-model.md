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

| Field               | Change                                                                        |
| ------------------- | ----------------------------------------------------------------------------- |
| `code`, `expiresAt` | unchanged                                                                     |
| `qrSvg`             | now the QR of `text`, or `null` when the text is longer than 1,200 bytes (R7) |
| `text`              | new: the `haex-vault://link?…` string, complete                               |

## Camera gate (Rust, in memory)

One boolean, set by `camera_scan_begin` / cleared by `camera_scan_end`, read by the permission handler. Cleared when the main
window closes. Not persisted.

## State transitions (sheet)

`form` → (scan | paste | type) → `form` prefilled → submit → existing `LinkJoinState` machine (unchanged). The scanner is a
sub-state of `form`: `closed` → `starting` → `scanning` → `closed`.
