# Feature Specification: Sync-Regeln

**Feature Branch**: `061-sync-rules`

**Created**: 2026-10-26

**Status**: Draft

**Input**: User description: "Sync-Regeln (Spec 061): Regeln pro Gerät, die zwei Orte des Dateibrowsers
einseitig oder beidseitig gleich halten. Grundlage ist das abgestimmte Design
docs/plans/2026-10-26-sync-rules-design.md (alle Entscheidungen dort sind vom Betreiber bestätigt).
Ersetzt den Entwurf 025, baut auf 056 auf (andere eigene Geräte erscheinen dort als Quelle); schließt
Kontakt-Freigaben, Hintergrunddienst und eigene Verschlüsselung der Regel aus."

## Begriffe

- **Dateibrowser**, **Quelle**, **Transfer**, **Agent**, **Agent-Berechtigung**, **eigene Daten von
  holzi**: wie in Spec 044. **Speicher**: wie in den Specs 038 und 050. **Verschlüsselter Ordner**: wie
  in Spec 048. **Geräte-Dateizugriff** und **Gerät der Vault**: wie in Spec 056.
- **Ort**: ein Ordner einer Quelle des Dateibrowsers: ein lokaler Ordner, ein Ordner auf einem anderen
  eigenen Gerät (Spec 056), ein Ordner in einem S3-, WebDAV- oder SFTP-Speicher oder ein entsperrter
  verschlüsselter Ordner. Ein Ort einer fremden Vault (empfangene Dateifreigabe) ist kein Ort im Sinne
  dieser Spec.
- **Sync-Regel**: die Zuordnung zweier Orte A und B mit **Richtung**, **Löschmodus** und **Intervall**.
  Sie gehört dem Gerät, auf dem der Nutzer sie anlegt, und läuft nur dort. Sie wird nicht auf andere
  Geräte synchronisiert.
- **Richtung**: **A→B**, **B→A** oder **beidseitig**. Bei einer einseitigen Regel heißt der Ort, aus
  dem kopiert wird, **Quellort**, der andere **Zielort**.
- **Abgleich**: ein Durchlauf einer Regel. Er vergleicht beide Orte mit dem **Stand** und bringt sie
  gemäß Richtung in Übereinstimmung.
- **Stand**: was holzi zu einer Regel vom letzten erfolgreichen Abgleich je Datei weiß (relativer Pfad,
  Größe und Änderungszeit auf beiden Seiten). Aus dem Vergleich mit dem Stand folgt, was seitdem neu,
  geändert, verschoben oder gelöscht ist. Die Änderungszeit einer Seite wird nur mit dem Stand
  derselben Seite verglichen, nie mit der Uhr einer anderen.
- **Plan**: die Liste der Schritte, die ein Abgleich ausführen würde (kopieren, verschieben, löschen,
  Konfliktkopie), bevor er etwas ändert.
- **Konfliktkopie**: das Ergebnis einer Datei, die seit dem letzten Abgleich auf beiden Seiten geändert
  wurde. Die Version mit der neueren Änderungszeit behält den Namen; die ältere wird als eigene Datei
  `Name (Konflikt Gerätename Datum).Endung` daneben abgelegt, auf beiden Seiten.
- **Löschmodus**: **Spiegeln**, **Behalten** oder **Nachfragen**: was mit einer Datei geschieht, die
  auf einer Seite gelöscht wurde.
- **Regel-Papierkorb**: der Ordner `.holzi-trash` im Ort, in den eine gespiegelte Löschung wandert,
  wenn der Ort keinen Papierkorb des Systems hat.
- **Regelstatus**: der Zustand einer Regel: **aktiv**, **läuft**, **wartet** (ein Ort ist nicht
  erreichbar), **Fehler**, **pausiert** oder **holzi nicht aktiv**.

## Beziehung zu bestehenden Specs

- **Design**: [`docs/plans/2026-10-26-sync-rules-design.md`](../../docs/plans/2026-10-26-sync-rules-design.md)
  hält die Entscheidungen, das Modell, den Ablauf und den Testansatz fest.
- **Spec 025** (Dateisync zwischen eigenen Geräten): entfällt. Diese Spec ersetzt den Entwurf, der noch
  auf Spaces, Dateiindex und verschlüsselten Objekten beruhte. 025 wird mit dem letzten Umsetzungsschritt
  gelöscht; Verweise in anderen Dokumenten zeigen danach auf diese Spec.
