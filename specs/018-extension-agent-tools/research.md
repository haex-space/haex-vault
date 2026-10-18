# Research: Werkzeuge von Erweiterungen für den Agenten

Stand der Quellen (2026-10-10): holzi `main` @ `1a213107`; vault-sdk `origin/main` @
`40ea29ce5ee2c913a541e97033e9e885fa6372bb` (Release 4.2.0); holzi pinnt `haex-bundle` auf vault-sdk
@ `36bf6e98f36c2362d42aa2d92c85288a3d91e775` (4.0.0; `crates/` unverändert bis 4.2.0); `rmcp =3.5.0`.

**Nachprüfung (T002, 2026-10-10, `graphify update .` → 20 665 Knoten, je Abfrage Budget 1000)**:

| Neues Artefakt                   | Abfrage                                                       | Kandidaten und warum keiner passt                                                                                                                                                                                       |
| -------------------------------- | ------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `extensions/agent_tools/link.rs` | „mcp client transport channel server request handler link“    | Android-Plugin (`Channel` für Insets/Netz) und dieser Vertrag; der bestehende MCP-Client `chat/tools/mcp.rs` startet Kindprozesse über stdio und bleibt dafür, `link.rs` nutzt dasselbe `rmcp`, aber das Kanalpaar (R1) |
| `chat/tools/extension_offer.rs`  | „match user message to tools offer score idf keyword“         | `offer.rs` (`search_score`, `words`, `word_matches`) und `appMatch.ts`: Bausteine, die R6 wiederverwendet; keine Bewertung der Nachricht gegen Werkzeuge vorhanden                                                      |
| `src/lib/extensions/mcpRelay.ts` | „relay port message postMessage frame invoke event extension“ | nur `vault_gate/invoke.rs` und Android-`Invoke`; der Ort bleibt `useExtensionFrame.ts` (Port-Handshake)                                                                                                                 |
| `openAppInBackground`            | „open app window minimized background without focus“          | Passwort-Import `open.rs`, `useWorkspaceBackground`; vorhanden ist nur `minimizeWindow` in `layoutState.ts`, das R9 nutzt                                                                                               |

## R1 — Transport: `rmcp` über ein Kanalpaar, das Frontend reicht JSON-RPC durch

**Decision**: holzi bleibt MCP-Client mit `rmcp`. Die Verbindung zu einer Erweiterung ist ein Paar aus
ungebundenen `futures::channel::mpsc`-Kanälen, das `rmcp` über `impl IntoTransport for (Sink, Stream)` (`transport/sink_stream.rs`,
ohne eigenes Cargo-Feature) annimmt. Rust schickt Nachrichten an das Frontend per Event
`extension-mcp-message {frame, message}`; das Frontend legt sie als Port-Nachricht
`{type: "haexspace:mcp", message}` auf den `MessagePort` des Rahmens. Antworten der Erweiterung nimmt
`useExtensionFrame` vom Port und gibt sie mit dem Command `extension_mcp_send {frame, message}` an Rust.
Rust bestimmt die Erweiterung aus der Rahmen-Sitzung (`FrameRegistry`), nie aus der Nachricht.

**Rationale**: ADR-0004 ließ offen, ob `rmcp` einen eigenen Transport braucht; `sink_stream` löst das ohne
neues Feature. Der Port ist der einzige Kanal in den Rahmen; ein eigener Nachrichtentyp trennt MCP sauber
von der Brücke (Richtung B), deren Anfragen nur von der Erweiterung zum Host laufen und keine
Host-Anfrage mit Id kennen (vault-sdk `src/client/events.ts`). Abbruch und Zeitlimit liefert `rmcp`
selbst (`Peer::send_cancellable_request`, `RequestHandle::cancel` sendet `notifications/cancelled`).

**Alternatives considered**: (a) Typisierter Aufruf über die bestehende Brücke nach dem Muster
`haextension:action:request` / `ai_action_respond` (vault-sdk `e70c52b`): weniger Code, aber ohne Liste,
Schema und Abbruch, und ADR-0004 hat MCP für Richtung A festgelegt; vom Operator am 2026-10-10 bestätigt.
(b) `transport-async-rw` über eine `duplex`-Pipe mit zeilenweisem JSON: unnötige Serialisierung, weil die
Nachrichten ohnehin als JSON-Werte durch das Frontend laufen.

