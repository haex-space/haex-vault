# Implementation Plan: External Bridge für haex-pass-browser

**Branch**: `049-external-bridge` | **Date**: 2026-10-21 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/049-external-bridge/spec.md`

## Summary

holzi bekommt einen Loopback-WebSocket-Dienst, mit dem `haex-pass-browser` aus haextension ohne
Änderung am Protokoll ausfüllt, TOTP-Codes holt, Zugangsdaten speichert und Passkeys benutzt. Alles
geht durch den Passwortdienst aus 034 und 036, mit einem eigenen Aufrufer je Client und dessen
Freigabe. Den Umfang bestimmen die Spec und die Clarification vom 2026-10-21:

- nur die Methoden, die der Client auf `main` nutzt; `update-item` und Lesezeichen sind „nicht
  unterstützt“;
- ein kleiner Begleit-PR in haextension;
- Einstellungen unter „Erweiterungen“.

Technischer Ansatz (Begründungen und verworfene Alternativen in [research.md](./research.md)):

- **Transport und Herkunft** (R1, R2): `tokio-tungstenite` 0.28 (steht schon im Lockfile) auf
  `127.0.0.1` und `[::1]`, Port 19455.
  - Der Header `Origin` muss `chrome-extension://…` oder `moz-extension://…` sein, sonst antwortet
    holzi vor dem Upgrade mit HTTP 403.
  - Grenzen: 64 KiB je Nachricht, 16 Verbindungen, 10 s bis zum Handshake.
- **Umschläge** (R3):
  - X25519 über `curve25519-dalek` 5 (`mul_clamped`, steht im Baum) und AES-256-GCM über
    `aes-gcm` 0.11 (steht im Lockfile).
  - Rohes gemeinsames Geheimnis als Schlüssel, wie Client und Vorlage.
  - Die Kennung prüft holzi gegen `SHA-256(publicKey)`.
- **Verbindung** (R4): Zustandsautomat je Verbindung, wiederholte Handshakes ohne zweiten Dialog,
  bis zu 4 Anfragen parallel, `requestId` je Verbindung, Widerruf mit `authorizationUpdate` und
  Schließen.
- **Aufrufer** (R5): `Caller::BrowserClient { id }` mit genau einer `Grant` aus der Zulassung. Das
  Standard-Tag hängt die Bridge selbst an.
- **Methoden** (R6 bis R9): eine Tabelle mit Modulpfaden und Vertragstest nach dem Muster von 017
  (FR-041).
  - `get-items`: Kopfdaten, Filter nach registrierbarer Domain, Einzelabfragen.
  - `totp_code` und `preset_list` im Dienst werden für Aufrufer mit Freigabe geöffnet.
  - `create-item` liest vier Felder.
- **Rückfragen** (R10): geparkte `oneshot`-Dialoge mit 120 s für Zulassung, Schreiben und
  Anwesenheit; sie enden mit „Ablehnen“ bei Ablauf, Ende der Verbindung und Schließen der Vault.
- **Passkeys** (R11, R12): Der Passkey-Dienst bekommt `UserPresence` (setzt UP/UV erst nach einer
  Bestätigung) und `PasskeyTarget::New` (Eintrag und Passkey in einer Transaktion).
  - UV nur mit erneut eingegebenem Vault-Passwort über das vorhandene `verify_current`.
  - ADR-0015 löst ADR-0009 in diesem Punkt ab.
- **Speicher** (R13): Ein/Aus und Port als Einstellungen des Geräts; gemerkte Zulassungen und
  Sperren in der neuen CRDT-Tabelle `haex_browser_clients` mit Fremdschlüssel auf das Gerät
  (ADR-0001).
- **Lebenszyklus** (R14): Start bei jedem Öffnen der Vault, Stopp über das Gate-Token.
- **Plattformen** (R15): Android über `PlatformCapabilities.browser_bridge = false`.
- **Begleit-PR in haextension** (R19): Antwort von `passkey-list`, neutrale Texte, Lesezeichen als
  „nicht verfügbar“. Danach ein neuer Pin und Release `haex-pass-browser-v1.3.0`.

## Technical Context

**Language/Version**: Rust (Tauri 2.12), TypeScript 6 (strict), Vue 3.5, Nuxt 4.5.2 (SPA),
Node 22.19 für Prüfskripte und das Fixture-Skript.

