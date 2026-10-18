# Feature Specification: Onboarding-Härtung und Modellwahl-Persistenz

**Feature Directory**: `specs/002-onboarding-model-prefs`
**Created**: 2026-09-10
**Status**: Draft
**Input**: Grill-Session vom 2026-09-10; Ergebnisse in [ADR-0001](../../docs/adr/0001-device-scoped-data-convention.md) und [CONTEXT.md](../../CONTEXT.md)

> **Cross-reference:** Spec 004 (`specs/004-chat-window-handling`) supersedes
> this document's assumptions about Chat entry, Composer layout, and
> Reasoning visibility. The model fallback and persistence contract defined
> here remains authoritative.

## Clarifications

### Session 2026-09-10

- Q: Ist ein "Gerät aus Vault entfernen"-Vorgang Teil dieses Features? → A: Nein. US5/FR-017 bleiben als forward-looking Contract-Anforderung (FK-Kaskade sorgt für Konsistenz wenn Retirement später gebaut wird); keine UI/Backend-Command in dieser PR. Getestet wird die Kaskade selbst per Schema-/Storage-Test.
- Q: Was zeigt die Chat-Ansicht, während der Session-Resolver ein Modell lädt? → A: Explizit sichtbarer Ladezustand mit Modellname + kontextueller Beschriftung; erster CUDA-Load pro Modell bekommt eine eigene Meldung ("Optimiere GPU für erste Nutzung, ~30s") gemäß Etappe-0-Findung #4. Chat erst nach Fertigstellung interaktiv.
- Q: Was passiert, wenn der Nutzer im Wizard "später via Anbieter" wählt? → A: Kein Gerätestandard wird gesetzt (Wert bleibt unbeschrieben). Wizard schließt und Nutzer landet nicht im Chat, sondern in einer Workspace-/Landing-Ansicht mit persistentem FAB unten rechts. FAB öffnet den Chat als Overlay oder Route (Design im plan). Alle Einstellungen (inkl. "Als Standard setzen") leben in einem eigenen Settings-Screen wie in haex-vault 2.x (Legacy).
- Q: Was ist der Umfang der Workspace-Landing + FAB in diesem Feature? → A: Minimaler Workspace-Stub (leere Landing-Route mit Instanznamen und FAB unten rechts) + FAB öffnet die existierende Chat-Ansicht + Minimaler Settings-Screen mit "Als Standard setzen"-Toggle. Voller Workspace-Ausbau folgt in einem eigenen Feature.
- Q: Braucht dieses Feature Cross-device-Attribution auf Chat-Messages? → A: Nein. Chat-Sessions sind bewusst device-portabel — es ist irrelevant und für den Nutzer nicht interessant, auf welchem Gerät eine Message geschrieben wurde. `chat_messages` bekommt KEINE `vault_device_uuid`-Spalte, US4/FR-016 fallen weg. Der `alias` wird nur im Wizard, in `known_devices` und im Settings-Screen verwendet (dort als "Du bearbeitest gerade die Einstellungen auf 'MacBook Air'"-Kontext).
- Q: Wann genau schreibt der `chat.last_active_model_id` (Post-Analyze-Korrektur)? → A: AUSSCHLIESSLICH bei erfolgreichem `send_message`, NICHT bei manuellem Picker-Wechsel. Dropdown-Auswahl ohne Nachricht hat keinen Persistenz-Effekt — der Nutzer kann bedenkenlos verschiedene Modelle ausprobieren.
- Q: Wie werden nutzer-sichtbare Texte gehandhabt? → A: `@nuxtjs/i18n` für alle Frontend-Strings; Backend liefert strukturierte Daten (Enum-Werte + Parameter), keine lokalisierten Strings. Sprachen: `de` + `en` konsistent mit der App-Setup-Konvention.

## Kontext (Warum dieses Feature)

Die aktuelle Chat-Oberfläche hat zwei UX-Lücken, die sich beide besonders im Adoption-Fall (Vault-Datei auf neues Gerät kopieren, dort öffnen) zeigen:

