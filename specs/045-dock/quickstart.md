# Quickstart: Dock prüfen

Voraussetzung: App gestartet (`pnpm tauri:dev`), neue Vault angelegt und entsperrt.

1. **Standard (US1, FR-003)**: Unten mittig steht eine Leiste mit Launcher, Dateibrowser, Chat,
   Passwörter, Einstellungen. Unten rechts schweben keine Schaltflächen mehr.
2. **Anheften (US1)**: Launcher öffnen, Rechtsklick auf „Adressbuch“ → „An Dock anheften“. Der Eintrag
   steht am Ende der Leiste. Klick öffnet das Adressbuch.
3. **Fokussieren (US2)**: Zweiten Arbeitsbereich anlegen, dorthin wechseln. Klick auf „Passwörter“ im
   Dock wechselt zurück und fokussiert die Instanz; es entsteht kein zweites Fenster.
4. **Mehrere Instanzen (US2)**: Chat zweimal öffnen (Launcher). Der Chat-Eintrag zeigt an seiner
   angehefteten Stelle den Zähler 2. Klick zeigt das Auswahlfeld, nach Arbeitsbereich
   gruppiert, mit „Neues Fenster“. Mittelklick öffnet eine dritte Instanz.
5. **Alle schließen (US2, FR-016)**: Chat-Tab in ein Fenster mit einem Passwörter-Tab ziehen bzw. per „+“
   ergänzen; Kontextmenü „Alle schließen“ am Chat-Eintrag schließt nur die Chat-Tabs.
6. **Platzierung (US3)**: Einstellungen → Allgemein → Dock. Alle Kanten und Ausrichtungen durchschalten.
   Ein Fenster maximieren: „Platz reservieren“ → Fenster endet am Dock; „Schweben“ → Dock liegt darüber;
   „Automatisch ausblenden“ → Dock erscheint nur an der Kante.
7. **Sortieren (US4)**: Einträge in der Leiste ziehen; in den Einstellungen „Fenster“ über
   „Hinzufügen“ ergänzen und wieder entfernen. Beim Launcher fehlt „Entfernen“.
8. **Rad (US5)**: Stil „Rad“, unten rechts. Klick fächert einen Viertelkreis auf; an „unten, Mitte“ einen
   Halbkreis. Fünf weitere Apps anheften → zweiter Bogen, nichts überlappt. Escape klappt zu. Mit Tab und
   Enter auffächern, Pfeiltasten wandern, Enter aktiviert.
9. **Kompaktmodus (US6)**: Leiste links, Fenster schmaler als 768 px ziehen → Leiste unten. Zurück → links.
   Fenster auf ca. 800 px: Leiste links bleibt, kein Springen (FR-033). Rad oben links → im Kompaktmodus
   unten links.
10. **Sync (FR-035/036)**: Zweites Gerät koppeln. Angeheftete App erscheint dort; die Platzierung des
    zweiten Geräts bleibt.
11. **Nicht verfügbare Erweiterung (FR-037)**: Erweiterung anheften, deaktivieren → Eintrag weg, in den
    Einstellungen „nicht verfügbar“. Wieder aktivieren → Eintrag an alter Stelle.

Automatisch:

```sh
pnpm check:wm-state      # inkl. check-wm-dock.ts
pnpm check:settings
pnpm typecheck && pnpm lint && pnpm format:check
pnpm test:e2e --grep dock   # nur Arch (nix-Host-Bridge)
```
