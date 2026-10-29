# Research: Navigation im Tab

**Spec**: [spec.md](./spec.md) | **Plan**: [plan.md](./plan.md) | **Datum**: 2026-09-25

Grundlage ist der Implementierungsstand von Spec 015 auf dem Branch
`015-workspace-shell` (noch nicht auf `main`). Pfade unten beziehen sich auf diesen
Stand; die Umsetzung von 020 beginnt erst, wenn 015 gemerged ist (R15).

## R1 — Eigener Router je Tab statt vue-router

**Decision**: Ein kleiner eigener Tab-Router: reine Reducer für die Historie und ein
reiner Pfad-Matcher unter `src/lib/wm/`, dazu zwei Vue-Bausteine
(`WmRouterView`, `WmLink`) und das Composable `useTabRouter()`.

**Rationale**:

- Nuxt stellt einen globalen vue-router bereit. `useRoute()`, `<RouterLink>` und
  `<RouterView>` lösen über Injection-Keys genau diesen Router auf; eine zweite
  Instanz je Tab müsste diese Keys in jedem Tab-Teilbaum überschreiben und
  kollidiert mit Nuxts Auto-Imports und Middleware.
- Eine vue-router-Instanz lebt an einer Komponente. Die Historie soll aber zum
  Tab gehören, beim Verschieben des Tabs mitwandern (FR-010) und reine Daten sein
  (FR-001) — das leistet eine im Store gehaltene Datenstruktur, nicht ein
  Router-Objekt mit Closures.
- Der Bedarf ist klein: Pfadmuster mit `:param`, verschachtelte Einträge,
  Push/Replace/Back/Forward/Go. Geschätzt 200–300 Zeilen einschließlich Tests,
  prüfbar ohne Nuxt im Node-Harness (Vorbild `check-wm-state.ts`).

**Alternatives considered**:

- _vue-router mit `createMemoryHistory` je Tab_: siehe oben (Injection-Konflikte,
  Historie nicht serialisierbar, an Komponentenlebensdauer gebunden).
- _haex-vault-2.x-Modell (`navigation.ts`, `useDrillDownNavigation`)_: Einträge sind
  `undo`/`redo`-Closures, die beim Unmount verwaisen; alle Tabs teilen
  `window.history` samt `navIndex`-Trick; verschachtelte Navigatoren mischen sich
  in einem Stack; keine Adressen. Genau diese Punkte schließt die Spec aus.

## R2 — Historie lebt im Shell-Store, nicht in der Komponente

**Decision**: Die Historie jedes Tabs liegt im Pinia-Store `stores/windowManager.ts` in
der bereits vorhandenen, nie persistierten Laufzeitstruktur `TabRuntime`
(Feld `history`), geschlüsselt nach Tab-Id.

**Rationale**: Der Store überlebt Remounts, die Tab-Id bleibt beim Verschieben
zwischen Fenstern und Arbeitsbereichen gleich, und `TabRuntime` ist in Spec 015
genau für flüchtige Tab-Daten angelegt. Schließen eines Tabs räumt die
Laufzeitdaten über das vorhandene `syncTabRuntime()` ab (FR-011).

**Alternatives considered**: Historie in der `WmTabPanel`-Instanz (geht beim
künftigen Tab-Drag zwischen Fenstern verloren); Historie im persistierten
`WmTab` (widerspricht der Betreiberentscheidung, FR-011).

## R3 — Ort als Pfad plus Query

**Decision**: `TabLocation = { path: string; query: Record<string, string> }`.
Der Pfad ist app-relativ und beginnt mit `/` (Start-Ort `/`). Der Titel eines
Eintrags wird separat im Historien-Eintrag gespeichert (R9), nicht im Ort.

**Rationale**: Ein Pfad ist das bekannteste Adressformat, trägt Hierarchie für
verschachtelte Ansichten (Präfix = Eltern-Ansicht, R4) und lässt sich als String
in Deep-Links, Tests und später in nativen Fenstern übergeben. Query deckt
Filter/Suche/Sortierung ab (FR-005). Gleichheit (FR-006) = gleicher normierter
Pfad und gleiche Query unabhängig von der Schlüsselreihenfolge.

