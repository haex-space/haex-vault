# Research: Modell bedient holzi über die Aktionen

**Spec**: [spec.md](./spec.md) | **Plan**: [plan.md](./plan.md) | **Stand**: 2026-09-30

Jede Entscheidung: **Entscheidung**, **Begründung**, **verworfene Alternativen**.
Belege sind Dateien im Repository (Stand `main` am 2026-09-30). Wo etwas aus
allgemeinem Wissen und nicht aus dem Repository stammt, steht es dabei.

## R1 — Die Aktionen kommen einzeln als Werkzeuge ins Rust-Register, die Definitionen schiebt das Frontend hinein

**Entscheidung**: Das Frontend ist die einzige Quelle der Aktionsdefinitionen
(`src/lib/actions/catalog.ts`). Nach dem Start der Tresor-Sitzung (und bei
Sprachwechsel) ruft es den neuen Command `set_agent_actions` mit allen für den
eingebauten Agenten aufrufbaren Definitionen auf. Rust legt je Aktion ein
`ActionTool` im bestehenden `ToolRegistry` an (Quelle `action`). Ausgeführt
wird eine Aktion über den Umlauf Rust → Ereignis `action-call-request` →
Frontend `runAction(id, input, { kind: 'builtinAgent' })` → Command
`respond_action_call` → Rust (R2).

**Begründung**: FR-001 verbietet eine zweite Werkzeugbeschreibung; der Katalog
liegt in TypeScript, die Schleife in Rust (`chat/tools/mod.rs`). Ein
`ActionTool` je Aktion gibt dem Modell flache Schemas mit dem echten
Eingabeschema der Aktion. Das ist für kleine Modelle entscheidend.

**Verworfen**:

- _Zwei Meta-Werkzeuge_ (`find_actions`, `call_action(id, input)`): verschachtelte
  Argumente, das Modell sieht das Schema der Zielaktion nicht. Kleine Modelle
  scheitern daran deutlich öfter.
- _Katalog in Rust duplizieren_: zwei Wahrheiten, Drift garantiert; verletzt FR-001.
- _Weg über MCP im selben Prozess_: Serialisierung und zusätzlicher Server ohne
  Nutzen (siehe ADR-0006, Annahmen der Spec); Spec 021 baut den MCP-Zugang auf
  derselben Definition.

## R2 — Der Umlauf Rust ↔ Frontend folgt dem bestehenden Freigabe-Muster

**Befund**: Ein „Rust ruft TS und wartet auf den Rückgabewert“ gibt es im Code
nicht. Vorbilder für „Ereignis senden, auf Command-Antwort warten“:
`pending_tool_approvals` + `respond_tool_permission`
(`chat/turn/tool_round.rs`, `chat/commands.rs`) und `sync/link/host_task.rs`
(mit `DECISION_TIMEOUT` und `select!` gegen Abbruch). Es gibt keine
Webview-Bereitschaftsprüfung; `emit` meldet keinen Fehler ohne Listener. Es
gibt genau ein Fenster (`main`), Chat und Window Manager teilen eine Webview.

**Entscheidung**: Neues Modul `chat/tools/action_bridge.rs` mit

- einem `EventEmitter` (`Arc<dyn Fn(&str, Value) + Send + Sync>`, wie in
  `adapters/cli_delegate/mod.rs`), der in `setup()` gesetzt wird (`ChatState`
  wird ohne `AppHandle` verwaltet, `lib.rs:133`),
- einer eigenen `HashMap<Uuid, oneshot::Sender<ActionReply>>`,
- einem Timeout von 60 s (Tab-Handler warten bis zu 5 s,
  `TAB_HANDLER_TIMEOUT_MS`; Aktionen wie Modell-Download sollen nicht
  sinnlos abbrechen) und `select!` gegen das Abbruch-Token des Zuges,
- einer Sperre (`tokio::sync::Mutex`), die das _Ausführen_ der Aktionen
  serialisiert, weil `execute_plans` die Aufrufe einer Runde mit `join_all`
  gleichzeitig startet und zwei gleichzeitige Fenster-Aktionen sich sonst
  ins Gehege kommen.

