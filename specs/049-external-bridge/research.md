# Research: External Bridge für haex-pass-browser

**Spec**: [spec.md](./spec.md) | **Plan**: [plan.md](./plan.md) | **Stand**: 2026-10-21

Quellen, auf die sich die Entscheidungen stützen:

- **Vorlage**: `https://github.com/haex-space/haex-vault-legacy`, Revision
  `8dce379d94e18fcd42c3b73686a06f984ca3f574`, Pfade `src-tauri/src/external_bridge/`,
  `src/composables/handlers/useCoreExternalRequestHandlers/`.
- **Client**: `https://github.com/haex-space/haextension`, Revision
  `c8133b94b37ca90f7b17011f4f58f8c9ab91d1f0`, Pfad `apps/haex-pass-browser/src/`.
- **holzi**: Stand `main` bei `35e1c34e`.

## R1 Server, Transport und Grenzen

**Decision**: Die Bridge ist ein eigener `TcpListener` mit `tokio-tungstenite` 0.28
(`accept_hdr_async`). Sie lauscht auf `127.0.0.1:<port>` und bindet zusätzlich `[::1]:<port>`,
soweit das Gerät IPv6-Loopback hat.

- **IPv4 und IPv6**: Der Client ruft `ws://localhost:<port>` auf (`background/connection.ts:140`),
  und ein Browser kann `localhost` zu `::1` auflösen. Scheitert das IPv4-Binden mit `AddrInUse`,
  ist der Zustand „Port belegt“ (FR-002). Scheitert `[::1]` mit `AddrNotAvailable`, läuft die
  Bridge nur auf IPv4 weiter. Belegt jemand anderes `[::1]:<port>`, ist der Zustand ebenfalls
  „Port belegt“, weil sonst ein Teil der Verbindungen bei einem fremden Dienst landet.
- **Grenzen**:
  - `WebSocketConfig`: Nachricht und Rahmen höchstens **64 KiB**. Eine Anlege-Anfrage braucht
    nach der Clarification 2026-10-21 keine Symbole mehr; die größte erwartete Nachricht ist eine
    Passkey-Antwort mit unter 4 KiB.
  - Höchstens **16** offene Verbindungen; weitere werden vor dem Upgrade geschlossen.
  - Ein Handshake muss innerhalb von **10 s** nach dem Upgrade kommen, sonst wird geschlossen.
  - Je Verbindung laufen höchstens **4** Anfragen gleichzeitig (siehe R4).
  - Nur Textrahmen werden gelesen; Binärrahmen schließen die Verbindung.

**Rationale**:

- `tokio-tungstenite` 0.28 steht schon im `Cargo.lock` (über `nostr-sdk`). Als direkte
  Abhängigkeit in derselben Version kommt keine neue Crate in den Baum.
- `accept_hdr_async` gibt Zugriff auf den Upgrade-Request; dort prüft R2 die Herkunft, bevor ein
  WebSocket entsteht.
- Die Vorlage setzt weder Grenzen noch eine Herkunftsprüfung.
- Vorbild für einen Loopback-Dienst, den das Vault-Gate stoppt, ist
  `src-tauri/src/files/media/server.rs:42-63`.

**Alternatives considered**:

- `axum` mit WebSocket-Route: eine neue Crate samt HTTP-Stack, die hier nichts kann, was
  `tokio-tungstenite` nicht schon kann.
- Nur IPv4 wie die Vorlage: Das wäre eine stille Lücke, wenn ein Browser `localhost` zuerst nach
  `::1` auflöst.

## R2 Herkunftsprüfung vor dem Handshake (FR-005, US6)

**Decision**: Der Callback von `accept_hdr_async` liest den Header `Origin`.

- Nur `chrome-extension://<id>` und `moz-extension://<uuid>` werden angenommen.
- Fehlt der Header, oder ist er `null`, `http(s)://…` oder etwas anderes, antwortet holzi mit HTTP
  403, und es entsteht kein WebSocket.
- Die Herkunft gehört zur Zulassung: Die gemerkte Zulassung speichert sie. Meldet sich derselbe
  Fingerabdruck mit einer anderen Herkunft, fragt holzi neu, genau wie bei einer geänderten
  Erklärung (FR-012).

**Rationale**:

- Browser setzen `Origin` beim WebSocket-Aufbau selbst. Eine Webseite kann ihn nicht fälschen; sie
  schickt immer ihre eigene Webadresse.
- Erweiterungen schicken die Adresse der Erweiterung:
  - Chromium: `chrome-extension://<id>` aus dem Service Worker.
  - Firefox: `moz-extension://<uuid>` aus den Hintergrundskripten. Die UUID bleibt je Installation
    gleich.
- Die Herkunft an die Zulassung zu binden, kostet nichts. Ein lokales Programm muss dann
  Fingerabdruck **und** Herkunft kennen; das schließt die Grenze aus den Assumptions der Spec nicht,
  macht sie aber enger.

**Prüfung**: Das Verhalten beider Browser muss an echten Browsern nachgewiesen werden; dafür ist
der erste Schritt von [quickstart.md](./quickstart.md) da (M1). Schickt ein Browser
entgegen der Erwartung keinen `Origin`, bleibt holzi geschlossen (keine Ausnahme), und der Plan
wird angepasst, bevor PR B gemergt wird.

