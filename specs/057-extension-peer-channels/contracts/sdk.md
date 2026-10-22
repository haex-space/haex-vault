# Contract: `client.peers` im vault-sdk

Eigene Änderung in `haex-space/vault-sdk` (nicht Teil dieses Repositorys). Sie spricht die Methoden
und Meldungen aus [bridge.md](./bridge.md). Diese Seite hält fest, was das SDK anbietet, damit
haex-doom und das SDK gegen dieselbe Form bauen.

## Methoden

| SDK                                                                                | Brücke                          | Ergebnis                 |
| ---------------------------------------------------------------------------------- | ------------------------------- | ------------------------ |
| `inviteAsync(contactId: string, payload: unknown)`                                 | `extension_peers_invite`        | `{ inviteId }`           |
| `cancelInviteAsync(inviteId: string)`                                              | `extension_peers_invite_cancel` | `void`                   |
| `openLobbyAsync(lobbyId: string, visibility: "contacts" \| "public")`              | `extension_peers_lobby_open`    | `{ joinCode }`           |
| `closeLobbyAsync(lobbyId: string)`                                                 | `extension_peers_lobby_close`   | `void`                   |
| `joinAsync(joinCode: string, displayName: string, payload: unknown)`               | `extension_peers_join`          | `{ channelId, lobbyId }` |
| `removeAsync(channelId: string)`                                                   | `extension_peers_remove`        | `void`                   |
| `sendAsync(channelId: string, data: Uint8Array, options?: { reliable?: boolean })` | `extension_peers_send`          | `void`                   |
| `closeAsync(channelId: string)`                                                    | `extension_peers_close`         | `void`                   |

Konstanten: `peers.MAX_FAST_BYTES = 1024`, `peers.MAX_RELIABLE_BYTES = 65536`,
`peers.MAX_PAYLOAD_BYTES = 4096`.

## Ereignisse

`client.peers.on(type, handler)` für `invite-status`, `channel-opened`, `joined`, `message`,
`channel-closed` (Daten wie in bridge.md; `message.data` als `Uint8Array` dekodiert).

## Fehler

`PeerError` mit `code` (Zahl) und `reason`; Abbildung auf die Namen des Entwurfs von haex-doom:

| `reason`                            | Name im Entwurf          |
| ----------------------------------- | ------------------------ |
| `contact_unknown`                   | `PEER_CONTACT_UNKNOWN`   |
| `invite_not_open`                   | `PEER_INVITE_UNKNOWN`    |
| `channel_unknown`, `channel_closed` | `PEER_CHANNEL_CLOSED`    |
| `too_large`                         | `PEER_PAYLOAD_TOO_LARGE` |
| `lobby_closed`                      | `PEER_LOBBY_CLOSED`      |
| `not_admitted`                      | `PEER_NOT_ALLOWED`       |
| `limit`, `network_unavailable`      | neu                      |

## Unterschiede zum Entwurf von haex-doom (`contracts/peers-api.md` @ `a076b693`)

1. `acceptAsync` und `declineAsync` entfallen; angenommen wird nur in holzis Abfrage.
2. `peers:invitation` und `peers:invite-withdrawn` entfallen; neu ist `peers:channel-opened` nach der
   Annahme (mit `inviteId`, `from`, `payload`).
3. `from` bzw. `joined` tragen `contactId` nur, wenn die Berechtigung `contacts` den Kontakt umfasst;
   `from.name` ist der Name von der Visitenkarte, nicht `displayName` aus dem Adressbuch.
4. `peers:invite-status` hat optional `hint: "share_current_card"` bei `unreachable`.
5. Neu: `cancelInviteAsync`.
6. Schnelle Nachrichten höchstens **1.024** Byte (Entwurf: 1.100).
7. Fehler sind Zahlencodes mit `reason` statt eigener Codes (Tabelle oben).
8. Das Öffnen nach einer Benachrichtigung geschieht ohne `?invite=`; die Erweiterung erhält
   `channel-opened`, sobald ihr Rahmen bereit ist.
9. Der Beitrittscode beginnt mit `holzi:join:` (nach Spec 054 mit dem neuen Präfix), nicht
   `haexpeer1:`; er bleibt undurchsichtig.
10. Draht: postcard mit `u32` BE statt CBOR mit `u32` LE; ALPN `holzi-ext-peer/1`; Ausweisen über die
    unterschriebene Geräteliste statt nur über die gespeicherte Karte. Für die Erweiterung unsichtbar.