Der Listener liegt **global** im Frontend (Composable
`src/composables/useAgentActions.ts`, von der Workspace-Seite gestartet, sobald
der Tresor offen ist, denn ein Plugin liefe schon vor dem Entsperren und beide
Commands brauchen einen offenen Tresor), nicht in `ChatApp.vue`: dessen
Listener existieren nur, solange ein Chat-Tab gemountet ist, und Aktionen wie
`wm.app.open` sollen auch dann laufen, wenn der Chat-Tab im Hintergrund liegt.

**Fehlerpfade**: Webview nicht bereit oder keine Antwort → Timeout →
Werkzeugfehler `action_timeout`. Tresor schließt (`reset_for_close`) →
ausstehende Absender werden verworfen → `tool_call_cancelled` (wie heute bei
Freigaben). Doppelte oder späte Antwort → stilles Nichts (Muster
`cancelled_tool_approvals`).

**Verworfen**: Ergebnis über Dateien/DB austauschen (Latenz, Aufräumen);
`eval`/`emit_to` mit Rückgabewert (Tauri 2.11 hat keinen).

## R3 — Werkzeugnamen: Punkte ersetzen

**Befund**: Aktions-IDs enthalten Punkte (`wm.tab.back`). Anthropic erlaubt
Werkzeugnamen nur aus `[a-zA-Z0-9_-]` mit höchstens 64 Zeichen (allgemeines
Wissen, nicht im Repository belegt; vor der Umsetzung gegen die
API-Dokumentation prüfen, `check:agent-actions` erzwingt die Regel ohnehin).
Im Repository gibt es keine
Namensbereinigung; bestehende MCP-Namen (`mcp:server:name`) verstoßen schon
dagegen, das ist nicht Teil dieser Spec.

**Entscheidung**: `toolName = id.replaceAll('.', '_')`. IDs verwenden
camelCase und keine Unterstriche, die Abbildung ist damit eindeutig und
umkehrbar; ein Prüfskript (`check:agent-actions`) erzwingt Eindeutigkeit,
Zeichenmenge und Länge ≤ 64 für alle Katalog-IDs. Die Abbildung ist eine
reine TS-Funktion (`src/lib/actions/agentTools.ts`); Rust erhält den Namen
fertig mit der Definition.

**Verworfen**: Namenstabelle in Rust (zweite Stelle); `__` als Trenner
(länger, kein Mehrwert).

## R4 — Risiko: drei Wirkungsarten statt zwei

**Befund**: `RiskClass { Safe, Risky }` (`chat/tools/mod.rs:26`) und
`decide(mode, risk)` (`chat/tools/permission.rs`). Verwendet in `events.rs`,
`adapters/cli_delegate/approval_bridge.rs`, `cli.rs`, `mcp.rs`, in Tests und im
Frontend (`useChat.ts`, `PermissionPrompt.vue`).

**Entscheidung**: `RiskClass` bekommt die Stufe `Change` zwischen `Safe` und
`Risky`. Abbildung: `read` → `Safe`, `write` → `Change`, `destructive` →
`Risky`. Matrix:

| Modus   | Safe  | Change | Risky |
| ------- | ----- | ------ | ----- |
| Manuell | fragt | fragt  | fragt |
| Auto    | läuft | läuft  | fragt |
| Plan    | läuft | Abl.   | Abl.  |

`run_command` und MCP-Werkzeuge bleiben `Risky`, ihr Verhalten ändert sich
nicht (Spec, Annahmen). Die Wire-Zeichenkette heißt `"change"`; das Frontend
(`RiskClass` in `useChat.ts`, `PermissionPrompt.vue`) kennt sie dann. Für
CLI-Delegates bleibt die Freigabe-Brücke unverändert: die Delegates
klassifizieren selbst, der `match` in `approval_bridge.rs` wird um die neue
Stufe ergänzt (Ergebnis wie `Safe` → unverändert).

**Downloads**: Aktionen wie `settings.models.downloadFromHf` sind `write` und
laufen daher im Modus „Auto“ ohne Rückfrage. Das ist Absicht (Klärung
2026-09-30). Eine allgemeine Steuerung großer Downloads und des Dateisyncs auf
Mobilgeräten (pausieren, Datenvolumen schonen) ist eine eigene Spec; bis dahin
ist Desktop das einzige Ziel.