**Primary Dependencies**:

- **Vorhanden**: `tokio` (net, sync, macros), `tokio-util` (`CancellationToken`, `TaskTracker`
  über `VaultGate`), `sha2` 0.11, `base64` 0.23, `psl`, `url`, `p256`, `ed25519-dalek` 3,
  `ciborium`, `zeroize`, `serde_json`, `ts-rs`, `log`, `tauri-plugin-notification`; im Frontend
  Pinia, `@nuxtjs/i18n`, haex-ui-Layer (`Group`, `Row`, `OptionRow`, `ShadcnDialog`).
- **Direkt neu, aber schon im Lockfile**: `tokio-tungstenite` 0.28 (über `nostr-sdk`),
  `aes-gcm` 0.11.1, `curve25519-dalek` 5.0.0 (über `ed25519-dalek`).
- **Keine neue Crate im Baum, keine neue Frontend-Abhängigkeit.**

**Storage**:

- Migration mit der nächsten freien Nummer (bei Planung `0038_browser_clients`): eine
  CRDT-Tabelle `haex_browser_clients`; `HOLZI_TRIGGER_VERSION` 25 → 26.
- Zwei Einstellungen des Geräts.
- Keine Änderung vorhandener Tabellen.
- Details in [data-model.md](./data-model.md).

**Testing**:

- **Rust, Einheit** (`*_tests.rs`): `browser_bridge/{envelope,handshake,origin,declaration,
methods}`, `passwords/site`, `webauthn` (Flags).
- **Rust, Integration**: `src-tauri/tests/browser_bridge.rs` (Protokoll gegen eine Test-Vault mit
  Test-Client) und `browser_bridge_contract.rs` (FR-041); `passwords_passkeys.rs` wird erweitert.
- **Interoperabilität**: Fixture aus WebCrypto (R18) mit `pnpm check:browser-bridge`.
- **End-to-End**: neues Szenario `browser-bridge` mit dem Feature-`e2e`-Binary
  `e2e_bridge_client`.
- **Von Hand**: [quickstart.md](./quickstart.md) M1 bis M11 mit der echten Erweiterung in
  Chromium und Firefox.
- **Regression**: `extension_bridge_contract`, `passwords_*`, `check:passwords`, `typecheck`,
  `lint`, `format:check`.

**Target Platform**: Tauri-Desktop (Linux, macOS, Windows). Android baut den Code, bietet die
Bridge aber nicht an (FR-043).

**Project Type**: desktop-app (Nuxt-SPA + Rust-Backend in einem Tauri-Projekt).

**Performance Goals**:

- `get-items` bei 5 000 Einträgen und 20 Treffern grob unter 500 ms (SC-002).
- Widerruf beim Client in unter 1 s (SC-007).
- Port nach Ausschalten oder Schließen in unter 2 s frei (SC-008).

**Constraints**:

- **Geheimnisse**: keines in Protokoll, Fehlertext, Ereignis oder `Debug`-Ausgabe (FR-039).
- **Antwortfelder**: Antworten bestehen nur aus ausgewählten Feldern, nie aus einem durchgereichten
  Modell (FR-040).
- **Aufrufer**: kommt nie aus dem Inhalt einer Anfrage (FR-014).
- **Rückfragen und Zulassungen**: nur aus dem Fenster; die Katalogaktionen haben den Bereich
  `guardrails` (FR-013).
- **Code-Regeln**: kein `unwrap`/`expect` auf Eingaben; Dateien ≤ 500 Zeilen.
  - Neue Logik kommt in neue Dateien.
  - `passkeys_ops.rs` (426) bekommt `PasskeyTarget::New` in einer eigenen Datei.
  - `settingsActions.ts` (482) bekommt die Aktionen der Bridge in einer eigenen Datei.
- **Testcode**: in eigenen Dateien.

**Scale/Scope**:

- 1 Tabelle, 1 neues Rust-Modul `browser_bridge` (rund 12 Dateien plus Tests), 4 geänderte
  Dienstmethoden.
- 6 Commands, 4 Katalogaktionen, 3 Dialoge, 1 Einstellungsseite.
- 2 Integrationstests, 1 e2e-Szenario.
- 1 PR in haextension.

