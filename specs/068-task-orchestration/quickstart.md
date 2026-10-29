# Quickstart: Orchestrierung prüfen

**Spec**: [spec.md](./spec.md) | **Vertrag**: [contracts/orchestrator.md](./contracts/orchestrator.md)

## Voraussetzungen

- Lokales Modell mit Werkzeugen (Qwen3-4B), Anthropic-Schlüssel (Haiku, Sonnet), Claude Code als
  Delegate, ein Projektordner mit Git-Repo.
- Einstellungen KI → Orchestrierung: an; Orchestrator lokal; Stufen lokal = Qwen3-4B, klein = Haiku,
  groß = Sonnet, Delegate = Claude Code. Freigabestufe „Manuell“. Vor M8 zusätzlich Opus als
  verfügbares Modell einrichten und die Stufe „groß“ auf Opus stellen.

## Automatisch

```sh
nix develop --command scripts/with-nix-host-bridge.sh cargo test -j 4 \
  --manifest-path src-tauri/Cargo.toml chat::
nix develop --command scripts/with-nix-host-bridge.sh cargo test -j 4 \
  --manifest-path src-tauri/Cargo.toml --test chat_parallel_turns
nix develop --command scripts/with-nix-host-bridge.sh cargo test -j 4 \
  --manifest-path src-tauri/Cargo.toml --test orchestrator_tasks
nix develop --command node --test scripts/check-orchestration-*.ts
```

Stufenwahl (SC-002, SC-003), nur Release:

```sh
HOLZI_TEST_GGUF=<qwen3-4b.gguf> nix develop --command scripts/with-nix-host-bridge.sh \
  cargo test --release -j 4 --manifest-path src-tauri/Cargo.toml --test orchestrator_routing_eval \
  -- --ignored --nocapture
```

## Von Hand

| #   | Schritt                                                                         | Erwartet                                                                           | Spec        |
| --- | ------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------- | ----------- |
| M1  | „Wie spät ist es?“                                                              | Antwort ohne Aufgabe                                                               | US3         |
| M2  | „Fasse die drei neuesten Mails von Jonas zusammen“                              | Quittung ≤ 2 s; Karte „klein“ mit Begründung; Bericht in eigenen Worten            | US1         |
| M3  | „Bau im Projekt X einen CSV-Export ein“                                         | Frage nach Projektordner; Delegate im Ordner; Freigaben über den Orchestrator      | US1, FR-026 |
| M4  | „Schick die Präsentation an das Team“                                           | Rückfrage vor der Übergabe; Freigabe vor dem Senden, benannt mit der Aufgabe       | US2         |
| M5  | Drei Aufgaben nacheinander; „Wie weit ist der Bericht?“; „Brich den Bericht ab“ | Alle laufen; Status ohne Unterbrechung; nur der Bericht endet                      | US4         |
| M6  | Aufgabe zu einer Datei, während eine andere dieselbe Datei bearbeitet           | Frage, ob sie warten soll                                                          | FR-021      |
| M7  | Aufgabenkarte aufklappen                                                        | Auftrag, Stufe, Modell, Begründung, Schritte, Rückfragen, Bericht, Tokens getrennt | US6         |
| M8  | „Nochmal, aber mit Opus“                                                        | Neue Aufgabe Stufe groß/Opus; alte bleibt                                          | US7         |
| M9  | Orchestrator auf Haiku stellen, Netz trennen                                    | Kennzeichnung sichtbar; ohne Netz übernimmt lokal, einmal gesagt                   | US5         |
| M10 | Vault sperren während zweier Aufgaben                                           | Beide „abgebrochen“                                                                | Edge Cases  |
