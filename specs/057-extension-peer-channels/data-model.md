# Data Model: Kanäle zwischen Erweiterungen auf verschiedenen Vaults

**Keine Migration, keine neue Tabelle.** Alles dieser Spec lebt im Arbeitsspeicher eines Geräts und
endet mit der Vault-Sitzung (FR-042). Dauerhaft bleibt nur die Berechtigung `peers` in den
bestehenden Tabellen `extension_permissions` bzw. `dev_extension_permissions_no_sync`
(`extensions/permissions/store.rs:37-38`). Gelesen werden: Erweiterungen (`extensions`), Kontakte und
Karten (`haex_contacts_cards`, `haex_contacts_blocked_vaults`), Gerätelisten (`device_lists`).

Der Zustand liegt in `ExtensionHost.peers: PeerState` (`extensions/peers/state.rs`), je Erweiterung
über `extension_id` (UUID der Registrierung) zugeordnet. Kennungen an die Erweiterung sind zufällige
UUIDv4 als Text; holzi prüft bei jedem Aufruf, dass die Kennung zur aufrufenden Erweiterung gehört
(FR-006).

## Erweiterungsidentität auf dem Draht

`ExtensionRef { public_key: String (64 Hex), name: String }` aus dem Manifest (Spec 017: eindeutig
durch Herausgeberschlüssel und Name). Auf der empfangenden Seite → `extension_id` über `extensions`
(`state = 'installed'`); eine Entwicklungsfassung zählt mit ihrem Manifest-Schlüssel und Namen, wenn
der Entwicklermodus auf diesem Gerät an ist (wie `dispatch::is_enabled`).

## Gegenseite (`Remote`)

| Feld            | Regeln                                                  |
| --------------- | ------------------------------------------------------- |
| `endpoint_id`   | `connection.remote_id()`                                |
| `device_pubkey` | aus dem Handshake (research R4)                         |
| `vault`         | Vault-Identität der Gegenseite                          |
| `trust`         | `OwnVault` \| `Contact { contact_id }` \| `Unverified`  |
| `card_name`     | Name von der Visitenkarte der Gegenseite, falls Kontakt |

`trust` wird einmal im Handshake bestimmt. Wird die Vault später blockiert oder ihre Karte gelöscht,
schließt holzi offene Kanäle zu ihr (research R12).

## Kanal (`Channel`)

| Feld           | Regeln                                                           |
| -------------- | ---------------------------------------------------------------- |
| `id`           | UUIDv4; Kennung für die Erweiterung (`channelId`)                |
| `extension_id` | gebundene Erweiterung                                            |
| `remote`       | `Remote`                                                         |
| `origin`       | `Invitation { invite }` \| `Lobby { lobby }`                     |
| `connection`   | die QUIC-Verbindung; Datagramme = schnelle Nachrichten           |
| `control`      | Sende-Hälfte des Steuerstreams (gesicherte Nachrichten, `Close`) |
| `state`        | `Open` → `Closed { reason: closed \| lost \| timeout }`          |

Zustandsübergänge: `Open` entsteht nur nach angenommener Einladung (`InviteConfirm`) oder zugelassenem
`Join` (FR-016, FR-024). `Closed` ist endgültig; die Meldung `peers:channel-closed` geht genau einmal
hinaus (FR-040), danach wird der Eintrag entfernt. Grenze: 16 je Erweiterung und Gerät.

## Lobby (`Lobby`, nur beim Gastgeber)

| Feld           | Regeln                                                                           |
| -------------- | -------------------------------------------------------------------------------- |
| `key`          | 16 zufällige Bytes; steht im Beitrittscode (research R6)                         |
| `lobby_id`     | Kennung der Erweiterung, 1–64 Zeichen `[A-Za-z0-9._-]`; eindeutig je Erweiterung |
| `extension_id` | gebundene Erweiterung                                                            |
| `visibility`   | `Contacts` (Standard) \| `Public`                                                |
| `secret`       | 16 zufällige Bytes, `Zeroizing`; verlässt das Gerät nur im Code                  |
| `banned`       | Menge aus Endpoint-IDs und Vault-Identitäten (bestätigter Entfernter)            |
| `channels`     | Kanäle, die aus dieser Lobby entstanden sind                                     |

Zustand: offen, solange der Eintrag existiert. `extension_peers_lobby_close` entfernt ihn; der Code ist
sofort unwirksam, `channels` bleiben offen (FR-022). Gibt es keinen Eintrag mit `visibility = Public`
mehr, nimmt das Gerät keine nicht bestätigten Verbindungen mehr an (FR-031). Grenze: 4 je
Erweiterung und Gerät.

