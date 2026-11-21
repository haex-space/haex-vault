# Feature Specification: Sitzung wiederherstellen (wählbar)

**Feature Branch**: `022-session-restore`
**Created**: 2026-09-25
**Status**: Draft
**Input**: Betreiberentscheidung vom 2026-09-25 beim Test von Spec 020: Welche
Arbeitsbereiche, Fenster und Tabs offen sind, wird standardmäßig nicht mehr
über das Ende einer Vault-Session hinaus gespeichert. Wer seine Sitzung
behalten möchte, schaltet das in den Einstellungen ein, wahlweise für dieses
Gerät oder für die ganze Vault. Sitzungen, die frühere Versionen ungefragt
gespeichert haben, werden beim Update entfernt.

## Begriffe

- **Sitzung**: welche Arbeitsbereiche es gibt und in welcher Reihenfolge,
  welcher davon aktiv ist, welche Fenster in welchem Arbeitsbereich offen sind
  (mit Position, Größe, minimiert oder maximiert) und welche Tabs jedes Fenster
  hat (welche App, Reihenfolge, aktiver Tab), dazu für jeden Tab sein Ort und
  seine ganze Vor-/Zurück-Historie (Spec 020) samt Position darin und den Titeln
  der Einträge. Nicht dazu gehören Scrollpositionen und nicht abgeschickte
  Eingaben. Spec 015 hat bisher alles außer Ort und Historie bei jedem Neustart
  wiederhergestellt (FR-023 dort).
- **Gespeicherte Sitzung**: eine Sitzung, die in der Vault liegt, damit der
  nächste Start sie wiederherstellen kann. Sie gehört immer zu genau einem
  Gerät.
- **Vault-Session**: die Zeitspanne vom Entsperren bis zum Sperren oder
  Schließen einer Vault, gleichbedeutend mit einem App-Prozess (Spec 013,
  ADR-0003).

## Beziehung zu bestehenden Specs

- [`015-workspace-shell`](../015-workspace-shell/spec.md): User Story 5
  („Layout bleibt über Neustarts erhalten“) und FR-023 bis FR-025 gelten nur
  noch, wenn der Nutzer die Wiederherstellung eingeschaltet hat (FR-001 dieser
  Spec). Ohne diese Einstellung beginnt jede Vault-Session leer. Dasselbe gilt
  für den Neustart-Teil von User Story 7. Alle übrigen Anforderungen an
  Fenster, Tabs und Arbeitsbereiche innerhalb einer Vault-Session gelten
  unverändert.
- [`013-vault-lifecycle-isolation`](../013-vault-lifecycle-isolation/spec.md)
  und ADR-0003: Sperren und Schließen beenden die Vault-Session. Ohne
  Einstellung lebt die Sitzung genau so lange. Das passt zum
  Internet-Café-Gedanken aus Spec 013: Die Vault-Datei verrät dann nicht,
  welche Fenster zuletzt offen waren.
- [`002-onboarding-model-prefs`](../002-onboarding-model-prefs/spec.md): Die
  neue Einstellung folgt demselben Muster wie das Standardmodell. Es gibt einen
  Wert für die ganze Vault und optional einen Wert nur für dieses Gerät, der den
  Vault-Wert überschreibt.
- [`020-tab-navigation`](../020-tab-navigation/spec.md): Mit eingeschalteter
  Wiederherstellung werden Ort und Vor-/Zurück-Historie jedes Tabs mitgespeichert
  und wiederhergestellt, wie in einem Browser. Das ersetzt für diesen Fall die
  Entscheidung aus Spec 020, die Historie nie zu speichern (FR-011 dort); ohne
  Einstellung bleibt es dabei. Einträge, deren Ziel es nicht mehr gibt (etwa eine
  gelöschte Unterhaltung), behandelt Spec 020 wie bisher. Die Einstellung ist
  eine Aktion im Katalog von Spec 020 (FR-024 dort), wie jede andere Änderung
  einer Einstellung.
- [`004-chat-window-handling`](../004-chat-window-handling/spec.md): Ein
  wiederhergestellter Chat-Tab zeigt die Unterhaltung, an der er stand. Das ist
  kein neuer Chat-Einstieg im Sinne von Spec 004, der weiterhin mit einer neuen
  Unterhaltung beginnt.
