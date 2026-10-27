# Feature Specification: Extensions als Öffner für E-Mail-Links

**Feature Branch**: `063-mailto-extensions`

**Created**: 2026-10-26

**Status**: Draft

**Input**: User description: "Tippt der Nutzer im Adressbuch bei einer E-Mail-Adresse auf „Mail
schreiben“, ruft holzi weiterhin die Funktion des Betriebssystems auf (mailto:). Zusätzlich sollen
Extensions, die E-Mails schreiben können (zum Beispiel haex-mail), als Option erscheinen: in holzi als
Auswahl hinter dem Mail-Knopf und im System, indem holzi sich als Programm für mailto: registriert."

## Begriffe

- **E-Mail-Link**: ein Link der Form `mailto:` (RFC 6068) mit einem oder mehreren Empfängern und
  optional Kopie-Empfängern, Betreff und Text.
- **Öffner**: eine installierte, aktivierte Extension, die sich dafür angemeldet hat, einen E-Mail-Link
  zu öffnen, indem sie ihren Schreib-Dialog mit den Angaben des Links füllt (zum Beispiel haex-mail).
- **Öffner-Anmeldung**: die Erklärung einer Extension in ihrem Manifest, dass sie E-Mail-Links öffnen
  kann. Sie ist keine Berechtigung: Die Extension erhält nur den Link, den der Nutzer ihr bewusst
  übergibt.
- **Öffnen-Auswahl**: das kleine Menü, in dem der Nutzer wählt, womit ein E-Mail-Link geöffnet wird: mit
  dem System oder mit einem der Öffner.
- **System-Öffnen**: das Weiterreichen des E-Mail-Links an das Betriebssystem, das daraufhin das dort
  eingestellte Mail-Programm startet. Das macht holzi heute schon.
- **Mail-Knopf**: der Knopf neben einer E-Mail-Adresse eines Kontakts (Spec 051 FR-006).

## Beziehung zu bestehenden Specs

- **Spec 051** (Adressbuch): FR-006 („E-Mail-Adressen öffnen auf jedem Gerät das Mail-Programm des
  Systems“) bleibt der Standard. Diese Spec fügt nur Öffner als weitere Wahl hinzu; ohne Öffner ändert
  sich nichts.
- **Spec 017** (Extension-Host): Die Übergabe an die Extension läuft über die Bridge, die der Host schon
  für Ereignisse an Extensions hat. Das Manifest bekommt ein neues, optionales Feld. Das
  Manifest-Schema und die Prüfung der Bündel liegen im externen Repo `vault-sdk`, haex-mail im Repo
  `haextension`; beide gehören zum Umfang dieser Spec.
- **Spec 060** (Kontakte teilen): Der Deep Link `haex-vault://` und seine Registrierung beim
  Betriebssystem kommen aus Spec 060. Diese Spec nutzt denselben Weg für das Schema `mailto:`. Die
  Registrierung für `mailto:` und die Weitergabe an die Auswahl setzen voraus, dass der Weg aus 060
  (Plugin, Eingang auf dem Desktop, Kaltstart auf Android) vorhanden ist.
- **Spec 057** (Kanäle zwischen Extensions) und **Spec 056** (Dateifreigaben): Beide schließen
  Extensions nicht von Kontakten aus; diese Spec gibt einer Extension weder Zugriff auf Kontakte noch
  auf andere Daten, nur auf den einen Link.

## User Scenarios & Testing _(mandatory)_

### User Story 1 - Eine Mail an einen Kontakt mit haex-mail schreiben (Priority: P1)

