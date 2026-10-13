# Feature Specification: Dateibrowser und Viewer

**Feature Branch**: `044-file-browser`

**Created**: 2026-10-07

**Status**: Implemented (T093 offen)

**Input**: User description: "Dateibrowser und Viewer als fester Bestandteil von holzi (Spec 044).
Grundlage ist das abgestimmte Design docs/plans/2026-10-07-file-browser-design.md, das alle
Entscheidungen enthält: ganzes Dateisystem des Geräts plus S3-Speicher aus Spec 038; Dateimanager
(durchsuchen, ansehen/abspielen, Ordner anlegen, umbenennen, kopieren, verschieben, löschen, Drag &
Drop ins Fenster); Viewer für Text, PDF (pdf.js), Bild, Video, Audio, sonst "Mit System-App öffnen";
Live-Suche ohne Index mit Filtern; Agents dürfen alles lesen, Schreiben über Approval-Gate,
holzi-eigene Daten für Agents gesperrt; Android mit MANAGE_EXTERNAL_STORAGE; Inhalte über lokalen
HTTP-Server mit Tokens (aus haex-vault), nie ganze Datei im RAM."

## Begriffe

- **Dateibrowser**: die eingebaute App von holzi für Dateien, neben Chat, Passwörtern und
  Einstellungen. Sie öffnet sich in Tabs wie die anderen Apps (Specs 015, 020).
- **Quelle**: woher der Dateibrowser Dateien zeigt: das **Gerät** (sein ganzes Dateisystem mit
  Laufwerken und bekannten Orten wie Dokumente, Bilder, Downloads) oder ein **Speicher** aus Spec 038
  (ein Bucket auf einer S3-Verbindung). In einem Speicher gelten Schlüsselpräfixe als Ordner.
- **Viewer**: die Ansicht, in der holzi eine Datei zeigt oder abspielt: Text, PDF, Bild, Video oder
  Audio. Alles andere zeigt eine **Info-Ansicht** mit Name, Typ, Größe, Änderungszeit und „Mit
  System-App öffnen“.
- **Vorschaubild**: ein verkleinertes Bild einer Bilddatei in Liste oder Raster.
- **Transfer**: ein Kopier- oder Verschiebevorgang, der Zeit braucht, besonders zwischen Gerät und
  Speicher (Hoch- und Herunterladen). Transfers zeigen Fortschritt und lassen sich abbrechen.
- **Freigabe-URL**: die Adresse, über die holzi den Inhalt einer geöffneten Datei an den Viewer gibt.
  Sie gilt nur für diese eine Datei und nur, solange Viewer oder Tab offen sind und die Vault
  entsperrt ist.
- **Agent**: der eingebaute Chat-Agent (Spec 032) und externe Agents über MCP (Spec 021).
- **Freigabestufe** (Approval-Modus, Spec 003, ADR 0006): bestimmt, welche Aktionen ein Agent ohne
  Rückfrage ausführt. Aktionen tragen die Stufe Safe (lesen), Change (ändern) oder Risky
  (zerstörend).
- **Agent-Berechtigung**: was ein Agent mit Dateien darf, in zwei getrennten Arten.
  **Dateien des Geräts**: eine Berechtigung je Agent; mit ihr liest, listet und durchsucht er das ganze
  Dateisystem, Ändern läuft über die Freigabestufe. **Speicher**: eine Berechtigung je Agent und
  Speicher, „Lesen“ oder „Lesen und Schreiben“.
- **Eigene Daten von holzi**: das Datenverzeichnis der App, auch im portablen Modus (Spec 014):
  Vault-Dateien, Schlüssel, Konfiguration, Bundles und Daten von Erweiterungen, Zwischenspeicher.

## Beziehung zu bestehenden Specs

- **Design**: [`docs/plans/2026-10-07-file-browser-design.md`](../../docs/plans/2026-10-07-file-browser-design.md)
  hält Aufbau, Datenfluss und Testansatz fest und nennt die Vorlagen aus haex-vault (Repository
  `https://github.com/haex-space/haex-vault`, Revision `fc4e84b6`). haex-files ist obsolet und kein
  Maßstab.
- **Spec 038** (Speicherverbindungen): liefert die Speicher, die der Dateibrowser als Quelle zeigt.
  Zugangsdaten bleiben wie dort in holzi; der Dateibrowser zeigt sie nie. Agents erreichen einen
  Speicher wie Erweiterungen dort nur mit einer Berechtigung je Speicher (FR-031a). Diese Spec erweitert den
  Zugriff auf einen Speicher um das Lesen von Teilbereichen eines Objekts (für Video und Audio).
- **Spec 017** (Erweiterungs-Host): Erweiterungen bekommen Dateizugriff weiterhin nur über ihre
  Berechtigungen dort (FR-046 bis FR-049). Diese Spec ändert daran nichts. Die Sperre der eigenen
  Daten von holzi entspricht FR-049 dort.
- **Spec 032 und ADR 0006** (Aktionen als Tools des eingebauten Agents): Die Dateiaktionen kommen in
  denselben Katalog. Abweichend von ADR 0006 laufen sie ohne Umweg über das Fenster (FR-034); das
  braucht ein eigenes ADR.
- **Spec 021** (MCP-Server): externe Agents bekommen dieselben Dateiaktionen nach ihrer Registrierung
  und mit ihren Grants dort.
- **Spec 022** (Sitzung wiederherstellen): Tabs des Dateibrowsers kommen mit Quelle und Ordner
  zurück.
