# Feature Specification: Einstellungs-App mit Kategorien

**Feature Branch**: `023-settings-app`
**Created**: 2026-09-26
**Status**: Implemented (all tasks T001–T074 done, 2026-09-27)

> **Hinweis (2026-10-15)**: Maßgeblich für Kategorien und Orte ist heute
> `src/lib/settings/registry.ts`. „Modelle“ und „Agenten“ sind seit PR #454 die
> Kategorie „KI“ (`/ai`, Orte `ai.*`); die Pfade `/models/…` und `/agents/…`
> in diesem Dokument sind der Stand bei der Umsetzung. „Darstellung“ gehört
> seit Spec 042 zu „Allgemein“.

**Input**: Die Einstellungen von holzi bekommen den Aufbau aus haex-vault 2.x (Legacy): links
eine Seitenleiste mit Kategorien, rechts der Inhalt der gewählten Kategorie mit
Titel und Beschreibung, Übersichten mit Unteransichten. Föderation wird eine
Kategorie der Einstellungen statt einer eigenen App, Darstellung kommt als
Kategorie hinzu. (Präzisiert in den Clarifications: Die Föderations-App
entfällt; die Kategorie „Föderation“ zeigt die Geräte der Vault.) Referenz: haex-vault 2.x @ `8dce379d94e18fcd42c3b73686a06f984ca3f574`,
`src/components/haex/system/settings/` und
`src/components/haex/system/settings-layout/`.

## Beziehung zu bestehenden Specs

- [`015-workspace-shell`](../015-workspace-shell/spec.md): Die Einstellungen
  bleiben die App „Einstellungen“ mit einer einzigen Instanz. Die App
  „Föderation“ entfällt (FR-016 bis FR-018 dieser Spec); die Weiterleitung der
  früheren Vollseiten-Adresse `/federation` (FR-004 dort) führt künftig in die
  Einstellungen.
- [`020-tab-navigation`](../020-tab-navigation/spec.md): Jede Kategorie und jede
  Unteransicht ist ein Ort im Tab. Vor, Zurück, Verlaufsliste, Öffnen an einem
  Ort und Deep-Links gelten ohne Sonderregeln. Das haex-vault-2.x-Muster
  `useDrillDownNavigation` wird nicht übernommen (Begründung in Spec 020). Jede
  Änderung einer Einstellung bleibt eine Aktion im Katalog von Spec 020.
- [`022-session-restore`](../022-session-restore/spec.md): Die Einstellung
  „Sitzung wiederherstellen“ bekommt einen festen Platz in einer Kategorie. Mit
  eingeschalteter Wiederherstellung kommt ein Einstellungs-Tab samt Ort und
  Historie zurück; das ist eine Funktion der Sitzung, keine Merk-Funktion der
  Einstellungen.
- [`002-onboarding-model-prefs`](../002-onboarding-model-prefs/spec.md),
  [`005-huggingface-model-discovery`](../005-huggingface-model-discovery/spec.md),
  [`007-cli-delegate`](../007-cli-delegate/spec.md),
  [`009-autonomous-delegate-mode`](../009-autonomous-delegate-mode/spec.md),
  [`010-stt-model-choice`](../010-stt-model-choice/spec.md): Diese Specs bleiben
  maßgeblich dafür, was die einzelnen Einstellungen tun. Diese Spec ordnet sie
  nur neu an.
- Die geplante Spec **Befehle und Tastenkürzel** bekommt später eine eigene
  Kategorie. Die geplante Spec **Desktop-Symbole und Raster** ergänzt
  „Darstellung“ um den Hintergrund des Arbeitsbereichs. Künftige Specs zur
  **Föderation** erweitern die Kategorie „Föderation“.

## Clarifications

### Session 2026-09-26

- Q: Was zeigt die Kategorie „Föderation“, solange holzi keine
  Föderationsfunktionen hat? → A: Die ursprüngliche Antwort, die Kategorie
  aufzuschieben und alte Wege nach „Allgemein“ zu führen, wurde beim
  Plan-Review durch die folgende Entscheidung ersetzt.
- Q: Gehört ein Hintergrund des Arbeitsbereichs (Bild oder Farbverlauf wie in
  haex-vault 2.x) zu dieser Spec? → A: Nein. „Darstellung“ enthält hier nur das
  Farbschema; der Hintergrund kommt mit der Spec Desktop-Symbole und Raster.
- Q: (Betreiber-Rückmeldung beim Testen von Spec 022) Brauchen Einstellungen
  einen Knopf zum Speichern? → A: Nein. Was ausgewählt ist, ist gespeichert;
  keine Einstellung hat einen Knopf zum Übernehmen (FR-021).
- Q: (Plan-Review) Soll die Föderation doch schon jetzt eine Kategorie der
  Einstellungen sein statt nur zu entfallen? → A: Ja. Die Kategorie
  „Föderation“ zeigt die Geräte der Vault: dieses Gerät und alle anderen
  bekannten Geräte mit Namen. Alte Wege zur Föderations-App führen dorthin.
