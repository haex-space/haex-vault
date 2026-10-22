# The passkey service claims user presence only after a confirmation bound to one ceremony

Status: accepted
Date: 2026-10-22

## Decision

Supersedes ADR-0009 on one point: the authenticator data may now claim user
presence (UP) and user verification (UV), under the conditions below. Everything
else ADR-0009 says about the passkey service stays.

- **One confirmation per ceremony.** UP is set only after the user confirmed in
  holzi's window this one request: this client, this relying party, this origin
  and this challenge (the window shows the SHA-256 of the challenge). The
  confirmation is consumed by that request alone; it is never remembered, never
  reused and never granted ahead.
- **UV only with the vault password.** UV is set only when the relying party
  requires it (`userVerification: "required"`) and the user re-entered the vault
  password in that confirmation. The password is checked against the vault file
  the same way a passphrase change checks it, and kept nowhere. For `"preferred"`
  and `"discouraged"` holzi asks no password and sets UP alone.
- **The claim cannot come from outside.** The service takes the presence as a Rust
  type (`UserPresence`: `None`, `Present`, `Verified`) that cannot be deserialized.
  Only the caller that showed the confirmation creates it; today that is the
  External Bridge (spec 049). Without it the service keeps the flags of ADR-0009:
  no UP, no UV.
- **Nothing after the end.** Declining, a wrong password, the 120 s deadline and a
  closed connection end the request without a passkey, a signature or an entry.
- **A passkey may come with its entry.** "Neuer Eintrag" creates the entry and the
  passkey in one transaction; if the passkey fails, the entry is not created.

## Consequences

- Relying parties that require user presence or verification accept holzi's
  passkeys through the browser extension.
- The confirmation is a modal question per sign-in; there is no "remember".
- A future caller that wants to claim presence needs its own ceremony-bound
  confirmation; it cannot borrow the bridge's.

See `specs/049-external-bridge/research.md` R11, R12 and
`specs/049-external-bridge/contracts/`.