- **Spec 043** (Android): Diese Spec ergänzt die Berechtigung „Zugriff auf alle Dateien“. Eine
  Veröffentlichung im Play Store bleibt nach 043 außerhalb des Umfangs.
- **Spec 036** (Anhänge mit Vorschau): verwandter Viewer für Anhänge von Passwörtern. Bild- und
  PDF-Viewer DÜRFEN geteilt werden; 036 FR-039 („keine PDF-Vorschau“) bleibt bis zu einer eigenen
  Änderung dort bestehen.
- **Folgende Specs**: 045 (Sync-Regeln, ersetzt 025) und der Umbau von 027/029 (Spaces) bauen auf
  dem Dateibrowser auf, etwa mit „Zu Space hinzufügen“ oder einem Sync-Status je Ordner. Sie sind
  nicht Teil dieser Spec.

## Clarifications

### Session 2026-10-07

- Q: Eingebaute App oder Erweiterung? → A: Eingebaute App von holzi (Betreiber).
- Q: Wie weit dürfen Agents ins Dateisystem? → A: Lesen, Auflisten und Suchen überall; Ändern über
  die Freigabestufe. Die eigenen Daten von holzi bleiben für Agents gesperrt. Das Risiko, dass ein
  Cloud-Modell gelesene Inhalte vom Gerät trägt und eine präparierte Datei den Agent zu heiklen
  Dateien lenkt, ist bewusst in Kauf genommen (Betreiber).
- Q: Was kann der Nutzer im Dateibrowser selbst tun? → A: Ein Dateimanager: durchsuchen, ansehen,
  abspielen, Ordner anlegen, umbenennen, kopieren, verschieben, löschen, Dateien aus dem System ins
  Fenster ziehen.
- Q: Welche Quellen in der ersten Fassung? → A: Das Gerät und die Speicher aus Spec 038. Andere eigene
  Geräte kommen später.
- Q: Wie sucht der Dateibrowser? → A: Live ab dem aktuellen Ordner, ohne Index. Ein Index kommt mit
  der späteren Spec zur Inhaltssuche.
- Q: Was passiert mit Formaten, die holzi nicht anzeigen kann? → A: Info-Ansicht mit „Mit System-App
  öffnen“. holzi wandelt keine Formate um.
- Q: Gilt „alles lesen“ für Agents auch für S3-Speicher? → A: Nein. Gerät und Speicher sind zwei
  getrennte Berechtigungen: „Dateien des Geräts“ je Agent (der eingebaute Agent hat sie ab Werk,
  externe Agents erst nach Erteilung) und je Speicher und Agent „Lesen“ oder „Lesen und Schreiben“,
  ab Werk für keinen Agent (Betreiber).
- Q: Was kann ein Agent aus Dateien lesen? → A: Text, den eingebetteten Text von PDFs, den Text von
  Office-Dokumenten (docx, odt, xlsx, ods) und Bilder, diese nur an Modelle, die Bilder annehmen
  (Spec 012). Keine Texterkennung in gescannten Dokumenten (Betreiber).
- Q: Darf ein Agent dem Nutzer eine Datei zeigen? → A: Ja. Er kann den Dateibrowser in einem Ordner
  oder mit einer Datei im Viewer öffnen, Wiedergabe eingeschlossen (Betreiber).
- Q: Wie weit reicht die Suche über Laufwerksgrenzen? → A: Sie überspringt virtuelle Systemordner und
  bleibt auf dem Laufwerk, auf dem sie startet; andere Laufwerke nur, wenn sie dort beginnt.
- Q: Wie greift holzi auf Android zu? → A: Mit „Zugriff auf alle Dateien“; echte Pfade wie auf dem
  Desktop, kein Weg über einzeln gewählte Ordner.

### Session 2026-10-11

- Q: Wie wählt man Einträge aus, und wie öffnet man sie? → A: Mit der Maus wählt ein Klick den Eintrag
  aus (Strg fügt hinzu oder nimmt weg, Umschalt wählt einen Bereich), ein Doppelklick, Enter oder
  Leertaste öffnet ihn, ein Klick auf die freie Fläche hebt die Auswahl auf. Auf Touch öffnet ein
  Tippen, solange nichts ausgewählt ist (Betreiber).

## User Scenarios & Testing _(mandatory)_

### User Story 1 - Dateien durchsuchen und ansehen (Priority: P1)

Anna öffnet den Dateibrowser. Links sieht sie bekannte Orte und Laufwerke, rechts den Inhalt ihres
Home-Ordners als Liste oder Raster mit Vorschaubildern. Sie klickt sich durch Ordner, springt über die
Pfadleiste zurück und öffnet eine Textdatei, ein PDF und ein Foto direkt in holzi.

**Why this priority**: Ohne Navigation und Anzeige gibt es keinen Dateibrowser; alles andere baut
darauf auf.

**Independent Test**: In einem Testordner mit Unterordnern, einer Textdatei, einem PDF und einem PNG
navigieren, zurückspringen und jede Datei öffnen. Die Liste zeigt Name, Typ, Größe und Änderungszeit;
das PNG hat ein Vorschaubild.

**Acceptance Scenarios**:

1. **Given** der Dateibrowser ist offen, **When** Anna einen Ordner öffnet, **Then** zeigt holzi seine
   Einträge mit Name, Typ, Größe und Änderungszeit, sortierbar nach jeder dieser Angaben.
2. **Given** ein Ordner mit Bildern, **When** Anna in die Rasteransicht wechselt, **Then** sieht sie
   Vorschaubilder, und es entstehen nur die für sichtbare Einträge.