1. Ein Nutzer, der eine bereits konfigurierte Vault auf ein zweites Gerät bringt, landet direkt auf einem halbfunktionalen Chat: die Anbietermodelle sind da (aus dem Sync), aber kein lokales GGUF liegt hier, kein Modell ist ausgewählt, und der Chat wirkt "kaputt" statt "muss noch eingerichtet werden". Der bestehende Onboarding-Katalog-Block versteckt sich, weil er "keine Modelle installiert UND keine api_key-Modelle" prüft.

2. Innerhalb eines Vaults auf einem Gerät gibt es kein persistentes Standardmodell. Beim erneuten Öffnen der App muss der Nutzer jedes Mal wieder ein Modell im Picker auswählen; hat er über mehrere Sessions dasselbe Modell benutzt, wird die App das nicht "wissen".

Zusätzlich fehlt eine menschliche Bezeichnung für die einzelnen Geräte eines Vaults — Cross-device-Anzeigen (welches Gerät hat wann was gemacht) müssten heute rohe UUIDs zeigen.

## User Scenarios & Testing _(mandatory)_

### User Story 1 - Gerätebenennung und Modell beim ersten Öffnen auf einem Gerät (Priority: P1)

Ein Nutzer öffnet eine Vault das erste Mal auf einem Gerät. Das kann eine ganz neue Vault sein (Genesis) oder eine kopierte, die er von einem anderen Gerät mitgebracht hat (Adoption). In beiden Fällen führt eine geführte Ersteinrichtung den Nutzer zuerst durch die Wahl eines Gerätenamens und optional durch die Modellwahl. Danach landet der Nutzer auf einer Workspace-/Landing-Ansicht mit persistentem Floating-Action-Button (FAB) unten rechts, über den der Chat aufgerufen wird.

**Why this priority**: Ohne dieses Story wirkt der Erst-Kontakt mit dem Vault auf einem Gerät entweder verwirrend (Adoption) oder benötigt manuelle Navigation zum verstreuten Panel (Genesis). Dieser Wizard ist die primäre Antwort auf die im Kontext genannten UX-Lücken 1 und Vorbereitung für 3 (Cross-device-Anzeigen).

**Independent Test**: Kann als geschlossener Slice getestet werden, indem eine Test-Vault einmal im Genesis-Modus und einmal via `cp` auf ein zweites Test-Gerät geöffnet wird. Beide Male muss der Wizard erscheinen, muss ein Pflichtfeld für den Gerätenamen haben und darf nur mit gesetztem Gerätenamen verlassbar sein. Nach Abschluss landet der Nutzer auf der Workspace-Landing.

**Acceptance Scenarios**:

1. **Given** eine Vault existiert nirgends, **When** der Nutzer sie neu anlegt und öffnet, **Then** wird der Nutzer auf eine eigenständige Onboarding-Seite geleitet, bekommt ein mit dem Betriebssystem-Hostname vorbefülltes Feld für den Gerätenamen (oder mit einem generischen Fallback wie "Neues Gerät", wenn der Hostname nicht ermittelbar ist) und drei Modellvorschläge, aus denen einer gewählt werden KANN. Der vorbefüllte Wert ist ein echter Eingabewert und darf unverändert bestätigt werden. Alternativ kann der Nutzer "später via Anbieter" wählen, ohne ein Modell zu setzen.
2. **Given** eine Vault liegt bereits auf Gerät A und wird als Datei auf Gerät B kopiert, **When** der Nutzer sie auf B das erste Mal öffnet, **Then** wird auf B derselbe Wizard gezeigt, obwohl auf A Anbietermodelle bereits konfiguriert sind — die Ersteinrichtung fragt trotzdem einen Gerätenamen für B ab und schlägt Modelle passend zu B's Hardware vor.
3. **Given** der Nutzer öffnet eine Vault, für die auf diesem Gerät bereits ein Name gesetzt wurde, **When** die App startet, **Then** wird der Wizard NICHT gezeigt und der Nutzer landet direkt auf der Workspace-Landing.
4. **Given** der Nutzer klickt "Abbrechen" oder verlässt die Wizard-Seite vor dem Abschluss des zweiten Schritts ohne Bestätigung, **When** er die App erneut öffnet, **Then** wird der Wizard erneut angezeigt, weil der Alias bis zum erfolgreichen Abschluss nicht persistiert wird.
5. **Given** der Nutzer schließt den Wizard mit "später via Anbieter" ab, **When** er auf der Workspace-Landing landet, **Then** ist der FAB unten rechts präsent und öffnet die Chat-Ansicht; kein Gerätestandard-Modell ist gesetzt.

