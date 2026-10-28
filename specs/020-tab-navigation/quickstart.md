# Quickstart: Navigation im Tab validieren

**Feature**: 020-tab-navigation | **Date**: 2026-09-25

Leitfaden zum Prüfen der fertigen Funktion. Datenformen und Schnittstellen stehen
in [data-model.md](./data-model.md) und [contracts/](./contracts/); hier stehen
nur Vorbereitung, Befehle und erwartete Ergebnisse.

## Voraussetzungen

- Spec 015 ist auf `main` gemerged, `020-tab-navigation` ist darauf rebased
  (research R15).
- Arbeit im Worktree `.worktrees/020-tab-navigation`, Abhängigkeiten dort regulär
  installiert (`pnpm install`, kein verlinktes `node_modules`).
- Werkzeuge aus dem Nix-Devshell; `pnpm tauri:dev` läuft darin.
- Eine Vault mit abgeschlossenem Onboarding und mindestens drei Chat-Verläufen.

## 1. Automatische Prüfungen

```sh
pnpm check:wm-navigation   # neu: Historie, Matcher, Chords, Aktionen, Store-Integration
pnpm check:wm-state        # 015-Regression, unverändert grün
pnpm check:chat-state         # Chat-Regression, unveränderte Testzahl
pnpm check:templates
pnpm typecheck && pnpm typecheck:scripts
pnpm lint && pnpm format:check
```

Erwartet: alle grün. `check:wm-navigation` deckt mindestens die Invarianten
1–7 aus [data-model.md](./data-model.md) ab, dazu den Ablauf von `runAction`
mit allen Fehlercodes, die Ablehnung jeder `guardrails`-Aktion für
`builtinAgent` und `externalAgent` bei Ausführung für `user` (SC-008), die
Pflicht zum ausdrücklichen Ziel für Agenten, `wm.system.back` in allen drei
Fällen sowie Push/Replace/No-op, die
50-Einträge-Grenze, `removeEntry` in beide Richtungen, verschachteltes Matching
mit Parametern und die Chord-Auflösung je Plattform (inkl. `yieldToTextInput`
unter macOS).

## 2. Manuelle Szenarien (`pnpm tauri:dev`)

