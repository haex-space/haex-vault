# Vertrag: Rahmen ↔ Panel (intern)

Der Rahmen (`FilesApp.vue`) stellt per `provide/inject` (Schlüssel `FILES_PANEL_KEY`) eine Funktion
`(side) => FilesPanelContext` bereit; ein `FilesPanel` holt seinen Kontext mit `useFilesPanel(side)`
(`composables/useFilesSplit.ts`). Der Kontext je Seite ist nötig, weil `side` und `active` je Panel
verschieden sind:

```ts
type FilesPanelContext = {
  side: 'left' | 'right'
  /** Ort dieses Panels, aus dem Tab-Ort gelesen (Quelle, Pfad, open, q, t, s, d). */
  place: ComputedRef<FilesPlace>
  /** Das Panel ist das aktive. */
  active: ComputedRef<boolean>
  /** Es gibt zwei sichtbare Panels. */
  split: ComputedRef<boolean>
  /** Navigiert dieses Panel (push) bzw. ändert nur Query-Teile (replace). Ist das Panel nicht aktiv,
   *  wird es zuerst aktiv (Verlaufstausch). Mischt `split`, `side` und `o.*` ein
   *  (contracts/split-location.md Regel 3). */
  go(place: FilesPlace, options?: { replace?: boolean }): void
  /** Setzt den Startort dieses Panels ohne Aktivierung oder Änderung der Panel-Historien. */
  initialize(place: FilesPlace): void
  /** `null` entfernt den Schlüssel (`open`, `q`, `t`, `s`, `d`). */
  setQuery(
    patch: Partial<Record<'open' | 'q' | 't' | 's' | 'd', string | null>>,
    options?: { push?: boolean },
  ): void
  /** Meldet Ziel und Neuladen dieses Panels an; der Rahmen gibt sie dem Gegenpanel. */
  register(handle: PanelHandle): () => void
  /** Das Gegenpanel (oder `null` ohne Split). */
  other(): PanelHandle | null
  /** Macht dieses Panel aktiv (Klick, Fokus). */
  activate(): void
  /** Fokus ins Gegenpanel (Tab); macht es aktiv (Verlaufstausch). */
  switchPanel(): void
  /** Die Daten des Rahmens: Quellen, Aufräumen. */
  sources: Ref<Sources | null>
  reloadSources(): void
  tabId: string
  /** Der Tab ist der vorderste seines Fensters (ein Speicher lädt dann neu). */
  tabActive: ComputedRef<boolean>
  openStorageSettings(): void
  /** Elemente des Rahmens, in die ein Panel Pfadleiste und Viewer teleportiert; `null`, bis der
   *  Rahmen eingehängt ist. Elemente statt IDs, denn mehrere Dateibrowser-Tabs sind zugleich
   *  eingehängt. */
  pathHost: Readonly<Ref<HTMLElement | null>>
  viewerHost: Readonly<Ref<HTMLElement | null>>
}
```

Pflichten des Panels:

- Alle Orts-Schreibzugriffe gehen über `go` und `setQuery`; das Panel ruft `useTabRouter()` nicht mehr.
  Zurück/Vor in der Werkzeugleiste des Panels entfallen: sie sind die Schaltflächen des Window Managers,
  die auf den Verlauf des aktiven Panels wirken.
- Nur das aktive Panel nimmt nach dem Öffnen eines Ordners selbständig den Fokus in die Liste
  (`FolderView takesFocus = context.active`); ein inaktives Panel nimmt nie programmatisch Fokus.
- Das Panel meldet sich mit `register` an und entfernt sich beim Unmount.
- Ein Panel beginnt ohne Pfad (`path === null`) am Startordner wie bisher (Gerät: Home oder erstes
  Laufwerk; Speicher: Wurzel; anderes Gerät: Home des Geräts). Die Initialisierung setzt den Ort
  dieses Panels mit dem rahmeninternen, nicht aktivierenden `initialize(place)` per `replace`; sie
  darf `active`, Seitenhistorie und die Aktivierung des Panels nicht ändern. `go` behält seine
  aktivierende Semantik für Benutzernavigation.
- Der Viewer eines Panels rendert in `viewerHost` des Rahmens (Teleport), nicht im Panel. Mit einem
  Panel steht die Pfadleiste in der Werkzeugleiste des Rahmens (`pathHost`, Teleport); mit zwei Panels
  im Kopf des jeweiligen Panels.
- Tastenereignisse außerhalb beider Panels (Werkzeugleiste, Seitenleiste) gibt der Rahmen an das aktive
  Panel weiter (`PanelHandle.keydown`); Ereignisse aus einem Panel behandelt dessen Wurzel, der Rahmen
  überspringt sie (`closest('[data-files-panel]')`).
- Der Rahmen besitzt `useFilesSplit` (Zustand, Spiegel in den Ort, Verlaufstausch über
  `router.swapHistory`). Das Panel kennt nur diesen Kontext.
- Das Panel setzt `data-files-panel="left|right"` und `data-active` an seine Wurzel, die E2E-Selektoren
  und die Kennzeichnung des aktiven Panels benutzen.