**Alternatives considered**:

- Prüfung nur im Handshake (Name des Clients): Der Name ist frei wählbar.
- Ein Token aus den Einstellungen in die Erweiterung kopieren: Die Erweiterung müsste geändert
  werden, und der Nutzer müsste etwas tun, was die Zulassung schon leistet.

## R3 Kryptografie der Umschläge

**Decision**: holzi spricht das Verfahren der Vorlage und des Clients unverändert.

- **Schlüsseltausch**: X25519 mit `curve25519-dalek` 5
  (`MontgomeryPoint(peer).mul_clamped(secret)`). Ein gemeinsames Geheimnis aus lauter Nullbytes
  (Punkt kleiner Ordnung) wird abgelehnt.
- **Verschlüsselung**: AES-256-GCM mit `aes-gcm` 0.11. Das rohe gemeinsame Geheimnis ist der
  Schlüssel. Der IV hat 12 Byte, der Tag (16 Byte) hängt am Chiffrat. Alles ist Standard-Base64
  mit Auffüllung (`base64::engine::general_purpose::STANDARD`).
- **Schlüssel des Servers**: zufällig bei jedem Start der Bridge, nur im Speicher, beim Stoppen
  überschrieben (`zeroize`). Er geht mit jeder `handshakeResponse` als `serverPublicKey` hinaus.
- **Antworten**: Jede Antwort bekommt ein frisches Schlüsselpaar des Servers. Ihr öffentlicher
  Schlüssel steht in `publicKey`. Das gemeinsame Geheimnis kommt aus diesem frischen Schlüssel und
  dem öffentlichen Handshake-Schlüssel des Clients (`background/crypto.ts:228-245`).
- **Kennung**: holzi prüft `clientId == hex(SHA-256(publicKey)[0..16])` mit `sha2` 0.11 und weist
  sonst ab (FR-006). Die Vorlage prüft das nicht (`connection.rs:102`).

**Rationale**:

- `curve25519-dalek` 5.0.0 steht über `ed25519-dalek` 3 schon im Baum. X25519 ist genau
  `mul_clamped` nach RFC 7748, und die RFC-Testvektoren prüfen die Umsetzung.
- `x25519-dalek` fehlt im Baum, und seine stabile Linie hängt an `curve25519-dalek` 4. Das wäre eine
  zweite Version derselben Crate.
- `aes-gcm` 0.11.1 steht schon im Lockfile und gehört zur selben RustCrypto-Generation wie
  `sha2` 0.11 und `hkdf` 0.13.
- **Bewertung des fehlenden KDF** (Assumption der Spec): Jede Nachricht hat ein eigenes
  Schlüsselpaar und damit einen eigenen Schlüssel, und der IV ist zufällig. Wiederverwendung von
  Nonces ist also ausgeschlossen. Der Verzicht auf HKDF weicht von der Empfehlung aus RFC 7748 §6.1
  ab, führt hier aber zu keinem bekannten Angriff. Ihn zu beheben, bräuchte eine Änderung des
  Protokolls in Client und Vorlage (Nicht im Umfang).
- **`action` ist nicht authentisiert**: Das Feld steht außerhalb des Chiffrats. Wer es auf dem Weg
  ändert, sitzt schon auf dem Gerät (Grenze aus ADR-0007).

**Alternatives considered**:

- HKDF und AAD einführen: Das bräche den unveränderten Client (FR-004).
- Ein dauerhafter Serverschlüssel je Vault: Er bringt nichts, weil der Client den Schlüssel bei
  jedem Handshake neu übernimmt, und er wäre ein weiteres Geheimnis in der Vault.

## R4 Verbindung als Zustandsautomat

**Decision**: Jede Verbindung läuft als eigene Aufgabe mit dem Zustand
`AwaitingHandshake → Pending | Authorized → Closed`.

- **Wiederholte Handshakes**: Der Client schickt den Handshake auf derselben Verbindung erneut,
  solange er nicht gepaart ist (`connection.ts:127-131`). Gleiche Kennung und gleicher Schlüssel
  beantwortet holzi mit dem aktuellen Zustand und öffnet keinen zweiten Dialog. Eine andere Kennung
  auf derselben Verbindung schließt sie.
- **Dialoge je Client**: Höchstens ein Zulassungsdialog je Fingerabdruck über alle Verbindungen
  (FR-007, US6 Szenario 4). Wartende Verbindungen werden nach der Antwort alle benachrichtigt.
- **Anfragen**:
  - Jede Anfrage läuft als eigene Teilaufgabe, je Verbindung höchstens 4 gleichzeitig (Semaphore).
    So hält eine Anfrage, die 120 s auf den Nutzer wartet, weder `get-items` noch Ping auf.
  - Ein Schreiber-Task je Verbindung nimmt die Antworten über einen begrenzten Kanal entgegen.
  - Die `requestId` gilt je Verbindung, nicht global (in der Vorlage global, Lücke 4).