- **Spec 056** (Dateizugriff und Dateifreigaben): liefert den Zugriff auf andere eigene Geräte. Ein
  eigenes Gerät ist dort eine Quelle wie ein Speicher; diese Spec braucht deshalb keinen eigenen
  Transport. Ein Gerät, dessen Schalter „Geräte-Dateizugriff“ aus ist, ist als Ort nicht erreichbar.
- **Spec 044** (Dateibrowser): liefert Quellen, Ordnerwahl, Transfers, Papierkorb und die Regeln für
  Agents und die eigenen Daten von holzi. Die Oberfläche der Regeln sitzt im Dateibrowser.
- **Specs 038 und 050** (Speicher): Speicher als Ort. Zugangsdaten bleiben auf dem Gerät, das die Regel
  ausführt.
- **Spec 048** (verschlüsselte Ordner): Wer Inhalte verschlüsselt beim Anbieter ablegen will, wählt
  einen verschlüsselten Ordner als Ort. Eine eigene Verschlüsselung der Regel gibt es nicht.
- **Spec 024** (Sync der eigenen Geräte): Regeln werden nicht über sie synchronisiert.
- **Spec 057** und **Spec 060**: nicht berührt. Fremde Vaults sind kein Ort.
- Der **Hintergrunddienst** (Android-Vordergrunddienst, Autostart auf dem Desktop) ist eine eigene,
  spätere Spec. Diese Spec setzt ihn nicht voraus.

## Clarifications

### Session 2026-10-26 (Brainstorming mit dem Betreiber)

- Q: Welche Orte darf eine Regel verbinden? → A: Jeden Ort, den der Dateibrowser zeigt (lokal, eigene
  Geräte, S3, WebDAV, SFTP, verschlüsselte Ordner). Freigaben von Kontakten sind ausgeschlossen, weil
  Abgleich und Entziehen kollidieren.
- Q: Welches Gerät führt eine Regel aus? → A: Genau eines, das, auf dem sie angelegt wurde. Die Regel
  wird nicht synchronisiert; die Gegenseite wird über den Zugriff aus 056 erreicht. Es gibt keinen
  geteilten Dateiindex.
- Q: Was passiert bei gleichzeitiger Änderung beidseitig? → A: Konfliktkopie. Wurde eine Datei auf einer
  Seite geändert und auf der anderen gelöscht, bleibt die geänderte und wird zurückkopiert.
- Q: Wie werden Löschungen behandelt? → A: Pro Regel Spiegeln, Behalten oder Nachfragen. Vorgabe:
  Spiegeln bei beidseitig, Behalten bei einseitig.
- Q: Wann läuft eine Regel? → A: Solange holzi mit entsperrter Vault läuft. Auslöser: Dateiwächter auf
  lokalen Ordnern, Intervall für entfernte Orte (Vorgabe 15 Minuten), „Jetzt abgleichen“. Der
  Hintergrunddienst ist eine eigene Spec.
- Q: Wie wird beim Anbieter verschlüsselt? → A: Gar nicht durch die Regel; der Nutzer wählt einen
  verschlüsselten Ordner (048) als Ort.
- Q: Wie erkennt holzi Änderungen und Verschiebungen? → A: Größe und Änderungszeit gegen den Stand
  derselben Seite. Eine verschwundene und eine neue Datei mit gleicher Größe und Änderungszeit sind eine
  Verschiebung. Eine Prüfsumme nur für nicht eindeutige Kandidaten und zur Kontrolle nach dem Kopieren.

## User Scenarios & Testing _(mandatory)_

### User Story 1 - Einen Ordner einseitig sichern (Priority: P1)

Anna möchte, dass ihr Ordner `Fotos` auf dem Laptop laufend in ihren S3-Speicher gesichert wird. Im
Dateibrowser wählt sie „Sync-Regeln“ und „Neue Regel“, wählt `Fotos` als Ort A und einen Ordner im
Speicher als Ort B, die Richtung „A→B“ und lässt den Löschmodus auf der Vorgabe „Behalten“. Nach dem
ersten Abgleich liegen alle Fotos im Speicher. Macht sie danach neue Fotos, erscheinen sie dort ohne
weiteres Zutun, solange holzi läuft. Löscht sie ein Foto auf dem Laptop, bleibt es im Speicher.

**Why this priority**: Das ist der einfachste und häufigste Fall und legt Regelmodell, Stand,
Abgleichsplan und Übertragung an, auf denen alles Weitere aufbaut.

**Independent Test**: Zwei lokale Ordner als Orte, einseitige Regel. Dateien anlegen, ändern, löschen
und umbenennen; nach jedem Abgleich den Zielort prüfen.

**Acceptance Scenarios**:

