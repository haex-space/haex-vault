# Quickstart: ADR-Korpus bereinigen

Diese Schritte prüfen die Dokumentationsänderung nach der Umsetzung. Sie führen keine Produkt- oder Laufzeittests aus.

1. Das Inventar in `research.md` gegen alle Dateien unter `docs/adr/` abgleichen: jede ADR-Datei kommt genau einmal vor.
2. Jede `remove`- oder `shorten`-Bewertung mit der benannten Spec öffnen und Entscheidung, Gründe und Folgen abgleichen.
3. Die Links auf entfernte ADR-Pfade in `specs/`, `plans/`, `docs/` und `README.md` suchen; keine Fundstelle darf auf eine nicht mehr vorhandene Datei zeigen.
4. HXEF-Vertrag und Vektoren unter `docs/formats/` sowie ihre Spec-Links auf Vorhandensein prüfen.
5. `git diff --check` ausführen und die Liste der geänderten bzw. entfernten Dokumente prüfen.

Erwartetes Ergebnis: jede verbleibende Entscheidung ist auffindbar, jede Entfernung ist im Inventar begründet, und es gibt keine Links auf fehlende ADR-Dateien.
