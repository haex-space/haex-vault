# Feature Specification: Zugangsdaten der Cloud-Speicher aus dem Passwortmanager wählen

**Feature Branch**: `052-storage-credentials-picker`

**Created**: 2026-10-14

**Status**: Draft

**Input**: User description: "die zugangsdaten für den cloud storage sollte entweder direkt aus dem
passwort manager herausgeholt werden oder darin gespeichert werden können" (Betreiber, 2026-10-14).
Ergänzungen des Betreibers: Beides soll gehen – vorhandene Einträge auswählen und neu eingegebene
Zugangsdaten sichtbar im Passwortmanager ablegen. Ein vorhandener Eintrag, den man verknüpft, bleibt ein
normaler Eintrag (keine zusätzliche Sperre für Erweiterungen und Agenten). „die werte sollten einen
passenden tag bekommen. haex-cloud-storage vielleicht“.

## Clarifications

### Session 2026-10-14

- Q: Was fehlt, wo es das Speichern im Passwortmanager schon gibt (038 FR-005)? → A: Beides:
  vorhandene Einträge im Verbindungsformular wählen können, und neu eingegebene Zugangsdaten als im
  Passwortmanager sichtbaren, auffindbaren und bearbeitbaren Eintrag ablegen, mit Hinweis im Formular
  und Sprung zum Eintrag.
- Q: Was passiert mit den Freigaben eines vorhandenen Eintrags, den man als Zugangsdaten verknüpft? →
  A: Nichts. Er bleibt ein normaler Eintrag und behält seine Freigaben; ein Aufrufer mit Freigabe darf
  ihn lesen wie jeden Eintrag. Für die von holzi selbst angelegten Einträge (Eigentümer `storage`)
  bleibt Regel Z14 aus Spec 034 unverändert.
- Q: Wie findet man diese Einträge im Passwortmanager? → A: Sie bekommen ein Tag, vorgeschlagen
  `haex-cloud-storage`.

## Begriffe

- **Zugangsdaten-Eintrag**: der Eintrag im Passwortmanager (Spec 034), aus dem holzi die Zugangsdaten
  einer Speicherverbindung liest. Er gehört einer von zwei Arten an:
  - **Von holzi angelegt**: entsteht, wenn der Nutzer im Formular neue Zugangsdaten eingibt. Titel
    „S3: …“, „WebDAV: …“ oder „SFTP: …“, Eigentümer `storage` (Spec 034 Z14, Spec 038 FR-005).
  - **Gewählt**: ein vorhandener Eintrag des Nutzers, den er im Formular auswählt. Er behält seinen
    Titel, seinen Ordner und seine Freigaben.
- **Verknüpfung**: der Verweis einer Speicherverbindung auf ihren Zugangsdaten-Eintrag. Die Werte
  werden nie in die Verbindung kopiert.
- **Cloud-Speicher-Tag**: das Tag, an dem man Zugangsdaten-Einträge im Passwortmanager erkennt;
  vorgeschlagener Name `haex-cloud-storage`.
- **Passende Einträge**: Einträge, die alle Werte enthalten, die das Protokoll der Verbindung braucht
  (S3: Access-Key-ID als Benutzername und Secret als Passwort; WebDAV: Benutzername und Passwort;
  SFTP: Benutzername und Passwort oder Benutzername und privater Schlüssel). Die Zuordnung von Feldern
  zu Werten ist dieselbe wie bei den von holzi angelegten Einträgen (Spec 038 data-model,
  Spec 050 data-model).

## Beziehung zu bestehenden Specs

