# Quickstart: Werkzeuge von Erweiterungen prüfen

Voraussetzung: HaexVault aus diesem Branch (`pnpm tauri:dev:cuda` oder `tauri:dev`), lokales Modell Qwen3-4B,
Test-Erweiterung `src-tauri/tests/fixtures/extension_e2e/agent-tools.xt`.

1. **Installieren (US2)**: Test-Erweiterung installieren → der Dialog listet unter den Berechtigungen die
   Werkzeuge „Einträge zählen“ (lesend), „Eintrag hinzufügen“ (ändernd), „Langsames Echo“ (lesend) mit
   Wirkungsart. Alle angehakt lassen, bestätigen.
2. **Frage ohne Namen (US1, SC-001)**: Modus „Auto“, neuer Chat: „Wie viele Einträge habe ich?“ → der Agent
   ruft „Einträge zählen“ auf, ohne Freigabe, und nennt die Zahl. Im Verlauf: Test-Erweiterung, Werkzeugtitel,
   Eingabe, Ergebnis.
3. **Kein Fenster offen (US1/3, SC-004)**: Alle Fenster der Test-Erweiterung schließen, Frage aus Schritt 2
   wiederholen → ein minimiertes Fenster entsteht, der Fokus bleibt im Chat, Antwort kommt.
4. **Ändernd in „Plan“ (032 FR-007)**: Modus „Plan“: „Füge einen Eintrag Milch hinzu“ → Aufruf wird ohne
   Rückfrage abgelehnt, der Agent sagt das.
5. **Manuell (US3/1)**: Modus „Manuell“, Frage aus Schritt 2 → Zustimmungsabfrage nennt „Test-Erweiterung“,
   „Einträge zählen“ und die Eingabe; „Ablehnen“ → nichts läuft.
6. **Nicht angehakt (R4)**: In den Einstellungen „Einträge zählen“ auf „Fragen“ stellen → im Modus „Auto“
   kommt eine Zustimmungsabfrage.
7. **Schalter (US3/3, SC-006)**: Einstellungen → Erweiterungen → Test-Erweiterung → „Für den Agenten
   verfügbar“ aus → Frage aus Schritt 2 → kein Werkzeug der Test-Erweiterung im Angebot; die App öffnet
   weiter normal.
8. **Abbruch (Edge)**: „Gib ein langsames Echo von hallo“ → während des Aufrufs Stopp → der Aufruf endet,
   die Test-Erweiterung meldet den Abbruch in ihrem Log.
9. **Host-Funktion durch den Agenten (US3/4)**: „Eintrag hinzufügen“ fordert eine nicht erteilte
   Berechtigung → die Abfrage sagt „ausgelöst durch einen Aufruf des Agenten“.
10. **Update (US2/2)**: Version mit zusätzlichem Werkzeug installieren → der Dialog fragt nur danach.
11. **Entwicklungsversion (US4)**: Test-Erweiterung im Entwicklermodus laden → Aufruf im Verlauf als
    Entwicklungsversion gekennzeichnet.
12. **Messung (FR-019)**: Die Modellprüfung mit Qwen3-4B laufen lassen → Bericht mit `offered`/`called` je
    Satz der Art `extension`; SC-001 bis SC-003 in `eval-results.md` eintragen.

    ```sh
    cargo test --manifest-path src-tauri/Cargo.toml --test model_tool_eval -- --ignored --nocapture
    ```
