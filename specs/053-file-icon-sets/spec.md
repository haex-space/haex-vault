# Feature Specification: Austauschbare Icon-Sets für den Dateibrowser

**Feature Branch**: `053-file-icon-sets`

**Created**: 2026-10-14

**Status**: Draft

**Input**: User description: „ich möchte im dateibrowser gerne die icons für ordner und dateien
austauschbar haben. zum einen möchte ich gerne 2-3 icon sets ausliefern, aber dem nutzer auch die
möglichkeit geben, eigene iconsets zu nutzen“ (Betreiber, 2026-10-14). Ergänzungen des Betreibers:
SVG muss mindestens unterstützt sein; die Sets sollen auf allen Geräten gleich sein und deshalb
synchronisiert werden; mitgeliefert werden soll ein Set mit vielen Dateitypen (das Icon-Set von
VS Code als Vorbild) und ein Adwaita-Set; weitere gute Vorschläge sind willkommen.

## Clarifications

### Session 2026-10-15

- Q: Wie fallen die Zustände eines Eintrags auf (verschlüsselter Ordner, gesperrt, ohne Zugriff,
  beschädigt)? → A: Die mitgelieferten Pakete Material und Phosphor tragen dafür eigene, farbige
  Bilder von holzi, in beiden gleich: Grün mit großem Schloss für einen lesbaren verschlüsselten
  Ordner, Rot für gesperrt, Grau mit Verbotszeichen für „kein Zugriff“. Lucide behält seine
  Symbole (Betreiber, 2026-10-15).
- Q: Wie bekommen die Standardordner des Home-Verzeichnisses ihre Icons? → A: Das Material-Paket
  kennt zusätzlich die deutschen, französischen und spanischen Namen (`Bilder`, `Dokumente`,
  `Musik`, `Schreibtisch` …) und nimmt das Bild der englischen Entsprechung (Betreiber, 2026-10-15).

- Q: Welches Set ist der Standard? → A: „Material“ (Betreiber, 2026-10-15). „Lucide“ bleibt das
  Rückfall-Set für alles, was ein Set nicht belegt oder was nicht verfügbar ist.

### Session 2026-10-14

- Q: Welche Bildformate dürfen eigene Sets enthalten? → A: SVG mindestens, zusätzlich PNG und WebP.
  SVG ist das riskanteste der drei Formate (Skripte, Ereignis-Attribute, externe Verweise); darum
  wird es nie als Teil der Seite eingebettet, sondern nur als Bild gezeigt, und beim Import bereinigt
  (FR-010 bis FR-012).
- Q: Gilt die Wahl pro Gerät oder überall? → A: Für alle Geräte derselben Vault. Die Auswahl und die
  eigenen Sets werden über den Datensync (Spec 024) verteilt. „Alle Vaults“ ist als „alle Geräte der
  Vault“ gemeint, denn verschiedene Vaults sind getrennte Datenbanken; bestätigt vom Betreiber.
- Q: Wie beschreibt ein Set seine Zuordnung? → A: Im Format der Icon-Themes von VS Code
  (`icon-theme.json`: Bilddefinitionen, Dateiendungen, Dateinamen, Ordnernamen, Ordner,
  Hell/Dunkel). So lassen sich vorhandene Themes aus dem VS-Code-Umfeld direkt importieren, und die
  mitgelieferten Sets nutzen dasselbe Format.
- Q: Wie kommt ein eigenes Set in holzi? → A: Nur über eine lokale Datei oder einen lokalen Ordner.
  Ein Herunterladen per Link ist nicht Teil dieser Spec.
- Q: Wie wird bei einem importierten Set festgelegt, ob es einfarbig (in der Textfarbe) oder bunt
  ist? → A: Gar nicht: Importierte Sets sind immer bunt, wie bei VS Code. Dort gibt es keine
  Einfärbung; ein Theme liefert fertige Bilder und bei Bedarf Bilder für das helle Schema. Nur die
  mitgelieferten einfarbigen Sets erscheinen in der Textfarbe der Oberfläche. (Das ist nur Lucide;
  die einfarbig gezeichneten Bilder von Phosphor werden beim Bauen in einer hellen und einer dunklen
  Fassung gefärbt, siehe Planung R1 und R2.)
- Q: Soll die Rolle „geöffneter Ordner“ vorkommen? → A: Nein. Der Dateibrowser zeigt nirgends einen
  geöffneten Ordner; die Rolle entfällt. Ein importiertes Theme mit einem Bild für den geöffneten
  Ordner (`folderExpanded`) wird trotzdem übernommen, das Feld wird ignoriert (siehe Annahmen).
- Q: Welches Set mit vielen Dateitypen wird mitgeliefert? → A: Phosphor (MIT) und das Material Icon
  Theme, vollständig (MIT, Teile Apache-2.0). Das Material Icon Theme enthält Logos von Firmen,
  Produkten, Sprachen und Werkzeugen; die MIT-Lizenz räumt darauf keine Rechte Dritter ein. Der
  Betreiber hat das Restrisiko am 2026-10-14 ausdrücklich angenommen („ich denke das Risiko können
  wir eingehen“); eine erste Fassung mit einem logofreien Auszug ist damit abgelöst (FR-017).
