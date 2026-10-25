# Research: Kanäle zwischen Erweiterungen auf verschiedenen Vaults

Alle Pfade relativ zu `src-tauri/src/`, Stand des Abzweigs von `main` am 2026-10-22 (`7165f840`).
Abhängigkeiten: iroh 1.3.0 (QUIC-Schicht `noq` 1.3.0), postcard, hmac/sha2, `subtle` 2.6 (schon
transitiv im `Cargo.lock`). Der Entwurf von haex-doom wird zitiert als „Entwurf“
(`haex-space/haextension` @ `a076b69321d0cfd63baa23d9b628620e1866e4f3`, `specs/020-haex-doom/`).

## R1 Eigene ALPN am Endpoint der Vault-Sitzung

**Decision**: Neue ALPN `EXT_PEER_ALPN = b"haex-vault-ext-peer/1"` in `sync/wire.rs` neben `SYNC_ALPN`
und `LINK_ALPN`. `SyncNode::bind` (`sync/endpoint.rs:167`) nimmt eine Liste weiterer
Protokoll-Handler `(alpn, handler)` entgegen und registriert sie am selben `Router`
(`.accept(...)`, `:213`); die Handler für diese Spec baut `sync::service::run`
(`sync/service.rs:197`) aus dem `ExtensionHost` und der `VaultDb`. Ausgehend kommt
`SyncNode::dial_alpn(addr, alpn)` nach dem Muster von `dial_link` (`:261`): Adresse in die
`MemoryLookup`, `endpoint.connect(addr, alpn)`. Jeder Kanal ist **eine eigene QUIC-Verbindung**.
Der Handler startet jede Verbindung auf dem Tracker der Sitzung wie `SyncProtocol` (`:141-157`), damit
Sperren und Schließen der Vault die Kanäle beenden.

**Rationale**: Jede geöffnete Vault betreibt den Endpoint schon, mit Relays aus den Einstellungen
(Spec 024 FR-008, `sync/servers.rs:112`) und Netzwechsel auf Android (`network_change`, `:423`).
iroh setzt die ALPNs aus allen Handlern am Router (`iroh-1.3.0/src/protocol.rs`); nach `spawn` kommt
keiner mehr dazu, daher die Liste bei `bind`. Eine Verbindung je Kanal macht das Ende eines Kanals
gleich dem Ende der Verbindung (Grund aus `close_reason()`), und Datagramme brauchen keine
Kanalkennung. Die Sync-Verbindung bleibt unberührt (FR-029); `holzi-sync/1` lehnt fremde Vaults
weiter ab.

Spec 056 plant dieselbe Erweiterung von `bind` (056 research R5). Wer zuerst landet, baut die Liste der
Handler; der andere hängt sich an.

Der Name folgt schon dem Schema von Spec 054 (FR-010, `haex-vault-…`); 054 muss ihn nicht mehr
umbenennen. Die ALPN steht als Konstante neben den anderen in `sync/wire.rs`.

**Alternatives considered**:

- Zweiter Endpoint nur für Kanäle. Verworfen: zweiter Schlüssel oder zweite Bindung desselben
  Schlüssels, doppelte Relay-Verbindungen, eigener Netzwechsel; die Visitenkarte nennt nur die
  Endpoint-ID des einen Endpoints.
- Kanäle als Streams in einer gemeinsamen Verbindung je Gegengerät. Verworfen: Datagramme bräuchten
  eine Kanalkennung, das Ende eines Kanals wäre ein eigenes Protokoll, und eine Erweiterung könnte
  über die gemeinsame Verbindung Zustände einer anderen beeinflussen.

## R2 Schnelle Nachrichten als QUIC-Datagramme

**Decision**: Schnelle Nachrichten gehen mit `Connection::send_datagram` (`iroh-1.3.0/src/endpoint/
connection.rs:995`) ungerahmt: die Bytes der Erweiterung sind das Datagramm. Empfangen über
`read_datagram` (`:918`) in einer Aufgabe je Kanal. Die Obergrenze ist fest **1.024 Byte**; beim
Öffnen prüft holzi `max_datagram_size()` (`:1038`) und lehnt einen Kanal ab, der weniger kann (in der
Praxis nicht: QUIC garantiert „a little over a kilobyte“). Senden wartet nie: `send_datagram`
verwirft bei vollem Puffer die ältesten Datagramme (FR-036, FR-038).