Ich sehe bei einem Kontakt eine E-Mail-Adresse und tippe auf den Mail-Knopf. Ist keine Extension
installiert, die E-Mails öffnen kann, passiert dasselbe wie bisher: Das Mail-Programm des Systems geht
auf. Gibt es mindestens einen Öffner, zeigt der Knopf stattdessen die Öffnen-Auswahl: „Mit dem System
öffnen“ und je eine Zeile pro Öffner, zum Beispiel „haex-mail“. Wähle ich haex-mail, öffnet holzi das
Fenster von haex-mail, und dessen Schreib-Dialog steht mit der Adresse als Empfänger offen. Nichts wird
gesendet, bevor ich es in haex-mail selbst tue.

**Why this priority**: Das ist der Kern des Wunsches: der Weg vom Kontakt zur Mail in der eigenen
Extension, ohne Umweg über das System. Er funktioniert allein, auch ohne Registrierung beim System.

**Independent Test**: haex-mail installieren, bei einem Kontakt auf den Mail-Knopf tippen, haex-mail
wählen: Schreib-Dialog mit dem Empfänger. Ohne installierten Öffner: das System-Mail-Programm geht auf
wie bisher.

**Acceptance Scenarios**:

1. **Given** kein Öffner ist installiert, **When** ich auf den Mail-Knopf tippe, **Then** öffnet holzi
   den E-Mail-Link beim System, ohne Auswahl, wie vor dieser Spec.
2. **Given** haex-mail ist installiert und hat sich als Öffner angemeldet, **When** ich auf den
   Mail-Knopf tippe, **Then** sehe ich die Öffnen-Auswahl mit „Mit dem System öffnen“ und „haex-mail“.
3. **Given** die Öffnen-Auswahl ist offen, **When** ich „haex-mail“ wähle, **Then** wird haex-mail
   geöffnet (oder in den Vordergrund geholt) und zeigt einen Schreib-Dialog mit der Adresse als
   Empfänger; es ist nichts gesendet.
4. **Given** die Öffnen-Auswahl ist offen, **When** ich „Mit dem System öffnen“ wähle, **Then** geht
   das Mail-Programm des Systems auf wie bisher.
5. **Given** die Öffnen-Auswahl ist offen, **When** ich sie ohne Wahl schließe, **Then** passiert
   nichts.
6. **Given** zwei Extensions sind Öffner, **When** ich auf den Mail-Knopf tippe, **Then** hat die Auswahl
   eine Zeile pro Extension, mit dem Namen der Extension.

---

### User Story 2 - Ein E-Mail-Link von außen öffnet holzi und ich wähle den Öffner (Priority: P2)

Ich tippe in einer anderen App, im Browser oder in einem Dokument auf einen E-Mail-Link. Hat der Nutzer
holzi beim System als Programm für E-Mail-Links eingestellt (oder wählt er es im System-Dialog „Öffnen
mit“), geht holzi auf und zeigt die Öffnen-Auswahl für genau diesen Link: die Öffner und, wenn das keine
Schleife ergibt, „Mit dem System öffnen“. Meine Wahl öffnet den Link in der Extension. Ist die Vault
gesperrt, kommt die Öffnen-Auswahl nach dem Entsperren.

**Why this priority**: Ergänzt den Kern um den Weg von außen; braucht die Registrierung beim System und
den Deep-Link-Eingang aus Spec 060, und ist deshalb getrennt prüfbar.

**Independent Test**: holzi als Programm für E-Mail-Links wählen, einen E-Mail-Link außerhalb von holzi
antippen: holzi geht auf, Öffnen-Auswahl, haex-mail wählen, Schreib-Dialog mit Empfänger, Betreff und
Text des Links.

**Acceptance Scenarios**:

1. **Given** holzi ist beim System für E-Mail-Links eingetragen, **When** ich außerhalb von holzi einen
   E-Mail-Link öffne, **Then** geht holzi auf und zeigt die Öffnen-Auswahl für diesen Link.
2. **Given** die Auswahl für einen Link von außen, **When** ich haex-mail wähle, **Then** zeigt dessen
   Schreib-Dialog Empfänger, Kopie-Empfänger, Betreff und Text des Links.