---

### User Story 2 - Modellwahl bleibt zwischen Sessions erhalten (Priority: P1)

Ein Nutzer, der schon eingerichtet ist und regelmäßig chattet, will nach jedem App-Start dort weitermachen, wo er aufgehört hat: mit dem Modell, das er zuletzt tatsächlich benutzt hat. Manuelle Ausflüge zu einem anderen Modell zum reinen Ausprobieren sollen NICHT als "neues Standardmodell" hängenbleiben.

**Why this priority**: Entfernt die tägliche Reibung "welches Modell habe ich gestern benutzt?". Genauso wichtig wie Story 1 für die Gesamtnutzung — Story 1 regelt den Erst-Kontakt, Story 2 den täglichen.

**Independent Test**: Kann gesondert getestet werden, indem ein bereits eingerichteter Nutzer die App schließt und erneut öffnet, nach einem Modellwechsel + Nachricht die App schließt und erneut öffnet, sowie nach einem Modellwechsel ohne Nachricht die App schließt und erneut öffnet — das erwartete Verhalten unterscheidet sich pro Fall.

**Acceptance Scenarios**:

1. **Given** der Nutzer chattet mit Modell X, schließt die App, **When** er sie erneut öffnet, **Then** wird Modell X automatisch geladen mit einem sichtbaren, kontextuell beschrifteten Ladehinweis, ohne dass der Nutzer im Picker etwas anwählen muss.
2. **Given** der Nutzer wechselt kurz auf Modell Y, schickt keine Nachricht, schließt die App, **When** er sie erneut öffnet, **Then** wird nicht Y sondern das vorher tatsächlich genutzte Modell wieder geladen.
3. **Given** der Nutzer wechselt auf Modell Y und schickt eine Nachricht, schließt die App, **When** er sie erneut öffnet, **Then** wird Y geladen — die Nachricht war die Absichtsbekundung.
4. **Given** das zuletzt aktive Modell wurde entfernt (Modell deinstalliert, Anbieter gelöscht), **When** die App öffnet, **Then** fällt das System auf das nächste sinnvolle Modell zurück (siehe Story 3 für die genaue Kette), OHNE die persistente Einstellung stumm zu überschreiben.
5. **Given** ein lokal installiertes Modell wird auf diesem Gerät das erste Mal geladen (CUDA-JIT-Kalt-Cache), **When** der Session-Resolver es lädt, **Then** zeigt die Chat-Ansicht "Optimiere GPU für erste Nutzung von \<Modellname\>, dauert einmalig etwa 30 Sekunden…" statt eines generischen Spinners.

---

### User Story 3 - Expliziter Standard mit Gerät- oder Vault-Reichweite (Priority: P2)

Ein Nutzer, der einen langfristigen Standardanker setzen will ("das ist MEIN Modell auf diesem Gerät" oder "auf allen Geräten dieses Vaults"), kann das explizit tun. Dieser Anker gilt als Fallback, wenn das zuletzt aktive Modell mal nicht ladbar ist, und beim ersten Öffnen einer Vault auf einem Gerät, bevor Story 2 greift.

**Why this priority**: Eine Ebene über Story 2, wichtig für Nutzer mit mehreren Geräten und mehreren Anbieterkonten (z. B. lokales Modell auf Laptop, Claude auf Desktop). Weniger dringend als Stories 1+2, weil die Grundnutzung schon ohne funktioniert.

**Independent Test**: Kann eigenständig getestet werden, indem ein Nutzer im Config-Kontext explizit "Als Standard für dieses Gerät" oder "vault-weit" setzt und dann eine App-Neustart-Runde durchführt.

**Acceptance Scenarios**:

