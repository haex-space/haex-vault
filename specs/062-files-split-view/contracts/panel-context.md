# Vertrag: Rahmen ↔ Panel (intern)

Der Rahmen (`FilesApp.vue`) stellt jedem `FilesPanel` per `provide/inject` (Schlüssel
`FILES_PANEL_KEY`) folgendes bereit:

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
  setQuery(
    patch: Partial<Pick<FilesPlace, 'open' | 'q' | 't' | 's' | 'd'>>,
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
  Laufwerk; Speicher: Wurzel; anderes Gerät: Home des Geräts); dazu schreibt es mit `go(…, { replace: true })`.
- Der Viewer eines Panels rendert in `#files-viewer-host` des Rahmens (Teleport), nicht im Panel.
- Der Rahmen besitzt `useFilesSplit` (Zustand, Spiegel in den Ort, Verlaufstausch über
  `router.swapHistory`). Das Panel kennt nur diesen Kontext.
- Das Panel setzt `data-files-panel="left|right"` und `data-active` an seine Wurzel, die E2E-Selektoren
  und die Kennzeichnung des aktiven Panels benutzen.