- ADR-0001 (gerätebezogene Daten): Eine gespeicherte Sitzung bleibt
  gerätebezogen, auch wenn die Einstellung für die ganze Vault gilt. Jedes Gerät
  speichert und sieht nur seine eigene.
- Synchronisierung (haex-crdt): Gelöschte Einträge synchronisierter Daten
  hinterlassen einen Löschvermerk, der an andere Geräte weitergegeben und dort
  angewendet wird. Eine Löschung ist dort also nicht spurlos. Diese Spec
  verlangt deshalb, dass gespeicherte Sitzungen gar nicht erst an andere Geräte
  gehen (FR-010), und erlaubt für die schon synchronisierten Altdaten einen
  Löschvermerk ohne Inhalt (FR-011).
- Die geplante Spec zur **Einstellungs-App** (Seitenleiste mit Kategorien)
  übernimmt diese Einstellung in eine passende Kategorie. Bis dahin steht sie in
  der heutigen Einstellungsansicht.
- Tab-Inhalte sind nicht betroffen: Chat-Verlauf, Einstellungen, Modelle und
  alle anderen Daten der Apps bleiben wie bisher in der Vault.

## Clarifications

### Session 2026-09-26

- Q: Dürfen Agenten die Einstellung „Sitzung wiederherstellen“ ändern, oder bleibt sie wie die Leitplanken dem Nutzer vorbehalten? → A: Agenten dürfen sie ein- und ausschalten, wenn ihnen der Bereich für Geräteeinstellungen freigegeben ist (Spec 021); sie ist keine Leitplanke.
- Q: (Betreiber-Rückmeldung beim Testen) Braucht die Einstellungsansicht Knöpfe zum Übernehmen? → A: Nein. Die Ansicht ist eine einzige Auswahl „Aus / Nur auf diesem Gerät / Auf allen Geräten dieser Vault“, und die gewählte Option gilt sofort.
- Q: Soll ein wiederhergestellter Tab dort weitermachen, wo er zuletzt stand, oder an der Startansicht seiner App beginnen? → A: Wie im Browser: Ort und ganze Vor-/Zurück-Historie jedes Tabs samt Position und Titeln der Einträge; keine Scrollposition, keine nicht abgeschickten Eingaben.

## User Scenarios & Testing _(mandatory)_

### User Story 1 - Standardmäßig beginnt jeder Start leer (Priority: P1)

Ein Nutzer arbeitet mit mehreren Arbeitsbereichen und Fenstern, schließt HaexVault
und öffnet die Vault später wieder. Er hat nichts eingestellt. Er findet einen
einzigen, leeren Arbeitsbereich vor und öffnet die Apps, die er gerade braucht.

**Why this priority**: Das ist der neue Standard. Wiederhergestellte Fenster
wirken beim Start unaufgeräumt und verraten, woran zuletzt gearbeitet wurde.

**Independent Test**: Ohne die Einstellung einzuschalten zwei Arbeitsbereiche
anlegen, im zweiten ein Fenster mit zwei Tabs öffnen, HaexVault beenden und die
Vault erneut öffnen: Es gibt genau einen Arbeitsbereich, er ist aktiv und hat
kein Fenster. Die Vault enthält keine gespeicherte Sitzung.

**Acceptance Scenarios**:

1. **Given** die Wiederherstellung ist weder für dieses Gerät noch für die Vault
   eingeschaltet, **When** der Nutzer HaexVault beendet und die Vault erneut öffnet,
   **Then** zeigt HaexVault genau einen Arbeitsbereich ohne Fenster.
2. **Given** dieselbe Ausgangslage, **When** der Nutzer die Vault sperrt und
   wieder entsperrt, **Then** beginnt die neue Vault-Session ebenfalls leer.
3. **Given** dieselbe Ausgangslage, **When** die Vault-Session läuft, **Then**
   wird zu keinem Zeitpunkt eine Sitzung in der Vault gespeichert.
4. **Given** der Nutzer öffnet HaexVault über eine Adresse, die eine App an einem Ort
   öffnet (Spec 020), **When** HaexVault erscheint, **Then** ist genau diese App
   im einzigen Arbeitsbereich geöffnet, und sonst nichts.

---

### User Story 2 - Wiederherstellung einschalten (Priority: P1)

