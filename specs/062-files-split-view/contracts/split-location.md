# Vertrag: Query-Schlüssel des Tab-Orts

Der Ort eines Dateibrowser-Tabs ist `TabLocation { path, query }` (Spec 020). Spec 044 legt fest:
`path` ist `/`, `/device`, `/storage/:id`, `/peer/:id` oder `/share/:id` (die Quelle), `query` trägt
`p`, `open`, `q`, `t`, `s`, `d`. Diese Spec ergänzt, ohne diese Schlüssel zu ändern:

| Schlüssel                  | Wert                                                      | Bedeutung                                                                                        |
| -------------------------- | --------------------------------------------------------- | ------------------------------------------------------------------------------------------------ |
| `split`                    | `1`                                                       | Split-Ansicht an. Fehlt der Schlüssel, ist sie aus. Jeder andere Wert gilt als aus.              |
| `side`                     | `r`                                                       | Das **aktive** Panel steht rechts. Fehlt der Schlüssel (oder jeder andere Wert), steht es links. |
| `o.src`                    | `device` \| `storage:<id>` \| `peer:<id>` \| `share:<id>` | Quelle des **anderen** Panels; `<id>` nach `OPAQUE_ID` (`[A-Za-z0-9-]{1,64}`).                   |
| `o.p`                      | Pfad                                                      | Ordner des anderen Panels (wie `p`).                                                             |
| `o.q`, `o.t`, `o.s`, `o.d` | wie `q`, `t`, `s`, `d`                                    | Suche und Filter des anderen Panels.                                                             |

Es gibt kein `o.open`: nur das aktive Panel navigiert und öffnet Dateien, es gibt also höchstens ein
`open`.

Regeln:

1. **Die einfachen Schlüssel (und der Pfad der Route) gehören dem aktiven Panel.** Jeder Eintrag
   eines Verlaufs ist damit eine gewöhnliche Dateibrowser-Ansicht. Ein Ort ohne neue Schlüssel ist ein
   Tab ohne Split (Rückwärtskompatibilität von Sitzungen aus Spec 022).
2. `split`, `side` und `o.*` sind der Spiegel des Rahmen-Zustands (data-model.md) im **aktuellen**
   Eintrag. Beim Mounten (Start, Sitzung) liest der Rahmen sie; danach gilt sein Zustand. Nach jedem
   Sprung im Verlauf (Zurück, Vor, Sitzung, Sprung über die Verlaufsliste) schreibt der Rahmen den
   Spiegel mit `replace` neu, wenn er vom Zustand abweicht. Einträge, die vor dem Einschalten des
   Splits entstanden, tragen die Schlüssel nicht; ein Zurück dorthin schaltet den Split nicht aus.
3. Ein Schreibvorgang auf den Ort des aktiven Panels MUSS `split`, `side` und `o.*` unverändert
   lassen. Nur `useFilesSplit` setzt sie.
4. Unbekannte Werte werden beim Lesen verworfen, nicht beim Schreiben repariert; `o.src` mit
   ungültiger Id gilt als fehlend. `split=1` ohne `o.*` ist gültig: das zweite Panel zeigt beim Einschalten
   den Ort des aktiven (FR-005).
5. Alle Werte sind Strings (JSON-fest, Spec 020 FR-001). Sitzungsdaten sind keine versionierte
   Konfiguration (Konstitution II).
6. `locationsEqual` (Spec 020 FR-006) berücksichtigt alle Schlüssel; ein unveränderter Ort erzeugt
   keinen neuen Eintrag.

Verlaufsart je Handlung im aktiven Panel: `push` bei Ordner öffnen, Quelle wählen, Datei öffnen;
`replace` bei Suche, Filter, Schließen des Viewers, Ein-/Ausschalten des Splits, Wechsel des aktiven Panels
(dabei wird der Verlauf des Tabs getauscht, kein Eintrag angehängt) und beim Zurückschreiben des Spiegels.

## Anschluss im Window Manager

Der Tab-Router (`useTabRouter`) und der Store bekommen je eine Funktion (Spec 020, Vertrag
`contracts/tab-navigation-contract.md` §2 wird um sie ergänzt):

```ts
// wmNavigation / windowManager Store
setHistory(tabId: string, history: TabHistory): boolean   // ersetzt den Verlauf; speichert die Sitzung

// useTabRouter()
swapHistory(next: TabHistory): TabHistory | null           // setzt `next`, gibt den bisherigen Verlauf zurück; `null`, wenn abgelehnt
```

Regeln: `history` MUSS die Invarianten von `TabHistory` erfüllen (1 ≤ Einträge ≤ 50, Index in
Grenzen), sonst gibt `setHistory` `false` zurück und ändert nichts; sie setzt den „letzten
Laufweg“ (`lastDirection`) und den Titel-Überschreiber des Tabs zurück; sie ändert keine andere
Tab-Verwaltung (Fenster, Arbeitsbereiche). Außerhalb eines Tabs (Test-Harness, `INERT_ROUTER`)
tut `swapHistory` nichts und gibt `null` zurück; `null` heißt immer „nichts getauscht“.