3. **Given** die Vault ist gesperrt, **When** ein E-Mail-Link von außen kommt, **Then** merkt sich holzi
   den Link, bis ich entsperrt habe, und zeigt dann die Auswahl; der Link bleibt nicht auf Dauer
   gespeichert.
4. **Given** holzi ist selbst das Programm des Systems für E-Mail-Links, **When** der Link von außen
   kommt, **Then** bietet die Auswahl „Mit dem System öffnen“ nicht an, weil es holzi erneut öffnen
   würde.
5. **Given** es gibt keinen Öffner, **When** ein E-Mail-Link von außen kommt, **Then** sagt holzi das
   (kein Öffner installiert) und der Link geht nicht verloren: Der Nutzer kann ihn kopieren.
6. **Given** der Link enthält ungültige oder zu große Angaben, **When** er ankommt, **Then** zeigt holzi
   die Auswahl nicht und sagt, dass der Link nicht gelesen werden kann.

---

### User Story 3 - Eine Extension meldet sich als Öffner an (Priority: P2)

Der Autor einer Extension erklärt im Manifest, dass sie E-Mail-Links öffnen kann. Beim Installieren zeigt
holzi das wie die anderen Angaben des Manifests. Sobald die Extension installiert und aktiviert ist,
erscheint sie in der Öffnen-Auswahl; wird sie entfernt oder deaktiviert, verschwindet sie daraus.
haex-mail meldet sich an und öffnet auf einen Link hin seinen Schreib-Dialog.

**Why this priority**: Ohne Anmeldung gibt es keine Öffner. Sie ist die Voraussetzung für Geschichte 1
und 2, aber für sich genommen klein und über die drei Repos aufteilbar.

**Independent Test**: Eine Test-Extension mit Anmeldung installieren: sie erscheint in der Auswahl und
empfängt den Link. Dieselbe ohne Anmeldung: sie erscheint nicht. Deaktivieren: sie verschwindet.

**Acceptance Scenarios**:

1. **Given** das Manifest einer Extension enthält die Anmeldung, **When** ich sie installiere und
   aktiviere, **Then** erscheint sie in der Öffnen-Auswahl.
2. **Given** das Manifest enthält die Anmeldung nicht, **When** ich sie installiere, **Then** erscheint
   sie dort nicht.
3. **Given** eine angemeldete Extension, **When** ich sie deaktiviere oder entferne, **Then** erscheint
   sie nicht mehr in der Auswahl.
4. **Given** ein Manifest mit einer Anmeldung für eine Art, die diese Version nicht kennt, **When** ich
   es installiere, **Then** wird die Extension installiert, die unbekannte Anmeldung wird ignoriert, und
   sie erscheint nirgends.
5. **Given** eine angemeldete Extension, die gerade nicht läuft, **When** ich sie in der Auswahl wähle,
   **Then** startet holzi sie und übergibt den Link, sobald sie bereit ist.

---

### Edge Cases

- **Mehrere Empfänger und Kopien**: Ein Link mit mehreren Empfängern, Kopie- und Blindkopie-Empfängern
  wird vollständig an den Öffner übergeben; die Extension zeigt sie in ihrem Dialog, holzi entscheidet
  nichts darüber.
- **Kopfzeilen-Einschleusung**: Ein Link kann in Empfänger, Betreff oder Text Zeilenumbrüche oder
  weitere Kopfzeilen einschmuggeln wollen. holzi gibt nur die benannten Felder als Text weiter, nie als
  rohe Kopfzeilen, und lässt Zeilenumbrüche in Empfänger und Betreff nicht zu.
- **Zu große Angaben**: Mehr als die Grenzen (Anzahl Empfänger, Länge von Betreff und Text) lässt holzi
  den Link ablehnen statt ihn zu kürzen.
- **Kodierung**: Prozent-kodierte und nicht-ASCII-Zeichen in Empfänger, Betreff und Text kommen
  unverändert dekodiert an.