Ein Nutzer möchte auf seinem Laptop jeden Morgen dort weitermachen, wo er
aufgehört hat. Er öffnet die Einstellungen, schaltet „Sitzung wiederherstellen“
für dieses Gerät ein und arbeitet weiter. Beim nächsten Start sind seine
Arbeitsbereiche, Fenster und Tabs wieder da.

**Why this priority**: Ohne diesen Weg verlören Nutzer, die ihre Sitzung
behalten wollen, eine Fähigkeit, die sie heute haben.

**Independent Test**: Die Einstellung für dieses Gerät einschalten, zwei
Arbeitsbereiche mit Fenstern und Tabs einrichten, HaexVault beenden und die Vault
erneut öffnen: Das Verhalten entspricht Spec 015 User Story 5 (Arbeitsbereiche,
Fenster mit Position, Größe und Zustand, Tabs mit Reihenfolge und aktivem Tab,
zuletzt aktiver Arbeitsbereich), und jeder Tab steht am selben Ort mit derselben
Vor-/Zurück-Historie.

**Acceptance Scenarios**:

1. **Given** die Einstellungsansicht, **When** der Nutzer die Wiederherstellung
   für dieses Gerät einschaltet, **Then** gilt sie sofort: Die aktuelle Sitzung
   wird ab diesem Moment gespeichert, ohne Neustart.
2. **Given** die Wiederherstellung ist eingeschaltet, **When** der Nutzer die
   Vault erneut öffnet, **Then** erscheint die zuletzt gespeicherte Sitzung
   dieses Geräts gemäß Spec 015 User Story 5.
3. **Given** ein wiederhergestellter Tab, **When** HaexVault erscheint,
   **Then** steht er am selben Ort wie beim letzten Speichern und hat dieselbe
   Vor-/Zurück-Historie mit derselben Position darin: Ein Chat-Tab zeigt dieselbe
   Unterhaltung, Zurück führt zu denselben Ansichten wie vorher. Scrollposition
   und nicht abgeschickte Eingaben kommen nicht mit.
4. **Given** die Einstellungsansicht, **When** der Nutzer sie öffnet, **Then**
   ist genau eine der Optionen „Aus“, „Nur auf diesem Gerät“ und „Auf allen
   Geräten dieser Vault“ gewählt, passend zu dem, was auf diesem Gerät gilt.
5. **Given** die Einstellungsansicht, **When** der Nutzer eine andere Option
   wählt, **Then** ist sie sofort gespeichert, ohne weiteren Knopf.

---

### User Story 3 - Für alle Geräte oder nur für dieses (Priority: P2)

Eine Nutzerin verwendet ihre Vault auf ihrem Arbeitsrechner, ihrem Laptop und
gelegentlich auf fremden Rechnern. Sie schaltet die Wiederherstellung auf dem
Arbeitsrechner nur für dieses Gerät ein; der Laptop und fremde Rechner beginnen
weiter leer. Später schaltet sie sie für die ganze Vault ein, und jedes Gerät
behält seine eigene Sitzung. „Aus“ schaltet sie wieder für alle Geräte aus.

**Why this priority**: Wer mehrere eigene Geräte hat, soll die Einstellung nicht
auf jedem einzeln setzen müssen. Wer nur einem Gerät traut, soll sie auf dieses
Gerät beschränken können. Ein Gerät gezielt aus einer eingeschalteten Vault
herauszunehmen bietet die Ansicht nicht an (Betreiberentscheidung 2026-09-26:
„Aus“ heißt aus auf allen Geräten); die Aktionen können es weiterhin (AS2).

**Independent Test**: Die Einstellung für die Vault einschalten und die Vault
auf einem zweiten Gerät öffnen: Dort wird die Sitzung ebenfalls gespeichert,
aber getrennt. Auf dem zweiten Gerät „Aus“ wählen: Auf beiden Geräten beginnt
jeder Start leer. Auf dem ersten Gerät „Nur auf diesem Gerät“ wählen: Nur dieses
speichert.

**Acceptance Scenarios**:

1. **Given** die Wiederherstellung ist für die Vault eingeschaltet und für
   dieses Gerät nicht gesetzt, **When** der Nutzer die Vault auf diesem Gerät
   öffnet, **Then** gilt die Einstellung der Vault.