- **Spec 038** (Speicherverbindungen) und **Spec 050** (WebDAV und SFTP): Diese Spec erweitert FR-005
  und FR-007 dort. Zugangsdaten MÜSSEN weiter im Passwortmanager liegen und nirgends sonst; neu ist,
  dass sie auch aus einem bereits vorhandenen Eintrag kommen können. Die Aussage „eine Erweiterung
  erreicht die Zugangsdaten nie“ (038 US3 Szenario 7, FR-005) gilt unverändert für von holzi angelegte
  Einträge und dafür, dass holzis Speicher-Funktionen Erweiterungen nie Zugangsdaten geben. Für
  gewählte Einträge gilt zusätzlich die Freigabe des Passwortmanagers wie für jeden Eintrag (siehe
  Klärung oben); 038 und 050 verweisen an den betroffenen Stellen auf diese Spec.
- **Spec 034** (Passwortmanager): Eigentümer, Regel Z14 und die Warnung vor dem Löschen eines Eintrags,
  den eine holzi-Funktion nutzt (FR-034), bleiben. Tags sind gewöhnliche Tags (es gibt keine reservierten
  Namen, Klärung in 034); das Cloud-Speicher-Tag ist nur eine Konvention von holzi.
- **Spec 024** (Sync): Verknüpfung und Einträge sind Vault-Daten und kommen auf alle eigenen Geräte;
  ein Eintrag, der noch nicht angekommen ist, zeigt „wird synchronisiert“ (038 Randfall).
- **Spec 023** (Einstellungen): Das Verbindungsformular bleibt ein bewusst bestätigtes Formular
  („Testen und speichern“, 038 FR-003), kein Speichern bei Auswahl.

## User Scenarios & Testing _(mandatory)_

### User Story 1 - Zugangsdaten aus einem vorhandenen Eintrag nehmen (Priority: P1)

Anna hat im Passwortmanager schon einen Eintrag „IDrive Zugang“ mit Access Key und Secret. Beim
Anlegen einer Speicherverbindung muss sie die Werte nicht noch einmal eintippen: Im Formular wählt sie
statt „Neue Zugangsdaten“ die Option „Eintrag aus dem Passwortmanager“, sucht den Eintrag in der Liste
passender Einträge und wählt ihn. holzi testet die Verbindung mit diesen Werten und speichert, wie sonst
auch, erst nach bestandenem Test. Die Werte werden nicht kopiert; die Verbindung verweist auf den
Eintrag.

**Why this priority**: Das ist die eigentliche Lücke: Heute müssen Zugangsdaten, die schon im
Passwortmanager liegen, erneut getippt werden, und es entstehen doppelte Einträge.

**Independent Test**: Einen Eintrag mit Benutzername und Passwort im Passwortmanager anlegen, im
Formular einer S3- oder WebDAV-Verbindung ihn wählen, „Testen und speichern“ ausführen: Die Verbindung
besteht, im Passwortmanager entsteht kein weiterer Eintrag, und ein Aufruf des Speichers nutzt die Werte
des Eintrags.

**Acceptance Scenarios**:

1. **Given** ein passender Eintrag im Passwortmanager, **When** Anna im Formular „Eintrag aus dem
   Passwortmanager“ wählt und ihn auswählt, **Then** testet holzi mit seinen Werten und legt nach
   bestandenem Test die Verbindung an, die auf diesen Eintrag verweist; im Passwortmanager entsteht
   kein neuer Eintrag.
2. **Given** das Formular für eine WebDAV-Verbindung, **When** Anna die Liste öffnet, **Then** zeigt sie
   nur passende Einträge (hier: mit Benutzername und Passwort), zuerst die mit dem Cloud-Speicher-Tag,
   mit Titel, Benutzername und Adresse, nie mit einem Geheimnis; eine Suche nach Titel, Benutzername
   und Adresse grenzt sie ein.
3. **Given** der Test mit den Werten des gewählten Eintrags scheitert (falsches Passwort, Server nicht
   erreichbar), **When** Anna speichern will, **Then** nennt holzi den Grund und speichert nichts (wie
   038 FR-003); sie kann einen anderen Eintrag wählen.
4. **Given** ein SFTP-Eintrag mit privatem Schlüssel in einem eigenen Feld und mit Passphrase, **When**
   Anna ihn für eine SFTP-Verbindung wählt, **Then** nutzt holzi Benutzername, Schlüssel und Passphrase
   des Eintrags wie bei einem von holzi angelegten.