**Transport**: Datagramme sind in der Grundeinstellung an: `noq-proto-1.3.0/src/config/transport.rs:581`
setzt `datagram_receive_buffer_size: Some(STREAM_RWND)` und einen Sendepuffer von 1 MiB. holzi setzt
heute keine eigene `QuicTransportConfig` (keine Treffer in `src-tauri/src`). Ein Test in PR 1 hält fest,
dass `max_datagram_size()` auf direktem Weg und über das Relay des Testaufbaus ≥ 1.024 ist; ändert
eine iroh-Version die Grundeinstellung, schlägt er an. Keine eigene Transport-Konfiguration, damit die
Einstellungen von iroh für Holepunching und Multipath bleiben (`endpoint/quic.rs:110-118`).

**Rationale**: Die Netzwerkschicht von DOOM wiederholt selbst; zuverlässige Streams würden bei Verlust
blockieren (Head-of-Line). 1.024 Byte lassen Abstand zum kleinsten QUIC-Paket (1.200 Byte) abzüglich
Kopf, AEAD-Tag und Rahmen von iroh; DOOM-Pakete sind einige hundert Byte. Die Spec verlangt
mindestens 1.000 Byte (FR-035).

**Alternatives considered**: 1.100 Byte wie im Entwurf. Verworfen: zu knapp am Minimum über
Relay-Pfade mit Multipath-Kopf; der Gewinn ist für DOOM null.

**Umsetzung PR 1**: Auf Loopback gilt `max_datagram_size()` ≥ 1.024; drei Kanäle mit 35 Nachrichten
je Sekunde und Richtung kamen vollständig an (Verzögerung Median etwa 1 ms, ohne Brücke). Der
Relay-Pfad wird in der E2E-Szene gemessen (T081).

## R3 Gesicherte Nachrichten auf dem Steuerstream

**Decision**: Der erste Bi-Stream der Verbindung (Handshake) bleibt als **Steuerstream** offen. Darauf
laufen Rahmen wie in `sync/wire.rs`: `u32 BE Länge ‖ postcard(Message)`, Länge vor jeder Allokation
geprüft; dafür gibt es schon die generischen `write_value`/`read_value`/`expect_value`
(`sync/wire.rs:192`, `:223`, `:251`), die mit einem eigenen Nachrichten-Enum arbeiten. Grenze je Rahmen: `HANDSHAKE_FRAME_LIMIT`
(64 KiB) vor der Zulassung, weil `Hello` eine Geräteliste trägt; danach 64 KiB + 64 Byte für eine
gesicherte Nachricht mit Kopf. Nachrichten in
[contracts/wire-protocol.md](./contracts/wire-protocol.md): `Reliable { data }`, `Close { reason }`
und die Zulassungsnachrichten.

**Rationale**: postcard mit Längenkopf ist schon der Draht von holzi (024, 056 R6). Ein Stream reicht:
gesicherte Nachrichten sind selten (Lobby, Start), Reihenfolge ist gewollt. Grenzen gegen
Speicherangriffe vor der Zulassung.

**Alternatives considered**: CBOR mit `u32 LE` wie im Entwurf. Verworfen: zweites Format im selben
Programm ohne Nutzen; die Erweiterung sieht den Draht nicht.

**Umsetzung PR 1**: Je Kanal warten höchstens 16 gesicherte Nachrichten auf den Stream; eine weitere
wird mit `7000 limit` (`details.limit: 16`) abgelehnt, statt Speicher hinter einer Gegenseite
anzuhäufen, die nicht liest. PR 4 trägt den Fall in die Fehlertabelle der Brücke ein.

## R4 Ausweisen: gegenseitige Herausforderung und unterschriebene Geräteliste

**Decision**: Ein gemeinsamer Baustein `sync/foreign_auth.rs` (mit `_tests.rs`) für „ein Gerät einer
anderen Vault weist sich aus“, den 056 (`holzi-files/1`) und diese Spec nutzen:

1. Wählender → `Hello { v, vault, device_pubkey, list_payload, list_sig, nonce }`.
2. Annehmender → `Hello { … }` mit eigenen Werten.
3. Beide → `Proof { sig }` über `lp(nonce_dial) ‖ lp(nonce_accept) ‖ lp(own endpoint) ‖
lp(other endpoint) ‖ lp(vault) ‖ lp(alpn)` mit dem Geräteschlüssel, neue Domäne
   `Domain::ExtPeerAuth` (`sync/signing.rs`). Die andere Endpoint-ID ist `connection.remote_id()`
   wie in `sync/handshake.rs:333-341`.

Prüfung je Seite:

