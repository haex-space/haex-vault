# Quickstart: CLI Delegate Backend manuell verifizieren

Manuelle Verifikations-Schritte für Reviewer/Operator vor dem Merge. Setzt eine lokale Installation
von `claude` (Pro/Max/Team-Abo) und/oder `codex` (ChatGPT-Abo) voraus — ohne beide CLIs lässt sich
nur der jeweils installierte Vendor-Pfad manuell prüfen.

## Vorbereitung

```bash
git switch 007-cli-delegate
which claude codex          # mindestens einer muss vorhanden sein
claude --version             # Live-Verifikation in dieser Planungssession: 2.1.241
codex --version               # Live-Verifikation in dieser Planungssession: codex-cli 0.147.0
pnpm install
cd src-tauri && cargo test --lib && cd ..
pnpm typecheck
```

## Bereits abgeschlossene Verifikationen

Die beiden ursprünglichen Pre-Implementation-Spikes wurden am 2026-09-16 live abgeschlossen und
bleiben als Regressionserwartungen bestehen:

1. **Codex-Live-Approval**: `codex app-server --stdio` pausiert bei
   `item/commandExecution/requestApproval` und setzt nach `{"decision":"accept"}` fort;
   `{"decision":"decline"}` verweigert die Aktion. Die Tests müssen diese Wire-Form verwenden,
   nicht `ExecCommandApprovalRequest`/`ReviewDecision`.
2. **Claude-Code-Isolation, Skills/Plugins**: ein isoliertes `CLAUDE_CONFIG_DIR` lädt keine
   host-seitigen Skills oder Plugins (nur das CLI-Bundleset, null User-Plugins). Credentials und
   Hooks bleiben ebenfalls isoliert. Diese drei Eigenschaften sind Regressionserwartungen.

## Szenario 1: Bestehendes Abo als Chat-Backend nutzen (User Story 1)

1. In den Einstellungen einen Claude- oder Codex-Delegate verbinden (`connect_cli_delegate`) —
   Browser-Flow abschließen.
2. Diesen Delegate als Backend für eine neue Nachricht wählen, eine Frage stellen, die Tool-Nutzung
   erfordert (z.B. "welche Dateien liegen in diesem Verzeichnis?").
3. **Erwartung**: die Antwort basiert auf tatsächlicher Tool-Ausführung des Delegates, erscheint im
   Chat wie jede andere Antwort, und die Konversation zeigt erkennbar, welches Backend geantwortet
   hat (spec.md FR-005).

## Szenario 2: Verbindung ist vault-portabel (User Story 2)

1. Delegate wie in Szenario 1 verbinden.
2. Die Vault-Datei auf eine zweite Maschine kopieren, die **noch nie** `claude login`/`codex login`
   ausgeführt hat.
3. Dort HaexVault mit dieser Vault öffnen, denselben Delegate für eine Nachricht wählen.
4. **Erwartung**: funktioniert ohne weiteren Login-Schritt auf der zweiten Maschine (spec.md
   Acceptance Scenario 2).
5. Credential im Backend absichtlich ungültig machen (z.B. `codex logout` _auf der ursprünglichen
   Maschine_ ändert nichts an HaexVaults eigener gespeicherter Kopie — stattdessen die gespeicherten Bytes
   in der DB testweise verfälschen) und erneut senden. **Erwartung**: klare "Verbindung erneuern"-
   Führung statt Rohfehler (FR-014).

## Szenario 3: Freigabe-Gate gilt live, wie beim eingebauten Tool-Loop (User Story 3)

Für **beide** installierten Backends wiederholen, sofern verfügbar:

**Manual**: Freigabe-Modus auf "Manuell", Delegate wählen, eine Aktion auslösen, die eine Tool-Nutzung
braucht. **Erwartung**: der Freigabe-Dialog erscheint (`tool-permission-request`, unverändertes
Event/Command-Paar aus 003), der Delegate-Prozess pausiert erkennbar (z.B. sichtbar verzögerte
Antwort), bis beantwortet — **nicht** eine Sammelfreigabe vor Start (das war die inzwischen verworfene
erste Antwort auf spec.md's Clarification, siehe dort).

**Auto**: Freigabe-Modus auf "Automatisch". Sichere Lesezugriffe laufen ohne Rückfrage, eine
Schreib-/Kommandoaktion pausiert für eine Freigabe.

**Plan/Block**: Aktionen, die die Posture blockiert, werden dem Delegate gar nicht erst gewährt; keine
Rückfrage erscheint.

## Szenario 4: Host-Isolation (User Story 4)

Auf einer Maschine mit vorhandener eigener `~/.claude/settings.json` (idealerweise mit einem
ungewöhnlichen `permissions.defaultMode` oder einem `SessionStart`-Hook, wie in dieser
Planungssession beim Operator selbst vorgefunden):

1. Delegate-Antwort auslösen.
2. **Erwartung**: das host-eigene Setting/Hook beeinflusst die Delegate-Antwort **nicht** — genau der
   Fehler, der in dieser Planungssession beim ersten (nicht-isolierten) Testlauf tatsächlich auftrat
   und durch `CLAUDE_CONFIG_DIR`-Isolation behoben wurde (research.md §3).
3. Nach Abschluss: Host-Verzeichnis auf neu angelegte Dateien/Settings außerhalb HaexVaults eigener
   Vault-Datei prüfen. **Erwartung**: keine.

## Szenario 5: Abbruch mitten in einer Delegate-Antwort (User Story 5)

1. Eine (spürbar langsame) Delegate-Antwort auslösen.
2. Während sie läuft: `abort_current_generation` aufrufen (Stop-Button).
3. **Erwartung**: der Delegate-Subprozess (`claude`/`codex`) wird sofort beendet — in der
   OS-Prozessliste prüfen, kein Zombie-Prozess —, keine weitere Ausgabe erscheint im Chat.