## R2 — Keine Anfragen der Erweiterung über die Werkzeug-Verbindung (FR-015)

**Decision**: Der MCP-Client meldet keine Client-Fähigkeiten (kein `sampling`, keine `roots`, keine
`elicitation`) und beantwortet jede Anfrage der Erweiterung an holzi außer `ping` mit „method not found“. Die
MCP-Verbindung liegt in `extensions/agent_tools/`, nicht im Chat; der Chat sieht nur ein `Tool`.
Ein Vertragstest prüft beides: dass eine Server-Anfrage (`sampling/createMessage`, `roots/list`,
`elicitation/create`) mit Fehler endet und keine Modellschicht erreicht, und dass der Brücken-Vertrag
(`tests/extension_bridge_contract.rs`) unverändert gilt.

**Rationale**: Diese Spec öffnet nur die Richtung Agent → Erweiterung. Der Operator will, dass Erweiterungen
den Agenten künftig nutzen können, mit einem Modell, das der Nutzer je Art von Anfrage festlegt
(Clarification 2026-10-10); Spec 047 beginnt damit über Profile. Solange es keine solche Festlegung gibt,
hat eine Anfrage der Erweiterung auf dieser Verbindung kein Ziel und wird abgelehnt. Eine spätere Spec
kann MCP-Sampling oder den Weg aus 047 dafür nutzen.

**Konsequenz für ADR-0004**: Der Satz „The LLM is never reachable from an extension“ widerspricht schon
Spec 047. Lieferung E ändert ihn in: Eine Erweiterung erreicht ein Modell nur über einen Weg, den der Nutzer
festgelegt hat; nie über die Brücke oder die Werkzeug-Verbindung ohne eine solche Festlegung.

**Alternatives considered**: Sampling jetzt mit Freigabe anbieten — vom Operator auf später gelegt.

## R3 — Wirkungsart aus dem Manifest (Clarification 2026-10-10)

**Decision**: `effect` im Manifest (`read` | `change` | `destructive`) wird auf die bestehende
Abbildung `ActionEffect` → `RiskClass` gelegt (`Safe`, `Change`, `Risky`); die Freigabe-Matrix
(`permission.rs`) bleibt unverändert. MCP-Annotationen (`readOnlyHint` usw.) der Laufzeit werden
ignoriert. ADR-0004 wird geändert: statt „Tools are `Risky` by default“ gilt die erklärte und bestätigte
Wirkungsart.

**Rationale**: Vom Operator entschieden; eine Erweiterung kann nur tun, wofür sie Berechtigungen hat.

**Alternatives considered**: Alles `Risky` (ADR-Stand) — macht „Plan“ für Erweiterungen unbrauchbar.

## R4 — Bestätigung als Berechtigungsart `agentTool`

**Decision**: Jedes erklärte Werkzeug wird eine Berechtigung der neuen Art `agentTool` in
`extension_permissions`: `action` = Wirkungsart, `target` = Werkzeugname. Damit gelten die Regeln aus 017
ohne neuen Speicher:

- Installation und Update zeigen die Werkzeuge im bestehenden `InstallDialog`, ein Update nur neue
  Werkzeuge und solche mit geänderter Wirkungsart (`new_declarations`, `install.rs:197`).
- Status `granted` (angehakt): Die Wirkungsart gilt. Status `ask` (nicht angehakt): Jeder Aufruf braucht eine Zustimmung, in jedem Modus, auch in „Plan“
  (`Tool::always_ask()`, ausgewertet in `plan_calls`, nicht über die Risikoklasse, die in „Plan“ ablehnen
  würde). Status `denied`: Das Werkzeug wird nicht angeboten.
- Der Schalter „Für den Agenten verfügbar“ (FR-016) ist eine Zeile `agentTool / * / denied`; „verweigert
  vor erteilt“ (017 FR-017) schaltet damit alle Werkzeuge der Erweiterung ab.
