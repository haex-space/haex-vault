# Vertrag: Navigation im Tab (App-Seite)

Ergänzt [`015-workspace-shell/contracts/shell-app-contract.md`](../../015-workspace-shell/contracts/shell-app-contract.md).
Apps sehen weiterhin nur `useWmTab()` und die hier beschriebenen Bausteine;
sie importieren nichts aus `stores/windowManager.ts` oder `components/wm/` außer
`WmRouterView` und `WmLink`.

## 1. Routen anmelden

In `src/components/wm/appRoutes.ts` (ersetzt `appComponents.ts`):

```ts
// Form, kein Implementierungscode
'system.chat': {
  routes: [
    { path: '/', component: ChatApp, children: [
      { path: '', component: ChatNewConversation },          // Start-Ort
      { path: 'thread/:id', component: ChatThreadView, titleKey: 'wm.chat.thread' },
    ]},
  ],
},
'system.federation': { component: FederationApp },           // ohne Routen = nur '/'
```

- Pfade der Kinder sind relativ zum Eltern-Eintrag; `''` ist das Index-Kind.
- Jede App hat einen Start-Ort `/`.
- Eine App ohne `routes` hat implizit genau `/` → ihre Komponente.
- Ob die Chat-Ansichten als eigene Kind-Komponenten oder als eine Komponente mit
  Watcher auf `route.params` umgesetzt werden, entscheidet die Umsetzung
  (research R10); der Vertrag ist nur die Routentabelle.

## 2. `useTabRouter()`

```ts
type TabRouter = {
  readonly route: {
    // reaktiv
    path: string
    query: Record<string, string>
    params: Record<string, string> // aus dem Muster, z. B. { id }
    matched: readonly { path: string; titleKey?: string }[]
  }
  readonly canGoBack: boolean
  readonly canGoForward: boolean
  push(to: string | TabLocation): void // neue Ansicht (FR-004), No-op bei gleichem Ort (FR-006)
  replace(to: string | TabLocation): void // gleiche Ansicht anders (FR-005)
  setQuery(
    patch: Record<string, string | null>,
    opts?: { push?: boolean },
  ): void // Standard: replace
  back(): void
  forward(): void
  skipCurrent(): boolean // Eintrag mit verschwundenem Ziel überspringen (FR-027)
  swapHistory(next: TabHistory): TabHistory | null // ganze Historie des Tabs tauschen (Spec 062)
}
```

- `to` als String: `'/models/hf/abc?sort=size'` (Query wird geparst); relative
  Pfade sind nicht erlaubt.
- Alle Aufrufe wirken ausschließlich auf den eigenen Tab (FR-008).
- `swapHistory(next)` (Spec 062, Dateibrowser mit zwei Panels) ersetzt die ganze Historie des eigenen
  Tabs durch `next` und gibt die bisherige zurück; verletzt `next` die Invarianten von `TabHistory`
  (1 bis 50 Einträge, Index in Grenzen) oder gibt es keinen Tab, ist das Ergebnis `null` und nichts
  ändert sich. Der Store tut dasselbe mit `setHistory(tabId, history): boolean`; beides setzt den
  letzten Laufweg und den dynamischen Titel des Tabs zurück und speichert die Sitzung (Spec 022).
  Der inerte Router gibt `null` zurück.
- Außerhalb einer Shell liefert `useTabRouter()` einen inerten Router mit Ort
  `/` (wie `useWmTab()`), damit Apps in Test-Harnesses montierbar bleiben.

**Regel für Apps (FR-005)**: `push` für alles, was der Nutzer als „andere Seite“
wahrnimmt (andere Ansicht, anderes Element); `replace`/`setQuery` für Filter,
Suche, Sortierung, Aufklappzustand, Tabs innerhalb einer Ansicht.

## 3. `WmRouterView` und `WmLink`

- `<WmRouterView />` rendert den gematchten Eintrag der eigenen Tiefe. Die
  Wurzel rendert die Shell selbst in `WmTabPanel`; Apps setzen
  `<WmRouterView />` dort ein, wo Kinder erscheinen (z. B. rechts neben der
  Seitenleiste).
- `<WmLink to="/models" [replace] [active-class]>` rendert ein `<a>` mit
  `href="#"` (kein Webview-Navigationsziel), löst `push`/`replace` aus und setzt
  `aria-current="page"`, wenn der aktuelle Pfad gleich oder (mit `prefix`) ein
  Unterpfad ist — Grundlage für die Seitenleisten-Markierung (US1 AS7).
- Unbekannter Pfad: `WmRouterView` der Wurzel ersetzt per `replace('/')` und
  zeigt den Hinweis `wm.nav.unknownLocation` (FR-014).

## 4. Erweiterung von `useWmTab()`

```ts
openApp(appId: string, at?: string | TabLocation): void   // FR-012
```

- Neue Instanz: Tab mit Historie `[at ?? '/']`.
- Offene Einzelinstanz: Tab aktivieren, Fenster wiederherstellen und
  fokussieren, Arbeitsbereich aktivieren (015 FR-016), dann `push(at)`.
- `setTitle(title)` bleibt; ein gesetzter Titel gilt bis zur nächsten Navigation
  des Tabs (research R9).
- `registerActionHandler(actionId, handler)`: meldet einen tab-gebundenen
  Handler für eine Aktion der eigenen App an (research R19,
  [wm-actions.md](./wm-actions.md) §2); gibt eine Abmeldefunktion zurück.

## 5. Legacy-Adressen

`/<seite>/<instance>` → `/workspace/<instance>?open=<appId>[&at=<pfad>]`. Die
Host-Seite ruft `openApp(appId, at)` einmal auf und entfernt beide Parameter per
`router.replace` (wie bisher `open`).

## 6. Was die Shell garantiert

- Historie überlebt Tab-Wechsel, Minimieren, Maximieren, Arbeitsbereichswechsel,
  Verschieben (FR-010), nicht aber Neustart, Sperren oder Schließen des Tabs
  (FR-011).
- Zurück/Vor schließen nie etwas (FR-007).
- Beim Navigieren wird die verlassene Ansicht unmontiert; Apps, die Eingaben
  retten wollen, tun das selbst (spec Annahmen).
- Inhalte eines Tabs (auch eingebettete Dokumente) können Ort und Historie
  nicht verändern; nur `useTabRouter()` der eigenen App und Aktionen tun das
  (FR-034, research R17).
