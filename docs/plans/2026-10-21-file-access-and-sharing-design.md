# File access between devices and sharing with contacts

**Status**: Design agreed with the operator on 2026-10-21; to be specified as spec 056.

**Replaces** the spaces draft [027](../../specs/027-spaces/spec.md) and the former 029 own-S3 draft,
and the "spaces as a frame of entries" rework agreed on 2026-10-07 ([file browser design](./2026-10-07-file-browser-design.md),
context table). Both drafts are replaced by spec [056](../../specs/056-file-access-sharing/spec.md).
[025](../../specs/025-own-device-file-sync/spec.md) stays as the
draft for later sync rules (mirroring). Supersedes in part
[`2026-09-28-sync-architecture.md`](./2026-09-28-sync-architecture.md) §8 (spaces).

## Context

The operator wants to reach files on every own device from every other own device, and to share
single folders and files with other people (contacts, spec 051) and revoke that per person.

Spaces solved sharing with a group key and encrypted objects stored at third parties (relay, S3), so
that content arrives while the sender is offline. Both choices force a new key generation whenever
a member leaves. The operator accepts that files are only reachable while both vaults are open.
A shared S3, WebDAV or SFTP source still contains its source data at that provider, but this feature
creates no additional copy or share ciphertext at a third party. Without such feature-created
ciphertext and without a group key, no content keys are needed at all: the iroh connection is
encrypted and authenticated with device keys, and revoking means the serving device stops answering.

## Decisions

