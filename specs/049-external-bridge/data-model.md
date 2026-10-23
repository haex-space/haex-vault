# Data Model: External Bridge für haex-pass-browser

**Spec**: [spec.md](./spec.md) | **Research**: [research.md](./research.md)

Neu ist **eine** CRDT-Tabelle für gemerkte Zulassungen und Sperren und **zwei** Einstellungen des
Geräts. Vorläufige Zustände und alles, was zu einer Verbindung gehört, leben nur im Speicher der
Bridge (R13). Vorhandene Tabellen ändern sich nicht. Die Migration bekommt die nächste freie Nummer;
bei der Planung ist das `0038_browser_clients`. `HOLZI_TRIGGER_VERSION` steigt von 25 auf 26.

## Tabelle `haex_browser_clients` (CRDT, geräteeigen)

Eine Zeile je Client und Gerät. Geräteeigen nach ADR-0001: Der Fremdschlüssel auf `known_devices`
legt das Gerät fest, und die Bridge liest nur die Zeilen des eigenen Geräts.

| Spalte              | Typ                        | Regeln                                                                      |
| ------------------- | -------------------------- | --------------------------------------------------------------------------- |
| `id`                | TEXT PK                    | `UUIDv5(NS, vault_device_uuid ":" client_id)` (`approvals::row_id`)         |
| `vault_device_uuid` | TEXT NOT NULL              | FK `known_devices(vault_device_uuid)` ON DELETE CASCADE                     |
| `client_id`         | TEXT NOT NULL              | Fingerabdruck, 32 Kleinbuchstaben-Hex, `= hex(SHA-256(public_key)[0..16])`  |
| `public_key`        | TEXT NOT NULL              | Standard-Base64 von 32 Byte X25519                                          |
| `client_name`       | TEXT NOT NULL              | Angabe des Clients, auf 100 Zeichen gekürzt, nur zur Anzeige                |
| `origin`            | TEXT NOT NULL              | `chrome-extension://…` oder `moz-extension://…` (R2)                        |
| `declaration`       | TEXT NOT NULL              | kanonisches JSON der Erklärung (Schlüssel sortiert, Listen sortiert)        |
| `status`            | TEXT NOT NULL              | `allowed` oder `blocked`                                                    |
| `access`            | TEXT                       | `read` oder `readWrite`; NULL genau bei `blocked`                           |
| `scope_tags`        | TEXT                       | JSON-Liste von Tag-Namen; NULL bedeutet „alle Einträge“; bei `blocked` NULL |
| `default_tag`       | TEXT                       | Tag-Name oder NULL                                                          |
| `write_denied`      | INTEGER NOT NULL DEFAULT 0 | 1 = gemerkte Ablehnung des Schreibens (FR-015)                              |
| `decided_at`        | INTEGER NOT NULL           | Unix-Millisekunden der Zulassung oder Sperre                                |
| `last_seen_at`      | INTEGER                    | Unix-Millisekunden des letzten zugelassenen Handshakes                      |

Keine UNIQUE-Constraint: Ein Konflikt hielte die Synchronisation an (wie in
`identity/migrations_storage.rs`). Eine Zeile je Client und Gerät folgt aus der abgeleiteten `id`.

**Regeln beim Schreiben**:

- **`allowed`**:
  - `access` ist gesetzt.
  - Bei `readWrite` ist `default_tag` gesetzt, wenn `scope_tags` nicht NULL ist, und liegt in
    `scope_tags` (FR-010).
  - `access` und `scope_tags` sind höchstens so weit wie `declaration` (R5).
- **`blocked`**: `access`, `scope_tags`, `default_tag` NULL und `write_denied` 0.
- **Wer schreibt**: Zeilen entstehen und ändern sich nur über die Commands des Fensters
  ([contracts/tauri-commands.md](./contracts/tauri-commands.md)), nie durch einen Client oder
  Agenten (FR-013).
- **Abgleich beim Handshake**: Eine Zeile gilt nur, wenn `client_id`, `public_key`, `origin` und
  `declaration` zum Handshake passen. Weicht `origin` oder `declaration` ab, fragt holzi neu. Bis
  zur Antwort führt es keine Anfrage aus (FR-012). Die neue Antwort ersetzt die Zeile.
- **Tag-Namen** statt Tag-Kennungen: wie die Freigaben aus 034 und 017 (`Scope::tags(names)`).
  Benennt der Nutzer ein Tag um, fällt der Bereich enger aus, nie weiter.

## Einstellungen des Geräts (`storage/preferences.rs`, `PrefScope::Device`)

| Schlüssel               | Typ  | Standard | Regeln                                           |
| ----------------------- | ---- | -------- | ------------------------------------------------ |
| `browserBridge.enabled` | bool | `false`  | FR-001                                           |
| `browserBridge.port`    | u16  | `19455`  | 1024–65535; Wechsel startet die Bridge neu (R14) |

## Zustand im Speicher (`browser_bridge::state`)

Lebt, solange die Bridge läuft. Ausschalten und das Schließen der Vault leeren ihn (FR-003).

- **`BridgeRuntime`**:
  - Port, Zustand, Serverschlüssel (zeroizing), Token.
  - Verbindungen: Verbindungskennung → Fingerabdruck und Sender.
- **`SessionDecisions`** je Fingerabdruck:
  - vorläufige Zulassung (dieselben Felder wie eine Zeile, ohne `id` und Gerät);
  - vorläufige Sperre;
  - vorläufige Erweiterung auf Schreiben (mit Standard-Tag);
  - vorläufige Ablehnung des Schreibens.