- Q (Planung, 2026-10-14): Was ändert die Planung an der Spec? → A: Auf Android gibt es heute keine
  Ordnerauswahl (Spec 043 FR-016), also dort nur das Archiv (FR-009, US3). Die Lizenzprüfung ist in der
  Planung gemacht (research R1); offen ist die fehlende LICENSE-Datei von holzi (Betreiber).
- Q: Welche Sets werden mitgeliefert, und nach welcher Regel? → A: „Lucide“ (bisheriges Aussehen,
  bleibt Standard), „Phosphor“ (viele Dateitypen) und „Material“ (das Material Icon Theme,
  vollständig). Mitgeliefert wird nur, was unter einer permissiven Lizenz steht (MIT, ISC, Apache-2.0,
  CC0) und kein Copyleft verlangt (Betreiber, 2026-10-14: „nur Icons und Icon-Sets, die eine offene
  Lizenz haben und die wir ohne Probleme nutzen können“); für die Logos im Material Icon Theme nimmt
  der Betreiber das Restrisiko an (siehe oben). Adwaita (LGPL-3, CC-BY-SA-Anteile) und vscode-icons
  (CC BY-SA) erfüllen die Regel nicht; sie bleiben vom Nutzer importierbar. Die Lizenzprüfung ist Teil
  dieser Spec (FR-017).

## Begriffe

- **Icon-Set** (kurz: Set): ein benanntes Aussehen für die Bilder von Ordnern und Dateien im
  Dateibrowser. Ein Set ordnet **Rollen**, Dateinamen, Dateiendungen und Ordnernamen jeweils ein Bild
  zu.
- **Rolle**: die Art eines Eintrags, die ein Set kennt, unabhängig vom Namen. Rollen sind: Ordner,
  Ordner ohne Zugriff, Datei, Bild, Video, Audio, Dokument/Text, PDF sowie die
  Zustände der Verschlüsselung aus Spec 048 (verschlüsselter Ordner, nicht lesbarer Ordner, nicht
  lesbare Datei, beschädigt).
- **Mitgeliefertes Set**: gehört zur App, ist nicht löschbar und wird nicht synchronisiert.
- **Eigenes Set**: vom Nutzer importiert, gehört zur Vault, ist entfernbar und wird synchronisiert.
- **Standard-Set**: „Material“, das Set, das eine Vault ohne eigene Wahl hat (Betreiber, 2026-10-15).
- **Rückfall-Set**: „Lucide“, das bisherige Aussehen. Es füllt jede Rolle, die ein anderes Set nicht
  belegt, und antwortet für jedes Set, das nicht verfügbar ist.
- **Einfarbiges Set**: das mitgelieferte Set „Lucide“ aus einfarbigen Symbolen, die holzi in der
  Textfarbe der Oberfläche zeichnet, hell und dunkel passend. Alle anderen Sets, auch „Phosphor“ (in
  einer hellen und einer dunklen Fassung gefärbt) und jedes importierte, sind **bunt**: ihre Bilder erscheinen, wie sie sind, wie bei VS Code.

## Beziehung zu bestehenden Specs

- **Spec 044** (Dateibrowser): Das Bild eines Eintrags (044 FR-003, FR-005) bleibt ein Vorschaubild bei
  Bildern im Raster, sonst ein Icon nach Art; neu ist nur, woher das Icon kommt. Die Einteilung nach
  Dateityp (044 FR-009, FR-014) wird weiter für die Rolle benutzt.
- **Spec 048** (Verschlüsselte Ordner): Die Zustände verschlüsselt, nicht lesbar und beschädigt sind
  Rollen eines Sets; fehlt einem Set ein Bild dafür, gilt das Standard-Set, damit ein Zustand nie
  unsichtbar wird.
- **Spec 023** (Einstellungen) und **Spec 035/036** (Gestaltung): Die Auswahl sitzt in den
  Einstellungen und wird beim Anklicken gespeichert, ohne Speichern-Knopf; „Zurücksetzen“ ist erlaubt.
- **Spec 024** (Sync): Auswahl und eigene Sets sind Vault-Daten und kommen auf alle Geräte der Vault.
- **Spec 043** (Android): Die Auswahl und der Import müssen auch auf dem Telefon gehen.

## User Scenarios & Testing _(mandatory)_

### User Story 1 - Ein anderes Icon-Set wählen (Priority: P1)

Anna öffnet die Einstellungen und sieht in der Gestaltung eine Auswahl „Icon-Set für Dateien“ mit den
mitgelieferten Sets. Zu jedem Set zeigt holzi eine Vorschau: einen Beispielordner mit den typischen
Dateien (Ordner, Bild, Video, Musik, PDF, Text, Programmcode, Archiv). Sie klickt auf „Phosphor“, und
sofort zeigen alle Dateilisten, im Raster wie in der Liste, und die Suchergebnisse das neue Aussehen. Es
gibt nichts zu bestätigen. Beim nächsten Start ist die Wahl noch da.