## Constitution Check

_GATE: Muss vor Phase 0 bestehen. Nach Phase 1 erneut geprüft — Ergebnis unten._

Geprüft gegen `.specify/memory/constitution.md` (v1.4.0) und die spaex-Constitution
`.spaex/constitution.md`.

| Prinzip / Vorgabe                                        | Status | Begründung                                                                                                                                                    |
| -------------------------------------------------------- | ------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| I Keine Geheimnisse in Git                               | ✅     | Fixture-Schlüssel (R18) sind reine Testschlüssel ohne Bezug zu Konten, öffentlich wie ADR-0012; Markierungen für SC-004 sind erfunden                         |
| II Keine lokalen absoluten Pfade                         | ✅     | Quickstart und Verträge nutzen repo-relative Pfade; `/etc/hosts` in M3 ist eine Anleitung, keine Konfiguration                                                |
| III Projektidentität geräteunabhängig                    | ✅     | Nicht berührt                                                                                                                                                 |
| IV Cross-Repo-Referenzen gepinnt                         | ✅     | Vorlage `8dce379d…`, Client `c8133b94…` (neu gepinnt); nach dem Begleit-PR dessen Merge-SHA                                                                   |
| V Externe Quellen nur per Opt-in                         | ✅     | Keine neue Harness-Quelle                                                                                                                                     |
| VI Selbstverändernde Anweisungen review-pflichtig        | ✅     | Keine Änderung an Constitution, Skills oder Berechtigungen                                                                                                    |
| VII Relay-Ausfall blockiert lokale Arbeit nicht          | ✅     | Alles lokal                                                                                                                                                   |
| VIII Keine Verheimlichung                                | ✅     | Der Agent bekommt keine Aktion; Rückfragen sind sichtbare Dialoge                                                                                             |
| Phasendisziplin                                          | ✅     | `plans/README.md:21`: 034 und 036 im Einsatz                                                                                                                  |
| Workflow: speckit, PR, Conventional Commits, kein Squash | ✅     | specify → clarify (2026-10-10, 2026-10-21) → plan → tasks → implement; Worktree je PR                                                                         |
| ADR bei prinzipienrelevanter Entscheidung                | ✅     | ADR-0015 „UP und UV nach zeremoniegebundener Bestätigung“ löst ADR-0009 in diesem Punkt ab (PR E)                                                             |
| Test-Code in separaten Dateien                           | ✅     | `*_tests.rs`, `src-tauri/tests/`, Testclient in der Testdatei                                                                                                 |
| Async nicht blockieren                                   | ✅     | Prüfung des Passworts und Dienstaufrufe über `gate.spawn_blocking`; der Server ist reines tokio                                                               |
| Keine neue Crate ohne Not                                | ✅     | Drei direkte Abhängigkeiten, alle schon im Lockfile; `x25519-dalek` und `axum` verworfen (R1, R3)                                                             |
| 500-LoC-Grenze                                           | ✅     | Siehe Constraints; keine Datei wächst über 500                                                                                                                |
| Graphify vor neuen benannten Artefakten                  | ⚠️     | Abfragen gelaufen (Rückfragen, Domain-Regel, Passwortprüfung, Aufrufer, Vertragstest). Zwei Kandidaten brauchen eine Entscheidung des Betreibers, siehe unten |
| `ponytail:`-Kommentar bei bewusster Vereinfachung        | ✅     | Geplant an: rohes ECDH-Geheimnis ohne KDF (R3), `last_seen_at` bei jedem Handshake (R13), `get-items`-Filter in der Bridge statt im Dienst (R7)               |
| Nicht-triviale Logik hinterlässt einen Check             | ✅     | Siehe Testing                                                                                                                                                 |

### Kandidaten aus graphify (Erweitern statt neu bauen)