2. **Given** die Wiederherstellung ist für die Vault eingeschaltet und für
   dieses Gerät ausgeschaltet, **When** der Nutzer die Vault auf diesem Gerät
   öffnet, **Then** beginnt die Vault-Session leer, und es wird keine Sitzung
   gespeichert.
3. **Given** die Wiederherstellung ist für die Vault ausgeschaltet und für
   dieses Gerät eingeschaltet, **When** der Nutzer die Vault auf diesem Gerät
   öffnet, **Then** wird die Sitzung dieses Geräts wiederhergestellt.
4. **Given** zwei Geräte mit eingeschalteter Wiederherstellung, **When** jedes
   seine Vault öffnet, **Then** sieht jedes nur seine eigene Sitzung.
5. **Given** die Wiederherstellung ist für die Vault eingeschaltet und für
   dieses Gerät ausgeschaltet, **When** der Nutzer „Auf allen Geräten dieser
   Vault“ wählt, **Then** wird der Wert für dieses Gerät entfernt und der Wert
   der Vault gilt wieder.

---

### User Story 4 - Ausschalten entfernt die gespeicherte Sitzung (Priority: P1)

Ein Nutzer hatte die Wiederherstellung eingeschaltet und entscheidet sich um. Er
schaltet sie aus. Die bisher gespeicherte Sitzung verschwindet sofort aus der
Vault, die offenen Fenster bleiben, wie sie sind.

**Why this priority**: Wer die Wiederherstellung ausschaltet, erwartet, dass
nichts mehr liegen bleibt. Sonst hätte das Ausschalten nur eine halbe Wirkung.

**Independent Test**: Die Wiederherstellung einschalten, Fenster und Tabs
öffnen, sie wieder ausschalten: Die Fenster bleiben offen, die Vault enthält
keine gespeicherte Sitzung dieses Geräts mehr, und der nächste Start beginnt
leer.

**Acceptance Scenarios**:

1. **Given** die Wiederherstellung gilt auf diesem Gerät, **When** der Nutzer
   eine Änderung vornimmt, nach der sie auf diesem Gerät nicht mehr gilt,
   **Then** wird die gespeicherte Sitzung dieses Geräts sofort aus der Vault
   entfernt.
2. **Given** dieselbe Änderung, **When** sie wirksam ist, **Then** bleiben die
   offenen Arbeitsbereiche, Fenster und Tabs unverändert.
3. **Given** die Wiederherstellung wird für die Vault ausgeschaltet, **When**
   ein anderes Gerät, auf dem sie nur über die Vault galt, die Vault das nächste
   Mal öffnet, **Then** entfernt es seine gespeicherte Sitzung und beginnt leer.
4. **Given** die Wiederherstellung wird für die Vault ausgeschaltet, **When** ein
   Gerät sie für sich selbst eingeschaltet hat, **Then** speichert dieses Gerät
   weiter.

---

### User Story 5 - Ungefragt gespeicherte Sitzungen verschwinden beim Update (Priority: P1)

Ein Nutzer hat mit einer früheren HaexVault-Version gearbeitet, die seine Sitzung
ungefragt gespeichert hat. Nach dem Update öffnet er die Vault. Die alte Sitzung
erscheint nicht mehr, und die Vault enthält sie danach auch nicht mehr.

**Why this priority**: Der neue Standard ist „nicht speichern“. Ohne diesen
Schritt blieben alte Sitzungen als tote Information in der Vault liegen und
reisten mit der Vault-Datei weiter.

**Independent Test**: Eine Vault, in der eine frühere Version eine Sitzung
gespeichert hat, mit der neuen Version öffnen: HaexVault ist leer, und die Vault
enthält danach keine Inhalte früherer Sitzungen mehr, von keinem Gerät der
Vault.

**Acceptance Scenarios**:

1. **Given** eine Vault mit einer Sitzung, die eine frühere Version gespeichert
   hat, **When** der Nutzer sie mit dieser Version öffnet, **Then** erscheint
   HaexVault mit genau einem leeren Arbeitsbereich.
2. **Given** dieselbe Vault, **When** das Öffnen abgeschlossen ist, **Then**
   enthält die Vault keine Inhalte früherer Sitzungen mehr, auch nicht die
   anderer Geräte, die frühere Versionen dorthin synchronisiert haben.