**Alternatives considered**: `{ view: string; params: object }` (weniger lesbar in
Links, keine natürliche Hierarchie); beliebiger `state`-Blob (verleitet zu
nicht serialisierbaren Inhalten).

## R4 — Routen je App, verschachtelt wie RouterView

**Decision**: Jede App meldet eine Routentabelle an:
`{ path, component, titleKey?, children? }`. Die reine Matcher-Funktion liefert
für einen Pfad die Kette der passenden Einträge von außen nach innen plus die
Parameter. `WmRouterView` rendert je Verschachtelungstiefe den Eintrag dieser
Tiefe (Tiefe über provide/inject, wie `RouterView`). Eine App ohne eigene
Routentabelle hat implizit genau die Route `/` auf ihre Wurzelkomponente — damit
bleibt `components/wm/appComponents.ts` rückwärtskompatibel und wird zur
Routentabelle erweitert (`appRoutes.ts`).

**Rationale**: Die Seitenleiste der künftigen Einstellungs-App ist dann eine
Eltern-Route (`/` mit Seitenleiste), die Kategorien sind Kinder
(`/models`, `/federation`), ihre Unteransichten Enkel (`/models/hf/:repo`). Alle
Wechsel schreiben in dieselbe Tab-Historie (FR-003); die aktive Kategorie ergibt
sich aus dem gematchten Kind — die in haex-vault 2.x (Legacy) getrennten Navigatoren entfallen.

**Alternatives considered**: flache Routen mit manueller Layout-Wahl in jeder App
(Wiederholung, keine gemeinsame Seitenleisten-Logik).

## R5 — Push, Replace und Grenzen

**Decision**:

- `push(to)`: legt einen Eintrag hinter der Position an, verwirft Vor-Einträge
  (FR-004); bei identischem Ort No-op (FR-006).
- `replace(to)`: ersetzt den aktuellen Eintrag (FR-005).
- `setQuery(patch)`: Komfortfunktion, ersetzt standardmäßig (Filter, Suche).
- `back()`, `forward()`, `go(delta)`: verschieben die Position; außerhalb der
  Grenzen wirkungslos (FR-007).
- Höchstens 50 Einträge, der älteste entfällt, die Position wird nachgeführt
  (FR-009). Die Verlaufsliste zeigt höchstens 15 Einträge je Richtung (FR-016).
- `removeEntry(index)` für verwaiste Ziele (R10, FR-014).

**Rationale**: Deckt sich mit Browser-Semantik; alle Operationen sind reine
Funktionen `(history, …) → history` und damit ohne Vue testbar.

## R6 — Eingaben und ihr Ziel

**Decision**:

| Eingabe                         | Ziel                                         | Umsetzung                                                                                          |
| ------------------------------- | -------------------------------------------- | -------------------------------------------------------------------------------------------------- |
| Alt+←/→ (alle), Cmd+[/] (macOS) | aktiver Tab des fokussierten Fensters        | ein globaler `keydown`-Listener auf der Shell-Host-Seite, Auflösung über die Aktions-Registry (R8) |
| Maustasten Zurück/Vor           | aktiver Tab des Fensters unter dem Zeiger    | `mouseup`/`auxclick` mit `button` 3/4 am Fenster-Wurzelelement, Default unterdrückt; Spike unten   |
| System-Zurück (Android-Geste)   | aktiver Tab des obersten sichtbaren Fensters | nativer Zurück-Hook → Aktion `wm.system.back` (R7)                                                 |

Für Alt+Pfeil gilt: Liegt der Fokus in einem editierbaren Element und läuft holzi
unter macOS, wird die Taste nicht abgefangen (Wortsprung hat Vorrang, FR-017).
Unter Linux und Windows hat Alt+Pfeil in Textfeldern keine Textbedeutung und
navigiert.

**Rationale**: Jede Eingabe hat genau eine Zielregel (FR-018, SC-002). Das Ziel
wird bei der Eingabe aufgelöst, nicht aus einem globalen „aktiven Tab“ (Fehler
in haex-vault 2.x).