- `device_list::check_pushed(list_payload, list_sig, vault)` (`sync/device_list.rs:371`).
- `device_pubkey` steht auf der Liste, ist nicht entfernt, und seine `endpoint_id` ist
  `connection.remote_id()`.
- **Eigene Vault**: statt der vorgelegten gilt die eigene gültige Liste (`valid_lists` + `effective`,
  `:238`, `:276`) → bestätigt.
- **Kontakt**: `contacts::card::received::stored(q, vault)` (`contacts/card/received.rs:102`) liefert
  eine gespeicherte Karte, `is_blocked` (`:165`) ist falsch → bestätigt. Die Generation der Liste MUSS
  ≥ `card.list_generation` und ≥ der höchsten in dieser Sitzung gesehenen sein (056 FR-003). Gibt es
  bei der Umsetzung schon `haex_file_peer_lists_no_sync` aus 056, zählt auch sie.
- **Blockierte Vault**: abgewiesen (FR-034), nach außen wie jede andere Ablehnung.
- **Sonst**: nicht bestätigt; erlaubt nur nach R7 (öffentliche Lobby offen).

**Rationale**: Die Karte allein enthält die Geräteliste nicht und veraltet; ein neues Gerät des Kontakts
käme nicht hinein, ein entferntes nicht hinaus (056 R4). Die Unterschrift über beide Endpoint-IDs bindet
den Geräteschlüssel an die QUIC-Gegenstelle. Die ALPN im Transkript verhindert, dass ein Beweis für
`holzi-files/1` für Kanäle gilt. Die Untergrenze aus der Karte ist dauerhaft vorhanden, ohne dass diese
Spec etwas speichert (FR-042).

`ponytail:` Die in der Sitzung gesehene Generation lebt nur im Arbeitsspeicher; nach einem Neustart gilt
wieder die Untergrenze der Karte. Aufrüstung: die Tabelle aus 056 mitnutzen, sobald sie existiert.

**Alternatives considered**: Endpoint-ID und Geräteschlüssel auf der gespeicherten Karte (Entwurf
R-10). Verworfen wegen veralteter Karten. Den Sync-Handshake öffnen. Verworfen: Grenze von 024
(ADR-0015).

**Umsetzung PR 1**: `ContactsService::card_of_vault` antwortet `Stored`, `Blocked` oder `Unknown`
statt `Option`, damit eine blockierte Vault abgewiesen und nicht als „nicht bestätigt“ behandelt
wird. Die Untergrenze „gesehene Generation“ gilt für jede fremde Vault, nicht nur für Kontakte;
höchstens 1.024 Vaults werden gemerkt. Einzelheiten in tasks.md, Notes from implementing PR 1.

## R5 Adressen: Visitenkarte jetzt, Anklopfen später

**Decision**: Für eine Einladung baut holzi je Gerät der gespeicherten Karte des Kontakts eine
`EndpointAddr` aus `endpoint_id` und `iroh_relay` (`contacts/card/format.rs:43`) und wählt alle
zugleich (R10). Scheitert jedes Wählen innerhalb von 10 s, gilt „nicht erreichbar“ mit
`hint: "share_current_card"` (Clarifications 2026-10-22). Sobald 056 das Anklopfen baut
(056 research R9, Arten 24103/24104), sucht holzi Geräte, die über die Karte nicht erreichbar waren,
darüber; die Antwortregel dafür steht in FR-012 (nur eigene Vault und nicht blockierte Kontakte).
Beitrittscodes tragen die aktuelle Adresse des Gastgebers (R6) und brauchen beides nicht.

**Rationale**: Entscheidung des Betreibers; das Anklopfen gibt es noch nicht (keine Treffer für
`knock` in `src-tauri/src`). Der Endpoint nutzt `presets::Minimal` ohne Adressveröffentlichung
(`sync/endpoint.rs:176`); das bleibt so (056 FR-010).

**Bekannte Grenze**: iroh wählt das Heim-Relay nach Latenz; wechselt es (Reise, anderes Netz), stimmt
das Relay der Karte nicht mehr, und das Gerät ist bis zu einer neuen Karte nicht erreichbar. Der
Aussteller erstellt diese Karte selbst neu (051 research R12), der Kontakt muss sie aber erst erhalten.

