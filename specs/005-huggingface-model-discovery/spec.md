# Feature Specification: Freie Hugging-Face-Modellsuche und Installation

**Feature Branch**: `005-huggingface-model-discovery`
**Created**: 2026-09-14
**Status**: Draft
**Input**: Nutzer möchte neben den kuratierten Qwen3-Vorschlägen beliebige kompatible Modelle von Hugging Face suchen, herunterladen und lokal verwenden können.

## Clarifications

### Session 2026-09-14

- Die kuratierten lokalen Vorschläge verwenden Qwen3. Qwen2.5 ist kein vorgeschlagenes Standardmodell mehr. Bestehende Qwen2.5-Beispiele in älteren Dokumenten sind entsprechend zu aktualisieren.
- „Beliebige Hugging-Face-Modelle“ bedeutet im ersten Schnitt jedes öffentlich zugängliche, vom lokalen Runtime-Adapter unterstützte GGUF-Modell. Safetensors-, PyTorch- oder andere nicht ladbare Artefakte werden nicht als installierbare Modelle ausgegeben.
- Der bestehende kuratierte Katalog bleibt erhalten und ist eine Empfehlungsschicht. Die freie Suche ergänzt ihn und ersetzt ihn nicht.
- Hugging-Face-Anmeldung, private Repositories und gated Modelle sind im ersten Schnitt nicht Bestandteil. Ein späterer Authentifizierungsweg darf den anonymen Such- und Downloadpfad nicht voraussetzen.
- Q: Wie wird die lokale Modell-ID für frei installierte Hugging-Face-Modelle gebildet? → A: Deterministisch aus Hugging-Face-Repository-ID und Dateiname abgeleitet. HaexVault verwendet dafür `hf-` plus den kleingeschriebenen Hex-SHA-256-Digest einer versionierten, längenpräfixierten Kodierung beider Werte. Die Kodierung nutzt den festen Präfix `haex-hf-model-id-v1`, je ein Big-Endian-`u32`-Byte-Längenpräfix und die UTF-8-Bytes von Repository-ID und Dateiname. Identische Quellen bleiben idempotent und unterschiedliche Paare erhalten kollisionsresistente IDs. Die ID ist nicht nutzer-editierbar. Der Anzeigename bleibt ein separates, frei editierbares Feld.
- Q: Woher stammen Quantisierung und Kontextfenster für Anzeige und Hardware-Fit-Prüfung bei frei gewählten HF-Dateien? → A: Zuerst aus Hugging-Face-Repository-Metadaten (README/config.json/Model-Card-Tags), dann Dateinamen-Heuristik als Fallback; ein GGUF-Header-Read per Range-Request erfolgt nur, wenn beides fehlt und eine sicherheitsrelevante Fit-Entscheidung (`TooBig`-Grenze) davon abhängt.
- Q: Wo im UI ist die freie Hugging-Face-Suche erreichbar? → A: Über einen dauerhaft erreichbaren „Modelle verwalten"-Bereich (unabhängig vom Installationsstatus), der kuratierten Katalog, freie Hugging-Face-Suche und installierte Modelle vereint. Dieser Bereich muss auch nach Abschluss des Onboardings jederzeit erreichbar bleiben, damit der Nutzer das aktive Modell zu jedem Zeitpunkt wechseln kann — nicht nur solange keine Modelle installiert sind.
- Q: Wie verhält sich die freie Suche, wenn ein Treffer exakt Repository-ID und Datei eines kuratierten Katalogeintrags entspricht? → A: Der Treffer bleibt in der freien Suche sichtbar, wird aber deutlich als „bereits im Katalog verfügbar" markiert und verlinkt zum entsprechenden Katalogeintrag. Es findet keine automatische Zusammenführung der Modell-ID mit dem Katalogeintrag statt.
- Q: Soll die Hugging-Face-Revision beim Download auf eine konkrete Commit-SHA festgeschrieben werden? → A: Ja. Der ausgewählte Branch-/Tag-Stand wird vor dem Download in eine konkrete Commit-SHA aufgelöst und für den Download sowie die spätere Reproduzierbarkeit persistiert.
- Zusätzliche Klarstellung: Der dauerhaft erreichbare „Modelle verwalten"-Bereich (siehe oben) MUSS auch nach Abschluss des Onboardings das Entfernen installierter Modelle mit expliziter Bestätigung ermöglichen. Dabei werden ausschließlich die lokalen Modellbytes und der Installations-/Download-Datensatz gelöscht; synchronisierte/nicht-lokale Source-/Provider-Metadaten, andere lokale Modelle sowie die bestehende Fallback-/Last-Active-Semantik aus Spec 002 bleiben konsistent.
- Zusätzliche Klarstellung: Für installierte öffentliche Hugging-Face-Modelle soll die Modellverwaltung den gespeicherten Upstream-Branch bzw. Tag regelmäßig bzw. auf ausdrückliche Aktualisierung prüfen. Bei einer abweichenden neueren Commit-SHA wird der Nutzer sichtbar informiert und erhält eine Aktion zur Installation dieser Revision; ein automatisches Ersetzen findet nicht statt.
- Zusätzliche Klarstellung: Vor jedem normalen Laden eines lokalen Modells muss HaexVault den SHA-256-Hash der tatsächlich gefundenen Datei berechnen und gegen den gespeicherten Geräte-Hash prüfen. Bei einer Abweichung oder fehlender Integritätsbasis darf der normale Ladepfad das Modell nicht laden. Ein separater, ausdrücklich bestätigter Override darf die vorgefundene Datei als `untrusted` laden, ohne den gespeicherten Hash zu ändern.
- Zusätzliche Klarstellung: Die freie Hugging-Face-Suche bietet Filter für Quantisierung, maximale GGUF-Dateigröße und Hardware-Passung. Dafür werden Dateidetails einschließlich Größe und Fit nach der Repository-Suche nachgeladen; ein gefilterter Treffer darf im anschließenden Datei-Picker nur noch passende Dateien anbieten. Dateien mit unbekannter Größe erfüllen einen gesetzten Größenhöchstwert nicht.
- Zusätzliche Klarstellung: Beim Öffnen der freien Hugging-Face-Suche wird ohne Suchbegriff automatisch eine Top-10-Liste der meistgeladenen öffentlichen GGUF-Repositorys angezeigt. Die Liste ist nach Downloads absteigend sortiert; eine explizite Suchanfrage bleibt auf höchstens 20 Treffer begrenzt.
- Zusätzliche Klarstellung: Ein fehlgeschlagener lokaler Integritätsvergleich setzt den gespeicherten Status nach Möglichkeit auf `untrusted`, ohne den erwarteten `fileSha256` zu verändern. Die ausdrücklich gewählte HF-Quellenreparatur muss den identischen Source-Short-Circuit umgehen und die gespeicherte Revision erneut laden. Ein GGUF-Header-Range-Request akzeptiert ausschließlich `206 Partial Content` und liest höchstens die angeforderte Byte-Grenze; ein vom Server ignorierter Range-Request wird abgelehnt.
- Zusätzliche Klarstellung: Ein vom Nutzer vorgegebenes Kontextfenster wird als `unknown` provenance gekennzeichnet, da es weder aus Hugging-Face-Metadaten noch aus dem GGUF-Header stammt.
- Zusätzliche Klarstellung: Ein laufender Download darf einen abgebrochenen Response-Body innerhalb desselben Vorgangs aus einer temporären `.part`-Datei fortsetzen. Ein Byte-Resume ist nur zulässig, wenn die vorherige Antwort einen `ETag` oder `Last-Modified`-Validator geliefert hat und der Retry diesen als `If-Range` mitsendet. Ohne Validator muss der Download bei Byte 0 neu beginnen. Ein `206 Partial Content` darf nur bei gültigem, zum angeforderten Offset passendem `Content-Range` angehängt werden; bei fehlenden oder widersprüchlichen Bereichsmetadaten muss der Teilstand verworfen und neu begonnen oder der Download strukturiert abgebrochen werden. Eine temporäre Datei darf erst nach exakt vollständiger, angekündigter Byte-Anzahl als GGUF veröffentlicht werden. Diese HTTP-Header sind flüchtige Transportmetadaten und werden nicht persistiert.