**Verworfen**: alle `write` fragen lassen (nimmt Auto seinen Sinn,
widerspricht der Klärung vom 2026-09-30); ein Feld `alwaysAsk` für teure
Aktionen (Downloads brauchen keine Extra-Frage, die Steuerung kommt mit der
Download-Spec).

## R5 — Chat-Aktionen, die sich selbst auslösen würden

**Befund**: `chat.message.send`, `chat.message.retry` und `chat.reply.cancel`
sind für Agenten aufrufbar (Spec 020, für externe Agenten gedacht). Läuft
ein eingebauter Zug, würde `send_message` durch `acquire_operation()`
abgelehnt; `chat.reply.cancel` würde den eigenen Zug abbrechen.

**Entscheidung**: Der Katalog bekommt das optionale Feld
`builtinAgentCallable` (Standard `true`); diese drei Aktionen setzen es auf
`false`. Sie bleiben für externe Agenten (021) unverändert aufrufbar. Der
Runner lehnt den Aufruf mit Aufrufer `builtinAgent` dann mit dem vorhandenen
Code `forbidden_for_agents` ab, und `set_agent_actions` enthält sie nicht.

**Verworfen**: Sie im Bridge-Code ausblenden (versteckte zweite Liste);
`agentCallable` auf `false` setzen (sperrt sie auch für 021).

## R6 — Werkzeug-Angebot pro Schritt: festes Kernangebot plus Suche (FR-011 bis FR-013)

**Befund**:

- Alle 51 für den eingebauten Agenten aufrufbaren Aktionen als Werkzeugdefinitionen
  (Name, Beschreibung, Eingabeschema) sind gemessen rund 13 200 Zeichen, also
  etwa 3 800 Token je Anfrage. Die lokalen Modelle haben laut Katalog ein
  Kontextfenster von 32 768 Token. Das wäre für Cloud-Modelle kaum spürbar,
  kostet aber bei einem Modell wie Qwen3-4B auf der CPU Zeit und wächst mit
  jeder neuen Quelle (haextensions bringen später viele Werkzeuge mit).
- Eine Liste, die sich je Nutzersatz ändert, verhindert, dass ein Anbieter oder
  die lokale Laufzeit den Anfangsteil des Prompts wiederverwenden kann. Eine
  feste Liste ist in jedem Zug gleich.
- Werkzeuge werden in `send_message` (`commands.rs` ~438–446) einmal je Zug in
  `ChatRequest.tools` gesetzt (`tool_specs()`, `commands.rs:189`);
  `append_round_to_request` ändert sie nicht, `TurnRunner.request` wird aber
  für jeden Schritt wiederverwendet und lässt sich zwischen den Schritten
  ändern.
- Das Modell kann nur Werkzeuge rufen, die in der Anfrage des jeweiligen
  Schrittes stehen: der Anthropic-Adapter schickt `tools` im Request, der
  lokale Adapter übergibt sie an mistralrs, das Aufrufe gegen genau diese
  Werkzeuge parst (`llm/local/stream.rs`). Der Lookup gegen das **Register**
  (`tool_round.rs:plan_calls`) allein reicht nicht.
- Wort-Treffer zwischen dem (deutschen) Nutzersatz und den (englischen)
  Beschreibungen sind unzuverlässig („mach es dunkler“ trifft „color scheme“
  nicht). Holzi soll deshalb nicht raten, was der Nutzer will.

**Entscheidung**: Ein einziges Verfahren für alle Modelle, lokal und Cloud.

1. **Festes Kernangebot**: höchstens 10 Werkzeuge, einschließlich des
   Suchwerkzeugs `find_actions`, in jedem Schritt und für jedes Modell
   identisch. Die Liste ist eine Konstante `CORE_AGENT_TOOLS` in
   `src/lib/actions/agentTools.ts` (neun Aktionen: `wm.state.get`,
   `wm.apps.list`, `wm.app.open`, `wm.tab.new`, `wm.tab.activate`,
   `wm.tab.close`, `settings.get`, `settings.appearance.setColorScheme`,
   `settings.models.list`); das Frontend markiert sie mit `core: true` in
   `AgentActionDef`. Die endgültige Zusammensetzung bestätigt der erste
   Messlauf (T046).
