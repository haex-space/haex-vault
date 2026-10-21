# Vertrag: Drahtprotokoll der External Bridge

**Spec**: [../spec.md](../spec.md) | **Research**: R1 bis R4, R6, R16

Gegenstück: `haex-pass-browser` in `https://github.com/haex-space/haextension`, Revision
`ab4e0b0ac9a05488659836a7526a250beab9dfd9`. Die Pfade unten liegen unter
`apps/haex-pass-browser/src/`.

- Die Form der Nachrichten ist die der Vorlage haex-vault 2.x
  (`https://github.com/haex-space/haex-vault-legacy`, Revision
  `8dce379d94e18fcd42c3b73686a06f984ca3f574`, `src-tauri/src/external_bridge/protocol.rs`).
- Wo holzi abweicht, steht das ausdrücklich da.

## Transport

| Punkt           | Wert                                                                                     |
| --------------- | ---------------------------------------------------------------------------------------- |
| Adresse         | `ws://127.0.0.1:<port>` und `ws://[::1]:<port>`; der Client ruft `ws://localhost:<port>` |
| Port            | Standard `19455`, Einstellung des Geräts                                                 |
| Upgrade         | nur mit `Origin: chrome-extension://…` oder `moz-extension://…`, sonst HTTP 403 (R2)     |
| Rahmen          | nur Text, je Rahmen ein JSON-Objekt; Binärrahmen schließen die Verbindung                |
| Grenze          | 64 KiB je Nachricht                                                                      |
| Handshake-Frist | 10 s nach dem Upgrade                                                                    |

Jede Nachricht hat das Feld `type`. Feldnamen sind camelCase.

## Handshake (Client → holzi)

```json
{
  "type": "handshake",
  "version": 2,
  "client": {
    "clientId": "3f1c…(32 hex)",
    "clientName": "haex-pass Browser Extension",
    "publicKey": "<Standard-Base64, 32 Byte X25519>",
    "requestedExtensions": [
      { "name": "core", "extensionPublicKey": "__core__" }
    ],
    "permissions": {
      "core": { "passwords": [{ "target": "*", "operation": "readWrite" }] }
    }
  }
}
```

holzi prüft der Reihe nach:

1. `version < 2`, oder `permissions.core.passwords` fehlt oder ist leer: holzi schickt
   `{"type":"error","code":"PERMISSIONS_DECLARATION_REQUIRED","message":"…"}`, dann
   `handshakeResponse` mit `authorized: false, pendingApproval: false`, und schließt.
   - **Abweichung**: Die Vorlage nimmt auch Erklärungen ohne Passwörter an.
2. `clientId != hex(SHA-256(base64decode(publicKey))[0..16])`, oder `publicKey` ist nicht 32 Byte
   lang: `handshakeResponse(false, false)` und Schließen (FR-006).
   - **Abweichung**: Die Vorlage prüft das nicht.
3. Gesperrt, gemerkt oder vorläufig: `handshakeResponse(false, false)` und Schließen.
4. Zugelassen, mit passender Erklärung und Herkunft (Daten-Modell, Abgleich):
   `handshakeResponse(true, false)`.
5. Sonst: `handshakeResponse(false, true)` und ein Zulassungsdialog, höchstens einer je
   `clientId`.

Weitere Erklärungen als `passwords` (etwa `database`, `bookmarks`) speichert holzi in der
Erklärung, gewährt sie aber nie (R5). `requestedExtensions[].actions` werden übergangen.

Ein wiederholter Handshake auf derselben Verbindung mit derselben `clientId` bekommt den
aktuellen Zustand nach Schritt 3 bis 5 und keinen zweiten Dialog. Eine andere `clientId` auf
derselben Verbindung schließt sie.

## handshakeResponse (holzi → Client)

```json
{
  "type": "handshakeResponse",
  "version": 2,
  "serverPublicKey": "<Standard-Base64, 32 Byte X25519>",
  "authorized": false,
  "pendingApproval": true
}
```

`serverPublicKey` steht immer darin. Der Schlüssel gilt, solange die Bridge läuft (R3).

