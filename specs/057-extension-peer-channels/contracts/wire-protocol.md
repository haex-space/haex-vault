# Contract: iroh-Protokoll `haex-vault-ext-peer/1`

ALPN `haex-vault-ext-peer/1` (`sync/wire.rs`, `EXT_PEER_ALPN`; schon mit dem Präfix nach Spec 054) am Endpoint der Vault-Sitzung (research R1). **Eine QUIC-Verbindung je Kanal.** Der
erste Bi-Stream, vom Wählenden geöffnet, ist Handshake und danach Steuerstream; schnelle Nachrichten
sind QUIC-Datagramme.

## Rahmen auf dem Steuerstream

- `u32` BE Länge ‖ postcard (`sync::wire::write_value`/`read_value`).
- Höchstens 64 KiB vor der Zulassung, danach 64 KiB + 64 Byte. Größer → Verbindung schließen
  (Code `PROTOCOL`).
- Jede Nachricht trägt `v: 1`; eine unbekannte Version oder Nachricht → schließen.

## Handshake (gemeinsamer Baustein `sync/foreign_auth.rs`, research R4)

```text
Wählender   → Hello { v, vault: [u8;32], device_pubkey: [u8;32],
                      list_payload: Vec<u8>, list_sig: [u8;64], nonce: [u8;32] }
Annehmender → Hello { … eigene Werte … }
Wählender   → Proof { v, sig: [u8;64] }
Annehmender → Proof { v, sig: [u8;64] }
```

`sig` mit dem Geräteschlüssel, `Domain::ExtPeerAuth` (`haex-vault-ext-peer-auth/v1`), über
`lp(nonce_dial) ‖ lp(nonce_accept) ‖ lp(own endpoint) ‖ lp(other endpoint) ‖ lp(vault) ‖ lp(alpn)`.

Prüfung je Seite, jede Abweichung → schließen ohne Grund:

1. `device_list::check_pushed(list_payload, list_sig, vault)` gelingt.
2. `device_pubkey` steht auf der Liste, nicht entfernt, `endpoint_id == connection.remote_id()`.
3. `sig` gilt.
4. Einordnung: eigene Vault (eigene gültige Liste gilt) → `OwnVault`; gespeicherte Karte, nicht
   blockiert, Generation ≥ `card.list_generation` und ≥ gesehener → `Contact`; blockiert → schließen;
   sonst `Unverified`.
5. Annehmender mit `Unverified` und keiner offenen Lobby → schließen. Ist nur eine Lobby „nur
   Kontakte“ offen, darf er genau einen begrenzten `Join` senden; ein gültiger Beweis endet mit
   `NotAdmitted` (FR-031).

Der Handshake muss in 5 s fertig sein.

## Zulassung (erste Nachricht des Wählenden nach dem Handshake, binnen 5 s)

```text
Invite  { v, invite: [u8;16], extension: { public_key: String, name: String },
          payload: Vec<u8> /* JSON ≤ 4 KiB */, expires_at: u64 /* Unix-ms */ }
Join    { v, lobby: [u8;16], proof: [u8;32], extension: { public_key, name },
          display_name: String /* 1–64 */, payload: Vec<u8> /* JSON ≤ 4 KiB */ }
```

`proof = HMAC-SHA256(secret, lp(nonce_dial) ‖ lp(nonce_accept) ‖ lp(lobby))`, geprüft mit
`verify_slice` (research R6).

### Einladung

```text
Eingeladen → InviteReply { v, invite, status: Accepted | Declined | NotInstalled }
Einladend  → InviteConfirm { v, invite }        // nur an die erste Annahme
Einladend  → Withdraw { v, invite }             // an alle anderen, bei Rückzug und Ablauf
```

- `Invite` nur von `Contact`; sonst schließen ohne Antwort.
- Erweiterung fehlt, deaktiviert oder `peers` verweigert → `NotInstalled`, schließen.
- Die Verbindung bleibt offen, solange die Einladungsabfrage steht (bis `expires_at`).
- Nach `InviteConfirm` ist die Verbindung der Kanal.

### Beitritt

```text
Gastgeber → JoinReply { v, status: Accepted | Closed | NotAdmitted }
```

- `Closed`: unbekannte `lobby`, geschlossen, falscher `proof`, andere Erweiterung, blockiert,
  gedrosselt, Grenze erreicht. Gleicher Weg für alle Fälle (HMAC mit Ersatzgeheimnis bei unbekannter
  `lobby`).
- `NotAdmitted`: gültiger `proof`, aber Lobby „nur Kontakte“ und `Unverified`, oder entfernt.
- Die gleiche Antwortzeit für `Closed` gilt nur für Beitritte, die den Gastgeber erreichen. Ein
  unlesbarer Beitrittscode wird vorher beim Beitretenden mit `lobby_closed` abgewiesen und fällt
  nicht unter den hostseitigen Zeitvergleich.
- Nach `Accepted` ist die Verbindung der Kanal; sonst schließen.

## Kanal

```text
Reliable { v, data: Vec<u8> /* ≤ 64 KiB */ }
Close    { v, reason: Closed }
```

- Schnelle Nachrichten: QUIC-Datagramm, Inhalt = Bytes der Erweiterung, ≤ 1.024 Byte, ungerahmt.
- `Close` vor `connection.close(0, b"closed")`. Gründe beim Empfänger: `Close` oder
  `ApplicationClosed` → `closed`; `TimedOut` → `timeout`; sonst `lost`.
- Leerlauf 30 s, Keep-Alive 5 s (Grundwerte von iroh/noq).

## Beitrittscode

```text
"haex-vault:join:" + BASE32_NOPAD(postcard(JoinCode {
    v: 1, endpoint_id: [u8;32], relay: Option<String>, lobby: [u8;16], secret: [u8;16] }))
```

Ein unlesbarer Code scheitert beim Beitretenden mit `lobby_closed`, ohne zu wählen.

## Grenzen

Siehe research R14. Drosselung der Zulassung in research R7.