## Context

HaexVault nutzt für lokale Vorschläge bereits Qwen3-Profile: Qwen3 4B für Desktops, Qwen3 1.7B für mobile Geräte und Qwen3 0.6B als Low-Memory-Fallback. Diese Auswahl ist bewusst kuratiert und hardwarebezogen.

Nutzer sollen darüber hinaus selbst ein passendes Modell auswählen können, ohne dass HaexVault für jede neue Modellfamilie einen Katalogeintrag benötigt. Heute existiert bereits ein Download-Contract für ein bekanntes Hugging-Face-Repository und eine bekannte Datei (`download_model_from_hf`), außerdem existieren lokale Installation, Import und Modellauflistung. Der Katalog wird bisher nur innerhalb der Chat-Ansicht angezeigt, solange kein Modell installiert ist; es gibt noch keinen dauerhaft erreichbaren Verwaltungsbereich. Dieses Feature führt einen dauerhaft erreichbaren „Modelle verwalten"-Bereich ein, der Discovery, eine verständliche Auswahl-UI sowie den jederzeitigen Wechsel des aktiven Modells bündelt — unabhängig vom Installationsstatus und auch nach Abschluss des Onboardings — und nutzt die bestehende Installations-/Laufzeitkette.

## User Scenarios & Testing

### User Story 1 - Hugging Face durchsuchen (Priority: P1)

Ein Nutzer öffnet in der Modellverwaltung eine freie Suche, gibt beispielsweise `llama gguf` oder eine Repository-ID ein und erhält passende öffentliche Hugging-Face-Repositories bzw. GGUF-Dateien.

**Why this priority**: Ohne Discovery muss der Nutzer Repository- und Dateinamen außerhalb von HaexVault zusammensuchen. Die freie Modellauswahl ist der Kern des Features.

**Independent Test**: Mit einem deterministischen Mock des Hugging-Face-Clients suchen und prüfen, dass nur installierbare Treffer angezeigt werden, leere Suche validiert wird und ein Netzwerkfehler als verständlicher Fehlerzustand erscheint.

**Acceptance Scenarios**:

