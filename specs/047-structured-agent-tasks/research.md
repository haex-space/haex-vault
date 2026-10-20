# Research: Structured Agent Tasks for Extensions

> **Abgelöst:** Diese Recherche dokumentiert Entscheidungen des verworfenen
> Spec-047-Entwurfs. Aktuelle Modellaufträge, Freigaben und Einwilligungen
> stehen in [Spec 055](../055-extension-model-access/).

**Stand**: 2026-10-09

## R1 — Asynchroner Extension-Task statt blockierender Bridge-Antwort

- **Decision**: Der Bridge-Aufruf startet einen kurzlebigen Task und liefert eine `taskId`. Das Ergebnis kommt über ein Event an die ursprüngliche Extension.
- **Rationale**: Lokale Vision-Modelle können deutlich länger als ein normaler Bridge-Aufruf laufen. Ein synchrones Warten würde den Bridge-Worker und den Extension-Frame unnötig koppeln.
- **Alternatives considered**: Synchroner Aufruf bis zum Ergebnis (verworfen: Timeout- und UI-Risiko); Polling durch die Extension (verworfen für den ersten Vertrag: mehr Zustand und zusätzliche Angriffsfläche als ein gezieltes Event).

## R2 — Wiederverwendung der vorhandenen Provider-/Chat-Abstraktion

- **Decision**: Der Task-Runner baut auf der vorhandenen `ChatState`-/`ProviderAdapter`-Kette auf, statt einen zweiten Provider-Stack einzuführen.
- **Rationale**: Modellwahl, Provider-Secrets und lokale/remote Adapter existieren bereits im Host. Der Task ergänzt nur Capability-Prüfung, einen begrenzten Request und Schema-Validierung.
- **Alternatives considered**: Direkter Provider-Aufruf aus haex-notes (verworfen: Anbieterbindung und Secrets in der Extension); neuer separater Inference-Service (verworfen: zusätzlicher Prozess und Installationsaufwand).

## R3 — Kein freier Agent-Tool-Loop im Task

- **Decision**: Ein strukturierter Task erhält genau den deklarierten Input, einen task-spezifischen Systemprompt und keine allgemeinen Werkzeuge. Das Resultat wird als JSON validiert.
- **Rationale**: Papiernotizen müssen nachvollziehbar in Regionen überführt werden. Freie Agent-Antworten oder Dateizugriff würden die Datenform und die Datenschutzgrenze unkontrollierbar machen.
- **Alternatives considered**: haex-notes sendet eine normale Chatnachricht (verworfen: freie Antwort und Verlauf statt Canvas-Vertrag); MCP-Tool für OCR (verworfen: zusätzliche Server-/Berechtigungsverwaltung).

## R4 — Modellfähigkeiten als harte Schranke

- **Decision**: Bildtasks verlangen mindestens Bildinput, OCR/Visionsverarbeitung und strukturierte Ausgabe. Fehlende oder unbekannte Fähigkeiten führen zu einer Ablehnung vor der Inferenz.
- **Rationale**: Ein textbasiertes Qwen3-Modell kann ein Bild nicht zuverlässig verarbeiten. Capability-Flags sind sicherer als ein Prompt, der ein Modell zu nicht vorhandenen Fähigkeiten auffordert.
- **Alternatives considered**: Capability aus Modellnamen ableiten (verworfen: unzuverlässig bei Imports); stiller Remote-Fallback (verworfen: verletzt Locality und Nutzererwartung).

## R5 — Task-Profil über bestehende Modellverwaltung

- **Decision**: Ein Profil referenziert vorhandene Provider-/Modellkennungen und den Harness-Typ; die Auswahl wird als Preference pro Task gespeichert. Das Profil enthält keine Zugangsdaten.
- **Rationale**: Nutzer können lokale Vision-Modelle, API-Modelle oder spätere Spezial-Harnesses wählen, ohne haex-notes neu zu bauen. Die eigentliche Credential-Verwaltung bleibt im Host.
- **Alternatives considered**: Provider-Konfiguration in der Extension (verworfen: Sicherheits- und Bindungsproblem); nur globales Chatmodell ohne Task-Profil (verworfen: OCR braucht häufig ein anderes Modell als der normale Chat).

## R6 — Lokales Vision-Modell bleibt eigener Folgebaustein

- **Decision**: Dieser Host-Task-Vertrag akzeptiert nur Modelle, die Bildinput und strukturierte Ausgabe tatsächlich unterstützen. Die Integration eines konkreten lokalen Vision-Backends, etwa eines Qwen-VL-GGUF oder eines spezialisierten OCR-Harnesses, bleibt ein nachgelagerter Provider-/Modellbaustein.
- **Rationale**: Der aktuelle lokale Chatpfad ist auf textbasierte GGUF-Nachrichten ausgerichtet. Ein Task-Vertrag darf diese Einschränkung nicht verschleiern oder den vorhandenen Textadapter fälschlich als Visionmodell ausgeben.
- **Alternatives considered**: Qwen3-Textprofil als OCR-Modell markieren (verworfen: falsche Capability); Bild in Prompttext umwandeln (verworfen: keine echte Bildwahrnehmung).
