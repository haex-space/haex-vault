# Contract: Hilfsprogramm `--mcp-agent` und IPC-Begrüßung

**Feature**: [../spec.md](../spec.md) | **Research**: R1, R2, R7, R15

## Aufruf

```text
<holzi> --mcp-agent <slug> [--vault <vault-device-uuid>] [--delegate-token <nachweis>]
```

- `<holzi>`: Pfad der ausführbaren Datei, den die Anleitung in holzi einsetzt (`current_exe`;
  unter Linux als AppImage der Wert von `$APPIMAGE`).
- `<slug>`: `[a-z0-9-]{1,32}`; Name der Nachweis-Datei. Die Anleitungen setzen `claude-code`,
  `codex` oder einen vom Nutzer gewählten Namen.
- `--vault`: wählt die Instanz, wenn mehrere holzi-Instanzen laufen (Spec 030). Ohne Angabe: Gibt
  es genau einen Endpunkt, wird er genommen; sonst Fehler „mehrere holzi-Instanzen offen“.
- `--delegate-token`: nur für Delegates, von holzi selbst gesetzt (R7); dann wird keine Datei
  gelesen oder geschrieben.

Das Hilfsprogramm startet keine Tauri-Laufzeit, kein Fenster und keine Vault.

## Ablauf

1. Endpunkt suchen. Sein Name ist `mcp-` und die ersten zwölf Hex-Ziffern der
   `vault_device_uuid` (Pfadgrenze von macOS): Unix-Socket `mcp-<12 hex>.sock` im Laufzeitverzeichnis
   des Benutzers (Verzeichnis `holzi-mcp`, Rechte `0700`), Windows `\\.\pipe\holzi-mcp-<12 hex>`.
   Keiner da → auf das erste `initialize` von stdin mit einem JSON-RPC-Fehler `-32000` und der
   englischen Meldung `holzi is not open` antworten (Rust hält keine lokalisierten Texte),
   Exit-Code 2 (FR-002).
2. Begrüßung senden (eine Zeile JSON):

   ```json
   {
     "hello": 1,
     "slug": "claude-code",
     "token": "<nachweis oder null>",
     "delegate": false
   }
   ```

3. Antwort lesen (eine Zeile JSON), `status`:

   | `status`   | Bedeutung                                            | Hilfsprogramm                                                       |
   | ---------- | ---------------------------------------------------- | ------------------------------------------------------------------- |
   | `accepted` | Nachweis gültig                                      | durchreichen                                                        |
   | `pending`  | Registrierungsdialog offen                           | durchreichen; der Nachweis kommt später als Steuerzeile (Schritt 4) |
   | `rejected` | `reason`: `blocked`, `registration_busy`, `disabled` | auf `initialize` mit Fehler antworten, Exit-Code 3                  |

4. Danach reicht das Hilfsprogramm MCP-Nachrichten zeilenweise durch (MCP über stdio ist
   zeilengetrenntes JSON). In Richtung App → Agent kann eine Zeile eine **Steuerzeile** sein:
   ein JSON-Objekt mit dem einzigen Schlüssel `holzi_control`, das nie eine JSON-RPC-Nachricht
   ist. Das Hilfsprogramm reicht Steuerzeilen nicht weiter:

   - `{"holzi_control": {"token": "<nachweis>"}}`: nach der Registrierung; das Hilfsprogramm
     schreibt ihn in die Nachweis-Datei (Rechte `0600`). Direkt danach folgt
     `notifications/tools/list_changed`.
   - `{"holzi_control": {"closing": "<grund>"}}`: vor dem Schließen durch die App
     (`denied`, `revoked`, `expired`, `vault_closed`, `disabled`); bei `revoked` löscht das Hilfsprogramm
     die Nachweis-Datei.

## Nachweis-Datei

`<Konfigurationsverzeichnis von holzi>/mcp-agents/mcp-<12 hex>/<slug>.token`; eine Zeile, Base64url.
Fehlt sie oder ist der Nachweis ungültig, gilt der Agent als unbekannt (neuer Dialog, bei `expired`
mit vorausgefüllten Rechten).

## Grenzen

- Begrüßung ≤ 4 KiB, Antwort binnen 5 s, sonst Abbruch.
- Eine MCP-Nachricht ≤ 1 MiB (wie HTTP, R14).

## Exit-Codes

| Code | Grund                                                      |
| ---- | ---------------------------------------------------------- |
| 0    | stdin geschlossen (Agent beendet)                          |
| 1    | Ein-/Ausgabefehler                                         |
| 2    | holzi nicht offen / Endpunkt fehlt                         |
| 3    | abgewiesen (Begrüßungsantwort)                             |
| 4    | Verbindung von holzi beendet (Widerruf, Vault geschlossen) |
