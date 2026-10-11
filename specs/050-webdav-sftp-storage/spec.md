# Feature Specification: WebDAV- und SFTP-Speicher neben S3

**Feature Branch**: `050-webdav-sftp-storage`

**Created**: 2026-10-11

**Status**: Draft

**Input**: User description: "WebDAV- und SFTP-Speicher als weitere Cloud-Speicher-Protokolle neben S3
(Spec 038). Erst WebDAV, dann SFTP. Beide sollen genauso behandelt werden wie S3-Speicher:
Verbindungen und Speicher in den Einstellungen, Zugangsdaten im Passwortmanager, Test vor dem
Speichern, Adressprüfung/keine Weiterleitungen/Zeitlimits, Dateibrowser, Erweiterungs-Berechtigungen,
verschlüsselte Ordner (048). Motivation: Anbieter ohne S3 (Hetzner Storage Box, Nextcloud, Koofr,
kDrive, NAS, eigene Server)."

## Begriffe

- **Protokoll** einer Speicherverbindung: S3 (Spec 038), WebDAV oder SFTP. Jede Verbindung hat genau
  eines; es lässt sich nach dem Anlegen nicht ändern.
- **Speicherverbindung**: wie in Spec 038, erweitert um WebDAV und SFTP. Bei WebDAV: die Adresse des
  Servers (einschließlich des Pfads, unter dem der Anbieter WebDAV anbietet) und die Zugangsdaten. Bei
  SFTP: Host, Port, der bestätigte **Hostschlüssel** und die Zugangsdaten.
- **Speicher**: bei S3 ein Bucket (Spec 038), bei WebDAV und SFTP ein **Ordner** auf dem Server,
  angegeben als Pfad unter der Verbindung; der Wurzelordner ist erlaubt. Name, Berechtigungen und
  Nutzung sind dieselben wie bei S3.
- **Zugangsdaten**: bei WebDAV Benutzername und Passwort (auch ein App-Passwort des Anbieters), bei
  SFTP Benutzername und entweder ein Passwort oder ein privater Schlüssel (optional mit Passphrase).
- **Hostschlüssel**: der öffentliche Schlüssel, mit dem sich ein SFTP-Server ausweist. holzi zeigt
  beim ersten Verbinden seinen Fingerabdruck; nach der Bestätigung des Nutzers gehört er zur
  Verbindung.
- **Objekt**, **Schlüssel**: wie in Spec 038. Bei WebDAV und SFTP ist ein Objekt eine Datei und ein
  Schlüssel ihr Pfad relativ zum Ordner des Speichers; Teile vor einem `/` sind Unterordner.
- **Verbindungstest**, **Bereich einer Erweiterung**, **Dialog von holzi**, **Berechtigung**: wie in
  Spec 038 und Spec 017.

## Beziehung zu bestehenden Specs

- **Grundsatz** (Spec 038): holzi spricht die Protokolle selbst und gibt sie über Berechtigungen an
  Erweiterungen und Agents weiter; niemand außer holzi und dem Passwortmanager sieht Zugangsdaten.
  Diese Spec setzt das für WebDAV und SFTP um.
- **Spec 038** (Speicherverbindungen): Diese Spec erweitert 038 um zwei Protokolle und hebt dort den
  Punkt „Andere Protokolle (… WebDAV, FTP)“ unter „Nicht im Umfang“ für WebDAV und SFTP auf. Alle
  Anforderungen von 038 gelten für die neuen Protokolle weiter, soweit diese Spec sie nicht
  ausdrücklich anpasst (FR-001 bis FR-017 dort, besonders Test vor dem Speichern, Zugangsdaten nur im
  Passwortmanager, Sync auf alle Geräte, Bereich je Erweiterung, Adressprüfung).
- **Spec 044** (Dateibrowser): Speicher aller Protokolle erscheinen dort gleich (US5 und FR-012,
  FR-025, FR-031a, FR-038 dort), auch für Agents mit Berechtigung je Speicher.
- **Spec 048** (verschlüsselte Ordner): Verschlüsselte Ordner MÜSSEN auch in WebDAV- und
  SFTP-Speichern funktionieren (FR-024). Was 048 vom Anbieter braucht (Auflisten, Teilbereiche lesen,
  Änderungszeit, eine Kennung für den Stand einer Datei, Kopieren), muss jedes Protokoll liefern.
- **Spec 017** (Erweiterungs-Host): Die Berechtigungsart `remoteStorage` gilt unverändert für alle
  Protokolle; die Aktion `add` hat bei SFTP den Host und Port des Servers als Ziel.
- **Spec 034** (Passwortmanager): verwahrt auch Passwörter, private Schlüssel und Passphrasen der
  neuen Protokolle.
- **Spec 024** (Sync der eigenen Geräte): Verbindungen, Speicher und bestätigte Hostschlüssel kommen
  mit dem Datensync auf alle eigenen Geräte.
- **Teilen** (Specs 027 bis 029 und ihr geplanter Umbau): WebDAV- und SFTP-Speicher sind nur Speicher
  der eigenen Vault, ohne jede Funktion zum Teilen mit anderen Vaults (Betreiber 2026-10-11). Spaces
  bleiben bei S3.