1. **Given** ein Ordner mit Dateien und ein leerer Zielort, **When** Anna die Regel anlegt und
   abgleicht, **Then** liegen danach alle Dateien und Unterordner mit gleichen Namen im Zielort.
2. **Given** eine bestehende Regel, **When** Anna eine neue Datei in den Quellort legt, **Then** liegt
   sie nach dem nächsten Abgleich im Zielort.
3. **Given** eine bestehende Regel, **When** Anna eine Datei im Quellort ändert, **Then** ersetzt die
   neue Version im Zielort die alte.
4. **Given** der Löschmodus „Behalten“, **When** Anna eine Datei im Quellort löscht, **Then** bleibt sie
   im Zielort, und ein späterer Abgleich kopiert sie nicht erneut.
5. **Given** eine einseitige Regel, **When** jemand eine Datei im Zielort ändert, **Then** überschreibt
   der Abgleich sie nicht; holzi meldet die Abweichung im Regelstatus.
6. **Given** ein Abgleich, der mittendrin abbricht, **When** er neu startet, **Then** setzt er sauber
   fort, und im Zielort liegt keine halbe Datei als fertig.
7. **Given** ein Zielort in einem verschlüsselten Ordner, **When** die Regel läuft, **Then** liegt der
   Inhalt beim Anbieter verschlüsselt, und die Regel braucht dafür keine eigene Einstellung.

---

### User Story 2 - Zwei Orte beidseitig gleich halten (Priority: P1)

Anna hat den Ordner `Arbeit` auf dem Laptop und auf dem Telefon. Auf dem Laptop legt sie eine Regel
„beidseitig“ zwischen dem lokalen `Arbeit` und `Arbeit` des Telefons an (erreichbar über den
Geräte-Dateizugriff aus 056). Was sie auf einer Seite hinzufügt, ändert oder löscht, erscheint nach dem
nächsten Abgleich auf der anderen. Haben sich dieselbe Datei auf beiden Seiten verändert, gehen beide
Versionen erhalten, die ältere als Konfliktkopie.

**Why this priority**: Das ist der Kern von „dauerhaft zwischen zwei Geräten synchron halten“ und
braucht die Konfliktbehandlung, die das Vertrauen in einen Abgleich ausmacht.

**Independent Test**: Zwei lokale Ordner, Regel „beidseitig“. Auf beiden Seiten Dateien anlegen, ändern,
umbenennen und löschen, auch dieselbe Datei auf beiden Seiten; nach dem Abgleich beide Seiten prüfen.

**Acceptance Scenarios**:

1. **Given** zwei Orte mit jeweils eigenen neuen Dateien, **When** die Regel abgleicht, **Then** haben
   beide danach alle Dateien.
2. **Given** eine Datei, die nur auf einer Seite geändert wurde, **When** die Regel abgleicht, **Then**
   gewinnt diese Version auf beiden Seiten.
3. **Given** dieselbe Datei auf beiden Seiten verschieden geändert, **When** die Regel abgleicht,
   **Then** behält die Version mit der neueren Änderungszeit den Namen, die andere liegt als
   `Name (Konflikt Gerätename Datum).Endung` daneben, und beide Seiten haben beide Dateien.
4. **Given** eine Datei, die auf einer Seite gelöscht und auf der anderen geändert wurde, **When** die
   Regel abgleicht, **Then** bleibt die geänderte Datei und wird auf die Seite zurückkopiert, auf der
   sie fehlt.
5. **Given** eine Datei, die auf einer Seite umbenannt oder in einen anderen Unterordner verschoben
   wurde, **When** die Regel abgleicht, **Then** wird sie auf der anderen Seite ebenso verschoben statt
   gelöscht und neu kopiert.
6. **Given** der Löschmodus „Spiegeln“, **When** Anna auf einer Seite eine Datei löscht, **Then** landet
   sie auf der anderen Seite im Papierkorb des Systems, sonst im Regel-Papierkorb, nicht endgültig
   gelöscht.
7. **Given** der Laptop führt die Regel aus und das Telefon ist aus, **When** der Abgleich fällig ist,
   **Then** steht die Regel auf „wartet“ ohne Fehler, und der Abgleich läuft nach, sobald das Telefon
   erreichbar ist.

---

### User Story 3 - Löschungen und Konflikte unter Kontrolle behalten (Priority: P2)

Anna will nie versehentlich Daten verlieren. Bei einer Regel mit dem Löschmodus „Nachfragen“ erscheinen
zu löschende Dateien im Regelstatus als Liste; sie bestätigt oder verwirft sie. Ist ein Ort plötzlich
leer, weil ein Laufwerk fehlt, löscht holzi nichts, sondern meldet „Ort nicht erreichbar“.