| #   | Neues Artefakt                                                  | Vorhandener Kandidat                                                                                                          | Vorschlag                                                                                                                                                                                                                                                                                                                                                                      | Status                                                                                   |
| --- | --------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ---------------------------------------------------------------------------------------- |
| K1  | Geparkte Rückfrage mit Frist (`browser_bridge/prompts.rs`)      | `src-tauri/src/files/agent/prompt.rs:1-60` (`PROMPT_TIMEOUT` 60 s, `ask` 157, `answer` 232) und `chat/tools/action_bridge.rs` | **(a)** Eigenes Modul nach demselben Muster, rund 150 Zeilen, Typen der Bridge. **(b)** Den Mechanismus (Kennung → `oneshot`, Frist, Abbruch über Token) als `prompt_park.rs` mit Typparametern herausziehen und die Bridge als dritten Nutzer darauf bauen. Die zwei vorhandenen Nutzer ziehen in einem eigenen Refactor-PR nach. Erspart rund 80 Zeilen im dritten Exemplar. | **Betreiber entscheidet am Plan-Gate.** Empfehlung (b), weil es das dritte Exemplar wäre |
| K2  | Registrierbare Domain für `get-items` und die Wahl des Eintrags | `src-tauri/src/passwords/webauthn.rs:45-63` `relying_party()` (privat, `psl::suffix_str`)                                     | Nach `passwords/site.rs` verschieben als `site_of(host) -> Option<String>`, mit der Regel für IP und `localhost`. Neue Form der Aufrufstelle: `checked_origin` ruft `site::is_public_suffix(rp_id)` statt `relying_party`                                                                                                                                                      | Empfohlen; gleiche Regel, eine Stelle                                                    |
| K3  | Prüfung des Vault-Passworts für UV                              | `src-tauri/src/instances/passphrase_change.rs:75` `verify_current` (privat)                                                   | `pub(crate)` machen und unverändert benutzen                                                                                                                                                                                                                                                                                                                                   | Klar, keine neue Implementierung                                                         |
| K4  | TOTP und Voreinstellungen für Aufrufer mit Freigabe             | `passwords/service/items.rs:196` `totp_code`, `service/presets.rs:10` `preset_list`                                           | Die Signatur um `grants` erweitern, statt neue Methoden anzulegen; Aufrufstellen der Nutzer-Commands übergeben `&[]`                                                                                                                                                                                                                                                           | Klar                                                                                     |
| K5  | Vertragstest der Wand                                           | `src-tauri/tests/extension_bridge_contract.rs`                                                                                | Eigener Test nach demselben Muster; beide Tabellen haben eigene Typen (`Method` der Bridge mit `need`), also kein gemeinsamer Code                                                                                                                                                                                                                                             | Klar                                                                                     |
| K6  | Aufrufer mit Freigaben                                          | `src-tauri/src/extensions/passwords.rs` `call()` 408-420                                                                      | Vorbild, nicht wiederverwendbar (Freigaben aus `extension_permissions`); die Bridge baut ihre eine `Grant` aus der Zulassung                                                                                                                                                                                                                                                   | Klar                                                                                     |

**Ergebnis nach Phase 1**: keine Verletzung. Die Entscheidung zu K1 fällt am Gate; beide Wege
erfüllen die Constitution.

## Lieferung (jede ein PR, jede auslieferbar)

| PR  | Inhalt                                                                                                                                                                  | Stories  | Berührt                                                                                                                                                     |
| --- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------- |
| A   | Spec-Anpassung 2026-10-21, Plan, Research, Datenmodell, Verträge, Quickstart, Tasks                                                                                     | –        | `specs/049-external-bridge/`, `plans/README.md`                                                                                                             |
| H   | Begleit-PR haextension: `passkey-list`, Texte, Lesezeichen „nicht verfügbar“                                                                                            | –        | `haex-space/haextension` `apps/haex-pass-browser`                                                                                                           |
| B   | Server, Umschläge, Herkunft, Handshake, Zulassungsdialog, Tabelle und Migration, Ein/Aus und Port, Plattform, `Caller::BrowserClient`, Vertragstest (ohne Methoden), K1 | US1, US6 | Rust `browser_bridge/`, `caller.rs`, `identity/`, `platform/`, `instances/{open,create,import}.rs`; Frontend Store, Dialog, Einstellungsseite (Grundgerüst) |
| C   | `get-items`, `get-totp`, Voreinstellungen, `create-item`, Rückfrage Schreiben, nicht unterstützte Methoden, `site.rs` (K2), Dienst (K4)                                 | US2, US3 | `browser_bridge/methods/`, `passwords/site.rs`, `passwords/service/{items,presets}.rs`                                                                      |
| D   | Verwaltung: Liste, Ändern, Widerruf, Sperren aufheben, Portwechsel, Zustandsanzeige                                                                                     | US5      | Frontend Einstellungsseite, Aktionen; Rust `browser_bridge/admin.rs`                                                                                        |
| E   | Passkeys, Anwesenheitsbestätigung, UV (K3), `UserPresence`, `PasskeyTarget::New`, ADR-0015, Hinweis in 036                                                              | US4      | `browser_bridge/methods/passkeys.rs`, `passwords/{webauthn,passkeys_ops,passkeys_new_entry,model_passkeys}.rs`, `docs/adr/`                                 |