3. **Given** eine Textdatei, ein PDF oder ein Bild, **When** Anna die Datei öffnet, **Then** zeigt
   holzi sie im Viewer; beim PDF kann sie blättern und zoomen, beim Bild zoomen und zur nächsten
   Datei im Ordner wechseln.
4. **Given** eine Datei, die holzi nicht anzeigen kann, **When** Anna sie öffnet, **Then** sieht sie
   die Info-Ansicht und kann die Datei mit der passenden App des Systems öffnen.
5. **Given** ein offener Ordner, **When** eine andere App darin eine Datei anlegt oder löscht,
   **Then** zeigt holzi das ohne manuelles Neuladen.
6. **Given** zwei Tabs des Dateibrowsers in verschiedenen Ordnern, **When** Anna holzi neu startet und
   die Sitzung wiederherstellt, **Then** stehen beide Tabs wieder in ihren Ordnern.

---

### User Story 2 - Video und Audio abspielen (Priority: P1)

Anna öffnet ein 4-GB-Video und ein MP3. Beide starten sofort, ohne vorher ganz geladen zu werden, und
sie kann im Video vor- und zurückspringen.

**Why this priority**: Abspielen war ausdrücklich gewünscht, und genau hier ist haex-vault unter
Linux gescheitert (MP3 lud erst ganz, MP4 startete nicht).

**Independent Test**: Ein großes MP4 und ein MP3 öffnen; beide starten in wenigen Sekunden, das Video
springt an eine beliebige Stelle, und der Speicherbedarf von holzi steigt dabei nicht mit der
Dateigröße.

**Acceptance Scenarios**:

1. **Given** ein Video in einem Format, das das System abspielen kann, **When** Anna es öffnet,
   **Then** beginnt die Wiedergabe, bevor die Datei ganz gelesen ist, und Springen an jede Stelle
   funktioniert.
2. **Given** eine Audiodatei, **When** Anna sie öffnet, **Then** spielt sie sofort, mit Springen,
   Lautstärke und Anzeige der Dauer.
3. **Given** ein Video in einem Format, das das System nicht abspielen kann, **When** die Wiedergabe
   scheitert, **Then** wechselt holzi in die Info-Ansicht mit „Mit System-App öffnen“.
4. **Given** eine geöffnete Datei, **When** Anna den Viewer oder den Tab schließt oder die Vault
   sperrt, **Then** liefert holzi den Inhalt über die bisherige Adresse nicht mehr aus.

---

### User Story 3 - Dateien verwalten (Priority: P2)

Anna räumt auf: Sie legt einen Ordner an, benennt Dateien um, kopiert und verschiebt eine Auswahl,
löscht alte Dateien und zieht Fotos aus dem System-Dateimanager in einen Ordner in holzi.

**Why this priority**: Gewünscht ist ein Dateimanager, nicht nur ein Betrachter. Ansehen und Abspielen
haben aber Vorrang.

**Independent Test**: In einem Testordner anlegen, umbenennen, kopieren (mit Namenskonflikt),
verschieben, löschen und eine Datei von außen hineinziehen; danach stimmt der Inhalt auf der Platte,
und Gelöschtes liegt im Papierkorb des Systems.

**Acceptance Scenarios**:

1. **Given** ein Ordner, **When** Anna einen neuen Ordner anlegt oder einen Eintrag umbenennt,
   **Then** erscheint die Änderung sofort, und ein schon vorhandener Name wird mit Grund abgelehnt.
2. **Given** eine Auswahl, **When** Anna kopiert oder verschiebt (über Menü, Zwischenablage von holzi
   oder Ziehen innerhalb von holzi), **Then** zeigt die Transferleiste den Fortschritt, und Anna kann
   abbrechen.
3. **Given** am Ziel gibt es einen Namen schon, **When** ein Transfer ihn erreicht, **Then** fragt
   holzi: ersetzen, beide behalten oder überspringen, mit „für alle übernehmen“.
4. **Given** ein Transfer wird abgebrochen oder scheitert, **When** Anna das Ziel ansieht, **Then**
   liegt dort keine halbe Datei.
5. **Given** eine Auswahl auf dem Desktop, **When** Anna löscht, **Then** landet sie im Papierkorb des
   Systems; auf Android und in einem Speicher fragt holzi vorher, weil dort endgültig gelöscht wird.
6. **Given** Dateien im System-Dateimanager, **When** Anna sie in einen Ordner im Dateibrowser zieht,
   **Then** kopiert holzi sie dorthin.

---

### User Story 4 - Suchen und filtern (Priority: P2)

Anna sucht ein Foto, von dem sie nur einen Teil des Namens weiß. Sie tippt im Dateibrowser, die
Treffer erscheinen nach und nach aus dem aktuellen Ordner und seinen Unterordnern, und sie filtert
auf „Bilder“ aus dem letzten Jahr.

**Why this priority**: Gewünscht war eine einfache Suche mit Filtern. Ohne sie ist ein großes
Dateisystem kaum nutzbar.

**Independent Test**: In einem Testbaum mit 10 000 Dateien nach einem Namensteil mit Tippfehler
suchen, auf Typ und Zeitraum filtern, die Suche durch neue Eingabe ersetzen.

**Acceptance Scenarios**:

1. **Given** ein offener Ordner, **When** Anna einen Suchbegriff tippt, **Then** erscheinen die ersten
   Treffer aus diesem Ordner und seinen Unterordnern, während die Suche weiterläuft; kleine Tippfehler
   finden die Datei trotzdem.
2. **Given** eine laufende Suche, **When** Anna den Begriff ändert, **Then** endet die alte Suche, und
   nur Treffer der neuen erscheinen.