**Why this priority**: Der Schutz vor Massenlöschen entscheidet, ob jemand einer dauerhaften Regel
traut. Er ist dennoch nachrangig gegenüber dem Abgleich selbst.

**Independent Test**: Regel mit „Nachfragen“ und Löschung auf einer Seite; Ort unterbrechen (Ordner
entfernen oder Gerät aus); prüfen, dass nichts gelöscht wird.

**Acceptance Scenarios**:

1. **Given** der Löschmodus „Nachfragen“, **When** Dateien auf einer Seite gelöscht wurden, **Then**
   listet der Regelstatus sie mit der Zahl der betroffenen Einträge, und der Abgleich löscht erst nach
   Bestätigung; Verwerfen kopiert die Datei zurück oder behält sie nach Wahl.
2. **Given** ein Ort, der leer listet oder dessen Wurzelordner fehlt, **When** die Regel abgleicht,
   **Then** bricht der Abgleich mit „Ort nicht erreichbar“ ab und löscht oder überschreibt nichts.
3. **Given** eine Datei, die nicht kopiert werden kann (gesperrt, kein Platz, Netz weg), **When** die
   Regel abgleicht, **Then** läuft der Rest weiter; die Datei steht mit Grund im Regelstatus und wird
   beim nächsten Abgleich erneut versucht.
4. **Given** einen Konflikt, **When** Anna den Regelstatus öffnet, **Then** sieht sie die Konfliktkopien
   und kann sie dort öffnen.

---

### User Story 4 - Regeln anlegen, ansehen und verwalten (Priority: P2)

Anna findet im Dateibrowser den Bereich „Sync-Regeln“ mit allen Regeln dieses Geräts: Name, beide Orte,
Richtung, Status und Zeitpunkt des letzten Abgleichs. Sie legt eine Regel mit zwei Ortswählern an,
sieht vor dem ersten Abgleich eine Vorschau des Plans, gleicht von Hand ab, pausiert, ändert oder löscht
die Regel. Löscht sie eine Regel, bleiben alle Dateien an beiden Orten unberührt.

**Why this priority**: Ohne Verwaltung und Vorschau ist die Funktion nicht benutzbar; sie ändert aber
nichts am Abgleich selbst.

**Independent Test**: Regel anlegen, Vorschau öffnen, abgleichen, pausieren, bearbeiten und löschen;
Orte und Stand prüfen.

**Acceptance Scenarios**:

1. **Given** der Bereich „Sync-Regeln“, **When** Anna „Neue Regel“ wählt, **Then** zeigt der Dialog zwei
   Ortswähler (der Ordnerwähler des Dateibrowsers), die Richtung, den Löschmodus und das Intervall mit
   Vorgaben (beidseitig → Spiegeln, einseitig → Behalten, 15 Minuten).
2. **Given** ein Zielort in einem Speicher ohne Verschlüsselung, **When** Anna ihn wählt, **Then** weist
   der Dialog darauf hin, dass ein verschlüsselter Ordner den Inhalt vor dem Anbieter schützt.
3. **Given** zwei Regeln, die sich überlappen (derselbe Ordner oder ein Ordner und sein
   Unterordner), **When** Anna die zweite speichern will, **Then** lehnt holzi sie mit Begründung ab.
4. **Given** eine neue Regel, **When** Anna die Vorschau öffnet, **Then** sieht sie den Plan mit allen
   Kopier-, Verschiebe-, Lösch- und Konfliktschritten, bevor etwas geändert wird.
5. **Given** eine Regel, **When** Anna sie pausiert, **Then** läuft kein Abgleich, bis sie sie wieder
   aktiviert; der Stand bleibt erhalten.
6. **Given** eine Regel, **When** Anna ihre Orte oder Richtung ändert, **Then** gilt danach ein neuer
   Stand, und holzi zeigt vorab den Plan.
7. **Given** holzi läuft nicht oder die Vault ist gesperrt, **When** Anna den Bereich später öffnet,
   **Then** steht der Status zuvor als „holzi nicht aktiv“, und die Regel holt den Abgleich nach.

---

### User Story 5 - Agents verwalten Sync-Regeln (Priority: P3)

Anna bittet einen Agent: „Sichere meine Fotos laufend in den Speicher.“ Mit der Agent-Berechtigung
„Sync-Regeln verwalten“ kann er Regeln und ihren Status lesen. Anlegen, Ändern und Löschen einer Regel
bestätigt Anna jedes Mal in einem Dialog.