## authorizationUpdate (holzi → Client)

```json
{ "type": "authorizationUpdate", "authorized": true }
```

| Ereignis                               | Folge                                                                                                             |
| -------------------------------------- | ----------------------------------------------------------------------------------------------------------------- |
| Zulassen im Dialog                     | `authorized: true` an alle offenen Verbindungen des Clients                                                       |
| Ablehnen, Ablauf des Dialogs, Widerruf | `authorized: false`, dann Schließen. Laufende Anfragen enden ohne Antwort; der Client verwirft sie beim Schließen |

**Abweichung**: Die Vorlage schickt beim Widerruf nichts.

## ping / pong

`{"type":"ping"}` beantwortet holzi in jedem Zustand mit `{"type":"pong"}`.
WebSocket-Ping-Rahmen beantwortet `tokio-tungstenite` selbst.

## error (holzi → Client)

```json
{
  "type": "error",
  "code": "DECRYPTION_ERROR",
  "message": "Could not decrypt the request"
}
```

| Code                               | Wann                                                                  |
| ---------------------------------- | --------------------------------------------------------------------- |
| `PARSE_ERROR`                      | kein JSON oder unbekannter `type`                                     |
| `PERMISSIONS_DECLARATION_REQUIRED` | Handshake, Schritt 1                                                  |
| `UNAUTHORIZED`                     | `request` vor einem zugelassenen Handshake                            |
| `DECRYPTION_ERROR`                 | Umschlag nicht entschlüsselbar, Punkt kleiner Ordnung, falsche Längen |

Ein `error`-Rahmen trägt keine `requestId`; der Client lässt die betroffene Anfrage ablaufen.

## request (Client → holzi)

```json
{
  "type": "request",
  "action": "get-items",
  "message": "<Standard-Base64: Chiffrat ‖ GCM-Tag>",
  "iv": "<Standard-Base64, 12 Byte>",
  "clientId": "3f1c…",
  "publicKey": "<Standard-Base64, flüchtiger X25519-Schlüssel des Clients>",
  "extensionPublicKey": "__core__",
  "extensionName": "core"
}
```

- **Schlüssel**: `AES-256-GCM` mit dem rohen Ergebnis von `X25519(serverSecret, publicKey)`.
  Der Klartext ist die Nutzlast der Methode plus `requestId` (32 Hex-Zeichen).
- **Feld `clientId`**: holzi übergeht es. Der Aufrufer ergibt sich aus der Verbindung (FR-014).
- **Ziel**: Ist das Paar `extensionPublicKey`/`extensionName` ein anderes als
  `__core__`/`core`, lehnt holzi mit `NOT_SUPPORTED` ab (FR-029).

## response (holzi → Client)

```json
{
  "type": "response",
  "action": "get-items",
  "message": "<Standard-Base64>",
  "iv": "<Standard-Base64, 12 Byte>",
  "clientId": "",
  "publicKey": "<Standard-Base64, frischer X25519-Schlüssel von holzi>",
  "extensionPublicKey": null,
  "extensionName": null
}
```

- **Schlüssel**: `AES-256-GCM` mit `X25519(frischesSecret, Handshake-publicKey des Clients)`.
- **Klartext bei Erfolg**:

  ```json
  { "requestId": "…", "success": true, "data": {} }
  ```

- **Klartext bei Fehler**:

  ```json
  {
    "requestId": "…",
    "success": false,
    "error": "Not found",
    "errorCode": "NOT_FOUND"
  }
  ```

### Fehler auf Anfragen

`error` ist eine der festen Zeichenketten unten. Holzi reicht nie Text des Dienstes durch (R16).
Der Client zeigt nur `error`.