## Reihenfolge

Auf Wunsch des Betreibers erst WebDAV, dann SFTP: User Stories 1 bis 3 und 6 (WebDAV-Teil) bilden die
erste Stufe und sind allein nutzbar; User Stories 4 und 5 und der SFTP-Teil von Story 6 folgen. Die
erste Stufe löst holzi dabei von der Annahme, dass jeder Speicher S3 spricht (FR-018), sodass SFTP
danach nur noch ein weiteres Protokoll ist.

## User Scenarios & Testing _(mandatory)_

### User Story 1 - WebDAV-Speicher in den Einstellungen verbinden (Priority: P1)

Anna hat eine Hetzner Storage Box. In den Einstellungen wählt sie bei einer neuen Speicherverbindung
das Protokoll WebDAV, nimmt die Voreinstellung „Hetzner Storage Box“, trägt ihren Benutzernamen und
ihr Passwort ein und legt einen Speicher für den Ordner `holzi` an. holzi testet, bevor es etwas
speichert. Fehlt der Ordner, bietet holzi an, ihn anzulegen.

**Why this priority**: Ohne Verbindung gibt es nichts, das Dateibrowser, Erweiterungen oder Agents
nutzen könnten; WebDAV ist laut Betreiber das erste Protokoll.

**Independent Test**: Gegen einen lokalen WebDAV-Testserver eine Verbindung und einen Speicher anlegen,
testen, umbenennen, die Zugangsdaten tauschen, entfernen. Danach liegen keine Zugangsdaten mehr in der
Vault, und die Dateien auf dem Server sind unverändert.

**Acceptance Scenarios**:

1. **Given** gültige Zugangsdaten und ein erreichbarer Server, **When** Anna eine WebDAV-Verbindung mit
   einem Speicher anlegt, **Then** testet holzi den Speicher, speichert Verbindung und Speicher erst
   danach, und die Zugangsdaten liegen im Passwortmanager.
2. **Given** falsches Passwort, ein nicht erreichbarer Server, eine Adresse, unter der kein WebDAV
   antwortet, oder fehlende Schreibrechte, **When** Anna speichern will, **Then** nennt holzi den Grund
   verständlich und speichert nichts.
3. **Given** der Ordner des Speichers fehlt auf dem Server, **When** der Test das feststellt, **Then**
   meldet holzi „Ordner fehlt“ und bietet an, ihn anzulegen; erst nach Annas Bestätigung legt holzi
   ihn an und testet erneut.
4. **Given** eine gespeicherte WebDAV-Verbindung, **When** Anna einen zweiten Speicher (anderer Ordner)
   darauf anlegt, **Then** muss sie die Zugangsdaten nicht erneut eingeben.
5. **Given** eine Voreinstellung (etwa Hetzner Storage Box, Nextcloud, Koofr, kDrive), **When** Anna sie
   wählt, **Then** füllt holzi die Adresse mit dem Muster des Anbieters vor und zeigt, welchen Teil sie
   ergänzen muss (etwa ihren Benutzernamen).
6. **Given** eine Adresse mit `http`, **When** sie nicht im lokalen Netz oder auf dem eigenen Rechner
   liegt, **Then** lehnt holzi sie ab; im lokalen Netz ist sie erlaubt und als unverschlüsselt
   gekennzeichnet (wie Spec 038 FR-017).
7. **Given** ein auf dem Laptop angelegter WebDAV-Speicher, **When** der Datensync gelaufen ist,
   **Then** steht er auf dem Telefon mit Zugangsdaten bereit, ohne erneute Eingabe.

---

### User Story 2 - WebDAV-Speicher im Dateibrowser und für Agents (Priority: P1)

Anna öffnet ihren WebDAV-Speicher im Dateibrowser. Sie blättert durch die Ordner, schaut ein Video an
und springt darin, lädt Fotos hoch, benennt einen Ordner um, kopiert und löscht Dateien. Ein Agent mit
Berechtigung für diesen Speicher sucht und liest dort wie in einem S3-Speicher.

**Why this priority**: Der Dateibrowser ist der sichtbarste Nutzen eines Speichers; ohne ihn wäre
WebDAV nur für Erweiterungen da.

**Independent Test**: Gegen einen lokalen WebDAV-Testserver alle Szenarien von Spec 044 User Story 5
durchspielen (Ordner wechseln, Video abspielen und springen, hoch- und herunterladen, Upload
abbrechen) sowie umbenennen, verschieben, kopieren und löschen.

**Acceptance Scenarios**:

1. **Given** ein WebDAV-Speicher, **When** Anna ihn in der Seitenleiste wählt, **Then** zeigt holzi seine
   Ordner und Dateien mit Größe und Änderungszeit, auch leere Ordner.
2. **Given** ein Video im Speicher, **When** Anna es öffnet, **Then** beginnt die Wiedergabe, ohne dass
   holzi es ganz herunterlädt, und Springen funktioniert.