**Why this priority**: Sinnvoll, aber ohne den Kern nicht nötig.

**Independent Test**: Agent mit und ohne Berechtigung versucht, eine Regel zu lesen, anzulegen und zu
löschen.

**Acceptance Scenarios**:

1. **Given** ein Agent ohne die Berechtigung, **When** er Regeln lesen oder ändern will, **Then** lehnt
   holzi ab.
2. **Given** ein Agent mit der Berechtigung, **When** er Regeln und Status liest, **Then** bekommt er sie
   ohne Rückfrage.
3. **Given** ein Agent mit der Berechtigung, **When** er eine Regel anlegt, ändert oder löscht, **Then**
   zeigt holzi dem Nutzer Orte, Richtung und Löschmodus und führt es erst nach Bestätigung aus.
4. **Given** eine Regel, die ein Agent angelegt hat, **When** sie läuft, **Then** braucht sie keinen
   Agent und hat nur die Rechte, die der Nutzer bestätigt hat.
5. **Given** eine Regel mit den eigenen Daten von holzi als Ort, **When** ein Agent sie anlegen will,
   **Then** lehnt holzi ab (Spec 044).

---

### Edge Cases

- **Gerät als Ort nicht erreichbar oder Geräte-Dateizugriff aus**: die Regel steht auf „wartet“ und
  holt nach (keine Fehlermeldung).
- **Vault gesperrt, während ein Abgleich läuft**: der Abgleich bricht ohne halbe Datei ab; der Stand hält
  nur Schritte, die ganz abgeschlossen sind.
- **Zugangsdaten eines Speichers ungültig**: die Regel steht auf „Fehler“ mit dem Grund; es wird nichts
  gelöscht.
- **Datei wird während des Kopierens geändert**: der Abgleich erkennt es an Größe oder Änderungszeit
  nach dem Kopieren, verwirft die Kopie und versucht es beim nächsten Abgleich erneut.
- **Gleichnamige Datei und Ordner** auf beiden Seiten (Datei hier, Ordner dort): als Konflikt behandeln,
  nichts überschreiben; die Seite mit der neueren Änderung behält den Namen, die andere bekommt eine
  Konfliktkopie.
- **Namensunterschiede** (Groß- und Kleinschreibung, unzulässige Zeichen, Sonderzeichen einer Seite):
  eine Datei, die ein Ort nicht ablegen kann, steht mit Grund unter den Fehlern; sie stoppt den
  Abgleich nicht.
- **Uhren verschiedener Geräte weichen ab**: die Erkennung von Änderungen vergleicht Zeiten nur mit dem
  Stand derselben Seite. Nur die Wahl der neueren Version bei einem Konflikt vergleicht Zeiten
  verschiedener Seiten.
- **Sehr große Dateien und Abbruch**: Kopien setzen fort, ohne bei jedem Abbruch von vorn zu beginnen.
- **Symbolische Links und Verknüpfungen** im Ort: werden nicht verfolgt, sondern als eigene Einträge
  übersprungen und im Status genannt.
- **Eine Regel pro Ort und Gegenüber**: überlappende Regeln sind unzulässig (Story 4).
- **Regel nach Löschen eines Orts oder Speichers**: die Regel steht auf „Fehler: Ort nicht mehr
  vorhanden“ und löscht nichts, bis der Nutzer sie anpasst.
- **Erster Abgleich mit Dateien auf beiden Seiten gleichen Namens und verschiedenen Inhalts**: als
  Konflikt behandeln (Konfliktkopie), nie stillschweigend überschreiben.

## Requirements _(mandatory)_

### Functional Requirements

**Regeln**

- **FR-001**: Der Nutzer MUSS eine Sync-Regel mit zwei Orten, einer Richtung (A→B, B→A, beidseitig),
  einem Löschmodus und einem Intervall anlegen, ändern, pausieren, fortsetzen und löschen können.
- **FR-002**: Als Ort MUSS jeder Ordner einer Quelle des Dateibrowsers zulässig sein: lokal, anderes
  eigenes Gerät (Spec 056), S3, WebDAV, SFTP und entsperrter verschlüsselter Ordner (Spec 048). Orte
  einer fremden Vault (empfangene Dateifreigaben) und die eigenen Daten von holzi MÜSSEN ausgeschlossen
  sein.
- **FR-003**: Eine Regel MUSS dem Gerät gehören, auf dem sie angelegt wurde, und nur dort laufen. Sie
  DARF NICHT auf andere Geräte synchronisiert werden.
