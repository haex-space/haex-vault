# Feature Specification: Structured Agent Tasks for Extensions

**Feature Branch**: `feat/structured-agent-tasks`
**Created**: 2026-10-09
**Status**: Abgelöst durch Spec 055 (2026-10-20). Das feste Format `paper-note-recognition` geht im Auftragsformat „Seitenerkennung“ auf, das die Erweiterung selbst beschreibt; Start, Abbruch, Ergebnisprüfung und das Ereignis bei Abschluss sind dort verallgemeinert (`extension_model_job_*`, `haextension:model-job:completed`), die Brückenmethoden `extension_ai_task_*` gibt es nicht mehr. Freigaben, Einwilligung und Anbieter regelt Spec 055. Die übrigen Dateien dieses Verzeichnisses sind historische Entwürfe und keine aktuellen Implementierungs- oder Integrationsanleitungen.
**Input**: haex-notes soll fotografierte Papiernotizen über den Holzi-Agenten aufbereiten können. Der Nutzer soll in Holzi Modell und Harness wählen können; ein lokales Vision-/OCR-Modell soll möglich sein.

## User Scenarios & Testing

### User Story 1 - Eine Erweiterung fordert eine strukturierte KI-Aufgabe an (Priority: P1)

Eine vertrauenswürdige Erweiterung übergibt Holzi ein Bild, eine benannte Aufgabe und ein erwartetes Ergebnisformat. Holzi verarbeitet die Aufgabe mit dem für diesen Zweck konfigurierten Agent-Profil und liefert ein maschinenlesbares Ergebnis zurück.

**Why this priority**: Ohne eine stabile Host-Schnittstelle müsste jede Erweiterung Provider, Zugangsdaten und Modellformate selbst implementieren.

**Independent Test**: Eine Test-Erweiterung sendet ein Bild an eine Fixture-Aufgabe und erhält ein schema-konformes Ergebnis, ohne einen externen Dienst aufzurufen.

### User Story 2 - Der Nutzer wählt Modell, Harness und Verarbeitungsort (Priority: P1)

Der Nutzer kann für strukturierte Aufgaben ein Profil auswählen. Das Profil kann ein lokales Vision-/OCR-Modell, einen anderen lokalen Harness oder einen ausdrücklich erlaubten externen Dienst verwenden. Die Erweiterung erfährt, welches Profil und welcher Verarbeitungsort tatsächlich verwendet wurden.

**Why this priority**: Die Modellfreiheit und der lokale Betrieb sind zentrale Anforderungen des Papiernotiz-Use-Cases.

**Independent Test**: Mit zwei Profilen liefert derselbe Fixture-Task dieselbe Ergebnisform, aber unterschiedliche Provenienz; ein nicht bildfähiges Textmodell wird verständlich abgelehnt.

### User Story 3 - Private Bilder bleiben unter Kontrolle des Nutzers (Priority: P1)

Vor einer Verarbeitung außerhalb des Geräts sieht der Nutzer, dass Bilddaten übertragen werden und wohin. Lokale Verarbeitung darf die Bilddaten nicht an einen externen Dienst weiterleiten. Ein Fehler darf das Original in der Erweiterung nicht unbrauchbar machen.

**Why this priority**: Schülernotizen können personenbezogene oder schulische Inhalte enthalten.

**Independent Test**: Eine Remote-Ausführung ohne Zustimmung startet nicht; eine lokale Ausführung erzeugt keine externe Anfrage; ein Timeout liefert einen kontrollierten Fehler.

### Edge Cases

- Das gewählte Modell unterstützt keine Bilder oder kein strukturiertes Ergebnisformat. Die Aufgabe wird vor der Bildübertragung abgelehnt.
- Das Modell liefert ungültiges JSON, fehlende Pflichtfelder oder unzulässige Werte. Holzi verwirft das Ergebnis und liefert einen validierbaren Fehler.
- Die Verarbeitung dauert länger oder wird abgebrochen. Die Erweiterung erhält einen eindeutigen Status und kann das Original weiterverwenden.
- Das Profil ist nicht verfügbar, etwa weil das lokale Modell nicht installiert ist. Holzi bietet keinen stillen Wechsel zu einem externen Dienst an.
- Ein Agent-Task versucht, nicht freigegebene Werkzeuge oder Dateipfade zu verwenden. Der Task wird ohne Ausführung dieser Zugriffe beendet.

## Requirements

### Functional Requirements