2. **Suche durch das Modell**: `find_actions({ query, cursor, limit })` durchsucht
   alle registrierten Aktionen wortbasiert (camelCase-zerlegte ID-Segmente,
   Beschreibung, deutsche und englische Titel; Gleichstand: stabile
   ID-Reihenfolge) und liefert höchstens 8 Treffer (zuvor 5) mit Name, Beschreibung und
   Eingabeschema. Ohne `query` wird derselbe stabile Katalog paginiert, bis
   `nextCursor` null ist; so ist jede registrierte Aktion abrufbar. Das Modell
   schreibt den Suchbegriff selbst, meist auf Englisch.
3. **Treffer werden angeboten**: nach einer Runde, in der `find_actions` lief,
   hängt der Zug die Treffer an `request.tools` an (eine Stelle,
   `extend_offer` in `chat/tools/offer.rs`, aufgerufen dort, wo die Runde an die
   Anfrage angehängt wird). Vor dem Anhängen werden Treffer aus dem Kernangebot
   und doppelte `toolName`s entfernt. Der nächste Schritt bietet höchstens fünf
   eindeutige Nicht-Kern-Treffer zusätzlich zum Kernangebot an; eine spätere
   Suche ersetzt die früheren Treffer. Damit gibt es höchstens 15
   Aktionswerkzeuge. `run_command` und MCP-Werkzeuge bleiben zusätzlich im
   Angebot und zählen nicht in dieses Aktionslimit. Alle Adapter übersetzen nur
   `request.tools` in ihr Format; es gibt keinen Sonderweg je Anbieter. Der
   Lookup gegen das Register bleibt als Sicherheitsnetz.

Keine Obergrenze je Modell: einheitlich, ohne Ausnahme (Klärung 2026-09-30).
Werkzeuge anderer Quellen (`run_command`, MCP) bleiben wie bisher im Angebot.

**Kosten, offen benannt**: Liegt die gewünschte Aktion nicht im Kernangebot,
braucht die Antwort einen zusätzlichen Schritt (Suche, dann Aufruf), bei
lokalen Modellen also einen weiteren Modelllauf. Und ein kleines Modell muss
von sich aus suchen, statt zu antworten, dass es das nicht kann. Der
Messlauf (US5) bewertet deshalb zwei Schritte und weist den Anteil der Sätze
aus, die das Ziel erreichen (`reachRate`). Zeigt er, dass das nicht reicht,
ist die Gegenprobe „alle Werkzeuge mitschicken“ für Cloud-Modelle ein Schalter
im Runner, keine neue Architektur.

**Verworfen**:

- _Alle Werkzeuge immer mitschicken_: einfachste Form, aber rund 4 000 Token
  je Anfrage und wachsend; für kleine lokale Modelle langsam und vermutlich
  ungenauer (nicht gemessen, darum Gegenprobe im Runner).
- _Auswahl nach Wörtern des Nutzersatzes_ (erste Fassung dieses Plans):
  unzuverlässig (Sprache, Synonyme), wechselt je Nutzersatz und verhindert
  Wiederverwendung des Prompt-Anfangs, dazu Sonderverhalten je Anfrage.
- _Embedding-Suche_ (neue Abhängigkeit, Start-Latenz); _zweiter Modellaufruf
  zur Auswahl_ (Kosten, Nichtdeterminismus).

## R7 — Fähigkeit „Werkzeugnutzung“

**Befund**: `ModelCapabilities` (`model_capabilities.rs`) ist als
`capabilities_json` in `models` gespeichert (Migration
`0018_models_add_capabilities`); neue Felder lesen sich ohne Migration als
`None` (`#[serde(default)]`). `models` ist eine CRDT-Tabelle: der Wert gilt
tresorweit und synchronisiert. `upsert_model` überschreibt
`capabilities_json` bei Konflikt vollständig. Anthropic liefert kein
Werkzeugfeld (`anthropic_capabilities.rs`).

**Entscheidung**: Neues Feld `tool_use: Option<ToolUse>` mit
`ToolUse { support: Supported | Unsupported, basis: Provider | Curated |
Template | SelfTest }`; `None` heißt „unbekannt“.