- Q: (Plan-Review) Welches Datum zeigt die Geräteliste, und aktualisiert sie
  sich live? → A: Nicht das Datum des Hinzufügens, sondern wann ein Gerät
  zuletzt online war, und die Liste soll sich über den CRDT-Sync live
  aktualisieren. Beides braucht den iroh-Sync, den holzi noch nicht hat; es
  kommt mit der Sync-Spec. Bis dahin zeigt die Liste Namen ohne Datum.
- Q: (Analyse) Wie heißt der Tab beim Navigieren in den Einstellungen? → A:
  Immer „Einstellungen“; die Verlaufsliste an Vor/Zurück zeigt die Orte.
- Q: (Analyse) Startet die Auswahl eines nicht installierten
  Spracherkennungsmodells den Download? → A: Nein. Die Auswahl enthält nur
  installierte Modelle; nicht installierte haben einen Knopf „Herunterladen“
  (FR-021).
- Q: (Betreiber-Rückmeldung nach US1) Wie viel Text zeigen Kopf und Ansichten?
  → A: Der Kopf zeigt nur Zurück-Pfeil und Titel, keine Beschreibungszeile. Die
  Ansichten zeigen nur ihre Liste bzw. Einstellung, ohne eigene Überschriften,
  Beschreibungsabsätze oder Karten-Rahmen; Zeilen einer Übersicht behalten ihre
  eine Zeile Beschreibung. Auf breiten Fenstern steht die Liste zentriert mit
  begrenzter Breite. Die Seitenleiste gleitet beim Ändern der Fensterbreite.
- Q: (Betreiber-Rückmeldung) Der Titel im Kopf springt, je nachdem ob der
  Zurück-Pfeil da ist. Was steht auf der Startseite einer Kategorie an seiner
  Stelle? → A: Das Symbol der Kategorie, im selben Platz wie der Pfeil; der
  Titel steht damit immer an derselben Stelle (FR-002).
- Q: (Betreiber-Rückmeldung) Der Launcher zeigt noch „Föderation“. Wann
  verschwindet die App? → A: Sofort, vor der Geräteliste: Die App entfällt aus
  Launcher und Tab-Menü, alte Aufrufe öffnen die Kategorie „Föderation“ (FR-016,
  FR-017); deren Inhalt folgt mit US5.
- Q: (Betreiber-Rückmeldung, Vorbild GNOME-Einstellungen) Was zeigt ein
  schmales Fenster statt der Symbolleiste? → A: Die Seitenleiste ist dort ganz
  ausgeblendet; ein Symbol in der Werkzeugleiste öffnet sie über den Inhalt, die Wahl
  einer Kategorie schließt sie wieder. In breiten Fenstern steht sie neben dem
  Inhalt und lässt sich über dasselbe Symbol ausblenden. Keiner der beiden
  Zustände wird gemerkt (FR-004).
- Q: (Betreiber-Rückmeldung) Gibt es doch eine Suche? → A: Ja, ein Suchfeld
  oben in der Seitenleiste führt direkt zu passenden Orten und einzelnen
  Einstellungen; das hebt „keine Suche über alle Einstellungen“ aus „Nicht im
  Umfang“ auf (FR-023). Ein globales Zurücksetzen bleibt ausgeschlossen.
- Q: (Betreiber-Rückmeldung, Vorbild COSMIC/GNOME) Wie sind Rahmen und Listen
  gestaltet? → A: Die erste Zeile ist eine schmale Werkzeugleiste nur mit dem
  Knopf für die Seitenleiste und der Suche (das Such-Symbol klappt dort zum
  Suchfeld auf). Darunter stehen der große Titel und der Inhalt. Listen sind
  abgerundete Gruppen mit fein getrennten Zeilen (Titel, Beschreibung, rechts
  das Bedienelement); Übersichten zeigen je Bereich eine eigene abgerundete
  Karte. Das ersetzt „ohne Karten-Rahmen“ aus der Rückmeldung nach US1; ohne
  Überschriften und Beschreibungsabsätze bleibt es (FR-002).
- Q: (Betreiber-Rückmeldung) Gelten Einstellungen pro Gerät oder pro Vault? →
  A: Pro Vault, auf jedem Gerät gleich; die Wahl „Dieses Gerät / Alle Geräte“
  entfällt überall. Ausnahmen sind nur Gerätename, Standard-Modell und
  Spracherkennungsmodell, weil Name, Modell-Dateien und Hardware am Gerät
  hängen; sie gelten nur für dieses Gerät (FR-024).
- Q: (Betreiber-Rückmeldung) Im dunklen Schema sind die Werte der
  Auswahllisten unlesbar. → A: Bedienelemente kommen aus dem haex-ui-Layer
  (Auswahlliste, Eingabefeld, Schalter, Häkchen); native Elemente bekommen mit
  `color-scheme` das passende Schema (FR-013).