**Eigenes Gerät auf der Karte**: Die Karte nannte früher das Relay nur von Geräten, deren Meldung der
Aussteller gehört hatte, nie das seines eigenen Geräts. Eine Vault mit einem Gerät veröffentlichte so
eine Karte ohne Relay, und Einladungen an sie endeten als `unreachable`. Seither steht jedes Gerät der
Vault mit seinem Heim-Relay auf der Karte, das eigene eingeschlossen, gleich für eines oder viele.

## R6 Beitrittscode und Geheimnis

**Decision**:

- Code: `haex-vault:join:` + Base32 ohne Auffüllung (`data_encoding::BASE32_NOPAD`, schon verwendet in
  `files/agent/folders.rs:12`) von `postcard(JoinCode { v: 1, endpoint_id: [u8;32],
relay: Option<String>, lobby: [u8;16], secret: [u8;16] })`. `lobby` ist eine zufällige Kennung von
  holzi, nicht die `lobbyId` der Erweiterung; die Zuordnung bleibt beim Gastgeber. Das Präfix folgt
  schon dem Schema von Spec 054.
- Das Geheimnis (128 Bit, `sync::keys::random_bytes`) verlässt den Beitretenden nie im Klartext:
  `Join` trägt `proof = HMAC-SHA256(secret, lp(nonce_dial) ‖ lp(nonce_accept) ‖ lp(lobby))` aus dem
  Handshake (R4). Der Gastgeber prüft mit `Mac::verify_slice` (zeitkonstant).
- **Einheitliche Antwort** (FR-025): unbekannte `lobby`, geschlossene Lobby, falscher Beweis, andere
  Erweiterung und blockierte Vault ergeben `JoinReply::Closed`. Für eine unbekannte `lobby` rechnet
  der Gastgeber den HMAC mit einem festen Ersatzgeheimnis, damit jeder Fall denselben Weg nimmt. Ein
  unlesbarer Code scheitert schon beim Beitretenden mit demselben Fehler wie `Closed`.
- `JoinReply::NotAdmitted` nur für einen Nicht-Kontakt in einer Lobby „nur Kontakte“ und für einen
  Entfernten — beide haben einen gültigen Beweis geliefert. In einer ausschließlich „nur Kontakte“
  offenen Situation darf ein nicht bestätigter Beitretender dafür genau einen begrenzten `Join`
  senden; ohne gültigen Beweis wird er weiterhin vor der Zulassung ohne Antwort getrennt.

**Rationale**: HMAC über die Nonces macht einen mitgeschnittenen `Join` wertlos und erlaubt einen
zeitkonstanten Vergleich mit vorhandenen Abhängigkeiten (`hmac`, wie der Link-Code,
`sync/link/code.rs:11`). Die zufällige `lobby` verrät nichts über die Erweiterung.

**Alternatives considered**: Geheimnis im Klartext und `subtle::ConstantTimeEq`. Verworfen: Klartext
auf dem Draht ist unnötig, und `subtle` würde direkte Abhängigkeit. Code mit `lobbyId` der Erweiterung
(Entwurf). Verworfen: Die Erweiterung wählt die Kennung, sie kann sprechend sein.

## R7 Zulassung und Drosselung

**Decision**: Ablauf je eingehender Verbindung (`extensions/peers/admission.rs`):

1. Handshake nach R4 binnen 5 s, sonst trennen.
2. Nicht bestätigt und keine Lobby offen → trennen ohne Grund. Ist nur eine Lobby „nur Kontakte“
   offen, darf die Gegenseite genau einen begrenzten `Join` senden; ein gültiger Beweis endet mit
   `NotAdmitted` (FR-031).
3. Erste Zulassungsnachricht (`Invite`, `InviteReply` gehört zu ausgehenden Verbindungen, `Join`)
   binnen 5 s nach dem Handshake, sonst trennen (FR-032).
4. `Invite` nur von bestätigten Gegenseiten; `Join` nach FR-024.

Drosselung (neu, es gibt im Sync keine): ein Zähler im Arbeitsspeicher

- höchstens 16 nicht bestätigte Verbindungen gleichzeitig im Zustand „noch nicht zugelassen“;
  Handshakes werden vorher in einer eigenen begrenzten Warteschlange eingeordnet. Die Prüfung der
  vorgelegten Geräteliste erfolgt vor der 16er-Grenze; bestätigte Kontakte und eigene verknüpfte
  Geräte benutzen eine separate Zulassungskapazität und können die 16 Plätze nicht belegen oder
  durch unbestätigten Verkehr ausgesperrt werden. Überzählige unbestätigte Handshakes werden sofort
  getrennt;