- **Fehler mit und ohne `requestId`**:
  - Jede Antwort auf eine entschlüsselbare Anfrage trägt deren `requestId`. Fehlt sie oder ist
    sie leer, verwirft holzi die Anfrage stumm, weil der Client sie ohnehin nicht zuordnen könnte.
  - Nicht entschlüsselbare Anfragen bekommen einen `error`-Rahmen wie in der Vorlage.
- **Widerruf und Ablehnen**: `authorizationUpdate { authorized: false }`, dann Schließen (FR-011).
  Wartende Anfragen dieser Verbindung scheitern; ihre Dialoge werden abgebrochen.
- **Ping**: `{"type":"ping"}` beantwortet holzi in jedem Zustand mit `{"type":"pong"}`.

**Rationale**:

- Die Vorlage bearbeitet Anfragen nacheinander im Leseschleifen-Task (`connection.rs:368-379`)
  und widerruft still (Lücke 6).
- Der Client verwirft ausstehende Anfragen, wenn die Verbindung endet (`connection.ts:224-230`);
  Schließen ist also das deutlichste Signal.

**Alternatives considered**: Ein Actor je Client statt je Verbindung. Er ist mehr Code, und der
Client hält ohnehin nur eine Verbindung.

## R5 Aufrufer, Freigaben und Standard-Tag

**Decision**: Neuer Aufrufer `Caller::BrowserClient { id }` mit dem Fingerabdruck als `id`
(`src-tauri/src/caller.rs:5-17`).

- **Andere Bereiche**: Die erschöpfenden `match`-Ausdrücke in `contacts/` und `files/` geben für
  ihn `Forbidden`; die Bridge erreicht diese Bereiche ohnehin nie.
- **Passwörter**: `passwords/access.rs` `reach()` behandelt ihn wie andere Aufrufer von außen
  (Vereinigung der Bereiche seiner Freigaben, nie der Papierkorb).
- **Freigaben**:
  - Die Zulassung ergibt genau eine `Grant { action, scope }`, mit `Scope::All` oder
    `Scope::tags(..)`.
  - Eine gemerkte Erweiterung auf Schreiben (FR-015) ändert `action` in der Zulassung selbst.
  - Eine vorläufige Erweiterung lebt im Speicher.
  - Eine gemerkte Ablehnung des Schreibens steht als `write_denied` in der Zulassung („Verweigerung
    schlägt Erteilung“).
- **Standard-Tag**: Das Modell aus 034 kennt keines. Die Bridge hängt es vor `create_item` an
  `ItemInput.tags`. Mit einem Bereich aus Tags verlangt `authorize_create` ohnehin ein Tag im
  Bereich (`access.rs:282-295`).
- **Erklärung**: Nur der Eintrag `permissions.core.passwords` zählt. Alle anderen Kategorien der
  Erklärung (`database`, `shell`, …) gewährt holzi nie; die Vorlage gewährt sie alle (Lücke 7).
  Erklärt ein Client keine Passwörter, antwortet holzi mit `PERMISSIONS_DECLARATION_REQUIRED`
  (FR-006).

**Rationale**:

- `extensions/passwords.rs` (`call()` 408-420, `grants_of` 84-108) ist das Vorbild für einen
  Aufrufer mit Freigaben.
- Den Aufrufer als eigene Variante zu führen, statt `Extension { id }` mitzubenutzen, hält
  Protokolle und Prüfungen eindeutig (ADR-0007: Der Aufrufer ergibt sich aus dem Eingang).

**Alternatives considered**:

- `Caller::Extension { id: "browser:<fp>" }`: Das vermischt zwei Eingänge, und jede Prüfung, die
  haextensions meint, träfe auch Clients.
- Das Standard-Tag in `Grant`: Es ist eine Eigenschaft der Bridge, nicht der Freigabe. 034 bleibt
  unverändert, wie die Spec verlangt.

## R6 Methodentabelle und Wand zum Modell (FR-018, FR-028, FR-029, FR-041)

**Decision**:

- `browser_bridge/methods.rs` führt eine Tabelle `METHODS` von
  `Method { name, need: GrantAction, handler, module }` mit den acht Methoden aus FR-018.
- Daneben steht eine Liste `NOT_SUPPORTED` mit `update-item` und den sechs `bookmarks-*`.
- Alles andere sowie jedes Ziel außer `__core__`/`core` wird ohne Wirkung abgelehnt.
- Ein Vertragstest `src-tauri/tests/browser_bridge_contract.rs` prüft die Wand:
  - jede Methode genau einmal;
  - jede Methode in [contracts/wire-protocol.md](./contracts/wire-protocol.md) aufgeführt;
  - kein `module` enthält `::chat`, `::llm`, `::adapters` oder `::providers`;
  - jedes `module` beginnt mit `holzi_lib::browser_bridge::`.
- **Eingaben**: Je Methode gibt es ein eigenes `serde`-Struct mit genau den Feldern, die die
  Methode liest. Andere Felder werden übergangen, nicht abgelehnt. Das ist FR-022 und die Antwort
  auf die ungefilterten Seitendaten in `webauthn-bridge.ts:127`: Eine Seite kann dort Felder wie
  `itemId` ergänzen, holzi liest sie nie.