3. **Given** eine Datei auf dem Gerät, **When** Anna sie hochlädt und den Upload abbricht oder die
   Verbindung abreißt, **Then** liegt unter dem Zielnamen keine halbe Datei; eine dort schon vorhandene
   Datei bleibt unverändert.
4. **Given** ein Ordner mit vielen Dateien, **When** Anna ihn umbenennt, verschiebt oder kopiert,
   **Then** erledigt holzi das als einen Vorgang mit Fortschritt, und danach liegen alle Dateien am
   neuen Ort.
5. **Given** der Server ist nicht erreichbar oder das Passwort wurde geändert, **When** Anna den Speicher
   öffnet, **Then** nennt holzi den Grund und verweist auf die Einstellungen des Speichers.
6. **Given** ein Agent mit Leseberechtigung für den WebDAV-Speicher, **When** er dort sucht und liest,
   **Then** gelingt das wie bei einem S3-Speicher; ohne Berechtigung ist der Speicher für ihn nicht
   erreichbar (Spec 044 FR-031a).

---

### User Story 3 - Erweiterungen nutzen einen WebDAV-Speicher (Priority: P1)

Die Backup-Erweiterung aus Spec 038 bekommt Berechtigung für Annas WebDAV-Speicher. Sie lädt hoch,
listet, lädt herunter und löscht genau wie bei einem S3-Speicher, ohne zu wissen oder wissen zu müssen,
welches Protokoll dahintersteht.

**Why this priority**: Der Grundsatz „holzi spricht das Protokoll, Erweiterungen nutzen es“ ist der
Zweck der Speicher; vorhandene Erweiterungen sollen ohne Änderung auch WebDAV nutzen können.

**Independent Test**: Die Test-Erweiterung aus Spec 038 läuft unverändert gegen einen S3- und einen
WebDAV-Speicher und liefert in beiden dieselben Ergebnisse, einschließlich der Prüfungen von Bereich,
Schlüsseln und Grenzen.

**Acceptance Scenarios**:

1. **Given** eine Berechtigung „Lesen und Schreiben“ für einen WebDAV-Speicher, **When** die
   Erweiterung hochlädt, auflistet, herunterlädt und löscht, **Then** gelingt alles, und die Schlüssel,
   die sie sieht, sind die, die sie selbst geschrieben hat.
2. **Given** dieselbe Folge von Aufrufen gegen einen S3- und einen WebDAV-Speicher, **When** die
   Erweiterung sie ausführt, **Then** sind Ergebnisse und Fehlerarten gleich, auch beim Auflisten mit
   einem Präfix, das mitten in einem Namen endet.
3. **Given** zwei Erweiterungen auf demselben WebDAV-Speicher, **When** beide auflisten, **Then** sieht
   jede nur ihren Bereich, auch über Schlüssel mit `..`, führendem `/` oder Teilen, die der Server
   anders deuten könnte.
4. **Given** eine Erweiterung mit Leseberechtigung, **When** sie die Speicher auflistet, **Then** sieht
   sie je Speicher Kennung, Art (`s3`, `webdav` oder `sftp`), Namen des Speichers, Namen des Anbieters
   und den Bucket bzw. Ordner, nie Adresse, Host, Port oder Zugangsdaten.

---

### User Story 4 - SFTP-Speicher verbinden (Priority: P2)

Anna verbindet dieselbe Storage Box zusätzlich per SFTP. Sie wählt das Protokoll SFTP und die
Voreinstellung (Port 23), meldet sich mit einem privaten Schlüssel an und bestätigt beim ersten
Verbinden den Fingerabdruck des Hostschlüssels, den sie mit der Angabe des Anbieters vergleicht. Ein
Freund verbindet seinen eigenen Server mit Passwort.

**Why this priority**: SFTP erreicht eigene Server, NAS und Anbieter, die kein WebDAV anbieten; es folgt
laut Betreiber nach WebDAV.

**Independent Test**: Gegen einen lokalen SFTP-Testserver eine Verbindung mit Passwort und eine mit
Schlüssel anlegen, den Hostschlüssel bestätigen, testen, entfernen; danach den Hostschlüssel des
Servers austauschen und prüfen, dass holzi die Verbindung verweigert.

**Acceptance Scenarios**:

1. **Given** Host, Port, Benutzername und Passwort oder privater Schlüssel, **When** Anna die Verbindung
   zum ersten Mal testet, **Then** zeigt holzi den Fingerabdruck des Hostschlüssels und verbindet erst
   nach ihrer Bestätigung weiter; lehnt sie ab, wird nichts gespeichert.
2. **Given** eine gespeicherte SFTP-Verbindung, **When** der Server später einen anderen Hostschlüssel
   zeigt, **Then** bricht holzi jeden Aufruf ab, bevor Zugangsdaten gesendet werden, und zeigt am
   Speicher eine deutliche Warnung. Nur Anna kann in den Einstellungen den neuen Schlüssel nach
   Vergleich des Fingerabdrucks annehmen.