5. **Given** ein Eintrag ohne die Werte, die das Protokoll braucht, **When** Anna die Liste öffnet,
   **Then** steht er nicht zur Auswahl; ein Hinweis nennt, dass nur passende Einträge erscheinen.
6. **Given** eine bestehende Verbindung mit eigenen oder gewählten Zugangsdaten, **When** Anna im
   Formular „Zugangsdaten tauschen“ wählt, **Then** kann sie einen anderen Eintrag wählen oder neue
   Werte eingeben; erst nach bestandenem Test ersetzt die neue Verknüpfung die alte.

---

### User Story 2 - Neue Zugangsdaten liegen sichtbar im Passwortmanager (Priority: P1)

Wenn Anna stattdessen neue Zugangsdaten im Formular eingibt, legt holzi sie wie bisher im
Passwortmanager ab, aber so, dass sie sie dort wiederfindet: Der Eintrag heißt „S3: IDrive“, trägt das
Tag `haex-cloud-storage`, zeigt die Adresse des Anbieters als URL, und sie kann ihn dort ansehen
(Geheimnis nur auf eigene Aktion), ändern, in einen Ordner legen und kopieren. Das Formular sagt es
ihr: „Gespeichert im Passwortmanager als ‚S3: IDrive‘“, mit einem Knopf „Eintrag öffnen“, der den
Eintrag im Passwortmanager zeigt.

**Why this priority**: Der Betreiber will, dass Zugangsdaten „darin gespeichert werden können“ und dort
erkennbar sind; heute weiß der Nutzer nicht, dass und wo sie liegen, und findet sie ohne Tag nicht
zwischen allen Einträgen.

**Independent Test**: Eine neue Speicherverbindung mit eingegebenen Zugangsdaten anlegen; im
Passwortmanager nach dem Tag filtern: Der Eintrag ist da, zeigt Titel, Benutzername, URL und lässt sich
ändern. Aus dem Formular „Eintrag öffnen“ wählen: Der Passwortmanager zeigt genau diesen Eintrag.

**Acceptance Scenarios**:

1. **Given** neue Zugangsdaten im Formular, **When** Anna bestanden getestet hat und speichert,
   **Then** liegt im Passwortmanager ein Eintrag mit dem Titel „<Protokoll>: <Anbietername>“, dem
   Cloud-Speicher-Tag, dem Benutzernamen, dem Geheimnis und der Adresse als URL; ein Geheimnis zeigt
   der Passwortmanager nur auf eine eigene Aktion (038 FR-005).
2. **Given** die Verbindung hat einen Zugangsdaten-Eintrag, **When** Anna das Formular öffnet,
   **Then** nennt es den Titel des Eintrags und bietet „Eintrag öffnen“ an; das Geheimnis zeigt das
   Formular nie.
3. **Given** Anna öffnet den Eintrag im Passwortmanager und ändert dort das Passwort, **When** sie
   danach den Speicher nutzt, **Then** gelten die neuen Werte (siehe US3).
4. **Given** zwei Verbindungen verwenden denselben Eintrag, **When** Anna eine von ihnen entfernt,
   **Then** bleibt der Eintrag; er wird erst gelöscht, wenn keine Verbindung ihn mehr nutzt und er von
   holzi angelegt wurde.
5. **Given** Zugangsdaten, die schon vor dieser Spec angelegt wurden, **When** holzi die Vault öffnet,
   **Then** tragen auch sie das Cloud-Speicher-Tag, ohne dass der Nutzer etwas tut; Titel und Werte
   bleiben unverändert.

---

### User Story 3 - Änderungen und Löschen wirken wie erwartet (Priority: P2)

