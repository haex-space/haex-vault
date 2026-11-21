# Phase 0 Research: Freie Hugging-Face-Modellsuche und Installation

## Entscheidung 1: Bestehendes `reqwest` wiederverwenden

**Decision**: Der Hugging-Face-Client verwendet die bereits vorhandene
`reqwest`-Dependency mit Rustls, JSON und Streaming. Es wird zunächst keine
weitere Hub-SDK-Dependency ergänzt.

**Rationale**:

- Das Projekt nutzt `reqwest` bereits für Provider- und Downloadpfade.
- Suchantworten und Repository-Dateilisten benötigen nur HTTP-GET, JSON-
  Deserialisierung und vorhandene Streaming-Unterstützung.
- Eine zusätzliche SDK-Abstraktion würde die Zahl der Fehler- und Updateflächen
  erhöhen, ohne im ersten Schnitt eine fachliche Fähigkeit zu liefern.

**Alternatives considered**:

- Hugging-Face-Hub-SDK: zusätzliche Dependency und unklarer Mehrwert für den
  begrenzten anonymen Read-/Downloadpfad.
- Frontend-seitiger Fetch: würde Netzwerk- und Validierungslogik aus dem
  bestehenden Rust-Download- und Fehlerboundary herauslösen und duplizieren.

## Entscheidung 2: Zweistufige Discovery statt Download aus Suchresultat

**Decision**: Suche liefert normalisierte Repository-Treffer; die konkrete
Dateiliste wird pro ausgewähltem Repository nachgeladen bzw. aus einem
Detail-Response ermittelt. Der Download akzeptiert danach ausschließlich einen
validierten Repository-/Datei-Contract.

**Rationale**:

- Die Suchantwort ist nicht zuverlässig reich genug, um alle GGUF-Dateien,
  Größen und Tokenizer-Hinweise darzustellen.
- Das reduziert Payload-Größe bei der Suche und macht die Nutzerentscheidung
  explizit, bevor große Metadatenlisten angezeigt werden.
- Der Download bleibt unabhängig von einer zuvor offenen Suchseite und kann
  dieselbe validierte Contract-Struktur verwenden.

**Alternatives considered**:

- Alle Dateien in jedem Suchresultat laden: unnötig langsam und potenziell
  viele Requests.
- Direkte URL-Eingabe: erfüllt Discovery nicht und umgeht die Sicherheits-
  und Metadatenvalidierung.

## Entscheidung 3: GGUF als Installationsgrenze

**Decision**: Im ersten Schnitt werden nur Dateien mit `.gguf`-Endung und
passenden Hub-Metadaten als installierbar angeboten. Die Datei wird vor der
Registrierung zusätzlich durch den bestehenden lokalen Loadpfad validiert.

**Rationale**:

- Der aktuelle lokale Adapter lädt GGUF und die lokale Dateistruktur ist darauf
  ausgelegt.
- Dateiendung allein ist keine Integritätsprüfung; der Runtime-Load bleibt die
  zweite, fachlich relevante Validierung.
- Andere Formate benötigen eigene Runtime-, Konvertierungs- und UX-Entscheidungen.

**Alternatives considered**:

- Alle Hugging-Face-Dateien anbieten: würde Downloads erzeugen, die HaexVault nicht
  verwenden kann.
- Nach Dateiname statt Metadaten filtern: Dateiendung und Hub-Dateityp werden
  gemeinsam geprüft; reine Namensheuristik wäre zu schwach.

## Entscheidung 4: Keine geratenen Tokenizer-Repositories

**Decision**: Ein Tokenizer-Repository wird automatisch vorausgefüllt, wenn
die Detaildaten einen belastbaren Hinweis liefern. Fehlt dieser Hinweis,
muss der Nutzer vor dem Download ein Repository angeben oder abbrechen.

**Rationale**:

- Quantisierungs-Repositories enthalten häufig GGUF-Dateien, aber nicht zwingend
  die passenden Tokenizer-Dateien.
- Ein falscher Default führt erst beim Modell-Load zu einem schwer verständlichen
  Fehler.
- Die bestehende Download-API benötigt bereits `tokenizer_repo`; die neue UI
  muss diesen Pflichtwert sichtbar machen.

**Alternatives considered**:

- Immer Repository-ID als Tokenizer verwenden: funktioniert bei manchen
  Original-Repositories, aber nicht verlässlich bei quantisierten Community-
  Repositories.
- Tokenizer erst beim Load abfragen: verschiebt einen bekannten Fehler in den
  Chat und erzeugt einen unbrauchbaren installierten Datensatz.

## Entscheidung 5: Vorhandene Modellverwaltung und atomaren Download erweitern

**Decision**: Freie Modelle werden mit demselben `models`-Datensatz, lokalen
Slugs, Publication-Lock, temporärer Datei, `list_installed_models` und
`load_model` verwaltet wie Katalogmodelle.

**Rationale**:

- Graphify und Quellprüfung zeigen, dass diese Pfade bereits die gemeinsame
  Modellgrenze bilden.
- Zwei lokale Registries würden Lösch-, Fallback- und Persistenzfehler
  wahrscheinlicher machen.