3. **Given** Treffer oder ein offener Ordner, **When** Anna nach Typ (Bild, Video, Audio, Dokument,
   Text), Größe oder Zeitraum filtert, **Then** zeigt holzi nur passende Einträge.
4. **Given** ein Ordner mit einem symbolischen Link auf einen übergeordneten Ordner, **When** Anna
   sucht, **Then** endet die Suche, ohne im Kreis zu laufen.

---

### User Story 5 - Einen S3-Speicher durchsuchen (Priority: P2)

Anna hat in Spec 038 einen Speicher für ihren Bucket angelegt. Im Dateibrowser erscheint er in der
Seitenleiste. Sie blättert durch die Ordner, schaut ein Video direkt aus dem Bucket an, lädt Fotos
vom Gerät hoch und eine Datei herunter.

**Why this priority**: Gewünscht als Quelle der ersten Fassung; setzt Story 1 und 2 voraus.

**Independent Test**: Gegen einen lokalen S3-kompatiblen Testserver: Speicher öffnen, Ordner
wechseln, ein Video abspielen und springen, Dateien hoch- und herunterladen, einen Upload abbrechen.

**Acceptance Scenarios**:

1. **Given** ein Speicher aus Spec 038, **When** Anna ihn in der Seitenleiste wählt, **Then** zeigt
   holzi seine Objekte als Ordner und Dateien.
2. **Given** ein Video im Speicher, **When** Anna es öffnet, **Then** beginnt die Wiedergabe, ohne dass
   holzi es ganz herunterlädt, und Springen funktioniert.
3. **Given** Dateien auf dem Gerät, **When** Anna sie in einen Ordner des Speichers kopiert, **Then**
   lädt holzi sie mit Fortschritt hoch; ein Abbruch hinterlässt im Speicher kein halbes Objekt.
4. **Given** der Speicher ist nicht erreichbar oder die Zugangsdaten sind abgelaufen, **When** Anna
   ihn öffnet, **Then** nennt holzi den Grund und verweist auf die Einstellungen des Speichers.

---

### User Story 6 - Ein Agent sucht und liest Dateien (Priority: P2)

Anna fragt im Chat: „Finde meine Steuerunterlagen von 2025 und fasse das Schreiben vom Finanzamt
zusammen.“ Der Agent sucht im Dateisystem, liest das PDF und antwortet. Danach fragt sie, was auf
einem Foto im selben Ordner zu sehen ist, und der Agent sieht es sich an. Auf „zeig es mir“ öffnet er
das Foto im Viewer. Als sie ihn bittet, die
Datei in einen anderen Ordner zu verschieben, fragt holzi nach ihrer Freigabestufe vorher.

**Why this priority**: Ausdrücklich gewünscht; setzt den Dateizugriff von Story 1 und 4 voraus.

**Independent Test**: Im Chat mit einem Testmodell eine Datei suchen und ein PDF, ein docx und ein
Bild lesen lassen (keine Rückfrage), dann verschieben lassen (Rückfrage je nach Freigabestufe), dann die Vault-Datei lesen
lassen (abgelehnt).

**Acceptance Scenarios**:

1. **Given** eine Datei irgendwo im Dateisystem und der Agent hat „Dateien des Geräts“, **When** er
   sucht, auflistet oder Angaben abfragt, **Then** bekommt er die Ergebnisse ohne Rückfrage (Stufe
   Safe).
2. **Given** eine Textdatei, ein PDF mit Textschicht oder ein Office-Dokument (docx, odt, xlsx, ods),
   **When** der Agent sie liest, **Then** bekommt er ihren Text bis zu einer Größengrenze und den
   Hinweis, wenn er abgeschnitten wurde; bei Tabellen mit Blattnamen.
3. **Given** ein Bild und ein Modell, das Bilder annimmt (Spec 012), **When** der Agent es liest,
   **Then** bekommt das Modell das Bild, verkleinert auf eine Größengrenze; nimmt das Modell keine
   Bilder an, bekommt der Agent stattdessen Angaben zum Bild und den Hinweis, dass er es nicht sehen
   kann.
4. **Given** ein gescanntes PDF ohne Textschicht oder ein anderes Format, **When** der Agent es
   liest, **Then** bekommt er die Angaben zur Datei und den Hinweis, dass holzi keinen Text daraus
   gewinnen kann.
5. **Given** der Agent will einen Ordner anlegen, kopieren oder umbenennen, **When** er die Aktion
   aufruft, **Then** gilt die Stufe Change; beim Verschieben oder Löschen gilt die Stufe Risky.
6. **Given** eine Datei unter den eigenen Daten von holzi, **When** ein Agent sie lesen, auflisten,
   ändern oder in Suchtreffern sehen würde, **Then** lehnt holzi mit einem Grund ab und gibt keinen
   Inhalt heraus; Suchtreffer enthalten sie nicht.
7. **Given** eine Suche des Agents, die viele Treffer liefert oder lange läuft, **When** die Grenze
   erreicht ist, **Then** bekommt er das Teilergebnis mit dem Hinweis, dass es unvollständig ist.
8. **Given** ein Speicher, für den der Agent keine Berechtigung hat, **When** er ihn auflisten, lesen
   oder hineinkopieren will, **Then** fragt holzi den Nutzer (Lesen erlauben, Lesen und Schreiben
   erlauben, Ablehnen); ohne Erlaubnis bekommt der Agent eine Ablehnung mit Grund, und die Liste der
   Quellen des Agents nennt nur Speicher, die er erreichen darf.
