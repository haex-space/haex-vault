# Implementation Plan: Kanäle zwischen Erweiterungen auf verschiedenen Vaults

**Branch**: `057-extension-peer-channels` | **Date**: 2026-10-22 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/057-extension-peer-channels/spec.md`

## Summary

Eine Erweiterung mit der Berechtigung `peers` öffnet Kanäle zu derselben Erweiterung auf Geräten
anderer Vaults: per Einladung an einen Kontakt, die der Nutzer in holzis eigener Abfrage annimmt, oder
per Beitrittscode einer Lobby („nur Kontakte“ oder „öffentlich“). Über einen Kanal laufen schnelle,
verlusttolerante und gesicherte Nachrichten, Ende-zu-Ende-verschlüsselt, direkt oder über ein Relay.
Erster Nutzer ist der Mehrspielermodus von haex-doom.

Technischer Ansatz (Begründungen und verworfene Alternativen in [research.md](./research.md)):

- **Eigene ALPN am vorhandenen Endpoint** (R1): `haex-vault-ext-peer/1` am `Router` der Vault-Sitzung,
  eine QUIC-Verbindung je Kanal; `SyncNode::bind` nimmt weitere Protokoll-Handler (auch für 056).
- **Schnelle Nachrichten = QUIC-Datagramme** (R2) bis 1.024 Byte, in der Grundeinstellung von
  iroh/noq aktiv; **gesicherte = Steuerstream** (R3) mit `u32 BE ‖ postcard` wie der Sync.
- **Ausweisen** (R4): gegenseitige Herausforderung mit Geräteschlüssel und unterschriebener
  Geräteliste, Generation ≥ Karte und ≥ gesehen; gemeinsamer Baustein `sync/foreign_auth.rs` für 056
  und 057. Ergebnis `OwnVault`, `Contact` oder `Unverified`.
- **Adressen** (R5): Geräte der gespeicherten Visitenkarte, alle zugleich; „nicht erreichbar“ mit
  Hinweis auf eine aktuelle Karte; später Anklopfen aus 056.
- **Lobbys** (R6, R7): Beitrittscode `haex-vault:join:` mit Endpoint, Relay, zufälliger Lobby-Kennung und
  128-Bit-Geheimnis; Beweis per HMAC über die Handshake-Nonces, zeitkonstant geprüft, eine Antwort für
  alle Fehlschläge; neue Drosselung der Zulassung.
- **Einladungen** (R9, R10): holzis Abfrage im Fenster oder als eigene Benachrichtigung mit Knöpfen;
  erste Annahme gewinnt über `InviteConfirm`/`Withdraw`; die Erweiterung erfährt erst nach der Annahme
  davon (`peers:channel-opened`), notfalls nachdem holzi sie geöffnet hat.
- **Zustellung** (R8): `bridge::events::emit_to_frames`; der Puffer vor der Bereitschaft verwirft
  schnelle Nachrichten zuerst, ein Überlauf gesicherter schließt den Kanal.
- **Lebenszyklus** (R11, R12): `PeerState::end_all` an den Stellen von `shells.end_all`, Abonnent auf
  Berechtigungen und blockierte Vaults; nichts wird gespeichert.
- **ADR 0016** (R17): Geräte fremder Vaults ohne Kontakt nur während einer offenen öffentlichen Lobby.

## Technical Context

**Language/Version**: Rust (Tauri 2.12, Edition und MSRV wie `src-tauri/Cargo.toml`); TypeScript 6
(strict), Vue 3.5, Nuxt 4.5 (SPA)

**Primary Dependencies**: vorhanden `iroh` 1.3.0 (QUIC `noq` 1.3.0, Datagramme), `postcard`, `hmac`,
`sha2`, `data-encoding`, `tokio`, `tokio-util`, `uuid`, `zeroize`, `serde_json`,
`tauri-plugin-notification` (Knöpfe über Aktionstypen). **Keine neue Abhängigkeit.** Frontend:
haex-ui-Dialoge wie `PermissionRequestDialog.vue`

**Storage**: keine Migration, keine neue Tabelle; Zustand nur im Arbeitsspeicher
([data-model.md](./data-model.md)); Berechtigung in den bestehenden Tabellen

**Testing**: `cargo test` (Lib-Tests in `*_tests.rs`; zwei bis drei Vaults im Prozess mit
Loopback-Endpoints und lokalem Relay, `sync/test_support.rs`; Uhr injiziert), Vertragstest
`src-tauri/tests/extension_bridge_contract.rs`, `pnpm check:extensions` (`FrameEventQueue`),
`pnpm typecheck`, `pnpm lint`, `pnpm format:check`, `pnpm lint:rust`; E2E nach Spec 033 mit zweiter
Vault (`scripts/e2e/scenarios/extension-peers.test.ts`); Handprüfung [quickstart.md](./quickstart.md)

**Target Platform**: Linux, macOS, Windows, Android (nichts `cfg(desktop)`); iOS hat in holzi noch
keinen Build (Spec 043, „iOS (eigene Spec)“)

**Project Type**: Desktop- und Mobil-App (Tauri) mit Erweiterungs-Host

**Performance Goals**: 3 Kanäle × 35 schnelle Nachrichten/s × 1.024 Byte je Richtung, ≥ 99 %
angekommen (SC-003); Zusatzverzögerung ≤ 5 ms Median, ≤ 15 ms p95 (SC-004); Einladung sichtbar < 5 s
(SC-001); Kanal bereit < 3 s (SC-002); Ende gemeldet < 2 s bzw. ≤ 30 s (SC-010)

**Constraints**: nichts gespeichert oder protokolliert außer der Berechtigung (FR-042); keine
öffentliche Adressauflösung (056 FR-010); einheitliche hostseitige Antwort für Codes, auch zeitlich (FR-025,
SC-007); Zulassung binnen 5 s (FR-032); Datagramme ≤ 1.024 Byte, gesicherte ≤ 64 KiB; keine
blockierende Arbeit auf dem Executor (Brückenmethoden laufen schon auf blockierenden Threads,
Netzarbeit async); keine Netzdienste und kein `sleep` in `cargo test`; ts-rs-Typen mit Präfix
`ExtensionPeer`

**Scale/Scope**: wenige Kanäle je Gerät (Grenze 16 je Erweiterung), Lobbys mit bis zu 4 Spielern,
Einladungen an Kontakte mit bis zu 32 Geräten (`card::format::MAX_DEVICES`)

## Constitution Check

_GATE: Must pass before Phase 0 research. Re-check after Phase 1 design._

| Regel                                                          | Stand                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| -------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Worktree auf Topic-Branch, PR nach `main`                      | ✅ `.worktrees/057-extension-peer-channels`, Branch `057-extension-peer-channels`; Lieferungen als eigene PRs, Rebase- oder Merge-Commit, kein Squash                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| Speckit-Ablauf                                                 | ✅ specify → clarify (Betreiber, 2026-10-22) → Freigabe → plan; danach tasks, analyze, implement                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                      |
| Tests in eigenen Dateien                                       | ✅ jede neue Rust-Datei mit `*_tests.rs` daneben; Aufbau mehrerer Vaults in `extensions/peers/test_support.rs`                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                        |
| 500 LoC je Datei                                               | ✅ Aufteilung unten (Handshake, Draht, Kanal, Zulassung, Lobby, Einladung, Abfrage, Brücke je eigene Datei). `sync/endpoint.rs` hat schon 647 Zeilen mit dokumentierter Ausnahme; die Handler-Liste kommt in eine neue Datei `sync/endpoint_protocols.rs`, `endpoint.rs` wächst nur um die Übergabe an den `Router`                                                                                                                                                                                                                                                                                                                                                                   |
| Graphify zuerst / Wiederverwendung                             | ✅ Graph des Abzweigs (`7165f840`) abgefragt (research R18). Wiederverwendet: `SyncNode`/`Router`/`MemoryLookup`, `dial_link`-Muster, `wire::write_value`/`read_value`, `signing`, `device_list::check_pushed`/`valid_lists`/`effective`, `card::received::stored`/`is_blocked`, Grants aus `extensions/contacts.rs`, `emit_to_frames`, `FrameEventQueue`, `Desktop::show_notification`, `showExtension`, Aufräumstellen von `shells.end_all`, `vault_changes()`, HMAC wie im Link-Code. Geprüft und nicht passend: Sync-Handshake (lehnt fremde Vaults ab), Erweiterungs-Benachrichtigungen (brauchen die Berechtigung der Erweiterung), `?invite=`-Öffnen (Klick trägt keinen Pfad) |
| Laziness Ladder / keine unnötige Abhängigkeit                  | ✅ keine neue Abhängigkeit; keine eigene Transport-Konfiguration; ein Kanal = eine Verbindung statt eigener Multiplex-Schicht                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                         |
| Eine Implementierung je Regel                                  | ✅ Ausweisen fremder Geräte einmal in `sync/foreign_auth.rs` (056 nutzt es mit, research R4); Zulassung an genau einer Stelle (`extensions/peers/admission.rs`); Aufräumen über `PeerState::end_all`                                                                                                                                                                                                                                                                                                                                                                                                                                                                                  |
| Kein `unwrap`/`expect` auf Eingaben, Fehler nicht verschlucken | ✅ Draht-Eingaben einmal geparst und begrenzt; nach außen nur Zahlencode und `reason` (FR-043), intern mit Ursache geloggt, ohne Inhalte (FR-042)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                     |
| Async nicht blockieren                                         | ✅ Datenbank-Lesen im Handler über `read_blocking` auf dem Blocking-Pool; Netz in Tasks auf dem Tracker der Sitzung                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                   |
| `ponytail:`-Markierungen                                       | ✅ Drosselung mit festen Fenstern (R7), ein Brückenaufruf je Datagramm (R8), gesehene Generation nur im Arbeitsspeicher (R4)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                          |
| I. No Secrets in Git                                           | ✅ Lobby-Geheimnisse nur im Arbeitsspeicher (`Zeroizing`); Testschlüssel zur Laufzeit erzeugt                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                         |
| II./III. Keine lokalen Pfade, Identität geräteunabhängig       | ✅ keine Pfade in Konfiguration; Verweis auf haex-doom mit Repository, voller SHA und Pfad                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                            |
| IV. Fremde Inhalte mit fester Revision                         | ✅ Entwurf von haex-doom gepinnt auf `a076b69321d0cfd63baa23d9b628620e1866e4f3`                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                       |
| VII. Ohne Relay weiterarbeiten                                 | ✅ Ohne Relay bleibt alles Lokale nutzbar; Kanäle dann nur direkt, sonst „nicht erreichbar“; ein Endpoint, der nicht bindet, lässt nur die Funktionen dieser Spec mit `network_unavailable` scheitern (FR-044)                                                                                                                                                                                                                                                                                                                                                                                                                                                                        |
| VIII. Keine Verschleierung                                     | ✅ nicht berührt                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                      |
| ADR für prinzipielle Entscheidungen                            | ✅ ADR 0016 „Devices of non-contact vaults for public lobbies“ (accepted), erweitert ADR-0015 nur für `haex-vault-ext-peer/1` während einer öffentlichen Lobby                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                        |
| Keine Hinweise auf Agents in Commits und PRs                   | ✅ Commits und PRs ohne Trailer                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                       |
| Phasen-Reihenfolge                                             | ✅ baut auf 017, 024 und 051 auf, alle in Benutzung. 056 ist keine Voraussetzung: Anklopfen kommt später dazu (research R5); `sync/foreign_auth.rs` entsteht hier oder in 056, je nachdem, was zuerst umgesetzt wird                                                                                                                                                                                                                                                                                                                                                                                                                                                                  |

Nach Phase 1 erneut geprüft: keine Verletzung, Complexity Tracking leer. Hinweis: `CLAUDE.md` mit
Speckit-Markern gibt es im Repository nicht; der Schritt „agent context update“ entfällt.

## Project Structure

### Documentation (this feature)

```text
specs/057-extension-peer-channels/
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/
│   ├── bridge.md            # Methoden, Meldungen, Fehler, Befehle für holzis Fenster
│   ├── wire-protocol.md     # haex-vault-ext-peer/1: Handshake, Zulassung, Kanal, Beitrittscode
│   └── sdk.md               # client.peers im vault-sdk und Unterschiede zum Entwurf von haex-doom
├── checklists/requirements.md
└── tasks.md                 # /speckit-tasks
docs/adr/0016-non-contact-devices-for-public-lobbies.md
```

### Source Code (repository root)

```text
src-tauri/src/
├── sync/
│   ├── wire.rs                           # EXT_PEER_ALPN
│   ├── signing.rs                        # Domain::ExtPeerAuth
│   ├── foreign_auth.rs (+ _tests)        # Hello/Proof, Listenprüfung, Generation, Einordnung (gemeinsam mit 056)
│   ├── endpoint.rs                       # Handler-Liste an den Router, dial_alpn
│   ├── endpoint_protocols.rs (+ _tests)  # weitere ALPN-Handler registrieren
│   └── service.rs                        # Handler für haex-vault-ext-peer/1 bauen, PeerState an die Sitzung binden
├── extensions/
│   ├── peers/
│   │   ├── mod.rs                        # PeerState, Grenzen, Fehlerabbildung
│   │   ├── state.rs (+ _tests)           # Kanäle, Lobbys, Einladungen, PendingOpen, end_all
│   │   ├── wire.rs (+ _tests)            # Nachrichten-Enum, JoinCode lesen/schreiben, HMAC-Beweis
│   │   ├── channel.rs (+ _tests)         # Datagramme, Steuerstream, Gründe beim Ende
│   │   ├── admission.rs (+ _tests)       # ProtocolHandler: Handshake, Zulassungsregeln, Zeitgrenzen
│   │   ├── throttle.rs (+ _tests)        # Drosselung mit injizierter Uhr
│   │   ├── lobby.rs (+ _tests)           # öffnen, schließen, beitreten, entfernen
│   │   ├── invite.rs (+ _tests)          # Wählen der Kartengeräte, erste Annahme gewinnt, Status
│   │   ├── prompt.rs (+ _tests)          # Einladungsabfrage: Fenster-Ereignis oder Benachrichtigung
│   │   ├── watch.rs (+ _tests)           # Abonnent: Berechtigung entzogen, Vault blockiert
│   │   ├── bridge.rs (+ _tests)          # extension_peers_* Handler
│   │   └── test_support.rs               # zwei/drei Vaults, Loopback, lokales Relay
│   ├── permissions/
│   │   ├── model.rs                      # PermissionKind::Peers, Action::Connect
│   │   ├── manifest_map.rs               # "peers" → Peers
│   │   └── target.rs                     # Peers nur "*"
│   ├── bridge/allowlist.rs               # extension_peers_* eintragen
│   ├── host.rs                           # ExtensionHost.peers
│   └── commands/
│       ├── frames.rs                     # letzter Rahmen → peers.end_all; Rahmen offen → PendingOpen
│       ├── manage.rs, dev.rs             # deaktivieren/entfernen → peers.end_all
│       └── peers.rs                      # extension_peer_invitation_answer, extension_peers_frame_overflow
├── extensions/registry/lifecycle.rs      # auf anderem Gerät gestoppt → peers.end_all
└── lib.rs                                # Befehle registrieren