- **FR-001**: Holzi MUST eine hostseitige Schnittstelle für benannte, strukturierte Agent-Tasks bereitstellen, die von einer berechtigten aktiven Erweiterung aufgerufen werden kann.
- **FR-002**: Ein Task MUST binäre Bilddaten oder eine vom Host autorisierte lokale Bildreferenz entgegennehmen können; die Erweiterung darf keinen unkontrollierten lokalen Dateipfad an ein Modell übergeben müssen.
- **FR-003**: Jeder Task MUST ein versioniertes Eingabe- und Ausgabeschema besitzen. Freier Modelltext darf nicht als erfolgreiches Task-Ergebnis gelten.
- **FR-004**: Das ausgewählte Profil MUST Harness, Modell, Verarbeitungsort und erforderliche Fähigkeiten abbilden. Für Bildaufgaben MUSS Bild-/OCR-Verarbeitung unterstützt werden.
- **FR-005**: Ein reines Textmodell MUSS für einen Bildtask vor der Verarbeitung abgelehnt werden. Holzi darf diese Prüfung nicht allein dem Prompt überlassen.
- **FR-006**: Lokale Profile MUST Bilddaten lokal verarbeiten. Remote-Profile MUST vor dem ersten Versand eine sichtbare Zustimmung und ein Ziel anzeigen.
- **FR-007**: Das Ergebnis MUST Provenienz enthalten: Task-Version, Profilkennung, Modellkennung, Harnesskennung, Verarbeitungsmodus und gegebenenfalls Remote-Ziel.
- **FR-008**: Holzi MUST ungültige Ergebnisse, Zeitüberschreitungen, Abbrüche und nicht verfügbare Profile als strukturierte Fehler melden.
- **FR-009**: Ein Task MUST standardmäßig keine allgemeinen Agent-Werkzeuge, Shell-Zugriffe oder beliebigen Dateizugriff erhalten. Zusätzliche Fähigkeiten müssen taskbezogen und explizit freigegeben sein.
- **FR-010**: Eine Erweiterung MUST einen laufenden Task abbrechen können. Ein Abbruch darf keine bereits gespeicherten lokalen Originaldaten verändern.
- **FR-011**: Zugangsdaten dürfen weder in Task-Anfragen, Ergebnissen, Erweiterungsspeicher noch synchronisierten Dokumentdaten landen.
- **FR-012**: Der Host MUST lokale Aufgaben auch dann weiter ausführen können, wenn kein Relay, Cloud-Dienst oder externer Agent verfügbar ist.

## Key Entities

- **Agent-Task**: Versionierte, benannte Operation mit Fähigkeitsanforderungen, Eingabe- und Ausgabeschema.
- **Agent-Profil**: Nutzerkonfiguration für Harness, Modell, Ort, Freigaben und optionale Fallback-Regeln.
- **Task-Ausführung**: Kurzlebiger Lauf mit Status, Abbruchmöglichkeit, Ergebnis oder strukturiertem Fehler.
- **Task-Provenienz**: Nicht-geheime Kennungen der tatsächlich verwendeten Task-, Profil-, Modell- und Harness-Konfiguration.
- **Fähigkeitsbeschreibung**: Aussage, ob ein Modell Bildinput, OCR, strukturierte Ausgabe und gegebenenfalls Layout-/Bounding-Box-Ausgabe unterstützt.

## Out of Scope for the First Release

- Allgemeine Erweiterungs-Chat-API mit dauerhaftem Gesprächsverlauf.
- Freie autonome Agenten mit beliebigen Werkzeugen innerhalb einer Erweiterung.
- Persönliche Handschriftprofile oder Training mit Schülerdaten.
- Automatischer Wechsel von einem lokalen zu einem Remote-Profil ohne ausdrückliche Zustimmung.

## Success Criteria

- **SC-001**: Eine Fixture-Erweiterung erhält bei 100 % der gültigen Testanfragen ein Ergebnis, das gegen das registrierte Ausgabeschema validiert.
- **SC-002**: 100 % der Bildtasks mit einem nicht bildfähigen Modell werden vor einer Modellanfrage abgelehnt.
- **SC-003**: Bei lokalen Profilen verlässt in einem instrumentierten Test kein Bildbyte den lokalen Host.
- **SC-004**: Bei Remote-Profilen startet ohne Zustimmung keine Übertragung; die Anzeige nennt Verarbeitungsmodus und Ziel.
- **SC-005**: Ein abgebrochener oder fehlgeschlagener Task lässt das von der Erweiterung gespeicherte Original in 100 % der Fehlerfalltests unverändert zugänglich.
- **SC-006**: Nutzer können Modell und Harness für den Papiernotiz-Task wechseln, ohne haex-notes neu zu bauen oder seine Datenmigration zu ändern.

## Assumptions

- Holzi besitzt bereits eine gemeinsame Provider-/Modellverwaltung für lokale und externe Modelle; diese Feature ergänzt einen strukturierten Task-Eingang und erweitert die Fähigkeitsbeschreibung.
- Das erste konkrete Ziel ist haex-notes mit dem Task `paper-note-recognition`.
- Für den ersten Vision-Test wird ein lokales Vision-/OCR-Modell verwendet, nicht das vorhandene reine Text-Qwen-Profil.
- Erweiterungen erhalten keine Providerzugangsdaten und speichern keine geheimen Profilinformationen.
