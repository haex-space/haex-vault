# Messergebnisse: Erweiterungswerkzeuge in der Modellprüfung

Spec 018, FR-019, SC-001 bis SC-004. Satz Version 4 (`eval_set.json`), 30 erfundene Erweiterungen mit
89 Werkzeugen (`extension_tools.json`), zwölf Sätze der Art `extension`, davon zwei, die ihre Erweiterung
nennen („Vokabeltrainer“, „Gartenkalender“).

## SC-001: das passende Werkzeug im ersten Schritt angeboten (ohne Modell gemessen)

**Stand: nicht erreicht.** 10 von 12 Sätzen (83 %), verlangt sind 9 von 10.

Das passende Angebot (`extension_offer.rs`) liest nur die Nachricht und die Texte der Werkzeuge, die
Messung braucht also kein Modell und läuft als Test (`runner_tests.rs::the_matched_offer_holds_the_level_measured_so_far`).
Die Sätze entstanden vor der ersten Messung und sind nicht auf sie zugeschnitten.

| Satz                                                                  | Angeboten | Grund                                                                                                                |
| --------------------------------------------------------------------- | --------- | -------------------------------------------------------------------------------------------------------------------- |
| `extension-calendar-de-1` „Was steht heute in meinem Kalender?“       | nein      | Die Werkzeugtexte sagen „Termine“, kein Text enthält „Kalender“                                                      |
| `extension-shopping-en-1` „Please put oat flakes on my grocery list.“ | nein      | Die Werkzeugtexte sagen „shopping list“; „grocery“ kommt nicht vor, „list“ und „put“ kommen in vielen Werkzeugen vor |
| die übrigen zehn                                                      | ja        |                                                                                                                      |

Schwelle und Gewichte (`MIN_WORDS`, `MIN_SCORE`) ändern daran nichts: in beiden Fällen fehlt das Wort, nicht
das Gewicht. Ein Wortverfahren überbrückt keine Synonyme. Mögliche Wege (Entscheidung des Operators):

1. Das Ergebnis annehmen und SC-001 bei „9 von 10“ lassen: die Zahl hängt an der Güte der Beispielsätze in
   den Manifesten (ein Werkzeug „Termine anzeigen“ ohne ein Beispiel mit „Kalender“ trifft die Frage nach dem
   Kalender nicht).
2. Mehr Beispielsätze je Werkzeug in die Fixture schreiben. Das misst dann die Fixture, nicht das Verfahren,
   solange die Sätze nicht unabhängig davon entstehen.
3. Einbettungen (Recherche R6, Alternative b): trifft Synonyme, braucht aber ein weiteres Modell je Gerät.

## SC-002, SC-003: Aufruf des Werkzeugs und kein `run_command` (Modell nötig)

**Noch nicht gemessen.** Das braucht einen Lauf mit Qwen3-4B (`cargo test --manifest-path src-tauri/Cargo.toml --test model_tool_eval -- --ignored --nocapture`,
Aufbau mit CUDA wie in Spec 046) und trägt `called` und `runCommandCalls` des Berichts in diese Datei ein.
Der Lauf ist als Test vorbereitet (`model_tool_eval.rs` übergibt die Fixture); die Bewertung selbst ist mit einem
Orakel und einem Modell, das nur `run_command` ruft, getestet.

## SC-004: Aufruf ohne offenes Fenster

**Noch nicht gemessen.** Braucht die laufende App mit der Test-Erweiterung (E2E oder von Hand).

## Beobachtung: Größe des Angebots

Mit `run_command` (das jede Chat-Antwort anbietet) hat der erste Schritt bis zu 12 Werkzeuge im Kern (9 Aktionen,
`find_actions`, `ask_user`, `run_command`) plus bis zu 3 des passenden Angebots (15), nach einer Suche bis zu
17 plus 3 (20). Recherche R6 rechnet mit 14 und 19, ohne `run_command`. Der Lauf bildet das ab (der Test
`a_found_tool_is_offered_in_the_second_step_with_the_search_result` prüft 17).