1. **Given** kein last_active-Modell und ein gerätespezifischer Standard ist gesetzt, **When** die App öffnet, **Then** wird der gerätespezifische Standard geladen.
2. **Given** kein gerätespezifischer Standard, aber ein vault-weiter Standard ist gesetzt, **When** die App auf einem Gerät ohne last_active öffnet, **Then** wird der vault-weite Standard geladen.
3. **Given** der Nutzer setzt "Modell Claude Opus 5, Standard für dieses Gerät", **When** die App auf demselben Gerät neu geöffnet wird und dieses Gerät nie gechattet hat, **Then** wird Claude Opus 5 geladen.
4. **Given** der Nutzer setzt einen gerätespezifischen Standard über einen bereits existierenden vault-weiten Standard, **When** die App startet, **Then** überschreibt der gerätespezifische Standard den vault-weiten (nur für dieses Gerät).

---

### User Story 4 - Gerätename im Settings-Kontext sichtbar (Priority: P3)

Der Nutzer erkennt in seinem Settings-Screen unmittelbar, welches Gerät er gerade konfiguriert. Der im Wizard vergebene Gerätename ist dort präsent und editierbar.

**Why this priority**: Vermeidet Verwirrung ("Setze ich das gerade für dieses Gerät oder ein anderes?"). Notwendig sobald ein Nutzer mehrere Geräte im Vault benutzt und in den Settings auf einem davon sitzt.

**Independent Test**: Kann getestet werden, indem der Settings-Screen geöffnet wird und der Gerätename dort erscheint. Ein Rename im Settings-Screen wird sofort übernommen.

**Acceptance Scenarios**:

1. **Given** der Nutzer hat im Wizard den Gerätenamen "MacBook Air" gesetzt, **When** er den Settings-Screen öffnet, **Then** wird "MacBook Air" als aktives Gerät benannt (z. B. "Einstellungen für: MacBook Air").
2. **Given** der Nutzer ändert im Settings-Screen den Gerätenamen von "MacBook Air" zu "Arbeitsrechner", **When** er die Änderung bestätigt, **Then** ist der neue Name sofort im Settings-Screen sichtbar und in `known_devices.alias` persistiert.

---

### User Story 5 - Bereitschaft für spätere Gerätehygiene (Priority: P3)

Dieses Feature liefert **keinen** Retire-Vorgang. Es sorgt aber dafür, dass ein zukünftiges "Gerät aus Vault entfernen" — wann immer es kommt — automatisch alle gerätespezifischen Einstellungen dieses Geräts mit-entfernt, ohne dass diese Retire-Aktion die Einstellungs-Tabelle explizit kennen muss. Zurück bleiben vault-weite Einstellungen und die Einträge anderer Geräte.

**Why this priority**: Vorbereitung für zukünftige Hygiene-/Datenschutz-Funktionalität, kein täglicher Nutzerpfad in diesem Feature. Wird als forward-looking Contract gebaut, nicht als sichtbares Verhalten.

**Independent Test**: Kann getestet werden, indem im Schema-/Storage-Test eine gerätespezifische Einstellung angelegt, dann die zugehörige Geräte-Registrierung gelöscht und geprüft wird, dass die Einstellungs-Zeile automatisch mit-verschwindet. Kein UI-Test in diesem Feature — die Retire-Aktion existiert noch nicht.

---

### Edge Cases

- **Erster Start ohne Netzwerk und ohne bereits konfigurierte Anbieter**: der Wizard muss weiterhin abschließbar sein — der Nutzer kann Modelle aus dem lokalen Katalog wählen (Download braucht Netzwerk, aber die Auswahl "ich richte später einen Anbieter ein" muss ohne Netz möglich sein).
- **Hostname-Fallback nicht ermittelbar**: der Wizard füllt den Alias mit einem generischen, lokalisierten Fallback vor, statt zu blockieren. Der Nutzer kann diesen Wert unverändert bestätigen.
- **Kollision beim Adoption-Sync**: zwei Geräte adoptieren dieselbe Vault parallel und schreiben unterschiedliche Werte für vault-weite Preferences. Der letzte gewinnt (LWW auf CRDT-Ebene) — beide Geräte einigen sich.
- **Zuletzt aktives Modell verweist auf ein Modell, das jetzt nicht mehr existiert (deinstalliert, Anbieter entfernt)**: die persistierte Einstellung wird nicht angefasst, der Session-Start greift automatisch auf die nächste Ebene der Fallback-Kette zurück. Wird das Modell später wieder verfügbar, funktioniert die alte Einstellung wieder.
- **Gerätespezifische Standardeinstellung ohne last_active auf einem frisch adoptierten Gerät**: die Fallback-Kette springt direkt auf den Standard, dann auf den Vault-Standard, dann auf erstes verfügbares — nie auf ein Modell, das auf diesem Gerät nicht ladbar ist.
- **Adoption der Vault während parallel eine Sync-Änderung läuft**: der Adoption-Pfad muss idempotent sein und darf keine Duplikate erzeugen (Details in ADR-0001).
- **Nutzer schließt Wizard-Fenster ohne Interaktion**: die Ersteinrichtung wird nicht abgeschlossen, der Wizard erscheint beim nächsten Öffnen erneut.