- **Anthropic**: `map_capabilities` setzt `Supported`/`Provider`. Die Wire-API
  kennt kein Feld; alle aktuellen Claude-Modelle unterstützen Werkzeuge
  (allgemeines Wissen). Ein Anbieter, der das Gegenteil meldet, hat Vorrang
  (FR-015): derzeit gibt es dafür kein Signal, der Code ist dafür vorbereitet.
- **Lokal**: `ModelCapabilities::local()` setzt `tool_use` aus dem
  Katalogeintrag (`CatalogEntry.tool_use`, neues optionales Feld, nach dem
  ersten Messlauf gefüllt), sonst `None`.
- **Schreiben** nach Probe oder Selbsttest über eine neue Funktion
  `storage::models::set_tool_use(tx, id, ToolUse)`, die das Feld in der
  bestehenden JSON ersetzt, statt die ganze Zeile zu überschreiben. Ein
  erneuter Download derselben Datei setzt das Feld zurück (gewollt: neue
  Datei, neuer Test, FR-018 / Szenario 8).
- **Keine Migration**, aber alle Struct-Literale anpassen (`..Default::default()`).

**Verworfen**: eigene Tabelle/Spalte (Migration und Trigger-Version
`HOLZI_TRIGGER_VERSION` 13 → 14 ohne Not); Wert nur im Speicher (verliert das
Ergebnis bei jedem Start, Selbsttest liefe immer wieder).

## R8 — Vorlagenprüfung bei lokalen Modellen (FR-018a)

**Befund**: mistralrs 0.8.1 hat **kein** `supports_tools`. Die einzige Prüfung
steckt in `apply_chat_template_to`: bei einer Vorlage aus benannten Teilen ohne
`tool_use` gibt es den Fehler „does not handle tool usage“; eine Vorlage als
Zeichenkette bekommt `tools` übergeben und **ignoriert sie stillschweigend**,
wenn sie nicht darauf verweist. `Model::tokenize(text, tools, …)` und
`detokenize` sind öffentlich.

**Entscheidung**: `llm/local/probe.rs` (hinter `#[cfg(feature = "llm-cpu")]`)
rendert eine feste Testnachricht einmal ohne und einmal mit einem Dummy-Werkzeug
über `tokenize` + `detokenize`. Gleicher Text oder der „does not handle tool
usage“-Fehler → `Unsupported`/`Template`. Sonst bleibt der Wert offen (die
Vorlage _kann_ Werkzeuge, ob das Modell sie gut nutzt, klärt R9). Der Aufruf
läuft einmal beim ersten Laden eines Modells mit `tool_use == None`
(`chat/model_loading.rs`).

**Verworfen**: Vorlagentext selbst nach `tools` durchsuchen (kein
Zugriff auf die Vorlage über die öffentliche API, `ChatTemplate` ist
pipeline-intern).

## R9 — Selbsttest und Messlauf teilen dieselbe Bewertung (FR-018b, FR-019 bis FR-022)

**Befund**: Kein Test ruft heute ein echtes Modell mit Werkzeugen
(`tests/local_inference.rs` setzt `tools: Vec::new()`, ist `#[ignore]` und
braucht `HOLZI_TEST_GGUF`). `LocalModel::load` und `stream_chat` laufen ohne
Tresor. mistralrs bietet `set_deterministic_sampler`,
`set_sampler_temperature`, `set_tool_choice` und `set_constraint` (Regex,
Lark, JSON-Schema, Llguidance).

**Entscheidung**: Neues Modul `chat/eval/` mit

- dem versionierten Satz `eval_set.json` (eingebettet per `include_str!`),
  Format in [contracts/eval-format.md](./contracts/eval-format.md),
- `tools.json`, einem Schnappschuss der Werkzeugdefinitionen, den
  `scripts/export-eval-tools.ts` aus dem Katalog erzeugt; `check:agent-actions`
  prüft, dass er aktuell ist (der Test misst so das echte, nicht ein
  veraltetes Angebot),
- `scoring.rs` (reine Bewertung: richtige Aktion, gültige und richtige
  Eingaben, keine unnötigen Aufrufe, Quoten gesamt, je Sprache, je Art,
  Recall der Auswahl),