1. **Given** die Modellverwaltung ist geöffnet, **When** der Nutzer einen Suchbegriff mit mindestens zwei Zeichen eingibt und die Suche ausführt, **Then** werden öffentliche Treffer mit Repository-ID, Modellname, Autor, Lizenz soweit vorhanden und verfügbaren GGUF-Dateien angezeigt.
2. **Given** ein Repository enthält keine GGUF-Datei, **When** die Ergebnisse geladen werden, **Then** wird es nicht als installierbarer Modelltreffer angezeigt.
3. **Given** der Nutzer gibt weniger als zwei Zeichen oder nur Whitespace ein, **When** er die Suche startet, **Then** wird keine Netzwerkanfrage ausgeführt und ein lokalisierter Validierungshinweis angezeigt.
4. **Given** die Hugging-Face-Anfrage schlägt fehl oder läuft in ein Timeout, **When** die Suche abgeschlossen wird, **Then** bleibt die bisherige Ergebnisliste erhalten und der Nutzer kann erneut suchen.
5. **Given** der Nutzer hat das Onboarding abgeschlossen und bereits mindestens ein Modell installiert, **When** er den „Modelle verwalten"-Bereich erneut öffnet, **Then** stehen kuratierter Katalog, freie Suche, installierte Modelle und der Wechsel des aktiven Modells weiterhin zur Verfügung.
6. **Given** ein Treffer der freien Suche entspricht exakt Repository-ID und Dateiname eines kuratierten Katalogeintrags, **When** die Ergebnisse angezeigt werden, **Then** bleibt der Treffer sichtbar, wird als „bereits im Katalog verfügbar" markiert und verlinkt auf den entsprechenden Katalogeintrag.
7. **Given** die Suche liefert GGUF-Dateien mit unterschiedlichen Größen, Quantisierungen oder Hardware-Fits, **When** der Nutzer Filter setzt, **Then** werden nur Repositories mit mindestens einer passenden Datei angezeigt und der Datei-Picker bietet aus diesem Treffer nur passende Dateien an.
8. **Given** der Nutzer öffnet die freie Hugging-Face-Suche ohne Suchbegriff, **When** die initiale Ansicht geladen wird, **Then** werden bis zu zehn der meistgeladenen öffentlichen GGUF-Repositorys angezeigt und können direkt gefiltert bzw. geöffnet werden.

### User Story 2 - Kompatible Datei auswählen und herunterladen (Priority: P1)

Ein Nutzer wählt aus einem Treffer eine konkrete GGUF-Datei, sieht die relevanten Metadaten und startet den Download. Nach erfolgreichem Abschluss kann er das Modell unmittelbar im Chat auswählen.

**Why this priority**: Repository-Suche allein bringt keinen nutzbaren Mehrwert; der Nutzer muss sicher eine konkrete, ladbare Datei installieren können.

**Independent Test**: Mit einem Mock-Repository und einem kleinen Testartefakt den Dateiauswahl-, Fortschritts-, atomaren Veröffentlichungs- und Registrierungsfluss ausführen und anschließend `list_installed_models` sowie das Laden des Modells prüfen.

**Acceptance Scenarios**:

1. **Given** ein Treffer enthält mehrere GGUF-Quantisierungen, **When** der Nutzer den Treffer öffnet, **Then** sieht er jede verfügbare Datei separat mit Dateiname, Größe und soweit ermittelbarer Quantisierung und muss eine Datei auswählen.
2. **Given** eine GGUF-Datei ist ausgewählt, **When** der Nutzer den Download startet, **Then** zeigt HaexVault laufenden Fortschritt, die Datei wird zunächst temporär geschrieben und erst nach vollständigem Erfolg als installiert sichtbar.
3. **Given** der Download ist erfolgreich, **When** der Download abgeschlossen wird, **Then** wird das Modell in der bestehenden lokalen Modellverwaltung registriert, erscheint in der Modellliste und kann im Chat geladen werden.
4. **Given** der Nutzer startet denselben Download erneut, **When** das Modell bereits mit derselben Datei installiert ist, **Then** wird kein zweiter unvollständiger Eintrag erzeugt und der Zustand wird idempotent behandelt.
5. **Given** Download, Registrierung oder Modellprüfung schlägt fehl, **When** der Fehler angezeigt wird, **Then** bleiben bestehende installierte Modelle nutzbar und ein unvollständiger Download wird nicht als installiert angeboten.
6. **Given** der Server kündigt zehn Bytes an, beendet den Body aber sauber nach vier Bytes, **When** der Download den Body erneut anfordert, **Then** wird der vier Byte große Teilstand niemals als vollständiges GGUF veröffentlicht; HaexVault setzt mit einem sicheren Resume fort oder startet bei Byte 0 neu und veröffentlicht erst nach exakt zehn Bytes.
7. **Given** ein Teilstand enthält bereits Bytes, aber die vorherige Antwort hatte weder `ETag` noch `Last-Modified`, **When** HaexVault den Download wiederholt, **Then** sendet HaexVault keinen `Range`-Request mit ungesichertem Offset, sondern verwirft den Teilstand und startet bei Byte 0.
8. **Given** ein Retry liefert `206 Partial Content`, aber `Content-Range` fehlt, beginnt am falschen Offset, widerspricht dem bekannten Gesamtumfang oder passt nicht zu `Content-Length`, **When** HaexVault die Antwort verarbeitet, **Then** hängt HaexVault die Bytes nicht an und veröffentlicht keine verkürzte oder aus verschiedenen Ständen zusammengesetzte Datei; der Download startet neu oder liefert einen strukturierten Fehler.
9. **Given** der Server liefert nach einem Resume-Versuch wegen eines geänderten Validators wieder `200 OK`, **When** HaexVault die Antwort verarbeitet, **Then** verwirft HaexVault den alten Teilstand und verarbeitet den vollständigen Body als neue Basis.

### User Story 3 - Modell-Metadaten und Runtime-Kompatibilität (Priority: P1)

Ein Nutzer erhält vor dem Download genug Informationen, um Größe, Lizenz und Hardware-Risiko einzuschätzen. HaexVault verhindert, dass nicht unterstützte oder offensichtlich nicht passende Dateien still installiert werden.

**Why this priority**: Frei auffindbare Modelle unterscheiden sich stark bei Format, Größe, Kontextfenster und Lizenz. Fehlende Grenzen würden zu Fehlbedienung, langen Fehl-Downloads und unklaren Runtime-Fehlern führen.

**Independent Test**: Fixtures mit gültigen GGUF-Dateien, nicht-GGUF-Dateien, fehlenden Metadaten, sehr großen Dateien und inkompatiblen Repository-Antworten gegen den Parser und die Fit-/Validierungsentscheidung testen.

