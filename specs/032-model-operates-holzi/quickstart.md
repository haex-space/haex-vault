# Quickstart: Modell bedient holzi über die Aktionen

**Spec**: [spec.md](./spec.md) | **Plan**: [plan.md](./plan.md)

Validierungsleitfaden, kein Bauplan. Jeder Abschnitt nennt Voraussetzung,
Aufruf und erwartetes Ergebnis. Rust-Aufrufe brauchen die Umgebung aus dem
Memory „cargo in holzi worktrees“ (`nix develop`, Host-Bridge); im Worktree
gilt ein echtes `pnpm install` (kein Symlink auf `node_modules`).

## 1. Automatische Prüfungen (CI-gleich)

```bash
pnpm check:agent-actions      # neu: Namen, Felder, Geheimnis-Stichprobe, Schnappschuss, Runner-Politik
pnpm check:wm-navigation      # Regression: check-wm-actions.ts bleibt grün
pnpm check:chat-state
pnpm check:templates
pnpm typecheck && pnpm typecheck:scripts && pnpm lint && pnpm format:check
cargo test --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml --no-default-features
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
```

Erwartet: alles grün. `check:agent-actions` schlägt fehl, wenn eine
Katalog-ID den Werkzeugnamen verletzt, `tools.json` veraltet ist oder ein
Beispielsatz eine unbekannte Aktion nennt.

## 2. Freigabe-Matrix (US2, FR-007)

Rust-Tests (`permission_tests.rs`, `tests/chat_tool_loop_permissions.rs`)
prüfen alle neun Zellen (3 Modi × 3 Stufen), die Leitplanken-Sperre
und das Verhalten nach Ablehnung. Erwartet: Manuell fragt immer; Auto lässt
`Safe` und `Change` laufen und fragt bei `Risky`; Plan lässt nur `Safe` laufen.

## 3. Ablauf mit Ersatz-Frontend (US1, US2)

`tests/action_bridge.rs` startet den Zug mit einem Stub-Adapter, der
`wm_state_get` ruft, und beantwortet `action-call-request` in der Testschleife.
Erwartet: Verlaufszeilen `tool_call` / `tool_result` mit Quelle `action`;
Zeitüberschreitung, Tresor-Schließen und späte Antwort liefern die in
[contracts/tauri-commands.md](./contracts/tauri-commands.md) genannten Fehler.

## 4. Manuell in der App (US1 bis US4)

Voraussetzung: `pnpm tauri:dev` (nix-Devshell), ein Tresor, ein Modell:
einmal Claude per API-Key, einmal das lokale Qwen3-4B.

1. Modus „Manuell“: „Öffne die Sync-Einstellungen“ → Freigabedialog in
   Klartext (Aktionstitel, Eingaben), nach „Erlauben“ öffnet sich die
   Einstellungs-App an der richtigen Stelle, das Modell bestätigt.
2. „Welche Tabs sind offen?“ → Verlauf zeigt eine `wm_state_get`-Zeile mit
   Ergebnis; die Antwort nennt die echten Tabs.
3. Modus „Auto“: „Stelle auf das dunkle Farbschema um“ → läuft ohne
   Rückfrage; „Schließe den Tab mit den Einstellungen“ → fragt (zerstörend).
4. Modus „Plan“: dieselbe Farbschema-Bitte → Ablehnung `blocked_by_plan_mode`,
   das Modell erklärt, Lesen geht weiter.
5. „Setze den Sync-Server auf …“ → das Modell hat dafür kein Werkzeug und
   erklärt, dass der Nutzer es selbst einstellen muss (Leitplanke), in jedem
   Modus. Fordert es die Aktion dennoch an, lehnt der Runner sie ab.
6. Ein Modell ohne Werkzeug-Vorlage wählen → Chat antwortet normal, Hinweis
   „kann holzi nicht bedienen“ (einmal).
7. Ein frisch geladenes lokales Modell mit Werkzeug-Vorlage wählen → sofort
   chatbar mit Unzuverlässigkeits-Hinweis; nach einem bestandenen Selbsttest
   (≤ 2 Minuten, vorläufig `SELF_TEST_PASS = 0.6`) wechselt der Wert auf
   „Supported“, und der Hinweis erscheint bei der nächsten Unterhaltung nicht
   mehr. Bei einem Ergebnis unter der Schwelle wechselt der Wert auf
   „Unsupported“; das Modell bleibt ohne holzi-Werkzeuge chatbar und erklärt,
   dass es holzi nicht bedienen kann.
8. Den MCP-Zugang einschalten, ein Profil mit MCP-Werkzeugen aktivieren und Claude Code oder Codex
   wählen → keine Registrierungsfrage; die Werkzeuge richten sich nach dem Profil des gewählten
   Abo-Agent-Modells. Den MCP-Zugang danach ausschalten und erneut wählen → ein Hinweis erklärt,
   dass holzi-Werkzeuge über den MCP-Zugang nicht verfügbar sind.

## 5. Messlauf je Modell (US5, SC-001, SC-002, SC-005, SC-008)

```bash
# lokal (braucht ein heruntergeladenes GGUF, kein Tresor)
HOLZI_TEST_GGUF=<pfad-zum-gguf> \
  cargo test --manifest-path src-tauri/Cargo.toml --test model_tool_eval -- --ignored --nocapture

# Cloud: Anbieter und Schlüssel aus der Umgebung, nie aus dem Repository
HOLZI_EVAL_PROVIDER=anthropic HOLZI_EVAL_MODEL=<modell-id> \
  ANTHROPIC_API_KEY=<schluessel> \
  cargo test --manifest-path src-tauri/Cargo.toml --test model_tool_eval -- --ignored --nocapture
```

Erwartet: `target/eval/<modell>.json` mit Quoten je Gesamt, Sprache und Art,
`reachRate` und Fehltreffern. Zweimal ausführen und die Abweichung der
Gesamtquote prüfen (≤ 10 Prozentpunkte). Die Ergebnisse tragen die erste
Mindestquote des Selbsttests und die Katalogeinträge (`tool_use`) nach.