## Requirements _(mandatory)_

### Functional Requirements

**Onboarding**

- **FR-001**: Das System MUSS erkennen, ob eine Vault das erste Mal auf einem Gerät geöffnet wird — sowohl im Genesis-Fall als auch im Adoption-Fall (Vault-Datei wurde von woanders kopiert).
- **FR-002**: Bei erstem Öffnen auf einem Gerät MUSS das System den Nutzer auf eine eigenständige Onboarding-Ansicht führen, bevor der Arbeitsbereich zugänglich ist.
- **FR-003**: Die Onboarding-Ansicht MUSS ein Pflichtfeld für einen Gerätenamen bieten und einen sinnvollen Platzhalter vorschlagen (Betriebssystem-Hostname wenn ermittelbar, sonst ein generischer Fallback).
- **FR-004**: Der Nutzer MUSS die Ersteinrichtung nur abschließen können, wenn ein nicht-leerer Gerätename gesetzt wurde und der zweite Wizard-Schritt abgeschlossen ist. Die Modellwahl ist optional, aber "später via Anbieter" ist eine explizite Abschlussaktion; der Alias wird erst dann persistiert.
- **FR-005**: Die Onboarding-Ansicht MUSS drei hardware-passende Modellvorschläge aus dem Katalog präsentieren (leichteste Option, mittlere Option, Maximaloption), abgeleitet aus einem bestehenden Hardware-Fit-Klassifikator.
- **FR-006**: Die Onboarding-Ansicht MUSS eine Option "später via Anbieter" anbieten, mit der ein Nutzer ohne Wahl eines lokalen Modells fortfahren kann. In diesem Fall wird KEIN Gerätestandard-Modell geschrieben; das Feld bleibt unbeschrieben.
- **FR-007**: Nach Abschluss des Onboardings MUSS der Nutzer auf die Workspace-Landing geleitet werden. Wenn im Wizard ein Modell gewählt wurde, MUSS es als expliziter Gerätestandard hinterlegt sein; sonst nicht.

**Modellwahl-Persistenz**

- **FR-008**: Das System MUSS pro Gerät das zuletzt aktiv genutzte Modell erinnern und es beim nächsten Öffnen der Vault auf demselben Gerät automatisch laden.
- **FR-009**: Als "aktiv genutzt" gilt AUSSCHLIESSLICH das erfolgreiche Senden einer Nachricht mit dem aktuell geladenen Modell. Ein manueller Modellwechsel im Picker (Dropdown-Auswahl ohne anschließende Nachricht) wird NICHT als intentionaler Wechsel gewertet — die App wechselt zwar visuell zum gewählten Modell, `chat.last_active_model_id` bleibt aber auf dem letzten tatsächlich benutzten Modell stehen.
- **FR-010**: Ein rein passiver Auto-Load ohne Nutzerinteraktion (z. B. beim App-Start "erstes verfügbares nehmen") DARF die Erinnerung NICHT verändern.
- **FR-011**: Der Nutzer MUSS über einen Settings-Screen ein Modell explizit als "Standard für dieses Gerät" ODER "vault-weiter Standard" setzen können. Dieser Trigger lebt NICHT im Chat-Sidebar, sondern in einem eigenen Settings-Screen analog haex-vault 2.x.
- **FR-012**: Wenn ein expliziter Standard gesetzt ist, MUSS er den Auto-Fallback beeinflussen: gerätespezifischer Standard schlägt vault-weiten Standard, vault-weiter Standard schlägt "erstes verfügbares".

