# Data model: Vault folders

## On disk

```text
<AppLocalData>/
  vaults.lock                  device-wide: exclusive while moving; shared while listing, opening, creating or importing vaults
  vaults/
    <name>/                    ^[A-Za-z0-9][A-Za-z0-9_-]{0,63}$
      vault.db                 SQLCipher database
      vault.db-wal, vault.db-shm
      vault.db.lock            held while the vault is open
      vault.db.vault-id        SHA-256 of the vault identity's public key, no secret
      vault.db.pending         exists only while the creation runs
      agent-browser/           the encrypted folder "Profil" (spec 065, 048)
      chat-files/              the encrypted folder of the chat files (spec 072, 048)
    .moving-<name>/            only during the move (R4); ignored by every reader
  filter-lists/                public filter lists and their download state (device-wide)
  models/  installation-id  inbox/   unchanged, device-wide
```

Rules: the root of `vaults/<name>/` holds only files named `vault.db*`; every other entry is a directory named for its purpose.
`vaults/<name>/` is the only place that may be named after a vault.

## Legacy places (read only by the move)

| Legacy                                                                                    | New                                            |
| ----------------------------------------------------------------------------------------- | ---------------------------------------------- |
| `instances/<name>.db` and `.db-wal`, `.db-shm`, `.db.lock`, `.db.vault-id`, `.db.pending` | `vaults/<name>/vault.db` and the same suffixes |
| `agent-browser/<name>/`                                                                   | `vaults/<name>/agent-browser/`                 |
| `chat-files/<name>/`                                                                      | `vaults/<name>/chat-files/`                    |
| `agent-browser/filters/` (list files only)                                                | `filter-lists/`                                |
| `agent-browser/filters/settings.json`, `agent-browser/dns.json`                           | rows in `preferences` (once), then removed     |

`instances/` is removed when it is empty after the move.

## Rows

| Table         | Key                                   | Value                  | Scope                                |
| ------------- | ------------------------------------- | ---------------------- | ------------------------------------ |
| `preferences` | `(nil UUID, "agentBrowser.trackers")` | `TrackerSettings` JSON | vault-wide, syncs                    |
| `preferences` | `(device UUID, "agentBrowser.dns")`   | `DnsSettings` JSON     | one device, read only by that device |

Both values are validated by the Rust types on read; a value that does not validate is treated as absent (defaults), as the
files are today.