3. **Given** die Bereinigung schlägt fehl, **When** HaexVault erscheint,
   **Then** startet sie trotzdem leer, die Vault bleibt nutzbar, und der Fehler
   wird protokolliert, ohne den Nutzer mit einem Fehlerbildschirm aufzuhalten.

---

### Edge Cases

- Der Nutzer beendet HaexVault hart (Absturz, Prozess beendet), während die
  Wiederherstellung eingeschaltet ist. Beim nächsten Öffnen erscheint die
  zuletzt gespeicherte Sitzung, höchstens mit den Änderungen der letzten
  Augenblicke vor dem Absturz verloren (wie in Spec 015).
- Derselbe Absturz bei ausgeschalteter Wiederherstellung: HaexVault ist leer wie
  nach einem normalen Beenden. Es gibt keinen Wiederherstellungsdialog.
- Der Nutzer schaltet die Wiederherstellung ein und sofort wieder aus. Danach
  liegt keine Sitzung in der Vault.
- Ein anderes Gerät hat den Vault-Wert geändert, während dieses Gerät eine
  Vault-Session offen hat. Die Änderung gilt auf diesem Gerät spätestens ab dem
  nächsten Öffnen; bis dahin gilt, was beim Öffnen galt. Die Einstellungsansicht
  zeigt schon den neuen Wert, also den, der ab dem nächsten Öffnen gilt.
- Ein wiederhergestellter Tab steht an einem Ort, den es nicht mehr gibt (eine
  gelöschte Unterhaltung, eine Ansicht, die diese Version nicht kennt). Es gilt
  Spec 020: Der Tab zeigt die Startansicht seiner App mit Hinweis, und Einträge
  gelöschter Unterhaltungen werden beim Vor- und Zurückgehen übersprungen.
- Die Einstellung lässt sich nicht lesen. HaexVault verhält sich wie bei
  ausgeschalteter Wiederherstellung und startet leer.
- Eine gespeicherte Sitzung ist nicht lesbar oder verweist auf eine unbekannte
  App. Es gilt Spec 015 FR-025: Start mit dem verwertbaren Rest oder leer, kein
  Fehlerbildschirm.
- Die Vault wird nach dem Update mit einer älteren HaexVault-Version geöffnet. Das
  ist nicht unterstützt (siehe Assumptions).

## Requirements _(mandatory)_

### Functional Requirements

**Einstellung**

- **FR-001**: HaexVault MUSS eine Einstellung „Sitzung wiederherstellen“ anbieten.
  Gilt sie auf einem Gerät, wird dort die Sitzung (siehe Begriffe) gespeichert
  und beim nächsten Öffnen wiederhergestellt, gemäß Spec 015 FR-023 bis FR-025
  und zusätzlich mit Ort und Vor-/Zurück-Historie jedes Tabs. Gilt sie
  nicht, DARF die Sitzung NICHT über das Ende der Vault-Session hinaus
  gespeichert werden.
- **FR-002**: Die Einstellung MUSS einen Wert für die ganze Vault und einen Wert
  nur für dieses Gerät haben können. Auf einem Gerät gilt der Gerätewert, falls
  gesetzt, sonst der Vault-Wert, sonst „aus“.

> **Seit Spec 023** (FR-024) hat die Einstellung nur noch einen Wert für die ganze Vault, ein Schalter „ein/aus“; die gespeicherte Sitzung selbst bleibt pro Gerät. Ein früherer Gerätewert wird beim Öffnen einmal zum Vault-Wert, wenn die Vault keinen hat, siehe [`023-settings-app`](../023-settings-app/spec.md).

- **FR-003**: Der Standard MUSS „aus“ sein: Eine neue Vault und eine
  aktualisierte Vault haben weder einen Vault- noch einen Gerätewert gesetzt.
- **FR-004**: Die Einstellungsansicht MUSS eine einzige Auswahl mit den
  Optionen „Aus“, „Nur auf diesem Gerät“ und „Auf allen Geräten dieser Vault“
  zeigen; die gewählte Option MUSS sofort gespeichert werden, ohne Knopf zum
  Übernehmen. Die Optionen setzen die beiden Werte so: „Auf allen Geräten“
  schaltet den Vault-Wert ein und entfernt den Gerätewert; „Nur auf diesem
  Gerät“ schaltet den Gerätewert ein und entfernt einen eingeschalteten
  Vault-Wert; „Aus“ entfernt beide Werte und schaltet die Wiederherstellung
  damit auf allen Geräten aus, die nicht selbst „Nur auf diesem Gerät“ gewählt
  haben. Einen ausgeschalteten Gerätewert bei eingeschaltetem Vault-Wert
  (US3 AS2) setzen nur die Aktionen; die Ansicht zeigt ihn als „Aus“.
