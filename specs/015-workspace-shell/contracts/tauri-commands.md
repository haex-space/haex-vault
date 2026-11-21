# Contract: Tauri commands (Shell-Layout)

**Feature**: 015-workspace-shell | **Date**: 2026-09-21

Wire-Konventionen wie in `preferences_commands` und `instances/*`: Argumente
kommen als ein Objekt `args` (`invoke('shell_…', { args })`), Felder sind
`camelCase`, Rückgabetypen sind ts-rs-Bindings unter `src/types/bindings/`
(`#[ts(export, export_to = "../../src/types/bindings/")]`). Kein Command nimmt eine
Geräte-UUID entgegen — das Backend löst das aktuelle Gerät selbst auf
(research R4). Fehlt ein geöffneter Vault: `HaexError::NoActiveInstance`.
Verstöße gegen Invarianten und Grenzen: `HaexError::InvalidInput { reason }`.
Rückgaben enthalten nie lokalisierte Texte.

## Datentypen

```ts
// src/types/bindings/ (generiert)
type WorkspaceDto = { workspaceId: string; position: number } // kein Name: Anzeige „Arbeitsbereich N“ aus position
type TabDto = { tabId: string; appId: string }
type WindowDto = {
  windowId: string
  workspaceId: string
  x: number
  y: number
  width: number
  height: number // Normalgeometrie
  isMinimized: boolean
  isMaximized: boolean
  stackOrder: number
  activeTabId: string
  tabs: TabDto[] // Reihenfolge = Leistenreihenfolge
}
type ShellLayoutDto = {
  workspaces: WorkspaceDto[] // nach position
  windows: WindowDto[] // nach stackOrder aufsteigend
  activeWorkspaceId: string
}
```

Grenzen (Details: data-model.md): `appId` 1–128
Zeichen; Größe 1–100 000, Position ±100 000; 1–100 Tabs je Fenster; ≤ 500 Fenster
je Gerät.

## Commands

### `shell_load_layout`

- **Args**: keine.
- **Returns**: `ShellLayoutDto` (das gesamte Layout des aktuellen Geräts).
- **Verhalten**: Legt bei Bedarf einen Standard-Arbeitsbereich an;
  korrigiert einen fehlenden oder ungültigen `shell.active_workspace_id` auf den
  ersten Arbeitsbereich und schreibt ihn; Fenster mit unbekanntem
  `workspaceId` oder ohne Tab liefert es **nicht** aus (Bereinigung ist Sache
  von `hydrate`, das Backend liefert, was gültig gespeichert ist).
- **Erfüllt**: FR-018, FR-023, FR-024.

### `shell_create_workspace`

- **Args**: keine.
- **Returns**: `WorkspaceDto` (hinten angefügt, `position` = bisherige Anzahl)
- **Verhalten**: Setzt den aktiven Arbeitsbereich **nicht**; das Frontend ruft
  danach `shell_set_active_workspace` (Wechsel zum neuen, FR-019).

### `shell_delete_workspace`

- **Args**: `{ workspaceId: string }`
- **Returns**: `{ workspaces: WorkspaceDto[], activeWorkspaceId: string }`
- **Verhalten**: In einer Transaktion: Tabs und Fenster des Arbeitsbereichs
  löschen, Arbeitsbereich löschen, Positionen verdichten. War er aktiv, wird sein
  vorheriger Nachbar (sonst der nächste) aktiv und in
  `shell.active_workspace_id` geschrieben. Der letzte Arbeitsbereich ist nicht
  löschbar → `InvalidInput` (I3). Bestätigung und Guards (FR-014/FR-021) laufen
  **vorher** im Frontend.

### `shell_set_active_workspace`

- **Args**: `{ workspaceId: string }`
- **Returns**: `void`
- **Verhalten**: Schreibt `shell.active_workspace_id` (Device-Scope) über die
  bestehende `preferences`-Speicherschicht; unbekannte ID → `InvalidInput`.

### `shell_save_windows`

- **Args**: `{ windows: WindowDto[] }`
- **Returns**: `void`
- **Verhalten**: Alles-oder-nichts in einer Transaktion. Je Fenster: Zeile
  einfügen oder aktualisieren; die Tab-Menge des Fensters wird durch `tabs`
  **ersetzt** (fehlende Tabs gelöscht, neue eingefügt, `position` = Index).
  Prüft I4/I5 und die Grenzen; jeder Verstoß verwirft den ganzen Aufruf.
  Deckt Öffnen, Verschieben, Größenändern, Minimieren, Maximieren, Stapelreihenfolge,
  Tab-Öffnen/-Wechsel/-Schließen und Verschieben zwischen Arbeitsbereichen ab.
- **Erfüllt**: FR-023, FR-039.

### `shell_close_windows`

- **Args**: `{ windowIds: string[] }`
- **Returns**: `void`
- **Verhalten**: Löscht die Fenster samt Tabs. Unbekannte IDs werden ignoriert
  (idempotent).

## Aufrufreihenfolge (Vertrag an das Frontend)

1. Es ist **höchstens ein** Shell-Aufruf gleichzeitig in Flug; das Frontend
   reiht sie in eine Schreib-Queue (sonst könnte ein verspätetes
   `shell_save_windows` ein Fenster nach `shell_close_windows` wiederbeleben).
2. Strukturelle Änderungen (Fenster/Tab öffnen und schließen, Verschieben
   zwischen Arbeitsbereichen, Arbeitsbereichs-Aktionen) gehen **sofort** in die
   Queue; Geometrie- und Stapeländerungen werden mit 400 ms Debounce als ein
   `shell_save_windows` gebündelt (research R13).
3. Vor `close_instance` (Sperren/Schließen der Vault) wartet das Frontend die
   Queue ab (`flushAsync`, FR-027).
4. Schlägt ein Aufruf fehl, bleibt der Zustand im Speicher, die betroffenen
   Fenster gelten als schmutzig und gehen im nächsten Aufruf erneut mit.

## Fehlerabbildung

| Situation                                                | Fehler                                      |
| -------------------------------------------------------- | ------------------------------------------- |
| Kein Vault geöffnet                                      | `NoActiveInstance`                          |
| Unbekannte `workspaceId`                                 | `InvalidInput`                              |
| Letzten Arbeitsbereich löschen                           | `InvalidInput`                              |
| Fenster ohne Tab, `activeTabId` ∉ Tabs, > 100 Tabs       | `InvalidInput`                              |
| Doppelte `tabId` im Aufruf oder in einem anderen Fenster | `InvalidInput`                              |
| Geometrie/`appId` außerhalb der Grenzen                  | `InvalidInput`                              |
| Datenbankfehler                                          | bestehende Abbildung über `HaexError::from` |

## Nicht Teil dieses Vertrags

- Tab-Inhalte, Entwürfe, Titel-Überschreibungen und Aufmerksamkeitsflags werden
  nie an das Backend gesendet.
- Bekannte App-Arten prüft das Backend nicht (`appId` ist opak, research R5/R9).