- Q: (Betreiber-Rückmeldung) Wohin führt der Zurück-Pfeil nach Modelle →
  Installierte Modelle → Modelle herunterladen? → A: Zu „Installierte Modelle“,
  also dorthin, woher der Nutzer kam, solange das in derselben Kategorie liegt;
  sonst zur übergeordneten Ansicht (FR-009).

### Session 2026-09-27

- Q: (Betreiber-Rückmeldung) Ein Deep-Link über `location.search` lädt die
  App nur neu und öffnet nichts. → A: Der Neuladen scheiterte im
  Entwicklungs-Webview an Vite, nicht an der App. Die Arbeitsfläche nimmt
  `?open=…&at=…` aber künftig auch an, wenn der Parameter bei geöffneter Seite
  in die Adresse kommt, nicht nur beim Laden (FR-011).
- Q: (Betreiber-Rückmeldung) Sollen die manuell geprüften Szenarien
  automatisch laufen? → A: Ja, als echte End-to-End-Tests gegen die gebaute
  App (Spec 016): jedes Quickstart-Szenario, das ohne Netz, zweites Gerät oder
  Zeitmessung auskommt (SC-007).

## User Scenarios & Testing _(mandatory)_

### User Story 1 - Einstellungen nach Kategorien finden (Priority: P1)

Ein Nutzer öffnet die Einstellungen. Links sieht er die Kategorien, rechts den
Inhalt der ersten Kategorie unter ihrem Titel. Er klickt
auf „Modelle“ und sieht dort alles, was Modelle betrifft, statt eine lange Seite
mit allen Einstellungen durchzuscrollen.

**Why this priority**: Das ist der Kern der Spec. Die heutige Einstellungsseite
ist eine lange Liste, die mit jeder neuen Einstellung unübersichtlicher wird.

**Independent Test**: Einstellungen öffnen, jede Kategorie anklicken: Jede zeigt
Titel, Beschreibung und nur ihre eigenen Einstellungen; jede bisherige
Einstellung ist in genau einer Kategorie zu finden.

**Acceptance Scenarios**:

1. **Given** die Einstellungen sind geschlossen, **When** der Nutzer sie öffnet,
   **Then** zeigt die Seitenleiste alle Kategorien, die erste ist ausgewählt und
   ihr Inhalt ist sichtbar.
2. **Given** die Einstellungen sind offen, **When** der Nutzer eine andere
   Kategorie wählt, **Then** ist diese in der Seitenleiste hervorgehoben und ihr
   Inhalt ersetzt den vorherigen.
3. **Given** eine Kategorie, **When** sie angezeigt wird, **Then** steht oben ihr
   Titel, darunter ihr Inhalt, der für sich scrollt, während Kopf und
   Seitenleiste stehen bleiben.
4. **Given** die Einstellungen vor dieser Spec, **When** der Nutzer eine
   beliebige frühere Einstellung sucht, **Then** findet er sie in genau einer
   Kategorie (Zuordnung in FR-005).

---

### User Story 2 - Unteransichten mit Übersicht und Zurück (Priority: P1)

Eine Kategorie mit mehreren Bereichen, zum Beispiel „Modelle“, zeigt zuerst eine
Übersicht: Zeilen mit Symbol, Titel, einer Zeile Beschreibung und einem Pfeil.
Der Nutzer wählt „Modelle herunterladen“, sucht auf HuggingFace, öffnet ein
Ergebnis und wählt eine Datei. Mit dem Zurück-Pfeil im Kopf kommt er Schritt für
Schritt zurück zur Übersicht; die Zurück-Taste des Tabs (Spec 020) wirkt
genauso.

**Why this priority**: Ohne Unteransichten müssten große Bereiche wie die
Modellverwaltung wieder auf einer Seite stehen. Die Navigation muss sich dabei
verhalten wie überall sonst im Tab.

**Independent Test**: In „Modelle“ zwei Ebenen tief gehen, dann einmal den
Zurück-Pfeil im Kopf und einmal die Zurück-Taste des Tabs benutzen: Beide führen
jeweils eine Ebene zurück. Vor im Tab führt wieder hinein.

**Acceptance Scenarios**:

1. **Given** eine Kategorie mit mehreren Bereichen, **When** der Nutzer sie
   wählt, **Then** sieht er ihre Übersicht mit einer Zeile je Bereich.
2. **Given** die Übersicht, **When** der Nutzer eine Zeile wählt, **Then** öffnet
   sich die Unteransicht mit eigenem Titel und einem Zurück-Pfeil im Kopf.
3. **Given** eine Unteransicht, die der Nutzer von der Übersicht aus geöffnet
   hat, **When** er den Zurück-Pfeil im Kopf wählt, **Then** ist die Übersicht
   wieder zu sehen, und Vor im Tab führt zurück in die Unteransicht.
