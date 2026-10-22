# Secrets sit unencrypted in the vault database and are protected by grants

Status: accepted
Date: 2026-10-02

## Decision

The password manager (spec 034) stores passwords, TOTP secrets, private passkey
keys and the values of custom fields **as plain values in the vault database**.
There is no second encryption layer and no master password of the password
manager. This is the model of haex-vault 2.x (Legacy), and it keeps the data an ordinary,
synced, recoverable part of the vault.

What protects the secrets instead:

- **The vault encryption.** The database is encrypted at rest; a secret is
  visible only to the devices of the vault (spec 024).
- **A grant for every access from outside the window.** Every read and write
  goes through one `PasswordsService` that takes a caller and the caller's
  grants. A grant has a kind (`Read`, `ReadWrite`) and a scope (all entries or
  entries with a tag). Lists carry headers without secrets; a secret is
  returned only on a single request for one entry that a grant covers.
- **The caller is the entrance, never an argument.** A Tauri command fixes the
  caller (`User` for the window, `BuiltinAgent` for the chat action); a holzi
  function calls the service from Rust with a fixed grant; extensions and
  external agents get their own entrances with specs 017–019 and 021. No
  command takes an identity from the frontend.
- **The built-in agent never receives a secret.** It sees title, tags and
  folder name of an entry through one read-only action
  (`passwords.items.search`), no username and no URL, and no action reads,
  copies or changes a secret. No error, log line or chat entry carries one.
- **Secrets stay in the backend until the user asks.** Standard reads are
  masked; reveal, copy to the clipboard and the TOTP code are computed in Rust.

## Consequences

- A process that can read the unlocked vault database reads the secrets. The
  vault encryption and the vault lock are the only protection against it;
  an idle lock of the password manager alone is not part of this decision.
- The grant check is the single place that has to be right. It is a pure Rust
  module (`passwords/access.rs`) tested without a database, and the service is
  tested against a real vault with planted marker values.
- Spec 038 (storage connections) can keep its credentials as ordinary entries with
  a tag of its choice and read them as an internal caller; Spec 056 never exposes
  those credentials when it serves a shared source.
- Management of grants (issue, show, revoke) stays with specs 017–019 and 021;
  ADR-0005 stays reserved for that spec.

See `specs/034-password-manager/research.md` R6, R7 and R19 and
`specs/034-password-manager/contracts/access.md`.