- Die Spec-002-Semantik, wonach das Dateisystem für „lokal installiert“
  autoritativ ist, bleibt erhalten.

**Alternatives considered**:

- Eigene Tabelle für Hugging-Face-Modelle: doppelte Buchführung und unnötige
  Migration.
- HF-Modelle nur als temporäre Chat-Auswahl: Modell wäre nach Neustart nicht
  wieder ladbar und könnte nicht Teil der bestehenden Fallback-Kette werden.

## Entscheidung 6: SHA-256-Prüfung vor jedem lokalen Load

**Decision**: Jede lokal ladbare Modell-Datei erhält einen SHA-256-Hash in der
gemeinsamen `models`-Zeile. Direkt vor jedem lokalen Runtime-Load wird die
kanonisch aufgelöste Datei vollständig in einem Blocking-Task gehasht und mit
dieser Referenz verglichen. Nur bei exakter Übereinstimmung wird ohne weitere
Rückfrage geladen.

Bei einer Abweichung zeigt die UI einen strukturierten Integritätsdialog. Der
Nutzer kann das aktuelle Modell ausdrücklich trotzdem laden und als unsicher
markieren, dasselbe Modell erneut von seiner HF-Quelle herunterladen oder ein
anderes Modell auswählen. Die erwartete SHA wird durch einen Mismatch niemals
automatisch überschrieben.

**Rationale**:

- Der Hash beschreibt den Dateiinhalt und ist unabhängig davon, auf welchem
  Gerät sich eine identische Datei befindet.
- Die Prüfung erkennt Austausch, nachträgliche Korrumpierung und fehlende
  Dateien, bevor `mistralrs` den Inhalt verarbeitet.
- Downloads, Updates und Importe setzen den Hash nach atomarer Veröffentlichung.
- Ein bewusst akzeptierter Mismatch bleibt sichtbar als `untrusted` markiert;
  die gespeicherte erwartete SHA bleibt für spätere Prüfungen erhalten.

**Alternatives considered**:

- Hash nur beim Download prüfen: erkennt spätere lokale Änderungen nicht.
- Hash bei jedem App-Start prüfen: lässt ein Modell zwischen Start und Load
  ungeschützt und erfüllt die Integritätsgrenze nicht.
- Mismatch automatisch akzeptieren: würde einen möglichen Austausch oder eine
  Korrumpierung verschleiern.

## Entscheidung 7: Source Identity, immutable Revision und Wiederholbarkeit

**Decision**: Die persistierten Metadaten unterscheiden Hub-Repository, Datei,
die beim Download aufgelöste Commit-SHA und optional den verfolgten Branch oder
Tag. Eine mutable Revision wie `main` wird vor dem Download über die Hub-API
aufgelöst; der eigentliche Datei-Resolve und die Registrierung verwenden nur
die konkrete SHA. Die lokale Modell-ID/der Slug wird vor der Installation
serverunabhängig und pfadsicher erzeugt bzw. validiert.

**Rationale**:

- Ein späterer Load benötigt keine erneute Suche.
- Die bestehende Pfadlogik schützt gegen Traversal, aber Source-Metadaten müssen
  zusätzlich nachvollziehbar bleiben.
- Die SHA macht Downloads reproduzierbar und verhindert, dass ein laufender
  Download unbemerkt auf einen anderen Upstream-Stand zeigt.
- Der zusätzliche Ref erlaubt, später den Upstream-Stand zu prüfen und dem
  Nutzer ein Update anzubieten. Bei einem direkten SHA-Pin ohne Ref wird kein
  automatisches Update erwartet.

**Alternatives considered**:

- Nur den vom Frontend gelieferten Namen speichern: Quelle wäre nicht
  nachvollziehbar und Kollisionen wären schwer zu erkennen.
- Nur eine frei erzeugte UUID speichern: verhindert zwar Kollisionen, verliert
  aber die Source-Identität und verschlechtert Wiederholung/UX.

## Entscheidung 8: Upstream-Updates als sichtbare, manuelle Aktion

**Decision**: Die Modellverwaltung prüft beim Öffnen und über eine explizite
Aktualisieren-Aktion die gespeicherten öffentlichen HF-Refs installierter
Modelle. Liefert Hugging Face eine andere Commit-SHA als die gespeicherte,
zeigt HaexVault einen Update-Hinweis mit alter und neuer SHA und bietet die
Installation dieser Revision an. Die bestehende Datei wird erst nach einem
erfolgreichen atomaren Download ersetzt.

**Rationale**:

- Nutzer bleiben über neue Modellstände informiert, ohne dass laufende Chats
  oder reproduzierbare lokale Installationen überraschend verändert werden.
- Die Prüfung kann denselben testbaren HTTP-Client und die bestehende
  Download-/Registrierungsgrenze wiederverwenden.
- Offline-, Timeout- und gelöschte-Repository-Fehler dürfen den lokalen
  Modellbestand nicht verändern.

**Alternatives considered**:

- Automatische Updates: ausgeschlossen, weil sie Modellverhalten und lokale
  Dateien ohne ausdrückliche Nutzerentscheidung verändern würden.