**Why this priority**: Das ist der Kern des Wunsches. Ohne diese Auswahl gibt es nichts zu tauschen.

**Independent Test**: In den Einstellungen das Set wechseln und im Dateibrowser prüfen, dass Ordner und
Dateien das neue Aussehen haben, ohne die Ansicht neu zu laden; App neu starten und prüfen, dass die
Wahl bleibt.

**Acceptance Scenarios**:

1. **Given** eine frische Vault, **When** Anna den Dateibrowser öffnet, **Then** sehen Ordner und
   Dateien aus wie im Standard-Set „Material“ (bis zum Betreiber-Beschluss vom 2026-10-15 war es
   „Lucide“, das bisherige Aussehen, das jetzt Rückfall-Set ist).
2. **Given** die Auswahl der Sets, **When** Anna ein Set anklickt, **Then** nutzen Liste, Raster und
   Suchergebnisse des Dateibrowsers sofort dieses Set, ohne Speichern-Knopf.
3. **Given** ein gewähltes Set, **When** Anna die App neu startet, **Then** ist dasselbe Set gewählt.
4. **Given** die Auswahl der Sets, **When** Anna sie öffnet, **Then** zeigt jedes Set eine Vorschau mit
   Beispielordner und Beispieldateien in seinem Aussehen, hell und dunkel passend zur Oberfläche.
5. **Given** ein anderes Set als der Standard, **When** Anna „Zurücksetzen“ wählt, **Then** gilt wieder
   das Standard-Set.
6. **Given** Bilder im Raster, **When** das Set gewechselt wird, **Then** zeigen Bilder weiter ihr
   Vorschaubild; das Set gilt nur dort, wo bisher ein Icon steht (044 FR-005).

---

### User Story 2 - Dateitypen erkennbar machen (Priority: P1)

Der Nutzen eines Sets mit „vielen Dateitypen“ liegt darin, dass `foto.png`, `main.rs`, `bericht.pdf` und `daten.csv` verschieden aussehen; ein
importiertes Theme kann das bis zu einzelnen Dateinamen wie `Dockerfile` und Ordnernamen wie `src`
treiben. holzi sucht das Bild eines Eintrags deshalb der Reihe nach:
zuerst nach dem genauen Dateinamen, dann nach der Endung, dann nach der Rolle (Bild, Video, Audio,
Dokument, PDF, sonst Datei). Bei Ordnern zuerst nach dem Ordnernamen, dann nach der Rolle „Ordner“. Was ein Set nicht kennt, übernimmt das Standard-Set. Dadurch ist auch ein unvollständiges
Set sofort brauchbar.

**Why this priority**: Ohne diese Auflösung hätte ein Set nur ein Bild für alle Dateien; sie ist die
Grundlage für ein Set im Stil der VS-Code-Icons.

**Independent Test**: Mit einem Test-Set, das für `Dockerfile`, `rs` und den Ordner `src` je ein Bild
hat und sonst nichts, eine Liste mit diesen Einträgen und unbekannten Dateien zeigen; jeder Treffer
zeigt sein Bild, alle anderen das Standard-Set.

**Acceptance Scenarios**:

1. **Given** ein Set mit Zuordnung für den Dateinamen `Dockerfile` und die Endung `rs`, **When** die
   Liste `Dockerfile` und `main.rs` zeigt, **Then** hat jede Datei ihr eigenes Bild.
2. **Given** eine Datei, deren Name und Endung das Set kennt, **When** das Bild gesucht wird, **Then**
   gewinnt der Name vor der Endung, und die Endung vor der Rolle.
3. **Given** Groß- und Kleinschreibung (`FOTO.PNG`), **When** das Bild gesucht wird, **Then** gilt die
   Endung ohne Rücksicht darauf.
4. **Given** ein Ordner `node_modules` und ein Set mit Bild dafür, **When** die Liste ihn zeigt, **Then**
   hat er dieses Bild; ein Ordner ohne Zuordnung zeigt das Ordner-Bild des Sets.
5. **Given** ein Set ohne Bild für die Rolle „nicht lesbare Datei“, **When** eine solche Datei vorkommt,
   **Then** zeigt holzi das Standard-Bild für diesen Zustand, nie ein leeres Feld.
6. **Given** ein Set mit Varianten für hell und dunkel, **When** die Oberfläche das Farbschema wechselt,
   **Then** wechseln die Bilder mit; ohne Varianten gilt das eine Bild in beiden Schemata (bei einem
   einfarbigen Set in der Textfarbe des jeweiligen Schemas).
7. **Given** ein Eintrag, den das Set nicht kennt, **When** das Bild gesucht wird, **Then** zeigt holzi
   das Standard-Bild der Rolle; es erscheint nie ein „kaputtes Bild“.

---

### User Story 3 - Ein eigenes Set importieren (Priority: P2)