> **Seit Spec 023** (FR-024) gilt der Standard nur für dieses Gerät, weil Modelle pro Gerät installiert sind; der vault-weite Standard entfällt. Ein früherer Vault-Wert wird beim Öffnen einmal zum Wert dieses Geräts, wenn es keinen hat, siehe [`023-settings-app`](../023-settings-app/spec.md).

- **FR-013**: Wenn ein zuletzt aktiv genutztes Modell nicht mehr ladbar ist (z. B. Anbieter entfernt oder GGUF deinstalliert), MUSS das System den Fallback anwenden, ohne die Erinnerung stumm zu überschreiben. Wird das Modell später wieder verfügbar, MUSS es wieder automatisch geladen werden.

**Session-Start-Fallback-Kette**

- **FR-014**: Das System MUSS beim Session-Start die folgende Prüfkette anwenden, in dieser Reihenfolge, und beim ersten ladbaren Treffer aufhören: (1) Zuletzt aktives Modell dieses Geräts, (2) gerätespezifischer Standard dieses Geräts, (3) vault-weiter Standard, (4) erstes verfügbares Modell aus der aktuellen Modell-Liste, (5) Onboarding-Ansicht anzeigen wenn nichts verfügbar.
- **FR-015**: Nur Kette-Position (4) darf ohne persistente Erinnerungen ausgeführt werden — sie MUSS als "temporäre Wahl für diese Session" gelten und keine Erinnerungs-Einträge schreiben.

**Ladezustand während Session-Start**