4. **Given** eine Unteransicht, die über einen Deep-Link direkt geöffnet wurde,
   **When** der Nutzer den Zurück-Pfeil im Kopf wählt, **Then** führt er zur
   übergeordneten Ansicht und mit weiteren Klicks Schritt für Schritt bis zur
   Übersicht der Kategorie, nicht aus den Einstellungen heraus.
5. **Given** der Nutzer wechselt aus einer Unteransicht in eine andere Kategorie
   und wieder zurück, **When** er die frühere Kategorie wählt, **Then** beginnt
   sie mit ihrer Übersicht, nicht mit der zuletzt offenen Unteransicht.

---

### User Story 3 - Direkt an eine Stelle springen (Priority: P2)

Ein Hinweis in holzi, ein anderer Teil der App oder ein Agent will den Nutzer zu
einer bestimmten Einstellung schicken, zum Beispiel zum Spracherkennungsmodell.
Die Einstellungen öffnen sich genau dort. Sind sie schon offen, springt der
vorhandene Tab dorthin.

**Why this priority**: Kontextbezogene Hinweise („Kein Modell installiert —
Modelle herunterladen“) werden erst nützlich, wenn sie an die richtige Stelle
führen.

**Independent Test**: holzi über eine Adresse mit Kategorie und Unteransicht
öffnen: Die Einstellungen zeigen genau diese Unteransicht. Dasselbe bei schon
offenen Einstellungen: kein zweiter Tab, der vorhandene springt dorthin.

**Acceptance Scenarios**:

1. **Given** die Einstellungen sind geschlossen, **When** eine Stelle in holzi
   sie an einer Kategorie oder Unteransicht öffnet, **Then** erscheinen sie
   genau dort.
2. **Given** die Einstellungen sind offen, **When** sie an einer anderen Stelle
   geöffnet werden, **Then** springt der vorhandene Tab dorthin, und Zurück
   führt zur vorherigen Stelle (Spec 020 FR-013).
3. **Given** eine Adresse mit einer unbekannten Kategorie oder Unteransicht,
   **When** sie geöffnet wird, **Then** zeigt holzi die erste Kategorie und einen
   Hinweis, statt eines Fehlers.

---

### User Story 4 - Farbschema wählen (Priority: P2)

Ein Nutzer arbeitet abends und möchte holzi dunkel. In „Darstellung“ wählt er
„Dunkel“. Die ganze App wechselt sofort. Ein anderer Nutzer lässt „System“
eingestellt, und holzi folgt dem Farbschema des Betriebssystems.

**Why this priority**: Die Oberflächenbibliothek bringt ein dunkles Schema
schon mit, holzi bietet es nur nicht an. Es ist die naheliegende erste
Einstellung für „Darstellung“.

**Independent Test**: Farbschema auf „Dunkel“, dann „Hell“, dann „System“
stellen und dabei das Farbschema des Betriebssystems umschalten: holzi folgt
jeweils sofort, ohne Neustart.

**Acceptance Scenarios**:

1. **Given** die Kategorie „Darstellung“, **When** der Nutzer „Hell“, „Dunkel“
   oder „System“ wählt, **Then** übernimmt die ganze App das Schema sofort, in
   allen offenen Fenstern.
2. **Given** „System“ ist gewählt, **When** das Betriebssystem sein Farbschema
   wechselt, **Then** folgt holzi ohne Zutun.
3. **Given** ein gewähltes Farbschema, **When** der Nutzer die Vault erneut
   öffnet, **Then** gilt dasselbe Schema wieder.
4. **Given** die Wahl des Farbschemas, **When** der Nutzer sie trifft, **Then**
   kann er sie wie das Standardmodell für dieses Gerät oder für die ganze Vault
   treffen; der Gerätewert geht vor.

---

### User Story 5 - Föderation in den Einstellungen (Priority: P2)

Die Föderation ist keine eigene App mehr, sondern eine Kategorie der
Einstellungen. Dort sieht der Nutzer, welche Geräte seine Vault nutzen: dieses
Gerät und alle anderen, die die Vault schon einmal geöffnet haben, mit ihren
Namen. Alte Wege zur Föderation (Launcher-Eintrag, frühere Adresse) führen in
diese Kategorie.

**Why this priority**: Betreiberentscheidungen vom 2026-09-25 und 2026-09-26.
Föderation ist eine Konfiguration der Vault, keine Arbeitsumgebung wie der
Chat. Die Geräteliste ist echter Inhalt (FR-006) und die Grundlage für spätere
Föderationsfunktionen.

**Independent Test**: Launcher öffnen: kein Eintrag „Föderation“ mehr. Die
frühere Föderations-Adresse öffnen: Die Einstellungen öffnen sich in der
Kategorie „Föderation“ und listen die Geräte der Vault.

**Acceptance Scenarios**:

1. **Given** der Launcher, **When** der Nutzer ihn öffnet, **Then** gibt es keine
   App „Föderation“ mehr.
2. **Given** die frühere Adresse der Föderation, **When** holzi sie öffnet,
   **Then** erscheinen die Einstellungen in der Kategorie „Föderation“.