3. **Given** ein privater Schlüssel mit Passphrase, **When** Anna ihn einträgt, **Then** liegen Schlüssel
   und Passphrase im Passwortmanager, und holzi zeigt den Schlüssel nirgends sonst im Klartext.
4. **Given** Anna hat noch keinen Schlüssel, **When** sie „Schlüssel erzeugen“ wählt, **Then** legt holzi
   ein neues Schlüsselpaar im Passwortmanager an und zeigt den öffentlichen Schlüssel zum Kopieren, damit
   sie ihn beim Anbieter hinterlegen kann.
5. **Given** falsche Zugangsdaten, ein nicht erreichbarer Host, ein fehlender Ordner oder fehlende
   Rechte, **When** Anna speichern will, **Then** nennt holzi den Grund und speichert nichts; beim
   fehlenden Ordner bietet holzi wie in US1 an, ihn anzulegen.
6. **Given** eine SFTP-Verbindung auf dem Laptop, **When** der Datensync gelaufen ist, **Then** steht sie
   auf dem Telefon mit Zugangsdaten und bestätigtem Hostschlüssel bereit.

---

### User Story 5 - SFTP-Speicher im Dateibrowser, für Agents und Erweiterungen (Priority: P2)

Alles, was User Story 2 und 3 für WebDAV festlegen, gilt für SFTP-Speicher genauso.

**Why this priority**: Folgt aus US4; ohne Nutzung wäre die Verbindung wertlos.

**Independent Test**: Die Tests von US2 und US3 laufen unverändert gegen einen SFTP-Speicher und
liefern dieselben Ergebnisse wie gegen S3 und WebDAV.

**Acceptance Scenarios**:

1. **Given** ein SFTP-Speicher, **When** Anna oder ein Agent oder eine Erweiterung ihn nutzt, **Then**
   gelten alle Szenarien von US2 und US3 unverändert.
2. **Given** ein Kopiervorgang innerhalb eines SFTP-Speichers, **When** der Server kein Kopieren beim
   Server anbietet, **Then** kopiert holzi über sich selbst, mit Fortschritt und ohne halbe Datei am
   Ziel.

---

### User Story 6 - Eine Erweiterung stößt einen WebDAV- oder SFTP-Speicher an (Priority: P2)

Eine Erweiterung bietet an, einen neuen Speicher einzurichten, und schlägt Protokoll, Name, Anbieter,
Adresse bzw. Host und Port sowie Ordner vor, oder einen neuen Ordner auf einer schon verbundenen
Verbindung. Wie in Spec 038 User Story 3 bestätigt Anna im Dialog von holzi und gibt Zugangsdaten nur
im Fenster von holzi über der ganzen App ein; bei SFTP bestätigt sie dort auch den Hostschlüssel.

**Why this priority**: Gleichstand mit S3 (Wunsch des Betreibers); Anna kann denselben Speicher auch in
den Einstellungen anlegen und freigeben.

**Independent Test**: Die Test-Erweiterung schlägt je einen WebDAV- und einen SFTP-Speicher vor; im
Dialog bestätigen, im Fenster über der App Zugangsdaten eingeben (und den Hostschlüssel bestätigen):
Speicher angelegt, Berechtigung erteilt. Ein Vorschlag mit Zugangsdaten oder mit einem Hostschlüssel
wird abgelehnt, ohne dass ein Dialog erscheint.

**Acceptance Scenarios**:

1. **Given** ein Vorschlag für einen WebDAV- oder SFTP-Speicher, **When** Anna bestätigt, Zugangsdaten
   eingibt und der Test gelingt, **Then** legt holzi Verbindung und Speicher an, erteilt der
   Erweiterung „Lesen und Schreiben“ dafür und gibt ihr die Kennung zurück.
2. **Given** ein Vorschlag mit eigener Adresse oder eigenem Host, **When** die Erweiterung keine
   Berechtigung `remoteStorage` mit der Aktion `add` für diesen Host hat, **Then** fragt holzi nach
   Spec 017 oder lehnt ab (wie Spec 038 FR-009b).
3. **Given** ein Vorschlag enthält Zugangsdaten, einen privaten Schlüssel oder einen Hostschlüssel bzw.
   Fingerabdruck, **When** holzi den Aufruf erhält, **Then** lehnt holzi ihn ab, bevor ein Dialog
   erscheint; den Hostschlüssel bestätigt nur Anna anhand dessen, was der Server zeigt.
4. **Given** Abbruch oder gescheiterter Test, **When** der Vorgang endet, **Then** wird nichts angelegt,
   und die Erweiterung erfährt nur den Abbruch (wie Spec 038 FR-013b).

---

### User Story 7 - Verschlüsselte Ordner in WebDAV- und SFTP-Speichern (Priority: P3)

Anna legt in ihrem WebDAV-Speicher einen verschlüsselten Ordner nach Spec 048 an. Der Anbieter sieht
nur Dateien mit zufälligen Namen; Anna arbeitet im Dateibrowser damit wie in einem S3-Speicher.

**Why this priority**: Für Anbieter, denen man nicht vertraut, ist das der eigentliche Schutz; es hängt
aber vom Fortschritt von Spec 048 ab.