**Offener Punkt (Spike in tasks)**: Ob die Maus-Seitentasten unter WebKitGTK
(Linux), WKWebView (macOS) und WebView2 (Windows) als DOM-Ereignis ankommen oder
vom Webview selbst als History-Navigation verarbeitet werden. Kommen sie im DOM
an, gilt die Tabelle. Verarbeitet der Webview sie selbst, wird das abgeschaltet,
wo die Plattform es erlaubt (Webview-Einstellung beim Fensteraufbau in
Rust); wo nicht, bewirken sie wegen der flachen Historie (R7) nichts in holzi —
FR-018 ist dann für diese Plattform als Einschränkung im PR zu vermerken.

## R7 — Webview-Historie: keine Sperr-Einträge, System-Zurück nativ

**Decision**:

- holzi hält die Browser-Historie des obersten Dokuments bewusst **flach**: Der
  Einstieg in die Workspace-Seite erfolgt per `router.replace`, es gibt keine
  Sperr- oder Hilfseinträge.
- `pages/workspace/[instance].vue` registriert `onBeforeRouteLeave` nur als
  **Abwehr**: Jede vom Router ausgelöste Navigation weg von der Workspace-Seite
  wird abgebrochen und **ignoriert** — sie wird nie als Zurück eines Tabs
  gedeutet. (Sperren und Schließen beenden den Prozess, Spec 013; es gibt keinen
  legitimen Router-Weg weg von der Seite.)
- **System-Zurück** kommt nicht über die Webview-Historie, sondern über den
  nativen Zurück-Hook der Plattform (Android: Zurück-Taste/-Geste über Tauri)
  und ruft die Aktion `wm.system.back` auf. Ziel: aktiver Tab des obersten
  sichtbaren Fensters; ohne Zurück-Eintrag in der Kompaktdarstellung die
  Fensterübersicht; offenes Shell-Overlay schließen (FR-019).

**Rationale**: Ein eingebettetes Dokument (iframe) teilt sich mit holzi die
gemeinsame Browser-Historie des Webviews. Mit Sperr-Einträgen könnte ein
`history.back()` aus einem iframe holzis Eintrag treffen und würde dann als
Zurück eines Tabs gedeutet — ein Verstoß gegen FR-034. Mit flacher Historie und
„abbrechen, nicht deuten“ hat Webview-Historie keinerlei Wirkung auf holzi
(FR-035).

**Alternatives considered**: Sperr-Eintrag im Router mit Deutung als
System-Zurück (erste Planfassung) — von iframes auslösbar; roher
`popstate`-Listener mit eigenem `pushState` (haex-vault 2.x) — kollidiert zusätzlich
mit vue-router.

**Spike (tasks)**: Die genaue Tauri-2-Schnittstelle für die Android-Zurück-Taste
(Plugin-Event bzw. `onBackButtonPress`) ist zu verifizieren. holzi hat heute kein
Android-Target; der Hook wird hinter einer Plattformprüfung angelegt und mit
`ponytail:` markiert, die Logik von `wm.system.back` ist unabhängig davon in
`check:wm-navigation` geprüft.

## R8 — Aktions-Registry, agentenfähig (Name „Aktion“, nicht „Command“)

**Decision**: Eine Registry `ActionDefinition` (reine Daten unter
`src/lib/actions/`) mit: `id`, `titleKey`, `description` (englisch, für
maschinelle Aufrufer; Tool-Beschreibungen sind im Projekt englisch),
`input` und `result` als JSON Schema, `target` (`none` | `tab` | `window` |
`workspace`, dazu ob ausdrücklich angebbar), `scope`, `effect`
(`read` | `write` | `destructive`), `agentCallable`, `defaultKeys`,
`yieldToTextInput`. Ein Runner `runAction(id, input, caller)` validiert die
Eingabe, prüft Aufrufer und Leitplanken, löst das Ziel auf und ruft den Handler.

**Rationale**:

- In holzi bezeichnet „Command“ Tauri-Commands. Graphify-Abfrage (Graph vom
  2026-09-21) zu `router`, `navigation`, `keybinding`, `shortcut`, `hotkey`,
  `keydown`: keine Frontend-Treffer; `command` trifft nur Tauri-Commands. Spec 015
  spricht bereits von „Shell-Aktionen“.
- JSON Schema, weil der vorhandene `ToolRegistry` des eingebauten Agenten
  (`src-tauri/src/chat/tools/`, `input_schema()` als `serde_json::Value`) und
  MCP (`tools/list`) genau dieses Format erwarten. Spec 021 kann die Aktionen dann
  ohne Umbau als Tools anbieten.