**Acceptance Scenarios**:

1. **Given** eine Datei ist kein GGUF oder ihr Download-Metadatum weist nicht auf ein unterstütztes Artefakt hin, **When** sie für die Installation angeboten werden soll, **Then** wird sie ausgefiltert oder als nicht installierbar markiert.
2. **Given** Größe oder Kontextfenster überschreiten die bekannte Hardware-Fit-Grenze, **When** der Nutzer die Datei auswählt, **Then** zeigt die UI eine deutliche Warnung und verlangt eine explizite Bestätigung vor dem Download.
3. **Given** Lizenzinformationen fehlen, **When** der Treffer angezeigt wird, **Then** wird „Lizenz nicht angegeben“ angezeigt; HaexVault behauptet keine Lizenzkonformität.
4. **Given** das Tokenizer-Repository kann nicht automatisch aus den Hugging-Face-Metadaten bestimmt werden, **When** der Nutzer den Download startet, **Then** muss er ein Tokenizer-Repository angeben oder den Vorgang abbrechen; ein späterer Modell-Load darf nicht erst an einem fehlenden stillschweigenden Wert scheitern.

### User Story 4 - Installierte eigene Modelle verwalten (Priority: P2)

Ein Nutzer erkennt im dauerhaft erreichbaren „Modelle verwalten"-Bereich, auch nach Abschluss des Onboardings, welche Modelle aus dem freien Hugging-Face-Pfad installiert sind, kann sie wie katalogbasierte Modelle auswählen und jederzeit mit Bestätigung lokal wieder entfernen.

**Why this priority**: Freie Modelle müssen sich im Alltag wie bestehende lokale Modelle verhalten; sonst entsteht eine zweite, inkonsistente Modellwelt.

**Independent Test**: Ein frei installiertes Modell und ein Katalogmodell gemeinsam auflisten, beide laden und anschließend nur das freie Modell entfernen.

**Acceptance Scenarios**:

1. **Given** ein frei installiertes GGUF ist vorhanden, **When** der Nutzer den „Modelle verwalten"-Bereich öffnet, **Then** wird es mit Name, Größe und Quelle als lokal verfügbar angezeigt.
2. **Given** ein eigenes Modell wird entfernt, **When** der Nutzer die Entfernung bestätigt, **Then** wird nur die lokale Datei gelöscht; synchronisierte Provider-/Modellmetadaten und andere lokale Modelle bleiben erhalten.
3. **Given** das zuletzt aktive Modell wird entfernt, **When** der Chat erneut geöffnet wird, **Then** greift die bestehende Fallback-Kette aus Spec 002, ohne die persistierte Erinnerung stumm zu überschreiben.
4. **Given** der Nutzer hat das Onboarding abgeschlossen, **When** er im „Modelle verwalten"-Bereich ein installiertes Modell entfernen möchte, **Then** kann er dies jederzeit tun, ohne das Onboarding erneut zu durchlaufen; die Entfernung verlangt eine explizite Bestätigung.
5. **Given** ein installiertes HF-Modell verfolgt einen öffentlichen Branch oder Tag und Hugging Face liefert dafür eine andere Commit-SHA, **When** die Modellverwaltung den Upstream-Stand prüft, **Then** wird der Nutzer mit aktueller und neuer SHA informiert und kann die neue Revision über den bestehenden atomaren Installationspfad installieren.
6. **Given** die Update-Prüfung ist offline, schlägt fehl oder das Repository ist nicht mehr verfügbar, **When** die Modellverwaltung aktualisiert wird, **Then** bleiben das installierte Modell und sein letzter bekannter Status nutzbar; es wird kein Update fälschlich als erfolgreich installiert markiert.
7. **Given** ein lokales Modell ist ausgewählt, **When** HaexVault es laden soll, **Then** wird der SHA-256-Hash der kanonischen lokalen Datei vor dem Runtime-Load berechnet und nur bei Übereinstimmung mit dem gespeicherten Geräte-Hash geladen.
8. **Given** die lokale Datei wurde ausgetauscht, korrumpiert, fehlt oder besitzt noch keine bestätigte Integritätsbasis, **When** der Nutzer das Modell laden möchte, **Then** wird der Ladevorgang mit einem sichtbaren Integritätsfehler abgebrochen und kein Modellruntime gestartet.
9. **Given** der normale Ladepfad meldet einen fehlenden oder abweichenden Hash, **When** der Nutzer den ausdrücklich bestätigten Integritäts-Override auswählt, **Then** lädt `load_model_with_integrity_override` die vorgefundene Datei als `untrusted`, ohne den gespeicherten `file_sha256` zu ändern.

## Edge Cases