**Independent Test**: Die Akzeptanztests von Spec 048 laufen gegen einen WebDAV- und einen
SFTP-Speicher und liefern dieselben Ergebnisse wie gegen S3.

**Acceptance Scenarios**:

1. **Given** ein WebDAV- oder SFTP-Speicher und Spec 048 ist gebaut, **When** Anna dort einen
   verschlüsselten Ordner anlegt und nutzt, **Then** gelten alle Szenarien von Spec 048 unverändert.
2. **Given** zwei Geräte schreiben gleichzeitig in denselben verschlüsselten Ordner, **When** beide
   fertig sind, **Then** erkennt holzi den Konflikt wie bei S3 (Spec 048 FR-021).

### Edge Cases

- **Server folgt nur mit Weiterleitung**: holzi folgt keiner Weiterleitung (wie Spec 038); der Test
  meldet „Adresse antwortet nicht als WebDAV“ und nennt, dass der Server auf eine andere Adresse
  verweist, ohne Teile der Adresse zu nennen, die Zugangsdaten enthalten könnten.
- **Selbst signiertes Zertifikat** (etwa ein NAS im Heimnetz): wird wie in Spec 038 abgelehnt; im
  lokalen Netz kann Anna `http` nutzen. Ausnahmen für einzelne Zertifikate gehören nicht zu dieser Spec.
- **Server zeigt einen anderen Hostschlüssel**: siehe US4 Szenario 2; das gilt auch, wenn die Änderung
  über den Sync von einem anderen Gerät kommt, auf dem Anna einen neuen Schlüssel angenommen hat (dann
  gilt der synchronisierte Schlüssel).
- **Hostschlüssel kommt vor der Verbindung oder den Zugangsdaten an** (Sync noch nicht fertig): wie
  Spec 038 „wird synchronisiert“.
- **Abgebrochener Upload hinterlässt eine Zwischendatei** (etwa nach Absturz): holzi blendet seine
  Zwischendateien im Dateibrowser und für Erweiterungen aus und entfernt solche, die älter als 24
  Stunden sind, wenn es sie sieht.
- **Leere Ordner nach dem Löschen**: Löscht eine Erweiterung die letzte Datei eines Unterordners in
  ihrem Bereich, entfernt holzi die dadurch leeren Unterordner in ihrem Bereich, damit ihr Bereich sich
  wie bei S3 verhält; außerhalb ihres Bereichs entfernt holzi nichts.
- **Namen, die der Server nicht speichern kann** (zu lange Teile, Zeichen, die das Dateisystem des
  Servers verbietet, Groß-/Kleinschreibung, die der Server gleich behandelt): holzi lehnt einen solchen
  Schlüssel einer Erweiterung mit dem Fehler für ungültige Schlüssel ab, statt eine andere Datei zu
  treffen; im Dateibrowser nennt holzi den Grund.
- **Sehr viele Dateien in einem Ordner oder sehr tiefe Ordner**: Auflisten hält dieselben Grenzen wie
  bei S3 ein (Spec 038, Spec 044); ein Server, der Auflisten über alle Ebenen verweigert, wird Ordner
  für Ordner gelesen.
- **Server ohne Teilbereichs-Lesen** (Video springen): holzi meldet das einmal, spielt ab, sobald genug
  gelesen ist, und funktioniert weiter.
- **Server, der beim Kopieren oder Verschieben nicht überschreibt**: holzi erreicht dasselbe Ergebnis
  wie bei S3 (das Ziel wird ersetzt), ohne dass das Ziel zwischendurch fehlt oder halb ist, soweit der
  Server das zulässt; sonst meldet holzi den Fehler, bevor etwas verloren geht.
- **Kontingent beim Anbieter erschöpft**: Hochladen scheitert mit einem eigenen verständlichen Fehler;
  keine halbe Datei bleibt zurück.
- **Gleichzeitige Änderung derselben Datei von außen** (etwa per Nextcloud-Client): Es gilt, was zuletzt
  geschrieben wurde; verschlüsselte Ordner erkennen Konflikte nach Spec 048.

## Requirements _(mandatory)_

### Functional Requirements

**Verbindungen und Speicher**

- **FR-001**: Der Nutzer MUSS beim Anlegen einer Speicherverbindung das Protokoll wählen können: S3,
  WebDAV oder SFTP. Das Protokoll einer gespeicherten Verbindung DARF sich NICHT ändern lassen.
- **FR-002**: Eine WebDAV-Verbindung MUSS aus der Adresse des Servers (mit Pfad) und Benutzername und
  Passwort bestehen. Eine SFTP-Verbindung MUSS aus Host, Port, bestätigtem Hostschlüssel und
  Benutzername mit Passwort oder privatem Schlüssel (optional mit Passphrase) bestehen.
- **FR-003**: Ein WebDAV- oder SFTP-Speicher MUSS aus einem Namen und einem Ordnerpfad unter seiner
  Verbindung bestehen; der Wurzelordner ist erlaubt. Mehrere Speicher DÜRFEN sich eine Verbindung
  teilen. Zwei Speicher derselben Verbindung, deren Ordner ineinander liegen, MUSS holzi beim Anlegen
  als Warnung nennen.