**Rationale**: Das Muster stammt aus `src-tauri/tests/extension_bridge_contract.rs` und
`extensions/bridge/dispatch.rs:38-41` (Spec 017, ADR-0004).

## R7 `get-items` und registrierbare Domain (FR-019 bis FR-021)

**Decision**: Ablauf von `get-items`:

1. Die Adresse parsen. Fehlt ein Schema, ergänzt holzi `https://` wie die Vorlage. Fehlt die
   Adresse oder ist sie unlesbar, wird abgelehnt.
2. Die Seite (Site) bestimmen:
   - Für Hosts die registrierbare Domain (`psl::domain_str`).
   - Für IP-Adressen, `localhost` und Hosts ohne registrierbare Domain der Host selbst.
3. `PasswordsService::list_headers(&caller, &grants)` und dann filtern: Einträge mit Adresse,
   gleicher Site, und, wenn `fields` angegeben ist, mit mindestens einem der Felder (`username`,
   `password`, oder `otp` gleich `has_totp`).
4. Jeden Treffer mit `read_secret_item` lesen. Der Dienst löst Verweise auf und lässt Felder weg,
   deren Quelle außerhalb des Bereichs liegt oder nicht auflösbar ist (`reveal.rs:128`, FR-021).
5. Die Antwort bilden:
   - Kennung, Titel (leer ergibt `"Untitled"`), Adresse.
   - `fields` mit Benutzername, Passwort, eigenen Feldern als Zeichenketten und `otp: "TOTP"`, wenn
     TOTP eingerichtet ist.
   - `hasTotp`, `autofillAliases`.
   - **Nie** `otp_secret`, Notiz, Anhänge oder Passkeys. Die Bridge baut ihre Antwort aus
     ausgewählten Feldern, nicht durch Weglassen.

- **Ort der Site-Bestimmung**: ein kleines Modul `passwords/site.rs` mit
  `fn site_of(host: &str) -> Option<String>`. Dorthin wandert die Suffix-Logik aus
  `webauthn.rs:45-63` (`relying_party`, privat), die die Prüfung der Herkunft schon nutzt. Beide
  Stellen nutzen dann dieselbe Regel (siehe Constitution Check, Kandidat K2).
- **Adressen mit Verweis**: `list_headers` leert für Aufrufer von außen Adressen, die einen Verweis
  enthalten (`items.rs:54-64`). Solche Einträge sind für `get-items` keine Treffer. Das ist
  hingenommen, weil der Client sonst über die Liste an den Verweis käme.

**Rationale**:

- Kopfdaten plus Einzelabfrage halten die Regeln des Dienstes unverändert (Clarification
  2026-10-10).
- Das Filtern von 5 000 Kopfdaten in Rust und bis zu 20 Einzelabfragen bleibt grob unter 500 ms
  (SC-002). Die Kopfdaten kommen aus einer Abfrage ohne Entschlüsselung einzelner Felder.

**Alternatives considered**:

- Ein SQL-Vorfilter mit `LIKE` wie die Vorlage: Er ist ungenau und unnötig bei dieser Größe.
- Eine eigene Methode `find_by_site` im Dienst: Sie wäre nur ein Filter über `list_headers`; der
  Filter bleibt in der Bridge, bis ein zweiter Aufrufer ihn braucht.

## R8 TOTP und Voreinstellungen für Aufrufer mit Freigabe (FR-024, FR-027)

**Decision**: Die vorhandenen Dienstmethoden werden erweitert, nicht verdoppelt.

- **TOTP**: `PasswordsService::totp_code(&caller, &grants, item_id)`.
  - Statt `require_user` prüft die Methode `authorize_read`. Der Nutzer übergibt leere Freigaben
    und erreicht wie bisher alles.
  - Ohne TOTP und außerhalb des Bereichs kommt dasselbe `PasswordsNotFound` (FR-024, FR-042).
  - Die Bridge antwortet `{ code, validFor }`, wobei `validFor` die Restzeit in Sekunden ist.
- **Voreinstellungen**: `preset_list(&caller, &grants)` erlaubt jeden Aufrufer mit mindestens
  einer Passwort-Freigabe (FR-027).
  - `get-password-config` liefert die Standard-Voreinstellung als
    `{ config, presetName }`.
  - `get-password-presets` liefert `{ presets: [{ id, name, isDefault, config }] }`.
  - `Preset` (`model.rs:569`) hat genau die Felder, die der Client als `config` liest.
- **Aufrufstellen**: Die bisherigen Aufrufe aus den Commands des Nutzers bekommen `&[]` als
  Freigaben.

**Rationale**: Die Spec ändert 034 Z11 genau für diese zwei Lesewege. Den Code berechnet weiter der
Dienst (`reveal::totp_code`, `reveal.rs:201`), das Secret verlässt ihn nicht (FR-040).

## R9 `create-item` (FR-022)

**Decision**: `create-item` liest genau vier Felder (`title`, `url`, `username`, `password`).

- Leerer Titel: Der Host der Adresse wird Titel; ohne Adresse heißt der Eintrag „Untitled“.
- `ItemInput` mit `tags = [Standard-Tag]` (leer bei „alle Einträge“ ohne Standard-Tag) und
  `group_id = None`.