| #   | Schritte                                                                                                                                                                                                                                             | Erwartet                                                                                                                                                 | Spec                |
| --- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------- |
| M1  | Chat öffnen, Titelleiste ansehen                                                                                                                                                                                                                     | Tab-Bereich beginnt links in der Titelleiste; Zurück/Vor-Schaltflächen sind nicht vorhanden                                                              | US1 AS1             |
| M2  | Im Chat-Verlauf nacheinander Verlauf A, B, C öffnen; zweimal Alt+←, einmal Alt+→                                                                                                                                                                     | Anzeige A → … zeigt nach 2× Alt+← A und nach Alt+→ B                                                                                                     | US1 AS2–3           |
| M3  | Nach M2 (zeigt B) Verlauf D öffnen                                                                                                                                                                                                                   | Vor deaktiviert                                                                                                                                          | US1 AS4             |
| M4  | Neue Unterhaltung, erste Nachricht senden, dann Zurück                                                                                                                                                                                               | Zurück führt zur vorigen Unterhaltung, nicht zu einer leeren neuen                                                                                       | R10                 |
| M5  | Tab-Historie mit `wm.tab.history` lesen und mit `wm.tab.go` einen Eintrag anspringen                                                                                                                                                                 | Verlauf stimmt mit den bisherigen Ansichten überein                                                                                                      | FR-016              |
| M6  | Chat und Einstellungen in zwei Fenstern nebeneinander; im Chat Historie aufbauen, Einstellungen fokussieren, Zeiger über Chat, Maustaste Zurück                                                                                                      | Chat navigiert zurück, Fokus bleibt bei Einstellungen                                                                                                    | US3 AS1             |
| M7  | Chat fokussieren, Alt+← / Alt+→ (macOS zusätzlich Cmd+[ / Cmd+])                                                                                                                                                                                     | Chat navigiert                                                                                                                                           | US3 AS2             |
| M8  | Cursor in das Chat-Eingabefeld, Text tippen, Alt+←                                                                                                                                                                                                   | Linux/Windows: Tab navigiert; macOS: Cursor springt ein Wort, Tab bleibt                                                                                 | US3 AS3             |
| M9  | Chat in ein Fenster mit einem zweiten Tab (Einstellungen) packen, Tabs wechseln und Alt+←/→ drücken                                                                                                                                                  | Tastenkürzel verwenden jeweils die Historie des aktiven Tabs                                                                                             | US2 AS2             |
| M10 | Chat-Fenster mit Historie minimieren/wiederherstellen, in anderen Arbeitsbereich verschieben                                                                                                                                                         | Historie unverändert                                                                                                                                     | US2 AS3             |
| M11 | Chat-Tab schließen und neu öffnen                                                                                                                                                                                                                    | Start-Ort, keine Historie                                                                                                                                | US2 AS4             |
| M12 | Ohne Zurück-Eintrag Zurück per Kürzel oder Maustaste                                                                                                                                                                                                 | nichts passiert, nichts schließt sich                                                                                                                    | US2 AS5, SC-005     |
| M13 | In den DevTools des obersten Dokuments `history.back()` und `history.go(-5)` ausführen                                                                                                                                                               | nichts passiert: kein Tab navigiert, die Vault bleibt offen (Abwehr ohne Deutung)                                                                        | FR-020, FR-035      |
| M14 | Im Launcher „Einstellungen“ wählen: einmal mit geschlossenen, einmal mit offenen Einstellungen (in einem anderen Arbeitsbereich)                                                                                                                     | geschlossen: neuer Tab am Start-Ort, Zurück deaktiviert; offen: vorhandener Tab wird aktiv, Arbeitsbereich wechselt, kein zusätzlicher Historien-Eintrag | US4 AS1, AS3        |
| M15 | `/settings/<instance>` und `/chat/<instance>` direkt aufrufen                                                                                                                                                                                        | Arbeitsbereich mit passendem Fenster am Start-Ort                                                                                                        | US4 AS4             |
| M16 | Einen Verlauf öffnen, anderen öffnen, den ersten löschen, Zurück                                                                                                                                                                                     | Eintrag wird übersprungen, keine leere Ansicht                                                                                                           | US6 AS3             |
| M17 | Historie aufbauen, holzi beenden, Vault neu öffnen                                                                                                                                                                                                   | Tabs am Start-Ort, Zurück deaktiviert                                                                                                                    | FR-011              |
| M18 | Anwendungsfenster unter 768 px ziehen                                                                                                                                                                                                                | Tab-Bereich bleibt sichtbar und bedienbar                                                                                                                | FR-015              |
| M19 | Nur Tastatur: Tab-Bereich und Fensteraktionen erreichen; mit Alt+←/→ navigieren                                                                                                                                                                      | Bedienelemente sind erreichbar, Navigation funktioniert                                                                                                  | FR-017, FR-022      |
| M20 | Alle Quickstart-Szenarien aus Spec 015                                                                                                                                                                                                               | weiterhin erfüllt                                                                                                                                        | SC-006              |
| M21 | In den DevTools in den Inhalt eines Tabs ein `<iframe>` mit einer lokalen Seite einfügen; darin 20× `history.pushState({}, '', '#' + i)` und danach `history.back()` bzw. `history.go(-30)` ausführen; anschließend Zurück per Kürzel oder Maustaste | keine Tab-Historie, kein aktiver Tab, kein Fenster ändert sich; holzis Zurück wirkt wie vorher                                                           | US8, FR-034, SC-009 |
| M22 | Maustaste Zurück mit dem Zeiger über dem iframe aus M21                                                                                                                                                                                              | wirkt auf den Tab (falls die Plattform sie liefert) oder höchstens auf das iframe; holzi navigiert keinen anderen Tab                                    | US8 AS3             |
| M23 | Bestandsaufnahme: jede Schaltfläche, jedes Menü, jedes Kürzel der Shell, des Chats und der Einstellungen durchgehen                                                                                                                                  | jede zustandsändernde Bedienung löst eine Aktion aus dem Katalog aus (`wm.actions.list`) oder trägt `action-exempt:`                                     | SC-007              |
| M24 | In den DevTools `performance.now()` vor dem Klick auf Zurück und nach dem Erscheinen der vorigen, bereits geladenen Unterhaltung messen                                                                                                              | höchstens 100 ms                                                                                                                                         | SC-003              |

## 3. Plattform-Spike (vor der Maus-Umsetzung)

Unter Linux (WebKitGTK), macOS (WKWebView) und Windows (WebView2) prüfen, ob die
Maus-Seitentasten `mouseup` mit `button` 3/4 im DOM erzeugen oder eine
Webview-History-Navigation auslösen. Ergebnis im PR festhalten; M6 muss in beiden
Fällen bestehen (research R6).

## 4. Nicht prüfbar in diesem Schnitt

- Die echte Android-Zurück-Geste: holzi hat noch kein Android-Target (research
  R7). Die Logik von `wm.system.back` prüft `check:wm-navigation`.
- Aufrufe durch echte Agenten: Zugang erst mit Spec 021; der Runner wird mit
  Agenten-Aufrufern in `check:wm-navigation` geprüft.
- Verschachtelte Ansichten mit Seitenleiste und der Sprung eines offenen Tabs an
  einen _anderen_ Ort (US4 AS2): brauchen eine App mit mehreren Ansichten außer
  dem Chat und kommen mit der Folge-Spec Einstellungs-App. Matcher und
  Singleton-Push sind in `check:wm-navigation` abgedeckt.