- Zeilen gelten für die Vault und werden synchronisiert (`SYNCED_TABLES`); Entwicklungsversionen nutzen
  `dev_extension_permissions_no_sync`.

**Rationale**: Bestätigen, Merken, Widerrufen, Synchronisieren und Anzeigen gibt es schon; die Einstellungs-
Ansicht `PermissionsView` listet die Werkzeuge dann je Zeile.

**Abweichung von der Spec (vom Operator am 2026-10-10 angenommen, FR-002 geändert)**: FR-002 verlangte auch bei geänderter **Eingabe** eine erneute Bestätigung. Mit
`target` = Name erkennt `new_declarations` nur neue Namen und geänderte Wirkungsarten. Eine geänderte
Eingabe ändert nicht, was das Werkzeug tun darf (das bestimmen Wirkungsart und Berechtigungen). Vorschlag:
FR-002 auf „neue Werkzeuge und Werkzeuge mit geänderter Wirkungsart“ kürzen (Entscheidung des Operators).

**Alternatives considered**: Eigene Tabelle `extension_agent_tools` — dupliziert Bestätigung, Sync und
Anzeige. Schema-Hash im `target` — macht die Anzeige unlesbar.

## R5 — Manifest-Block `tools` und seine Prüfung

**Decision**: Das Manifest bekommt einen optionalen Block `tools` (Liste). Je Werkzeug: `name`
(`^[a-z][a-z0-9_]{0,31}$`, eindeutig), `title` `{de, en}`, `description` (Englisch, ≤ 1024 Zeichen),
`inputSchema` (JSON-Schema-Teilmenge), `effect`, `examples` `{de: [...], en: [...]}` (je 1–10 Sätze,
≤ 200 Zeichen). Höchstens 32 Werkzeuge je Erweiterung. Geprüft wird an zwei Stellen:

1. **Format** im Crate `haex-bundle` (`assert_valid_manifest`, vault-sdk): Struktur, Namen, Grenzen,
   `effect`-Werte → sonst `manifest_invalid`. Neue Testvektoren `bad-tools-*.xt`. Damit lehnen holzi, das
   Werkzeug `haex sign` und haex-vault 2.x (Legacy) dasselbe ab.
2. **Schema-Teilmenge** in holzi (`schema_in_subset`, `action_tool.rs`): ein Werkzeug außerhalb der Teilmenge
   wird nicht angeboten und in der Einstellungs-Ansicht als „nicht unterstützt“ gezeigt; die Erweiterung
   installiert trotzdem (wie `unsupported_categories` bei Berechtigungen).

Das Manifest ist kanonisches JSON (RFC 8785) ohne Gleitkommazahlen (`jcs.rs`); Schemas dürfen also nur
ganze Zahlen enthalten. Das steht im Vertrag.

**Rationale**: Format-Regeln gehören zum Bundle-Format, das alle Hosts teilen (ADR-0008); die Teilmenge ist
eine Eigenschaft von holzis Modellanbindung.

**Alternatives considered**: Prüfung nur in holzi — dann signiert `haex sign` Bundles, die holzi ablehnt.

## R6 — Passendes Angebot ohne Suche durch das Modell (FR-006, FR-007)

**Decision**: Vor dem ersten Schritt bewertet holzi die Nachricht gegen jedes anbietbare Werkzeug mit einer
gewichteten Wortüberlappung über Titel, Beschreibung und Beispielsätze: Wörter wie in `offer.rs::words`
(inkl. Präfix ab fünf Buchstaben, 046 R15), Gewicht je Wort nach seiner Seltenheit im Bestand aller
Werkzeugtexte (IDF), sodass „ich“, „habe“, „wie“ kaum zählen. Die Erweiterung wird zusätzlich über ihren
Namen und Anzeigenamen getroffen (Wortlaut wie `matchApp`, 046). Bis zu **3** Werkzeuge über einer festen
Schwelle kommen ins Angebot; sie bleiben für alle Schritte der Antwort (wie das Kernangebot).
Die Grenze: Kernangebot ≤ 11, plus passendes Angebot ≤ 3 im ersten Schritt (≤ 14); nach einer Suche
Aktionen ≤ 16 plus passendes Angebot ≤ 3 (≤ 19, unter den 20, die der Operator für die Websuche erwartet).
Schwelle und Gewichte sind Konstanten in `chat/tools/extension_offer.rs`; SC-001 misst sie.