| #   | Question                         | Decision                                                                                                                                                                                                                                                                                                                                                           |
| --- | -------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| 1   | Spaces?                          | No. Folders and files are shared with single contacts. Several contacts per share, revoked individually. No keys to roll out.                                                                                                                                                                                                                                      |
| 2   | Mirror or live access?           | Live access, like a network drive. A share shows up as a storage in the file browser of the recipient (next to S3, WebDAV, SFTP). Nothing is stored at the recipient unless they copy it out. No file index, no reconciliation, no conflict copies.                                                                                                                |
| 3   | Rights for contacts              | Two levels: read, or read and write. Write covers create, change, rename, move and delete inside the share. Deleting on desktop goes to the owner's system trash (spec 044 FR-023).                                                                                                                                                                                |
| 4   | Which own device serves a share? | Exactly the device whose storage holds the source. The share definition syncs to all own devices (spec 024) and can be managed from any of them; a change takes effect once the serving device has it. Until then the UI shows "revocation waits for ‹device›".                                                                                                    |
| 5   | How does a recipient learn of it | Live query, no announcement message. The recipient's device asks the contact's devices which shares exist for its vault; a found share is remembered (and synced to the recipient's own devices).                                                                                                                                                                  |
| 6   | What can be shared               | Everything the file browser shows: local folders and files, encrypted folders (spec 048), S3, WebDAV, SFTP. The owner's device decrypts or proxies; credentials and keys never leave it.                                                                                                                                                                           |
| 7   | Own devices                      | Every own device reaches the whole file system of every other own device, read and write. No share definition: entitled is every device on the vault's current device list. On by default, switchable off per device ("share this device's files with my other devices", local, not synced).                                                                       |
| 8   | Contacts                         | A foreign vault sees only the roots shared with its vault identity, with the granted right.                                                                                                                                                                                                                                                                        |
| 9   | Protocol                         | Own iroh protocol for file operations, no iroh-blobs (hashing before every read, a full download for remote sources) and no WebDAV tunnel (a WebDAV server to build, rights mapped onto HTTP auth).                                                                                                                                                                |
| 10  | Finding the serving device       | Knock over Nostr: a short-lived gift wrap (as presence in spec 024) to the owner's device keys on the relays from the business card. The owner's device answers with its current address only if it shares something with that vault identity. No public iroh address discovery. _Changed 2026-10-23: dialed from the card like spec 057, see ADR 0015 amendment._ |
| 11  | Agents                           | As for remote storages (specs 038, 050): permission per storage on the requesting device. Rules of spec 044 apply (read yes, write through the approval gate, holzi's own data blocked).                                                                                                                                                                           |

Accepted risk (decision 7): a lost or compromised linked device can read and write the whole file
system of every other own device while that device runs with the vault open. The per-device switch
limits it; an ADR records it during planning.

## Model

**Share** (owner): id, display name, source (storage of the file browser plus path; a folder or a
single file), serving device, grants (vault identity of a contact, right). Synced via spec 024.

**Received share** (recipient): contact, share id, name, right, serving device of the owner. Created
on the first successful query, synced via spec 024.

**Who may connect**: entitlement belongs to a vault identity, not a device. On connect the
requesting device presents its vault's device list signed with the vault identity (spec 024). The
serving device checks the signature against the vault identity of the contact and that the
requesting device is on that list. A list with a lower generation than the last one seen is
refused. A contact's new phone works after the contact's business card has been refreshed; a removed
device loses access once a newer list was seen.

## Protocol

A second ALPN `haex-vault-files/1` on the iroh endpoint of the vault session, next to `holzi-sync/1`.

Handshake: device key, then the signed device list. Classification:

- own vault and the switch is on → mode "own device", the whole file system;
- vault identity of a contact with at least one active share on this device → mode "contact", only
  its roots;
- otherwise → refuse without revealing whether shares exist.

Operations: `shares` (roots for this caller), `list`, `stat`, `read(offset, len)` streamed and
resumable, `write` (into a temp file, then atomic replace), `mkdir`, `rename`/`move` within one
root, `delete`.

`write` on an existing file carries a mutation-safe expected version (the server-issued/content
identity `etag`; size and modification time are informational only). If the file changed meanwhile
the expected identity differs and the answer is "conflict"; the recipient chooses to overwrite or
save as a copy.

Every request re-checks the grant and the path. Paths are canonicalized; `..`, symlinks and
junctions leading out of the root are refused, for read, write, rename and move alike. Revoking a
grant aborts running streams of that contact.

## User interface

Owner:

- "Share …" in the context menu of a folder or file: pick contacts (only those with confirmed holzi
  addresses from a valid business card), right per contact; existing grants are shown and can be
  changed or revoked in the same dialog.
- Overview "My shares" in the file browser: source, serving device, contacts with rights, status
  (active / source missing / revocation waits for ‹device›).
- Contact page (spec 051): "Shares with this contact".
- Device settings: the switch of decision 7.

Recipient, file browser sidebar:

- "My devices": every other own device as a storage, online or unreachable.
- "From contacts": expanding a contact dials its devices from the card (ADR 0015 amendment); its shares appear as storages. Known shares stay
  visible but greyed out while the contact is unreachable.
- Read-only shares hide write actions. Copying out of a share into own storages always works.
- Write conflict dialog as above.

## Error cases

- Serving device offline or vault locked: "unreachable"; running transfers abort without a partial
  file (spec 044 FR-026), temp files are removed.
- Grant revoked during a transfer: immediate abort, same cleanup.
- Remote source at the owner fails (S3 down, encrypted folder locked): "source at the owner not
  available", no internal details.
- Contact deleted at the owner: its grants go away; the delete dialog says so.
- Connection without entitlement: closed in the handshake without a reason (ADR 0015 amendment; the knock was dropped).

## Testing

- Unit tests (`*_tests.rs`): path containment with symlink and `..` cases, handshake
  classification, grant check per request, version conflict on write.
- Integration: two vaults in one process on the test support of spec 024: contact reads and
  writes, revocation mid-stream, own device with the switch off.
- E2E: multi-device setup of spec 033 extended by a second vault.

## Out of scope

Expiry dates, contact groups, offline copies (mirroring), notification of a new share, share links
for people without holzi. Mirroring and cloud backup belong to the later sync rules (025).

## Follow-ups with spec 056

- Delete spec 029 once 056 is in use (027 stays for 028); update the references to them (24 documents) and to "045 sync rules" /
  "rework 027/029" in specs 044 and 048.
- Mark §8 of the sync architecture as superseded.
- Spec 026 (blind relay) is no longer needed for file sharing; whether it is needed for anything
  else is not decided here.
- Spec 028 (data shares of extensions) is unaffected.
