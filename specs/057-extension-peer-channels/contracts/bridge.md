# Contract: Brücke für Kanäle (Erweiterung ↔ holzi)

Methoden der Erweiterungs-Brücke (Spec 017, `extensions/bridge/allowlist.rs`) und Meldungen an die
Rahmen. PR 4 trägt sie in `specs/017-extension-host/contracts/bridge.md` ein (Tabelle der Methoden und
„Meldungen holzi → Erweiterung“); der Vertragstest `src-tauri/tests/extension_bridge_contract.rs` kennt
sie dann.

Alle Methoden laufen über `dispatch::call` (aktive Erweiterung), gelten für die aufrufende Erweiterung
(aus dem Rahmen, nie aus den Parametern) und prüfen die Berechtigung `peers`/`connect`/`*`. Bytes in
Parametern kommen als `{"$bytes": "<base64>"}` (017), in Meldungen als Base64-Text.

## Berechtigung

```json
"permissions": {
  "peers": [{ "target": "*", "action": "connect" }],
  "contacts": [{ "target": "*", "action": "read" }]
}
```

Art `peers`, Aktion `connect`, Ziel nur `*`. Wie jede Art aus dem Manifest bei der Installation
bestätigt oder zur Laufzeit erfragt (1004 mit `details: { resourceType: "peers", action: "connect",
target: "*" }`), vault-weit. `contacts` braucht nur, wer einlädt (research R12).

## Methoden

### `extension_peers_invite`

```text
params: { contactId: string, payload: JSON (≤ 4 KiB) }
result: { inviteId: string }
```

Kommt sofort zurück; das Ergebnis folgt als `peers:invite-status`. Geht an alle Geräte der
Visitenkarte zugleich (research R5, R10).

### `extension_peers_invite_cancel`

```text
params: { inviteId: string }
result: {}
```

Zieht eine offene Einladung auf allen Geräten zurück. Keine Statusmeldung danach. Bereits beendet →
`1001 invite_not_open`.

### `extension_peers_lobby_open`

```text
params: { lobbyId: string (1–64, [A-Za-z0-9._-]), visibility: "contacts" | "public" }
result: { joinCode: string }
```

Der Code ist für die Erweiterung undurchsichtig (`holzi:join:…`, research R6). Eine zweite offene
Lobby mit derselben `lobbyId` → 3001.

### `extension_peers_lobby_close`

```text
params: { lobbyId: string }
result: {}
```

Code sofort unwirksam, Kanäle bleiben (FR-022). Unbekannte `lobbyId` → `{}` (idempotent).

### `extension_peers_join`

```text
params: { joinCode: string, displayName: string (1–64), payload: JSON (≤ 4 KiB) }
result: { channelId: string, lobbyId: string }
```

Wartet höchstens 15 s auf die Antwort des Gastgebers. Fehler: `1001 lobby_closed` (unlesbar,
unbekannt, geschlossen, falsches Geheimnis, andere Erweiterung, blockiert, gedrosselt — ein Fall),
`1002 not_admitted` (Lobby „nur Kontakte“ und kein Kontakt, oder entfernt), `8001
network_unavailable` (Gastgeber nicht erreichbar oder Endpoint fehlt).

### `extension_peers_remove`

```text
params: { channelId: string }
result: {}
```

Nur für Kanäle aus einer eigenen Lobby: schließt und sperrt Gerät und, wenn bestätigt, Vault für diese
Lobby (FR-027). Sonst `1001 channel_unknown`.

### `extension_peers_send`

```text
params: { channelId: string, data: bytes, reliable?: boolean (default false) }
result: {}
```

`reliable: false` → schnelle Nachricht ≤ 1.024 Byte; `true` → gesicherte Nachricht ≤ 64 KiB. Größer →
`7000 too_large` mit `details.limit`. Geschlossener Kanal → `8001 channel_closed`.

### `extension_peers_close`

```text
params: { channelId: string }
result: {}
```

Beide Seiten erhalten `peers:channel-closed { reason: "closed" }` (die eigene Seite ebenfalls, genau
einmal).

Es gibt **keine** Methoden zum Annehmen oder Ablehnen einer Einladung: Das geschieht nur in holzis
Einladungsabfrage (Spec FR-016).

## Meldungen holzi → Erweiterung

An alle offenen Rahmen der Erweiterung auf dem Gerät (`bridge::events::emit_to_frames`), nicht flach
(`data` ist das Objekt).