- **`PendingApprovals`**: Fingerabdruck → offener Zulassungsdialog und wartende Verbindungen.
  Höchstens einer je Fingerabdruck (FR-007).
- **`Prompts`**: Kennung → `oneshot`-Sender, Art, Frist (R10).

### Zustand der Bridge (FR-037)

```text
Off ──einschalten──▶ Starting ──bind ok──▶ Running
                         │                    │
                         └─AddrInUse─▶ PortInUse    (bleibt bis Portwechsel oder Aus)
Running ──ausschalten / Vault schließt──▶ Off
(keine Vault offen) → NoVault        (Android) → NotAvailable
```

### Verbindung (R4)

```text
AwaitingHandshake ──Handshake ok──▶ AwaitingProof ──proof passt, zugelassen──▶ Authorized
        │                                   │            └─unbekannt──▶ Pending ──Zulassen──▶ Authorized
        │                                   │                               └─Ablehnen/Ablauf──▶ Closed
        │                                   ├─proof fehlt, falsch, doppelt──▶ error PROOF_FAILED ▶ Closed
        │                                   └─gesperrt──▶ handshakeResponse(false,false) ▶ Closed
        ├─Protokoll < 3──▶ error PROTOCOL_UPDATE_REQUIRED ▶ Closed
        ├─ohne Erklärung──▶ error PERMISSIONS_DECLARATION_REQUIRED ▶ Closed
        ├─Kennung passt nicht──▶ handshakeResponse(false,false) ▶ Closed
        └─10 s ohne Handshake und Nachweis──▶ Closed
Authorized ──Widerruf──▶ authorizationUpdate(false) ▶ Closed
Authorized ──Erklärung oder Herkunft geändert (neuer Handshake)──▶ Pending
```

## Aufrufer und Freigabe (R5)

`Caller::BrowserClient { id: client_id }`. Die Freigaben ergeben sich so:

| Quelle                               | `Grant.action` | `Grant.scope`                        |
| ------------------------------------ | -------------- | ------------------------------------ |
| Zulassung `read`                     | `Read`         | `All` oder `Scope::tags(scope_tags)` |
| Zulassung `readWrite`                | `ReadWrite`    | wie oben                             |
| vorläufige Erweiterung auf Schreiben | `ReadWrite`    | Bereich der Zulassung                |

Eine gemerkte oder vorläufige Ablehnung des Schreibens verhindert die Rückfrage nach FR-015. Die
Methode scheitert dann sofort.

## Anwesenheitsbestätigung (nie gespeichert)

| Feld        | Inhalt                                                                                                      |
| ----------- | ----------------------------------------------------------------------------------------------------------- |
| Kennung     | UUID der Rückfrage                                                                                          |
| Client      | Fingerabdruck, angegebener Name, Herkunft                                                                   |
| Zeremonie   | `create` oder `get`                                                                                         |
| Gegenstelle | `rp_id`, `rp_name`                                                                                          |
| Herkunft    | geprüft nach `webauthn::checked_origin`                                                                     |
| Aufgabe     | SHA-256 der Aufgabe (bindet die Bestätigung an genau eine Anfrage, FR-031)                                  |
| Benutzer    | `userName`, `userDisplayName` (beim Anlegen)                                                                |
| Wahl        | Anlegen: „Neuer Eintrag“ oder Eintrags-Kennungen im Bereich mit derselben Site. Anmelden: Passkey-Kopfdaten |
| UV          | verlangt ja/nein                                                                                            |
| Frist       | 120 s                                                                                                       |
| Ergebnis    | `Confirmed { choice, presence }` oder `Denied`                                                              |

`presence` ist `Present` oder, bei richtig eingegebenem Vault-Passwort, `Verified`. Das Ergebnis
gilt nur für den einen Aufruf des Passkey-Dienstes, der auf diese Rückfrage folgt.

## Erkennung der Erweiterung (nie gespeichert)

Keine Spalte und keine Migration (FR-045, R21). `Recognition` ist `Known { name }`,
`Lookalike { name }`, `Unknown` oder `Unverifiable`. `known::recognise(origin, client_name)`
berechnet es aus der festen Liste `KNOWN` bei jedem Aufbau von `BridgeClientRef` und
`BridgeClientView`.

## Änderungen an vorhandenen Typen

| Typ                             | Datei                          | Änderung                                                                 |
| ------------------------------- | ------------------------------ | ------------------------------------------------------------------------ |
| `Caller`                        | `src-tauri/src/caller.rs`      | Variante `BrowserClient { id: String }`                                  |
| `PasskeyCreateRequest`          | `passwords/model_passkeys.rs`  | `item_id` → `target: PasskeyTarget`; neues Feld `presence: UserPresence` |
| `PasskeyConfirmRequest`         | ebenda                         | neues Feld `presence: UserPresence`                                      |
| `UserPresence`                  | ebenda, neu                    | `None` (Standard), `Present`, `Verified`; kein `Deserialize`             |
| `PasskeyTarget`                 | ebenda, neu                    | `Existing(String)`, `New { title, url, username, tags }`                 |
| `PlatformCapabilities`          | `platform/mod.rs`              | Feld `browser_bridge: bool`                                              |
| `BridgeClientRef`               | `browser_bridge/prompts.rs`    | Feld `recognition: Recognition` (PR F)                                   |
| `BridgeClientView`              | `browser_bridge/admin.rs`      | Feld `recognition: Recognition` (PR F)                                   |
| `PasswordsService::totp_code`   | `passwords/service/items.rs`   | Parameter `grants`, `authorize_read` statt `require_user`                |
| `PasswordsService::preset_list` | `passwords/service/presets.rs` | Parameter `caller`, `grants`; jede Passwort-Freigabe genügt              |