- **Link ohne Empfänger**: `mailto:?subject=…` ist gültig; der Schreib-Dialog öffnet ohne Empfänger.
- **Extension öffnet nicht**: Startet die Extension nicht oder antwortet nicht in angemessener Zeit,
  sagt holzi das und lässt dem Nutzer die anderen Wahlen offen.
- **Mehrere Fenster oder Vaults**: Ist mehr als eine Vault offen, geht der Link an die, deren Fenster
  den Mail-Knopf hat; ein Link von außen geht an die zuletzt aktive Vault und die Auswahl nennt das
  nicht weiter, der Nutzer kann sie schließen.
- **Android, Kaltstart**: Öffnet der Link holzi erst, dauert es bis zum Entsperren; danach erscheint die
  Auswahl wie auf dem Desktop.
- **Sperrbildschirm und Berechtigung**: Eine Extension bekommt den Link nur nach der Wahl des Nutzers in
  der Auswahl, nie von selbst, auch nicht, wenn sie sich angemeldet hat.
- **Mehrere Links kurz nacheinander**: Kommen mehrere Links von außen, bevor der Nutzer gewählt hat,
  zeigt holzi sie nacheinander; kein Link geht verloren und keiner wird ohne Wahl geöffnet.

## Requirements _(mandatory)_

### Functional Requirements

**Mail-Knopf und Auswahl**

- **FR-001**: Gibt es keinen Öffner, MUSS der Mail-Knopf den E-Mail-Link wie bisher beim System öffnen,
  ohne zusätzliche Auswahl.
- **FR-002**: Gibt es mindestens einen Öffner, MUSS der Mail-Knopf statt dessen die Öffnen-Auswahl
  zeigen, mit „Mit dem System öffnen“ und je einer Zeile pro Öffner (Name der Extension).
- **FR-003**: Wählt der Nutzer „Mit dem System öffnen“, MUSS holzi den E-Mail-Link wie bisher an das
  System geben. Wählt er einen Öffner, MUSS holzi den Link nur an diese Extension geben und das System
  nicht aufrufen.
- **FR-004**: Schließt der Nutzer die Auswahl ohne Wahl, DARF holzi nichts öffnen.
- **FR-005**: holzi DARF einen Link nie ohne Wahl des Nutzers an eine Extension geben, auch nicht an
  eine einzige angemeldete.

**Der Link und die Übergabe**

- **FR-006**: holzi MUSS einen E-Mail-Link nach RFC 6068 lesen: Empfänger (mehrere, getrennt), Kopie-
  und Blindkopie-Empfänger, Betreff und Text, Prozent-Kodierung und nicht-ASCII-Zeichen eingeschlossen.
  Ein Link, der das nicht erfüllt, MUSS abgelehnt werden.
- **FR-007**: holzi MUSS dem Öffner die gelesenen Felder als Text übergeben (Empfänger, Kopie,
  Blindkopie, Betreff, Text), nie den rohen Link und nie rohe Kopfzeilen. Zeilenumbrüche in Empfänger,
  Kopie, Blindkopie und Betreff MÜSSEN abgelehnt werden.
- **FR-008**: holzi MUSS höchstens 50 Empfänger je Feld, höchstens 998 Zeichen im Betreff und
  höchstens 16 KiB im Text zulassen. Einen Link über einer dieser Grenzen MUSS holzi ablehnen, nicht
  kürzen.
- **FR-009**: Die Übergabe MUSS über die Bridge der Extension laufen und MUSS die Extension starten,
  wenn sie nicht läuft, und den Link zustellen, sobald sie bereit ist. Antwortet sie nicht in
  angemessener Zeit, MUSS holzi das dem Nutzer sagen.
- **FR-010**: Die Übergabe MUSS einer Extension nichts über den Kontakt verraten außer dem, was im Link
  steht. Eine Öffner-Anmeldung gibt keinen Zugriff auf Kontakte oder andere Daten.