| Typ                    | Daten                                                                                                                | Seite      |
| ---------------------- | -------------------------------------------------------------------------------------------------------------------- | ---------- |
| `peers:invite-status`  | `{ inviteId, status: "accepted" \| "declined" \| "not-installed" \| "unreachable" \| "expired", channelId?, hint? }` | einladend  |
| `peers:channel-opened` | `{ channelId, origin: "invitation", inviteId, from: { contactId?, name }, payload }`                                 | eingeladen |
| `peers:joined`         | `{ lobbyId, channelId, displayName, verified: boolean, contactId?, payload }`                                        | Gastgeber  |
| `peers:message`        | `{ channelId, data: base64, reliable: boolean }`                                                                     | beide      |
| `peers:channel-closed` | `{ channelId, reason: "closed" \| "lost" \| "timeout" }`                                                             | beide      |

- `status` kommt genau einmal je Einladung; `channelId` nur bei `accepted`; `hint: "share_current_card"`
  nur bei `unreachable` (Clarifications 2026-10-22).
- `peers:channel-opened` kommt erst nach der Annahme in holzis Abfrage und nach `InviteConfirm`. Ist kein
  Rahmen offen, öffnet holzi die Erweiterung, und die Meldung wartet bis zu ihrem Rahmen (research R9).
- `contactId` steht nur da, wenn die Berechtigung `contacts` der Erweiterung den Kontakt umfasst
  (051 FR-031); `name` ist der Name von der Visitenkarte der Gegenseite. `displayName` beim Beitritt ist
  selbst gewählt und ungeprüft.
- `peers:message` mit `reliable: false` darf im Puffer eines noch nicht bereiten Rahmens verworfen
  werden; müsste eine mit `reliable: true` verworfen werden, schließt holzi den Kanal mit `lost`
  (research R8).

## Fehler

Nur die festen Codes von `ExtensionErrorCode`; der Grund in `details.reason`:

| `code`                          | `details.reason`                        | Bedeutung                                                                             |
| ------------------------------- | --------------------------------------- | ------------------------------------------------------------------------------------- |
| 1002 `PermissionDenied`         | —                                       | `peers` verweigert                                                                    |
| 1004 `PermissionPromptRequired` | —                                       | holzi fragt; erneut nach `extension:permission-resolved`                              |
| 1001 `NotFound`                 | `contact_unknown`                       | Kontakt fehlt, außerhalb der Berechtigung `contacts` oder ohne Visitenkarte           |
| 1001 `NotFound`                 | `invite_not_open`, `channel_unknown`    | fremde, unbekannte oder beendete Kennung (gleich behandelt, FR-006)                   |
| 1001 `NotFound`                 | `lobby_closed`                          | Beitritt nicht möglich (einheitliche Antwort, FR-025)                                 |
| 1002 `PermissionDenied`         | `not_admitted`                          | nur Kontakte / entfernt                                                               |
| 7000 `LimitExceeded`            | `too_large`, `limit`                    | Nachricht oder Inhalt zu groß; Grenze für Kanäle/Lobbys/Einladungen (`details.limit`) |
| 8001 `NotAvailable`             | `channel_closed`, `network_unavailable` | Kanal zu; Endpoint der Vault fehlt oder Gastgeber nicht erreichbar                    |
| 3001 `Validation`               | `invalid_input`                         | Parameter unlesbar (Kennung, Anzeigename, Sichtbarkeit, kein JSON)                    |

Keine Meldung enthält Adressen, Schlüssel oder Daten anderer Erweiterungen (FR-043).

## Befehle für holzis Fenster (nicht für Erweiterungen)

| Befehl / Ereignis                          | Richtung       | Inhalt                                                           |
| ------------------------------------------ | -------------- | ---------------------------------------------------------------- |
| `extension-peer-invitation`                | Rust → Fenster | `{ inviteId, extensionId, extensionName, contactName }`          |
| `extension-peer-invitation-gone`           | Rust → Fenster | `{ inviteId, reason: "withdrawn" \| "expired" \| "taken" }`      |
| `extension_peer_invitation_answer`         | Fenster → Rust | `{ inviteId, accept: boolean }` → `{ result: "open" \| "gone" }` |
| `extension_peers_frame_overflow`           | Fenster → Rust | `{ frame, channelId }`                                           |
| `extension-notification-click` (bestehend) | Rust → Fenster | `{ extensionId }` — öffnet die Erweiterung nach der Annahme      |