- Die Hugging-Face-Suche liefert viele oder doppelte Treffer: Ergebnisse werden begrenzt, dedupliziert und deterministisch sortiert; die konkrete Grenze wird im Plan festgelegt.
- Ein Repository enthält mehrere GGUF-Dateien mit unterschiedlichen Chat-Templates oder Tokenizern: Jede Datei bleibt ein eigener auswählbarer Treffer; ein nicht sicher bestimmbarer Tokenizer wird nicht geraten.
- Ein Repository oder Dateiname enthält Pfadtrenner, `..`, Steuerzeichen oder unerwartete Unicode-Namen: Die Eingabe wird als Hugging-Face-ID bzw. Dateiname validiert und darf niemals den lokalen Modellpfad verlassen.
- Ein Download wird abgebrochen, die App beendet oder die Verbindung unterbrochen: temporäre Dateien bleiben unsichtbar für `list_installed_models`; ein laufender Retry darf nur mit einem passenden Entity-Validator fortsetzen, andernfalls muss er bei Byte 0 neu beginnen. Kein Resume- oder Neustartpfad darf einen beschädigten finalen GGUF erzeugen.
- Hugging Face antwortet mit Redirect, fehlender `Content-Length`, ungültigem `Content-Range` oder HTTP-Fehler: Redirects werden nur innerhalb des Download-Clients verfolgt, unbekannter Fortschritt bleibt zulässig, ein Resume ohne vollständige Bereichsmetadaten wird verworfen oder neu gestartet, und Fehler werden strukturiert gemeldet.
- Derselbe Repository-/Dateiname-Kandidat wird mehrfach oder aus unterschiedlichen Suchergebnissen heraus installiert: Da die Modell-ID deterministisch aus Repository-ID und Dateiname abgeleitet wird, führt dies immer zum selben Slug; die bestehende Publikationssperre und Idempotenzprüfung (US2 AC4) verhindern einen zweiten, widersprüchlichen Eintrag.
- Ein Nutzer öffnet die Modellverwaltung offline: Katalogeinträge und bereits installierte Modelle bleiben sichtbar; freie Suche und neue Downloads zeigen einen retrybaren Offline-Fehler.
- Ein öffentliches Modell wird später gelöscht oder ersetzt: Bereits installierte Dateien bleiben lokal nutzbar; eine erneute Suche bzw. ein erneuter Download kann fehlschlagen.
- Die beim Download angegebene HF-Revision ist ein Branch oder Tag: Vor dem Download wird sie in eine Commit-SHA aufgelöst. Kann der Stand nicht eindeutig aufgelöst werden, wird der Download abgelehnt und kein lokaler Eintrag erzeugt.
- Ein installierter HF-Branch oder Tag zeigt auf eine neue Commit-SHA: Die Modellverwaltung zeigt einen Update-Hinweis mit alter und neuer SHA. Der Nutzer entscheidet selbst, ob die neue Revision installiert wird; die bestehende Datei bleibt bis zum erfolgreichen atomaren Austausch erhalten.
- Ein HF-Modell wurde direkt auf eine Commit-SHA gepinnt: Ohne zugehörigen Branch/Tag wird keine automatische Update-Erwartung behauptet; die Modellquelle bleibt trotzdem reproduzierbar ladbar.
- Eine lokale Modell-Datei wurde nach der Installation verändert: Der SHA-256-Vergleich schlägt vor dem Runtime-Load fehl; die Datei wird weder automatisch überschrieben noch still als neue Referenz akzeptiert.
- Ein historisch vorhandenes Modell besitzt noch keinen gespeicherten Hash: Es wird als nicht verifiziert angezeigt und löst denselben Integritätsdialog aus. Der Nutzer kann es unsicher laden, die Quelle reparieren (HF erneut herunterladen bzw. lokal neu importieren) oder ein anderes Modell auswählen.
- Der Nutzer entfernt ein installiertes Modell nach Abschluss des Onboardings über den dauerhaften „Modelle verwalten"-Bereich: Nur die lokalen Modellbytes und der Installations-/Download-Datensatz werden mit Bestätigung gelöscht; Source-/Provider-Metadaten, andere lokale Modelle sowie die Fallback-/Last-Active-Kette aus Spec 002 bleiben konsistent, auch wenn das entfernte Modell zuvor das aktive Modell war. Der fehlende lokale Pfad wird nicht gelistet, beim Laden als `ModelNotFound` behandelt und kann später erneut installiert werden.

## Requirements

### Functional Requirements

