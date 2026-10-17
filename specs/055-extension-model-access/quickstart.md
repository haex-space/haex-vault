# Quickstart: Erweiterungen nutzen holzis Modelle

Handtest nach den Lieferungen A und B (und C, wenn ein lokales Bildmodell vorhanden ist). Schritte 1–12 laufen
mit `pnpm tauri:dev` und der Test-Erweiterung `model-jobs.xt` (`src-tauri/tests/fixtures/extension_e2e/model-jobs/`).

Voraussetzung: ein Anbieter mit Bildmodell, zum Beispiel ein lokaler Ollama auf `localhost` oder ein Testserver mit
OpenAI-kompatibler Schnittstelle, und ein zweiter Anbieter in der Cloud oder im Netz.

1. **Anbieter eintragen**: Einstellungen → Anbieter → „OpenAI-kompatibel“: Adresse, Schlüssel. Die Modelle
   erscheinen. Bei einem Bildmodell „nimmt Bilder an“ setzen. _(FR-018, FR-019, SC-007)_
2. **Erweiterung installieren** und öffnen. Sie stellt ohne Freigabe einen Auftrag. _(FR-002, SC-001)_
   - Erwartet: holzi fragt nach der Freigabe eines Modells (Auswahl und höchster Aufwand), Antwort „für die
     Sitzung“ oder „dauerhaft“ (FR-004).
3. **Freigeben** und wiederholen: Das Ergebnis kommt mit Herkunft (Modell, Aufwand, Ort). _(FR-001, FR-023)_
4. **Aufwand begrenzen**: Freigabe auf „mittel“, die Erweiterung verlangt „hoch“. Die Herkunft nennt „mittel“.
   _(FR-006)_
5. **Ziel außerhalb des Geräts**: Ein Auftrag mit Bild an das Cloud-Modell fragt zuerst nach der Einwilligung mit
   Ziel und Ort; „ablehnen“ sendet nichts. _(FR-015, SC-004)_
6. **Sitzung gegen dauerhaft**: Nach „für die Sitzung“ fragt der nächste Auftrag nicht; nach dem Neustart von holzi
   wieder. Nach „dauerhaft“ nicht, bis der Nutzer in den Einstellungen widerruft. _(FR-016)_
7. **Rangfolge und Ausweichweg**: Zwei Modelle freigeben, das bevorzugte absichtlich unbrauchbar machen (Testmodus
   der Test-Erweiterung: Schleife). Das zweite liefert; ohne Einwilligung für dessen Ziel endet der Auftrag mit
   einem Fehler und die Herkunft nennt den ersten Versuch. _(FR-027, FR-017)_
8. **Kaputte Ausgabe**: Leere Ausgabe, Schleife, chinesische Zeichen und Schemaverstoß liefern jeweils einen
   Fehler `Unusable`, nie ein Ergebnis. _(FR-010, FR-011, SC-005)_
9. **Mehrere Schritte**: Der Auftrag „lesen, dann korrigieren“ liefert beide Ergebnisse; fehlt für den zweiten Schritt
   ein Modell, scheitert der Start vor dem ersten Schritt. _(FR-009, FR-025)_
10. **Grenzen und Abbruch**: Ein Bild über 25 MiB und ein neuntes Bild werden vor der Verarbeitung abgelehnt (Name
    der Grenze); ein Auftrag über der Höchstdauer bricht ab; „Abbrechen“ ändert keine Daten der Erweiterung.
    _(FR-029, FR-022, SC-012)_
11. **Nichts gespeichert**: Nach den Schritten 3–10 zeigt der Vergleich der Tabelleninhalte (Prüfskript der
    Integrationstests) nur Berechtigungen und Einwilligungen als Änderung. _(FR-028, SC-011)_
12. **Modellliste**: Die Erweiterung ruft `extension_model_list`: nur freigegebene Modelle, mit Name, Fähigkeiten und
    Ort, ohne Adresse oder Schlüssel. _(FR-030)_

Nur mit Lieferung C: **Lokal mit Bild** — ein lokales Bildmodell lesen lassen, bei laufendem Netzwerkmitschnitt
darf kein Bildbyte das Gerät verlassen (SC-003); ein reines Textmodell lehnt denselben Auftrag vor dem Senden ab
(SC-006).

Übersprungene Schritte kommen in die Beschreibung des PR.