## Beitrittscode (`JoinCode`)

`haex-vault:join:` + Base32 ohne Auffüllung von `postcard({ v: 1, endpoint_id, relay: Option<String>,
lobby: key, secret })`. Erzeugt bei `lobby_open`, nie gespeichert. Siehe
[contracts/wire-protocol.md](./contracts/wire-protocol.md).

## Ausgehende Einladung (`OutgoingInvite`, einladende Seite)

| Feld           | Regeln                                                                                                       |
| -------------- | ------------------------------------------------------------------------------------------------------------ |
| `id`           | UUIDv4 (`inviteId`), auch auf dem Draht                                                                      |
| `extension_id` | einladende Erweiterung                                                                                       |
| `contact_id`   | eingeladener Kontakt (von `contacts` der Erweiterung umfasst, research R12)                                  |
| `payload`      | JSON ≤ 4 KiB                                                                                                 |
| `expires_at`   | jetzt + 5 min                                                                                                |
| `devices`      | je Kartengerät: `Dialing` → `Waiting` \| `Unreachable` \| `Declined` \| `NotInstalled`                       |
| `state`        | `Open` → `Accepted { channel }` \| `Declined` \| `NotInstalled` \| `Unreachable` \| `Expired` \| `Withdrawn` |

Regeln (FR-017, FR-018): Das erste `InviteReply::Accepted` gewinnt → `InviteConfirm` an dieses Gerät,
`Withdraw` an alle anderen. `Declined`/`NotInstalled` erst, wenn kein Gerät mehr `Dialing` oder
`Waiting` ist und alle erreichten so antworteten (gemischt `Declined`/`NotInstalled` → `Declined`);
`Unreachable`, wenn keines erreicht wurde (mit `hint: share_current_card`). `Withdrawn` entsteht nur
durch die Erweiterung selbst oder `end_all` und wird nicht als Status gemeldet. Grenze: 16 offene je
Erweiterung.

## Eingehende Einladung (`IncomingInvite`, eingeladene Seite)

| Feld           | Regeln                                                                             |
| -------------- | ---------------------------------------------------------------------------------- |
| `id`           | `inviteId` der einladenden Seite                                                   |
| `extension_id` | dieselbe Erweiterung auf diesem Gerät                                              |
| `remote`       | `Remote` mit `trust = OwnVault` ist ausgeschlossen, `Contact` Pflicht              |
| `payload`      | JSON ≤ 4 KiB; erreicht die Erweiterung erst nach der Annahme                       |
| `prompt`       | `InApp` \| `Notification(Box<dyn ShownNotification>)`                              |
| `state`        | `Asking` → `Accepted` (wartet auf `InviteConfirm`) → Kanal \| `Declined` \| `Gone` |

Grenzen: 4 offene je fremder Vault, 16 insgesamt (research R10). `Gone` bei `Withdraw`, Ablauf oder
Ende der Verbindung; Abfrage und Benachrichtigung verschwinden (FR-015).

## Wartende Zustellung (`PendingOpen`)

Ein angenommener Kanal, dessen Erweiterung noch keinen Rahmen hat: `channel`, die wartenden Meldungen
(zuerst `peers:channel-opened`, danach die Nachrichten des Kanals, höchstens 256). Geht hinaus, sobald ein
Rahmen der Erweiterung seine erste Anmeldung meldet (`extension_frame_reloaded`); nach 60 s ohne Rahmen
schließt holzi den Kanal mit `closed` (research R9).

## Drosselung (`Throttle`)

| Feld                | Regeln                                                                                                                                      |
| ------------------- | ------------------------------------------------------------------------------------------------------------------------------------------- |
| `pending`           | Zahl nicht zugelassener, nicht bestätigter Verbindungen, höchstens 16; bestätigte Kontakte und eigene Geräte haben eine getrennte Kapazität |
| `per_endpoint`      | `endpoint_id → { attempts_in_window, failed_joins, banned_until }`, höchstens 1.024 Einträge                                                |
| `unverified_window` | neue nicht bestätigte Verbindungen im laufenden 10-s-Fenster, höchstens 20                                                                  |

Werte und Begründung in research R7. Die Uhr ist injiziert, damit Tests ohne `sleep` laufen.

## Gesehene Gerätelisten (`SeenGenerations`)

`vault → höchste in dieser Sitzung gesehene Generation`; Untergrenze zusätzlich
`card.list_generation` (research R4). Nur im Arbeitsspeicher.