3. **Given** die Kategorie „Föderation“, **When** der Nutzer sie öffnet,
   **Then** sieht er jedes bekannte Gerät der Vault einmal mit seinem Namen;
   dieses Gerät steht zuerst und ist als „Dieses Gerät“ markiert.
4. **Given** der Nutzer ändert in „Allgemein“ den Namen dieses Geräts, **When**
   er danach „Föderation“ öffnet, **Then** steht dort der neue Name.

---

### User Story 6 - Schmale Fenster (Priority: P2)

Ein Nutzer verkleinert das Einstellungsfenster oder nutzt holzi im Kompaktmodus
(Spec 015). Die Seitenleiste verschwindet, der Inhalt bekommt den ganzen Platz.
Ein Symbol in der Werkzeugleiste öffnet die Seitenleiste über den Inhalt; die Wahl
einer Kategorie schließt sie wieder (wie in den GNOME-Einstellungen).

**Why this priority**: Fenster lassen sich frei verkleinern, und im Kompaktmodus
ist wenig Platz. Eine volle Seitenleiste würde den Inhalt dort erdrücken.

**Independent Test**: Das Einstellungsfenster schrittweise schmaler ziehen: Ab
einer bestimmten Breite verschwindet die Seitenleiste, das Symbol in der Werkzeugleiste
öffnet sie als Vollbild-Menü, der Inhalt bleibt ohne waagerechtes Scrollen bedienbar.

**Acceptance Scenarios**:

1. **Given** ein breites Einstellungsfenster, **When** es angezeigt wird,
   **Then** zeigt die Seitenleiste Symbol und Namen jeder Kategorie.
2. **Given** das Fenster wird schmaler als eine feste Grenze, **When** es
   angezeigt wird, **Then** ist die Seitenleiste ausgeblendet; die
   Werkzeugleiste zeigt weiter ihr Symbol und die Suche.
3. **Given** ein schmales Fenster, **When** der Nutzer das Symbol wählt,
   **Then** füllt die Seitenleiste den Platz unter der Werkzeugleiste; die Wahl einer Kategorie oder
   Escape schließt sie wieder.
4. **Given** ein breites Fenster, **When** der Nutzer die Seitenleiste über ihr
   Symbol ausblendet, **Then** bekommt der Inhalt die ganze Breite, und
   dasselbe Symbol blendet sie wieder ein.
5. **Given** die Breite des Fensters (nicht des Bildschirms), **When** sie sich
   ändert, **Then** entscheidet sie allein über die Darstellung der Seitenleiste.

---

### Edge Cases

- Eine Kategorie wird geöffnet, während eine Einstellung darin gerade lädt oder
  ein Download läuft: Der Download läuft weiter, die Anzeige zeigt seinen Stand,
  sobald die Unteransicht wieder offen ist.
- Ein Nutzer ist mitten in einer Eingabe (Gerätename) und wechselt
  die Kategorie: Er verlässt damit das Feld, die Eingabe wird gespeichert
  (FR-021). Nur eine ungültige Eingabe geht verloren, wie beim Navigieren in
  Spec 020 („Inhalte bleiben nicht zwingend erhalten“).
- Eine gespeicherte Sitzung (Spec 022) enthält einen Tab der entfallenen App
  „Föderation“: Er wird beim Wiederherstellen verworfen wie jede unbekannte App
  (Spec 015 FR-025).
- Eine Unteransicht hängt von Daten ab, die es nicht mehr gibt (ein gelöschtes
  Modell, ein getrennter Anbieter): Sie zeigt einen Hinweis und den Weg zurück
  zur Übersicht, keinen Fehlerbildschirm.
- Das Farbschema lässt sich nicht lesen oder die Vault ist noch gesperrt
  (Entsperr- und Einrichtungsseiten): holzi folgt dem Farbschema des
  Betriebssystems.
- Eine Kategorie hat nur einen Bereich: Sie zeigt den Inhalt direkt, ohne
  Übersicht mit einer einzigen Zeile.
- Ein Gerät hat noch keinen Namen (Einrichtung dort nicht abgeschlossen): Die
  Geräteliste zeigt „Unbenanntes Gerät“.

## Requirements _(mandatory)_

### Functional Requirements

**Aufbau**

- **FR-001**: Die Einstellungen MÜSSEN links eine Seitenleiste mit den
  Kategorien und rechts den Inhalt der gewählten Kategorie zeigen. Die
  Seitenleiste ist eine flache Liste ohne Gruppen; die gewählte Kategorie ist
  hervorgehoben.
