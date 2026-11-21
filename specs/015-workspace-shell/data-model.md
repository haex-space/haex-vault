# Data Model: Workspace-Shell

**Feature**: 015-workspace-shell | **Date**: 2026-09-21

Entscheidungen und Begründungen: [research.md](./research.md) (R1–R3, R9). Alle
Tabellen folgen [ADR-0001](../../docs/adr/0001-device-scoped-data-convention.md):
CRDT-getrackt (kein `_no_sync`), `vault_device_uuid` Teil des Primärschlüssels,
Fremdschlüssel auf `known_devices(vault_device_uuid) ON DELETE CASCADE`.

## Persistent (SQLite, Migration `0019_shell_layout`)

### `workspaces`

| Spalte              | Typ              | Regel                                                    |
| ------------------- | ---------------- | -------------------------------------------------------- |
| `vault_device_uuid` | TEXT NOT NULL    | FK → `known_devices` ON DELETE CASCADE; nie die Nil-UUID |
| `workspace_id`      | TEXT NOT NULL    | UUID v4, vom Backend vergeben                            |
| `position`          | INTEGER NOT NULL | dicht 0…n−1 je Gerät                                     |

PK `(vault_device_uuid, workspace_id)`; `UNIQUE (workspace_id)` (Ziel der Fremdschlüssel, R2); Index `(vault_device_uuid, position)`.

Es gibt bewusst **keine Namensspalte**: Die Oberfläche zeigt „Arbeitsbereich N“ bzw.
„Workspace N“ mit N = `position` + 1 (Spec FR-019). Wird ein mittlerer
Arbeitsbereich gelöscht, verdichtet die Speicherschicht die `position`-Werte; die
Nummern bleiben dadurch lückenlos, ohne dass etwas umbenannt werden muss.

### `shell_windows`

| Spalte              | Typ                        | Regel                                                                                           |
| ------------------- | -------------------------- | ----------------------------------------------------------------------------------------------- |
| `vault_device_uuid` | TEXT NOT NULL              | FK wie oben                                                                                     |
| `window_id`         | TEXT NOT NULL              | UUID v4, vom Frontend vergeben                                                                  |
| `workspace_id`      | TEXT NOT NULL              | FK → `workspaces(workspace_id)` ON DELETE CASCADE (Ziel; Fallback ohne FK: Speicherschicht, R2) |
| `x`, `y`            | INTEGER NOT NULL           | Normalgeometrie, ±100 000                                                                       |
| `width`, `height`   | INTEGER NOT NULL           | Normalgeometrie, 1–100 000                                                                      |
| `is_minimized`      | INTEGER NOT NULL DEFAULT 0 | 0/1                                                                                             |
| `is_maximized`      | INTEGER NOT NULL DEFAULT 0 | 0/1; die Normalgeometrie bleibt beim Maximieren unverändert (R7)                                |
| `stack_order`       | INTEGER NOT NULL           | Rang der Stapelreihenfolge je Gerät (höher = weiter vorn), beim Speichern dicht                 |
| `active_tab_id`     | TEXT NOT NULL              | einer der Tabs des Fensters                                                                     |

PK `(vault_device_uuid, window_id)`; `UNIQUE (window_id)` (Ziel des Fremdschlüssels, R2); Index `(vault_device_uuid, workspace_id)`.

### `shell_window_tabs`

| Spalte              | Typ              | Regel                                                                                           |
| ------------------- | ---------------- | ----------------------------------------------------------------------------------------------- |
| `vault_device_uuid` | TEXT NOT NULL    | FK wie oben                                                                                     |
| `tab_id`            | TEXT NOT NULL    | UUID v4, vom Frontend vergeben                                                                  |
| `window_id`         | TEXT NOT NULL    | FK → `shell_windows(window_id)` ON DELETE CASCADE (Ziel; Fallback ohne FK: Speicherschicht, R2) |
| `app_id`            | TEXT NOT NULL    | opak, 1–128 Zeichen, keine Steuerzeichen (z. B. `system.chat`)                                  |
| `position`          | INTEGER NOT NULL | dicht 0…n−1 je Fenster                                                                          |

PK `(vault_device_uuid, tab_id)`; Index `(vault_device_uuid, window_id, position)`.

### Bestehend, unverändert: `preferences`

Ein neuer Schlüssel `shell.active_workspace_id`, Device-Scope
(`PrefScope::Device`), Wert = `workspace_id`. Fehlt der Schlüssel oder verweist er auf
einen nicht (mehr) vorhandenen Arbeitsbereich, gilt der erste nach `position`;
`shell_load_layout` schreibt den korrigierten Wert zurück.

### Invarianten (Speicherschicht in Transaktionen, wo nicht das Schema selbst sie garantiert)

