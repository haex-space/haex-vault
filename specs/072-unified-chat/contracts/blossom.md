# Contract: files on a Blossom-Server

Research [R11, R12, R13](../research.md). HaexVault is a Blossom client (BUD-01, BUD-02, BUD-06).

## Encrypted file format `HXCF` v1

```text
magic "HXCF" | version u8 = 1 | segment_size u32 BE (1 MiB) | nonce_prefix [u8; 8]
segment_0 … segment_n      each: AES-256-GCM(key, nonce = prefix ‖ u32 BE index,
                                             aad = "HXCF1" ‖ u8 last_flag, plaintext ≤ segment_size)
```

- One random 256-bit key per file; the key travels only inside the encrypted rumor.
- The receiver rejects a missing last segment, a reordered segment, or a plaintext whose SHA-256 differs
  from the rumor's `sha256`.

## Upload

1. `HEAD /upload` with `Authorization: Nostr <base64url-no-pad(event)>` and headers `X-SHA-256`,
   `X-Content-Length`, `X-Content-Type: application/octet-stream` (BUD-06/BUD-11). The signed kind
   24242 event has `t=upload`, an `x` tag for the ciphertext hash, an `expiration` tag, and human-readable
   content. A refusal (`4xx` with `X-Reason`) is shown to the user (`blossom:<status>`), the file is not
   sent via Blossom.
2. `PUT /upload`, body = HXCF ciphertext (streamed), header `Authorization: Nostr <base64url-no-pad(event)>`:

   ```json
   { "kind": 24242, "content": "Upload encrypted chat file", "created_at": <now>,
     "tags": [["t","upload"], ["x","<ciphertext sha256>"], ["expiration","<now + 10 min>"]] }
   ```

   signed by a fresh per-blob key, or by the user's Blossom upload key if one is set (R12).

3. The response's blob descriptor `url` is used as `blossom.url`; its `sha256` must equal the computed one.

## Download

`GET <url>`; verify ciphertext SHA-256, decrypt, verify plaintext SHA-256. `404` → "Datei nicht mehr
verfügbar".

## Delete

`DELETE /<sha256>` with `Authorization: Nostr <base64url-no-pad(event)>`; its kind 24242 event has
`content: "Delete expired chat file"`, `t=delete`, `x=<sha256>`, and an `expiration` tag. It is signed by
the same credential that signed the upload: the per-blob key in `chat_blobs_no_sync`, or the configured
key loaded from password-manager entry `chat.blossom.keyEntry`. Delete only when `pending_devices` is
empty (every recipient device and the sender's other devices sent a `files` receipt), or in the 30-day
sweep (R13). A failed delete is retried at the next sweep.

## HTTP client

The Blossom request wrapper must reject every non-HTTPS URL before using the pinned client (the test
server may use `http://127.0.0.1` only), disable redirects and proxies, and enforce 30 s connect / 5 min
per-transfer timeouts even if the shared pinned client has no timeout. The plaintext cap is 100,000,000
bytes (100 MB);
the ciphertext transfer cap includes the HXCF header and one 16-byte GCM tag per 1 MiB segment:
`plaintext_bytes + 17 + 16 * ceil(plaintext_bytes / 1 MiB)`.