- **FR-002**: Die Einstellungen MÜSSEN oben eine schmale Werkzeugleiste haben,
  die nur den Knopf für die Seitenleiste und die Suche enthält (FR-004,
  FR-023). Jede Kategorie und jede Unteransicht MUSS darunter ihren Titel groß
  zeigen. Vor dem Titel steht bei Unteransichten ein Zurück-Pfeil, bei der
  Startseite einer Kategorie an derselben Stelle ihr Symbol, damit der Titel
  nicht springt; eine Beschreibungszeile gibt es nicht. Nur der Inhalt unter
  dem Titel scrollt. Der Inhalt zeigt Einstellungen und Listen als abgerundete
  Gruppen mit fein getrennten Zeilen (Titel, eine Zeile Beschreibung, rechts
  das Bedienelement), Übersichten als eine abgerundete Karte je Bereich, wie in
  den COSMIC- und GNOME-Einstellungen; eigene Überschriften und
  Beschreibungsabsätze gibt es nicht, nur einen kurzen Gruppennamen, wo eine
  Ansicht mehrere Gruppen oder eine Auswahl hat. Der Inhalt steht auf breiten
  Fenstern zentriert mit begrenzter Breite. Die Seitenleiste wechselt ihre
  Breite mit einem Übergang.
- **FR-003**: Eine Kategorie mit mehreren Bereichen MUSS eine Übersicht zeigen:
  eine Zeile je Bereich mit Symbol, Titel, einer Zeile Beschreibung und einem
  Pfeil. Eine Kategorie mit nur einem Bereich MUSS diesen direkt zeigen.
- **FR-004**: Die Seitenleiste MUSS unterhalb einer festen Breite des Fensters
  ausgeblendet sein; ein Symbol in der Werkzeugleiste öffnet sie über den Inhalt, die
  Wahl einer Kategorie oder eines Suchtreffers und Escape schließen sie. Darüber
  steht sie neben dem Inhalt und lässt sich ausblenden. Maßgeblich ist die
  Breite des Einstellungsfensters, nicht die des Bildschirms; ob sie ein- oder
  ausgeblendet ist, wird nicht gemerkt.

**Kategorien**

- **FR-005**: Die Einstellungen MÜSSEN in dieser Reihenfolge diese Kategorien
  haben, jede mit eigenem Symbol:

  | Kategorie   | Inhalt                                                                                                                                                                           |
  | ----------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
  | Allgemein   | Gerätename, Sitzung wiederherstellen (Spec 022)                                                                                                                                  |
  | Darstellung | Farbschema (US4)                                                                                                                                                                 |
  | Modelle     | Übersicht mit: Standardmodell; installierte Modelle mit Updates und Löschen; Modelle herunterladen (Empfehlungen und HuggingFace-Suche mit Dateiauswahl); Spracherkennungsmodell |
  | Agenten     | Übersicht mit: Anbieter verbinden; Autonomiemodus; Deny-Regeln                                                                                                                   |
  | Föderation  | Geräte der Vault (US5)                                                                                                                                                           |

- **FR-006**: Es DÜRFEN nur Kategorien erscheinen, die Inhalt haben. Kategorien
  aus haex-vault 2.x ohne Gegenstück in holzi (Erweiterungen, Kontakte, Identitäten,
  Speicher, Sicherheit, Protokolle, Entwickler und ähnliche) entfallen, bis eine
  eigene Spec sie füllt.
- **FR-007**: Jede Einstellung, die es vor dieser Spec gab, MUSS mit
  unverändertem Verhalten in genau einer Kategorie erreichbar sein.

**Navigation**

- **FR-008**: Jede Kategorie und jede Unteransicht MUSS ein eigener Ort im Sinne
  von Spec 020 sein. Der Wechsel der Kategorie und das Öffnen einer Unteransicht
  MÜSSEN neue Einträge in der Historie des Tabs erzeugen; Vor, Zurück und die
  Verlaufsliste gelten ohne Sonderregeln. Der Tab heißt dabei immer
  „Einstellungen“; die Verlaufsliste zeigt die Titel der Orte.
- **FR-009**: Der Zurück-Pfeil im Kopf einer Unteransicht MUSS zur vorigen
  Station der Historie führen, wenn sie in derselben Kategorie liegt, und dann
  wie Zurück im Tab wirken (kein doppelter Eintrag). Sonst (nach einem
  Deep-Link oder einem Sprung aus einer anderen Kategorie) MUSS er zur
  übergeordneten Ansicht navigieren. Seine Beschriftung nennt das Ziel.
- **FR-010**: Ein neu geöffneter Einstellungs-Tab MUSS mit der ersten Kategorie
  beginnen. Die zuletzt geöffnete Kategorie oder Unteransicht DARF NICHT von den
  Einstellungen gemerkt werden. Wählt der Nutzer eine Kategorie in der
  Seitenleiste, beginnt sie mit ihrer Übersicht.
- **FR-011**: Die Einstellungen MÜSSEN sich an jeder Kategorie und jeder
  Unteransicht öffnen lassen (Spec 020 FR-012), auch über Deep-Links und durch
  Agenten mit passender Berechtigung. Sind sie schon offen, MUSS der vorhandene
  Tab dorthin navigieren. Ein Deep-Link MUSS auch wirken, wenn er bei schon
  geöffneter Arbeitsfläche in die Adresse kommt.
