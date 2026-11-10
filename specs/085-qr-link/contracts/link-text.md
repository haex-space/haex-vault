# Contract: link text

```text
haex-vault://link?v=1&c=<CODE>[&n=<name>][&nr=<url>]…[&ir=<url>]…[&off=<url>]…
```

| Key   | Count     | Meaning                                                             |
| ----- | --------- | ------------------------------------------------------------------- |
| `v`   | exactly 1 | text version; this spec is `1`                                      |
| `c`   | exactly 1 | the link code, 26 Base32 characters A–Z 2–7, no dashes              |
| `n`   | 0..1      | vault name suggestion, at most 64 characters, percent-encoded UTF-8 |
| `nr`  | 0..10     | an added Nostr-Relay, `ws://` or `wss://`                           |
| `ir`  | 0..10     | an added iroh-Relay, `http://` or `https://`                        |
| `off` | 0..40     | a server switched off (built-in or added), either kind              |

Rules:

- Scheme and host compare case-insensitively; keys are lower case.
- Each URL at most 256 UTF-8 bytes, no whitespace, checked by `servers::validate` and `is_nostr_relay` / `is_iroh_relay`.
- Unknown keys are ignored within `v=1`. Exactly one numeric `v` is required; any numeric version other than `1` (including a
  future or earlier version) is `UnsupportedVersion`. A missing, duplicated, or non-numeric `v` is `NotALinkCode`.
- The text contains no passphrase, no key and no device secret; `c` is the only secret and keeps spec 024's lifetime.
- The text is the exact content of the QR code and of the "Copy link" action. Producer and parser are the same module.
- Plain-code input (no scheme) is accepted by the parser as `c` alone; it is never produced by a new main device.
- Maximum parsed length 49,152 UTF-8 bytes. This covers the worst case of 60 server URLs at the 256-byte limit after percent-encoding, the 64-character vault name, and the link syntax; anything longer is `NotALinkCode`.

Example (usual case, one added Nostr-Relay, one built-in switched off):

```text
haex-vault://link?v=1&c=ABCDEFGHIJKLMNOPQRSTUVWXY2&n=Privat&nr=wss%3A%2F%2Frelay.example.org&off=wss%3A%2F%2Fnos.lol
```