- `effect` lässt sich später direkt auf `RiskClass` abbilden (`read` → `Safe`,
  sonst `Risky`), `scope` ist die Einheit für Berechtigungen je Agent.

**Alternatives considered**: `commands.ts` (Namenskollision); eine
Schema-Bibliothek wie zod (neue Abhängigkeit; wir brauchen nur eine kleine
Teilmenge — `object`, `properties`, `required`, `string`, `number`, `integer`,
`boolean`, `array`, `enum` —, die ein eigener Validator von ~100 Zeilen prüft);
Tastenkürzel direkt in Komponenten (widerspricht FR-024).

## R9 — Titel

**Decision**: Angezeigter Tab-Titel = dynamischer Titel der App
(`useWmTab().setTitle`, Spec 015) → sonst `titleKey` der tiefsten gematchten
Route mit `titleKey` → sonst App-Name. Beim Verlassen eines Eintrags speichert
der Store den zu diesem Zeitpunkt angezeigten Titel im Eintrag (`title`), sodass
die Verlaufsliste Titel „zum Zeitpunkt des Besuchs“ zeigt (FR-021). Ein
dynamischer Titel wird beim Navigieren zurückgesetzt; die App setzt ihn für die
neue Ansicht bei Bedarf neu.

## R10 — Chat

**Decision**: Der Chat bekommt die Routen `/` (neue Unterhaltung, Start-Ort) und
`/thread/:id`. Ein neues Composable `useChatNavigation.ts` verbindet Router und
bestehende Logik, weil `ChatApp.vue` mit 499 Zeilen an der 500-Zeilen-Grenze
steht:

- Verlaufseintrag öffnen → `push('/thread/:id')`; ein Watcher auf den Ort ruft
  das vorhandene `selectThread(id)` auf — damit gelten die bestehenden Regeln für
  laufende Antworten und ausstehende Freigaben unverändert (FR-027).
- Neue Unterhaltung → `push('/')`.
- Die erste Nachricht einer neuen Unterhaltung legt den Thread an
  (`useComposer.ts`, `activeThreadId.value = result.threadId`) → `replace`
  auf `/thread/:id`, damit Zurück nicht auf eine leere neue Unterhaltung führt.
- Aktiver Thread gelöscht (Spec 006, FR-015) → `replace('/')`.
- Ort zeigt auf einen nicht existierenden Thread → Eintrag entfernen und in
  dieselbe Richtung weiter (Zurück bzw. Vor); gibt es keinen, `replace('/')`.
- Spec 004 bleibt erfüllt: jeder Chat-Einstieg (neuer Tab, Neustart) beginnt am
  Start-Ort `/`.

## R11 — Öffnen an einem Ort

**Decision**: `wm.openApp(appId, location?)` und `addTab(windowId, appId,
location?)`; `useWmTab()` bekommt `openApp(appId, location?)` für Sprünge
zwischen Apps. Neue Instanz: Historie `[location ?? '/']`. Vorhandene
Einzelinstanz: aktivieren wie bisher (`activateExistingSingleton`), dann `push`
(No-op bei gleichem Ort). Die Legacy-Weiterleitung `?open=<appId>` bleibt und
nimmt optional `&at=<pfad>` an. Unbekannter Pfad → `WmRouterView` ersetzt
per `replace('/')` und zeigt einen Hinweis (Toast aus dem haex-ui-Layer)
(FR-014).

## R12 — Aufbewahrung, Sperren, Neustart

**Decision**: Keine Persistenz; `TabDto`/Migrationen bleiben unverändert, es gibt
**keine** neuen Tauri-Commands. `hydrate` legt für jeden wiederhergestellten Tab
eine Historie `[{ path: '/' }]` an. Sperren/Schließen beendet den Prozess
(Spec 013), damit ist alles verworfen.

## R13 — Prüfungen

**Decision**: Neues Skript `scripts/check-wm-navigation.ts` (Node mit
Type-Stripping, Vorbild `check-wm-state.ts`) für Historien-Reducer, Matcher,
Chord-Normierung und Aktionsauflösung sowie Store-Integration (openApp mit Ort,
Singleton-Push, Verschieben erhält Historie, hydrate ohne Historie). Neues
`pnpm check:wm-navigation` und ein CI-Schritt.