Der Eintrag im Passwortmanager ist die einzige Quelle der Werte. Ändert Anna das Passwort dort, nutzt
holzi beim nächsten Zugriff das neue, auf allen ihren Geräten, ohne dass sie die Verbindung bearbeitet.
Löscht sie den Eintrag, warnt der Passwortmanager vorher und nennt die Speicher, die ihn nutzen; löscht
sie ihn trotzdem, zeigt die Verbindung „neue Zugangsdaten nötig“ und bietet, neue Werte einzugeben oder
einen anderen Eintrag zu wählen.

**Why this priority**: Ohne das sind verknüpfte Einträge eine Kopie mit anderem Namen; mit diesem
Verhalten sind sie wirklich dieselbe Quelle. Das Verhalten steht größtenteils schon in 038; hier wird es
für gewählte Einträge gesichert.

**Independent Test**: Verbindung mit gewähltem Eintrag anlegen, im Passwortmanager das Passwort ändern
und den Speicher erneut testen (nutzt das neue); den Eintrag löschen (Warnung, dann „neue Zugangsdaten
nötig“); einen anderen Eintrag wählen (Verbindung läuft wieder).

**Acceptance Scenarios**:

1. **Given** eine Verbindung mit gewähltem Eintrag, **When** Anna im Passwortmanager Benutzername,
   Passwort oder Schlüssel ändert, **Then** nutzt holzi beim nächsten Aufruf die neuen Werte, auf jedem
   ihrer Geräte, sobald der Sync die Änderung gebracht hat.
2. **Given** ein Eintrag, den eine Verbindung nutzt, **When** Anna ihn im Passwortmanager löschen will,
   **Then** warnt der Passwortmanager vorher und nennt die Speicherverbindungen (034 FR-034); das
   Löschen bleibt erlaubt.
3. **Given** der Eintrag ist gelöscht oder im Papierkorb, **When** der Speicher genutzt wird, **Then**
   scheitert der Aufruf ohne Details der Zugangsdaten, und die Einstellungen zeigen am Speicher „neue
   Zugangsdaten nötig“ mit „Anderen Eintrag wählen“ und „Neue Zugangsdaten eingeben“ (038 Randfall).
4. **Given** der Eintrag ist auf diesem Gerät noch nicht angekommen, **When** der Speicher genutzt
   wird, **Then** zeigt holzi „wird synchronisiert“, nicht „neue Zugangsdaten nötig“ (wie 038).
5. **Given** Anna ändert den Eintrag so, dass ihm ein Wert fehlt, den das Protokoll braucht (zum Beispiel
   Passwort geleert), **When** der Speicher genutzt wird, **Then** gilt er als „neue Zugangsdaten nötig“.

---

### User Story 4 - Auch im Fenster für Speichervorschläge von Erweiterungen (Priority: P3)

Schlägt eine Erweiterung einen Speicher vor (038 US3, 050 US5), erscheint das Fenster von holzi über der
App, in dem der Nutzer die Zugangsdaten eingibt. Dort kann er ebenfalls „Eintrag aus dem
Passwortmanager“ wählen. Die Erweiterung erfährt weiterhin nur, dass der Speicher angelegt wurde, nie
welcher Eintrag oder welche Werte.

**Why this priority**: Der gleiche Weg wie im Formular, nur an einer zweiten Stelle; die Basis (US1)
funktioniert ohne ihn.

**Independent Test**: Die Test-Erweiterung ruft „Speicher hinzufügen“ ohne Zugangsdaten auf; im Fenster
von holzi einen vorhandenen Eintrag wählen: Speicher angelegt, Erweiterung bekommt weder Eintrag noch
Werte.

**Acceptance Scenarios**:

1. **Given** ein Speichervorschlag einer Erweiterung, **When** Anna im Fenster von holzi einen
   vorhandenen passenden Eintrag wählt und bestätigt, **Then** testet holzi mit seinen Werten, legt
   Verbindung und Speicher an und erteilt die Berechtigung; die Erweiterung erhält keinen Hinweis auf den
   gewählten Eintrag.