- **FR-004**: holzi MUSS Voreinstellungen für verbreitete Anbieter anbieten, mindestens Hetzner Storage
  Box (WebDAV und SFTP), Nextcloud, Koofr und Infomaniak kDrive (WebDAV), sowie „Anderer Server“ für
  jedes Protokoll. Eine Voreinstellung füllt nur Felder vor; der Nutzer kann alles ändern.
- **FR-005**: Vor dem ersten Speichern und nach jeder Änderung von Adresse, Host, Port, Zugangsdaten
  oder Ordner MUSS holzi den Verbindungstest von Spec 038 (FR-003 dort) ausführen: ein eigenes
  Testobjekt im Ordner schreiben, lesen, auflisten und löschen. Die Gründe für ein Scheitern MÜSSEN
  zusätzlich „Adresse antwortet nicht als WebDAV“, „Ordner fehlt“ und „Hostschlüssel unbekannt oder
  geändert“ umfassen.
- **FR-006**: Meldet der Test „Ordner fehlt“, MUSS holzi anbieten, den Ordner anzulegen, und DARF ihn
  nur nach Bestätigung des Nutzers anlegen. holzi DARF beim Anbieter sonst keine Ordner außerhalb
  seiner eigenen Test- und Zwischendateien anlegen oder löschen, außer auf eine Aktion des Nutzers oder
  einer berechtigten Erweiterung in ihrem Bereich.
- **FR-007**: Alle Zugangsdaten der neuen Protokolle (Passwort, privater Schlüssel, Passphrase) MÜSSEN
  im Passwortmanager liegen und DÜRFEN sonst nirgends in lesbarer Form gespeichert werden (wie Spec
  038 FR-005).
- **FR-008**: holzi MUSS auf Wunsch ein SSH-Schlüsselpaar erzeugen, den privaten Schlüssel im
  Passwortmanager ablegen und den öffentlichen Schlüssel zum Kopieren anzeigen. Ein vorhandener
  privater Schlüssel MUSS sich einfügen oder aus einer Datei einlesen lassen.
- **FR-009**: Verbindungen, Speicher und bestätigte Hostschlüssel MÜSSEN Vault-Daten sein und mit dem
  Datensync auf alle eigenen Geräte gelangen (wie Spec 038 FR-006). Das Ergebnis des letzten Tests
  bleibt je Gerät.
- **FR-010**: Beim Entfernen gilt Spec 038 FR-007 unverändert: Dateien und Ordner beim Anbieter bleiben.

**Sicherheit der Verbindung**

- **FR-011**: Für WebDAV MÜSSEN die Regeln von Spec 038 FR-017 gelten: Zertifikate werden geprüft,
  `http` nur im lokalen Netz oder auf dem eigenen Rechner und als unverschlüsselt gekennzeichnet,
  Link-Local-Adressen nie, Prüfung der Adresse vor jedem Aufruf, Verbindung genau zur geprüften
  Adresse, ein beim Festlegen öffentlicher Endpunkt nie zu einer lokalen Adresse. holzi DARF keiner
  Weiterleitung folgen.
- **FR-012**: Für SFTP MÜSSEN dieselben Regeln zu Adressen gelten (lokal oder öffentlich festgelegt,
  Link-Local nie, Prüfung vor jedem Aufruf, Verbindung genau zur geprüften Adresse). Statt eines
  Zertifikats MUSS holzi den Hostschlüssel prüfen: beim ersten Verbinden den Fingerabdruck zeigen und
  erst nach Bestätigung des Nutzers Zugangsdaten senden; bei einem anderen als dem bestätigten Schlüssel
  jeden Aufruf abbrechen, bevor Zugangsdaten gesendet werden. Einen neuen Hostschlüssel DARF nur der
  Nutzer in holzi annehmen, nie eine Erweiterung oder ein Agent.
- **FR-013**: Jeder Aufruf bei einem WebDAV- oder SFTP-Server MUSS dieselben Zeitgrenzen einhalten wie
  bei S3 (Spec 038, Spec 044); kein Aufruf DARF holzi blockieren.
- **FR-014**: WebDAV MUSS sich mit Benutzername und Passwort anmelden können, wie es verbreitete Server
  verlangen; ein Passwort DARF nie über `http` zu einem öffentlichen Endpunkt gehen (folgt aus FR-011).

**Nutzung: Dateibrowser, Agents, Erweiterungen**

- **FR-015**: WebDAV- und SFTP-Speicher MÜSSEN im Dateibrowser (Spec 044) alles können, was ein
  S3-Speicher dort kann: Ordner und Dateien zeigen (auch leere Ordner), Video und Audio mit Springen
  abspielen, hoch- und herunterladen mit Fortschritt und Abbruch, Ordner anlegen, umbenennen,
  verschieben, kopieren, löschen, suchen.