- je Endpoint-ID höchstens 5 Verbindungsversuche in 10 s und nach 3 gescheiterten `Join` eine Sperre
  von 60 s für diese Endpoint-ID (Antwort `Closed`, kein Unterschied nach außen);
- insgesamt höchstens 20 neue nicht bestätigte Verbindungen je 10 s;
- je Erweiterung die Grenzen aus FR-041 (R14).

Bestätigte Kontakte fallen nur unter die Grenze je Endpoint-ID, nie unter die globalen für nicht
bestätigte (FR-033: gültige Beitritte von Kontakten werden nicht ausgesperrt). Ein Kontakt wird
dafür nach der Gerätelistenprüfung aus der Warteschlange der unbestätigten Verbindungen genommen.

**Rationale**: 128 Bit Geheimnis machen Durchprobieren aussichtslos; die Drosselung schützt das Gerät
vor Last. Endpoint-IDs sind billig zu erzeugen, daher zusätzlich globale Grenzen für Unbekannte.

`ponytail:` Feste Fenster je Endpoint-ID in einer `HashMap` mit Aufräumen beim Zugriff; Obergrenze
der Einträge 1.024, älteste fallen heraus. Aufrüstung: Token-Bucket je Präfix der Netzwerkadresse.

## R8 Zustellung an die Rahmen

**Decision**: Rust meldet mit `bridge::events::emit_to_frames` (`extensions/bridge/events.rs:17`) an
alle offenen Rahmen der Erweiterung: `peers:invite-status`, `peers:channel-opened`, `peers:joined`,
`peers:message`, `peers:channel-closed` ([contracts/bridge.md](./contracts/bridge.md)). Bytes gehen als
Base64.

Puffer vor der Bereitschaft: `FrameEventQueue` (`src/lib/extensions/bridge.ts:95-150`) hält höchstens
256 Meldungen und verwirft heute zuerst `shell:output`. Erweiterung: Als verwerfbar gelten
`shell:output` **und** `peers:message` mit `reliable: false`. Müsste die Warteschlange eine
`peers:message` mit `reliable: true` verwerfen, ruft das Fenster den neuen Befehl
`extension_peers_frame_overflow { frame, channelId }`; holzi schließt den Kanal mit Grund `lost` und
meldet das beiden Seiten (FR-038). Nach der Bereitschaft gibt es keinen Puffer in holzi; der Port
liefert sofort.

Senden: ein Brückenaufruf je Nachricht (`extension_bridge_call` → blockierender Thread →
`dispatch::call` mit `is_enabled`-Lesen, `extensions/bridge/dispatch.rs:49-66`). Bei 3 Kanälen × 35/s
sind das ~105 Aufrufe je Sekunde, jeder unter einer Millisekunde.

`ponytail:` Ein Aufruf und eine Meldung je Datagramm. Aufrüstung: `extension_peers_send` mit einer Liste
und Bündeln eingehender Datagramme je 5 ms, falls SC-004 nicht erreicht wird.

**Rationale**: Ein Weg für alle Meldungen (017 FR-042, FR-005 dieser Spec); die bestehende Regel
„verwerfbare Meldungen zuerst“ passt zu schnellen Nachrichten.

## R9 Einladungsabfrage von holzi

**Decision**:

- **Rahmen der Erweiterung offen** (`host.frames.of_extension(id)` nicht leer): Rust sendet
  `extension-peer-invitation { inviteId, contactName, extensionName, extensionIcon? }` an holzis
  Fenster. Neue Komponente `src/components/extensions/PeerInvitationDialog.vue` (Vorbild
  `PermissionRequestDialog.vue`) zeigt „[Kontakt] lädt dich zu [Erweiterung] ein – Annehmen /
  Ablehnen“; die Antwort geht über den neuen Befehl `extension_peer_invitation_answer { inviteId,
accept }`. Der Name des Kontakts kommt aus holzis Adressbuch (`ContactsService` mit
  `Caller::Internal { feature: "extensionPeers" }`), nie an die Erweiterung.
- **Kein Rahmen offen**: holzi zeigt eine eigene Systembenachrichtigung über
  `Desktop::show_notification` (`extensions/host.rs:42`, Umsetzung `extensions/desktop.rs:161`) mit
  zwei Knöpfen `accept`/`decline`. Antwort `Button(accept|decline)` → wie oben; `Body` (Antippen ohne
  Knopf, oder ein System ohne Knöpfe) → holzis Fenster kommt nach vorn und zeigt den Dialog;
  `Closed` → nichts, die Einladung bleibt bis zum Ablauf. Erlaubt das System keine Benachrichtigungen
  (`notifications_allowed()` falsch, Android, 043 FR-023), zeigt holzi den Dialog im Fenster.