- `runner.rs`: nimmt ein `Arc<dyn ProviderAdapter>`, ruft je Satz bis zu zwei
  Schritte mit der Auswahl aus R6 (deterministischer Sampler, wo der Adapter es
  zulässt) ab und bewertet den vollständigen Werkzeugaufruf-Batch jedes
  bewerteten Schritts. Die erwartete und beobachtete Aufrufmenge muss exakt
  übereinstimmen; fehlende, doppelte oder unerwartete Aufrufe dürfen nicht als
  korrekt gelten. Ersatzhandler gibt es nicht: der Lauf führt nie eine Aktion
  aus, er bewertet nur die Aufrufe, darum kann er keine Daten verändern
  (FR-021).

Einstiege:

- **Selbsttest in der App**: `chat/tools/selftest.rs` startet als
  Hintergrundaufgabe (`tokio::spawn`) mit einer festen Teilmenge (Markierung
  `selfTest: true` im Satz, ≈ 5 Sätze), bricht ab, wenn die Sitzung das Modell
  wechselt, schreibt das Ergebnis über `set_tool_use`, sendet
  `model-tool-use-updated`.
- **Vollständiger Messlauf**: `tests/model_tool_eval.rs`, `#[ignore]`, liest
  `HOLZI_TEST_GGUF` (lokal) oder `HOLZI_EVAL_PROVIDER` + Schlüssel aus der
  Umgebung (Cloud), schreibt den Bericht nach `target/eval/<modell>.json`.
  Kein Tresor nötig (FR-022). Schlüssel stehen nie im Repository.

**Schwellen**: Selbsttest „unterstützt“ ab der Mindestquote aus dem ersten
Messlauf (Annahme der Spec); bis dahin ein Platzhalter `SELF_TEST_PASS =
0.6`, markiert mit `ponytail:`-Kommentar und Verweis auf Aufgabe T046.

**Verworfen**: Den kompletten Zug (`run_turn`) durchlaufen lassen (braucht
Tresor und Datenbank, langsamer, misst die Schleife statt das Modell);
Constrained Decoding im Eval erzwingen (würde den Messwert verfälschen, weil
der Lauf in der App ohne es arbeitet — siehe R10).

## R10 — Constrained Decoding bleibt außerhalb dieser Spec

**Befund**: mistralrs kann Werkzeugaufrufe per Grammatik erzwingen
(`set_constraint`, `set_tool_choice`), holzi nutzt das bisher nicht. Die
Aufgabenliste aus Spec 003 (T014) versprach `set_tool_choice`, im Code ist es
nicht zu finden.

**Entscheidung**: Nicht Teil von 032. Der erste Messlauf (T046) zeigt, ob die
Quoten ohne Erzwingung reichen. Falls nicht, wird Constrained Decoding eine
eigene kleine Spec; der Runner aus R9 bekommt dann einen Schalter, um beide
Varianten zu messen. Vermerkt in den Annahmen des Plans.

**Begründung**: Erst messen, dann eine Maßnahme einführen, die in jedem
Adapter anders aussieht (Anthropic braucht sie nicht).

## R11 — Hinweise „einmalig je Unterhaltung“ (US4, FR-016/017/023)

**Befund**: Es gibt keinen Mechanismus für Hinweiszeilen. Vorhanden:
vorübergehende Ereignisse (`chat-retry`, `chat-message-error`), das
`StatusBanners.vue`-Banner, und die Rolle `System` in `chat_messages`, die
niemand anlegt und die `history_to_messages` verwirft. Backend-Texte dürfen
nicht lokalisiert sein (`CONTEXT.md`).

**Entscheidung**: `send_message` bestimmt je Zug den Zustand
`ToolAvailability = Offered | OfferedUnverified | Unsupported | Delegate`
und sendet das Ereignis `chat-tool-availability { threadId, state }`. Das
Frontend zeigt für `OfferedUnverified`, `Unsupported` und `Delegate` ein
schließbares Hinweisbanner in `StatusBanners.vue` (Schlüssel
`chat.toolNotice.*` in `de.json` und `en.json`), **einmal je Unterhaltung und
Zustand pro App-Sitzung** (Menge im Speicher, `ponytail:`-Kommentar: nach einem
App-Neustart erscheint der Hinweis erneut; Upgrade-Pfad ist eine
Systemzeile in `chat_messages`).