| `errorCode`                 | `error`                       | Wann                                                                                    |
| --------------------------- | ----------------------------- | --------------------------------------------------------------------------------------- |
| `INVALID_REQUEST`           | `Invalid request`             | Pflichtfeld fehlt oder ist unlesbar (etwa `url` bei `get-items`)                        |
| `NOT_FOUND`                 | `Not found`                   | nicht vorhanden **oder** außerhalb des Bereichs (FR-042), auch für `get-totp` ohne TOTP |
| `PERMISSION_DENIED`         | `Permission denied`           | Art fehlt und Schreiben nicht erklärt, gemerkte Ablehnung, Ablehnen der Rückfrage       |
| `PERMISSION_PROMPT_TIMEOUT` | `Permission prompt timed out` | Rückfrage nach FR-015 nach 120 s ohne Antwort                                           |
| `ORIGIN_MISMATCH`           | `Origin mismatch`             | Passkey ohne Herkunft oder mit unpassender Herkunft (FR-032)                            |
| `PASSKEY_EXCLUDED`          | `Passkey already registered`  | `excludeCredentials` trifft einen Passkey im Bereich                                    |
| `UNSUPPORTED_ALGORITHM`     | `Unsupported algorithm`       | RS256-Passkey beim Anmelden (036 FR-034)                                                |
| `USER_CANCELLED`            | `USER_CANCELLED`              | Anwesenheitsbestätigung abgelehnt, abgelaufen, falsches Vault-Passwort                  |
| `NOT_SUPPORTED`             | `Not supported`               | `update-item`, `bookmarks-*`, unbekannte Methode, anderes Ziel (FR-028, FR-029)         |
| `SERVER_ERROR`              | `Internal error`              | jeder andere Fehler; Einzelheiten nur im Protokoll ohne Inhalte                         |

`USER_CANCELLED` steht wörtlich in `error`. Die Erweiterung meldet der Seite dann
`NotAllowedError`, statt den Dialog des Browsers zu öffnen
(`contentScripts/webauthn-inject.ts:345-349`).

## Methoden

Je Methode die Art, die sie verlangt (FR-018), dann Eingabe und Ausgabe. Felder der Eingabe, die
hier nicht stehen, übergeht holzi (R6). Binärwerte sind Standard-Base64 mit Auffüllung.

### `get-items` — Lesen

Eingabe:

```json
{
  "url": "https://login.example.de/x",
  "fields": ["username", "password", "otp"]
}
```

Ausgabe:

```json
{
  "entries": [
    {
      "id": "…",
      "title": "Example",
      "url": "https://example.de",
      "fields": {
        "username": "anna",
        "password": "…",
        "otp": "TOTP",
        "PIN": "1234"
      },
      "hasTotp": true,
      "autofillAliases": { "username": ["email"] }
    }
  ]
}
```

- **Treffer**: Einträge mit derselben Site wie die angefragte Adresse (R7).
- **`fields` in der Eingabe** (optional): schränkt auf Einträge ein, die mindestens eines der
  Felder haben.
- **Werte in der Ausgabe**: Alle Werte in `fields` sind Zeichenketten. Felder, deren Verweis nicht
  auflösbar ist, fehlen.
- **`autofillAliases`**: genau die Form, die 034 speichert.

### `get-totp` — Lesen

| Eingabe              | Ausgabe                                |
| -------------------- | -------------------------------------- |
| `{ "entryId": "…" }` | `{ "code": "123456", "validFor": 17 }` |

### `create-item` — Lesen und Schreiben

| Eingabe                                                                              | Ausgabe                               |
| ------------------------------------------------------------------------------------ | ------------------------------------- |
| `{ "title": "Shop", "url": "https://shop.de", "username": "anna", "password": "…" }` | `{ "entryId": "…", "title": "Shop" }` |

- Alle vier Felder sind optional.
- Leerer Titel: der Host der Adresse, ohne Adresse `Untitled`.
- Der Eintrag bekommt das Standard-Tag und liegt in der obersten Ebene.

### `get-password-config` — Lesen

Eingabe: `{}`. Ausgabe:

```json
{
  "config": {
    "length": 20,
    "uppercase": true,
    "lowercase": true,
    "numbers": true,
    "symbols": true,
    "excludeChars": "",
    "usePattern": false,
    "pattern": ""
  },
  "presetName": "Standard"
}
```