- Der Dienst legt den ersten Stand im Verlauf an (`passwords/items.rs:388`). Der Passwortmanager
  zeigt den Eintrag ohne Neuladen über `vault-data-changed` (FR-025, `src/stores/passwords.ts:79`).
- Antwort: `{ entryId, title }`.

## R10 Rückfragen an den Nutzer: geparkte Dialoge

**Decision**: Ein Modul `browser_bridge/prompts.rs` nach dem Muster von
`src-tauri/src/files/agent/prompt.rs`:

- Rust sendet ein Ereignis, parkt auf einem `oneshot` und wartet höchstens **120 s**.
- Es bricht ab, wenn das Token der Verbindung oder der Bridge feuert (Ende der Verbindung,
  Ausschalten, Schließen der Vault).
- Ergebnis von Abbruch und Ablauf ist immer „Ablehnen“.
- Drei Arten:
  - **Zulassung** (FR-008 bis FR-010).
  - **Schreibzugriff** (FR-015).
  - **Anwesenheit** (FR-030 bis FR-033).
- Ereignis `browser-bridge-prompt`, Abbruch `browser-bridge-prompt-cancelled`, Antwort über den
  Command `browser_bridge_answer`.
- Im Frontend reiht der Store `src/stores/browserBridge.ts` die Fragen ein und zeigt sie
  nacheinander. Die drei Dialoge sind in `src/components/wm/Desktop.vue` eingehängt, neben
  `PermissionRequestDialog`.
- Vor jedem Dialog holt sich holzi nach vorn (`raiseWindowAsync`,
  `src/composables/useExtensionHost.ts:53-66`). Ist das Fenster danach nicht im Fokus, zeigt holzi
  eine Systembenachrichtigung über `tauri-plugin-notification`, die das Fenster öffnet.
- **Antwort ohne Dialog**: Kein Client, keine haextension und kein Agent kann einen dieser
  Dialoge beantworten (FR-013). `browser_bridge_answer` ist ein Tauri-Command des Fensters und
  keine Katalogaktion; der Agent hat keinen Weg dorthin.

**Rationale**:

- Die Rückfragen aus 017 (`extensions/permissions/prompts.rs`) arbeiten mit einem Wiederholversuch
  des SDK (Fehler 1004, dann `extension:permission-resolved`). Der Client der Bridge kennt das
  nicht; er wartet auf genau eine Antwort.
- Das geparkte Muster gibt es zweimal: `files/agent/prompt.rs` mit 60 s und
  `chat/tools/action_bridge.rs`. Ob die Bridge das dritte Exemplar bekommt oder ob der Mechanismus
  in ein gemeinsames Modul wandert, entscheidet der Betreiber am Plan-Gate (Kandidat K1 im
  Constitution Check).

## R11 Passkeys über die Bridge (FR-030 bis FR-036)

**Decision**: Ablauf von `passkey-create`:

1. **Freigabe prüfen**. Hat die Zulassung nur „Lesen“, kommt erst die Rückfrage nach FR-015
   (US4 Szenario 6).
2. **Herkunft prüfen** mit `webauthn::checked_origin(origin, rp_id)` (`webauthn.rs:69-95`).
   Fehlt die Herkunft oder passt sie nicht, wird ohne Dialog abgelehnt (FR-032).
3. **Ausgeschlossene Passkeys** vorab prüfen: `passkey_list` im Bereich, geschnitten mit
   `excludeCredentials`. Gibt es einen Treffer, kommt die Ablehnung `PasswordsPasskeyExcluded`
   ohne Dialog (Edge Case). Der Dienst prüft beim Anlegen in der Transaktion noch einmal.
4. **Anwesenheitsbestätigung** zeigen: Client, Gegenstelle, Herkunft, Benutzer und die Wahl des
   Eintrags („Neuer Eintrag“ oder ein Eintrag im Bereich mit derselben Site nach R7). Bei
   `userVerification: "required"` kommt die Eingabe des Vault-Passworts dazu (R12).
5. **`passkey_create`** mit der neuen Anwesenheit (siehe Dienständerungen) und mit
   `pub_key_cred_params = [-7]` (ES256, FR-034).

Ablauf von `passkey-get`:

1. Herkunft prüfen wie beim Anlegen.
2. Kandidaten bestimmen: `passkey_list(rp_id)` im Bereich, eingeschränkt auf `allowCredentials`,
   falls angegeben. Ohne Kandidaten kommt `PasswordsNotFound` ohne Dialog.
3. Anwesenheitsbestätigung mit der Wahl des Kontos, wenn mehrere passen.
4. `passkey_confirm` mit genau dem gewählten Passkey (`allow_credentials = [gewählt]`) und der
   Anwesenheit. `last_used_at` setzt der Dienst in derselben Transaktion.

`passkey-list` liefert die Kopfdaten aus 036 FR-027 im Bereich (FR-036).

Änderungen am Passkey-Dienst (036, `passwords/service/passkeys.rs`, `passkeys_ops.rs`,
`webauthn.rs`):