- **FR-001**: Das System MUSS neben dem kuratierten Qwen3-Katalog eine freie Suche nach öffentlichen Hugging-Face-Repositories und deren Dateien anbieten.
- **FR-002**: Die freie Suche MUSS Suchanfragen validieren, ein begrenztes Ergebnis liefern und Ergebnisse deterministisch sortieren.
- **FR-003**: Das System MUSS installierbare GGUF-Dateien anhand der Hugging-Face-Metadaten erkennen und nicht unterstützte Artefakte aus dem Installationspfad ausschließen.
- **FR-004**: Ein Suchtreffer MUSS mindestens Repository-ID, Anzeigename, Autor, Lizenzstatus, Datei, Dateigröße soweit vorhanden und verfügbare Runtime-/Tokenizer-Metadaten enthalten.
- **FR-005**: Der Nutzer MUSS eine konkrete GGUF-Datei auswählen können, wenn ein Repository mehrere installierbare Dateien enthält.
- **FR-006**: Der Download MUSS den bestehenden `download_model_from_hf`-Pfad und die bestehenden Fortschritts-/Completion-Events erweitern oder wiederverwenden; eine parallele Modellregistrierung ist nicht zulässig.
- **FR-007**: Downloads MÜSSEN atomar veröffentlicht werden. Unvollständige, abgebrochene oder fehlerhafte Dateien DÜRFEN nicht als installiertes Modell erscheinen.
- **FR-008**: Ein erfolgreich installiertes eigenes Modell MUSS über `list_installed_models`, den bestehenden Modell-Picker und `load_model` nutzbar sein.
- **FR-009**: Das System MUSS die lokale Modell-ID für frei installierte Modelle deterministisch aus Repository-ID und Dateiname ableiten. Dafür MUSS es `hf-` plus den kleingeschriebenen Hex-SHA-256-Digest einer versionierten, längenpräfixierten Kodierung des Paars verwenden. Die Kodierung MUSS den festen Präfix `haex-hf-model-id-v1`, je ein Big-Endian-`u32`-Byte-Längenpräfix und die UTF-8-Bytes von Repository-ID und Dateiname verwenden. Identische Paare MÜSSEN dieselbe ID ergeben; unterschiedliche Paare müssen kollisionsresistent getrennt werden. Die Modell-ID ist für den Nutzer nicht editierbar, ein separater Anzeigename bleibt frei änderbar. Modell-ID, Repository-ID, Dateiname und Tokenizer-Repository MÜSSEN so persistiert werden, dass ein späterer Load ohne erneute Suche möglich ist. Ein vorhandener ID-Eintrag mit abweichendem Quellenpaar oder abweichender finaler GGUF bleibt ein Konflikt.
- **FR-010**: Das System MUSS lokale Modellpfade aus validierten IDs und Dateinamen erzeugen; Suchresultate DÜRFEN keine Pfadtraversal- oder beliebige Dateischreiboperation ermöglichen.
- **FR-011**: Vor dem Download MUSS die UI Größe, Quantisierung, Kontextfenster und Lizenzstatus anzeigen. Quantisierung und Kontextfenster werden zuerst aus Hugging-Face-Repository-Metadaten (README/config.json/Model-Card-Tags), dann per Dateinamen-Heuristik ermittelt; sind beide erfolglos, gelten sie als nicht ermittelbar, außer eine sicherheitsrelevante `TooBig`-Fit-Entscheidung erfordert einen zusätzlichen GGUF-Header-Read per Range-Request.
- **FR-012**: Bei einem `Tight`- oder `TooBig`-Hardware-Fit MUSS eine sichtbare Warnung erscheinen; `TooBig` darf nur nach expliziter Bestätigung heruntergeladen werden.
- **FR-013**: Wenn kein verlässliches Tokenizer-Repository vorliegt, MUSS das System den Nutzer vor dem Download danach fragen oder den Download ablehnen; es darf keinen unbrauchbaren lokalen Eintrag erzeugen.
- **FR-014**: Netzwerk-, HTTP-, Timeout-, Abbruch-, Speicher- und Validierungsfehler MÜSSEN als lokalisierbare strukturierte Fehler an die UI gelangen; Backend-Commands dürfen keine lokalisierten UI-Texte voraussetzen.
- **FR-015**: Offline- und Suchfehler DÜRFEN vorhandene Katalog-, Provider- und Installationsdaten nicht löschen oder unbrauchbar machen.
- **FR-016**: Die kuratierten Qwen3-Profile MÜSSEN unverändert als separate Empfehlungen verfügbar bleiben; Qwen2.5 darf nicht als neuer Standard oder Vorschlag eingeführt werden.
- **FR-017**: Die bestehende Modellwahl-Persistenz und Fallback-Kette aus Spec 002 MUSS auch für frei installierte Modelle gelten.
- **FR-018**: Alle sichtbaren Texte dieses Features MÜSSEN über die bestehende i18n-Grenze in Deutsch und Englisch gepflegt werden.
- **FR-019**: Das System MUSS einen dauerhaft erreichbaren „Modelle verwalten"-Bereich bereitstellen, der kuratierten Katalog, freie Hugging-Face-Suche, installierte Modelle und den Wechsel des aktiven Modells vereint. Dieser Bereich MUSS unabhängig vom Installationsstatus und auch nach Abschluss des Onboardings jederzeit erreichbar sein, nicht nur in einer Leeransicht ohne installierte Modelle.
- **FR-020**: Entspricht ein Treffer der freien Suche exakt Repository-ID und Dateiname eines kuratierten Katalogeintrags, MUSS die UI ihn weiterhin in den Suchergebnissen anzeigen, deutlich als „bereits im Katalog verfügbar" kennzeichnen und zum entsprechenden Katalogeintrag verlinken. Eine automatische Zusammenführung der Modell-ID mit dem Katalogeintrag ist nicht vorgesehen.
- **FR-021**: Der „Modelle verwalten"-Bereich (FR-019) MUSS das Entfernen eines installierten Modells mit expliziter Bestätigung ermöglichen, jederzeit auch nach Abschluss des Onboardings. Die Entfernung MUSS ausschließlich die lokalen Modellbytes und den Installations-/Download-Datensatz löschen; synchronisierte Provider-/Modellmetadaten, andere lokale Modelle sowie die bestehende Fallback-/Last-Active-Semantik aus Spec 002 (FR-017) MÜSSEN dabei konsistent bleiben.
- **FR-022**: Vor jedem freien HF-Download MUSS eine angeforderte Branch-, Tag- oder sonstige mutable Revision auf eine konkrete Commit-SHA aufgelöst werden. Der Download MUSS ausschließlich diese aufgelöste SHA verwenden und sie zusammen mit dem optional verfolgten Upstream-Ref persistieren; eine nicht auflösbare Revision MUSS den Download ohne persistierten Eintrag abbrechen.
- **FR-023**: Die Modellverwaltung MUSS für installierte HF-Modelle mit gespeichertem Upstream-Ref den aktuellen Commit-Stand abfragen können und eine abweichende neuere SHA sichtbar melden. Die Prüfung darf bestehende lokale Modelle und Metadaten bei Netzwerk- oder Hub-Fehlern nicht verändern.
- **FR-024**: Bei verfügbarem Update MUSS der Nutzer die neue Commit-SHA ausdrücklich zur Installation auswählen können. Die Aktualisierung MUSS über denselben atomaren Download-/Registrierungspfad erfolgen, die Modell-ID und aktive Auswahl erhalten und die bisherige Datei bis zum erfolgreichen Austausch nutzbar lassen. Automatische Updates sind ausgeschlossen.
- **FR-025**: Vor jedem normalen Laden eines lokalen Modells MUSS das System den SHA-256-Hash der kanonisch aufgelösten lokalen Datei in einem blockierenden Arbeitsschritt berechnen und mit dem gespeicherten `file_sha256` vergleichen. Der normale Runtime-Load darf nur bei exakter Übereinstimmung beginnen; ein bewusst bestätigter Unsicherheits-Override ist ein separater Pfad.
- **FR-026**: Bei fehlender Datei, fehlendem gespeicherten Hash, Hash-Abweichung oder Hash-/Dateisystemfehler MUSS der normale Ladepfad mit einem strukturierten, lokalisierbaren Integritätsfehler abgebrochen werden. Der normale Pfad DARF die erwartete Referenz nicht automatisch aktualisieren und keinen Runtime-Load starten. Der separate, ausdrücklich bestätigte `load_model_with_integrity_override`-Pfad DARF die aktuell vorgefundene Datei als `untrusted` laden, MUSS aber `file_sha256` unverändert lassen.
- **FR-027**: Erfolgreiche Katalog-/HF-Downloads, Updates und lokale Importe MÜSSEN den SHA-256-Hash der final veröffentlichten Datei in `models.file_sha256` speichern. Der Hash beschreibt den Dateiinhalt und wird auf jedem Gerät gleich ermittelt.
- **FR-028**: Für historische lokale Dateien ohne Integritätsmetadaten MUSS die Modellverwaltung denselben ausdrücklich bestätigten Integritätsdialog anbieten: unsicher laden, die Quelle reparieren (HF erneut herunterladen bzw. lokal neu importieren) oder ein anderes Modell auswählen.
- **FR-029**: Die freie Hugging-Face-Suche MUSS Filter für Quantisierung, maximale GGUF-Dateigröße und Hardware-Passung anbieten. Die Filterentscheidung MUSS auf den normalisierten Dateikandidaten einschließlich nachgeladener Größen-/Fit-Metadaten beruhen; ein Repository wird nur angezeigt, wenn mindestens eine Datei alle gesetzten Filter erfüllt. Bei gesetzter Größenbegrenzung MÜSSEN Dateien ohne bekannte Größe ausgeschlossen werden. Die anschließende Dateiauswahl DARF die gesetzten Filter nicht umgehen.
- **FR-030**: Die freie Hugging-Face-Suche MUSS beim Öffnen ohne Suchbegriff automatisch bis zu zehn meistgeladene öffentliche Repositorys mit installierbaren GGUF-Dateien anzeigen. Diese Default-Liste MUSS nach der Download-Metrik absteigend sortiert sein; explizite Suchanfragen MÜSSEN weiterhin bis zu 20 Treffer liefern können und die bestehende Mindestlänge von zwei Zeichen einhalten.
- **FR-031**: Ein fehlgeschlagener lokaler Hashvergleich MUSS den Integritätsstatus nach Möglichkeit auf `untrusted` setzen, ohne `fileSha256` zu überschreiben. `repair_source` MUSS bei HF-Modellen einen expliziten Fresh-Download auch bei identischer Quelle erzwingen. Der sicherheitsrelevante GGUF-Header-Range-Request MUSS `206 Partial Content` verlangen und darf höchstens `maxBytes` aus dem Response-Body puffern; andere Statuscodes sind strukturierte Fehler.
- **FR-032**: Ein laufender HF-Download MUSS einen abgebrochenen Response-Body innerhalb desselben Vorgangs aus der temporären `.part`-Datei fortsetzen dürfen. Ein Retry mit `Range` MUSS an einen `ETag` oder `Last-Modified`-Validator der vorherigen Antwort gebunden sein und diesen als `If-Range` mitsenden. Fehlt der Validator, MUSS der Teilstand verworfen und bei Byte 0 neu begonnen werden; Validatoren werden nicht als Modellmetadaten persistiert.
- **FR-033**: Eine Resume-Antwort mit `206 Partial Content` MUSS ein parsebares `Content-Range` mit Start gleich dem angeforderten Offset, konsistentem Ende und Gesamtumfang liefern. Ein vorhandener `Content-Length` MUSS zur Bereichslänge passen, ein bereits bekannter Gesamtumfang darf nicht abweichen, und die Datei darf nur finalisiert werden, wenn die heruntergeladenen Bytes exakt dem validierten Gesamtumfang entsprechen. Fehlende oder widersprüchliche Metadaten MÜSSEN einen Neustart bei Byte 0 oder einen strukturierten Fehler auslösen; `Content-Length` allein darf bei einem Resume keinen Gesamtumfang ableiten.
- **FR-034**: Kurz beendete Response-Bodies sowie transient fehlerhafte Antworten (mindestens HTTP 408, 429 und 5xx oder ein lesebezogener Timeout) MÜSSEN mit einem begrenzten Retry-Budget behandelt werden. Ein HTTP-416 beim Resume MUSS einen Neustart bei Byte 0 ermöglichen; permanente HTTP-Fehler dürfen nicht unnötig wiederholt werden. Kein Retry darf einen unvollständigen oder widersprüchlichen Teilstand veröffentlichen.

