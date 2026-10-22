# Quickstart: Kanäle zwischen Erweiterungen von Hand prüfen

Voraussetzung: zwei Rechner, oder zwei Instanzen nach Spec 030, in getrennten Netzen oder mit
erzwungenem Relay (siehe 5.).

- **Anna**: Vault A auf Gerät A1.
- **Ben**: Vault B auf B1 (Laptop) und B2 (zweites Gerät, verknüpft nach Spec 024).
- **Dave**: Vault D auf D1, **kein** Kontakt von Anna.
- Anna und Ben haben die Visitenkarte des anderen im Adressbuch (Spec 051).
- Auf allen Geräten ist eine Testerweiterung installiert, die `peers` und `contacts` erklärt, Kanäle
  öffnet, Nachrichten zählt und Meldungen anzeigt: die E2E-Probe
  `src-tauri/tests/fixtures/extension_e2e/probe.xt`, in PR 4 um `peers` erweitert. Für haex-doom gilt dasselbe
  mit dem SDK aus `haex-space/vault-sdk`.

Automatische Prüfungen vor der Handprüfung:

```bash
nix develop --command scripts/with-nix-host-bridge.sh cargo test --lib sync::foreign_auth
nix develop --command scripts/with-nix-host-bridge.sh cargo test --lib extensions::peers
nix develop --command scripts/with-nix-host-bridge.sh cargo test --test extension_bridge_contract
nix develop --command pnpm typecheck && nix develop --command pnpm lint
nix develop --command pnpm check:extensions   # FrameEventQueue (scripts/check-extensions-bridge.ts)
nix develop --command pnpm test:e2e -- extension-peers
nix develop --command pnpm format:check
```

## 1. Berechtigung (US3)

1. A1: Testerweiterung installieren. Der Installationsdialog nennt „Verbindungen zu Geräten anderer
   Vaults“ und den Hinweis auf die Netzwerkadresse (FR-003).
2. A1: Berechtigung `peers` auf „verweigert“ → Einladen, Lobby öffnen und Beitreten liefern 1002, im
   Netzmitschnitt verlässt nichts das Gerät. Auf „fragen“ → beim ersten Aufruf fragt holzi.

## 2. Einladung (US1)

1. B1: Testerweiterung offen. B2: Testerweiterung geschlossen.
2. A1: Ben einladen. B1 zeigt holzis Dialog „Anna lädt dich zu ‹Erweiterung› ein“, B2 eine
   Systembenachrichtigung mit „Annehmen“/„Ablehnen“ (SC-001: < 5 s). Die Rahmen auf B1 erhalten nichts.
3. B2: „Annehmen“ in der Benachrichtigung → holzi öffnet die Erweiterung, sie meldet
   `peers:channel-opened`. B1: Dialog verschwindet (SC-009: < 2 s). A1: `accepted` mit `channelId`.
4. Wiederholen, B1 und B2 lehnen ab → A1: einmal `declined`.
5. Wiederholen, niemand antwortet → nach 5 min `expired`, Abfragen verschwinden.
6. B: Anna blockieren, dann A1 lädt ein → B zeigt nichts, A1: `unreachable` mit Hinweis.
7. B1/B2: Erweiterung deaktivieren, A1 lädt ein → A1: `not-installed`.

## 3. Nachrichten und Ende (US2)

1. Mit einem offenen Kanal A1 ↔ B1: Testerweiterung sendet 10 min lang 35 schnelle Nachrichten/s zu
   1.024 Byte je Richtung und alle 5 s eine gesicherte. Erwartet: ≥ 99 % schnelle angekommen, alle
   gesicherten in Reihenfolge (SC-003); Verzögerung über der Laufzeit im Median ≤ 5 ms (SC-004, die
   Testerweiterung misst mit Zeitstempeln und gemessener Umlaufzeit).
2. 1.025 Byte schnell senden → `7000 too_large`.
3. B1: Kanal schließen → A1 `closed` (SC-010: < 2 s). Neu verbinden, B1 Netzwerk trennen → A1 nach
   ≤ 30 s `timeout`. Neu verbinden, A1 Vault sperren → B1 `closed`.

## 4. Lobby mit Code (US4, US5)

1. A1: Lobby „nur Kontakte“ öffnen, Code kopieren. B1 tritt bei → A1 `peers:joined` mit
   `verified: true`. D1 tritt mit demselben Code bei → `1002 not_admitted`.
2. A1: B1 entfernen → B1 `closed`; B1 tritt erneut bei → `not_admitted` (SC-011).
3. A1: Lobby schließen → B2 tritt bei → `1001 lobby_closed`; Kanal zu B1 (falls neu) bleibt.
4. A1: öffentliche Lobby öffnen → D1 tritt bei → `verified: false`. Lobby schließen, D1 verbindet sich
   ohne Code (Testwerkzeug) → sofort getrennt (SC-006).
5. Falscher Code, Code einer geschlossenen Lobby, Code mit geändertem Geheimnis → jeweils
   `lobby_closed`, gleiche Antwortzeit (SC-007, automatisiert in `extensions::peers`).

## 5. Relay

Beide Geräte mit `HOLZI_E2E_SYNC_LOOPBACK=1` (Debug-Build, nur Loopback, `sync/endpoint.rs:56`) und
dem Relay des E2E-Aufbaus nach Spec 043 starten → Kanal entsteht über das Relay (SC-005); Schritte 3.1
und 3.2 wiederholen.

## 6. Android

B2 als Android-Gerät: Benachrichtigung mit Knöpfen; mit verweigertem `POST_NOTIFICATIONS` erscheint der
Dialog im Fenster. App in den Hintergrund während eines Kanals → A1 erhält nach ≤ 30 s `timeout` oder
`lost`.