**Rationale**: 046 (R15, R16, `eval-results.md`) zeigt, dass Qwen3-4B Werkzeuge, die es erst suchen
muss, nicht verlässlich nutzt. Die Beispielfrage „Wie viele ungelesene Mails habe ich?“ nennt die
Erweiterung nicht; ein Treffer nur über den Namen reicht also nicht. Das Verfahren ist deterministisch und
ohne Modell testbar.

**Alternatives considered**: (a) Nur bei genannter Erweiterung anbieten — verfehlt das Hauptbeispiel.
(b) Einbettungen (Vektoren) — braucht ein zusätzliches Modell auf jedem Gerät; erst prüfen, wenn das
Wortverfahren SC-001 verfehlt. (c) Alle Werkzeuge immer anbieten — sprengt die Grenze bei 30 Erweiterungen.

## R7 — Aktionssuche über Erweiterungswerkzeuge (FR-008)

**Decision**: `find_actions` durchsucht Aktionen und Erweiterungswerkzeuge gemeinsam; ein Treffer trägt
dieselbe Form (`tool`, `description`, `inputSchema`). `extend_offer` nimmt Erweiterungswerkzeuge als
gefundene Werkzeuge an. Gleiche Bewertung wie bisher (`search_score`), Beispielsätze zählen mit.

**Rationale**: FR-013 aus 032 („jede Aktion über die Suche erreichbar“) gilt sinngemäß weiter.

## R8 — Werkzeugname, Herkunft und Anzeige (FR-009, FR-011)

**Decision**: Der Name für das Modell ist `x_<slug>_<tool>`: `slug` = Manifest-`name` in Kleinbuchstaben,
nur `[a-z0-9]`, gekürzt; bei Kollision zweier Erweiterungen ein Suffix aus den ersten Zeichen der
Erweiterungs-Id; insgesamt ≤ 64 Zeichen (`is_valid_tool_name`). `Tool` bekommt eine Methode
`origin() -> Option<ToolOrigin>` (Erweiterungs-Id, Anzeigename, Werkzeugtitel de/en, Entwicklungsversion);
`source()` bleibt `"haextension"`. Das Freigabe-Event `tool-permission-request` und die Werkzeugzeile
im Verlauf bekommen das optionale Feld `toolOrigin`; neue Spalte `tool_origin TEXT` (JSON) in
`chat_messages`, damit der Verlauf auch nach dem Entfernen der Erweiterung lesbar bleibt.

**Rationale**: `source()` ist `&'static str` und persistiert wortgleich; eine Herkunft je Erweiterung braucht
ein eigenes Feld. Die Anzeige (032 FR-008) nennt Erweiterung und Werkzeug in Klartext.

**Alternatives considered**: Herkunft im Werkzeugnamen kodieren und im Frontend auflösen — scheitert, wenn
die Erweiterung entfernt ist.

## R9 — Rahmen für den Aufruf (FR-012, SC-004)

**Decision**: Ein Aufruf braucht einen gemounteten Rahmen der Erweiterung im aktiven Arbeitsbereich. Gibt
es einen (sichtbar oder minimiert), nutzt holzi seine Rahmen-Sitzung. Sonst öffnet der neue Reducer
`openAppInBackground` die App in einem neuen, minimierten Fenster, ohne `activeWindowId` oder Fokus zu
ändern; `singleInstance` bleibt gewahrt, weil eine vorhandene Instanz immer Vorrang hat. Rust wartet auf die
MCP-Bereitschaft (`initialize`) mit Zeitlimit (10 s). Im kompakten Layout (Telefon) öffnet holzi einen
Hintergrund-Tab ohne Wechsel; wie der kompakte Modus „minimiert“ abbildet, prüft eine Aufgabe in der
Umsetzung am Code von 043/045.