### Key Entities

- **HuggingFaceSearchQuery**: Validierte freie Suchanfrage mit Paging-/Limit-Information.
- **HuggingFaceModelResult**: Öffentliches Repository mit normalisierten Metadaten und installierbaren Datei-Kandidaten.
- **HuggingFaceFileCandidate**: Konkrete GGUF-Datei mit Repository-ID, Dateiname, aufgelöster Commit-SHA, optionalem Upstream-Ref, Größe, Quantisierung und Tokenizer-Hinweisen. Quantisierung und Kontextfenster stammen zuerst aus Repository-Metadaten, dann aus einer Dateinamen-Heuristik; ein GGUF-Header-Read ist nur für sicherheitsrelevante `TooBig`-Entscheidungen vorgesehen. Ein exakter Katalogtreffer trägt zusätzlich `catalogMatch` und `catalogEntryId`.
- **InstalledModel**: Bestehende lokale Modell-Entität; erhält Quelle, den erwarteten `fileSha256` und den Integritätsstatus eines Downloads/Imports, bleibt aber in derselben lokalen Modellverwaltung wie Katalogmodelle. Die Modell-ID ist bei freien Downloads deterministisch aus Repository-ID und Dateiname abgeleitet und nicht nutzer-editierbar; der Anzeigename ist ein separates, frei editierbares Feld.
- **HuggingFaceUpdateStatus**: Nicht-lokaler Prüfstatus für ein installiertes HF-Modell mit gepinnter SHA, optionaler neuer SHA, Upstream-Ref, Prüfzeitpunkt und retrybarem Fehlerstatus. Ein verfügbares Update wird sichtbar angezeigt, aber nicht automatisch installiert.
- **ModelIntegrity**: Integritätsinformationen der Modell-Entität mit erwartetem SHA-256-Hash und Status (`verified`, `untrusted`, `unknown`). Vor jedem lokalen Load wird diese Referenz gegen den aktuellen Datei-Hash geprüft.