- **FR-005**: Eine Änderung der Einstellung MUSS auf diesem Gerät sofort
  wirken, ohne Neustart: Beginnt sie zu gelten, wird die aktuelle Sitzung ab
  sofort gespeichert; hört sie auf zu gelten, gilt FR-007.
- **FR-006**: Das Setzen, Ändern und Zurücksetzen der Einstellung MUSS eine
  Aktion im Katalog von Spec 020 sein, mit derselben Wirkung aus Oberfläche,
  Tastenkürzel und für Agenten. Die Einstellung ist keine Leitplanke: Agenten
  dürfen sie ändern, sobald ihnen der Bereich für Geräteeinstellungen
  freigegeben ist (Spec 021).

**Entfernen gespeicherter Sitzungen**

- **FR-007**: Hört die Einstellung auf diesem Gerät auf zu gelten, MUSS die
  gespeicherte Sitzung dieses Geräts sofort aus der Vault entfernt werden. Die
  offenen Arbeitsbereiche, Fenster und Tabs bleiben unverändert.
- **FR-008**: Öffnet ein Gerät die Vault und die Einstellung gilt dort nicht,
  MUSS eine noch vorhandene gespeicherte Sitzung dieses Geräts entfernt werden,
  bevor HaexVault erscheint (zum Beispiel, weil ein anderes Gerät den Vault-Wert
  ausgeschaltet hat).
- **FR-009**: Beim ersten Öffnen einer Vault mit dieser Version MÜSSEN alle
  Sitzungen, die frühere Versionen gespeichert haben, entfernt werden, die
  aller Geräte der Vault.
- **FR-010**: Eine gespeicherte Sitzung DARF NICHT an andere Geräte
  weitergegeben werden, weder beim Speichern noch beim Entfernen. Entfernen
  MUSS sie vollständig aus der Vault-Datei dieses Geräts löschen, ohne einen
  Vermerk zu hinterlassen, der an andere Geräte geht.
- **FR-011**: Für Sitzungen, die frühere Versionen bereits an andere Geräte
  synchronisiert haben, DÜRFEN nach dem Entfernen keine Inhalte in der Vault
  bleiben (welche Apps offen waren, Fensterpositionen und -größen, Anzahl und
  Reihenfolge der Arbeitsbereiche). Ein Löschvermerk, den die Synchronisierung
  zum Weitergeben der Löschung braucht, ist zulässig, solange er nur die
  Kennung des gelöschten Eintrags trägt und keine dieser Inhalte.
- **FR-012**: Schlägt ein Entfernen fehl, MUSS HaexVault trotzdem normal
  starten beziehungsweise weiterlaufen. Die Vault MUSS nutzbar bleiben, der
  Fehler MUSS protokolliert werden, und das Entfernen MUSS beim nächsten Öffnen
  erneut versucht werden. Eine nicht entfernte Sitzung DARF bei ausgeschalteter
  Einstellung NICHT wiederhergestellt werden.

**Verhalten der Fensterverwaltung**

- **FR-013**: Ohne geltende Einstellung MUSS jede Vault-Session (Öffnen,
  Entsperren, Neustart nach Absturz) mit genau einem Arbeitsbereich ohne Fenster
  beginnen, es sei denn, der Start öffnet ausdrücklich eine App (FR-014).
- **FR-014**: Öffnet der Start eine App an einem Ort (Deep-Link, frühere
  Vollseiten-Adressen, Spec 020 FR-012), MUSS diese App erscheinen: ohne
  geltende Einstellung im einzigen Arbeitsbereich, mit geltender Einstellung
  zusätzlich zur wiederhergestellten Sitzung wie bisher.
- **FR-015**: HaexVault MUSS während der Vault-Session alle Fähigkeiten aus
  Spec 015 und 020 behalten, unabhängig von der Einstellung.