- **FR-016**: Ein abgebrochener oder gescheiterter Upload, ein Kopieren oder ein Verschieben DARF unter
  dem Zielnamen keine halbe Datei hinterlassen; eine dort vorhandene Datei MUSS bis zum vollständigen
  Erfolg unverändert bleiben.
- **FR-017**: Die neun Funktionen `extension_remote_storage_*` (Spec 038 FR-008) MÜSSEN für WebDAV- und
  SFTP-Speicher dieselben Ergebnisse, Fehlerarten, Bereiche, Schlüsselprüfungen und Grenzen liefern wie
  für S3-Speicher, sodass eine Erweiterung ohne Änderung jedes Protokoll nutzen kann. Das gilt auch für
  Auflisten mit einem Präfix, das mitten in einem Namen endet.
- **FR-017a**: Die Liste der Speicher für eine Erweiterung MUSS je Speicher die Art (`s3`, `webdav`,
  `sftp`) und statt des Buckets den Ordner nennen; Adresse, Host, Port, Hostschlüssel und Zugangsdaten
  DÜRFEN NICHT darin stehen (erweitert Spec 038 FR-009a).
- **FR-017b**: Eine Erweiterung MUSS einen WebDAV- oder SFTP-Speicher vorschlagen können wie einen
  S3-Speicher (Spec 038 FR-009b, FR-013, FR-013a, FR-013b): mit Protokoll, Name, Anbieter, Adresse bzw.
  Host und Port und Ordner, oder einen neuen Ordner auf einer vorhandenen Verbindung. Ein Vorschlag mit
  Zugangsdaten, privatem Schlüssel, Passphrase, Hostschlüssel oder Fingerabdruck MUSS abgelehnt werden,
  bevor ein Dialog erscheint. Die Aktion `add` hat bei WebDAV den Host der Adresse, bei SFTP Host und
  Port als Ziel.
- **FR-018**: holzi MUSS für jede Verbindung das Protokoll verwenden, das zu ihr gehört, in allen
  Teilen, die Speicher nutzen (Einstellungen, Test, Dateibrowser, Übertragungen, Suche, Agents,
  Erweiterungen, verschlüsselte Ordner). Kein Teil DARF annehmen, dass ein Speicher S3 spricht, außer
  den S3-spezifischen Einstellungen und Spec 029.
- **FR-019**: Agents MÜSSEN WebDAV- und SFTP-Speicher nur mit einer Berechtigung je Speicher erreichen,
  wie S3-Speicher (Spec 044 FR-031a).
- **FR-020**: Zugangsdaten, private Schlüssel und Passphrasen DÜRFEN weder Erweiterungen noch Agents
  erreichen, nicht in Antworten, Fehlermeldungen, im Log oder über die Passwort-Funktionen
  (wie Spec 038 FR-012, Spec 044 FR-038).
- **FR-021**: Fehler der Server MÜSSEN als dieselben wenigen Fehlerarten ankommen wie bei S3 (nicht
  gefunden, Zugang abgelehnt, Netzwerkfehler, Grenze überschritten, ungültiger Schlüssel), ergänzt um
  „Hostschlüssel geändert“ für die Einstellungen und den Dateibrowser.
- **FR-022**: holzi MUSS seine Zwischendateien (Uploads, Kopien) im Dateibrowser, für Agents und für
  Erweiterungen ausblenden und Zwischendateien, die älter als 24 Stunden sind, entfernen, wenn es sie
  beim Auflisten sieht.
- **FR-023**: Alle Funktionen MÜSSEN auf allen Plattformen von holzi gleich arbeiten, auch auf Android
  und iOS (wie Spec 038 FR-016).

**Verschlüsselte Ordner**

- **FR-024**: Verschlüsselte Ordner nach Spec 048 MÜSSEN in WebDAV- und SFTP-Speichern mit demselben
  Verhalten funktionieren wie in S3-Speichern, einschließlich Erkennen geänderter Dateien,
  Teilbereichs-Lesen, Aufräumen nach Änderungszeit und Erkennen von Konflikten.

### Key Entities

- **Speicherverbindung**: wie in Spec 038, zusätzlich mit **Protokoll** (S3, WebDAV, SFTP). Je nach
  Protokoll: Endpunkt, Region, Adressierung (S3); Adresse mit Pfad (WebDAV); Host, Port, bestätigter
  Hostschlüssel mit Zeitpunkt der Bestätigung (SFTP). Verweis auf die Zugangsdaten im Passwortmanager.
  Gehört der Vault, synchronisiert.
- **Speicher**: Name, Bucket (S3) oder Ordnerpfad (WebDAV, SFTP), zugehörige Verbindung. Ziel der
  Berechtigungen der Art `remoteStorage`. Testergebnis je Gerät, nicht synchronisiert.
- **Zugangsdaten** (Eintrag im Passwortmanager): bei WebDAV Benutzername und Passwort; bei SFTP
  Benutzername und Passwort oder privater Schlüssel mit optionaler Passphrase.
