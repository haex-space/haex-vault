# Test vault from before the rename

`pre_rename_vault.db` is a vault file as the app wrote it before it was renamed, with invented
content only. `tests/pre_rename_vault.rs` opens a copy of it through the file import and checks that
the data is still there. The test names no name that is stored in the vault, so it stays valid
through the rename; when it fails, a name stored in the vault no longer matches what the code looks
for.

- Passphrase: `pre-rename-test-passphrase` (invented, protects nothing).
- Folder `Work`; the entry `Mail` in it (user `alice@example.org`, password `pre-rename-mail-secret`,
  note, address, one-time secret `JBSWY3DPEHPK3PXP`, tags `work` and `mail`, one custom field
  `PIN` = `4711`).
- The entry `Router` (user `admin`, password `pre-rename-router-secret`) with one attachment
  `router-notes.txt`.
- The entry `Old shop` (user `bob`, password `pre-rename-shop-secret`) in the trash.
- One contact `Carol Example`, made in the app, with an e-mail address and the three addresses of
  the app (vault identity, Nostr server, iroh endpoint).
- One chat provider (the local one), the device rows and the signed device list of the first device.

It was written by a one-off test on the last commit before the rename (the generator is not kept,
it needs the old names) and is not meant to be regenerated.