**Befund kompakter Modus (T023, Code von 043/045)**: Der kompakte Modus kennt keinen eigenen Tab-Zustand.
Er zeigt das aktive Fenster flächig (`windowDisplayRect` mit `compact`), und `Window.vue` blendet ein
minimiertes Fenster per `v-show` aus, mit oder ohne `compact`. Ein Hintergrund-Rahmen ist darum in beiden
Darstellungen dasselbe minimierte Fenster, ohne Wechsel. Gemountet wird nur, was im aktiven Arbeitsbereich
liegt (`windowsInActiveWorkspace`), und ein Tab erst, wenn er einmal aktiv war (`markTabMounted`). Deshalb
nimmt `openAppInBackground` nur einen Tab im aktiven Arbeitsbereich. Eine nur einmal erlaubte Erweiterung,
die in einem anderen Arbeitsbereich offen ist, bekommt keinen zweiten Rahmen; der Aufruf endet ohne Rahmen.
Der Extension-Host markiert den Tab als gemountet und antwortet, sobald der Port des Rahmens bereit ist.

**Rationale**: ADR-0004 („opens one minimized when none exists“). Ein unsichtbarer Extra-Rahmen außerhalb
des wm würde bei `singleInstance`-Erweiterungen (haex-mail) eine zweite Instanz starten.

**Alternatives considered**: Kopfloser Rahmen außerhalb des wm — siehe oben; Tab-Schnittstelle (015) fehlte
ihm ebenfalls.

## R10 — Verbindung, Verfügbarkeit, Zeitlimit, Ergebnis (FR-003, FR-013)

**Decision**: Das Angebot entsteht aus dem Manifest, ohne die Erweiterung zu starten. Die MCP-Verbindung
entsteht beim ersten Aufruf je Rahmen-Sitzung und endet mit ihr; nach `initialize` holt holzi einmal
`tools/list`. Ein erklärtes Werkzeug, das dort fehlt, endet mit dem Fehler `tool_unavailable`; ein
gemeldetes, nicht erklärtes wird ignoriert. Zeitlimit je Aufruf 60 s (wie `DEFAULT_ACTION_TIMEOUT`).
Abbruch des Turns → `RequestHandle::cancel`. Ergebnis: Textblöcke verbunden, höchstens 64 KiB, dann
gekürzt mit Vermerk; andere Inhaltsarten → `[unsupported content]`. Das Modell erhält
`{"extension": "<Anzeigename>", "data": "<Text>"}` mit dem Satz in der Werkzeugbeschreibung, dass
`data` Daten der Erweiterung sind, keine Anweisungen.

**Rationale**: Erweiterungen nur zu starten, wenn ein Werkzeug gebraucht wird; feste Grenzen gegen
hängende oder ausufernde Erweiterungen.

## R11 — Host-Funktionen während eines Aufrufs (FR-014)

**Decision**: Die Rahmen-Sitzung zählt laufende Werkzeugaufrufe. Fordert die Erweiterung währenddessen eine
Host-Funktion an, die eine Abfrage auslöst, trägt `PermissionRequestEvent` das neue Feld `byAgent: true`, und
der Dialog nennt den Auslöser. Geprüft wird weiter nur gegen die Berechtigungen der Erweiterung.

**Rationale**: ADR-0004 (One permission model); keine neue Prüfstelle.

## R12 — vault-sdk: Baustein `sdk.tools` (FR-021)