9. **Given** der Agent darf einen Speicher nur lesen, **When** er dorthin kopieren, verschieben,
   umbenennen oder dort löschen will, **Then** lehnt holzi ab, unabhängig von der Freigabestufe.
10. **Given** eine Datei, die der Agent erreichen darf, **When** er sie dem Nutzer zeigen will,
    **Then** öffnet holzi den Dateibrowser mit der Datei im Viewer (Video und Audio spielen); bei
    einem Ordner öffnet er den Dateibrowser in diesem Ordner. Das gilt als Stufe Change, wie andere
    Aktionen, die Fenster öffnen.

---

### User Story 7 - Alle Dateien auf Android (Priority: P3)

Ben nutzt holzi auf seinem Android-Telefon. Beim ersten Öffnen des Dateibrowsers erklärt holzi, dass
es „Zugriff auf alle Dateien“ braucht, und führt ihn in die Systemeinstellungen. Danach sieht er den
internen Speicher und die SD-Karte wie auf dem Desktop.

**Why this priority**: Android ist eine Plattform von holzi (043), aber Desktop zuerst.

**Independent Test**: Auf einem Android-Gerät ohne die Berechtigung den Dateibrowser öffnen
(Erklärung), Berechtigung erteilen, zurückkehren (Liste erscheint), Berechtigung entziehen
(Erklärung erscheint wieder).

**Acceptance Scenarios**:

1. **Given** die Berechtigung fehlt, **When** Ben den Dateibrowser öffnet, **Then** sieht er eine
   Erklärung und einen Knopf in die Systemeinstellungen statt einer leeren Liste.
2. **Given** Ben erteilt die Berechtigung, **When** er zu holzi zurückkehrt, **Then** zeigt der
   Dateibrowser den Speicher des Geräts ohne Neustart.
3. **Given** die Berechtigung ist erteilt, **When** Ben Fotos ansieht, Videos abspielt oder Dateien
   verwaltet, **Then** verhält sich holzi wie auf dem Desktop, außer dass Löschen endgültig ist und
   vorher fragt.

---

### Edge Cases

- **Kein Zugriff durch das System** (Ordner eines anderen Nutzers): Der Eintrag erscheint mit
  Schloss-Symbol; Öffnen zeigt „Kein Zugriff“. Die Suche überspringt ihn ohne Fehlermeldungsflut.
- **Der offene Ordner verschwindet**: holzi springt zum nächsten vorhandenen übergeordneten Ordner und
  zeigt einen Hinweis.
- **Ein Laufwerk wird entfernt**: Es verschwindet aus der Seitenleiste; Transfers dorthin brechen mit
  Grund ab.
- **Symbolische Links**: Sie sind markiert; Öffnen folgt ihnen, die Suche nicht. Zeigt ein Link in die
  eigenen Daten von holzi, gilt für Agents die Sperre.
- **Suche ab der Wurzel des Systems**: Sie bleibt auf dem Systemlaufwerk; eingehängte USB-, Zusatz-
  und Netzlaufwerke durchsucht sie nicht, virtuelle Systemordner auch nicht.
- **Ein Ordner wird in sich selbst kopiert oder verschoben**: holzi lehnt vor dem Start ab.
- **Zu wenig Platz am Ziel**: Ist der freie Platz bekannt (Gerät), prüft holzi vor dem Start;
  sonst scheitert der Transfer mit Grund und ohne halbe Datei.
- **Netzfehler bei einem Speicher**: holzi versucht es bis zu dreimal mit wachsender Wartezeit erneut; danach
  bietet die Transferleiste „Erneut versuchen“.
- **Sehr große Textdatei** (über 5 MB): Der Viewer zeigt die ersten 5 MB mit Hinweis. Binärdateien
  mit Textendung zeigen die Info-Ansicht.
- **Beschädigtes Bild**: Statt Vorschaubild ein Symbol; holzi versucht es erst wieder, wenn sich die
  Datei ändert.
- **Sehr großes Bild in einem Speicher** (über 50 MB): Kein Vorschaubild, nur Symbol; Öffnen lädt es.
- **Ordner mit sehr vielen Einträgen** (50 000): Die Ansicht bleibt bedienbar.
- **Die Vault wird gesperrt, während ein Transfer läuft**: Der Transfer bricht ab, ohne halbe Datei,
  und alle Freigabe-URLs verfallen.
- **Eigene Daten von holzi im Dateibrowser**: Der Nutzer sieht sie, kann sie aber nicht ändern,
  verschieben oder löschen.

## Requirements _(mandatory)_

### Functional Requirements

**Navigation und Anzeige**

- **FR-001**: holzi MUSS einen eingebauten Dateibrowser haben, der wie die anderen eingebauten Apps
  in Tabs geöffnet wird.
- **FR-002**: Der Dateibrowser MUSS als Quellen das ganze Dateisystem des Geräts (Laufwerke und
  bekannte Orte) und jeden Speicher aus Spec 038 anbieten.
- **FR-003**: Für jeden Eintrag MUSS holzi Name, Typ, Größe und Änderungszeit zeigen, in Liste oder
  Raster, sortierbar nach jeder Angabe. Versteckte Dateien MÜSSEN sich ein- und ausblenden lassen
  (Vorgabe: aus). Ansicht, Sortierung und diese Wahl MÜSSEN je Gerät gespeichert werden.
- **FR-004**: Eine Pfadleiste MUSS jeden übergeordneten Ordner direkt erreichbar machen; vor und
  zurück MÜSSEN wie in einem Browser funktionieren.