**Begründung**: Kein Schema, kein Sync von Hinweistexten, kein Eingriff in den
Verlauf, den das Modell nie sieht. Die Abweichung von „einmalig je
Unterhaltung“ (Neustart zeigt erneut) ist klein und benannt.

**Verworfen**: `System`-Zeile im Verlauf (Sync über CRDT, Marker-Text plus
Lokalisierung im Frontend, mehr Fläche für Fehler).

## R12 — Freigabe-Dialog in Klartext (FR-008)

**Befund**: `PermissionPrompt.vue` zeigt `toolName`, `toolInput` und
`riskClass`, Titel „Approve: {name}“ (`chat.permission.requestTitle`).

**Entscheidung**: Für Werkzeuge der Quelle `action` schlägt das Frontend die
Aktion über die umgekehrte Namensabbildung (R3) nach und zeigt den
lokalisierten Titel (`actions.<id>`), das Ziel (Tab-/Fenster-/Arbeitsbereichs-
Titel aus dem Store) und die Eingaben als Liste „Feld: Wert“. Kein Backend-
Text. `riskClass` zeigt „Änderung“ für `change`.

## R13 — Ergebnisse und Fehler ohne Geheimnisse und Interna (FR-006, FR-010, SC-004)

**Entscheidung**:

- `respond_action_call` gibt an das Modell nur `ok`/`result` oder `code`,
  `field` und `message` weiter; das Feld `error` der Runner-Antwort (roher
  Fehler) wird nie gesendet. Bei Code `failed` ersetzt ein fester Text die
  Meldung des Handlers (er kann Pfade oder Interna enthalten).
- `check:agent-actions` prüft für jede für den eingebauten Agenten aufrufbare
  Aktion, dass Ergebnisschema und Eingabeschema keine Felder mit Namen wie
  `secret|private|password|passphrase|token|apiKey|credential` enthalten
  (Groß-/Kleinschreibung egal; Ausnahmeliste begründet und kurz). Die 9
  Leitplanken-Aktionen sind bereits gesperrt.
- `settings.devices.identity` gibt laut Beschreibung nur öffentliche
  Schlüssel zurück; ein Test prüft die Antwortform.
- `chat.messages.list` bleibt für den eingebauten Agenten aufrufbar, liefert
  aber nur eine sichere Projektion je Nachricht (`id`, `role`, `createdAt` und
  `status`). `content`, Vorschauen und daraus abgeleitete Felder werden aus
  dem Agent-Ergebnis entfernt. Der SC-004-Test legt eine Nachricht mit einem
  bekannten geheimen Wert an und bestätigt, dass weder der Wert noch ein
  `content`-Feld im Ergebnis auftaucht.

**Grenze**: Das Schema-Prüfen fängt nur Felder, die im Ergebnisschema
stehen. Werte, die ein Handler über ein allgemeines Feld durchreicht, fängt es
nicht — das ist dieselbe Grenze wie bei Spec 020 FR-032; der Test bleibt
Stichprobe über den Katalog.

## R14 — CLI-Delegates (FR-023)

**Befund**: `send_message` kennt `session.provider_kind`
(`commands.rs` ~510); Delegates ignorieren `req.tools` und emittieren nie
`ToolCalls`.

**Entscheidung**: Bei `ProviderKind::CliDelegate` bleibt `ChatRequest.tools`
leer (wie heute, jetzt aber ausdrücklich), es läuft weder Auswahl noch
Fähigkeitsprüfung, und der Zustand `Delegate` geht an das Frontend (R11).
Kein neuer Code in `adapters/cli_delegate/`.

## R15 — ADR

**Entscheidung**: ADR-0006 „Aktionen als Werkzeuge des eingebauten Agenten:
im Prozess, nicht über MCP“. Die Nummer 0005 bleibt für Spec 021
reserviert (Spec 020 verweist darauf). Inhalt: eine Definition, zwei
Eingänge; Risiko-Abbildung auf drei Stufen; `builtinAgentCallable`; was 021
übernimmt.