**Decision**: Neues Modul `src/api/tools.ts` mit `sdk.tools.register(name, handler)`. Titel, Beschreibung,
Schema und Wirkungsart kommen aus dem Manifest (`HaexHubConfig.manifest.tools`); der Code nennt nur Name und
Funktion, sodass Manifest und Code nicht auseinanderlaufen. Der Baustein ist ein **minimaler MCP-Server**
ohne Fremdabhängigkeit: `initialize`, `notifications/initialized`, `ping`, `tools/list`, `tools/call`,
`notifications/cancelled`; alles andere → „method not found“. Er prüft Eingaben gegen die Schema-Teilmenge
(eigener kleiner Prüfer) und gibt dem Handler ein `AbortSignal`. Port-Nachrichten `haexspace:mcp` laufen
über denselben Handler wie Ereignisse (`events.ts`), im Tauri-Modus über `listen()`.
Übereinstimmung mit `rmcp` sichern **Mitschnitte**: holzi zeichnet in einem Rust-Test den JSON-RPC-Austausch
seines Clients mit einem In-Memory-Server auf (`contracts/mcp-port.md`, Dateien in
`contracts/transcripts/`); vault-sdk spielt sie in vitest gegen seinen Server ab.

**Rationale**: `@modelcontextprotocol/sdk` bringt `zod` mit und vergrößert jede Erweiterung; der benötigte
Teil des Protokolls ist klein. Ohne Fremdabhängigkeit bleibt die Bundle-Größe stabil.

**Alternatives considered**: Offizielles TS-SDK mit eigenem `Transport` für den Port — korrekter
Protokollumfang ohne eigene Arbeit, aber größer; Wechsel jederzeit möglich, weil die öffentliche API
`sdk.tools.register` gleich bliebe.

## R13 — Test-Erweiterung und Signatur (FR-018)

**Decision**: Neue Test-Erweiterung `tests/fixtures/extension_e2e/agent-tools.xt` mit Quellen daneben,
gebaut und signiert wie `probe.xt` in `tests/extension_e2e_fixtures.rs` (`haex_bundle::build_archive`, aus
einem Seed abgeleiteter Testschlüssel; veraltete Dateien lassen den Test scheitern). Werkzeuge: `count_entries`
(read), `add_entry` (change), `slow_echo` (read, für Zeitlimit und Abbruch), und eines außerhalb der
Schema-Teilmenge. Ihr Code bindet den gebauten vault-sdk-Baustein ein (gepinnte Version).

**Rationale**: Bestehendes Verfahren; kein neuer Schlüssel.

## R14 — Modellprüfung mit Erweiterungswerkzeugen (FR-019, SC-001 bis SC-003)

**Decision**: Eval-Set Version 4 mit der Art `extension`. Die Werkzeuge kommen aus einer Fixture
`eval/extension_tools.json`: 30 erfundene Erweiterungen mit je 2–4 Werkzeugen und Beispielsätzen,
darunter eine Mail-Erweiterung mit `count_unread`. `run_eval` nimmt diese Werkzeuge in die Suche und ins
passende Angebot. Der Bericht führt je Satz `offered` (passendes Angebot traf, ohne Modell messbar, SC-001)
und `called` (SC-002) sowie Aufrufe von `run_command` (SC-003; `run_command` wird dafür im Eval angeboten).

## R15 — Phasen und Repos

**Decision**: Lieferungen: **A** vault-sdk (Format-Prüfung im Crate, Typen, `sdk.tools`, Mitschnitt-Tests,
Release); **B** holzi Manifest und Bestätigung (Pin-Erhöhung auf A, `agentTool`, Dialog, Einstellungen);
**C** holzi Transport und Aufruf (Relay, `ExtensionTool`, Rahmen im Hintergrund, Herkunft, Verlauf,
`byAgent`); **D** holzi Angebot und Messung (passendes Angebot, Suche, Test-Erweiterung, Eval v4); **E**
ADR-0004 ändern. B setzt das gemergte A voraus (Pin auf Commit-SHA, Prinzip IV). Spec 019 (Werkzeuge in
haex-mail usw.) setzt D voraus.

**Rationale**: 017 und 032 sind im täglichen Einsatz; 046 ist gemergt. Die Reihenfolge folgt den
Abhängigkeiten über die Repos.

## R16 — Agent-Kontextdatei

**Decision**: Entfällt wie in 046 (R12): `CLAUDE.md` wurde mit `c082fe89` („migrate holzi to spaex v4“)
entfernt; es gibt keine Datei mit `SPECKIT`-Markern.
