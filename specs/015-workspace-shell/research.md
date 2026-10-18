# Research: Workspace-Shell

**Feature**: 015-workspace-shell | **Date**: 2026-09-21

Alle Punkte aus dem Technical Context sind hier aufgelöst; es bleiben keine
`NEEDS CLARIFICATION`. Jeder Abschnitt: Entscheidung, Begründung, verworfene
Alternativen. Belege stammen aus dem Stand von `main` (c52c1c3) und dem
gepinnten haex-vault-2.x-Commit `8dce379d94e18fcd42c3b73686a06f984ca3f574`.

## Graphify-Konsultation (Constitution)

Abfragen gegen `graphify-out/graph.json` des primären Checkouts (Fork-Point-
Snapshot, wie vorgeschrieben unverändert genutzt):

| Kandidat                                                             | Ergebnis                                                                        | Konsequenz                                                                                      |
| -------------------------------------------------------------------- | ------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------- |
| `storage::preferences` (`PrefScope`, `insert_or_update`, `get`)      | Per-device Key/Value nach ADR-0001, CRDT-getrackt                               | **Erweitert**: der aktive Arbeitsbereich wird als Preference gespeichert (R1), kein neuer Store |
| `usePreferences`, `useDevice`, `active_database`                     | Bestehende Wrapper/Helfer                                                       | **Wiederverwendet** unverändert                                                                 |
| `known_devices::get_vault_device_uuid`                               | Löst `installation_uuid → vault_device_uuid` auf                                | **Wiederverwendet** (R4)                                                                        |
| Fenstermanager, Drag-/Resize-Composable, Viewport-/Breakpoint-Helfer | Keine Treffer (nur `useAutoResizeTextarea`, das nichts mit Fenstern zu tun hat) | Neu zu schreiben, aber auf `@vueuse/core` (bereits Abhängigkeit) gestützt (R7)                  |
| Workspace-Stub `pages/workspace/[instance].vue`                      | Enthält die Modell-Preload-Statuslogik                                          | **Wird verschoben**, nicht dupliziert (R11)                                                     |

Die Abfragen `query "workspace window layout"` und `query "viewport breakpoint compact"`
liefern erwartungsgemäß nur Token-Treffer (siehe bekannte Schwäche von
`graphify query`); die belastbaren Aussagen stammen aus `explain` und direktem
Lesen.

## R1. Persistenzform

**Decision**: Drei neue CRDT-getrackte, gerätebezogene Tabellen —
`workspaces`, `shell_windows` und `shell_window_tabs` — plus ein
Preference-Schlüssel `shell.active_workspace_id` (Device-Scope) für den zuletzt
aktiven Arbeitsbereich.

**Rationale**: Das Backend soll die Invarianten besitzen (mindestens ein
Arbeitsbereich, dichte Reihenfolge, jedes Fenster in genau einem Arbeitsbereich,
jedes Fenster mit mindestens einem Tab, Löschen mit Fenstern und Tabs). Das ist
mit Zeilen testbar (`cargo test`), mit einem JSON-Blob nur im Frontend.
Feingranulare Zeilen halten den CRDT-Schreibaufwand klein (Verschieben eines
Fensters ändert eine Zeile, nicht das ganze Layout).
Der aktive Arbeitsbereich ist ein einzelner Skalar und passt exakt in die schon
vorhandene `preferences`-Tabelle (`chat.autonomy_mode` ist das Vorbild).

**Alternatives considered**:

- Ganzes Layout als ein JSON-Wert in `preferences` — verworfen: Invarianten nur im
  Frontend, jeder Drag schreibt und synchronisiert den ganzen Blob.
- `_no_sync`-Tabellen — verworfen: ADR-0001 stellt klar, dass `_no_sync` das
  Mitreisen mit der kopierten `.db`-Datei nicht verhindert (Kopie-Szenario aus
  Spec US5-3).
- Vault-weite (nicht gerätebezogene) Arbeitsbereiche wie in einer Datei-Kopie
  „mitgenommen“ — verworfen: widerspricht FR-024 und ADR-0001.

## R2. Fremdschlüssel Tab → Fenster → Arbeitsbereich: erst prüfen, dann festlegen

**Decision**: Ziel sind echte Datenbank-Fremdschlüssel mit `ON DELETE CASCADE`, damit
die Datenbank selbst garantiert, dass ein gelöschter Arbeitsbereich keine Fenster
und ein gelöschtes Fenster keine Tabs zurücklässt:

- `shell_windows.workspace_id → workspaces(workspace_id)` und
  `shell_window_tabs.window_id → shell_windows(window_id)`, jeweils **einspaltig**.
  Die Elterntabellen bekommen dafür zusätzlich `UNIQUE (workspace_id)` bzw.
  `UNIQUE (window_id)` (die IDs sind UUIDs; der zusammengesetzte Primärschlüssel
  aus ADR-0001 bleibt unverändert).
- Alle drei Tabellen behalten außerdem den Fremdschlüssel aus ADR-0001 auf
  `known_devices(vault_device_uuid) ON DELETE CASCADE`.

**Warum erst prüfen**: haex-crdt ist die Bibliothek, über die holzi seine SQLite-Vault
synchronisiert. Sie schreibt jede `CREATE TABLE` beim Migrieren um (fügt die
versteckten Sync-Spalten hinzu, installiert Trigger) und schaltet beim Einspielen
von Sync-Daten die Fremdschlüsselprüfung vorübergehend aus (laut ADR-0001).
Belegt ist bisher nur der Fall „Fremdschlüssel auf die Primärschlüsselspalte einer
Tabelle“ (`REFERENCES known_devices(vault_device_uuid)` in allen holzi-Migrationen,
ein Beispiel in den haex-crdt-Tests des gepinnten Rev `b8c9c6c…`). Ob das
Umschreiben einen Fremdschlüssel auf eine `UNIQUE`-Spalte unverändert lässt, habe
ich nicht geprüft.

**Spike (erster Rust-Task)**: Ein `cargo test`, der `0019` gegen einen frischen und
gegen einen mit `0018` bereits provisionierten Vault anwendet, Zeilen einfügt und ein
Elternobjekt löscht. Besteht er, gilt das Ziel. Besteht er nicht, gilt der
**Fallback**: keine Fremdschlüssel zwischen den drei Tabellen, `workspace_id` und
`window_id` sind indexierte Textspalten, die Speicherschicht löscht Kinder und
Eltern in einer Transaktion (I7). Dann steht an der Migration ein
`ponytail:`-Kommentar — Grenze: die Integrität hängt am Code statt am Schema;
Upgrade-Pfad: Fremdschlüssel nachrüsten, sobald haex-crdt dafür einen Test hat.

**Alternatives considered**: Zusammengesetzter Fremdschlüssel
`(vault_device_uuid, workspace_id)` — nicht bevorzugt: über das belegte
Ein-Spalten-Muster hinaus, mit mehr Umschreib-Risiko, und die Geräte-Trennung ist
bereits durch den Fremdschlüssel auf `known_devices` und die gefilterten Abfragen
gesichert.

## R3. Migration und Trigger-Version

**Decision**: Eine neue Migration `0019_shell_layout` legt die drei Tabellen und
Indizes an. `HOLZI_TRIGGER_VERSION` wird von 10 auf 11 angehoben; der
Doc-Kommentar bekommt die Zeile
`- 11: 0019_shell_layout introduced three new CRDT-tracked tables`.

**Rationale**: Vorbild ist `0011_preferences` (Version 4): Neue getrackte Tabellen
brauchen ihre Trigger auf jedem bestehenden Vault, nicht nur auf frischen.
`--> statement-breakpoint` trennt die Statements (bestehende Konvention).

**Risiko**: Die Worktrees 013 und 014 stehen ebenfalls auf 0018 / Version 10. Wer
zuerst nach `main` merged, behält 0019/11; der zweite nummeriert neu. Das ist
ein reiner Merge-Handgriff, kein Designproblem; der Plan hält es hier fest, damit
der PR-Review es nicht überrascht.