- **Zwischendatei**: eine Datei, die holzi beim Hochladen oder Kopieren vorübergehend anlegt und nach
  Erfolg unter den Zielnamen bringt; an ihrem Namen erkennbar, wird nicht gespeichert.

## Success Criteria _(mandatory)_

### Measurable Outcomes

- **SC-001**: Mit bereitliegenden Zugangsdaten richtet ein Nutzer mit einer Voreinstellung einen WebDAV-
  oder SFTP-Speicher in unter 2 Minuten ein und sieht das Testergebnis in unter 10 Sekunden bei
  erreichbarem Server.
- **SC-002**: Der Prüfkatalog der Erweiterungsfunktionen aus Spec 038 (Bereich, Schlüssel, Grenzen,
  Zugangsdaten) besteht für WebDAV- und SFTP-Speicher zu 100 % mit denselben Ergebnissen wie für S3.
- **SC-003**: In 100 % der geprüften Abbrüche (Upload, Kopieren, Verschieben, Verbindung reißt ab) liegt
  unter dem Zielnamen keine halbe Datei, und eine vorhandene Datei ist unverändert.
- **SC-004**: In 100 % der Fälle mit geändertem Hostschlüssel sendet holzi keine Zugangsdaten an den
  Server.
- **SC-005**: In 100 % der geprüften Fälle (Antworten, Fehlermeldungen, Log, Passwort-Funktionen)
  erreichen Erweiterungen und Agents keine Passwörter, privaten Schlüssel oder Passphrasen.
- **SC-006**: Ein Video in einem WebDAV- oder SFTP-Speicher beginnt bei einer Leitung mit mindestens
  der Bitrate des Videos in unter 3 Sekunden zu spielen, und ein Sprung an eine beliebige Stelle zeigt
  in unter 3 Sekunden wieder Bild.
- **SC-007**: Ein auf einem Gerät angelegter WebDAV- oder SFTP-Speicher ist nach dem Sync auf einem
  zweiten Gerät ohne erneute Eingabe nutzbar, auch auf Android.
- **SC-008**: Alle Funktionen arbeiten nachweislich mit je einem selbst betriebenen WebDAV- und
  SFTP-Testserver (auch im automatischen Test) und von Hand mit einer Hetzner Storage Box (WebDAV und
  SFTP) und einer Nextcloud. Weitere Anbieter werden nach Bedarf geprüft.

## Assumptions

- Ein Speicher ist ein Ordner auf dem Server; holzi legt ihn nur nach Bestätigung an (FR-006) und
  löscht ihn nie beim Entfernen des Speichers.
- **WebDAV** meint WebDAV nach dem verbreiteten Standard mit Anmeldung über Benutzername und Passwort.
  Anbieter-eigene Erweiterungen (etwa Hochladen in Stücken bei Nextcloud) und Anmeldung über den
  Browser (OAuth) gehören nicht dazu; ein Upload geht in einem Stück und beginnt nach einem Abbruch von
  vorn, wie heute bei S3 nach einem gescheiterten Versuch.
- **SFTP** meint SFTP über SSH mit Passwort oder Schlüssel; SCP, FTP und FTPS gehören nicht dazu.
- Der **Bereich einer Erweiterung** und ihr Präfix sind dieselben wie in Spec 038; bei WebDAV und SFTP
  ist das Präfix ein Ordnerpfad unter dem Speicher.
- **Grenzen** für Größe, Laufzeit und Anzahl sind die vorhandenen aus Spec 017, 038 und 044; diese Spec
  führt keine neuen ein, außer der Längengrenze von Schlüsseln, die je Protokoll aus dem folgt, was
  Server verbreitet speichern können.
- Das **vault-sdk** kennt heute nur S3-Vorschläge (`type: "s3"`, Bucket, Region, Adressierung). Für
  FR-017b wird es um WebDAV- und SFTP-Vorschläge ohne Zugangsdaten ergänzt (eigener PR im vault-sdk,
  wie in Spec 038). Bestehende Erweiterungen funktionieren ohne diese Ergänzung weiter.
- Die genauen Adressmuster der Voreinstellungen (FR-004) legt der Plan fest und prüft sie gegen die
  Doku der Anbieter.
- Die Reihenfolge WebDAV vor SFTP ist eine Entscheidung des Betreibers (2026-10-11).

## Nicht im Umfang

- FTP, FTPS, SMB, NFS und die APIs von Google Drive, OneDrive, Dropbox.
- Anbieter-eigene Erweiterungen von WebDAV (Hochladen in Stücken, Freigabelinks, Papierkorb von
  Nextcloud) und Anmeldung über den Browser.
- Ausnahmen für selbst signierte Zertifikate oder eigene Zertifizierungsstellen.
- Fortsetzen eines abgebrochenen Uploads an der Abbruchstelle.
- Teilen von WebDAV- oder SFTP-Speichern oder Dateien darin mit anderen Vaults, auch Spaces darauf.
- Ein Wechsel des Protokolls einer bestehenden Verbindung oder ein Umzug der Daten zwischen Speichern
  verschiedener Protokolle über die normalen Kopierfunktionen des Dateibrowsers hinaus.
