# Data Model: Dock

## Gespeichert

### `dock.items` (Präferenz, Vault-Scope, synchronisiert)

JSON-Array, Reihenfolge = Reihenfolge im Dock.

```ts
type DockControlId = 'launcher' | 'workspaces' | 'windows'

type DockItem =
  { kind: 'control'; id: DockControlId } | { kind: 'app'; appId: string } // System-App oder `extension.<id>`
```

Regeln beim Lesen (`normalizeDockItems`):

| Fall                                                 | Ergebnis                                                                                             |
| ---------------------------------------------------- | ---------------------------------------------------------------------------------------------------- |
| Wert fehlt, kein JSON, kein Array                    | Default `[launcher, system.files, system.chat, system.passwords, system.settings]`, nichts schreiben |
| Element mit unbekanntem `kind` oder falschem Feldtyp | Element verwerfen                                                                                    |
| `appId` ist ein Legacy-Alias (`system.federation`)   | auf Ziel-App abbilden (`resolveAppAlias`)                                                            |
| doppelter Steuer-Eintrag oder doppelte `appId`       | erstes Vorkommen bleibt                                                                              |
| kein `launcher`                                      | `launcher` an Position 0 einfügen                                                                    |
| `appId` nicht in `allApps()`                         | bleibt gespeichert, `available: false`, im Dock ausgeblendet                                         |

Geschrieben wird immer die normalisierte Liste einschließlich nicht verfügbarer Apps, und nur auf eine
Nutzeränderung hin (FR-038).

### `dock.placement` (Präferenz, Geräte-Scope, nicht synchronisiert)

```ts
type DockEdge = 'top' | 'bottom' | 'left' | 'right'
type DockAlign = 'start' | 'center' | 'end'
type DockStyle = 'bar' | 'wheel'
type DockMode = 'reserved' | 'floating' | 'autohide'

type DockPlacement = {
  style: DockStyle
  edge: DockEdge
  align: DockAlign
  mode: DockMode // für Leiste und Rad
}
```

Default: `{ style: 'bar', edge: 'bottom', align: 'center', mode: 'reserved' }`. Ein ungültiges Feld fällt
einzeln auf seinen Default zurück.

## Abgeleitet (nie gespeichert)

### Effektive Platzierung

```ts
effectivePlacement(p: DockPlacement, compact: boolean): DockPlacement
```

| Bedingung          | Ergebnis                                                          |
| ------------------ | ----------------------------------------------------------------- |
| `!compact`         | `p`                                                               |
| `compact`, `bar`   | `{ bar, bottom, center, reserved }`                               |
| `compact`, `wheel` | `{ wheel, bottom, align: p.align === 'start' ? 'start' : 'end' }` |

### Dock-Eintrag zur Anzeige

```ts
type DockInstance = { tabId: string; windowId: string; workspaceId: string }

type DockEntry =
  | { kind: 'control'; id: DockControlId }
  | {
      kind: 'app'
      appId: string
      pinned: boolean
      instances: DockInstance[] // Tabs dieser App in allen Arbeitsbereichen
    }
```

`resolveDockEntries` liefert erst die verfügbaren Einträge aus `dock.items` in ihrer Reihenfolge, dann
(`pinned: false`) jede App mit Instanzen, die nicht angeheftet ist, in der Reihenfolge ihres ersten Tabs.
Aufmerksamkeit kommt zur Renderzeit aus `wm.appHasAttention(appId)`.

### Aktivierung

```ts
dockActivation(entry): { kind: 'open' } | { kind: 'focus'; tabId: string } | { kind: 'choose' }
```

0 Instanzen → `open`, 1 → `focus`, ≥ 2 → `choose`.

## Zustand im Window Manager (geändert)

`WmState.compact` wird nicht mehr aus `area.width` abgeleitet, sondern aus der Breite des App-Fensters
(research R1). `area` bleibt die Fläche, in die Fenster passen.