| #   | Invariante                                                                                                                                                  | Spec           |
| --- | ----------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------- |
| I1  | Nach `shell_load_layout` existiert ≥ 1 Arbeitsbereich für das aktuelle Gerät                                                                                | FR-018         |
| I2  | `position` der Arbeitsbereiche ist dicht 0…n−1 (nach Anlegen und Löschen)                                                                                   | FR-019         |
| I3  | Der letzte Arbeitsbereich ist nicht löschbar (`InvalidInput`)                                                                                               | FR-019         |
| I4  | Ein Fenster verweist auf einen vorhandenen Arbeitsbereich desselben Geräts, sonst `InvalidInput`                                                            | FR-006         |
| I5  | Ein Fenster hat 1–100 Tabs; `tab_id` je Gerät eindeutig; `active_tab_id` ∈ Tabs; `position` dicht                                                           | FR-006, FR-037 |
| I6  | Speichern eines Fensters ersetzt seine Tab-Menge vollständig in derselben Transaktion                                                                       | FR-023         |
| I7  | Löschen eines Arbeitsbereichs entfernt seine Fenster und deren Tabs; Schließen eines Fensters seine Tabs (Datenbank-Cascade; Fallback R2: eine Transaktion) | FR-021         |
| I8  | Alle Zugriffe filtern nach dem im Backend aufgelösten Gerät; das Frontend übergibt keine Geräte-UUID                                                        | FR-024         |
| I9  | Höchstens 500 Fenster je Gerät (Schutz vor Fehlverhalten, keine UI-Regel)                                                                                   | –              |

Kopie/Adoption (FR-024, SC-006): Ein kopierter Vault auf einem neuen Gerät bekommt
eine neue `vault_device_uuid` (`HaexBootstrap`); alle Abfragen filtern danach, die
Zeilen des Quellgeräts sind unsichtbar, und `shell_load_layout` legt einen
Standard-Arbeitsbereich an.

## Frontend-Zustand (nur im Speicher)

```text
ShellState
├─ workspaces: Workspace[]            (geordnet)      Workspace { id }
├─ windows: ShellWindow[]
│    └─ ShellWindow { id, workspaceId, x, y, width, height,
│                     minimized, maximized, stack, tabs: ShellTab[], activeTabId }
│         └─ ShellTab { id, appId }                    ← persistiert
├─ activeWorkspaceId: string
├─ activeWindowId: string | null       (fokussiertes Fenster im aktiven Arbeitsbereich)
├─ nextStack: number                   (monoton steigend)
├─ area: { width, height }             (Größe des Fensterbereichs)
└─ compact: boolean                    (abgeleitet: Fensterbreite ≤ COMPACT_MAX_WIDTH)

TabRuntime  (je tabId, nie persistiert)
├─ attention: boolean
├─ titleOverride: string | null
├─ guard: CloseGuard | null
└─ mounted: boolean                    (lazy Mount, R8)
```

Abgeleitet: Aufmerksamkeit eines **Fensters** = mindestens ein Tab mit
`attention`; eines **Arbeitsbereichs** = mindestens ein Fenster mit Aufmerksamkeit.

### Zustandsübergänge (reine Funktionen in `src/lib/shell/`)

**Fenster**

```text
openApp(appId) ──► [Singleton mit Tab?] ─ja─► activateTab(tab): Tab aktiv, Fenster restore+focus, Arbeitsbereich aktiv
                        │nein
                        ▼
               neues Fenster (1 Tab): Geometrie = App-Standard + Kaskade, stack = ++nextStack, fokussiert
Normal ◄──toggleMaximize──► Maximiert           (Normalgeometrie bleibt)
Normal/Maximiert ──minimize──► Minimiert         (aktives Fenster → nächstes im Stapel)
Minimiert ──restore/focus──► Normal/Maximiert    (stack = ++nextStack)
* ──closeWindow──► entfernt (Tabs mit)           (Guards zuvor, Store-Ebene)
* ──moveToWorkspace(id)──► gleiche Tabs, neuer Arbeitsbereich
```

**Tab**

```text
addTab(windowId, appId): Singleton mit Tab irgendwo → activateTab (kein neuer Tab)
                         sonst Tab hinten anfügen, activeTabId = neuer Tab
switchTab(windowId, tabId): activeTabId setzen
closeTab(windowId, tabId): letzter Tab → closeWindow
                           sonst Tab entfernen; war er aktiv → rechter Nachbar,
                           sonst (letzter) linker Nachbar
```

**Arbeitsbereich**: `create` (hinten), `delete` (Fenster und Tabs
mit; letzter nicht), `setActive`. Löschen des aktiven → Nachbar (vorheriger, sonst
nächster) wird aktiv.

**Wiederherstellen** (`hydrate(layout, apps, area)`):

1. Tabs mit unbekannter `appId` verwerfen; Fenster ohne Tab verwerfen (FR-025).
2. `activeTabId` außerhalb der Tabs → erster Tab.
3. Fenster mit unbekanntem Arbeitsbereich → ersten Arbeitsbereich zuordnen.
4. Geometrie korrigieren: Größe auf ≥ `minSize` und ≤ Bereich, Position vollständig
   in den Bereich klemmen (FR-026); die Korrektur wird beim nächsten Speichern
   geschrieben, nicht sofort.
5. `stack` aus dem Rang neu vergeben; `activeWorkspaceId` prüfen (I1).
6. Kein Tab-Inhalt wird gemountet, bevor sein Fenster sichtbar ist (R8).

### Validierung (Spec → Regel)

| Regel                                                                    | Ort                      |
| ------------------------------------------------------------------------ | ------------------------ |
| Fenster-Mindestgröße je App, Klemmen in den Bereich (FR-009)             | `geometry.ts` (Frontend) |
| Nur Titelleisten-Ausschnitt 64 × 32 px muss sichtbar bleiben beim Ziehen | `geometry.ts`            |
| Singleton über alle Fenster (FR-016, FR-033)                             | `tabs.ts`                |
| IDs, Grenzen, Tab-Anzahl, `activeTabId` ∈ Tabs                           | Rust-Speicherschicht     |