- **FR-012**: Ein unbekannter Ort in den Einstellungen MUSS zur ersten Kategorie
  mit Hinweis führen (Spec 020 FR-014).

**Darstellung**

- **FR-013**: holzi MUSS ein Farbschema mit den Werten „Hell“, „Dunkel“ und
  „System“ anbieten; Standard ist „System“. Die Wahl MUSS sofort in der ganzen
  App gelten und bei „System“ dem Betriebssystem folgen. Alle Bedienelemente
  MÜSSEN in beiden Schemata lesbar sein.
- **FR-014**: Das Farbschema gilt für die Vault (FR-024). Das Setzen MUSS eine
  Aktion im Katalog von Spec 020 sein. Vor dem Entsperren einer Vault folgt
  holzi dem Betriebssystem.
- **FR-015**: „Darstellung“ MUSS in dieser Spec nur das Farbschema enthalten.
  Ein Hintergrund des Arbeitsbereichs (Bild oder Farbverlauf) gehört zur Spec
  Desktop-Symbole und Raster.

**Föderation**

- **FR-016**: Die App „Föderation“ MUSS entfallen: kein Launcher-Eintrag, kein
  Eintrag im Menü für neue Tabs, kein Öffnen als Fenster.
- **FR-017**: Die frühere Adresse der Föderation und jeder Aufruf, der die App
  „Föderation“ öffnen will, MÜSSEN die Einstellungen in der Kategorie
  „Föderation“ öffnen.
- **FR-018**: Der Sperr-Knopf der bisherigen Föderations-App entfällt ersatzlos;
  gesperrt wird weiter über den Chat und die künftige Tastenkürzel-Spec.
- **FR-022**: Die Kategorie „Föderation“ MUSS die bekannten Geräte der Vault
  zeigen: je Gerät einmal, mit Namen (oder „Unbenanntes Gerät“); dieses Gerät
  zuerst und markiert, die übrigen nach Namen. Die Liste ist nur eine Anzeige.
  Sie MUSS auch für Agenten mit Leserecht auf die Einstellungen abrufbar sein.
  „Zuletzt online“ je Gerät und die Live-Aktualisierung über den Sync kommen
  mit der Spec, die den iroh-Sync baut.

**Allgemein**

- **FR-019**: Die Aufteilung DARF keine Einstellung in ihrem Verhalten ändern
  (außer der Bedienung nach FR-021); alle Änderungen bleiben die bestehenden
  Aktionen aus Spec 020 und 022.
- **FR-020**: Die Kategorien, die Titel der Orte und die Beschreibungen der Zeilen MÜSSEN auf Deutsch
  und Englisch vorliegen.
- **FR-021**: Keine Einstellung DARF einen Knopf zum Speichern, Übernehmen oder
  Zurücksetzen haben. Auswahlen (Optionen, Auswahllisten, Schalter) MÜSSEN beim
  Wählen gespeichert werden; Textfelder beim Verlassen des Feldes und, wenn
  einzeilig, mit Enter. Eine ungültige Eingabe (etwa ein leerer Gerätename)
  wird nicht gespeichert, sondern am Feld erklärt. Eine Einstellung, die leer
  bleiben darf (etwa das Standard-Modell), bietet „Keins“ als wählbare Option
  statt eines Knopfs zum Zurücksetzen. Knöpfe bleiben nur für Handlungen, die
  etwas starten, und nicht für Werte (Anbieter verbinden, Modell herunterladen,
  Update prüfen, Modell löschen). Das Vorbild ist „Sitzung wiederherstellen“
  (Spec 022 FR-004).
- **FR-024**: Einstellungen MÜSSEN für die Vault gelten, auf jedem Gerät
  gleich: Farbschema, Sitzung wiederherstellen (ein oder aus), Autonomie für
  Delegaten, Verbotsregeln, Berechtigungsmodus des Chats und die Aufwandsstufe
  je Modell. Eine Wahl zwischen
  „Dieses Gerät“ und „Alle Geräte“ DARF es nicht geben. Nur für dieses Gerät
  gelten Gerätename, Standard-Modell und Spracherkennungsmodell. Werte, die
  frühere Versionen für ein Gerät bzw. für die Vault gespeichert haben, werden
  beim Öffnen der Vault einmal übernommen: ein Gerätewert wird zum Vault-Wert,
  wenn die Vault noch keinen hat; beim Standard-Modell wird ein Vault-Wert zum
  Wert dieses Geräts, wenn es noch keinen hat. Danach sind die alten Werte
  gelöscht.
