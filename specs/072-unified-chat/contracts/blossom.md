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

1. `HEAD /upload` with headers `X-SHA-256`, `X-Content-Length`, `X-Content-Type: application/octet-stream`
   (BUD-06). A refusal (`4xx` with `X-Reason`) is shown to the user (`blossom:<status>`), the file is not
   sent via Blossom.
2. `PUT /upload`, body = HXCF ciphertext (streamed), header `Authorization: Nostr <base64(event)>`:

   ```json
   { "kind": 24242, "content": "Upload", "created_at": <now>,
     "tags": [["t","upload"], ["x","<ciphertext sha256>"], ["expiration","<now + 10 min>"]] }
   ```

   signed by a fresh per-blob key, or by the user's Blossom upload key if one is set (R12).

3. The response's blob descriptor `url` is used as `blossom.url`; its `sha256` must equal the computed one.

## Download

`GET <url>`; verify ciphertext SHA-256, decrypt, verify plaintext SHA-256. `404` → "Datei nicht mehr
verfügbar".

## Delete

`DELETE /<sha256>` with a kind 24242 event `t=delete`, `x=<sha256>`, signed by the key that uploaded it
(kept in `chat_blobs_no_sync`). Triggered when all recipient vaults confirmed `files` (receipt), or by
the 30-day sweep (R13). A failed delete is retried at the next sweep.

## HTTP client

The pinned client (`remote_storage/http.rs`): HTTPS only (except `http://127.0.0.1` in tests), no redirects,
no proxy, timeouts 30 s connect / 5 min per transfer, ≤ 100 MB.