- **FR-016**: Die Dokumentation von Spec 015 MUSS bei User Story 5, FR-023 bis
  FR-025 und dem Neustart-Teil von User Story 7 vermerken, dass sie nur bei
  eingeschalteter Wiederherstellung gelten; die von Spec 020 MUSS bei FR-011
  vermerken, dass Ort und Historie bei eingeschalteter Wiederherstellung
  erhalten bleiben. Beide verweisen auf diese Spec.

### Key Entities

- **Einstellung „Sitzung wiederherstellen“**: Ein-/Aus-Wert, einmal für die
  Vault und optional je Gerät. Der geltende Wert eines Geräts ergibt sich aus
  FR-002. Die Einstellung selbst wird wie andere Einstellungen synchronisiert.
- **Gespeicherte Sitzung**: siehe Begriffe. Existiert nur, solange die
  Einstellung auf ihrem Gerät gilt, und verlässt dieses Gerät nie (FR-010).
- **Frühere Sitzungsdaten**: Von Versionen mit Spec 015 ungefragt gespeicherte
  und synchronisierte Sitzungen aller Geräte. Werden beim Update einmalig
  entfernt (FR-009, FR-011).

## Success Criteria _(mandatory)_

### Measurable Outcomes

- **SC-001**: Ohne geltende Einstellung zeigt HaexVault in 100 % der Starts,
  ob nach normalem Beenden, Sperren oder Absturz, genau einen Arbeitsbereich
  ohne Fenster (ohne Deep-Link).
- **SC-002**: Mit geltender Einstellung stellt HaexVault die Sitzung in 100 %
  der normalen Neustarts so wieder her, wie Spec 015 User Story 5 es
  beschreibt, und jeder Tab hat danach denselben Ort und dieselbe
  Vor-/Zurück-Historie wie beim letzten Speichern.
- **SC-003**: Nach dem Ausschalten und nach dem ersten Öffnen mit dieser Version
  findet eine Untersuchung der Vault-Datei keine Inhalte einer entfernten
  Sitzung mehr (höchstens Löschvermerke nach FR-011).
- **SC-004**: Eine gespeicherte Sitzung taucht in 0 % der Fälle auf einem
  anderen Gerät auf, auch nicht nach einer Synchronisierung.
- **SC-005**: Ein Nutzer findet die Einstellung und schaltet sie in weniger als
  30 Sekunden ein, ausgehend vom geöffneten Arbeitsbereich.
- **SC-006**: Das Öffnen einer Vault dauert höchstens so lange wie vorher. Die
  einmalige Bereinigung beim Update verlängert es um höchstens eine Sekunde.
- **SC-007**: Alle automatischen Prüfungen und die manuellen Szenarien von Spec
  015 und 020 bestehen, die Neustart-Szenarien aus Spec 015 mit eingeschalteter
  Wiederherstellung.

## Assumptions

- Ein Downgrade auf eine ältere HaexVault-Version nach dem Update ist nicht
  unterstützt, wie bei anderen Schemaänderungen auch.
- Mit eingeschalteter Wiederherstellung kommen Ort und Historie jedes Tabs
  zurück, aber keine Tab-Inhalte darüber hinaus: keine Scrollpositionen, keine
  Entwürfe. Das kann eine spätere Spec ergänzen, wenn Apps ihren Zustand melden
  können.
- Die Synchronisierung bietet eine Möglichkeit, Daten gerätelokal zu halten, so
  dass sie nicht weitergegeben werden. Wie FR-010 umgesetzt wird, klärt der
  Plan.
- Desktop-Symbole und das Raster auf dem Arbeitsbereich (geplante Folge-Spec)
  sind nicht Teil dieser Spec. Ob sie gespeichert werden, entscheidet die
  Folge-Spec.
- Der PR zu dieser Spec setzt auf dem PR zu Spec 020 auf (gestapelt), weil beide
  den Store der Fensterverwaltung ändern.

## Nicht im Umfang

- Mehrere benannte Sitzungen oder ein Sitzungsverlauf. Gespeichert wird genau
  eine Sitzung je Gerät.
- Ein Wiederherstellungsdialog nach Abstürzen.
- Änderungen an der Speicherung von Tab-Inhalten (Chat-Verlauf, Einstellungen,
  Modelle).
- Die Einordnung der Einstellung in eine Kategorie der künftigen
  Einstellungs-App.