Anna hat ein Icon-Theme aus dem VS-Code-Umfeld heruntergeladen, entpackt in einem Ordner. In den
Einstellungen wählt sie „Eigenes Icon-Set hinzufügen“, zeigt auf den Ordner (auf dem Telefon: auf die Archivdatei), und
holzi prüft ihn. Hat er ein lesbares Manifest und brauchbare Bilder, erscheint das Set mit Vorschau in
der Liste und lässt sich wählen. Sind einzelne Bilder unbrauchbar oder zu groß, nennt holzi sie und
übernimmt den Rest; ist der ganze Ordner kein Set, sagt es, warum, und legt nichts an. Skripte in
SVG-Bildern entfernt holzi, bevor es das Bild speichert.

**Why this priority**: Der zweite Teil des Wunsches („eigene Iconsets“). Mit den mitgelieferten Sets
allein (US1) ist die Funktion schon nützlich, deshalb nicht P1.

**Independent Test**: Ein Beispiel-Theme importieren und wählen (Bilder erscheinen); ein Theme mit
einem SVG, das ein Skript enthält, importieren und prüfen, dass das gespeicherte Bild kein Skript mehr
enthält; einen Ordner ohne Manifest importieren und die Fehlermeldung prüfen.

**Acceptance Scenarios**:

1. **Given** ein Ordner mit `icon-theme.json` und Bildern in SVG, PNG oder WebP, **When** Anna ihn
   importiert, **Then** erscheint ein neues Set mit dem Namen aus dem Manifest (oder dem Ordnernamen)
   in der Liste, mit Vorschau, wählbar.
2. **Given** dieselbe Auswahl als Archivdatei (zip), **When** Anna sie importiert, **Then** kommt dasselbe
   Ergebnis.
3. **Given** ein SVG-Bild mit einem Skript, einem Ereignis-Attribut (zum Beispiel `onload`), einem
   eingebetteten fremden Dokument oder einem Verweis auf eine externe Adresse, **When** das Set
   importiert wird, **Then** speichert holzi das Bild ohne diese Teile (FR-011); das Bild bleibt
   sichtbar, soweit es ohne sie darstellbar ist.
4. **Given** ein Bild, das sich nicht als SVG, PNG oder WebP lesen lässt oder ein Limit überschreitet
   (FR-013), **When** Anna importiert, **Then** übernimmt holzi das Set ohne dieses Bild und nennt es
   mit Dateiname und Grund; die Rollen darauf fallen auf das Standard-Set zurück.
5. **Given** ein Ordner ohne lesbares Manifest oder ohne ein einziges brauchbares Bild, **When** Anna
   importiert, **Then** legt holzi nichts an und sagt in einem Satz, was fehlt.
6. **Given** ein Set mit demselben Namen wie ein vorhandenes eigenes Set, **When** Anna importiert,
   **Then** fragt holzi, ob das vorhandene ersetzt werden soll; ohne Bestätigung bleibt es unberührt.
7. **Given** ein Manifest, das auf Bilder außerhalb des Ordners zeigt (zum Beispiel `../../x.svg` oder
   einen absoluten Pfad), **When** importiert wird, **Then** liest holzi nur Bilder innerhalb des
   gewählten Ordners oder Archivs und meldet die anderen als nicht übernommen.
8. **Given** ein importiertes Set, **When** Anna im Dateibrowser ein Verzeichnis ansieht, **Then**
   erscheinen die Bilder des Sets nur als Bilder, nie als ausführbarer Teil der Seite (FR-010).

---

### User Story 4 - Auswahl und eigene Sets auf allen Geräten (Priority: P2)

Anna wählt auf dem Laptop ein Set und importiert ein eigenes. Auf dem Telefon, das zur selben Vault
gehört, erscheint kurz darauf dasselbe eigene Set in der Liste, und die Wahl gilt dort ebenfalls. Wählt
sie auf dem Telefon ein anderes, gilt das auch auf dem Laptop. Die mitgelieferten Sets liegen auf jedem
Gerät in der App; synchronisiert wird nur, welches gewählt ist.

**Why this priority**: Der Betreiber will, dass das Aussehen überall gleich ist. Die Funktion läuft aber
auch auf einem Gerät allein, deshalb nach der Kernfunktion.

**Independent Test**: Mit zwei Geräten derselben Vault: Auf dem einen ein eigenes Set importieren und
wählen; auf dem anderen prüfen, dass es nach dem Sync da und gewählt ist; dort ein mitgeliefertes
wählen und auf dem ersten prüfen.

**Acceptance Scenarios**:

1. **Given** zwei Geräte einer Vault, **When** auf dem einen ein eigenes Set importiert wird, **Then**
   steht es nach dem Sync auf dem anderen in der Liste.
2. **Given** dieselben Geräte, **When** auf einem ein Set gewählt wird, **Then** gilt es nach dem Sync
   auf dem anderen.
3. **Given** ein eigenes Set, das auf diesem Gerät erst teilweise angekommen ist, **When** es gewählt
   ist, **Then** zeigt holzi das Standard-Set und in den Einstellungen „wird synchronisiert“; ein halbes
   Set erscheint nie.