Ohne Voreinstellung: `{ "config": null, "presetName": null }`.

### `get-password-presets` — Lesen

| Eingabe | Ausgabe                                                                                      |
| ------- | -------------------------------------------------------------------------------------------- |
| `{}`    | `{ "presets": [{ "id": "…", "name": "…", "isDefault": true, "config": { … wie oben … } }] }` |

### `passkey-create` — Lesen und Schreiben, mit Anwesenheitsbestätigung

Eingabe:

```json
{
  "relyingPartyId": "example.de",
  "relyingPartyName": "Example",
  "userHandle": "<b64>",
  "userName": "anna@example.de",
  "userDisplayName": "Anna",
  "challenge": "<b64>",
  "excludeCredentials": ["<b64>"],
  "requireResidentKey": true,
  "userVerification": "preferred",
  "origin": "https://login.example.de"
}
```

Ausgabe:

```json
{
  "credentialId": "<b64>",
  "publicKey": "<b64 SPKI>",
  "attestationObject": "<b64 CBOR>",
  "clientDataJson": "<b64>",
  "transports": ["internal"],
  "passkeyId": "…"
}
```

- **`requireResidentKey`**: Alle Passkeys von holzi sind auffindbar; das Feld wird gelesen und
  hat keine Wirkung.
- **`userVerification`**: Bei `"required"` fragt holzi das Vault-Passwort ab und setzt UV (FR-033).
- **`clientDataJson`**: `{"type":"webauthn.create","challenge":"<b64url der Aufgabe>","origin":"<origin>","crossOrigin":false}`.
- **`attestationObject`**: `fmt: "none"`, `attStmt: {}`, `authData` mit den Flags `UP|AT|BE|BS`
  (bei UV zusätzlich `UV`) und dem Zähler 0.

### `passkey-get` — Lesen, mit Anwesenheitsbestätigung

Eingabe:

```json
{
  "relyingPartyId": "example.de",
  "challenge": "<b64>",
  "allowCredentials": [
    { "id": "<b64>", "type": "public-key", "transports": ["internal"] }
  ],
  "userVerification": "preferred",
  "origin": "https://login.example.de"
}
```

Ausgabe:

```json
{
  "credentialId": "<b64>",
  "authenticatorData": "<b64>",
  "signature": "<b64 DER>",
  "clientDataJson": "<b64>",
  "userHandle": "<b64>",
  "passkeyId": "…"
}
```

- **`userHandle`**: nur bei auffindbaren Passkeys.
- **Signatur**: ES256, oder EdDSA bei importierten Passkeys, über `authData ‖ SHA-256(clientDataJSON)`.

### `passkey-list` — Lesen

| Eingabe                              | Ausgabe                                                                                                                                                                              |
| ------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `{ "relyingPartyId": "example.de" }` | `{ "passkeys": [{ "id", "credentialId", "relyingPartyId", "relyingPartyName", "userName", "userDisplayName", "nickname", "createdAt", "lastUsedAt", "isDiscoverable", "itemId" }] }` |

- Es kommen nur Passkeys im Bereich (FR-036), nie Schlüssel.
- `credentialId` ist Standard-Base64.

### Nicht unterstützt (FR-028)

`update-item`, `bookmarks-collections-list`, `bookmarks-collection-create`, `bookmarks-list`,
`bookmarks-upsert`, `bookmarks-delete` und `bookmarks-device-upsert` bekommen
`success: false, errorCode: "NOT_SUPPORTED"`. Es gibt keine Rückfrage und keine Wirkung, und die
Verbindung bleibt offen.

## Was der Vertragstest prüft

`src-tauri/tests/browser_bridge_contract.rs` (FR-041):

- Die Liste der Methoden in `browser_bridge::methods::METHODS` stimmt genau mit den acht Methoden
  oben überein.
- Die Liste der nicht unterstützten Methoden stimmt genau mit der Liste oben überein.
- Kein Handler-Modul liegt unter `::chat`, `::llm`, `::adapters` oder `::providers`.