**Rationale**: `scripts/check-wm-state.ts` hat bereits 829 Zeilen und liegt
über der 500-Zeilen-Grenze (in Spec 015 als Complexity Tracking mit
„aufteilen, sobald erreicht“ vermerkt). Neue Prüfungen kommen deshalb in eine
eigene Datei; die Aufteilung des bestehenden Skripts ist ein Befund für 015,
nicht Teil dieser Spec.

## R14 — Oberfläche

**Decision**: Nur der vorhandene Stack: haex-ui-Layer (`ShadcnDropdownMenu`,
`ShadcnTooltip`, Toast), `@lucide/vue` (`arrow-left`, `arrow-right`),
Tailwind. Keine neue Abhängigkeit. Beide Schaltflächen bleiben in der
Kompaktdarstellung sichtbar (FR-015) und erhalten dort die Touch-Größe aus
015 (T037/`326b0ed`).

## R15 — Abhängigkeit von 015 und Phasen-Disziplin

**Decision**: Spec und Plan liegen auf `020-tab-navigation` (Basis `main`). Die
Umsetzung beginnt erst nach dem Merge von 015; dann wird der Branch auf `main`
rebased. `plans/README.md` bekommt einen Roadmap-Eintrag (Aufgabe in tasks).

**Rationale**: Constitution, Phasen-Disziplin: Folgefunktionen dürfen vorab
spezifiziert, aber erst umgesetzt werden, wenn ihre Voraussetzungen im Einsatz
sind.

## R16 — ADR

**Decision**: Kein ADR für diese Spec. Die Entscheidungen berühren kein Prinzip
der Constitution; sie sind in dieser Datei begründet. Die Zugriffsrichtung
„externer Agent → holzi“ (MCP-Server, Anmeldung, Berechtigungen je Agent)
ergänzt ADR-0004 um eine dritte Richtung und bekommt mit Spec 021 ein eigenes
ADR-0005.

## R17 — Isolation eingebetteter Dokumente (haextensions)

**Decision**: holzis Navigationszustand lebt ausschließlich im Shell-Store; die
Browser-Historie wird nie gelesen (R7). Dazu:

- **Tastatur**: Der globale `keydown`-Listener sitzt im obersten Dokument; über
  einem fokussierten iframe erreicht ihn die Taste nicht. Browser-eigene
  Kürzel des Webviews werden abgeschaltet, wo die Plattform das anbietet
  (WebView2: Browser-Accelerator-Keys aus), damit Alt+Pfeil im iframe keine
  Webview-Navigation auslöst. Weiterleitung aus dem iframe an holzi übernimmt
  die SDK-Brücke (Spec 017).
- **Maus**: Seitentasten über einem iframe gehen an das iframe-Dokument. Ob der
  Webview daraus eigene History-Navigation macht, klärt der Spike aus
  quickstart §3; wenn ja, wird sie abgeschaltet, wo möglich. Andernfalls trifft
  sie höchstens das iframe (FR-035) — holzi bleibt unberührt, weil es keine
  Webview-Einträge deutet.
- **Programmatisch**: `pushState`/`history.back()` eines iframes wirken nur auf
  dessen Dokument; holzi reagiert nicht darauf.
- **Künftige Extension-Tabs** (017): Der History-Polyfill des SDK
  (`haex-space/vault-sdk` @ `502593e84b8d289b0986a2777754d6bd8f52da5e`,
  `src/polyfills/history.ts`) wird zur Navigations-Brücke: Er meldet Orte über
  den MessagePort an die Shell, statt die gemeinsame Historie zu füllen, und
  folgt Anweisungen der Shell per `popstate`. So nutzen Extension-Tabs dieselbe
  Tab-Historie; die Brücke ist kooperativ, die Garantie von FR-034 gilt auch
  ohne sie.

**Prüfung**: `check:wm-navigation` kann kein iframe laden; der Schutz wird
manuell nach quickstart M21 geprüft (DevTools: iframe in einen Tab einfügen,
`pushState` und `history.back()` darin auslösen).