4. **Given** ein eigenes Set, das auf einem Gerät entfernt wird, **When** der Sync die Änderung bringt,
   **Then** verschwindet es überall; war es gewählt, gilt überall wieder das Standard-Set.
5. **Given** zwei Geräte wählen gleichzeitig verschiedene Sets, **When** beide synchronisieren,
   **Then** gilt auf beiden am Ende dasselbe (die spätere Wahl, Spec 024).
6. **Given** ein gewähltes mitgeliefertes Set, das eine ältere App-Version auf einem anderen Gerät noch
   nicht kennt, **When** dieses Gerät die Wahl erhält, **Then** zeigt es das Standard-Set, bis es die
   App-Version hat, und stürzt nicht.

---

### User Story 5 - Eigene Sets entfernen und Herkunft einsehen (Priority: P3)

Ein eigenes Set, das Anna nicht mehr will, entfernt sie in der Liste; mitgelieferte Sets lassen sich
nicht entfernen. Zu jedem Set zeigt holzi Name, Urheber und Lizenz, damit Anna weiß, woher die Bilder
kommen. Für die mitgelieferten Sets steht dieselbe Angabe auf einer Seite „Lizenzen“ der App.

**Why this priority**: Aufräumen und Herkunft sind wichtig (Lizenzpflicht bei den mitgelieferten Sets),
aber die Funktion läuft auch ohne.

**Independent Test**: Ein eigenes Set entfernen (verschwindet, auch auf anderen Geräten); die
Lizenzseite öffnen und die Angaben zu jedem mitgelieferten Set finden.

**Acceptance Scenarios**:

1. **Given** ein eigenes Set, **When** Anna „Entfernen“ wählt und bestätigt, **Then** verschwindet es
   aus der Liste und von allen Geräten; war es gewählt, gilt das Standard-Set.
2. **Given** ein mitgeliefertes Set, **When** Anna die Liste ansieht, **Then** bietet es kein
   „Entfernen“ an.
3. **Given** ein Set, **When** Anna seine Details öffnet, **Then** zeigt holzi Name, Urheber, Lizenz
   und Quelle, soweit das Set sie nennt.
4. **Given** die Seite „Lizenzen“, **When** Anna sie öffnet, **Then** steht für jedes mitgelieferte Set
   Urheber, Lizenz und der verlangte Hinweis.

---

### Edge Cases

- **Set ohne Zuordnung für Ordner**: Ordner zeigen das Standard-Bild für Ordner (FR-003).
- **Manifest mit Bildverweisen, die nicht existieren**: diese Einträge werden übersprungen und beim
  Import genannt (US3 Szenario 4); das Set bleibt brauchbar.
- **Sehr großes Set** (über dem Limit): holzi lehnt es ab und nennt das Limit (FR-013), statt die
  Synchronisierung zu belasten.
- **Zu viele eigene Sets**: ab dem Limit (FR-013) bietet holzi erst nach dem Entfernen eines Sets den
  Import an.
- **Gewähltes Set fehlt** (entfernt, noch nicht angekommen, Set einer neueren App-Version): das
  Standard-Set gilt, nie ein leerer Dateibrowser; die Einstellungen sagen, warum.
- **Set, dessen Bild sich nicht laden lässt** (zum Beispiel beim Lesen beschädigt): dieser Eintrag
  zeigt das Standard-Bild; der Dateibrowser bleibt bedienbar.
- **Lange Listen**: Bilder werden nur für sichtbare Einträge geladen; ein großes Set verlangsamt das
  Öffnen eines Ordners nicht spürbar (SC-004).
- **Eintrag in einem verschlüsselten Ordner**: wie jeder Eintrag; die Zustände der Verschlüsselung haben
  eigene Rollen und werden nie durch ein Dateityp-Bild verdeckt (Spec 048).
- **SVG mit Animation oder mit Verweis auf Schriften**: Animationen und externe Verweise werden beim
  Import entfernt; das Bild erscheint statisch.
- **Archiv, das sich beim Entpacken über sein Ziel hinaus ausbreitet** (sehr viele oder sehr große
  Dateien, Pfade mit `..`): holzi bricht ab, bevor etwas gespeichert wird (FR-013).
- **Set mit nur einem Teil der Rollen** (zum Beispiel nur Ordner): erlaubt; der Rest kommt vom
  Standard-Set (US2).
- **Erweiterungen**: Eine Erweiterung kann das Set weder lesen noch ändern; sie erhält nur das, was sie
  heute vom Dateibrowser bekommt (FR-016).

## Requirements _(mandatory)_

### Functional Requirements

- **FR-001**: Der Nutzer MUSS in den Einstellungen unter der Gestaltung ein Icon-Set für Ordner und
  Dateien des Dateibrowsers wählen können. Die Wahl MUSS beim Anklicken gelten und gespeichert
  werden, ohne Speichern-Knopf; „Zurücksetzen“ auf das Standard-Set MUSS es geben.