- **Anwesenheit**: `PasskeyCreateRequest` und `PasskeyConfirmRequest` bekommen das Feld
  `presence: UserPresence` (`None | Present | Verified`, Standard `None`).
  - `create_auth_data` (`webauthn.rs:287`) und `get_auth_data` (304) setzen daraus `UP` bzw.
    `UP|UV`.
  - Nur `browser_bridge` erzeugt `Present` oder `Verified`, und zwar nur nach einer
    Anwesenheitsbestätigung.
  - Der Wert ist ein Rust-Typ ohne `Deserialize`, damit keine Eingabe von außen ihn setzen kann.
- **Ziel des Passkeys**: `PasskeyCreateRequest.item_id` wird zu
  `target: PasskeyTarget { Existing(item_id) | New { title, url, username, tags } }`.
  - `New` legt Eintrag und Passkey in **einer** Transaktion an. Scheitert der Passkey, entfällt
    auch der Eintrag (FR-034).
  - Die Prüfung `authorize_create` bzw. `authorize_update` bleibt im Dienst.
- **Kodierung** (FR-035): Der Client schickt Standard-Base64 mit Auffüllung, der Dienst spricht
  Base64URL ohne Auffüllung. Die Bridge übersetzt in beide Richtungen; `credentialId` steht in der
  Datenbank ohnehin als Standard-Base64.
  - Der Dienst baut `clientDataJSON` mit der Aufgabe als Base64URL und mit der Herkunft aus der
    Anfrage.
  - `attestationObject` ist CBOR mit `fmt: "none"` und `authData` als Byte-String
    (`webauthn-inject.ts`, `attestation.ts:63-82`).
- **Antwortfelder**:
  - Anlegen: `credentialId`, `publicKey` (SPKI), `attestationObject`, `clientDataJson`,
    `transports: ["internal"]`, `passkeyId`.
  - Anmelden: `credentialId`, `authenticatorData`, `signature`, `clientDataJson`, `userHandle`
    (nur bei auffindbaren Passkeys) und `passkeyId`.
  - `publicKeyCose` liest der Client nicht und entfällt.
  - `"hybrid"` meldet holzi nicht, weil es nicht stimmt.
- **ADR**: ADR-0015 hält fest, dass der Passkey-Dienst UP und UV setzt, wenn ein
  zeremoniegebundener Nachweis vorliegt. Das löst ADR-0009 in diesem Punkt ab.
  `specs/036-password-redesign/contracts/passkey-service.md` bekommt einen Hinweis auf die
  Änderung.

**Rationale**:

- Die Bridge baut keinen zweiten Passkey-Weg (Spec, Beziehung zu 036).
- Die Prüfungen vor dem Dialog gibt es, damit ein Dialog nie für eine Anfrage erscheint, die
  ohnehin scheitert (US4 Szenario 4, Edge Cases).

**Alternatives considered**:

- UP immer setzen wie die Vorlage: verworfen durch ADR-0009 und FR-031.
- Eintrag und Passkey als zwei Dienstaufrufe mit Aufräumen bei Fehler: Ein Absturz dazwischen
  ließe einen leeren Eintrag zurück.

## R12 UV über das Vault-Passwort (FR-033)

**Decision**:

- Verlangt die Gegenstelle `userVerification: "required"`, fragt die Anwesenheitsbestätigung
  zusätzlich das Vault-Passwort ab.
- Rust prüft es mit `verify_current` aus `src-tauri/src/instances/passphrase_change.rs:75-90`:
  - eine zweite, nur lesende Verbindung mit `PRAGMA key`;
  - wird `pub(crate)` und bleibt an seinem Ort;
  - läuft über `gate.spawn_blocking`.
- Ist das Passwort falsch oder bricht der Nutzer ab, gilt das als Ablehnen.
- Das Passwort reist als `Passphrase` (zeroizing) durch den Command und wird nicht gespeichert.
- Bei `"preferred"` und `"discouraged"` fragt holzi nicht nach dem Passwort und setzt nur UP.

**Rationale**: Die Prüfung gibt es schon für den Passwortwechsel. Eine zweite Implementierung
widerspräche „eine Implementierung je Regel“.

## R13 Speicher: Einstellungen und Zulassungen (FR-001, FR-009, FR-037)

**Decision**:

- **Ein/Aus und Port**: Einstellungen mit Bereich Gerät in `storage/preferences.rs`
  (`PrefScope::Device(uuid)`), Schlüssel `browserBridge.enabled` und `browserBridge.port`.
  - Das Fenster schreibt sie über die Katalogaktion `settings.browserBridge.set` mit Bereich
    `guardrails`. Damit ist sie für den Agenten nicht aufrufbar (`settingsActions.ts:100`, FR-013).
- **Gemerkte Zulassungen und Sperren**: neue CRDT-Tabelle `haex_browser_clients` mit
  Fremdschlüssel `vault_device_uuid` auf `known_devices` (ADR-0001; Vorbild
  `extension_permissions`, `identity/migrations_extensions.rs:63-73`).
  - Migration mit der nächsten freien Nummer (bei Planung `0038`).
  - `HOLZI_TRIGGER_VERSION` 25 → 26.
  - Felder in [data-model.md](./data-model.md).
- **Vorläufige Zulassungen, Sperren und Erweiterungen**: im Speicher der Bridge. Sie verfallen
  beim Ausschalten und mit dem Prozess beim Schließen der Vault (ADR-0003, FR-003).