- **FR-015a**: Während der Session-Resolver ein Modell lädt, MUSS die Chat-Ansicht einen sichtbaren Ladezustand mit Modellnamen anzeigen. Das Chat-Eingabefeld MUSS bereits sichtbar und editierbar sein, damit der Nutzer seine Frage formulieren kann; der Sendevorgang (einschließlich Enter) MUSS bis zur Fertigstellung der Ladung deaktiviert bleiben.
- **FR-015b**: Der Ladezustand MUSS kontextuell beschriftet sein. Vier semantische Kategorien: `connecting` (api_key-Modelle, Beispieltext: "Verbinde mit \<Anbietername\>…"), `loading` (lokale Warm-Loads, Beispieltext: "Lade \<Modellname\>…"), `cuda-jit-warmup` (erster CUDA-Load pro Gerät und Modell, Beispieltext: "Optimiere GPU für erste Nutzung von \<Modellname\>, dauert einmalig etwa 30 Sekunden…" — siehe Etappe-0-Findung #4), `ready` (Signal zum Ausblenden). Das Backend liefert die semantische Kategorie plus die für die Beschriftung nötigen Parameter (Modellname, Anbietername), das Frontend übersetzt via `@nuxtjs/i18n` in die aktive Sprache.
- **FR-015c**: Das System MUSS erkennen können, ob ein lokaler Modell-Load der "Erst-Load pro Gerät und Modell" ist, um die `cuda-jit-warmup`-Kategorie zu wählen. Diese Unterscheidung gilt vor allem für den CUDA-Pfad (JIT-Cache-Warmup als sichtbarer Kalt-Effekt); auf CPU-Only oder Metal-Builds fällt die Klassifizierung auf `loading` zurück, weil dort kein vergleichbarer Warmup-Effekt existiert.

**Alias-Sichtbarkeit**

- **FR-016**: Der Settings-Screen MUSS den aktuellen Gerätenamen prominent anzeigen (welches Gerät gerade konfiguriert wird) und einen Rename-Vorgang erlauben. Chat-Messages tragen KEINE device-Attribution — Chat-Sessions sind bewusst device-portabel; wo eine Message geschrieben wurde ist für den Nutzer irrelevant.

**Gerätehygiene**

- **FR-017**: Das Datenmodell dieses Features MUSS so aufgebaut sein, dass ein zukünftiger Retire-Vorgang (Löschen einer Geräte-Registrierung) automatisch alle gerätespezifischen Einstellungen dieses Geräts konsistent mit-entfernt, OHNE dass die Retire-Aktion die Einstellungs-Tabelle explizit kennen muss. Der vault-weite Standard und Einstellungen anderer Geräte bleiben unberührt. Dieses Feature stellt die Struktur bereit; die Retire-Aktion selbst ist nicht Teil dieses Features.

**Workspace-Landing und FAB**

- **FR-018a**: Nach abgeschlossenem Onboarding MUSS die Standard-Route nach Vault-Open eine Workspace-Landing sein, KEIN direkter Chat-Einstieg. Die Landing zeigt in diesem Feature mindestens den aktiven Instanznamen und einen persistenten Floating-Action-Button unten rechts.
- **FR-018b**: Der FAB MUSS von der Workspace-Landing aus den Chat aufrufen (als Overlay, Route oder anderes Layout — konkrete UI-Form ist Plan-Detail, nicht Spec-Detail). Der Chat bleibt in Funktionalität und Datenmodell unverändert; nur der Zugangs-Punkt wechselt.
- **FR-018c**: Ein Settings-Screen MUSS als eigene Route zugänglich sein (Aufruf aus der Workspace-Landing). In diesem Feature enthält der Settings-Screen mindestens den Trigger "Modell als Standard setzen" mit Scope-Auswahl (dieses Gerät / vault-weit), sowie die Möglichkeit den Gerätenamen zu ändern. Weitere Einstellungen sind Sache späterer Features.

**Nicht-Regression**

- **FR-018**: Bestehende Funktionalität — lokale Chat-Sessions, Anbieter-Chat, Streaming, Abbruch, Nachrichten-Persistenz — MUSS unverändert weiterlaufen. Dieses Feature ist eine Ergänzung, kein Ersatz.
- **FR-019**: Nutzer, die den Wizard bereits einmal abgeschlossen haben, DÜRFEN NICHT erneut mit dem Wizard konfrontiert werden, es sei denn sie öffnen die Vault auf einem NEUEN Gerät.

**Internationalisierung**

- **FR-020**: Alle nutzer-sichtbaren Texte dieses Features (Onboarding-Wizard-Labels, Workspace-Landing, Settings-Screen, Loading-Labels, Fehlermeldungen, Tier-Vorschlags-Chips) MÜSSEN über `@nuxtjs/i18n` internationalisiert werden. Backend-Commands und -Events liefern strukturierte Daten (Enum-Werte, IDs, Parameter), NIEMALS lokalisierte Strings. Alle Locale-Einträge werden in `de` und `en` gepflegt, konsistent mit der bestehenden App-Setup-Konvention.

### Key Entities _(include if feature involves data)_

- **Präferenz-Eintrag**: Eine Einstellung, die entweder für ein spezifisches Gerät oder vault-weit gilt. Trägt einen Namespace-Schlüssel (z. B. "chat.default_model_id") und einen Textwert. Vault-weite Einträge sind auf allen Geräten sichtbar, gerätespezifische Einträge gelten nur auf ihrem Gerät. Wird ein Gerät retiriert, gehen seine gerätespezifischen Einträge mit.
- **Gerätename (Alias)**: Ein menschenlesbarer Name für ein Gerät im Kontext einer Vault, im Wizard gesetzt und im Settings-Screen editierbar. Wird im Settings-Screen als Kontext-Anzeige verwendet ("Einstellungen für: MacBook Air"). Wird NICHT auf Chat-Messages angezeigt (Sessions sind device-portabel).
- **Installiertes Modell**: Ein lokal verfügbares Modell, dessen Existenz aus der Anwesenheit einer Datei im dafür bestimmten lokalen Verzeichnis abgeleitet wird — nicht mehr aus einer separaten Registrierungstabelle.

## Success Criteria _(mandatory)_

### Measurable Outcomes

- **SC-001**: Ein Nutzer, der eine Vault das erste Mal auf einem neuen Gerät öffnet, schließt den Wizard ohne externe Hilfe in unter 30 Sekunden ab.
- **SC-002**: Bei täglicher Nutzung findet ein Nutzer nie die Situation "welches Modell hatte ich zuletzt" — bei 100 aufeinanderfolgenden Öffnungen ohne bewussten Modellwechsel wird das gleiche Modell wie zuletzt automatisch geladen.
- **SC-003**: Ein Nutzer der den Settings-Screen öffnet erkennt in unter 3 Sekunden welches Gerät er gerade konfiguriert — der Gerätename ist prominent sichtbar; keine rohe interne Kennung erscheint an nutzersichtbarer Stelle in diesem Feature.
- **SC-004**: Nach einem `cp`-basierten Umzug einer Vault auf ein neues Gerät ist die neue Instanz innerhalb einer Wizard-Runde mit einem geladenen Modell chatfähig, ohne Rückgriff auf Dokumentation.
- **SC-005**: Ein Nutzer, der zum Ausprobieren ein anderes Modell im Picker wählt und die App schließt OHNE eine Nachricht zu senden, kehrt beim erneuten Öffnen zum vorherigen tatsächlich benutzten Modell zurück. Ausprobier-Klicks im Dropdown haben keinen Nachhall.
- **SC-006**: Bei einem Modell-Load der länger als drei Sekunden dauert weiß der Nutzer jederzeit welches Modell geladen wird — sichtbarer Text zeigt Modellname und Kontext; kein "die App hängt"-Eindruck entsteht selbst bei dem 30-Sekunden-CUDA-Erst-Load.

## Assumptions

- Die bestehende `known_devices`-Tabelle hat bereits ein nullbares `alias`-Feld, das für den Gerätenamen genutzt werden kann.
- Ein Hardware-Fit-Klassifikator existiert bereits im Backend (`hardware::fit`) und liefert für Katalog-Modelle eine Einschätzung `Fits` / `Tight` / `TooBig` / `Unknown`.
- Der Bootstrap-Prozess mintet bei erstem Öffnen auf einem neuen Gerät bereits eine frische Geräte-Identität — dieser Mechanismus wird nur zusätzlich um eine Sentinel-Zeile für "vault-weit" ergänzt, siehe ADR-0001.
- Die Modell-Auswahl im Wizard umfasst NUR lokale Katalog-Modelle plus die "später via Anbieter"-Option — Anbieter selbst werden in diesem Feature nicht hinzugefügt (Anbieter-Setup ist ein bestehender Flow).
- Der Nutzer öffnet die Vault immer über die App; direkte SQLite-Manipulation ist nicht Teil des betrachteten Nutzerpfads.
- Betriebssystem-Hostname ist auf gängigen Desktop-Plattformen zuverlässig ermittelbar; wenn nicht, ist der generische Fallback (z. B. "Neues Gerät") akzeptabel.
- Der schon geplante Config-Overlay-Screen für weitreichende Einstellungen (Modell + Effort + Thinking + Skills etc.) kommt als eigenes Folge-Feature — für dieses Feature reicht ein minimaler "Als Standard setzen"-Trigger in unmittelbarer Nähe des Model-Pickers.

## Explicit Non-Goals (out of scope)

- Cross-device-Attribution auf Chat-Messages ("dieser Chat lief auf …") — Sessions sind bewusst device-portabel; `chat_messages` bekommt KEINE device-Spalte. Der Alias erscheint nur im Wizard und im Settings-Kontext.
- Voller Workspace-Ausbau mit Widgets, Panels, Dashboard-Elementen — dieses Feature liefert nur einen minimalen Landing-Stub. Voller Ausbau ist ein eigenes Feature nach haex-vault-2.x-Vorbild.
- Vollständiger Settings-Screen — dieses Feature liefert nur den Rahmen plus zwei Kern-Trigger (Modell-Standard, Gerätename-ändern). Weitere Einstellungen kommen inkrementell.
- Vollständiges Config-Overlay im Stil einer Kommando-Palette (Modell + Effort + Thinking-Toggle + Skills etc.) — separates Folge-Feature.
- Neue Anbieter-Adapter (OpenAI, Google, Groq) — separates Folge-Feature nach dem etablierten Adapter-Muster.
- Tool-Calling-Infrastruktur und externe Recherche für kleine lokale Modelle — separates größeres Feature, das eigene Design-Runde braucht.
- Modellwechsel mitten im Gespräch mit sichtbarer Zuordnung, Kontextbudget-Vorprüfung, Absturz-/Sperren-Verhalten, OOM-Verhalten — verbleiben als Etappe-3-Rest wie im Projektplan geführt.
- Der Retire-Vorgang für Geräte (UI + Backend-Command "Gerät aus Vault entfernen") — dieses Feature liefert nur die Datenstruktur, die einen späteren Retire-Vorgang automatisch mit-bereinigt. Die Retire-Aktion selbst kommt als eigenes Feature.