- Verschwindet die Einladung (Rückzug, Ablauf, anderes Gerät gewann), schließt holzi die
  Benachrichtigung (`ShownNotification::close`) und sendet `extension-peer-invitation-gone`.
- **Nach der Annahme**: Ist kein Rahmen offen, sendet Rust `extension-notification-click
{ extensionId }`; holzis Fenster öffnet die Erweiterung über `windowManager.showExtension`
  (`src/stores/windowManager.ts:321`). Die Meldung `peers:channel-opened` wartet in
  `PeerState.pending_open` (mit den Nachrichten, die inzwischen eintreffen) und geht hinaus, sobald ein
  Rahmen dieser Erweiterung seine erste Anmeldung meldet (`extension_frame_reloaded`,
  `extensions/commands/frames.rs`): `extension_frame_open` ist zu früh, denn holzis Fenster kennt die
  Kennung des Rahmens erst mit dem Ergebnis des Aufrufs und verwirft Meldungen dafür. Die
  `FrameEventQueue` hält sie bis zur Bereitschaft des Ports. Wartet sie 60 s ohne Rahmen, schließt
  holzi den Kanal mit `closed`.

Die Benachrichtigung braucht keine Berechtigung `notifications` der Erweiterung: Sie läuft nicht über
`extension_notifications_show`, sondern direkt über den `Desktop` (FR-014).

**Rationale**: Entscheidung des Betreibers (Option B). Der Klick einer Erweiterungs-Benachrichtigung
öffnet die Erweiterung schon heute ohne Pfad (`useExtensionHost.ts:108-114`); deshalb trägt die
Zustellung nicht über `?invite=` (Entwurf), sondern über die Meldung nach dem Öffnen.

## R10 Einladung auf dem Draht und „erste Annahme gewinnt“

**Decision**: Die einladende Seite wählt alle Kartengeräte zugleich. Je Verbindung nach R4:
`Invite { invite, extension: { public_key, name }, payload, expires_at }`. Die eingeladene Seite:

- nicht bestätigt oder blockiert → trennen ohne Antwort (einladende Seite: dieses Gerät „nicht
  erreichbar“);
- Erweiterung fehlt, ist deaktiviert oder `peers` steht auf „verweigert“ → `InviteReply::NotInstalled`
  und trennen;
- sonst Einladungsabfrage (R9), Verbindung bleibt offen (iroh hält sie mit Keep-Alive 5 s,
  `iroh-1.3.0/src/socket.rs:109`).

Antworten: `InviteReply::Accepted` oder `::Declined`. Die einladende Seite bestätigt nur die erste
Annahme mit `InviteConfirm`; alle anderen Verbindungen bekommen `Withdraw` und werden geschlossen. Erst
nach `InviteConfirm` meldet die eingeladene Seite `peers:channel-opened`; kommt stattdessen `Withdraw`,
zeigt ihre Abfrage „Einladung nicht mehr offen“. Endstatus bei der einladenden Seite nach FR-018:
`declined`/`not-installed` erst, wenn alle erreichten Geräte so geantwortet haben; `unreachable`, wenn
keines erreicht wurde; `expired` nach 5 min; `withdrawn` meldet holzi nicht als Status (die
Erweiterung hat ihn selbst ausgelöst).

Grenzen gegen Spam: höchstens 4 offene eingehende Einladungen je fremder Vault und 16 insgesamt; mehr
werden ohne Abfrage verworfen.

## R11 Lebenszyklus und Aufräumen

**Decision**: `ExtensionHost` bekommt `peers: PeerState`. `PeerState::end_all(extension_id)` schließt
Kanäle (`Close { reason }` auf dem Steuerstream, dann `connection.close`), Lobbys und offene Einladungen
(mit `Withdraw`) und meldet `peers:channel-closed`. Aufgerufen an denselben Stellen wie
`host.shells.end_all`:

- letzter Rahmen geschlossen: `extensions/commands/frames.rs:116-120`;
- deaktiviert und entfernt: `extensions/commands/manage.rs:69-72`, `:110-113`, Entwicklermodus
  `extensions/commands/dev.rs:144-147`;
- auf einem anderen Gerät deaktiviert oder entfernt: `extensions/registry/lifecycle.rs:146-163`.