- **FR-005**: Vorschaubilder MÜSSEN verkleinert erzeugt und nur für sichtbare Einträge angefragt
  werden. Sie MÜSSEN auf dem Gerät zwischengespeichert und DÜRFEN nie synchronisiert werden; ändert
  sich die Datei, entsteht ein neues.
- **FR-006**: Ein offener Ordner auf dem Gerät MUSS Änderungen durch andere Programme ohne manuelles
  Neuladen zeigen, auf dem Desktop und auf Android. Ein Ordner in einem Speicher MUSS sich manuell und
  beim Zurückkehren in den Tab neu laden; auf dem Gerät MUSS holzi beim Zurückkehren ebenfalls neu
  laden, falls die Beobachtung ein Ereignis verpasst hat.
- **FR-007**: Jeder Tab des Dateibrowsers MUSS seine eigene Quelle und seinen eigenen Ordner haben, und
  die Sitzung (Spec 022) MUSS beide wiederherstellen.
- **FR-008**: Eine Ansicht MUSS auch mit 50 000 Einträgen in einem Ordner bedienbar bleiben: Scrollen
  bis ans Ende ohne Bild, das länger als 100 ms braucht.

**Viewer**

- **FR-009**: holzi MUSS Textdateien, PDFs, Bilder, Videos und Audiodateien im Viewer zeigen bzw.
  abspielen, soweit das System das Format darstellen kann.
- **FR-010**: Der PDF-Viewer MUSS blättern und zoomen können, und zwar auf allen Plattformen,
  unabhängig davon, ob die Webview PDFs selbst darstellt.
- **FR-011**: Der Bild-Viewer MUSS zoomen und zur vorigen oder nächsten Datei im Ordner wechseln
  können.
- **FR-012**: Video- und Audiowiedergabe MÜSSEN beginnen, bevor die Datei ganz gelesen ist, und
  Springen an jede Stelle MUSS möglich sein, für Dateien auf dem Gerät wie in einem Speicher.
- **FR-013**: holzi DARF eine Datei zum Anzeigen oder Abspielen nie vollständig in den Arbeitsspeicher
  laden; der Speicherbedarf beim Abspielen DARF nicht mit der Dateigröße wachsen.
- **FR-014**: Kann holzi eine Datei nicht anzeigen oder scheitert die Wiedergabe, MUSS die
  Info-Ansicht erscheinen, mit „Mit System-App öffnen“. holzi DARF dafür keine Formate umwandeln.
- **FR-015**: Textdateien über 5 MB MUSS der Viewer bis 5 MB zeigen und den Rest mit Hinweis
  weglassen; Binärdateien MÜSSEN die Info-Ansicht zeigen.
- **FR-016**: Den Inhalt einer geöffneten Datei DARF holzi nur über eine Freigabe-URL ausliefern, die
  genau diese Datei nennt, nicht erratbar ist und verfällt, wenn Viewer oder Tab schließen, und
  spätestens, wenn die Vault gesperrt wird. Andere Programme auf dem Gerät DÜRFEN ohne diese Adresse
  nichts erhalten.

**Dateien verwalten**

- **FR-017**: Der Nutzer MUSS Ordner anlegen und Einträge umbenennen können. Ein vorhandener Name
  MUSS mit Grund abgelehnt werden.
- **FR-017a**: Mit der Maus MUSS ein Klick einen Eintrag auswählen und ein Doppelklick ihn öffnen;
  Enter und Leertaste MÜSSEN ihn ebenfalls öffnen. Auf Touch MUSS ein Tippen ihn öffnen, solange
  nichts ausgewählt ist.
- **FR-018**: Der Nutzer MUSS eine Auswahl kopieren und verschieben können, über Menü, eine
  Zwischenablage innerhalb von holzi und Ziehen innerhalb von holzi, auch zwischen Gerät und
  Speicher.
- **FR-019**: Transfers MÜSSEN Fortschritt zeigen, abbrechbar sein und in einer Transferleiste stehen,
  solange sie laufen oder gescheitert sind.
- **FR-020**: Ein abgebrochener oder gescheiterter Transfer DARF am Ziel keine halbe Datei und kein
  halbes Objekt hinterlassen.
- **FR-021**: Bei einem Namenskonflikt am Ziel MUSS holzi fragen: ersetzen, beide behalten (mit neuem
  Namen) oder überspringen, mit „für alle übernehmen“.
- **FR-022**: Einen Ordner in sich selbst oder einen seiner Unterordner zu kopieren oder zu
  verschieben MUSS holzi vor dem Start ablehnen.
- **FR-023**: Auf dem Desktop MUSS Löschen in den Papierkorb des Systems gehen. Auf Android und in
  einem Speicher MUSS holzi vor dem endgültigen Löschen fragen und die Zahl der betroffenen Einträge
  nennen.
- **FR-024**: Dateien und Ordner, die der Nutzer aus dem System in einen Ordner des Dateibrowsers
  zieht, MUSS holzi dorthin kopieren.
- **FR-025**: Netzfehler bei einem Speicher MUSS holzi bis zu dreimal mit wachsender Wartezeit
  wiederholen; danach MUSS der Transfer als gescheitert mit „Erneut versuchen“ erscheinen.
- **FR-026**: Wird die Vault gesperrt, MÜSSEN laufende Transfers abbrechen, ohne halbe Datei; für
  Freigabe-URLs gilt FR-016.

**Suche und Filter**

- **FR-027**: Die Suche MUSS ab dem aktuellen Ordner alle Unterordner durchlaufen, Treffer liefern,
  während sie läuft, und unscharf nach Namen suchen (kleine Tippfehler finden die Datei).
