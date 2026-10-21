# Vertrag: Anklopfen über Nostr

Nur für Kontakte; eigene Geräte finden sich über die Präsenz aus Spec 024 (research R9).

## Hülle

Wie die Präsenz (`sync/presence.rs:225` `wrap_payload`, `:283` `unwrap_rumor`):

- Gift-Wrap: äußere Art `21059` (flüchtig), NIP-44 v2 von einem Einmalschlüssel an den Empfänger,
  `p`-Tag = Empfänger.
- Siegel: Art 13, NIP-44 v2 vom Absender-Geräteschlüssel an den Empfänger, unterschrieben vom
  Absender-Geräteschlüssel.
- Innere Nachricht: unsigniert, Art unten, Inhalt JSON. Ereignisse über 16 KiB werden verworfen.

Der Empfänger ist ein **Geräteschlüssel**, nicht der Postfach-Schlüssel der Präsenz.

## 24103 Anklopfen

Absender: ein Gerät der anfragenden Vault. Empfänger: jedes Gerät auf der Visitenkarte des Kontakts
(`ContactCard.devices[].device_pubkey`). Relays: `ContactCard.nostr_relays`.

```json
{
  "v": 1,
  "vault": "<hex x-only>",
  "list": "<base64 list_payload>",
  "listSig": "<hex 64 Byte>",
  "replyRelays": ["wss://…"],
  "ts": 1760000000,
  "nonce": "<hex 16 Byte>"
}
```

`replyRelays` sind die eigenen `effective_nostr_relays()`, höchstens 10.

## 24104 Antwort

Absender: das Gerät des Eigentümers. Empfänger: der Siegel-Unterzeichner des Anklopfens. Relays:
`replyRelays`.

```json
{
  "v": 1,
  "endpoint": "<hex endpoint id>",
  "irohRelay": "https://…",
  "addrs": ["192.0.2.1:4433"],
  "ts": 1760000001,
  "nonce": "<nonce des Anklopfens>"
}
```

## Regeln beim Gerät des Eigentümers

1. Frisch: `ts` höchstens 150 s alt und höchstens 30 s in der Zukunft; eine schon gesehene `nonce`
   wird verworfen.
2. `listSig` gilt für `list` unter `vault`; der Siegel-Unterzeichner steht auf der Liste; Generation
   nach `haex_file_peer_lists_no_sync` (wie im Handshake).
3. `vault` hat eine gespeicherte, nicht blockierte Karte und mindestens ein Zugriffsrecht auf eine
   Freigabe, die **dieses** Gerät bedient.
4. Höchstens eine Antwort je `vault` in 5 s.
5. Sonst keine Antwort (FR-009).

## Regeln beim Anklopfenden

- Nur nach einer Handlung des Nutzers (Kontakt aufklappen, bekannte Freigabe öffnen; FR-028, FR-029).
- Antwort nur annehmen, wenn `nonce` passt, frisch ist und der Siegel-Unterzeichner auf der Karte des
  Kontakts steht; dann Adresse in die `MemoryLookup`, verbinden mit `holzi-files/1`.
- Ohne Antwort nach 10 s: „nicht erreichbar“.