Entzug der Berechtigung: Ein Abonnent auf `AppState::vault_changes()` (`vault_events.rs:42`) reagiert auf
die Tabellen `permissions::store::TABLES` (`extensions/permissions/store.rs:42`) und prüft für jede
Erweiterung mit offenen Kanälen, Lobbys oder Einladungen `evaluate` für `peers`/`connect`; ist sie (`Deny`; „noch nicht entschieden“ genügt nicht), `end_all`. Eine Einladung öffnet
auch Kanäle für eine Erweiterung, deren `peers` noch niemand entschieden hat; die Brücke fragt, wenn
sie den Kanal zum ersten Mal nutzt. Zusätzlich prüft jeder Brückenaufruf die Berechtigung (wie alle Arten).

Vault schließt: Der Sync-Dienst endet, `SyncNode::shutdown` (`sync/endpoint.rs:428`) bricht den Tracker
ab, die Verbindungen schließen; `PeerState` wird mit dem `CancellationToken` der Sitzung geleert. Die
Gegenseite sieht `ApplicationClosed` → `closed`.

Gründe beim Ende (FR-040): `Close`-Nachricht oder `ApplicationClosed`/`LocallyClosed` → `closed`;
`ConnectionError::TimedOut` (Leerlauf 30 s, Grundeinstellung `max_idle_timeout`,
`noq-proto-1.3.0/src/config/transport.rs:560`) → `timeout`; alles andere → `lost`.

## R12 Blockierte Vaults und Kontakte-Berechtigung

**Decision**:

- Blockierte Vaults: `contacts::card::received::is_blocked` in R4, in jeder Rolle abgewiesen. Ein
  späteres Blockieren schließt offene Kanäle zu dieser Vault (derselbe Abonnent wie R11 auf
  `haex_contacts_blocked_vaults`).
- Einladen: `contactId` MUSS von der Berechtigung `contacts` der Erweiterung umfasst sein; die Prüfung
  nutzt die Grant-Auflösung von `extensions/contacts.rs` (Gruppen nach Namen → Kennungen,
  `Caller::Extension`). Fehlt sie, gibt es den Kontakt nicht oder hat er keine Karte →
  `1001 contact_unknown`, für alle drei gleich (051 FR-031).
- Meldungen an die Erweiterung nennen einen Kontakt (`contactId`) nur, wenn ihre Berechtigung
  `contacts` ihn umfasst; sonst nur den Namen von der Visitenkarte der Gegenseite (`card.name`), den
  diese selbst veröffentlicht hat.

## R13 Fehlercodes

**Decision**: Nur die festen Codes von `ExtensionErrorCode` (`extensions/error.rs:16-39`), Grund in
`details.reason` wie in 055 ([contracts/bridge.md](./contracts/bridge.md)): 1002/1004 Berechtigung,
1001 `contact_unknown`/`invite_not_open`/`channel_unknown`/`lobby_closed`, 1002 `not_admitted`,
7000 `too_large`/`limit`, 8001 `channel_closed`/`network_unavailable`, 3001 für unlesbare Parameter.

**Rationale**: Das vault-sdk liest die Zahlencodes; neue Zahlen bräuchten eine neue Fassung der
Fehlertabelle in allen SDKs. Die Namen des Entwurfs (`PEER_*`) bildet das SDK aus `reason` ab.

## R14 Grenzen

| Grenze                                       | Wert                      |
| -------------------------------------------- | ------------------------- |
| Inhalt einer Einladung, eines Beitritts      | 4 KiB (JSON, UTF-8)       |
| Anzeigename beim Beitritt                    | 1–64 Zeichen              |
| schnelle Nachricht                           | 1.024 Byte                |
| gesicherte Nachricht                         | 64 KiB                    |
| Kanäle je Erweiterung und Gerät              | 16                        |
| offene Lobbys je Erweiterung und Gerät       | 4                         |
| offene ausgehende Einladungen je Erweiterung | 16                        |
| Ablauf einer Einladung                       | 5 min                     |
| Zulassung nach dem Handshake                 | 5 s                       |
| Leerlauf bis `timeout`                       | 30 s (iroh/noq-Grundwert) |
| Wählen der Kartengeräte                      | 10 s je Gerät, parallel   |
| `extension_peers_join` bis Antwort           | 15 s                      |

Die Spec verlangt mindestens 8 Kanäle, 2 Lobbys, 8 Einladungen (FR-041); die Werte geben Spielraum.

## R15 Android und iOS