- **FR-004**: holzi MUSS beim Speichern überlappende Regeln ablehnen (derselbe Ort, oder ein Ort
  innerhalb des anderen), auch Regeln, deren Ort als Zielort für eine andere Regel dient.
- **FR-005**: Vorgaben beim Anlegen: Löschmodus „Spiegeln“ bei beidseitig, „Behalten“ bei einseitig;
  Intervall 15 Minuten.
- **FR-006**: Das Löschen einer Regel DARF keine Datei in einem der Orte verändern oder löschen.

**Abgleich**

- **FR-007**: Ein Abgleich MUSS beide Orte mit dem Stand der Regel vergleichen und daraus einen Plan
  bilden, bevor er etwas ändert. Der Plan MUSS ohne Änderung einsehbar sein (Vorschau).
- **FR-008**: Eine Datei MUSS als geändert gelten, wenn sich Größe oder Änderungszeit gegenüber dem Stand
  derselben Seite unterscheiden. Die Änderungszeiten verschiedener Seiten DÜRFEN NICHT zur Erkennung
  von Änderungen verglichen werden.
- **FR-009**: Eine verschwundene und eine neue Datei mit gleicher Größe und gleicher Änderungszeit MÜSSEN
  als Verschiebung oder Umbenennung gelten und auf der Gegenseite als solche ausgeführt werden, statt
  gelöscht und neu kopiert zu werden. Nur bei mehreren nicht eindeutig zuordenbaren Kandidaten DARF
  holzi eine Prüfsumme bilden.
- **FR-010**: Bei einer einseitigen Regel MUSS ein Abgleich nur Änderungen vom Quellort zum Zielort
  übertragen. Eine Änderung im Zielort DARF er NICHT überschreiben; er MUSS sie im Regelstatus melden.
- **FR-011**: Bei einer beidseitigen Regel MUSS ein Abgleich Änderungen in beide Richtungen übertragen.
- **FR-012**: Eine Datei, die seit dem letzten Abgleich auf beiden Seiten geändert wurde, MUSS zu einer
  Konfliktkopie führen: Die Version mit der neueren Änderungszeit behält den Namen, die ältere wird als
  `Name (Konflikt Gerätename Datum).Endung` daneben abgelegt; beide Seiten bekommen beide Dateien.
  Das gilt auch für den ersten Abgleich, wenn dieselbe Datei auf beiden Seiten verschieden ist.
- **FR-013**: Eine Datei, die auf einer Seite gelöscht und auf der anderen geändert wurde, DARF NICHT
  gelöscht werden; die geänderte Datei MUSS auf die Seite zurückkopiert werden, auf der sie fehlt.
- **FR-014**: Eine Kopie MUSS zuerst in eine temporäre Datei am Ziel gehen und erst nach vollständiger
  Übertragung ihren Namen bekommen. Eine halbe Datei DARF NIE als vollständig gelten.
- **FR-015**: holzi MUSS den Stand je Datei erst fortschreiben, nachdem ihr Schritt vollständig
  gelungen ist, sodass ein abgebrochener Abgleich beim nächsten Mal sauber fortsetzt.
- **FR-016**: Nach einer Kopie MUSS holzi die Änderungszeit des Ziels an die der Quelle angleichen, wo
  der Ort das erlaubt, und das Ergebnis über Größe und, wo nötig, eine Prüfsumme kontrollieren.
- **FR-017**: Große Dateien MÜSSEN fortsetzbar übertragen werden; Netzfehler bei einem Speicher MUSS
  holzi wie in Spec 044 FR-025 wiederholen.
- **FR-018**: Symbolische Links und Verknüpfungen DÜRFEN NICHT verfolgt werden; holzi MUSS sie
  überspringen und im Regelstatus nennen.

**Löschen**

- **FR-019**: Der Löschmodus „Spiegeln“ MUSS eine auf einer Seite gelöschte Datei auf der anderen Seite
  nicht endgültig löschen, sondern in den Papierkorb des Systems legen, wo der Ort einen hat, sonst in
  den Regel-Papierkorb `.holzi-trash` des Ortes.
- **FR-020**: Der Löschmodus „Behalten“ MUSS die Datei auf der anderen Seite lassen und sich merken, dass
  sie dort bewusst bleibt, damit sie nicht zurückkopiert wird.
- **FR-021**: Der Löschmodus „Nachfragen“ MUSS die anstehenden Löschungen mit ihrer Zahl im Regelstatus
  auflisten und erst nach Bestätigung des Nutzers ausführen; der Nutzer MUSS verwerfen können.