**Bestätigt (2026-09-24, T003)**: Spec 013 ist bereits vollständig auf `main`
gemerged (zuletzt PR #133) und dort in 0018/Version 10 aufgegangen — keine
offene Konkurrenz mehr um `0019`/11. Für Spec 014 existiert noch kein Branch
(Implementierung startet erst nach 015); `0019_shell_layout` und
`HOLZI_TRIGGER_VERSION = 11` sind damit zum Zeitpunkt der Implementierung von
015 frei. Sollte 014 vor 015 mergen, nummeriert 015 beim Rebase neu (reiner
Merge-Handgriff, siehe oben).

## R4. Aktuelles Gerät wird im Backend aufgelöst

**Decision**: Die Shell-Commands nehmen **keine** Geräte-UUID vom Frontend. Sie
lösen das aktuelle Gerät selbst auf (`read_or_mint_installation_uuid` +
`known_devices::get_vault_device_uuid`, wie `current_device_info`).

**Rationale**: Ein Frontend, das eine fremde Geräte-UUID übergeben könnte, könnte
das Layout eines anderen Geräts schreiben — die Trennung aus FR-024 soll nicht
von der Sorgfalt des Aufrufers abhängen. `preferences_commands` übergibt die UUID
zwar, das ist dort aber Teil des Vertrags (vault-weite Scopes).

**Follow-up**: Der Helfer `current_device_uuid` bleibt vorerst privat in
`shell_commands.rs`. Zwei bestehende Stellen lösen dasselbe auf; eine Extraktion
nach `state_utils` ist naheliegend, aber kein Teil dieses Features
(Constitution: keine vorschnelle Abstraktion).

## R5. Befehlsfläche und Fehlerabbildung

**Decision**: Sechs Commands (Vertrag in `contracts/tauri-commands.md`), gleiches
Muster wie `preferences_commands`: `active_database` → `spawn_blocking` →
`db.with_connection`, Mehrfachschreibvorgänge in `conn.unchecked_transaction()`.
Verstöße gegen Invarianten → `HolziError::InvalidInput { reason }`, fehlender Vault →
`NoActiveInstance`. Eingaben werden validiert:

- Arbeitsbereiche haben **keinen Namen**: keine Spalte, kein Umbenennen. Die
  Oberfläche zeigt „Arbeitsbereich N“ bzw. „Workspace N“ mit N = Position + 1; das
  Backend liefert nie lokalisierte Texte (CONTEXT.md, i18n) und muss deshalb auch
  keinen Standardnamen vergeben. Das Löschen eines mittleren Arbeitsbereichs
  verdichtet die Positionen, die Nummern bleiben lückenlos (Vorbild haex-vault 2.x (Legacy): dort
  `Workspace ${n}` als gespeicherter Name mit Umbenennen; hier bewusst einfacher).
- `appId`: 1–128 Zeichen, keine Steuerzeichen; sonst **opak** (kein CHECK auf
  bekannte Werte), damit künftige App-Arten (`extension.*`, Specs 017/018) keine
  Migration brauchen.
- Geometrie: `width`/`height` 1–100 000, `x`/`y` innerhalb ±100 000 (Schutz vor
  Unsinnswerten, keine UI-Regel — die Frontend-Korrektur passiert vor dem Speichern).
- Tabs: ein Fenster hat 1–100 Tabs und einen `activeTabId`, der einer seiner
  Tabs ist; `tabId` ist eine UUID, `appId` folgt derselben Regel wie oben.
- IDs sind UUIDs; Arbeitsbereichs-IDs vergibt das Backend, Fenster- und Tab-IDs
  das Frontend (beide müssen vor der ersten Persistierung existieren).

**Alternatives considered**: Ein einziger `shell_save_layout`-Command, der das ganze
Layout ersetzt — verworfen: großer Schreibblock je Drag, Race bei zwei schnellen
Speichervorgängen und keine Stelle für die Löschregel „letzter Arbeitsbereich“.

## R6. Frontend-Zustand: reine Reducer plus dünner Store

**Decision**: Die Fensterlogik liegt als **reine TypeScript-Module** in
`src/lib/shell/` (`types.ts`, `apps.ts`, `geometry.ts`, `layoutState.ts`,
`tabs.ts`): Öffnen, Fokussieren, Minimieren, Maximieren, Schließen, Verschieben
zwischen Arbeitsbereichen, Tab öffnen/wechseln/schließen (Nachbar-Aktivierung),
Singleton-Auflösung über alle Fenster, Kaskade, Geometriekorrektur. Ein Pinia-Store
(`src/stores/shell.ts`) hält den Zustand, ruft die Reducer auf und delegiert die
Persistenz an `useShellLayout` (Tauri-`invoke`).

**Rationale**: Das bestehende Frontend hat kein Vitest/Browser-Harness. Belastbare
Prüfung läuft über Node-Skripte mit Type-Stripping (`scripts/check-chat-state.ts`,
`tsconfig.scripts.json`: `erasableSyntaxOnly`, `verbatimModuleSyntax`,
`import type`). Reine Module ohne Nuxt-Auto-Imports laufen dort direkt und sind
die kleinste Prüfung, die bricht, wenn die Logik bricht (Constitution/ponytail).
Die flache Composable-Konvention aus Spec 012 (R7) wird eingehalten:
Abhängigkeiten als Parameter, keine Store-Untermodule.

**Konsequenz**: Module in `src/lib/shell/` importieren einander mit expliziter
`.ts`-Endung und **ohne** `~/`-Alias, damit `node scripts/…` sie auflösen kann.

**Alternatives considered**: Ein großer Store wie in haex-vault 2.x (`windowManager/
{index,lifecycle,tabs,state}.ts`, 1 000+ Zeilen mit Nuxt-Auto-Imports mitten in
den Reduktionen, z. B. `useWorkspaceStore()` in `activateSingletonTab`) —
verworfen: nicht ohne Nuxt-Runtime testbar. Die Tab-Logik wird trotzdem portiert
(Singleton-Suche über alle Fenster, Nachbar-Aktivierung, Letzter-Tab-schließt-
Fenster), aber als reine Funktionen und ohne die aus dem aktiven Tab abgeleiteten
Legacy-Felder (`IWindow.type/sourceId/title/…`).

## R7. Fenster-Interaktion und Kompakt-Erkennung

**Decision**: Ein Composable `useWindowPointerGesture` implementiert Verschieben
und 8-Wege-Größenänderung mit Pointer Events und Pointer Capture
(`touch-action: none` an Titelleiste und Griffen), Frames über
`requestAnimationFrame` gebündelt. Die Rechnung (neue Geometrie aus Start,
Delta, Grenzen, Mindestgröße) steckt in `geometry.ts` und ist rein. Die
Kompaktdarstellung nutzt `useWindowSize` aus `@vueuse/core` (bereits Abhängigkeit,
`@vueuse/nuxt` auto-importiert) gegen die Konstante `COMPACT_MAX_WIDTH = 767`.

**Rationale**: Verschieben und Größenändern brauchen dieselbe Zeiger-Schleife;
`useDraggable` deckt nur das Verschieben ab und würde einen zweiten Mechanismus
für die Griffe nötig machen. Keine neue Abhängigkeit (Fenster-Bibliotheken wie
`vue-draggable-resizable` verworfen: Nutzen gering, eigenes Stil-/Kompaktmodell
wäre trotzdem nötig).

**Regeln** (aus FR-009/FR-026): Beim Ziehen bleibt mindestens ein
64 × 32-Pixel-Ausschnitt der Titelleiste im sichtbaren Bereich; beim Wiederherstellen
wird ein Fenster **vollständig** in den Bereich geklemmt, wenn es hineinpasst,
sonst auf die Bereichsgröße verkleinert (nie unter die Mindestgröße der App).

**Maximieren** (FR-039): Das Fenster behält seine **Normalgeometrie** in
`x/y/width/height` und trägt zusätzlich `isMaximized`. Ein maximiertes Fenster
wird beim Rendern auf den Bereich gezogen; Wiederherstellen setzt nur das Flag
zurück — die Geometrie muss nirgends gemerkt oder zurückgerechnet werden. Aus
demselben Grund schreibt die Kompaktdarstellung nie Geometrie (FR-028). Ein
Doppelklick auf die Titelleiste (nicht auf Tab, „+“ oder Schaltflächen) schaltet
um; in der Kompaktdarstellung ist Maximieren ausgeblendet.

## R8. Tab- und Fensterinhalt bleibt erhalten

**Decision**: Fenster und Tabs werden bei Minimieren, Maximieren, Fokus-, Tab- und
Arbeitsbereichswechsel nie unmounted, sondern per `v-show` verborgen (Fenster
außen, je Tab ein `role="tabpanel"` innen). Ein Tab-Inhalt wird **lazy** gemountet,
sobald er in dieser Sitzung zum ersten Mal sichtbar wird (ein aus dem Layout
wiederhergestellter Tab in einem nicht aktiven Arbeitsbereich oder ein
inaktiver Tab kostet beim Start nichts) und bleibt danach gemountet.

**Rationale**: FR-013 verlangt, dass laufende Antworten und Eingabe-Entwürfe
weder abbrechen noch verloren gehen. Der Chat hält diesen Zustand in seiner
Komponente (Streaming-Listener, Composer-Text). `v-show` ist die einfachste
Umsetzung.

**Follow-up (ponytail)**: Grenze — alle besuchten Tabs bleiben im Speicher
(größenordnungsmäßig Dutzende). Upgrade-Pfad: `KeepAlive` mit begrenztem Cache
und Zustandsübergabe für Apps, die es brauchen.

## R9. Apps, Identität und App-Registry

**Decision**: Eine App-Definition ist reine Daten (`src/lib/shell/apps.ts`):
`id`, `titleKey`, `icon`, `defaultSize`, `minSize`, `multiInstance`
(`defaultSize`/`minSize` gelten für ein **neues Fenster** mit dieser App als
erstem Tab; ein Fenster mit mehreren Tabs behält seine Geometrie). Die
Zuordnung `id → Komponente` liegt getrennt in `src/components/shell/appComponents.ts`
(`defineAsyncComponent`), damit der reine Teil ohne Vue testbar bleibt. IDs sind
namespaced Strings: `system.chat`, `system.settings`, `system.federation`; künftige
Arten (`extension.<…>`) sind für das Speichermodell zulässig, für die Registry in
dieser Spec unbekannt — ein gespeicherter Tab mit unbekannter `appId` wird beim
Wiederherstellen verworfen, ebenso ein Fenster, das dadurch keinen Tab mehr hat
(FR-025). Fenster-ID und Tab-ID sind je eine UUID v4; Persistenz und Verwaltung
laufen über sie, nie über die `appId` (Voraussetzung für Spec 016).

**Rationale**: Die geforderte Öffnung für den Typ „extension“ (ADR-0004) kostet nur
eine offene Textspalte und eine Registry, die um neue Arten erweitert werden kann.
Der **Tab** ist die Instanz einer App; das **Fenster** ist nur der Rahmen mit
Geometrie. Ein Chat-Tab, der später vervielfacht wird (Spec 016), braucht damit
weder neue Tabellen noch eine neue Fensterart.

**Titel**: Fenster- und Tab-Titel kommen aus der App-Definition (`titleKey`, per
i18n). Eine App darf ihren Tab-Titel dynamisch überschreiben (`setTitle` im
App-Vertrag, R12); persistiert wird der Titel nicht.

## R10. Seiten werden zu Apps, der Chat wird zuvor zerlegt

**Decision**:

- **Schritt 0, vor allem anderen: `pages/chat/[instance].vue` (1 320 Zeilen:
  742 `<script setup>`, 577 `<template>`) wird verhaltensgleich zerlegt.** Das ist die
  produktive Chat-Oberfläche, keine Test- oder Migrationsdatei. Sie hat einen
  Ausnahmevermerk im Dateikopf samt Split-Plan (`useComposer`), aber 1 320 Zeilen
  sind zu viel; die Ausnahme endet mit diesem Feature.
  - **Logik in Composables** (`src/composables/`), denn das Harness
    `check-chat-state.ts` lädt sie generisch mit und prüft sie damit weiter:
    `useComposer.ts` (Senden, Abbrechen, neue Unterhaltung, Eingabe- und
    Busy-Zustand — der schon im Dateikopf geplante Schnitt) und
    `useComposerAttachments.ts` (Anhänge hinzufügen und neu bewerten).
  - **Darstellung in Kindkomponenten** (`src/components/chat/`), rein über
    Props/Emits, ohne eigene Logik, die das Harness nicht sähe:
    `ThreadSidebar.vue` (der `<aside>`-Block, ≈ 150 Zeilen Template),
    `MessageList.vue` (Transkript samt Reasoning, Tool-Zeilen und Übersetzung der
    Audit-Marker) und `Composer.vue` (Eingabebereich; nutzt die vorhandenen
    `ComposerControl`, `ComposerAttachments`, `VoiceInputControl`).
  - **`ChatApp.vue`** bleibt als schlanker Orchestrator (Layout, `onMounted`-Wiring,
    Berechtigungsmodus) mit dem Ziel unter 500 Zeilen.
  - **Absicherung**: `check:chat-state` muss nach **jedem** Schritt dieselbe Zahl
    grüner Replay-Tests melden wie vorher (reines Verschieben, kein neues
    Verhalten); jeder Schritt ist ein eigener Commit. Die genauen Symbolgrenzen legt
    `tasks.md` nach erneutem Lesen der Datei fest — die Größen oben stammen aus den
    Zeilenbereichen von Template und Skript, nicht aus einer Symbolanalyse.
- Danach: `git mv` nach `src/components/apps/ChatApp.vue`; `settings` →
  `SettingsApp.vue`, `federation` → `FederationApp.vue`. Nur Routenkopplungen werden
  ersetzt (`useRoute().params.instance` → `useInstancesStore().activeInstance`, die
  zwei Einstellungs-`NuxtLink`s → `shell.openApp('system.settings')`, `lock()` ruft
  vorher `shell.flushAsync()`); der Layout-Wrapper (`h-screen`) wird zu
  `h-full min-h-0`.
- Die drei Seitendateien bleiben als **Redirect-Seiten** bestehen
  (`definePageMeta({ redirect })`) auf `/workspace/<instance>?open=system.<app>`;
  die Shell-Seite verbraucht `open` einmal und entfernt es per `router.replace`.
  `onboarded` bleibt an der Shell-Seite.
- `scripts/check-chat-state.ts:47` lädt die Seite von einem festen Pfad — der Pfad
  wird auf `ChatApp.vue` geändert; die Harness-Logik bleibt unverändert.

**Rationale**: FR-003/FR-004 verlangen, dass der Chat ein Fenster-Inhalt wird. Die
Datei dafür anzufassen und dabei die 500-Zeilen-Grenze weiter zu verletzen, wäre
inkonsequent. Zuerst zerlegen, dann verschieben hält beide Änderungen getrennt
prüfbar.

**Alternatives considered**: Erst verschieben, später zerlegen (der ursprüngliche
Plan) — verworfen: der Umzug ändert Routenkopplungen in genau dieser Datei, ein
späterer Split müsste erneut alles anfassen. Split als eigener Vorgänger-PR — möglich
und gleichwertig; als erste Phase derselben Spec bleibt es im selben Review-Fluss.

## R11. Modell-Preload-Status

**Decision**: Die Preload-Listener und der Statuszustand aus dem heutigen
Workspace-Stub wandern unverändert in ein Composable `useModelPreloadStatus`
und werden in `ShellStatusBar.vue` angezeigt. Die i18n-Schlüssel
`workspace.modelPreload.*` bleiben unverändert.

**Rationale**: FR-005 (Status unabhängig von offenen Fenstern sichtbar) und Spec 004.
Verschieben statt neu schreiben (Constitution: bestehenden Kandidaten erweitern).

## R12. Vertrag zwischen Shell und App

**Decision**: Apps sprechen die Shell über `useShellTab()` an (`provide`/`inject`,
pro **Tab**): `tabId`, `windowId`, `requestAttention()` / `clearAttention()`,
`registerCloseGuard(guard)`, `setTitle(title | null)`, `closeSelf()`. Der Chat
registriert einen Guard, der bei laufender Antwort oder ausstehender Freigabe
Bestätigung verlangt und bei Bestätigung die bestehende Abbruchfunktion aufruft
(Spec 003). Der Chat setzt die Aufmerksamkeit, wenn `onToolPermissionRequest`
feuert, und löscht sie, wenn die Freigabe beantwortet wird oder der Turn endet.
Schließt der Nutzer ein Fenster, befragt die Shell die Guards **aller** seiner
Tabs und zeigt eine Bestätigung, die die betroffenen Tabs nennt.

**Rationale**: Apps bleiben von der Shell entkoppelt (kein Import von Store oder
Komponenten der Shell), und der Vertrag ist für spätere Apps (auch Extensions)
derselbe. Details: `contracts/shell-app-contract.md`.

## R13. Speichern, Sperren und Schließen

**Decision**: Geometrie- und Stapeländerungen werden mit 400 ms Debounce als Batch
gespeichert (`shell_save_windows`); strukturelle Änderungen (Öffnen, Schließen,
Arbeitsbereich verschieben) sofort. Schlägt Speichern fehl, bleibt der Zustand im
Speicher, das Fenster wird als schmutzig markiert, und der nächste Speichervorgang
schreibt es erneut (Edge Case „Speichern schlägt fehl“). `shell.flushAsync()`
wird **vor** `closeAsync()` aufgerufen; zusätzlich versucht die Shell beim
Schließen des Anwendungsfensters ein Flush über Tauris `onCloseRequested`.

**Follow-up (ponytail)**: Das Flush beim Prozessende ist best-effort — Grenze: ein Absturz
verliert höchstens die letzten 400 ms Layoutänderungen. Kein Upgrade-Pfad nötig.

**Rationale / Verbindung zu Spec 013**: Sobald 013 „Schließen beendet den Prozess“
umsetzt, muss `flushAsync()` vor dem Schließen abgewartet werden; genau das
leistet die Reihenfolge oben.

## R14. Oberfläche

**Decision**:

- Launcher, Fensterübersicht und Arbeitsbereichs-Übersicht nutzen `UiDrawerModal`
  aus dem haex-ui-Layer (responsiv: Dialog am Desktop, Drawer mobil).
- Bestätigungen (Tab/Fenster mit laufender Antwort schließen, Arbeitsbereich
  löschen): `ShadcnAlertDialog`.
- Arbeitsbereichs-Aktionen (löschen, Fenster verschieben), das „+“-Menü und
  die Tab-Liste (Chevron): `ShadcnDropdownMenu` (Tastaturbedienung und Fokus
  liefert reka-ui).
- Kein Karussell/Swipe zwischen Arbeitsbereichen (haex-vault 2.x nutzt Swiper): nicht
  gefordert, Wechsel läuft über Auswahl und Übersicht.
- Aufmerksamkeitshinweis: Punkt-Badge am Tab, im Chevron-Eintrag, am Eintrag in
  Fensterübersicht und Launcher-Leiste und an der Arbeitsbereichs-Auswahl.
- Tastatur/Screenreader (FR-029, FR-038): Titelleistenbuttons mit `aria-label`;
  Tab-Leiste nach dem ARIA-Tab-Muster (`role="tablist"`/`"tab"`/`"tabpanel"`,
  `aria-selected`, Roving-`tabindex`, Pfeiltasten/Home/End wechseln, Eingabe/
  Leertaste aktiviert); Fenster bekommen `role="group"` mit `aria-label` = Titel des
  aktiven Tabs; Fokus wandert beim Fokussieren ins Fenster (`tabindex="-1"` +
  `focus()`); Icons `aria-hidden` (Vorbild: Branch `fix/icon-aria-hidden`).

**Komponenten der Titelleiste** (je eine Datei, zusammen deutlich unter 500 Zeilen):
`ShellWindow.vue` (Rahmen, Griffe, Titelleisten-Grundriss), `ShellTabBar.vue`
(Leiste, Scroll-Pfeile, „+“), `ShellNewTabMenu.vue` („+“-Liste),
`ShellTabListMenu.vue` (Chevron-Dropdown), `ShellWindowControls.vue` (Minimieren,
Maximieren, Schließen).

**Rationale**: Nur bereits vorhandene Komponenten; keine neue UI-Abhängigkeit.

## R15. Tests und CI

**Decision**:

- **Rust**: `cargo test` mit getrennten `*_tests.rs` (Constitution): Invarianten der
  Speicherschicht (Standard-Arbeitsbereich, letzter nicht löschbar, dichte
  Positionen, Löschen entfernt Fenster und Tabs, Tab-Ersatz je Fenster in einer
  Transaktion, `activeTabId` ∈ Tabs, Fenster ohne Tab abgelehnt, Geräteisolation,
  Adoption/Kopie zeigt keine fremden Zeilen), Command-Validierung.
- **Frontend**: neues Skript `scripts/check-shell-state.ts`
  (`pnpm check:shell-state`, in `ci.yml` neben `check:chat-state`) prüft die reinen
  Module und den Store mit gemocktem `invoke`: Singleton über Fenster hinweg,
  Tab öffnen/wechseln/schließen (Nachbar, letzter Tab schließt Fenster), Maximieren
  ohne Geometrieverlust, Kaskade, Klemmen, Kompaktrückkehr, Speichern/Flush-
  Reihenfolge, unbekannte App beim Wiederherstellen (Tab und leeres Fenster).
- **Regression**: `check:chat-state`, `check:templates`, `typecheck`,
  `typecheck:scripts`, `lint`, `format:check`.
- **Manuell**: `quickstart.md` (kein Browser-Harness im Projekt; UI-Verhalten
  wird gegen die Akzeptanzszenarien im Tauri-Dev-Build geprüft).

## R16. Sprache und Begriffe

**Decision**: Neuer Namensraum `shell.*` in `de.json`/`en.json` (u. a. Neuer Tab,
Alle Tabs, Tab schließen, Minimieren, Maximieren, Wiederherstellen, Schließen);
UI-Begriff
„Arbeitsbereich“, Code „workspace“. Obsolete Schlüssel `workspace.chatFab.*`,
`workspace.settings.*`, `workspace.heading` entfallen mit dem Stub (nur wenn
nach dem Umbau ungenutzt, geprüft mit `rg`). `CONTEXT.md` bekommt die Begriffe
**Shell**, **App**, **Fenster (Window)** und **Launcher**.

## R17. Phasen-Disziplin (Constitution)

**Befund**: `plans/README.md` beschreibt die Reihenfolge Integrationsbasis →
Instanzlebenszyklus → Anbieter → Chat → Paketabnahme, danach Zwei-Geräte-Sync.
Eine Shell-Phase steht dort nicht; die Spec wurde vom Betreiber am 2026-09-21
ausdrücklich angefordert.

**Konsequenz**: Die Constitution erlaubt, spätere Phasen vorab zu spezifizieren
und zu planen, verbietet aber die Implementierung vor den Voraussetzungen. Der
Plan blockiert nicht; vor `/speckit-implement` muss der Betreiber bestätigen,
dass die Shell jetzt in die Roadmap gehört, und `plans/README.md` bekommt einen
Eintrag (Aufgabe in `tasks.md`).

## R18. Titelleisten-Layout (Firefox-Bedienung)

**Decision**: Die Titelleiste ist eine Flex-Zeile:

```text
[ tablist ][ + ][ ── Zieh-/Doppelklickfläche ── ]   [ ⌄ ][ – ][ ⤢ ][ × ]
```

- **`tablist`**: `flex: 0 1 auto; min-width: 0; overflow-x: auto` — sie schrumpft
  nur, wenn der Platz fehlt. **„+“**: `flex: none`, steht dadurch unmittelbar
  hinter dem letzten Tab und bleibt bei Überlauf am rechten Ende der Leiste
  stehen (Firefox-Verhalten, FR-032).
- **Scroll-Pfeile** (`lucide:chevron-left`/`-right`) erscheinen nur bei Überlauf an
  den Leistenkanten (wie haex-vault 2.x). Der **Chevron der Tab-Liste**
  (`lucide:chevron-down`) sitzt dagegen immer im rechten Schaltflächenblock, vor
  Minimieren/Maximieren/Schließen (FR-034) und ist damit klar vom Scroll-Pfeil zu
  unterscheiden (Tooltip und `aria-label` „Alle Tabs“).
- **Ein Tab**: kein Tab-Rahmen, kein Tab-Schließen; Icon und Titel stehen in
  derselben `tablist`-Box, sodass das „+“ an derselben Stelle folgt (FR-031).
- **Zieh- und Doppelklickfläche**: nur die freie Fläche (und der Titel bei einem Tab)
  zieht das Fenster und schaltet per Doppelklick Maximieren um; Tabs, „+“ und
  Schaltflächen halten den `pointerdown` an (`@pointerdown.stop`).
- **Kompakt** (FR-036): `tablist` entfällt; es bleiben Titel des aktiven Tabs
  (gekürzt), „+“ und der Chevron; Minimieren/Maximieren sind ausgeblendet.
- **Sichtbar halten**: Nach Aktivierung (Klick, Chevron-Auswahl, „+“) ruft die
  Leiste `scrollIntoView({ inline: 'nearest' })` am aktiven Tab auf (FR-035).
- **„+“-Menü**: alle Apps der Registry (Icon, Name); bei einer Einzelinstanz-App mit
  vorhandenem Tab steht ein „geöffnet“-Hinweis, die Auswahl aktiviert den Tab
  (FR-033). **Tab-Liste**: ein Eintrag je Tab in Leistenreihenfolge, aktiver Tab mit
  Häkchen, Aufmerksamkeits-Badge.

**Rationale**: Die Rechte-Seite des „+“ in haex-vault 2.x (`window/index.vue`,
`newTabMenuItems`) bleibt inhaltlich erhalten; verschoben wird nur die Stelle, und
der frei gewordene Platz gehört dem Chevron. Die Anordnung ist reines CSS-Flex ohne
JavaScript-Messung.

**Alternatives considered**: „+“ dupliziert direkt den aktiven Tab (haex-vault 2.x
`addNewTabFromActive`) — verworfen: bei Einzelinstanz-Apps wirkungslos und für
Chat/Einstellungen/Föderation in dieser Spec immer ein No-op; das Menü ist
einheitlich. Tabs per Drag umsortieren oder herauslösen — verworfen (nicht im
Umfang, Spec „Nicht im Umfang“).