Danach kommen Politur, die Handprüfung nach quickstart, der neue Pin auf die Merge-SHA von H und
der Release der Erweiterung. H läuft parallel zu B bis E; M6 und M7 der Handprüfung brauchen H.

## Project Structure

### Documentation (this feature)

```text
specs/049-external-bridge/
├── spec.md                    # angepasst 2026-10-21 (Pin, Umfang, Begleit-PR)
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/
│   ├── wire-protocol.md       # Nachrichten, Umschläge, Methoden, Fehler
│   └── tauri-commands.md      # Commands, Katalogaktionen, Ereignisse, Rückfragen
├── checklists/requirements.md
└── tasks.md                   # Phase 2 (/speckit-tasks)
```

### Source Code (repository root)

```text
src-tauri/src/
├── browser_bridge/                                                   [neu]
│   ├── mod.rs                 # start_for_active_instance, stop, Zustand
│   ├── server.rs              # Listener v4/v6, Origin-Callback, Grenzen
│   ├── connection.rs          # Zustandsautomat je Verbindung, Schreiber, Semaphore
│   ├── envelope.rs            # X25519 + AES-256-GCM, Kennung aus Schlüssel
│   ├── protocol.rs            # serde-Typen der Nachrichten
│   ├── declaration.rs         # kanonische Erklärung, Abbildung auf höchste Freigabe
│   ├── approvals.rs           # gemerkt (Tabelle) + vorläufig (Speicher), Abgleich beim Handshake
│   ├── prompts.rs             # geparkte Rückfragen (K1)
│   ├── methods/mod.rs         # METHODS, NOT_SUPPORTED, Dispatch, Fehlerabbildung
│   ├── methods/items.rs       # get-items, get-totp, create-item, Voreinstellungen
│   ├── methods/passkeys.rs    # passkey-create/-get/-list, Kodierungen
│   ├── admin.rs               # Übersicht, Ändern, Widerruf, Entsperren
│   ├── commands.rs            # Tauri-Hüllen
│   └── *_tests.rs
├── caller.rs                                  # + BrowserClient                      [ändern]
├── passwords/site.rs, site_tests.rs           # site_of (K2)                         [neu]
├── passwords/webauthn.rs                      # Flags aus UserPresence; nutzt site    [ändern]
├── passwords/passkeys_ops.rs                  # presence, target                     [ändern]
├── passwords/passkeys_new_entry.rs            # Eintrag + Passkey in einer Transaktion [neu]
├── passwords/model_passkeys.rs                # UserPresence, PasskeyTarget          [ändern]
├── passwords/service/{items,presets}.rs       # grants-Parameter (K4)                 [ändern]
├── passwords/access.rs, contacts/…, files/…   # Match-Arme für BrowserClient          [ändern, klein]
├── instances/{open,create,import}.rs          # Start der Bridge                     [ändern, klein]
├── instances/passphrase_change.rs             # verify_current pub(crate) (K3)        [ändern, klein]
├── platform/mod.rs                            # browser_bridge                       [ändern, klein]
├── identity/migrations_browser_clients.rs     # 0038                                 [neu]
├── identity/migrations.rs                     # Registrierung, Triggerversion 26     [ändern]
├── bin/e2e_bridge_client.rs                   # Feature e2e                          [neu]
└── lib.rs                                     # Modul, Commands                      [ändern]
src-tauri/tests/browser_bridge.rs, browser_bridge_contract.rs, fixtures/browser_bridge/webcrypto.json [neu]
src-tauri/tests/passwords_passkeys.rs          # UP/UV, PasskeyTarget::New            [ändern]
src-tauri/Cargo.toml                           # tokio-tungstenite, aes-gcm, curve25519-dalek direkt [ändern]

src/
├── stores/browserBridge.ts                    # Rückfragen-Warteschlange, Übersicht  [neu]
├── components/browserBridge/
│   ├── ApprovalDialog.vue, WriteAccessDialog.vue, PresenceDialog.vue               [neu]
│   └── ClientRow.vue, ClientEditor.vue                                             [neu]
├── components/settings/BrowserBridgeSettings.vue                                   [neu]
├── components/wm/Desktop.vue                  # Dialoge einhängen                    [ändern]
├── lib/settings/registry.ts                   # Abschnitt und subView unter extensions [ändern]
├── lib/actions/browserBridgeActions.ts        # 4 Aktionen, guardrails               [neu]
├── composables/useDeviceCapabilities.ts       # browserBridge                        [ändern]
├── i18n/locales/{de,en}.json                  # browserBridge.*                      [ändern]
└── types/bindings/*.ts                        # ts-rs                                [generiert]

scripts/
├── fixtures/browser-bridge-envelope.ts        # WebCrypto-Fixture (R18)               [neu]
├── check-browser-bridge.ts                    # pnpm check:browser-bridge            [neu]
└── e2e/scenarios/browser-bridge.test.ts                                            [neu]

docs/adr/0015-passkey-presence-after-ceremony-bound-confirmation.md                 [neu, PR E]
specs/036-password-redesign/contracts/passkey-service.md  # Hinweis auf 049/ADR-0015 [ändern, PR E]
plans/README.md                                # Zeile 049                            [ändern]
```