- **FR-002**: Die Wahl MUSS überall gelten, wo der Dateibrowser das Bild eines Eintrags zeigt (Liste,
  Raster, Suchergebnisse) und sofort wirken, ohne die Ansicht neu zu laden. Vorschaubilder von Bildern
  im Raster (044 FR-005) bleiben, wie sie sind.
- **FR-003**: holzi MUSS das Bild eines Eintrags in dieser Reihenfolge bestimmen: (1) Ordner: Name des
  Ordners; Datei: genauer Dateiname, (2) Endung (ohne Rücksicht auf Groß- und Kleinschreibung),
  (3) Rolle, (4) Standard-Set. Fehlt ein Bild auf einer Stufe, MUSS die nächste gelten; es DARF nie
  ein leeres oder kaputtes Bild erscheinen.
- **FR-004**: Ein Set MUSS auch unvollständig brauchbar sein: jede Rolle, die es nicht belegt, MUSS
  das Standard-Set liefern.
- **FR-005**: Die Zustände der Verschlüsselung (Spec 048: verschlüsselt lesbar, nicht lesbar,
  beschädigt) und „Ordner ohne Zugriff“ MÜSSEN Rollen sein, die ein Dateityp-Bild nie verdeckt.
- **FR-006**: Ein Set MUSS Bilder für Hell und Dunkel unterstützen; die Bilder MÜSSEN mit dem
  Farbschema der Oberfläche wechseln. Das einfarbige mitgelieferte Set (Lucide) MUSS in der
  Textfarbe der Oberfläche erscheinen. Ein importiertes Set ist immer bunt: holzi färbt seine Bilder
  nie ein und rät nicht, ob sie einfarbig sind; sein Aussehen im hellen und dunklen Schema bestimmt
  allein das Set durch seine Hell-/Dunkel-Bilder, wie bei VS Code.
- **FR-007**: holzi MUSS diese Sets mitliefern: „Lucide“ (Rückfall, bisheriges Aussehen), „Phosphor“
  (viele Dateitypen: Dokumente, Tabellen, Bilder, Audio, Video, Archive und die häufigen
  Programmiersprachen als Buchstaben-Symbole, dazu Ordner und Schloss) und „Material“ (das Material
  Icon Theme mit Ordnern nach Namen und Bildern für die gängigen Dateitypen und Werkzeuge, wie bei VS
  Code). Weitere Sets DÜRFEN nur mitgeliefert werden, wenn sie FR-017 erfüllen.
- **FR-008**: Jedes Set, mitgeliefert oder eigen, MUSS dasselbe Format haben: ein Manifest im Format
  der Icon-Themes von VS Code (Bilddefinitionen, Dateiendungen, Dateinamen, Ordnernamen, Ordner,
  Hell/Dunkel), ergänzt um die Rollen für die Zustände (FR-005). Eine Angabe, ob ein Set einfarbig ist, gehört
  nicht zum Format, das ein Nutzer schreibt oder importiert; sie ist eine Eigenschaft der
  mitgelieferten Sets.
- **FR-009**: Der Nutzer MUSS ein eigenes Set aus einer lokalen Archivdatei importieren können, auf
  dem Desktop zusätzlich aus einem lokalen Ordner; einen Import über einen Link gibt es nicht. Auf
  dem Telefon gibt es keine Ordnerauswahl (Spec 043 FR-016); dort ist nur das Archiv möglich, und die
  Auswahl „Ordner“ wird ausgeblendet. Der Import MUSS auf Desktop und auf dem Telefon möglich sein
  (Spec 043).
- **FR-010**: Bilder eines Sets MÜSSEN ausschließlich als Bilder angezeigt werden. Ein Bild DARF nie
  als ausführbarer Teil der Seite eingebettet werden; kein Bild DARF Code ausführen, die Oberfläche
  verändern oder Daten nach außen senden.
- **FR-011**: Beim Import MUSS holzi SVG-Bilder bereinigen und nur die bereinigte Fassung speichern:
  Skripte, Ereignis-Attribute, eingebettete fremde Dokumente, Animationen und Verweise auf externe
  Adressen oder Dateien MÜSSEN entfernt werden. Diese Bereinigung MUSS es genau einmal geben, nicht
  einmal in jeder Schicht der App; die Anzeige verlässt sich auf sie und ist zusätzlich nach FR-010
  gesichert.
- **FR-012**: Beim Import MUSS holzi PNG- und WebP-Bilder als solche prüfen (lesbar, Format passt zur
  Endung); Bilder, die sich nicht als SVG, PNG oder WebP lesen lassen, MÜSSEN abgelehnt werden.
- **FR-013**: holzi MUSS Grenzen setzen und beim Überschreiten den Import ablehnen oder das Bild
  auslassen, mit Nennung des Grundes: Gesamtgröße eines Sets, Größe und Pixelmaße eines Bildes, Anzahl
  Bilder und Anzahl eigener Sets, Archive mit überlangem Inhalt oder Pfaden außerhalb des Ziels.
  Ein Archiv DARF nie außerhalb des Ziels schreiben.