## R16 — Offene Stellen, bewusst nicht gelöst

- **Constrained Decoding** (R10): erst nach Messung.
- **Katalog der Empfehlungsliste** (nur Qwen3 0.6B/1.7B/4B): Aufnahme eines
  größeren Modells entscheidet sich nach dem ersten Messlauf, nicht hier.
- **Vordergrund-App als Lage** (R6).
- **Hinweis nach Neustart** (R11).
- **Aktionen aus haextensions** (017–019): erscheinen später als weitere
  Quelle im Register; `source` ist ein freier Text in `chat_messages.tool_source`
  (kein CHECK), das TS-Typfeld `toolSource` wird auf `'mcp' | 'cli' | 'action'`
  erweitert.

## R17 — Erste Messung (T046)

Qwen3-4B (Q4_K_M, CPU, Release-Build, deterministisch, kein Systemprompt, wie in der App):
16 von 30 Sätzen (53 %), je 8 von 15 auf Deutsch und Englisch, Smalltalk 6 von 6,
Lesen 4 von 11, Ändern 6 von 13. `reachRate` 0,54, `extraSteps` 0, keine unnötigen
Aufrufe. Zwei Läufe ergaben denselben Bericht (SC-008).

**Befund**: Das Modell ruft `find_actions` nie auf. Für die 11 Sätze mit einem
Werkzeug außerhalb des Kernangebots antwortet es mit Text oder ruft ein falsches
Kernwerkzeug auf (meist `settings_get`); auch zwei Kernsätze (offene Tabs, Apps)
enden als Text. Der Suchweg trägt bei diesem Modell nicht.

**Selbsttest**: 4 von 5 Sätzen (0,8). Ein Modell, das nur redet, bestünde die zwei
Smalltalk-Sätze (0,4). `SELF_TEST_PASS = 0,6` trennt beides; es gibt nur diesen einen
Messpunkt (Qwen3 0,6B und 1,7B waren nicht installiert, ein Claude-Modell nicht
messbar ohne Schlüssel). Der Katalogeintrag von Qwen3-4B trägt `tool_use: supported`.

**Offen**: Ob eine kurze Anweisung an das Modell hilft, misst R18.

## R18 — Anweisung zu den Werkzeugen (Systemprompt)

Jeder Zug mit Werkzeugen trägt einen kurzen englischen Text (`chat/tools/prompt.rs`):
holzi mit den Werkzeugen bedienen, ein Werkzeug aufrufen statt zu beschreiben, mit
`find_actions` suchen, wenn keines passt, in der Sprache des Nutzers antworten. Derselbe
Text für jedes Modell; ein Zug ohne Werkzeuge bekommt ihn nicht, ein Zug ohne Suche eine
Fassung ohne den Suchsatz (Review von #198). Der Messlauf und der Selbsttest senden ihn
ebenfalls.

Qwen3-4B, gleiche Bedingungen wie R17, mit Anweisung: 20 von 30 (67 %), je 10 von 15 auf
Deutsch und Englisch, Lesen 7 von 11, Ändern 7 von 13, Smalltalk 6 von 6, `reachRate`
0,67, `extraSteps` 3, keine unnötigen Aufrufe, Selbsttest-Teilmenge 5 von 5. Der Lauf
dauerte 768 Sekunden (R17: 464).

**Befund**: Alle 13 Sätze mit Werkzeug im Kernangebot bestehen jetzt (vorher scheiterten
dort fünf Lesesätze mit Text). Von den 11 Sätzen außerhalb des Kernangebots besteht einer:
das Modell sucht dreimal, ruft sonst meist ein falsches Kernwerkzeug auf oder antwortet in
Text, zweimal mit erfundenem Inhalt. Der Suchweg bleibt für ein 4B-Modell unzuverlässig.

**Selbsttest**: `SELF_TEST_PASS = 0,6` bleibt: Qwen3-4B 1,0, ein Modell, das nur redet, 0,4.
Weiterhin nur ein gemessenes Modell.

**Offen**: die kleineren Qwen3-Modelle, ein Claude-Modell, und wie sich die Suche für kleine
Modelle zuverlässiger machen lässt (zum Beispiel Treffer sofort im selben Schritt anbieten).