2. **Given** derselbe Aufruf mit mitgeschickten Zugangsdaten oder einer Eintrags-Kennung, **When** holzi
   ihn bekommt, **Then** lehnt holzi ihn ab (038 US3 Szenario 4); eine Erweiterung kann nie einen
   Eintrag wählen.

---

### Edge Cases

- **Eintrag im Papierkorb gewählt** (Liste zeigt ihn nicht; ist er nach der Auswahl noch im Papierkorb
  gelandet): wie gelöscht (US3 Szenario 3).
- **Derselbe Eintrag für mehrere Verbindungen**: erlaubt; die Warnung beim Löschen nennt alle.
- **Eintrag eines anderen Protokolls** (zum Beispiel ein S3-Eintrag für WebDAV): steht zur Auswahl, wenn
  er die nötigen Werte hat; der Titel bleibt, wie er ist.
- **Entfernen einer Verbindung mit gewähltem Eintrag**: der Eintrag bleibt immer; nur von holzi
  angelegte Einträge ohne weitere Verbindung löscht holzi wie bisher (038 FR-007).
- **Verbindung wechselt von von-holzi-angelegtem auf gewählten Eintrag**: der bisherige von holzi
  angelegte Eintrag wird gelöscht, wenn keine andere Verbindung ihn nutzt und der Test mit dem neuen
  bestanden ist.
- **Cloud-Speicher-Tag vom Nutzer entfernt oder umbenannt**: die Verbindung funktioniert weiter; das
  Tag ist nur Auffindhilfe und wird bei einem neu angelegten Eintrag wieder gesetzt.
- **Gewählter Eintrag hat Freigaben für Erweiterungen oder Agenten**: bleibt, wie er ist (Klärung); das
  Formular weist vor der Auswahl nicht darauf hin, der Passwortmanager zeigt die Freigaben wie bei
  jedem Eintrag.
- **Gewählter Eintrag nicht lesbar** (zum Beispiel Eintrag wird gerade synchronisiert): holzi zeigt
  „wird synchronisiert“ und speichert erst, wenn der Test lief.
- **Vault ohne Passwortmanager-Daten** (neu angelegt): die Liste ist leer und erklärt, dass es noch keine
  passenden Einträge gibt; „Neue Zugangsdaten“ bleibt der Standard.

## Requirements _(mandatory)_

### Functional Requirements

- **FR-001**: Das Verbindungsformular MUSS für jedes Protokoll (S3, WebDAV, SFTP) zwei Wege für
  Zugangsdaten bieten: neue Werte eingeben (wie bisher) oder einen vorhandenen Eintrag des
  Passwortmanagers wählen. Für eine neue Verbindung ist „Neu eingeben“ vorgewählt.
- **FR-002**: Die Auswahl MUSS nur passende Einträge zeigen (siehe Begriffe), keine Einträge im
  Papierkorb, zuerst die mit dem Cloud-Speicher-Tag, und eine Suche nach Titel, Benutzername und Adresse
  bieten. Sie DARF kein Geheimnis zeigen.
- **FR-003**: Eine Verbindung MUSS auf ihren Zugangsdaten-Eintrag verweisen, nie dessen Werte kopieren
  oder zwischenspeichern, die über den Aufruf hinaus liegen. Bei jedem Aufruf liest holzi die aktuellen
  Werte des Eintrags.
- **FR-004**: Nach einer Änderung der Verknüpfung (neue Verbindung, anderer Eintrag, neue Werte) MUSS
  holzi vor dem Speichern testen und bei einem Fehler nichts speichern (038 FR-003, 050 FR-005).
- **FR-005**: Neu eingegebene Zugangsdaten MÜSSEN weiter als Eintrag mit Eigentümer `storage` im
  Passwortmanager liegen (Spec 034 Z14) und dürfen sonst nirgends in lesbarer Form gespeichert werden.
  Der Eintrag MUSS für den Nutzer im Passwortmanager sichtbar, durchsuchbar und änderbar sein.