- **FR-022**: Meldet ein Ort eine leere Liste oder fehlt sein Wurzelordner, MUSS der Abgleich mit „Ort
  nicht erreichbar“ abbrechen. Eine leere Liste DARF NIE als „alles gelöscht“ gelten.

**Fehler und Status**

- **FR-023**: Eine Datei, die fehlschlägt, DARF den Abgleich nicht stoppen. holzi MUSS sie mit Grund im
  Regelstatus nennen und beim nächsten Abgleich erneut versuchen.
- **FR-024**: Ist ein Ort nicht erreichbar (anderes Gerät aus oder Geräte-Dateizugriff aus, Netz weg),
  MUSS die Regel auf „wartet“ stehen, ohne Fehlermeldung, und nachholen, sobald er erreichbar ist.
- **FR-025**: Der Regelstatus MUSS je Regel Zustand, Zeitpunkt des letzten Abgleichs, Fehler,
  Abweichungen, anstehende Löschungen und Konfliktkopien zeigen.
- **FR-026**: Wird die Vault gesperrt oder holzi beendet, MÜSSEN laufende Abgleiche abbrechen, ohne halbe
  Datei (wie Spec 044 FR-026); die Regel steht danach auf „holzi nicht aktiv“.

**Auslöser**

- **FR-027**: Eine Regel MUSS laufen, solange holzi mit entsperrter Vault läuft: durch einen Wächter auf
  lokalen Orten, durch das Intervall bei entfernten Orten und auf Wunsch von Hand („Jetzt abgleichen“).
- **FR-028**: Auf dem Desktop MUSS eine Regel auch bei geschlossenem Fenster laufen, solange holzi im
  Tray weiterläuft. Auf Android MUSS sie bei geschlossener App pausieren; ein Hintergrunddienst gehört
  nicht zu dieser Spec.
- **FR-029**: Die Regel DARF keine Zugangsdaten und Schlüssel an das andere Gerät geben; Zugangsdaten
  eines Speichers verlassen das ausführende Gerät nicht.

**Verschlüsselung**

- **FR-030**: Die Regel DARF keine eigene Verschlüsselung und keine eigenen Schlüssel haben. Der Nutzer
  verschlüsselt beim Anbieter, indem er einen verschlüsselten Ordner (Spec 048) als Ort wählt; die
  Regel MUSS mit einem solchen Ort wie mit jedem anderen arbeiten, solange er entsperrt ist.
- **FR-031**: Beim Wählen eines Speichers ohne Verschlüsselung als Zielort MUSS der Dialog auf den
  verschlüsselten Ordner hinweisen.

**Oberfläche**

- **FR-032**: Der Dateibrowser MUSS einen Bereich „Sync-Regeln“ zeigen: Liste mit Name, Orten,
  Richtung, Status und letztem Abgleich; je Regel „Jetzt abgleichen“, Pause, Vorschau, Bearbeiten,
  Löschen und die Listen der Fehler, anstehenden Löschungen und Konflikte.
- **FR-033**: Das Anlegen MUSS zwei Ortswähler nutzen, die der Ordnerwähler des Dateibrowsers sind.
- **FR-034**: Neue Oberflächenbausteine, die in haex-ui fehlen, MÜSSEN dort ergänzt statt lokal
  nachgebaut werden.

**Agents**

- **FR-035**: Es MUSS eine Agent-Berechtigung „Sync-Regeln verwalten“ geben, standardmäßig aus. Lesen
  von Regeln und Status MUSS mit ihr ohne Rückfrage gehen; Anlegen, Ändern und Löschen einer Regel MUSS
  der Nutzer jedes Mal in einem Dialog bestätigen, der Orte, Richtung und Löschmodus nennt.
- **FR-036**: Eine Regel läuft ohne Agent; sie hat nur die Rechte, die der Nutzer beim Anlegen bestätigt
  hat. Die eigenen Daten von holzi bleiben für Agents gesperrt (Spec 044 FR-033).

### Key Entities

- **Sync-Regel**: Name, Zustand (aktiv oder pausiert), Ort A, Ort B, Richtung, Löschmodus, Intervall.
  Gehört dem ausführenden Gerät und wird nicht synchronisiert.
- **Ort**: Quelle plus Ordnerpfad. Quellen sind lokal, eigenes Gerät, S3, WebDAV, SFTP, verschlüsselter
  Ordner.
- **Stand**: je Regel und Datei der relative Pfad sowie Größe und Änderungszeit auf Seite A und B beim
  letzten erfolgreichen Abgleich, dazu die Vermerke „bewusst behalten“.