## Success Criteria

### Measurable Outcomes

- **SC-001**: Ein Nutzer kann von der Modellverwaltung aus ein öffentliches Repository suchen, eine GGUF-Datei auswählen und den Download ohne manuelle Eingabe einer URL starten.
- **SC-002**: 100 % der nicht-GGUF-Treffer in den Test-Fixtures werden aus dem installierbaren Ergebnisstrom ausgeschlossen.
- **SC-003**: Kein abgebrochener oder fehlgeschlagener Download erscheint nach Neustart in `list_installed_models`.
- **SC-004**: Ein erfolgreich installiertes freies Modell kann nach Neustart ohne erneute Hugging-Face-Suche geladen und im Chat verwendet werden.
- **SC-005**: Für Suchfehler, Downloadfehler und fehlende Tokenizer-Metadaten wird jeweils ein lokalisierter, retrybarer UI-Zustand angezeigt.
- **SC-006**: Die kuratierten Qwen3-Vorschläge und die bestehende Modell-Fallback-Kette bestehen alle vorhandenen Regressionstests unverändert.
- **SC-007**: Ein Nutzer kann nach Abschluss des Onboardings jederzeit ein installiertes Modell im „Modelle verwalten"-Bereich mit Bestätigung entfernen, ohne dass andere lokale Modelle, synchronisierte Metadaten oder die Fallback-Kette beeinträchtigt werden.
- **SC-008**: Ein installierter HF-Download enthält in allen erfolgreichen Testfällen eine konkrete Commit-SHA; ein simuliertes Upstream-Update wird in der Modellverwaltung erkannt und mit einer expliziten Installationsaktion angezeigt, ohne das bisherige Modell automatisch zu ersetzen.
- **SC-009**: In allen erfolgreichen normalen lokalen Load-Testfällen wird die Datei vor dem Runtime-Start gehasht und gegen den gespeicherten Geräte-Hash geprüft; in allen Mismatch-/Missing-Hash-Fixtures startet über den normalen Ladepfad kein Runtime-Load. Ein separater Override-Test darf nur nach ausdrücklicher Bestätigung laden, markiert den Zustand als `untrusted` und verändert den gespeicherten Hash nicht.
- **SC-010**: Nach einer HF-Suche kann ein Nutzer die Treffer nach Quantisierung, maximaler GGUF-Dateigröße und Hardware-Passung filtern; kein angezeigtes Repository enthält danach ausschließlich nicht passende Dateien, und der Datei-Picker bietet keine ausgefilterte Datei zur Installation an.
- **SC-011**: Beim Öffnen der freien Suche erscheinen ohne weitere Eingabe bis zu zehn meistgeladene öffentliche Repositorys mit installierbaren GGUF-Dateien; die Liste bleibt filter- und auswählbar.
- **SC-012**: Die Download-Regressionstests zeigen für einen abgebrochenen Body mit Validator ein korrektes Resume, für einen fehlenden Validator einen Neustart bei Byte 0 und für fehlendes oder inkonsistentes `Content-Range` keine Veröffentlichung eines verkürzten oder zusammengesetzten GGUF. Permanente HTTP-Fehler werden nicht unnötig wiederholt.

## Assumptions

- Der lokale Runtime-Adapter unterstützt im ersten Schnitt GGUF-Dateien; weitere Formate benötigen eine eigene Spec.
- Hugging Face bleibt der einzige externe Discovery-Dienst dieses Features.
- Anonyme öffentliche Hub-Anfragen sind für den ersten Schnitt ausreichend; Zugangsdaten werden weder gespeichert noch in Logs ausgegeben.
- Der vorhandene Download-Client, lokale Pfad-Validator, `models`-Speicher und Modell-Picker werden erweitert, sofern sie den Contract bereits erfüllen.
- Hardware-Fit ist eine Warn-/Entscheidungshilfe und keine Garantie, dass jedes Modell auf jedem Gerät performant läuft.
- Die genaue API-Version, Paging-Strategie und ein eventueller Hub-Client werden im Plan festgelegt; neue Dependencies werden nur bei nachgewiesenem Bedarf ergänzt.

## Out of Scope

- Private oder gated Hugging-Face-Repositories und Login-/Tokenverwaltung.
- Konvertierung von Safetensors, PyTorch, MLX oder anderen Nicht-GGUF-Formaten.
- Automatische Modelltests, Qualitätsrankings oder Sicherheits-/Lizenzgutachten.
- Teilen oder Synchronisieren der Modellbytes zwischen Geräten; synchronisiert werden höchstens bestehende Modellmetadaten nach den Regeln von Spec 002.
