# Vertrag: Aktionen und Eingaben

Die Aktionen der Spec (FR-024–FR-033) heißen im Code **Aktionen**, weil „Command“
in holzi Tauri-Commands bezeichnet (research R8). Datenformen:
[data-model.md](../data-model.md#shellactiondefinition-aktion).

## 1. Katalog

**Umsetzungsstand:** Der ausgelieferte Katalog steht in `src/lib/actions/` (58
Aktionen; `wm.actions.list` liefert ihn zur Laufzeit). Gegenüber der Liste unten
kamen `wm.window.setGeometry`, `wm.workspaces.overview`,
`chat.model.retryLoad`, `chat.model.downloadRecommended`,
`chat.modelIntegrity.decide` (Leitplanke), `chat.voice.setAutoSend` sowie weitere
Einstellungsaktionen hinzu. Der Sperren-Knopf (seit der Chat-Rahmen-Änderung
der letzte Eintrag des Launchers, Logik in `useVaultLock`) bleibt bewusst direkt
(Spec 013 prüft ihn); eine Aktion `wm.vault.lock` folgt mit
der Spec für Tastenkürzel.

Die folgende Liste ist der Mindestumfang. Die endgültige Liste der
Einstellungs- und Chat-Aktionen entsteht in tasks aus einer Bestandsaufnahme
aller Bedienelemente (SC-007); jede dort gefundene zustandsändernde Bedienung
bekommt eine Aktion oder eine begründete `action-exempt:`-Ausnahme (R20).

### Shell (`binding: global`)

| Id                                            | Ziel      | Bereich         | Wirkung     | Agent                     | Kürzel                                  |
| --------------------------------------------- | --------- | --------------- | ----------- | ------------------------- | --------------------------------------- |
| `wm.tab.back`                                 | tab       | `wm.navigation` | write       | ja                        | Alt+ArrowLeft; mac + Meta+BracketLeft   |
| `wm.tab.forward`                              | tab       | `wm.navigation` | write       | ja                        | Alt+ArrowRight; mac + Meta+BracketRight |
| `wm.tab.go` (`steps`)                         | tab       | `wm.navigation` | write       | ja                        | –                                       |
| `wm.tab.navigate` (`to`, `replace?`)          | tab       | `wm.navigation` | write       | ja                        | –                                       |
| `wm.app.open` (`appId`, `at?`)                | none      | `wm.navigation` | write       | ja                        | –                                       |
| `wm.tab.new` (`appId`, `at?`)                 | window    | `wm.layout`     | write       | ja                        | –                                       |
| `wm.tab.activate`                             | tab       | `wm.layout`     | write       | ja                        | –                                       |
| `wm.tab.close`                                | tab       | `wm.layout`     | destructive | ja                        | –                                       |
| `wm.window.focus`                             | window    | `wm.layout`     | write       | ja                        | –                                       |
| `wm.window.minimize`                          | window    | `wm.layout`     | write       | ja                        | –                                       |
| `wm.window.toggleMaximize`                    | window    | `wm.layout`     | write       | ja                        | –                                       |
| `wm.window.close`                             | window    | `wm.layout`     | destructive | ja                        | –                                       |
| `wm.window.moveToWorkspace` (`toWorkspaceId`) | window    | `wm.layout`     | write       | ja                        | –                                       |
| `wm.workspace.create`                         | none      | `wm.layout`     | write       | ja                        | –                                       |
| `wm.workspace.switch`                         | workspace | `wm.layout`     | write       | ja                        | –                                       |
| `wm.workspace.delete`                         | workspace | `wm.layout`     | destructive | ja                        | –                                       |
| `wm.windows.overview`                         | none      | `wm.layout`     | write       | ja                        | –                                       |
| `wm.launcher.open`                            | none      | `wm.layout`     | write       | ja                        | –                                       |
| `wm.system.back`                              | none      | `wm.navigation` | write       | nein (nur Plattform-Hook) | –                                       |
| `wm.state.get`                                | none      | `wm.read`       | read        | ja                        | –                                       |
| `wm.tab.history`                              | tab       | `wm.read`       | read        | ja                        | –                                       |
| `wm.apps.list`                                | none      | `wm.read`       | read        | ja                        | –                                       |
| `wm.actions.list`                             | none      | `wm.read`       | read        | ja                        | –                                       |

Fenster- und Tab-Schließen durch einen Agenten laufen durch dieselben Guards wie
beim Nutzer (015 FR-014): Die Bestätigung erscheint beim Nutzer; lehnt er ab,
liefert die Aktion `failed` mit `message: "declined by user"`.

### Chat (`binding: tab`, `appId: system.chat`)

| Id                                                 | Bereich      | Wirkung     | Agent    |
| -------------------------------------------------- | ------------ | ----------- | -------- |
| `chat.conversation.new`                            | `chat.write` | write       | ja       |
| `chat.conversation.open` (`threadId`)              | `chat.read`  | write       | ja       |
| `chat.conversation.rename` (`threadId`, `title`)   | `chat.write` | write       | ja       |
| `chat.conversation.delete` (`threadId`)            | `chat.write` | destructive | ja       |
| `chat.conversations.list`                          | `chat.read`  | read        | ja       |
| `chat.messages.list` (`threadId`)                  | `chat.read`  | read        | ja       |
| `chat.message.send` (`text`)                       | `chat.write` | write       | ja       |
| `chat.reply.cancel`                                | `chat.write` | write       | ja       |
| `chat.approval.decide` (`requestId`, `decision`)   | `guardrails` | write       | **nein** |
| `chat.permissionMode.set` (`manual`/`auto`/`plan`) | `guardrails` | write       | **nein** |

### Chat model preferences (`binding: global`)

| Id                              | Bereich           | Wirkung | Agent |
| ------------------------------- | ----------------- | ------- | ----- |
| `chat.model.select` (`modelId`) | `settings.models` | write   | ja    |

### Einstellungen (`binding: global`)

| Id                                                         | Bereich           | Wirkung             | Agent    |
| ---------------------------------------------------------- | ----------------- | ------------------- | -------- |
| `settings.get`                                             | `settings.read`   | read                | ja       |
| `settings.device.setAlias` (`alias`)                       | `settings.device` | write               | ja       |
| `settings.models.setDefault` (`modelId`)                   | `settings.models` | write               | ja       |
| `settings.models.setStt` (`modelId`)                       | `settings.models` | write               | ja       |
| `settings.models.download` / `.cancelDownload` / `.delete` | `settings.models` | write / destructive | ja       |
| `settings.delegate.connectProvider`                        | `guardrails`      | write               | **nein** |
| `settings.autonomy.setMode`                                | `guardrails`      | write               | **nein** |
| `settings.delegate.setDenyRules`                           | `guardrails`      | write               | **nein** |

`settings.get` gibt Werte der Leitplanken lesbar zurück, aber **nie**
Zugangsdaten oder Geheimnisse von Anbietern.

## 2. Aufruf

```ts
shell.runAction(id: string, input?: Record<string, unknown>, caller?: ActionCaller): Promise<ActionOutcome>
// caller Standard: { kind: 'user' }
```

- Ablauf und Fehlercodes: [data-model.md](../data-model.md#actionoutcome-ergebnis).
- Schaltflächen, Menüs und Tastenkürzel rufen ausschließlich `runAction`
  (FR-024); in Vue-Komponenten über `useAction(id)` (liefert eine
  Aufruf-Funktion mit `caller: user`).
- Tab-gebundene Handler registriert die App mit
  `useWmTab().registerActionHandler(id, handler)`; ein Handler je Aktion und
  Tab-Instanz, Abmeldung beim Unmount.

## 3. Tastatur

- Ein einziger `keydown`-Listener (Capture-Phase) im obersten Dokument auf der
  Shell-Host-Seite.
- Normierung `Ctrl+Alt+Shift+Meta+<KeyboardEvent.code>`; Plattform `mac`, wenn
  `navigator.userAgentData?.platform ?? navigator.platform` mit „Mac“ beginnt.
- Treffer → `preventDefault()` + `runAction(id, {}, user)`. Ausnahme:
  `yieldToTextInput` für die Plattform gesetzt und `event.target` editierbar
  (`input`, `textarea`, `[contenteditable]`) → nicht abfangen (FR-017).
- Komponentenlokale Tasten (Pfeiltasten der Tab-Leiste, Escape in Popovern,
  Enter im Composer) bleiben lokal (FR-026).
- Browser-eigene Kürzel des Webviews werden abgeschaltet, wo die Plattform es
  anbietet (research R17).

## 4. Maus

- Am Fenster-Wurzelelement (`wm/Window.vue`, `data-wm-window-id`):
  `mouseup` mit `button === 3` → `wm.tab.back`, `button === 4` →
  `wm.tab.forward`, mit dem aktiven Tab dieses Fensters als `tabId`; kein
  `focusWindow` (FR-018). `mousedown`/`auxclick` dieser Tasten:
  `preventDefault`.
- Außerhalb von Fenstern: nichts. Über eingebetteten Dokumenten: siehe research
  R17.

## 5. System-Zurück

- Nur der native Zurück-Hook der Plattform ruft `wm.system.back` auf
  (research R7). Die Browser-Historie des Webviews löst nie eine Aktion aus.
- `pages/workspace/[instance].vue` bricht jede Router-Navigation weg von der
  Seite ab und ignoriert sie (`onBeforeRouteLeave`), ohne sie zu deuten.
- `wm.system.back`:
  1. Offenes Shell-Overlay (Launcher, Fensterübersicht,
     Arbeitsbereichs-Übersicht, „+“-Liste oder Chevron) → schließen.
  2. Sonst aktiver Tab des obersten sichtbaren Fensters → `back()`.
  3. Kein Zurück-Eintrag: Kompaktdarstellung → Fensterübersicht öffnen; sonst
     nichts. Die App wird nie verlassen.

## 6. Zugriff auf die Tab-Historie

- In der Titelleiste gibt es keine Zurück/Vor-Schaltflächen und keine
  Verlaufsliste. Tastenkürzel und Maustasten bleiben gemäß FR-017–018 verfügbar.
- `wm.tab.history` liefert die Historie; `wm.tab.go` springt zu einem Eintrag.
- Titelleisten- und Tab-Bedienelemente starten keinen Fenster-Drag.