**Anmeldung im Manifest**

- **FR-011**: Das Manifest MUSS ein optionales Feld haben, in dem eine Extension erklärt, welche Arten
  von Links sie öffnen kann. Diese Spec kennt die Art „E-Mail schreiben“. Die Prüfung des Bündels im
  Repo `vault-sdk` MUSS das Feld kennen und eine falsch geformte Angabe ablehnen.
- **FR-012**: Eine Extension MUSS nur in der Auswahl erscheinen, wenn sie installiert, aktiviert und
  angemeldet ist. Deaktivieren, Entfernen oder Aktualisieren ohne die Anmeldung MUSS sie aus der Auswahl
  nehmen.
- **FR-013**: Eine Anmeldung für eine Art, die diese Version nicht kennt, MUSS die Installation nicht
  verhindern und MUSS ignoriert werden.
- **FR-014**: Die Installationsansicht MUSS die Anmeldung zeigen („kann E-Mail-Links öffnen“), wie die
  übrigen Angaben des Manifests.
- **FR-015**: haex-mail MUSS sich anmelden und auf einen Link hin seinen Schreib-Dialog mit den
  übergebenen Feldern öffnen, ohne etwas zu senden.

**Registrierung beim System und Links von außen**

- **FR-016**: holzi MUSS sich beim Betriebssystem als Programm für E-Mail-Links eintragen können, auf
  dem Desktop und auf Android, so dass der Nutzer es im System als Öffner wählen kann. holzi DARF sich
  nicht selbst zum Standard machen oder eine Systemeinstellung ändern; die Wahl bleibt beim Nutzer.
- **FR-017**: Kommt ein E-Mail-Link von außen, MUSS holzi die Öffnen-Auswahl für genau diesen Link
  zeigen (FR-002 bis FR-005). Ist die Vault gesperrt, MUSS holzi den Link bis nach dem Entsperren
  vormerken und danach die Auswahl zeigen. Der Link DARF nicht auf Dauer gespeichert werden.
- **FR-018**: Bei einem Link von außen DARF die Auswahl „Mit dem System öffnen“ nicht anbieten, wenn
  holzi selbst das Programm des Systems für E-Mail-Links ist, weil das holzi erneut öffnen würde. Ob
  holzi das weiß, MUSS die Spec für jede Plattform festlegen; wo es das nicht erkennen kann, MUSS holzi
  das Weiterreichen an das System so absichern, dass keine Schleife entsteht.
- **FR-019**: Gibt es für einen Link von außen keinen Öffner, MUSS holzi das sagen und dem Nutzer den
  Link zum Kopieren lassen.
- **FR-020**: Der Eingang eines Links von außen DARF nie selbst eine Mail öffnen, senden oder eine
  Extension starten; er zeigt nur die Auswahl.

**Allgemein**

- **FR-021**: Alle Texte der Oberfläche MÜSSEN in Deutsch und Englisch vorliegen.
- **FR-022**: Die Auswahl MUSS mit Tastatur bedienbar sein und für Hilfstechnik jede Zeile mit dem Namen
  benennen.
- **FR-023**: Spec 051 FR-006 gilt unverändert für alle anderen Aktionen des Knopfs (Kopieren, Anrufen,
  Nachricht, Karte, Web): Nur der Mail-Knopf bekommt die Auswahl.

### Key Entities

- **Öffner-Anmeldung**: Eintrag im Manifest einer Extension: welche Art von Links sie öffnen kann
  („E-Mail schreiben“). Hat keine Berechtigung, keinen Zugriff.
- **Öffnen-Anfrage**: der gelesene E-Mail-Link: Empfänger, Kopie, Blindkopie, Betreff, Text. Entsteht
  aus einem Link vom Mail-Knopf oder von außen, wird nicht gespeichert und geht nur auf Wahl des Nutzers
  an eine Extension.