- **`last_seen_at`**: wird bei jedem zugelassenen Handshake geschrieben. Handshakes gibt es beim
  Start des Browsers und nach Verbindungsabbrüchen, also selten. Die Synchronisation mit anderen
  Geräten ist harmlos, weil dort niemand die Zeile liest (sie ist geräteeigen).

**Rationale**: ADR-0001 schließt `_no_sync`-Tabellen für geräteeigene Daten aus, weil sie beim
Kopieren der Datei mitreisen. Die Vorlage nutzt `_no_sync`; das übernimmt holzi nicht.

**Alternatives considered**: Zulassungen als JSON in den Einstellungen. Das ist schlecht
abfragbar, und jede Änderung schreibt die ganze Liste neu.

## R14 Lebenszyklus (FR-003, SC-008)

**Decision**: `browser_bridge::start_for_active_instance(app)` liest die Einstellungen und startet
die Bridge, wenn sie eingeschaltet ist.

- **Aufrufstellen**: alle drei Wege, auf denen eine Vault aufgeht: `instances/open.rs:133-163`,
  `instances/create.rs:147-152` und `instances/import.rs:204-216` (dieselbe Liste wie
  `sync::start_for_active_instance`).
- **Laufzeit**: Die Bridge läuft unter `gate.spawn` mit einem Kind des Gate-Tokens.
- **Stoppen**: Ausschalten oder das Gate-Token beendet die Bridge in dieser Reihenfolge:
  1. Das Token feuert.
  2. Die Annahme-Schleife endet; beide Listener werden fallen gelassen, der Port ist frei.
  3. Jede Verbindung schickt einen Close-Rahmen.
  4. Offene Dialoge enden mit „Ablehnen“.
  5. Der Speicherzustand wird geleert.
- **Portwechsel**: Stoppen und Starten mit dem neuen Port.
- **Zeit**: Ziel unter 2 s (SC-008). Die Integrationsprobe misst das und prüft danach mit `bind`,
  dass der Port frei ist.

## R15 Plattformen (FR-043)

**Decision**:

- `PlatformCapabilities` (`src-tauri/src/platform/mod.rs:44-69`) bekommt `browser_bridge: bool`:
  `true` für den Desktop, `false` für Android.
- Die Commands antworten auf Android mit `NotAvailable`.
- Die Einstellungen zeigen `NotOnThisDevice.vue`.
- Der Code wird überall gebaut, ohne `cfg`; `tokio-tungstenite` baut auch für Android. Das folgt
  der Regel in `platform/mod.rs:1-6`.

## R16 Protokolle und Fehlertexte (FR-039, FR-042, SC-004)

**Decision**:

- **Protokolle** (`log`) enthalten nur Methode, die ersten 8 Zeichen des Fingerabdrucks und die
  Art des Ergebnisses (`ok`, `denied`, `not_found`, `not_supported`, `timeout`, `error`). Nie
  Adressen, Nutzlasten, Fehlerketten des Dienstes oder Titel.
- **Fehlertexte an den Client**: ein geschlossener Satz fester Zeichenketten mit optionalem
  `errorCode` (Liste in [contracts/wire-protocol.md](./contracts/wire-protocol.md)). Fehler des
  Dienstes werden auf diesen Satz abgebildet, nie durchgereicht.
- **„Nicht gefunden“ und „außerhalb des Bereichs“** sind derselbe Fehler (FR-042).
- **Prüfung**: Die Integrationsprobe installiert einen erfassenden `log`-Logger (ein Logger je
  Testbinary). Sie pflanzt Markierungen in Secrets, Adressen und Einträge außerhalb des Bereichs
  und sucht sie in Antworten und im erfassten Protokoll (SC-004).

## R17 Teststrategie

**Decision**:

- **Rust, Einheit**:
  - Umschläge: RFC-7748-Vektoren und eine Fixture, die WebCrypto erzeugt hat (R18).
  - Zerlegen der Nachrichten, Herkunftsregel, Prüfung der Kennung.
  - `site_of`, Kanonisierung der Erklärung, Abbildung Zulassung → Freigabe.
  - Je Datei `*_tests.rs` nach Konvention.
- **Rust, Integration**: `src-tauri/tests/browser_bridge.rs` startet die Bridge auf Port 0 gegen
  eine Test-Vault. Ein Test-Client in der Testdatei spricht das Protokoll. Geprüft werden:
  - Zugriffsmatrix (SC-003);
  - Webseiten-Herkunft, v1, falsche Kennung, große Nachrichten, 100 Handshakes (SC-006);
  - Widerruf unter 1 s (SC-007);
  - Stoppen unter 2 s (SC-008);
  - nicht unterstützte Methoden (SC-009);
  - Lecks (SC-004);
  - Passkeys mit Prüfung von Signatur und Flags über `p256` (SC-005 ohne Browser).
- **Rust, Vertrag**: `tests/browser_bridge_contract.rs` (R6).
- **Passkey-Dienst**: `tests/passwords_passkeys.rs` wird um UP/UV und `PasskeyTarget::New`
  erweitert.