src-tauri/tests/
├── extension_bridge_contract.rs          # neue Methoden
└── fixtures/extension_e2e/peers.xt       # E2E-Probe mit peers-Berechtigung

src/
├── components/extensions/PeerInvitationDialog.vue   # „[Kontakt] lädt dich zu [Erweiterung] ein“
├── composables/useExtensionHost.ts       # Ereignisse extension-peer-invitation(-gone)
├── lib/extensions/bridge.ts              # FrameEventQueue: peers:message (schnell) verwerfbar, Überlauf melden
├── components/extensions/PermissionSummary.vue, InstallDialog.vue  # Text für peers (FR-003)
└── i18n/locales/{de,en}.json             # extensions.peers.*, extensions.permissions.peers

scripts/
├── check-extensions-bridge.ts            # Fälle der Warteschlange
└── e2e/scenarios/extension-peers.test.ts # zwei Vaults: Einladung, Kanal, Ende

specs/017-extension-host/contracts/bridge.md  # Methoden und Meldungen eintragen
```

**Structure Decision**: Alles der Funktion unter `extensions/peers/`, weil Kanäle an Erweiterungen
gebunden sind und deren Zustand (`ExtensionHost`) und Aufräumstellen brauchen. `sync/` bekommt nur, was
allgemein zum Endpoint gehört: ALPN, Domäne, das Ausweisen fremder Geräte und die Registrierung weiterer
Handler, damit die Grenze von 024 bleibt und 056 dieselben Teile nutzt. Frontend in den bestehenden
Ordnern des Erweiterungs-Hosts.

## Lieferungen (PRs)

Jede Lieferung ist für sich prüfbar und wird vor der nächsten gemergt. Das SDK
(`haex-space/vault-sdk`, `client.peers`, [contracts/sdk.md](./contracts/sdk.md)) ist eine eigene
Änderung dort und kann gegen den Vertrag parallel zu PR 2–4 entstehen; nutzbar ist es ab PR 4.

| PR  | Inhalt                                                                                                                                                                                                                                                                                                                                                            | Prüfbar durch                                                                                                                                                                                                                                                       | Stories / FRs                                                                                       |
| --- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------- |
| 1   | Berechtigungsart `peers`; `EXT_PEER_ALPN`, `Domain::ExtPeerAuth`, Handler-Liste am Router, `dial_alpn`; `sync/foreign_auth.rs`; `extensions/peers/` mit `state`, `wire`, `channel`, Grundgerüst `admission` (nur `OwnVault`/`Contact`); Datagramme und Steuerstream; Gründe beim Ende; `end_all` an allen Aufräumstellen und Abonnent für Berechtigung/Blockieren | Integrationstests mit zwei Vaults: Ausweisen (eigene, Kontakt, blockiert, veraltete Liste, falsche Gegenstelle), Kanal über Test-Hilfe öffnen, Datagramme direkt und über Relay (`max_datagram_size ≥ 1024`), gesicherte Reihenfolge, Last 3 × 35/s, Ende mit Grund | US2, US3 (ohne Brücke); FR-001, FR-004, FR-007, FR-008, FR-029, FR-030, FR-034–FR-040, FR-042       |
| 2   | Lobbys und Beitrittscodes (`lobby.rs`, `JoinCode`, HMAC-Beweis), Zulassung für `Unverified` nur bei öffentlicher Lobby, 5-s-Grenze, Sperren nach Entfernen, Drosselung (`throttle.rs`), ADR 0016                                                                                                                                                                  | Tests: alle hostseitigen Antworten von FR-025 gleich (Inhalt und Mediane), Nicht-Kontakt in „nur Kontakte“, Entfernter, Fremder ohne öffentliche Lobby getrennt, Drosselung mit injizierter Uhr                                                                     | US4, US5; FR-020–FR-028, FR-031–FR-033                                                              |
| 3   | Einladungen: Wählen der Kartengeräte, `Invite`/`InviteReply`/`InviteConfirm`/`Withdraw`, Status, Ablauf, Hinweis; Einladungsabfrage (`prompt.rs`, `PeerInvitationDialog.vue`, Benachrichtigung mit Knöpfen, Fallback im Fenster), Öffnen der Erweiterung und `PendingOpen`                                                                                        | Tests mit einem Kontakt mit zwei Geräten (erste Annahme gewinnt, gleichzeitige Annahme, Ablehnen auf allen, blockiert → unreachable, nicht installiert); Dialog von Hand nach quickstart.md 2                                                                       | US1; FR-009–FR-019                                                                                  |
| 4   | Brücke: `extension_peers_*` in der Erlaubtliste, Meldungen, Fehlerabbildung, Grenzen je Erweiterung; `FrameEventQueue` und `extension_peers_frame_overflow`; Texte für Installation und Abfrage (FR-003); Vertrag in 017 `bridge.md`; Vertragstest; Probe-Erweiterung; E2E                                                                                        | Vertragstest, `pnpm check:extensions`, E2E `extension-peers`, Handprüfung quickstart.md vollständig                                                                                                                                                                 | alle Stories über die Brücke; FR-002, FR-003, FR-005, FR-006, FR-041, FR-043, FR-044; SC-001–SC-012 |

## Offene Risiken

- **Datagrammgröße über Relay-Pfade**: 1.024 Byte sollten passen; PR 1 misst es. Ist es knapper,
  sinkt die Grenze vor PR 4 (die Spec verlangt ≥ 1.000).
- **Heim-Relay gewechselt**: Bis zum Anklopfen aus 056 sind solche Geräte nur mit neuer Karte
  erreichbar (bewusst, Clarifications 2026-10-22).
- **Fremde Relays**: Ein Beitretender muss das Relay des Gastgebers erreichen, auch wenn es nicht in
  seinen eigenen Einstellungen steht. iroh verbindet sich dafür mit dem Relay der Gegenadresse; hat der
  Nutzer alle Relays abgeschaltet, kann das ausbleiben. PR 2 testet es.
- **Zeitgleichheit (SC-007)**: Messung im Test kann auf geteilten CI-Maschinen schwanken; der Test
  vergleicht Mediane über 1.000 Läufe und läuft ohne Netz.
- **Umbenennung (054)**: ALPN, Signaturdomäne und Code-Präfix tragen schon das neue Präfix nach dem
  Schema von 054 (`haex-vault-…`); 054 muss sie nicht mehr anfassen.
- **Reihenfolge mit 056**: Wer `sync/foreign_auth.rs` und die Handler-Liste zuerst baut, legt die Form
  fest; der zweite passt sich an. 056 research R4 und R5 beschreiben dieselben Bausteine noch als
  eigene Teile von `holzi-files/1`; das wird beim Umsetzen der zweiten Spec angeglichen.
- **Brückendurchsatz**: ein Aufruf je Datagramm (R8); wird SC-004 verfehlt, kommt das Bündeln.

## Complexity Tracking

Keine Verletzungen.