**Decision**: Nichts ist `cfg(desktop)`. Android: Der Endpoint läuft, solange der Prozess lebt und die
Vault offen ist; im Hintergrund kann Android das Netz anhalten, Kanäle enden dann mit `timeout` oder
`lost` (Spec Edge Cases). Nach der Rückkehr meldet `resume` den Netzwechsel (`sync/resume.rs`,
`network_change`). Benachrichtigungen mit Knöpfen über das Notification-Plugin (Aktionstypen,
`extensions/desktop.rs:178-190`); verweigert der Nutzer `POST_NOTIFICATIONS`, zeigt holzi den Dialog im
Fenster. iOS hat in holzi noch keinen Build (Spec 043 „iOS (eigene Spec)“); der Code enthält nichts,
was dort anders wäre.

## R16 Testansatz

- Unit (je `*_tests.rs`): Handshake mit eigener Vault, Kontakt, blockierter Vault, Fremdem, veralteter
  Liste (Generation unter Karte), falscher Gegenstelle; Beitrittscode lesen/schreiben; HMAC-Beweis;
  Zulassungsregeln als Tabelle; Drosselung mit gesteuerter Uhr; Gründe beim Ende; Grenzen.
- Integration: zwei und drei Vaults im Prozess mit Loopback-Endpoints und lokalem Relay
  (`sync/test_support.rs`, `sync/endpoint_tests.rs`); Einladung an einen Kontakt mit zwei Geräten
  (erste Annahme gewinnt); öffentliche Lobby mit Fremdem; Datagramme über Relay mit
  `max_datagram_size() ≥ 1024`; Last 3 × 35/s × 10 s ohne `sleep` (Zeit gemessen, nicht gewartet).
- Zeitgleichheit (SC-007): Mikro-Messung der hostseitigen Pfade „unbekannt/geschlossen/falsch“ im
  Test, Mediane über 1.000 Läufe; unlesbare Codes werden separat als lokaler Fehler geprüft.
- Frontend: `FrameEventQueue` verwirft schnelle Nachrichten zuerst und meldet den Überlauf gesicherter
  (`scripts/check-extensions-bridge.ts`, `pnpm check:extensions`).
- E2E nach Spec 033 mit zweiter Vault, Szenario `scripts/e2e/scenarios/extension-peers.test.ts` mit der
  um `peers` erweiterten Probe `src-tauri/tests/fixtures/extension_e2e/peers.xt`: Einladung, Abfrage,
  Kanal, Nachricht, Ende.

Keine Netzdienste in `cargo test`, keine Wartezeiten mit `sleep`.

## R17 ADR

**Decision**: ADR 0016 „Devices of non-contact vaults for public lobbies“ (Status proposed) hält fest,
dass `haex-vault-ext-peer/1` während einer offenen öffentlichen Lobby Geräte fremder Vaults ohne Kontakt
zulässt, über ADR-0015 hinaus, und mit welchen Schranken.

## R18 Graphify und Wiederverwendung

Graph des Abzweigs (`graphify-out`, `indexed_at_sha` `7165f840`) abgefragt nach: Drosselung von
Versuchen je Gegenstelle, zeitkonstantem Vergleich eines Geheimnisses, Dialog mit Annehmen/Ablehnen aus
einem Rust-Ereignis, Ausweisen fremder Geräte mit unterschriebener Geräteliste. Ergebnis: keine
Drosselung im Bestand (Treffer nur `browser_bridge::connection::Peer`, ohne Zähler); kein Helfer für
zeitkonstanten Vergleich (HMAC-Prüfung im Link-Code ist das Muster); Dialoge aus Rust:
`remote_storage_dialog::Dialog`, `PermissionRequestDialog.vue`; Ausweisen: nur der Sync-Handshake.

Wiederverwendet: `SyncNode`/`Router`/`MemoryLookup`, `dial_link`-Muster, `wire::write_frame`/
`read_frame`-Regeln, `signing::lp`/`sign`/`verify`, `device_list::check_pushed`/`valid_lists`/
`effective`, `contacts::card::received::stored`/`is_blocked`, `extensions/contacts.rs`-Grants,
`bridge::events::emit_to_frames`, `FrameEventQueue`, `Desktop::show_notification`,
`windowManager.showExtension`, Aufräumstellen von `shells.end_all`, `vault_changes()`,
`BASE32_NOPAD`, `hmac`. Neu und nötig: `sync/foreign_auth.rs` (gemeinsam mit 056),
`extensions/peers/` (Zustand, Draht, Zulassung, Lobby, Einladung, Brücke), `PeerInvitationDialog.vue`.
