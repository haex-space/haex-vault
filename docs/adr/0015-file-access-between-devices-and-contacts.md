# File access between own devices and with contacts

Status: proposed
Date: 2026-10-21

## Context

Spec 056 replaces spaces (027, 029) for files. The operator accepts that files are only reachable
while both vaults are open (design `docs/plans/2026-10-21-file-access-and-sharing-design.md`). Two
consequences reach beyond the spec:

1. Until now a linked device could read and write the **vault data** of the other own devices
   (spec 024, D27). With spec 056 it reaches their **whole file system**: home directories, SSH keys,
   browser profiles, work files.
2. The iroh endpoint of a vault session so far accepted only devices of the own vault
   (`holzi-sync/1` refuses a foreign vault, `RejectCode::ForeignVault`). Spec 056 lets devices of other
   vaults connect.

## Decision

**Every device on the vault's current device list may read and write the whole file system of every
other own device, unless that device switched it off.** The switch is per device, on by default, and
stored only on that device (`haex_file_device_access_no_sync`), so no other device can turn it back
on. A remote own device sees exactly what the serving device's file browser shows, holzi's own data
included and with the same rules: read-only for the user, blocked for agents.

**Devices of other vaults connect over a separate ALPN, `haex-vault-files/1`, never over
`holzi-sync/1`.** They prove a device key on a device list signed by their vault identity, the vault
identity must belong to a contact with a verified business card, and they reach only the roots shared
with that identity, checked on every request; holzi's own data stays blocked for them. A serving device answers a Nostr knock only for an
entitled vault, so its address is not revealed to anyone else. Public iroh address discovery stays
off.

## Consequences

- A lost or compromised own device reaches the file system of every other own device that runs with
  the vault open and the switch on. Removing it from the device list (spec 024) ends that; the switch
  limits it in advance.
- `holzi-sync/1` keeps its guarantee that only own devices exchange vault data. `haex-vault-files/1` has
  separate modes: an own-vault device can access the whole file system, including holzi's own data
  under the read-only-for-users and blocked-for-agents rules; a contact device can access only its
  shared roots, and holzi's own data stays blocked.
- File contents served by this feature travel only over the direct iroh connection. A shared S3,
  WebDAV or SFTP source still contains its source data at that provider, but the feature creates no
  additional copy or share ciphertext there, so revoking needs no key rotation.