- **FR-014**: Ein Manifest DARF nur auf Bilder innerhalb des gewählten Ordners oder Archivs zeigen;
  andere Verweise MÜSSEN abgelehnt werden (US3 Szenario 7).
- **FR-015**: Die Auswahl und die eigenen Sets MÜSSEN Vault-Daten sein und über den Datensync (Spec 024) auf alle Geräte derselben Vault kommen; ein Set DARF auf einem Gerät erst erscheinen, wenn es
  vollständig angekommen ist. Mitgelieferte Sets DÜRFEN nicht synchronisiert werden; nur ihre Wahl.
- **FR-016**: Erweiterungen und Agenten DÜRFEN das Set weder lesen noch ändern, noch eine Wahl treffen;
  das Icon-Set ist eine Einstellung des Nutzers.
- **FR-017**: Ein Set darf nur mitgeliefert werden, wenn es (a) unter einer permissiven Lizenz steht
  (MIT, ISC, Apache-2.0 oder CC0), (b) Teile aus Drittquellen nur enthält, wenn die Quelle benannt und
  ebenfalls permissiv lizenziert ist und im Paket und auf der Lizenzseite steht, und (c) kein
  Copyleft verlangt. Enthält ein Set Logos, Marken oder Zeichen Dritter, MUSS die Lizenzseite sagen,
  dass diese ihren Inhabern gehören und nur den Dateityp kennzeichnen; das gilt für das Material
  Icon Theme, dessen Restrisiko der Betreiber angenommen hat. Die Prüfung MUSS vor der Aufnahme
  geschehen und mit Quelle, Version und Lizenztext festgehalten werden. Urheber, Lizenz und der
  verlangte Hinweis MÜSSEN in der App auf einer Seite „Lizenzen“ stehen. Für ein vom Nutzer
  importiertes Set gilt die Regel nicht (er bringt es mit); die Importvorschau zeigt Urheber und
  Lizenz an, soweit das Set sie nennt, und weist darauf hin, dass der Nutzer für die Lizenz
  verantwortlich ist.
- **FR-018**: Die Bilder eines Sets MÜSSEN erst geladen werden, wenn ein Eintrag sichtbar wird; ein
  Set DARF nicht bei jedem Start als Ganzes geladen werden. Ein Wechsel des Sets MUSS auch bei einem
  Set mit tausend Bildern ohne spürbare Pause geschehen.
- **FR-019**: Ein eigenes Set MUSS der Nutzer entfernen können (mit Bestätigung); war es gewählt, MUSS
  überall das Standard-Set gelten. Mitgelieferte Sets DÜRFEN nicht entfernbar sein.
- **FR-020**: Ist das gewählte Set nicht verfügbar (entfernt, noch nicht synchronisiert, aus einer
  neueren App-Version), MUSS holzi das Standard-Set zeigen und in den Einstellungen den Grund nennen.
- **FR-021**: Ein Set MUSS in den Einstellungen mit Vorschau stehen (Beispielordner mit typischen
  Dateien), in der gewählten Hell-/Dunkel-Darstellung; zu jedem Set MÜSSEN Name, Urheber, Lizenz und
  Quelle einsehbar sein, soweit das Set sie nennt.
- **FR-022**: Alle neuen Texte MÜSSEN übersetzt sein; der Begriff ist „Icon-Set“, nicht „Theme“ oder
  „Layout“. Die Bedienung MUSS auf Desktop und Telefon gehen und die vorhandene Gestaltung der
  Einstellungen übernehmen (Group, Row, OptionRow; Spec 023, 035/036).

### Key Entities

- **Icon-Set**: ein benanntes Aussehen mit Manifest, Bildern, Herkunft (Urheber, Lizenz, Quelle). Entweder mitgeliefert oder eigen.
- **Zuordnung**: Teil eines Sets; bildet Dateinamen, Endungen, Ordnernamen und Rollen auf Bilder ab,
  jeweils mit Varianten für Hell und Dunkel.
- **Rolle**: Art eines Eintrags unabhängig vom Namen (Ordner, Datei, Bild, Video,
  Audio, Dokument, PDF, Zustände der Verschlüsselung, ohne Zugriff).
- **Auswahl**: das gewählte Set der Vault; ein Vault-Datum, das mit dem Sync auf alle Geräte kommt.
- **Eigenes Set**: Set aus einem Import; Vault-Datum, entfernbar, synchronisiert.

## Success Criteria _(mandatory)_

### Measurable Outcomes

- **SC-001**: Ein Set zu wechseln dauert höchstens zwei Klicks in den Einstellungen, und die geöffnete
  Dateiliste zeigt das neue Aussehen innerhalb einer Sekunde, ohne Neuladen.
- **SC-002**: Mit „Material“ haben mindestens 50 häufige Ordnernamen (zum Beispiel `src`, `docs`,
  `images`, `config`, `test`) ein eigenes Ordner-Bild und mindestens 100 häufige Dateinamen und
  Endungen (zum Beispiel `readme`, `license`, `json`, `yaml`, `zip`, `rs`, `ts`, `vue`, `py`)
  ein passendes Bild. Mit „Phosphor“ haben mindestens 40 häufige Dateiendungen (zum Beispiel `pdf`,
  `docx`, `xlsx`, `pptx`, `zip`, `png`, `mp3`, `mp4`, `rs`, `ts`, `vue`, `py`, `md`, `csv`) ein
  passendes Bild statt des allgemeinen Datei-Bildes, und die Zustände Ordner, Ordner ohne Zugriff und
  Schloss sind belegt.
