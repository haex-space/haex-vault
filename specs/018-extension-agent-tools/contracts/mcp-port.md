# Vertrag: MCP über den Port einer Erweiterung

holzi ist MCP-Client (`rmcp` 3.5, `extensions/agent_tools/link.rs`), die Erweiterung Server (vault-sdk
`sdk.tools`). Protokollversion fest **`2025-11-25`** (die neueste mit dem `initialize`-Handshake; holzi fragt
sie an, der Server antwortet mit derselben). `clientInfo` ist `{name: "holzi", version: <holzi>}`.

## Transport

| Richtung            | Weg                                                                                                                        |
| ------------------- | -------------------------------------------------------------------------------------------------------------------------- |
| holzi → Erweiterung | Rust-Event `extension-mcp-message {frame, message}` → Frontend → `port.postMessage({type: "haexspace:mcp", message})`      |
| Erweiterung → holzi | `port.postMessage({type: "haexspace:mcp", message})` → `useExtensionFrame` → Command `extension_mcp_send {frame, message}` |

Der native Tauri-Modus des SDK bekommt keinen MCP-Weg: holzi lädt Erweiterungen nur im iframe
(ADR-0004), und kein Host böte das Gegenstück (vault-sdk#64).

`message` ist ein JSON-RPC-2.0-Objekt, unverändert. Rust bestimmt die Erweiterung aus `frame`
(`FrameRegistry`); eine unbekannte Rahmen-Sitzung wird verworfen.

## Methoden

| Methode                     | Richtung        | Pflicht | Verhalten des Servers (vault-sdk)                                                                  |
| --------------------------- | --------------- | ------- | -------------------------------------------------------------------------------------------------- |
| `initialize`                | Client → Server | ja      | Antwort mit `capabilities: {tools: {}}`, `serverInfo` aus Manifest                                 |
| `notifications/initialized` | Client → Server | ja      | keine Antwort                                                                                      |
| `ping`                      | beide           | ja      | `{}`                                                                                               |
| `tools/list`                | Client → Server | ja      | alle registrierten Werkzeuge, die im Manifest stehen; Titel, Beschreibung, Schema aus dem Manifest |
| `tools/call`                | Client → Server | ja      | Eingabe gegen Schema prüfen; Fehler → `isError: true` mit Feld; sonst Handler mit `AbortSignal`    |
| `notifications/cancelled`   | Client → Server | ja      | `AbortSignal` des laufenden Aufrufs auslösen                                                       |
| jede andere Anfrage         | beide           | —       | Fehler `-32601` (method not found)                                                                 |

holzi meldet in `initialize` **keine** Client-Fähigkeiten und beantwortet jede Anfrage des Servers außer
`ping` mit `-32601` (R2).

## Ergebnis eines Aufrufs

Handler-Rückgabe `string` → ein Textblock; Objekt → ein Textblock mit JSON; Ausnahme → `isError: true` mit
der Meldung. holzi verbindet Textblöcke, kürzt auf 64 KiB, und reicht dem Modell
`{"extension": "<Anzeigename>", "data": "<Text>"}` weiter (R10).

## Mitschnitte

`contracts/transcripts/*.jsonl`: je Zeile `{"dir": "c2s" | "s2c", "message": <JSON-RPC>}`. Aufgezeichnet von
`src-tauri/src/extensions/agent_tools/contract_tests.rs` (holzis Client gegen einen In-Memory-Server mit den
Werkzeugen `echo`, `slow`, `fail`, verbunden über JSON wie das Frontend-Relay); `HOLZI_WRITE_MCP_TRANSCRIPTS=1`
schreibt sie neu, sonst scheitert der Test, wenn sie veraltet sind. Versionen in `clientInfo` und `serverInfo`
stehen als `<version>`.

| Datei                           | Inhalt                                                                 |
| ------------------------------- | ---------------------------------------------------------------------- |
| `handshake.jsonl`               | `initialize`, Ergebnis, `notifications/initialized`                    |
| `list.jsonl`                    | `tools/list` und Ergebnis                                              |
| `call-ok.jsonl`                 | `tools/call` `echo` mit Ergebnis                                       |
| `call-invalid-input.jsonl`      | `tools/call` ohne Pflichtfeld → `isError: true` mit Meldung            |
| `call-error.jsonl`              | Werkzeugfehler → `isError: true`                                       |
| `call-cancelled.jsonl`          | `tools/call` `slow`, dann `notifications/cancelled` (nur holzis Seite) |
| `server-request-rejected.jsonl` | `sampling/createMessage` vom Server → `-32601`                         |

Regeln für das Abspielen in vault-sdk: Der Server muss jede `c2s`-Nachricht annehmen, auch das von `rmcp`
gesetzte `params._meta.progressToken`, und auf Anfragen Antworten liefern, die den `s2c`-Zeilen entsprechen,
wobei `serverInfo` frei ist und Texte von `echo`/`fail` die Test-Werkzeuge des Abspielers bestimmen. Auf einen
abgebrochenen Aufruf darf der Server antworten oder schweigen.