- **End-to-End**: neues Szenario `scripts/e2e/scenarios/browser-bridge.test.ts`.
  - Der Treiber startet einen kleinen Rust-Client `src-tauri/src/bin/e2e_bridge_client.rs` (Feature
    `e2e`, wie `bin/e2e_nostr_relay.rs`). Er verbindet sich mit gesetztem
    `Origin: chrome-extension://e2e`.
  - Geprüft werden Zulassungsdialog, Zulassen mit „Merken“, Liste in den Einstellungen, Widerruf und
    Anwesenheitsbestätigung (Bestätigen, Ablehnen).
  - Die Kategorieliste `settings-categories.test.ts` bleibt unverändert, weil es keine neue
    Kategorie gibt.
- **Von Hand**: [quickstart.md](./quickstart.md) mit der echten Erweiterung in Chromium und Firefox
  (SC-001, SC-005 gegen eine echte Gegenstelle, Zeit bis zur ersten Anmeldung).

**Rationale**: Die echte Erweiterung in einem Browser in der CI zu steuern, braucht einen zweiten
Browser-Treiber neben `tauri-driver`. Das ist teuer und schwankend. Die Interoperabilität der
Kryptografie sichert R18; das Verhalten der Erweiterung prüft die Handprobe.

## R18 Interoperabilität mit WebCrypto

**Decision**: Das Skript `scripts/fixtures/browser-bridge-envelope.ts` erzeugt mit der WebCrypto
von Node 22 (`X25519`, `AES-GCM`, dieselben Aufrufe wie `background/crypto.ts`) zwei Dinge:

- ein Testschlüsselpaar des Clients mit Kennung;
- einen verschlüsselten Request an einen festen Testschlüssel des Servers.

Das Ergebnis liegt in `src-tauri/tests/fixtures/browser_bridge/webcrypto.json`.

- **Richtung Client → Server**: Der Rust-Test entschlüsselt die Fixture.
- **Richtung Server → Client**: Der Rust-Test erzeugt eine Antwort mit festem Schlüsselpaar und
  vergleicht sie mit der Fixture. Das Skript prüft mit `pnpm check:browser-bridge`, dass WebCrypto
  diese Antwort entschlüsselt.
- **Schlüssel in der Fixture**: Es sind reine Testschlüssel, öffentlich wie in ADR-0012. Sie
  schützen nichts.

## R19 Begleit-PR in haextension

**Decision**: Ein PR in `haex-space/haextension` auf `main`, nur `apps/haex-pass-browser`:

1. **`passkey-list`**: Der Hintergrund antwortet `{ success, data: { passkeys } }`, oder das
   Inhaltsskript liest `passkeys`. Gewählt wird die Form, die zu `relayVaultRequest` passt; ein
   Vitest-Test belegt, dass die Liste ankommt.
2. **Neutrale Texte** in `locales/de.ts`/`en.ts`, `PasskeyConsentPrompt.vue` und
   `connection.ts:193`.
3. **Lesezeichen**: Liefert `bookmarks-*` den Fehler „nicht unterstützt“ (siehe Vertrag), zeigt
   die Erweiterung „In dieser Vault-App nicht verfügbar“ und stellt die Wiederholung alle fünf
   Minuten ein.

Danach wird die Spec auf die Merge-Revision gepinnt, und die Erweiterung wird veröffentlicht
(`haex-pass-browser-v1.3.0`, Release-Workflow `browser-extension-release.yml`).

**Rationale**: Ohne Punkt 1 erreicht keine Passkey-Anmeldung holzi. Der Fehler liegt in der
Erweiterung (`main.ts:241` gegen `webauthn-bridge.ts:143-150`), nicht im Protokoll.

## R20 Lieferungen

**Decision**: Die Arbeit kommt in sechs PRs:

| PR  | Inhalt                                                                                                          | User Stories |
| --- | --------------------------------------------------------------------------------------------------------------- | ------------ |
| A   | Spec-Anpassung, Plan und Artefakte (dieser PR)                                                                  | –            |
| H   | Begleit-PR in haextension, parallel zu B bis E                                                                  | –            |
| B   | Server, Kryptografie, Handshake, Herkunft, Zulassungsdialog, Tabelle, Ein/Aus und Port, Plattform, Vertragstest | US1, US6     |
| C   | `get-items`, `get-totp`, Voreinstellungen, `create-item`, Rückfrage Schreiben, nicht unterstützte Methoden      | US2, US3     |
| D   | Verwaltung in den Einstellungen: Liste, Ändern, Widerruf, Sperren aufheben, Portwechsel                         | US5          |
| E   | Passkeys, Anwesenheitsbestätigung, UV, ADR-0015, Hinweis im Vertrag von 036                                     | US4          |

Danach folgen Politur, Handprüfung nach quickstart, ein neuer Pin und der Release der
Erweiterung.

**Rationale**:

- Jeder PR ist für sich prüfbar.
- B ist die Sicherheitsgrundlage und kommt zuerst, ganz durchgeprüft.
- Phasendisziplin: `plans/README.md:21` setzt „034 und 036 im Einsatz“ voraus. Beide sind gemergt
  (034 mit PR #222, 036 mit den Stufen 1 bis 5) und in täglichem Gebrauch.