- **FR-006**: Ein von holzi angelegter Eintrag MUSS das Cloud-Speicher-Tag tragen (vorgeschlagener Name
  `haex-cloud-storage`), den Titel „<Protokoll>: <Anbietername>“, den Benutzernamen und die Adresse als
  URL. Bereits vorhandene von holzi angelegte Einträge MÜSSEN das Tag beim Öffnen der Vault nachgetragen
  bekommen, ohne Titel oder Werte zu ändern. Ein gewählter Eintrag bekommt das Tag beim Verknüpfen
  ebenfalls, damit er im Passwortmanager als verwendet erkennbar ist; holzi ändert sonst nichts an ihm.
- **FR-007**: Das Formular MUSS zeigen, in welchem Eintrag die Zugangsdaten liegen (Titel), „Eintrag
  öffnen“ anbieten und nie ein Geheimnis zeigen (038 FR-005).
- **FR-008**: Ein gewählter Eintrag MUSS ein normaler Eintrag bleiben: holzi ändert weder Eigentümer
  noch Freigaben oder Ordner. Spec 034 Z14 und die Warnung vor dem Löschen (FR-034) gelten wie bisher.
- **FR-009**: Löschen oder Papierkorb eines verknüpften Eintrags MUSS die Verbindung auf „neue
  Zugangsdaten nötig“ setzen (ohne Details) und im Formular „Anderen Eintrag wählen“ und „Neue
  Zugangsdaten eingeben“ anbieten; fehlt er nur, weil er noch nicht angekommen ist, MUSS holzi „wird
  synchronisiert“ zeigen (038 FR-006, Randfall).
- **FR-010**: Fehlt einem verknüpften Eintrag nach einer Änderung ein Wert, den das Protokoll braucht,
  MUSS die Verbindung wie bei einem fehlenden Eintrag „neue Zugangsdaten nötig“ zeigen.
- **FR-011**: Eine Erweiterung DARF über die Funktionen für Speicher nie Zugangsdaten, Eintrags-Kennung
  oder Titel des Zugangsdaten-Eintrags erhalten; eine Wahl eines Eintrags durch eine Erweiterung MUSS
  abgelehnt werden (038 FR-005, US3 Szenarien 4 und 7). Ob eine Erweiterung oder ein Agent einen
  gewählten Eintrag über den Passwortmanager lesen darf, bestimmen allein dessen Freigaben (FR-008).
- **FR-012**: Beim Entfernen einer Verbindung MUSS holzi einen gewählten Eintrag immer behalten und
  einen von holzi angelegten nur löschen, wenn keine andere Verbindung ihn nutzt (038 FR-007).
- **FR-013**: Das Fenster für Speichervorschläge von Erweiterungen MUSS dieselbe Auswahl wie das Formular
  bieten (US4), ohne dass die Erweiterung etwas von der Wahl erfährt.
- **FR-014**: Verknüpfung und Tag MÜSSEN Vault-Daten sein und mit dem Datensync auf alle eigenen Geräte
  kommen (038 FR-006, Spec 024); gleichzeitige Änderungen enden überall gleich.
- **FR-015**: Die Auswahl und das Formular MÜSSEN auf Desktop und auf dem Telefon bedienbar sein
  (Spec 043), ohne neue Berechtigungen.
- **FR-016**: Alle neuen Texte MÜSSEN übersetzt sein; Begriffe wie in holzi üblich („Cloud-Speicher“,
  „Passwortmanager“, nie bloß „Relay“).

### Key Entities

- **Speicherverbindung**: Endpunkt, Protokoll, Anbieter und der Verweis auf ihren Zugangsdaten-Eintrag
  (038, 050); neu ist nur, dass der Verweis auch auf einen vom Nutzer gewählten Eintrag zeigen kann.
- **Zugangsdaten-Eintrag**: Eintrag im Passwortmanager mit den Werten der Zugangsdaten; entweder von
  holzi angelegt (Eigentümer `storage`) oder gewählt (normaler Eintrag).