- **FR-028**: Eine neue Eingabe MUSS die laufende Suche beenden.
- **FR-029**: Filter nach Typ (Bild, Video, Audio, Dokument, Text), Größe und Zeitraum MÜSSEN auf
  Suchtreffer und auf den offenen Ordner wirken.
- **FR-030**: Die Suche DARF symbolischen Links nicht folgen und MUSS Ordner ohne Zugriff ohne
  Fehlermeldung überspringen. Sie MUSS auf dem Laufwerk bleiben, auf dem sie startet, und virtuelle
  Systemordner (etwa `/proc`, `/sys`, `/dev` unter Linux) überspringen; ein anderes Laufwerk
  durchsucht sie nur, wenn sie dort beginnt. Das gilt auch für Agents. In einem Speicher MUSS sie den Fortschritt zeigen.

**Agents**

- **FR-031**: holzi MUSS Agents Aktionen zum Auflisten, Abfragen von Angaben, Suchen und Lesen von
  Inhalten (FR-035a) anbieten, mit der Stufe Safe. Auf dem Gerät MÜSSEN sie das ganze Dateisystem
  erreichen, sofern der Agent die Berechtigung „Dateien des Geräts“ hat. Der eingebaute Agent MUSS sie ab Werk
  haben; externe Agents (Spec 021) MÜSSEN sie erst erteilt bekommen.
- **FR-031a**: Einen Speicher DARF ein Agent nur mit einer eigenen Berechtigung für genau diesen
  Speicher erreichen: „Lesen“ oder „Lesen und Schreiben“, ab Werk für keinen Agent. Fehlt sie, MUSS
  holzi den Nutzer fragen (Lesen erlauben, Lesen und Schreiben erlauben, Ablehnen) und die Antwort
  merken. Schreiben, Löschen und Kopieren in einen Speicher MÜSSEN „Lesen und Schreiben“ verlangen,
  zusätzlich zur Freigabestufe. Speicher ohne Berechtigung DÜRFEN in keiner Antwort an den Agent
  auftauchen.
- **FR-031b**: Der Nutzer MUSS die Datei-Berechtigungen jedes Agents sehen und widerrufen können, für
  das Gerät und je Speicher.
- **FR-032**: holzi MUSS Agents Aktionen zum Anlegen von Ordnern, Kopieren und Umbenennen (Stufe
  Change) und zum Verschieben und Löschen (Stufe Risky) anbieten, im Rahmen ihrer Berechtigungen
  (FR-031, FR-031a).
- **FR-032a**: Ein Agent MUSS den Dateibrowser in einem Ordner oder mit einer Datei im Viewer öffnen
  können, Wiedergabe eingeschlossen, mit der Stufe Change wie die übrigen Aktionen, die Fenster
  öffnen. Er DARF dabei nur Dateien und Ordner nennen, die er nach FR-031, FR-031a und FR-033
  erreichen darf.
- **FR-033**: Die eigenen Daten von holzi MÜSSEN für Agents vollständig gesperrt sein: kein Lesen,
  kein Auflisten, keine Angaben, kein Ändern, keine Suchtreffer. Geprüft MUSS immer das tatsächliche
  Ziel werden, nach Auflösen von `..` und symbolischen Links. Eine Ablehnung MUSS den Grund nennen und
  DARF keinen Inhalt enthalten.
- **FR-034**: Die Dateiaktionen für Agents MÜSSEN auch funktionieren, wenn kein Fenster von holzi
  offen ist; ausgenommen ist das Öffnen im Dateibrowser (FR-032a), das ein Fenster braucht.
- **FR-035**: Das Lesen durch einen Agent MUSS eine Größengrenze haben und ein Abschneiden
  kennzeichnen. Die Suche eines Agents MUSS eine Grenze für Laufzeit und Trefferzahl haben und ein
  Teilergebnis als unvollständig kennzeichnen.
- **FR-035a**: Lesen durch einen Agent MUSS liefern: den Text von Textdateien, den eingebetteten Text
  von PDFs, den Text von Office-Dokumenten (docx, odt, xlsx, ods; Tabellen mit Blattnamen) und Bilder.
  Bilder MUSS holzi nur an Modelle geben, die laut Spec 012 Bilder annehmen, und vorher auf eine
  Größengrenze verkleinern; sonst MUSS der Agent Angaben zum Bild und einen Hinweis bekommen. Für
  andere Formate und PDFs ohne Textschicht MUSS er Angaben zur Datei und einen Hinweis bekommen.
  holzi DARF dafür keine Texterkennung ausführen.
- **FR-036**: Externe Agents (Spec 021) MÜSSEN dieselben Aktionen bekommen, mit ihren Grants dort und
  den Berechtigungen aus FR-031 und FR-031a.
  Wer eine Aktion aufruft, MUSS sich aus dem Eingang ergeben, nie aus einer Angabe des Aufrufers
  (wie ADR 0007).

**Schutz der eigenen Daten und Android**

- **FR-037**: Die eigenen Daten von holzi MUSS der Nutzer im Dateibrowser sehen können, aber nicht
  ändern, verschieben oder löschen.
- **FR-038**: Zugangsdaten eines Speichers DÜRFEN weder im Dateibrowser noch in Fehlermeldungen,
  Freigabe-URLs oder Antworten an Agents erscheinen.
- **FR-039**: Auf Android MUSS holzi „Zugriff auf alle Dateien“ verwenden. Fehlt die Berechtigung,
  MUSS der Dateibrowser erklären, wozu er sie braucht, in die Systemeinstellungen führen und nach der
  Rückkehr ohne Neustart neu prüfen.