- **SC-003**: Ein importiertes Theme aus dem VS-Code-Umfeld mit gültigem Manifest wird in höchstens
  einer Minute übernommen und erscheint ohne Handarbeit wählbar.
- **SC-004**: Das Öffnen eines Ordners mit 1000 Einträgen dauert mit einem großen Set nicht spürbar
  länger als mit dem Standard-Set (Unterschied unter 200 ms), und der Speicherverbrauch wächst nicht
  mit der Größe des Sets, sondern mit der Zahl der sichtbaren Einträge.
- **SC-005**: Kein importiertes Bild kann Code ausführen oder Daten nach außen senden: In einem Test
  mit Bildern, die Skripte, Ereignis-Attribute und externe Verweise enthalten, wird nichts davon
  gespeichert oder ausgeführt.
- **SC-006**: Eine Wahl oder ein neues eigenes Set erscheint auf einem zweiten Gerät derselben Vault
  nach dem Sync, ohne dass der Nutzer dort etwas tut; ein halbes Set wird nie sichtbar.
- **SC-007**: In 100 % der Fälle, in denen ein Set eine Rolle nicht belegt oder nicht verfügbar ist,
  erscheint ein Bild des Standard-Sets; nie ein leeres oder kaputtes Bild.
- **SC-008**: Für jedes mitgelieferte Set sind Urheber, Lizenz und Hinweis auf der Seite „Lizenzen“
  zu finden, und keines hat eine Lizenz, die das Mitliefern verbietet.

## Assumptions

- „Alle Vaults gleich“ bedeutet: alle Geräte derselben Vault (Klärung); verschiedene Vaults sind
  getrennte Datenbanken und haben je ihre Wahl.
- Die Auswahl sitzt in den Einstellungen unter der Gestaltung als eigene Gruppe „Icon-Set für Dateien“,
  im Stil der übrigen Gruppen. Eine eigene Kategorie ist nicht nötig.
- Die Bilder der Orte in der Seitenleiste des Dateibrowsers (Dokumente, Downloads und so weiter) und die
  Icons der Bedienelemente bleiben unverändert; diese Spec betrifft das Bild eines Eintrags (Ordner und
  Datei) in Liste, Raster und Suchergebnissen.
- Vorgeschlagene Grenzen (die Planung darf sie anpassen): höchstens 5 MB je Set, höchstens 2000 Bilder
  je Set, höchstens 256 KB je Bild, Pixelbilder höchstens 512 × 512 px, höchstens 10 eigene Sets. Die
  Grenzen gelten für eigene Sets; die mitgelieferten erfüllen sie ebenfalls.
- Die Sets mit vielen Dateitypen sind Phosphor (MIT, ein Urheber) und das Material Icon Theme (MIT,
  Teile von Pictogrammers und Google unter Apache-2.0, Release 5.39.0, vollständig); die
  Lizenzprüfung steht in der Planung (research R1). Phosphor ist einfarbig gezeichnet und wird beim
  Bauen in einer hellen und einer dunklen Fassung gefärbt; es hat keine Zuordnung nach Dateinamen
  oder Ordnernamen, die Endungen werden von uns auf die vorhandenen Bilder abgebildet. Material
  bringt Ordnernamen, Dateinamen und Endungen mit, auch für Sprachen und Werkzeuge. Adwaita,
  Catppuccin, Seti, vscode-icons, Tabler und Papirus werden nicht mitgeliefert (Copyleft, Logos
  ohne Mehrwert oder zu kleine Abdeckung); der Nutzer kann sie importieren.
- Der Import liest nur lokale Dateien; ein Import per Link (mit eigenen Sicherheitsfragen: Herkunft,
  Prüfung, Offline) kann später als eigene Spec folgen.
- Das Format der Icon-Themes von VS Code wird gelesen, soweit es Dateien, Endungen, Ordner und
  Hell/Dunkel betrifft; das Bild für den geöffneten Ordner (`folderExpanded`) gehört dazu nicht; Teile, die für den Dateibrowser keine Bedeutung haben (zum Beispiel
  Sprach-Kennungen von Editoren), werden ignoriert, ohne dass der Import scheitert.
- Eigene Sets teilen sich den Datensync mit den übrigen Vault-Daten; ein Limit von 5 MB je Set und 10
  Sets hält die zusätzliche Last begrenzt.
- Nicht im Umfang: Herunterladen von Sets per Link, ein Editor für Icons oder Sets, Icons für einzelne
  Ordner des Nutzers („dieser Ordner bekommt ein Herz“), Icons für Aktionen und Bedienelemente der
  App, ein Set, das Vorschaubilder erzeugt.