- Nur manuelle Suche: würde installierte Modelle nicht zuverlässig über neue
  Upstream-Stände informieren.

## Entscheidung 9: Metadaten-Provenienz und Katalogabgleich

**Decision**: Quantisierung und Kontextfenster werden mit einer Provenienz
(`hub_metadata`, `filename_heuristic`, `gguf_header`, `unknown`) normalisiert.
Ein GGUF-Header wird nur per Range-Request gelesen, wenn die `TooBig`-Entscheidung
sonst sicherheitsrelevant unklar bleibt. Jeder Such-/Detailtreffer erhält einen
expliziten `catalogMatch`-Status und optional eine `catalogEntryId`; die freie
Suche bleibt sichtbar und wird nicht automatisch mit dem Katalog verschmolzen.

**Rationale**:

- Die UI kann Unsicherheit ehrlich anzeigen und Entscheidungen nachvollziehbar
  machen.
- Der Katalogabgleich bleibt eine Darstellungshilfe und verändert weder die
  deterministische HF-Modell-ID noch die Persistenzsemantik.

## Entscheidung 10: Begrenztes Suchergebnis

**Decision**: Die freie Suche liefert standardmäßig höchstens 20 normalisierte
Repository-Treffer pro Seite. Ein vom Client angefordertes Limit wird auf
diesen Maximalwert gekappt; Deduplizierung und deterministische Sortierung
erfolgen nach der Normalisierung.

**Rationale**: Ein konkretes Limit hält Payload und UI überschaubar und macht
Verhalten sowie Tests reproduzierbar.

## Entscheidung 11: HTTP-Client als testbare Grenze

**Decision**: HTTP-Aufrufe werden hinter einem kleinen internen Client-/Transport-
Boundary isoliert. Normalisierung, GGUF-Filter und Sortierung bleiben ohne
Netzwerk testbar; `wiremock` deckt Status, Timeout, JSON und Redirect-Verhalten ab.

**Rationale**: Bestehende Rust-Tests nutzen bereits `wiremock`; keine echte
Hugging-Face-Verbindung ist für deterministische Tests erforderlich.

**Alternatives considered**: Live-Hub-Tests wären flakey, langsam und abhängig
von externer Verfügbarkeit.

## Entscheidung 12: Sicheres resumierbares HTTP-Downloadverhalten

**Decision**: Ein laufender Modell-Download darf einen abgebrochenen Body aus
seiner temporären `.part`-Datei fortsetzen. Ein Retry darf nur dann Bytes
anhängen, wenn die vorherige Antwort einen `ETag` oder `Last-Modified`-
Validator geliefert hat; die Anfrage sendet diesen Validator als `If-Range`.
Fehlt der Validator, wird die `.part`-Datei verworfen und der Download bei
Byte 0 neu gestartet.

Eine `206 Partial Content`-Antwort wird nur akzeptiert, wenn ihr `Content-Range`
parsebar ist, am angeforderten Offset beginnt, ein konsistentes Ende und einen
konkreten Gesamtumfang angibt und zu einem vorhandenen `Content-Length` sowie
zu einem bereits bekannten Gesamtumfang passt. Bei fehlenden oder
widersprüchlichen Metadaten wird nicht angehängt, sondern bei Byte 0 neu
begonnen oder ein strukturierter Fehler geliefert. Finalisiert wird nur, wenn
die heruntergeladene Byte-Anzahl exakt dem validierten Gesamtumfang entspricht.

Kurz beendete Bodies, 408, 429, 5xx und lesebezogene Timeouts bleiben innerhalb
eines begrenzten Retry-Budgets retrybar; ein 416 beim Resume startet neu. Andere
permanente HTTP-Fehler werden nicht unnötig wiederholt. Entity-Validatoren und
Bereichsmetadaten sind flüchtige Transportdaten und werden nicht persistiert.

**Rationale**:

- Ein `Range`-Offset ohne Entity-Validator kann Bytes zweier verschiedener
  Modellversionen unbemerkt zusammenfügen.
- `Content-Length` beschreibt nur die Länge des aktuellen Response-Bodys;
  `Content-Range` bestätigt zusätzlich dessen absolute Position und den
  Gesamtumfang.
- Exakte Finalisierungsbedingungen verhindern, dass ein sauber beendeter, aber
  verkürzter Body als vollständiges GGUF veröffentlicht wird.

**Alternatives considered**:

- Resume ohne Validator: verworfen, weil ein CDN oder Hub zwischen Retries eine
  andere Modellversion liefern kann.
- `Content-Length` als Ersatz für `Content-Range`: verworfen, weil daraus bei
  einer `206`-Antwort weder der absolute Startoffset noch die vollständige
  Dateigröße zuverlässig hervorgeht.

## Ergebnis

Die Planung kann auf der bestehenden Rust-/Nuxt-Struktur aufsetzen. Es sind
keine neuen externen Dependencies oder Secrets notwendig. Für die
Implementierungsphase bleiben nur die konkrete API-Payload-Abbildung und die
UI-Ausgestaltung der bereits entschiedenen SHA-Update-Benachrichtigung offen.