- **Plan**: die Schritte eines Abgleichs (kopieren, verschieben, löschen, Konfliktkopie).
- **Regelstatus**: Zustand, letzter Abgleich, Fehlerliste, Abweichungen, anstehende Löschungen,
  Konflikte.

## Success Criteria _(mandatory)_

### Measurable Outcomes

- **SC-001**: Der Nutzer legt eine einseitige Regel zwischen zwei Orten in unter einer Minute an.
- **SC-002**: Nach einem Abgleich sind bei einer einseitigen Regel alle Dateien des Quellorts im Zielort
  mit gleichem Pfad, gleicher Größe und gleicher Änderungszeit vorhanden (100 % in den Tests).
- **SC-003**: Kein Abgleich verliert Daten: Bei gleichzeitiger Änderung derselben Datei sind nach dem
  Abgleich beide Versionen noch vorhanden; bei Löschung gegen Änderung bleibt die Änderung.
- **SC-004**: Ein leerer oder nicht erreichbarer Ort löst in keinem Test eine Löschung auf der
  Gegenseite aus.
- **SC-005**: Ein abgebrochener Abgleich (Vault sperren, Netz weg, Absturz) hinterlässt in keinem Test
  eine halbe Datei als fertig und setzt beim nächsten Lauf fort, ohne bereits vollständig kopierte
  Dateien erneut zu übertragen.
- **SC-006**: Eine umbenannte oder verschobene Datei wird in 100 % der eindeutigen Fälle ohne erneute
  Übertragung ihres Inhalts auf der Gegenseite nachgezogen.
- **SC-007**: Ein Abgleich eines unveränderten Ordners mit 10 000 Dateien lokal dauert unter 5 Sekunden.
- **SC-008**: Der Nutzer erkennt am Regelstatus ohne weitere Hilfe, ob eine Regel läuft, wartet oder
  einen Fehler hat, und worin der Fehler besteht.

## Assumptions

- Die Gegenseite eines eigenen Geräts wird über den Zugriff aus Spec 056 erreicht; es gibt keinen
  Transport und keinen Dateiindex der Regeln über die Geräte hinweg.
- Ein Ort stellt Größe und Änderungszeit je Datei bereit, so wie der Dateibrowser sie zeigt. Ob ein
  verschlüsselter Ordner (Spec 048) beides im Klartext je Datei liefert, prüft der Plan. Liefert er es
  nicht, entscheidet der Plan über einen Ersatz; die Anforderung, dass ein verschlüsselter Ordner als Ort
  funktioniert, bleibt.
- Ein Speicher ändert die Änderungszeit einer Datei nach dem Kopieren nicht immer auf den Wert der Quelle
  (zum Beispiel S3). Dann nutzt der Stand die vom Ort gemeldete Zeit nach dem Kopieren, nicht die der
  Quelle; die Erkennung vergleicht nur Werte derselben Seite.
- Der Regel-Papierkorb `.holzi-trash` liegt im Ort selbst und wird vom Abgleich nicht übertragen; er
  wird nicht automatisch geleert. Eine Aufräumregel wäre eine spätere Erweiterung.
- Das Intervall für entfernte Orte beträgt standardmäßig 15 Minuten und ist je Regel einstellbar. Bei
  lokalen Orten erkennt ein Dateiwächter Änderungen sofort.
- Die Regeln liegen im Datenbestand des ausführenden Geräts (nicht synchronisiert); ein neues Gerät
  startet ohne Regeln.
- Der Betreiber akzeptiert, dass Regeln nur laufen, solange holzi aktiv ist; auf Android läuft eine
  Regel bei geschlossener App nicht.
- Die Spec-Nummer 061 ist frei (Stand 2026-10-26).

## Nicht im Umfang

- **Hintergrunddienst** (Android-Vordergrunddienst, Autostart auf dem Desktop): eigene Spec.
- **Orte fremder Vaults** (empfangene Dateifreigaben von Kontakten): ausgeschlossen, weil Abgleich und
  Entziehen kollidieren.
- **Eigene Verschlüsselung der Regel** und eigene Schlüssel: nicht vorgesehen; verschlüsselte Ordner
  (Spec 048) decken den Bedarf.
- **Geteilter Dateiindex** über Geräte hinweg und Regeln, die auf mehreren Geräten laufen: nicht
  vorgesehen; jede Regel läuft auf genau einem Gerät.
- **Automatisches Aufräumen des Regel-Papierkorbs**, **Versionsverlauf** und **Bandbreitenbegrenzung**:
  spätere Erweiterungen.