**Umsetzungsstand (T038, 2026-09-25)**: Tauri 2.11.5 (laut `Cargo.lock`) reicht
WebView2s `browser_accelerator_keys` nicht durch — nur `wry` 0.55 kennt die
Einstellung, und holzis Fenster entstehen aus `tauri.conf.json`. Eine gezielte
Abschaltung der Maus-Navigation von WebKitGTK/WKWebView bietet Tauri ebenfalls
nicht. Es gibt daher keinen Rust-Anteil. Wirkung trotzdem spezifikationsgemäß:
Die History des obersten Dokuments bleibt flach, und die Abwehr in
`pages/workspace/[instance].vue` bricht jede Router-Navigation weg von der
Seite ab, ohne sie zu deuten. Eine vom Webview selbst ausgelöste
Rück-Navigation trifft deshalb höchstens ein eingebettetes Dokument (FR-035).
Einschränkung für den PR: Unter Windows kann Alt+← bei Fokus in einem iframe
dessen eigene History bewegen.

## R18 — Aufrufer, Bereiche, Leitplanken

**Decision**:

- `ActionCaller = { kind: 'user' } | { kind: 'builtinAgent' } | { kind:
'externalAgent'; agentId: string }`.
- Bereiche (erste Liste): `wm.layout`, `wm.navigation`, `wm.read`,
  `chat.read`, `chat.write`, `settings.read`, `settings.device`,
  `settings.models`, `guardrails`.
- `guardrails` umfasst Autonomie-Modus, Deny-Regeln, Anbieter verbinden und
  Zugangsdaten sowie (künftig) Agenten-Berechtigungen; jede Aktion dort hat
  `agentCallable: false`. Der Runner lehnt jeden Aufruf mit `caller.kind ≠
'user'` ab (`forbidden_for_agents`), bevor die Eingabe den Handler erreicht.
- Für Agenten-Aufrufer ist ein ausdrückliches Ziel Pflicht (FR-030); fehlt es:
  `target_required`.
- In dieser Spec rufen nur Oberfläche und Tastatur (`user`) auf; die
  Agenten-Aufrufer existieren im Typ und in den Tests, einen Zugang gibt es erst
  mit Spec 021.

## R19 — Globale und tab-gebundene Handler

**Decision**: Zwei Arten von Handlern:

- **Global**, beim App-Start registriert (Nuxt-Plugin): Shell-Aktionen und
  Aktionen, die nur das Backend brauchen (Einstellungen über
  `usePreferences`/`useDevice`/Provider-Composables). Sie funktionieren, ohne
  dass eine App offen ist — ein Agent kann eine Einstellung ändern, ohne das
  Fenster zu öffnen.
- **Tab-gebunden**, von der gemounteten App-Instanz über `useWmTab()`
  registriert (Chat: Nachricht senden, Antwort abbrechen, Freigabe, Verlauf
  öffnen). Ist die App nicht offen, öffnet der Runner sie (Einzelinstanz:
  aktiviert), wartet auf die Registrierung (Zeitlimit 5 s → `app_unavailable`)
  und ruft dann auf.

**Rationale**: Chat-Zustand (laufender Turn, Freigaben) lebt in der
Chat-Instanz; Einstellungen leben im Backend. So bleibt jede Logik an einer
Stelle, und Oberfläche und Agenten teilen sie.

## R20 — Vollständigkeit des Katalogs prüfen

**Decision**: `check:wm-navigation` prüft den Katalog strukturell (eindeutige
Ids, gültige Schemas, jeder Bereich existiert, jede `guardrails`-Aktion ist
`agentCallable: false`, der Runner lehnt Agenten dort ab). `check:templates`
sperrt direkte Aufrufe schreibender Schnittstellen in `.vue`-Dateien unter
`src/`: Shell-Store-Mutationen, Schreibzugriffe auf Präferenzen, Gerät, Modelle,
HF und Provider sowie die schreibenden Aktionen des Models-Stores. Ausnahmen
tragen `action-exempt: <Grund>` in der Zeile oder bis zu drei Zeilen davor.
Ergänzend eine manuelle Prüfliste in quickstart (M23).

**Rationale**: Eine Regel „@click nur über useAction“ erkennt nicht, ob ein
Handler Zustand ändert. Die Sperrliste trifft genau die Aufrufe, die am Katalog
vorbeigehen würden, und lässt reine Ansichts-Umschalter unberührt.