- **FR-023**: Die Werkzeugleiste MUSS eine Suche haben: Ein Such-Symbol klappt
  dort ein Suchfeld auf. Es findet Orte über Titel, Beschreibung und
  hinterlegte Suchbegriffe und einzelne Einstellungen über ihre Bezeichnung,
  ohne Rücksicht auf Groß- und Kleinschreibung und Akzente; jedes Wort der
  Eingabe muss vorkommen. Treffer erscheinen in der Seitenleiste statt der
  Kategorien (sie wird dafür eingeblendet), zeigen Bezeichnung und Pfad (etwa
  „Modelle › Modelle herunterladen“) und führen per Klick oder Enter (erster
  Treffer) an den Ort; danach ist die Suche geschlossen. Escape oder der
  Knopf im Feld leeren es erst und schließen es dann. Orte mit Parametern (ein
  HuggingFace-Repo) sind keine Treffer. Suchbegriffe liegen auf Deutsch und
  Englisch vor (FR-020).

### Key Entities

- **Kategorie**: ein Eintrag der Seitenleiste mit Kennung, Symbol, Titel,
  Beschreibung und entweder direktem Inhalt oder einer Übersicht mit Bereichen.
  Hat einen Ort im Tab.
- **Bereich / Unteransicht**: ein Teil einer Kategorie mit eigenem Titel, eigener
  Beschreibung und eigenem Ort; kann weitere Unteransichten haben (etwa
  HuggingFace-Suche → Ergebnis → Dateiauswahl).
- **Farbschema**: Einstellung mit den Werten Hell, Dunkel, System; gilt für die
  Vault (FR-024), Standard System.
- **Gerät der Vault**: eine Installation von holzi, die die Vault geöffnet hat;
  Name, ob es dieses Gerät ist.

## Success Criteria _(mandatory)_

### Measurable Outcomes

- **SC-001**: Jede bisherige Einstellung ist von den geöffneten Einstellungen aus
  mit höchstens drei Klicks erreichbar.
- **SC-002**: 100 % der bisherigen Einstellungen sind in genau einer Kategorie zu
  finden und verhalten sich wie vorher (die bestehenden automatischen Prüfungen
  und manuellen Szenarien der betroffenen Specs bestehen).
- **SC-003**: Ein Kategoriewechsel und das Öffnen einer Unteransicht zeigen den
  neuen Inhalt in höchstens 100 ms (bei geladenen Daten).
- **SC-004**: Bei einer Fensterbreite von 360 px sind alle Kategorien erreichbar
  und alle Einstellungen ohne waagerechtes Scrollen bedienbar.
- **SC-005**: Ein Wechsel des Farbschemas ist in allen offenen Fenstern in
  weniger als einer Sekunde sichtbar.
- **SC-006**: Kein Weg in holzi öffnet mehr eine eigene Föderations-App.
- **SC-007**: Jedes Quickstart-Szenario, das weder Netz noch ein zweites Gerät
  noch eine Zeitmessung braucht, läuft als End-to-End-Test gegen die gebaute
  App und besteht.

## Assumptions

- Die Kategorie-Namen „Allgemein“, „Darstellung“, „Modelle“, „Agenten“,
  „Föderation“ sind Arbeitstitel; die Beschreibungen formuliert der Plan.
- Der Wechsel der Kategorie ist eine Navigation im Tab wie jeder andere Klick zu
  einer neuen Ansicht (Spec 020); das entspricht auch haex-vault 2.x, wo Zurück einen
  Kategoriewechsel rückgängig macht.
- Das Farbschema gilt wie alle Einstellungen außer Gerätename und Modellen für
  die Vault (FR-024); die ursprüngliche Annahme „Gerät vor Vault“ hat der
  Betreiber am 2026-09-26 verworfen.
- Eine Akzentfarbe, die Schriftgröße und eine Sprachwahl sind nicht Teil dieser
  Spec; haex-vault 2.x hat die ersten beiden auch nicht. holzi zeigt die Oberfläche
  heute immer auf Deutsch; die englischen Texte (FR-020) liegen trotzdem vor.
- Die zu große Komponente der HuggingFace-Modellverwaltung (Complexity Tracking
  aus Spec 020) wird bei der Aufteilung in Bereiche zerlegt; das ist eine Folge
  dieser Spec, keine eigene Anforderung.
- Spec 022 ist in `main`; diese Spec baut auf ihrer Einstellung „Sitzung
  wiederherstellen“ auf (Auswahl, die beim Wählen speichert).

## Nicht im Umfang

- Ein globales Zurücksetzen; die Suche (FR-023) durchsucht keine Inhalte von
  Listen (etwa installierte Modelle oder HuggingFace-Ergebnisse).
- Kategorien ohne heutigen Inhalt (siehe FR-006).
- Geräte umbenennen (außer diesem), entfernen, sperren oder ihren
  Synchronisationsstand zeigen; die Geräteliste ist nur eine Anzeige (FR-022).
- „Zuletzt online“ je Gerät und die Live-Aktualisierung der Geräteliste; beides
  kommt mit der Sync-Spec (iroh), weil holzi heute keinen Sync-Transport hat.
- Ein Hintergrund des Arbeitsbereichs (FR-015).
- Neue Einstellungen außer dem Farbschema.
- Tastenkürzel für einzelne Kategorien (kommt mit der Tastenkürzel-Spec).
