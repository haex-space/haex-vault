# Seitenerkennung: Auftrag der Erweiterung

Der frühere Vertrag mit den Operationen `prepare` und `recognize` aus Spec 047
ist abgelöst. haex-notes ruft den aktuellen Modellauftragsvertrag über
`models.runAsync` auf; die Host-Brücke und die Einwilligungsregeln stehen in
[Spec 055](../../055-extension-model-access/contracts/bridge.md), das
Auftrags- und Ergebnisformat in
[job-format.md](../../055-extension-model-access/contracts/job-format.md).

haex-notes besitzt dabei die Anweisung und das erwartete Ergebnisformat. Für
die Seitenerkennung beschreibt es zwei Schritte (`page-recognition` zum Lesen
und `text-fix` zur Korrektur), wie im Beispiel von Spec 055. Holzi wählt das
freigegebene Modell, prüft die Ausgabe gegen das von der Erweiterung gelieferte
Schema und meldet das Ergebnis über `haextension:model-job:completed`.

Die Erweiterung bleibt für Bildauswahl, lokale Vorverarbeitung, Warnungen,
Original, Canvas-Abbildung und Export zuständig. Zugangsdaten, lokale Pfade
und persönliche Handschriftprofile gehören nicht in den Auftrag. Ein Fehler
oder Abbruch darf das importierte Original nicht verändern; ungültige oder
unbrauchbare Modellantworten fallen auf manuelle Bearbeitung zurück.