- **Cloud-Speicher-Tag**: Tag zur Erkennung im Passwortmanager; ein gewöhnliches Tag, keine
  Zugriffsregel.

## Success Criteria _(mandatory)_

### Measurable Outcomes

- **SC-001**: Eine Verbindung mit Zugangsdaten aus einem vorhandenen Eintrag lässt sich anlegen, ohne
  ein einziges Zugangsdaten-Feld zu tippen (außer der Suche), in höchstens vier Schritten
  (Option wählen, Eintrag wählen, testen und speichern).
- **SC-002**: Nach dem Anlegen mit gewähltem Eintrag liegt im Passwortmanager kein zusätzlicher Eintrag
  (null Duplikate).
- **SC-003**: Jeder von holzi für eine Verbindung angelegte Eintrag ist im Passwortmanager mit einem
  einzigen Tag-Filter auffindbar; 100 % der bereits vorhandenen von holzi angelegten Einträge tragen das
  Tag nach dem ersten Öffnen der Vault.
- **SC-004**: Ändert der Nutzer Werte eines verknüpften Eintrags, nutzt der nächste Zugriff auf jedem
  Gerät die neuen Werte, ohne dass er die Verbindung bearbeitet.
- **SC-005**: Kein Geheimnis eines Zugangsdaten-Eintrags erscheint im Formular, in der Auswahl, in
  Fehlermeldungen oder Logs; eine Erweiterung erhält über die Speicher-Funktionen nie Zugangsdaten oder
  die Kennung des Eintrags.
- **SC-006**: Wer einen verknüpften Eintrag löscht, wird vorher gewarnt und sieht danach am Speicher
  „neue Zugangsdaten nötig“ statt eines unverständlichen Fehlers.

## Assumptions

- Das Cloud-Speicher-Tag heißt `haex-cloud-storage`; der Betreiber nannte den Namen als Vorschlag
  („vielleicht“) und kann ihn bei der Planung ändern.
- Mehrere Verbindungen dürfen denselben Eintrag nutzen; es gibt keine Warnung beim Wählen, aber beim
  Löschen (FR-012, 034 FR-034).
- Die Zuordnung von Eintragsfeldern zu Werten (Benutzername = Access-Key-ID oder Benutzername,
  Passwort = Secret oder Passwort, eigene Felder `sessionToken`, `privateKey`, `passphrase`) bleibt wie
  in Spec 038 und 050; ein anderer Eintrag muss diese Felder verwenden, um „passend“ zu sein.
- Einträge, die von holzi angelegt sind, bleiben für Erweiterungen und Agenten unsichtbar (Z14); nur
  für vom Nutzer gewählte, vorher schon vorhandene Einträge gilt allein deren Freigabe. Das ist eine
  bewusste Abweichung von 038 FR-005 für gewählte Einträge und wird dort vermerkt.
- Das Verknüpfen ändert den Eigentümer eines Eintrags nie; Z14 bleibt: kein Command und keine
  Bridge-Methode setzt oder ändert ihn. Ein gewählter Eintrag bleibt also ohne Eigentümer.
- Das Cloud-Speicher-Tag ist ein Tag wie jedes andere: Erteilt der Nutzer einer Erweiterung oder einem
  Agenten eine Freigabe für genau dieses Tag, erreicht sie die gewählten (nicht die von holzi
  angelegten) Einträge damit.
- Ein Eintrag mit Passkeys, TOTP oder Anhängen ist wählbar, wenn er die nötigen Werte hat; holzi liest
  nur die genannten Felder.
- Die Anzeige im Formular und die Auswahl übernehmen die Gestaltung der vorhandenen Einstellungen
  (Spec 023, Group/Row/OptionRow); es gibt kein neues Fenster.
- Nicht im Umfang: Zugangsdaten anderer holzi-Funktionen (E-Mail, ICS, Agenten-Modelle), Import von
  Zugangsdaten aus anderen Passwortmanagern, Rotation der Zugangsdaten beim Anbieter.