**Structure Decision**:

- Die Bridge ist ein eigenes Modul am Rand: Netz, Kryptografie und Dialoge. Es kennt den
  Passwortdienst nur über dessen öffentliche Methoden.
- Der Dienst kennt die Bridge nicht. Er erfährt nur von einer neuen Aufrufer-Variante und von
  `UserPresence`.
- Reine Logik ist ohne Netz und Datenbank prüfbar: Umschläge, Kennung, Herkunft, Erklärung, Site
  und Fehlerabbildung.

## Complexity Tracking

| Aufwand                                                      | Warum nötig                                                                                                               |
| ------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------- |
| Eine neue CRDT-Tabelle und Migration                         | Gemerkte Zulassungen und Sperren je Gerät (FR-009, ADR-0001); `_no_sync` scheidet aus (R13)                               |
| Drei direkte Rust-Abhängigkeiten (alle im Lockfile)          | WebSocket-Server mit Zugriff auf den Upgrade-Header, X25519 und AES-GCM, wie es der unveränderte Client verlangt (R1, R3) |
| Eine neue Aufrufer-Variante                                  | Eigener Eingang mit eigenen Freigaben (ADR-0007, R5)                                                                      |
| Änderung am Passkey-Dienst (`UserPresence`, `PasskeyTarget`) | FR-031, FR-033, FR-034; ADR-0015                                                                                          |
| Begleit-PR in einem anderen Repository                       | Ohne ihn erreicht keine Passkey-Anmeldung holzi (R19)                                                                     |

## Bewusste Grenzen (aus research.md)

- **Schlüsselableitung**: Das Protokoll leitet keinen Schlüssel ab und authentisiert `action`
  nicht. Das bleibt aus Kompatibilitätsgründen (R3).
- **Besitz des privaten Schlüssels**: Der Client weist ihn nicht nach. Ein lokales Programm, das
  Fingerabdruck, Schlüssel und Herkunft kennt, kann Anfragen stellen, aber keine Antwort lesen
  (Spec, Assumptions).
- **Adressen mit Verweis**: Sie sind für `get-items` keine Treffer (R7).
- **`last_seen_at`**: wird bei jedem zugelassenen Handshake synchronisiert, obwohl nur dieses Gerät
  es liest (R13).
- **Algorithmus**: Passkeys entstehen immer mit ES256, solange die Erweiterung keine Algorithmen
  mitschickt (FR-034).
- **Echte Browser**: Die Erweiterung in Chromium und Firefox wird nur von Hand geprüft (R17); die
  Kryptografie sichert die WebCrypto-Fixture ab (R18).