- **Öffnen-Auswahl**: die Wahl des Nutzers für eine Anfrage: System oder ein Öffner, oder nichts.

## Success Criteria _(mandatory)_

### Measurable Outcomes

- **SC-001**: Ein Nutzer mit haex-mail kommt von einer E-Mail-Adresse im Adressbuch zu einem
  Schreib-Dialog mit dem Empfänger in höchstens zwei Tipps (Mail-Knopf, Öffner).
- **SC-002**: Ohne Öffner verhält sich der Mail-Knopf auf allen Geräten genau wie vor dieser Spec; kein
  zusätzlicher Tipp.
- **SC-003**: In keinem Ablauf wird eine Mail gesendet oder ein Link an eine Extension gegeben, ohne
  dass der Nutzer in der Auswahl gewählt hat.
- **SC-004**: 100 % der gültigen E-Mail-Links nach RFC 6068 aus einer Testsammlung (mehrere Empfänger,
  Kodierung, nicht-ASCII, ohne Empfänger) kommen mit den richtigen Feldern an; 100 % der Links mit
  Zeilenumbrüchen in Empfänger oder Betreff, ungültiger Form oder über den Grenzen werden abgelehnt.
- **SC-005**: Ein Link von außen führt auf Desktop und Android von der Auswahl zum Schreib-Dialog mit
  allen Feldern des Links, auch wenn die Vault zuerst entsperrt werden muss.
- **SC-006**: Eine angemeldete Extension erscheint innerhalb von 2 Sekunden nach dem Aktivieren in der
  Auswahl und nach dem Deaktivieren nicht mehr.

## Assumptions

- Es gibt in dieser Spec nur eine Art von Link: E-Mail (`mailto:`). Telefon, SMS, Karte und andere
  Schemata kommen, wenn der Wunsch da ist, mit einer eigenen Spec; die Anmeldung im Manifest ist so
  geformt, dass weitere Arten hinzukommen können.
- Der Nutzer wählt bei jedem Öffnen neu. Einen Standard-Öffner zu merken ist offen und nicht im Umfang.
- Der Deep-Link-Weg aus Spec 060 (Plugin, Eingang auf dem Desktop, Kaltstart auf Android) ist
  vorhanden oder wird vor Geschichte 2 gebaut; Geschichte 1 und 3 brauchen ihn nicht.
- Auf dem Desktop trägt sich holzi beim System ein; ob und wie das System den Nutzer danach wählen
  lässt, hängt vom System ab. Wo ein System ein Programm nur über seine Einstellungen als Standard
  zulässt, bleibt das beim Nutzer (FR-016).
- Die Grenzen sind: höchstens 50 Empfänger je Feld, Betreff bis 998 Zeichen, Text bis 16 KiB. Die Zahlen
  stehen im Vertrag und sind hier Annahmen, die der Plan bestätigt.
- haex-mail öffnet seinen bestehenden Schreib-Dialog mit den Feldern; dass es das kann, ist Arbeit im
  Repo `haextension`. Das Manifest-Feld und seine Prüfung sind Arbeit im Repo `vault-sdk`; holzi nimmt
  dessen neue Version über die gepinnte Revision auf.
- Es gibt noch keine Nutzer mit bestehenden Daten; Übergangswege entfallen.

## Nicht im Umfang

- Andere Links als `mailto:` (`tel:`, `sms:`, `geo:`, Webseiten).
- Einen Standard-Öffner merken, pro Kontakt oder allgemein.
- Dass holzi selbst Mails sendet oder anzeigt; das tut die Extension.
- Eine Öffnen-Auswahl für andere Aktionen als den Mail-Knopf im Adressbuch und Links von außen (zum
  Beispiel E-Mail-Adressen in anderen Apps von holzi).
- Dass holzi sich selbst zum Standard-Mail-Programm des Systems macht.