### Key Entities

- **Quelle**: Gerät oder Speicher (Spec 038); Name, Art, Wurzeln (Laufwerke, bekannte Orte oder
  Bucket).
- **Eintrag**: Datei oder Ordner in einer Quelle; Name, Pfad, Art, Größe, Änderungszeit, Typ,
  Kennzeichen für versteckt, symbolischen Link und fehlenden Zugriff.
- **Vorschaubild**: gehört zu Quelle, Pfad, Größe und Änderungszeit eines Bildes; nur auf dem Gerät
  zwischengespeichert.
- **Freigabe-URL**: genau eine Datei, ein Tab, verfällt mit Viewer, Tab oder Sperren der Vault.
- **Transfer**: Art (kopieren, verschieben), Quelle und Ziel, Einträge, Fortschritt, Zustand
  (läuft, abgebrochen, gescheitert mit Grund, fertig).
- **Agent-Berechtigung für Dateien**: Agent, Art (Dateien des Geräts oder ein Speicher), Stufe
  (für Speicher: Lesen oder Lesen und Schreiben); widerrufbar.
- **Einstellungen des Dateibrowsers**: Ansicht, Sortierung, versteckte Dateien; je Gerät.

## Success Criteria _(mandatory)_

### Measurable Outcomes

- **SC-001**: Ein Ordner mit 1 000 Einträgen auf dem Gerät ist in unter 1 Sekunde sichtbar; einer
  mit 50 000 Einträgen scrollt bis ans Ende ohne Bild, das länger als 100 ms braucht.
- **SC-002**: Ein 4-GB-Video auf dem Gerät beginnt in unter 2 Sekunden zu spielen, und ein Sprung an
  eine beliebige Stelle ist in unter 2 Sekunden sichtbar. In einem Speicher mit guter Verbindung
  beginnt es in unter 5 Sekunden.
- **SC-003**: Beim Abspielen eines 4-GB-Videos wächst der Speicherbedarf von holzi um weniger als
  100 MB.
- **SC-004**: Die ersten Suchtreffer erscheinen in unter 1 Sekunde, wenn sie im aktuellen Ordner
  liegen; eine Suche über 10 000 Dateien ist in unter 10 Sekunden fertig.
- **SC-005**: In 100 % der Fälle eines Prüfkatalogs (direkte Pfade, `..`, symbolische Links, Groß-
  und Kleinschreibung, Suchtreffer) erreicht ein Agent nichts aus den eigenen Daten von holzi, und
  ohne Berechtigung für einen Speicher erfährt er nicht einmal, dass es ihn gibt.
- **SC-006**: In 100 % der geprüften Abbrüche und Fehler (Abbruch durch den Nutzer, Netzfehler,
  fehlender Platz, Sperren der Vault) bleibt am Ziel keine halbe Datei zurück.
- **SC-007**: Eine Freigabe-URL liefert nach dem Schließen ihres Tabs oder dem Sperren der Vault in
  100 % der Fälle nichts mehr aus.
- **SC-008**: Wiedergabe von MP3 und MP4 mit Springen, PDF und Bilder funktionieren nachweislich unter
  Linux, Windows, macOS und Android, geprüft in der CI bei jedem PR.

## Assumptions

- Abgespielt wird, was die Webview des jeweiligen Systems kann. Unter Linux hängt das von den
  installierten Codecs ab; holzi liefert keine mit.
- Vorschaubilder gibt es nur für Bilder; Videos bekommen ein Symbol.
- Ein Speicher hat keine Benachrichtigung über Änderungen; deshalb wird dort manuell und beim
  Zurückkehren neu geladen.
- Der Grenzwert für Vorschaubilder aus einem Speicher liegt bei etwa 50 MB, der für Text im Viewer
  bei 5 MB; die Grenzen für Agents (etwa 30 Sekunden oder 500 Treffer, Text bis zu einer festen Größe)
  legt der Plan fest.
- Externe Agents nutzen die Aktionen erst, wenn Spec 021 gebaut ist; bis dahin gilt diese Spec nur
  für den eingebauten Agent.
- Freigabe-URLs sind nur für holzi selbst gedacht; sie sind keine Freigabe an andere Personen.
- holzi hat noch keine Nutzer; es gibt nichts zu übernehmen oder zu migrieren.

## Nicht im Umfang

- Andere eigene Geräte als Quelle (etwa die Fotos des Telefons vom Desktop aus ansehen).
- Inhaltssuche (etwa „Fotos vom Strand“), ein Suchindex und Embeddings; dafür kommt eine eigene Spec.
- Umwandeln von Formaten (Video, Audio, HEIC) und Vorschaubilder für Videos.
- Texterkennung (OCR) in gescannten PDFs und Bildern; Video und Audio für Agents.
- Bearbeiten von Dateien in holzi, auch von Text.
- Ziehen aus holzi heraus in andere Programme.
- Sync-Regeln, Spaces und Freigaben an andere Personen (Specs 045, 027, 029); der Dateibrowser zeigt
  dazu in dieser Spec nichts.
- Zugriff über einzeln gewählte Ordner auf Android (Storage Access Framework).
- Änderungen am Dateizugriff von Erweiterungen (Spec 017).
- Andere Protokolle als S3 (SMB, FTP). WebDAV- und SFTP-Speicher bringt Spec 050
  ([`050-webdav-sftp-storage`](../050-webdav-sftp-storage/spec.md)); sie erscheinen hier wie S3-Speicher.
