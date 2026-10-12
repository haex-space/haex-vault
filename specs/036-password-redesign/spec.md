# Feature Specification: Passwortmanager-Redesign

**Feature Branch**: `036-password-redesign`
**Created**: 2026-10-03
**Status**: Draft
**Input**: Der Passwortmanager aus Spec 034 bekommt die Bedienung des Passwortmanagers von
haex-vault: einen Eintrag in drei Tabs (Details, Extra, Verlauf) mit Wischgeste zwischen
den Tabs, Brotkrumen, Auswahlleiste, Kontextmenüs, Ausschneiden, Kopieren und Einfügen und
Tastaturkürzel in der Liste, Passkeys als Dienstfunktionen (anlegen, abrufen und signieren,
auflisten) mit Aufrufer und Bereich (ein Passkey gehört immer zu einem Eintrag und folgt
dessen Tags), und Dateianhänge als Karten mit Vorschau und Lightbox. Die Felder und die
Darstellung aus Spec 035 sind die Grundlage der Oberfläche. Die External Bridge (Autofill,
Browser-Erweiterung) ist nicht Teil dieser Spec. Referenz: haex-vault @
`8dce379d94e18fcd42c3b73686a06f984ca3f574`, `src/components/haex/system/passwords/`.

## Beziehung zu bestehenden Specs

- [`034-password-manager`](../034-password-manager/spec.md): Diese Spec ändert dessen
  Oberfläche und erweitert den Dienst um Passkey-Funktionen. Datenmodell, Zugriffsprüfung
  (FR-024 bis FR-032 dort), Papierkorb, Verlauf, Import, Anhänge im Datenbestand und der
  Sync bleiben, wie sie sind. Wo diese Spec ein Verhalten von 034 ersetzt, steht es unten
  ausdrücklich („ersetzt“ oder „ändert“ mit der Nummer der Anforderung aus 034).
- [`035-appearance-and-fields`](../035-appearance-and-fields/spec.md): Alle Eingaben der
  neuen Tabs und Dialoge benutzen die Felder aus 035 (Schwebelabel, Ring in der
  Akzentfarbe); Farben kommen aus den Darstellungs-Werten, nie fest verdrahtet.
- [`020-tab-navigation`](../020-tab-navigation/spec.md) und
  [`022-session-restore`](../022-session-restore/spec.md): Der gewählte Tab eines Eintrags
  gehört zum Ort des Fenster-Tabs (Vor, Zurück, Sitzung wiederherstellen). Ein Ort enthält
  nie ein Geheimnis.
- [`030-app-multi-instance`](../030-app-multi-instance/spec.md): Mehrere Fenster des
  Passwortmanagers sind möglich; was zwischen ihnen geteilt wird, regelt FR-021.
- [`024-own-device-sync`](../024-own-device-sync/spec.md): Passkey-Zähler und alle neuen
  Schreibvorgänge sind gewöhnliche Vault-Daten und gehen über den Sync der eigenen Geräte.
- [`015-workspace-shell`](../015-workspace-shell/spec.md): Tastaturkürzel der Liste gelten
  nur im Passwortmanager-Fenster und kollidieren nicht mit den Kürzeln des Window Managers
  (wm).
- Specs **017–019 (haextensions)**, 017 im Bau, und die geplante Spec **021 (MCP-Server)**:
  Sie vergeben die Freigaben; diese Spec legt fest, was eine Freigabe für Passkeys bedeutet
  (FR-030 bis FR-036).

## Clarifications

### Session 2026-10-03

- Q: Wer darf einen eigenständigen Passkey (ohne Eintrag) sehen, anlegen und benutzen? → A:
  Es gibt keinen eigenständigen Passkey. Ein Passkey ist immer Teil eines gewöhnlichen
  Eintrags, und dieser Eintrag trägt Tags. Jede Erweiterung oder die External Bridge mit einer
  Freigabe für ein Tag dieses Eintrags darf alles zu diesem Eintrag sehen (nach Art der Freigabe)
  und auch dessen Passkeys benutzen. Eine eigene Ansicht für Passkeys, ein Zuordnen und ein
  Lösen entfallen; ein Passkey wird am Eintrag verwaltet.
- Q: Dürfen externe Agenten über MCP Passkeys nur auflisten oder auch anlegen und bestätigen?
  → A: Sie sind Aufrufer wie Erweiterungen: Mit einer Freigabe für ein Tag des Eintrags dürfen
  sie alles, was die Art der Freigabe deckt. Nur der eingebaute Agent im Chat bekommt keine
  Passkey-Funktion.
- Q: Gilt die Ablage in allen Fenstern des Passwortmanagers oder nur im Fenster, in dem sie
  gefüllt wurde? → A: In allen Fenstern des Passwortmanagers im Prozess (FR-021); sie
  verschwindet mit dem letzten geschlossenen Fenster.
- Q: Was kommt beim Ausschneiden und Kopieren ganzer Einträge und Ordner in die Zwischenablage
  des Betriebssystems? → A: Nichts. Die Ablage der Einträge ist intern. Die normale
  Zwischenablage gehört den einzelnen Werten (Benutzername, Passwort, TOTP-Code, Adresse), die
  der Nutzer ausdrücklich kopiert, mit dem automatischen Leeren aus 034; sie lassen sich überall
  einfügen, auch im Chatfenster, in einer Erweiterung oder im Browser.
- Q: Was wird beim Kopieren eines Eintrags mitgenommen, und wie heißt die Kopie? → A: Alles.
  Beim Einfügen einer Kopie fragt ein Dialog (auf schmalen Fenstern eine Schublade) per
  Checkbox, ob der Verlauf übernommen wird, und ob Benutzername, Passwort und Passkeys als
  Verweis statt als Wert übernommen werden; alle anderen Werte werden immer als Wert kopiert.
  Der Titel ist im Dialog änderbar, Vorgabe ist der Originaltitel mit angehängtem „Kopie“
  (Deutsch) oder „Copy“ (Englisch) je nach Sprache der Oberfläche.
- Q: Wie funktionieren Verweise („by reference“)? → A: Wie in KeePass als Platzhalter im Text
  eines Werts: `{$<Eintrag>:username}`, `{$<Eintrag>:password}` und
  `{$<Eintrag>:extra:<Schlüssel>}` für das Feld eines eigenen Schlüssel/Wert-Paares. Das ist ein
  einheitliches System für Verweise zwischen Einträgen, nicht nur beim Kopieren; der Kopier-Dialog
  setzt solche Platzhalter. Passkeys sind kein Text und werden per Verbindung verwiesen (FR-046).
  In haex-vault, vault-sdk, haextension und atoms gibt es kein solches System (nur Anhänge
  verweisen dort über die Prüfsumme); es ist neu. Die Tiefe ist wie in KeePass 12 Stufen. Beim
  KeePass-Import werden dessen Verweise `{REF:…}` in dieses System umgewandelt, soweit
  eindeutig (haex-vault tut das nicht).

### Session 2026-10-11

- Q: Was zeigt die obere Leiste, solange ein Eintrag offen ist (Ansicht, Bearbeiten, Neu)? → A:
  Suche, Tag-Filter und „+“ ergeben dort keinen Sinn und entfallen; der Knopf der Seitenleiste
  und das Menü bleiben. Die Ansicht des Eintrags zeigt über sich die Brotkrumen wie die Liste,
  mit dem Titel des Eintrags als letztem Teil (Betreiber).

## User Scenarios & Testing _(mandatory)_

### User Story 1 - Einen Eintrag in Tabs lesen und bearbeiten (Priority: P1)

Der Nutzer öffnet einen Eintrag. Statt einer langen Seite sieht er drei Tabs: **Details**
(Titel, Benutzername, Passwort, Adresse, Tags, Notiz, Ablaufdatum, Symbol und Farbe, TOTP),
**Extra** (eigene Felder, Anhänge, Passkeys, Autofill-Aliase) und **Verlauf**. Er wechselt
durch Tippen auf den Tab oder durch Wischen nach links und rechts; die Tabs folgen der
Wischgeste mit. Leere Felder sind in der Ansicht ausgeblendet, beim Bearbeiten sind alle
da. Fehlt beim Speichern der Titel, springt die Ansicht zum Tab Details und markiert das
Feld.

**Why this priority**: Der Eintrag ist die Stelle, an der der Nutzer die meiste Zeit
verbringt; auf einem Telefon ist die lange Seite am unhandlichsten.

**Independent Test**: Einen Eintrag mit TOTP, eigenen Feldern, einem Anhang und einem
Passkey öffnen, mit Tippen und mit Wischen durch die Tabs gehen, im Bearbeiten den Titel
leeren und speichern.

**Acceptance Scenarios**:

1. **Given** ein geöffneter Eintrag, **When** der Nutzer auf „Extra“ tippt oder nach links
   wischt, **Then** erscheint der Tab Extra mit eigenen Feldern, Anhängen, Passkeys und
   Aliasen, und der gewählte Tab ist hervorgehoben.
2. **Given** der Tab Extra, **When** der Nutzer nach rechts wischt, **Then** kommt er zu
   Details zurück; am ersten und letzten Tab geschieht beim Weiterwischen nichts.
3. **Given** ein Eintrag im Bearbeiten, **When** der Nutzer den Titel leert und speichert,
   **Then** wird nichts gespeichert, der Tab Details wird gewählt, und das Feld nennt den
   Fehler.
4. **Given** ein Eintrag im Bearbeiten, **When** der Nutzer zwischen Details und Extra
   wechselt, **Then** bleiben seine ungespeicherten Eingaben in beiden Tabs erhalten.
5. **Given** ein Eintrag ohne Notiz und ohne Ablaufdatum, **When** er ihn ansieht, **Then**
   sind diese beiden Felder nicht zu sehen; beim Bearbeiten erscheinen sie.
6. **Given** ein Fenster mit 360 px Breite, **When** er die Tabs benutzt, **Then** ist alles
   ohne waagerechtes Scrollen der Seite erreichbar, und eine waagerechte Wischgeste auf
   einem Eingabefeld oder einem Code verwechselt holzi nicht mit einem Tabwechsel.
7. **Given** die Systemeinstellung „Bewegung reduzieren“, **When** er die Tabs wechselt,
   **Then** erscheint der neue Tab ohne Gleitbewegung.

---

### User Story 2 - Den Verlauf als Zeitleiste lesen und einen alten Stand wiederherstellen (Priority: P1)

Der Tab **Verlauf** zeigt die Stände eines Eintrags als Zeitleiste, neuester oben: je Stand
ein Punkt auf einer Linie, bei genügend Breite mit der Zeit („vor 2 Tagen“) und den Namen
der geänderten Felder. Der neueste Stand ist vorgewählt. Rechts (bei schmalem Fenster
darunter) steht der gewählte Stand schreibgeschützt, Geheimnisse maskiert und auf
Wunsch aufdeckbar. „Diesen Stand wiederherstellen“ bleibt erhalten und legt, wie in 034,
einen neuen Stand an, statt den Verlauf zu kürzen.

**Why this priority**: Der Verlauf ist schon da (034) und der Anlass, ihn von einer eigenen
Seite in einen Tab zu holen; er darf dabei nichts von seinem Nutzen verlieren.

**Independent Test**: Einen Eintrag dreimal ändern, den Tab Verlauf öffnen, den ältesten
Stand wählen, ein Geheimnis aufdecken und den Stand wiederherstellen.

**Acceptance Scenarios**:

1. **Given** ein Eintrag mit vier Ständen, **When** der Nutzer den Tab Verlauf öffnet,
   **Then** sieht er vier Punkte, neuester oben und gewählt, jeder mit Zeit und den Namen der
   geänderten Felder (außer bei sehr schmalem Fenster, wo nur die Punkte stehen und die
   Angaben im gewählten Stand erscheinen).
2. **Given** ein gewählter Stand, **When** er ein Geheimnis darin aufdeckt, **Then** sieht er
   es, und beim Wechsel zu einem anderen Stand oder Tab ist es wieder verborgen.
3. **Given** ein gewählter älterer Stand, **When** er ihn wiederherstellt und bestätigt,
   **Then** hat der Eintrag dessen Werte, und der Verlauf hat einen neuen obersten Stand;
   die älteren Stände bleiben.
4. **Given** ein Eintrag im Bearbeiten, **When** der Nutzer die Tabs ansieht, **Then** ist
   der Verlauf nicht wählbar (er gilt für den gespeicherten Eintrag); nach dem Speichern
   oder Verwerfen ist er wieder da.
5. **Given** ein Eintrag ohne Änderungen seit dem Anlegen, **When** er den Verlauf öffnet,
   **Then** steht dort der eine Anfangsstand und ein Hinweis, dass es nichts Älteres gibt.
6. **Given** die Adresse des Eintrags im Verlauf (`entry/:id/history` aus 034), **When** sie
   geöffnet wird (Vor, Zurück, wiederhergestellte Sitzung), **Then** öffnet sie den Eintrag
   auf dem Tab Verlauf.

---

### User Story 3 - Zurechtfinden und Einträge verschieben mit Brotkrumen, Auswahl und Ablage (Priority: P1)

Die Liste zeigt über sich die **Brotkrumen** des aktuellen Ordners („Alle Einträge › Arbeit
› Server“); jeder Teil außer dem letzten ist ein Ziel zum Hineinspringen. Wer mehrere
Einträge oder Ordner markiert, sieht an Stelle der Brotkrumen die **Auswahlleiste** mit der
Anzahl, „Alle auswählen“ und den Aktionen **Ausschneiden**, **Kopieren**, Tags, Löschen und
(bei genau einem Eintrag) Bearbeiten. Solange er Einträge zieht, stehen dort wieder die
Brotkrumen, damit er die Auswahl auf einen ihrer Teile fallen lassen kann (Spec 043, T080). Ausgeschnittene oder kopierte Einträge liegen in der
**Ablage** der Einträge; im Zielordner fügt der Nutzer sie mit **Einfügen** ein. Ausgeschnitten
heißt verschieben, kopiert heißt Kopien anlegen, auch von ganzen Ordnern samt Inhalt.

**Why this priority**: Wer viele Einträge ordnet (etwa nach einem Import), braucht mehr als
das einzelne Ziehen aus 034, vor allem auf einem Telefon, wo Ziehen mühsam ist.

**Independent Test**: In einer Vault mit drei Ordnerebenen einen Ordner mit Einträgen
kopieren und in einen anderen Ordner einfügen, zwei Einträge ausschneiden und woanders
einfügen, über die Brotkrumen zurückspringen, Einfügen eines Ordners in seinen eigenen
Unterordner versuchen.

**Acceptance Scenarios**:

1. **Given** der Ordner „Server“ unter „Arbeit“, **When** die Liste ihn zeigt, **Then** stehen
   darüber die Teile „Alle Einträge“, „Arbeit“ und „Server“; ein Tipp auf „Arbeit“ zeigt
   dessen Inhalt, im Papierkorb, in einer Tag-Ansicht und in der Suche steht dort stattdessen
   deren Name.
2. **Given** drei markierte Einträge, **When** die Auswahlleiste erscheint, **Then** zeigt sie
   „3 ausgewählt“, ein Kontrollkästchen „Alle auswählen“ (teilweise gefüllt, wenn nicht alle
   markiert sind) und die Aktionen; bei sehr schmaler Leiste stehen die Aktionen in einem
   Überlaufmenü.
3. **Given** zwei markierte Einträge, **When** der Nutzer „Ausschneiden“ wählt, in einen
   anderen Ordner geht und „Einfügen“ wählt, **Then** liegen die Einträge dort und nicht mehr
   im alten Ordner; die Ablage ist leer.
4. **Given** ein markierter Ordner mit Unterordnern und Einträgen, **When** der Nutzer
   „Kopieren“ und im Zielordner „Einfügen“ wählt, **Then** erscheint der Kopier-Dialog
   (FR-015), und nach dem Bestätigen gibt es dort eine Kopie des Ordners mit allen
   Unterordnern und Einträgen; das Original bleibt unverändert, die Ablage bleibt gefüllt
   (mehrfaches Einfügen ist möglich). Bricht der Nutzer den Dialog ab, entsteht nichts.
5. **Given** ein ausgeschnittener Ordner, **When** der Nutzer ihn in sich selbst oder einen
   seiner Unterordner einfügen will, **Then** lehnt holzi ab, nennt den Grund, verschiebt
   nichts und lässt die Ablage gefüllt.
6. **Given** ausgeschnittene Einträge, **When** die Liste sie zeigt, **Then** sind sie
   abgeblendet, bis sie eingefügt oder die Ablage geleert wird.
7. **Given** eine gefüllte Ablage und keine Auswahl, **When** die Liste zu sehen ist, **Then**
   zeigt die Leiste „n in der Ablage“ mit „Einfügen“ und „Ablage leeren“.
8. **Given** ein Telefon, **When** der Nutzer lange auf einen Eintrag drückt, **Then** beginnt
   die Auswahl mit diesem Eintrag, und ein Tipp auf weitere Einträge fügt sie hinzu.
9. **Given** ein Eintrag oder Ordner, der auf einem anderen Gerät gelöscht wurde, während er
   in der Ablage liegt, **When** der Nutzer einfügt, **Then** werden die übrigen eingefügt,
   und holzi nennt, wie viele nicht mehr da waren.
10. **Given** ein gezogener Eintrag, der Teil einer Auswahl ist, **When** der Nutzer ihn auf
    einen Ordner, einen Teil der Brotkrumen oder „Alle Einträge“ fallen lässt, **Then** wird
    die ganze Auswahl dorthin verschoben (nicht nur der gezogene Eintrag).

---

### User Story 4 - Kontextmenüs und Tastaturkürzel (Priority: P2)

Ein Rechtsklick (oder ein Menüknopf an der Zeile, wo es keine rechte Maustaste gibt) öffnet
das Menü des Eintrags, des Ordners oder der leeren Fläche. Am Rechner arbeiten Tastaturkürzel
auf der Liste: Alles auswählen, Ausschneiden, Kopieren, Einfügen, Löschen, Öffnen, Suche und
der schnelle Griff nach Benutzername und Passwort eines Eintrags, ohne ihn zu öffnen.

**Why this priority**: Die Menüs und Kürzel machen die Aktionen aus Story 3 schnell und
auffindbar, sind aber ohne sie über die Auswahlleiste ebenfalls erreichbar.

**Independent Test**: Auf einen Eintrag, einen Ordner und die leere Fläche rechtsklicken und
jede Aktion auslösen; mit der Tastatur zwei Einträge auswählen, ausschneiden, in einen
Ordner gehen, einfügen; Benutzername und Passwort per Kürzel kopieren.

**Acceptance Scenarios**:

1. **Given** ein Eintrag, **When** der Nutzer das Menü öffnet, **Then** bietet es Öffnen,
   Benutzername kopieren, Passwort kopieren (ausgegraut, wenn keins da ist), Ausschneiden,
   Kopieren, Löschen; im Papierkorb stattdessen Wiederherstellen und Endgültig löschen.
2. **Given** ein Ordner, **When** der Nutzer das Menü öffnet, **Then** bietet es Öffnen,
   Bearbeiten, Neuer Unterordner, Ausschneiden, Kopieren, Einfügen (wenn die Ablage gefüllt
   ist, und dann in diesen Ordner), Löschen; im Papierkorb Wiederherstellen und Endgültig
   löschen.
3. **Given** die leere Fläche der Liste, **When** der Nutzer das Menü öffnet, **Then** bietet
   es Neuer Eintrag, Neuer Ordner und Einfügen (wenn die Ablage gefüllt ist).
4. **Given** die Liste mit Fokus, **When** der Nutzer die Kürzel aus FR-016 benutzt, **Then**
   tun sie, was dort steht, und nie etwas, solange der Fokus in einem Eingabefeld, einem
   Dialog oder einem markierten Text steht (dort gilt das übliche Verhalten, etwa Kopieren
   des markierten Texts).
5. **Given** ein Passwort, das per Kürzel kopiert wurde, **When** die Zwischenablage-
   Einstellung aus 034 gilt, **Then** wird sie nach der dort eingestellten Zeit geleert, wie
   beim Kopieren aus dem Eintrag.
6. **Given** ein Gerät ohne Maus oder Tastatur, **When** der Nutzer eine Aktion aus einem
   Menü oder Kürzel braucht, **Then** erreicht er sie über die Auswahlleiste oder die Seite
   des Eintrags.

---

### User Story 5 - Passkeys: anlegen, benutzen, auflisten, am Eintrag verwalten (Priority: P2)

Heute zeigt der Passwortmanager Passkeys nur an (aus Import oder Sync). Jetzt bietet sein
Dienst Funktionen, mit denen ein berechtigter Aufrufer einen Passkey **anlegt** (holzi erzeugt
das Schlüsselpaar), eine Anmeldung **bestätigt** (holzi signiert die Aufgabe der Gegenstelle
mit dem privaten Schlüssel, der Schlüssel verlässt den Dienst nie) und Passkeys **auflistet**.
Aufrufer sind die haextensions, externe Agenten über MCP und später die External Bridge,
nicht ein Mensch an der Oberfläche. Ein Passkey ist immer Teil eines gewöhnlichen Eintrags;
wer für ein Tag dieses Eintrags freigegeben ist, sieht den Eintrag samt seinen Passkeys und
kann sie benutzen. Der Nutzer sieht und verwaltet die Passkeys eines Eintrags in dessen Tab
Extra.

**Why this priority**: Ohne diese Funktionen kann die External Bridge später nichts tun, und
für den Alltag ist es nachrangig, weil es noch keine Browser-Anbindung gibt.

**Independent Test**: Mit der Test-Freigabe einer Erweiterung einen Passkey für „example.com“
anlegen, die Anmeldung bestätigen und die Signatur mit dem öffentlichen Schlüssel prüfen,
den Passkey im Tab Extra des Eintrags sehen, umbenennen und löschen.

**Acceptance Scenarios**:

1. **Given** ein Aufrufer mit Freigabe „Lesen und Schreiben“ für das Tag „web“ und ein
   Eintrag mit diesem Tag, **When** er einen Passkey für eine Gegenstelle (Kennung, Name,
   Benutzer, Aufgabe) an diesem Eintrag anlegt, **Then** hängt am Eintrag ein neuer Passkey mit
   eigenem Schlüsselpaar und Zähler 0, und der Aufrufer erhält die Antwort, die die Gegenstelle
   zum Registrieren braucht; der private Schlüssel steht in keiner Antwort. **When** er es für
   einen Eintrag ohne Tag aus seinem Bereich oder ohne Angabe eines Eintrags versucht, **Then**
   wird abgelehnt.
2. **Given** ein Passkey und eine Aufgabe der Gegenstelle, **When** ein Aufrufer mit Freigabe
   „Lesen“ die Anmeldung bestätigt, **Then** erhält er eine gültige Signatur über die Aufgabe,
   der Zähler bleibt bei 0, und „zuletzt benutzt“ wird gesetzt; eine Signatur, die die
   Gegenstelle mit dem öffentlichen Schlüssel prüft, ist gültig. UP und UV werden ohne
   vertrauenswürdigen, zeremoniegebundenen Presence-Nachweis nicht gesetzt.
3. **Given** eine Anfrage, deren Herkunft nicht zur Kennung der Gegenstelle des Passkeys
   passt, **When** der Aufrufer bestätigen will, **Then** wird abgelehnt und nichts signiert.
4. **Given** eine Anfrage mit einer Liste ausgeschlossener Passkeys, **When** ein passender
   schon existiert, **Then** legt holzi keinen neuen an und sagt es dem Aufrufer.
5. **Given** mehrere passende Passkeys an Einträgen im Bereich des Aufrufers, **When** er sie
   auflistet, **Then** erhält er Kennung, Gegenstelle, Benutzer, Spitzname, Zeiten, den
   Eintrag und ob der Passkey auffindbar ist, nie Schlüssel; er kann nach Gegenstelle und
   Eintrag filtern; Passkeys an Einträgen außerhalb seines Bereichs fehlen, ohne dass er es
   erfährt.
6. **Given** ein Eintrag mit zwei Passkeys, **When** der Nutzer den Tab Extra öffnet, **Then**
   sieht er sie mit Spitzname oder Gegenstellenname, Gegenstelle, Benutzer, Anlegedatum und
   zuletzt benutzt und kann sie umbenennen oder löschen.
7. **Given** ein Passkey, **When** der Nutzer ihn löscht und bestätigt, **Then** ist er weg,
   und holzi nennt in der Bestätigung die Gegenstelle, damit er weiß, dass er sich dort
   nicht mehr mit ihm anmelden kann.
8. **Given** ein Passkey, den zwei Geräte benutzt haben, **When** die Geräte sich über den
   Sync treffen, **Then** bleibt `signCount` auf beiden Geräten 0; ein synchronisierter
   Nicht-Null-Zähler wird nicht vorgetäuscht.

---

### User Story 6 - Anhänge als Karten mit Vorschau (Priority: P3)

Im Tab Extra stehen die Anhänge als **Karten** in einem Raster, das mit der Breite von drei
auf eine Spalte schrumpft. Jede Karte zeigt ein Vorschaubild (Bilder) oder ein Symbol nach
Dateityp, den Namen, die Größe und den Typ und hat Knöpfe für Speichern unter, Umbenennen und
Entfernen. Ein Tipp auf ein Bild öffnet eine **Lightbox**: das Bild groß, zoombar, mit Pfeilen
oder Wischen zum vorigen und nächsten Bild des Eintrags, mit Schließen und Speichern unter.

**Why this priority**: Anhänge funktionieren schon (034); Karten und Lightbox sind Komfort.

**Independent Test**: Einem Eintrag drei Bilder und ein PDF anhängen, das Raster auf einem
schmalen Fenster ansehen, durch die Bilder in der Lightbox blättern, ein PDF-Symbol
antippen, einen Anhang umbenennen.

**Acceptance Scenarios**:

1. **Given** ein Eintrag mit drei Bildern und einem PDF, **When** der Nutzer den Tab Extra
   öffnet, **Then** sieht er vier Karten, drei mit Vorschaubild, eine mit PDF-Symbol, jede mit
   Name, Größe und Typ.
2. **Given** ein Bild, **When** der Nutzer es antippt, **Then** öffnet die Lightbox mit
   diesem Bild; Pfeile, Wischen und die Pfeiltasten wechseln zwischen den Bildern des
   Eintrags (nicht zum PDF), Escape und der Schließen-Knopf schließen sie, Zoom und
   Verschieben gehen mit Tippen, Zwicken und dem Mausrad.
3. **Given** ein Anhang ohne Vorschau (PDF, Text, anderes), **When** der Nutzer ihn antippt,
   **Then** öffnet holzi nichts in der App, sondern bietet Speichern unter an; die Karte hat
   dafür auch einen eigenen Knopf.
4. **Given** der Tab Extra im Bearbeiten, **When** der Nutzer einen Anhang umbenennt oder
   entfernt, **Then** wirkt das wie in 034 (Umbenennen und Entfernen gelten sofort, wie
   dort festgelegt), und die Karte zeigt es.
5. **Given** ein Bild, das sich nicht darstellen lässt (beschädigt), **When** die Karte oder
   Lightbox es laden will, **Then** zeigt sie das Dateityp-Symbol und eine Meldung, statt
   leer zu bleiben.
6. **Given** viele Bilder (30) an einem Eintrag, **When** der Nutzer den Tab Extra öffnet,
   **Then** laden nur die Vorschaubilder im sichtbaren Bereich nach und nicht alle Bilder
   in voller Größe.

---

### User Story 7 - Werte eines Eintrags in anderen Einträgen wiederverwenden (Priority: P2)

Wer dieselben Zugangsdaten für mehrere Dienste braucht (ein gemeinsames Konto, ein Passkey
für mehrere Adressen), will sie nicht mehrfach pflegen. Wie in KeePass kann ein Wert eines
Eintrags einen **Verweis** auf den Wert eines anderen Eintrags enthalten: einen Platzhalter
wie `{$<Eintrag>:username}`, `{$<Eintrag>:password}` oder `{$<Eintrag>:extra:<Schlüssel>}`.
Beim Anzeigen, Kopieren und Benutzen setzt holzi den Wert der Quelle ein; ändert man ihn in
der Quelle, stimmt er in allen Zielen. Der Kopier-Dialog (Story 3) setzt solche Platzhalter,
und der Editor bietet das Einfügen eines Verweises an. Ein Passkey ist kein Text und wird
stattdessen per Verbindung zur Quelle verwiesen.

**Why this priority**: Der Kopier-Dialog bietet es an, und es ist das eine einheitliche System
für alle Verweise zwischen Einträgen. Ohne Verweise bleibt alles andere benutzbar.

**Independent Test**: Eine KeePass-Datei mit einem Verweis auf ein Passwort importieren, einen
Eintrag mit Benutzername, Passwort, einem eigenen Feld „PIN“ und einem Passkey kopieren und dabei das Passwort als Verweis wählen, das Passwort in der Quelle
ändern, die Kopie ansehen, im Feld eines dritten Eintrags einen Verweis auf „PIN“ einfügen, das
Passwort in der Kopie überschreiben, die Quelle löschen.

**Acceptance Scenarios**:

1. **Given** ein Eintrag „Konto“ und eine Kopie mit „Passwort als Verweis“ (ihr Passwort ist
   `{$<Konto>:password}`), **When** der Nutzer das Passwort in „Konto“ ändert, **Then** zeigt,
   kopiert und benutzt die Kopie das neue Passwort; das Feld zeigt den Verweis als Marke „Passwort
   von Konto“, und ein Tipp darauf öffnet die Quelle.
2. **Given** ein Wert mit Text und Verweis, etwa `admin-{$<Konto>:extra:PIN}`, **When** er
   angezeigt oder kopiert wird, **Then** steht dort „admin-“ und die PIN der Quelle.
3. **Given** ein Feld im Bearbeiten, **When** der Nutzer „Verweis einfügen“ wählt, einen Eintrag
   sucht und Benutzername, Passwort oder eines seiner eigenen Felder wählt, **Then** steht der
   Verweis an der Textstelle des Cursors; tippt der Nutzer einen Platzhalter von Hand, erkennt
   holzi ihn ebenso.
4. **Given** ein Verweisfeld, **When** der Nutzer die Marke löscht oder das Feld überschreibt,
   **Then** wird nur dieser Teil zum eigenen Text; andere Verweise im Eintrag bleiben.
5. **Given** eine Quelle, deren Wert selbst einen Verweis enthält, **When** das Ziel gelesen
   wird, **Then** wird die Kette bis zur eigentlichen Quelle aufgelöst (bis zu 12 Stufen, wie
   in KeePass); ein Verweis, der auf sich zurückführt (A verweist auf B, B auf A), wird beim
   Speichern abgelehnt, und kommt er dennoch an (Sync) oder ist die Kette länger als 12
   Stufen, zeigt das Feld „Verweiskreis oder zu tief“ statt eines Werts (KeePass liefert dort
   leeren Text; holzi nie).
6. **Given** eine Kopie mit „Passkeys per Verweis übernehmen“, **When** ein Aufrufer über die
   Kopie bestätigt, **Then** signiert der Dienst mit dem Schlüssel der Quelle, der Zähler
   der Quelle bleibt 0, und die Kopie hat keinen eigenen Schlüssel.
7. **Given** eine Quelle, auf die zwei Einträge verweisen, **When** der Nutzer sie endgültig
   löscht, **Then** warnt holzi mit der Zahl der Ziele und bietet an, die Verweise in eigene
   Werte umzuwandeln (der Platzhalter wird durch den heutigen Wert ersetzt); Verweise von
   Passkeys fallen dabei weg, was die Warnung sagt. Abbrechen lässt alles unverändert.
8. **Given** ein Aufrufer, dessen Freigabe das Ziel, aber nicht die Quelle deckt, **When** er
   ein Feld mit Verweis liest, **Then** erhält er für dieses Feld keinen Wert, nicht den der
   Quelle und keinen Hinweis, dass ein Verweis besteht; in Listen erscheinen Felder mit Verweis
   für ihn leer.
9. **Given** eine KeePass-Datei, in der ein Eintrag mit `{REF:P@I:<UUID>}` das Passwort eines
   anderen Eintrags nennt, **When** der Nutzer sie importiert, **Then** trägt der importierte
   Eintrag den Verweis `{$<neue Kennung>:password}` auf den importierten Eintrag; ein Verweis
   auf ein anderes Feld oder ohne eindeutigen Treffer bleibt Text und steht im Importbericht.
10. **Given** ein Verweis in einem alten Stand des Verlaufs, **When** der Nutzer ihn
    wiederherstellt, **Then** gilt der Platzhalter wieder; ist die Quelle weg, sagt holzi es und
    stellt den Rest her.

---

### Edge Cases

- **Wischen im Bearbeiten.** Wer in einem Textfeld Text markiert oder den Cursor mit dem
  Finger zieht, wechselt dabei nie den Tab; ein Wischen, das auf einem Eingabefeld, einem
  Code oder einem waagerecht scrollbaren Bereich beginnt, gehört diesem Element.
- **Ein Eintrag wird auf einem anderen Gerät gelöscht, während er in der Ablage liegt oder
  im Tab geöffnet ist.** Einfügen überspringt ihn und nennt die Zahl; der offene Tab sagt es
  wie in 034.
- **Einfügen in den Papierkorb, in die Suche oder in eine Tag-Ansicht.** „Einfügen“ ist dort
  nicht angeboten, weil es keinen Zielordner gibt; die Ablage bleibt gefüllt.
- **Kopieren eines Eintrags mit Passkeys.** Ein Passkey wird nie als Wert kopiert (eine
  Credential-ID gibt es nur einmal, eine Kopie wäre ein geklonter Passkey). Ohne die Checkbox
  „Passkeys per Verweis“ bekommt die Kopie keine Passkeys, und der Dialog sagt das vorher.
- **Kopieren in einen Ordner mit gleichnamigem Eintrag.** Namen müssen nicht eindeutig sein;
  die Kopie trägt den Titel aus dem Dialog.
- **Die Quelle eines Verweises wird auf einem anderen Gerät gelöscht oder ist (noch) nicht
  angekommen.** Das Feld zeigt „Quelle nicht verfügbar“, Kopieren und Benutzen melden das,
  statt den Platzhalter als Text zu verwenden; nichts stürzt ab, und nichts wird still
  geändert.
- **Ein Wert enthält Text, der wie ein Verweis aussieht** (etwa ein Passwort mit `{$`). Nur ein
  Platzhalter in genau der Form `{$<Eintrag>:…}` mit einer Kennung in UUID-Form gilt als
  Verweis; alles andere bleibt Text. Gibt es den Eintrag zu einer solchen Kennung nicht, ist es
  trotzdem ein Verweis, und das Feld zeigt „Quelle nicht verfügbar“ (siehe oben). Ein Zeichen, das den Platzhalter bricht, wird im Schlüssel mit `\`
  geschützt, und der Editor setzt den Platzhalter selbst richtig zusammen.
- **Die Quelle eines Verweises liegt im Papierkorb.** Der Nutzer sieht den Wert weiter, ein
  Aufrufer von außen nicht (wie bei jedem Eintrag im Papierkorb).
- **Sehr viele ausgewählte Einträge** (mehrere hundert). Aktionen laufen als eine
  Änderung, bleiben benutzbar und melden das Ergebnis einmal.
- **Ausschneiden und Fenster schließen.** Die Ablage hält nur Kennungen und überlebt das
  Schließen eines Fensters, solange ein anderes Fenster des Passwortmanagers offen ist; mit
  dem letzten Fenster (oder dem Vault-Wechsel) ist sie weg.
- **Ein Passkey-Aufrufer ohne passende Freigabe** erfährt nicht, ob ein Passkey existiert
  (wie FR-029 in 034).
- **Die Uhr oder der Zähler eines Geräts geht falsch.** Der Zähler eines synchronisierten
  Passkeys bleibt 0 und hängt nicht an der Uhr; die Oberfläche zeigt den Nutzungszeitpunkt
  separat.
- **Ein Passkey ohne Eintrag** (nur durch Fremddaten oder einen Import ohne Eintrag möglich;
  holzi legt nie einen an). Er zählt für Aufrufer von außen als nicht vorhanden und wird in
  der Oberfläche nicht angezeigt; holzi löscht ihn nicht still.
- **Ein Passkey, dessen Gegenstelle der Nutzer nicht mehr kennt.** Der Tab Extra zeigt
  trotzdem Kennung und Benutzer; löschen geht immer.
- **Ein Bild ist sehr groß** (bis zur Anhangsgrenze von 034). Die Vorschau wird verkleinert
  erzeugt; die Lightbox lädt das Bild erst beim Öffnen.
- **Reduzierte Bewegung.** Weder Tabwechsel noch Lightbox noch die Auswahlleiste benutzen
  Gleit- oder Zoomanimationen, wenn der Nutzer es so eingestellt hat.

## Requirements _(mandatory)_

### Functional Requirements

**Eintrag in Tabs**

- **FR-001**: Ein Eintrag MUSS in drei Tabs erscheinen: **Details**, **Extra** und **Verlauf**.
  Details enthalten Titel, Benutzername, Passwort, Adresse, Tags, Notiz, Ablaufdatum, Symbol,
  Farbe und TOTP samt Live-Code und Restzeit; Extra enthält eigene Felder, Anhänge, Passkeys
  und Autofill-Aliase. Das ersetzt die eine lange Seite aus 034.
- **FR-002**: Zwischen den Tabs MUSS der Nutzer durch Tippen auf den Tab und durch eine
  waagerechte Wischgeste wechseln können; die Wischgeste MUSS dem Finger folgen, vom ersten
  zum letzten Tab nicht weiterführen und mit der Einstellung „Bewegung reduzieren“ ohne
  Gleitbewegung wechseln. Mit der Tastatur MUSS der Wechsel mit den Pfeiltasten auf der
  Tab-Leiste gehen.
  _Erweitert 2026-10-05:_ Am Desktop wechseln auch Ziehen mit der Maus und eine waagerechte
  Wischgeste auf dem Touchpad den Tab.
- **FR-003**: Eine Wischgeste, die auf einem Eingabefeld, einem markierbaren Text, einem
  Code oder einem waagerecht scrollbaren Bereich beginnt, DARF den Tab nicht wechseln.
- **FR-004**: Beim Bearbeiten MUSS jede Eingabe beim Wechsel zwischen Details und Extra
  erhalten bleiben; der Entwurf, der Dialog bei ungespeicherten Änderungen und die
  Konfliktbehandlung aus 034 gelten unverändert. Ein Speichern, das an einem fehlenden oder
  ungültigen Pflichtfeld scheitert, MUSS zu dem Tab wechseln, der dieses Feld enthält, und das
  Feld markieren.
- **FR-005**: In der Ansicht MÜSSEN leere Felder ausgeblendet sein, beim Bearbeiten alle
  Felder erscheinen. Ein Tab ohne sichtbaren Inhalt (etwa Extra bei einem Eintrag ohne alles
  daraus) MUSS einen Hinweis zeigen, was er hier aufnehmen kann.
  _Präzisiert 2026-10-06 (PR #287):_ Das gilt auch für das Passwort: ohne Passwort zeigt die
  Ansicht keine Zeile „Passwort“; Details ohne jedes Feld zeigen den Hinweis.
- **FR-006**: Der gewählte Tab MUSS zum Ort des Fenster-Tabs gehören (Vor, Zurück, wiederherge-
  stellte Sitzung); der Ort `entry/:id/history` aus 034 MUSS den Tab Verlauf öffnen. Ein Ort
  DARF nie ein Geheimnis enthalten (die Regel aus 034 und 022 für Orte bleibt).

**Verlauf als Tab**

- **FR-007**: Der Tab Verlauf MUSS die Stände des Eintrags als Zeitleiste zeigen, neuester
  oben und vorgewählt: je Stand ein Punkt, bei ausreichender Breite die relative Zeit
  (mit dem absoluten Zeitpunkt als Hinweis) und die Namen der geänderten Felder. Der
  gewählte Stand MUSS rechts (bei schmalem Fenster darunter) schreibgeschützt mit allen im
  Stand gespeicherten Feldern stehen; Geheimnisse sind maskiert und nur auf Wunsch sichtbar
  und beim Wechsel von Stand oder Tab wieder verborgen.
- **FR-008**: „Diesen Stand wiederherstellen“ MUSS erhalten bleiben und sich wie in 034
  verhalten (nach Bestätigung, legt einen neuen obersten Stand an, kürzt nichts). Während
  des Bearbeitens DARF der Tab Verlauf nicht wählbar sein.
- **FR-009**: Die eigene Seite des Verlaufs aus 034 MUSS entfallen; was sie konnte, kann der
  Tab. Kein Stand, kein Wiederherstellen und keine Aufdeckfunktion DARF dabei verloren gehen.

**Liste und Navigation**

- **FR-010**: Die Liste MUSS über sich die **Brotkrumen** des aktuellen Ortes zeigen: Wurzel
  „Alle Einträge“, die Vorfahren und den aktuellen Ordner; jeder Teil außer dem letzten ist
  ein Ziel. Im Papierkorb, in einer Tag-Ansicht und in der Suche MUSS an der Stelle der Name
  der Ansicht stehen. Zu lange Pfade MÜSSEN sich kürzen (die mittleren Teile in einem
  Überlaufmenü), statt die Zeile zu sprengen.
- **FR-010a**: Solange ein Eintrag offen ist (Ansicht, Bearbeiten, Neu), MUSS die obere Leiste
  nur den Knopf der Seitenleiste und das Menü zeigen; Suche, Tag-Filter und „+“ entfallen. Die
  Ansicht eines Eintrags MUSS über sich die Brotkrumen wie die Liste zeigen: Wurzel „Alle
  Einträge“ und die Ordner bis zu seinem Ordner, jeder Teil ein Ziel, als letzter Teil der
  Titel des Eintrags mit seinem Icon. Ein Eintrag im Papierkorb zeigt dort den Papierkorb. Der
  Editor behält seinen Kopf mit Abbrechen und Speichern, damit kein Link an der Nachfrage zu
  ungespeicherten Änderungen vorbeiführt.
- **FR-011**: Mehrfachauswahl MUSS wie in 034 per Umschalt-Klick (Bereich), Strg/Cmd-Klick
  (einzeln) und, sobald eine Auswahl besteht, per einfachem Klick möglich sein, auf dem
  Telefon per Langdruck auf eine Zeile (mit einer kurzen Rückmeldung am Gerät, wo es die
  gibt) und per Kontrollkästchen je Zeile. Ordner und Einträge MÜSSEN gemeinsam auswählbar
  sein. Beim Ordnerwechsel oder Öffnen eines Eintrags MUSS die Auswahl enden.
- **FR-012**: Bei bestehender Auswahl MUSS statt der Brotkrumen die **Auswahlleiste** stehen:
  Schließen, ein Kontrollkästchen „Alle auswählen“ mit drei Zuständen, die Anzahl und die
  Aktionen Bearbeiten (nur bei genau einem Eintrag), Ausschneiden, Kopieren, Tags (nur bei
  Auswahlen aus Einträgen) und Löschen. Bei schmaler Leiste MÜSSEN die Aktionen in ein
  Überlaufmenü wandern. Die Aktionen Verschieben in einen Ordner, Tag hinzufügen und
  entfernen und Löschen aus 034 MÜSSEN erhalten bleiben.
- **FR-013**: **Ausschneiden** MUSS die Auswahl in die Ablage legen und die Zeilen abblenden;
  **Kopieren** legt sie in die Ablage, ohne etwas abzublenden; **Einfügen** wirkt auf den
  geöffneten Ordner (oder den Ordner, dessen Menü es ausgelöst hat; die Wurzel zählt als
  Ordner). Nach erfolgreichem Ausschneiden-Einfügen MUSS die Ablage leer sein, nach
  Kopieren-Einfügen gefüllt bleiben; bei einem Fehler bleibt sie gefüllt. Eine Leiste ohne
  Auswahl MUSS bei gefüllter Ablage „n in der Ablage“ mit Einfügen und „Ablage leeren“ zeigen.
- **FR-014**: Ausgeschnitten und eingefügt MÜSSEN Einträge ihren Ordner wechseln und Ordner
  ihren Elternordner; ein Einfügen eines Ordners in sich selbst oder einen seiner
  Unterordner MUSS abgelehnt werden, bevor irgendetwas verschoben wird, mit einer Meldung, die
  den Grund nennt. Einträge und Ordner, die in der Zwischenzeit gelöscht wurden, MÜSSEN
  übersprungen und mit ihrer Zahl gemeldet werden; die übrigen werden eingefügt.
- **FR-015**: Beim **Einfügen einer Kopie** MUSS ein Dialog (auf schmalen Fenstern eine
  Schublade) aufgehen, bevor etwas entsteht; Abbrechen legt nichts an. Er enthält: den
  **Titel** (Vorgabe: der Originaltitel mit angehängtem „Kopie“ bei deutscher, „Copy“ bei
  englischer Oberfläche, änderbar; bei mehreren Einträgen oder einem Ordner ein gemeinsamer
  änderbarer Zusatz statt der Titel), die Checkbox **Verlauf übernehmen** (Vorgabe: aus; dann
  bekommt die Kopie ihren eigenen Anfangsstand) und die Checkboxen **Benutzername als
  Verweis**, **Passwort als Verweis** und **Passkeys per Verweis übernehmen** (Vorgabe: aus; aus
  heißt für Benutzername und Passwort Kopie als Wert, an heißt: der Wert der Kopie ist der
  Platzhalter auf den Wert der Vorlage (FR-044); für Passkeys heißt aus: keine Passkeys, siehe
  Randfälle). Alles andere (Adresse, Notiz, Ablaufdatum, Symbol, Farbe, TOTP, eigene Felder,
  Tags, Aliase, Anhänge) MUSS immer als Wert kopiert werden; die Anhänge teilen über ihre
  Prüfsumme die Binärdaten, es wird nichts doppelt gespeichert. Die Kopie ist ein neuer Eintrag
  (neue Kennung). Ordner MÜSSEN rekursiv mit Unterordnern und Einträgen kopiert werden; die
  Wahl im Dialog gilt für alle Einträge der Kopie. Ausschneiden und Kopieren steht im Papierkorb
  nicht zur Verfügung, Einfügen in ihn auch nicht.
- **FR-016**: Auf der Liste MÜSSEN folgende Kürzel gelten (Strg, auf macOS Cmd), solange der
  Fokus nicht in einem Eingabefeld, einem Dialog oder einem markierten Text steht:
  Strg+A alles Sichtbare auswählen, Esc Auswahl aufheben, Strg+X ausschneiden, Strg+C
  kopieren, Strg+V einfügen, Entf löschen (mit Bestätigung wie sonst), Eingabe öffnen,
  Strg+F Suche fokussieren, Pfeiltasten Fokus in der Liste bewegen (mit Umschalt die Auswahl
  erweitern), Strg+B Benutzername und Strg+Umschalt+C Passwort des fokussierten oder
  einzeln ausgewählten Eintrags kopieren. Die Kürzel MÜSSEN im Fenster des Passwortmanagers
  wirken und DÜRFEN die Kürzel des Window Managers (wm) nicht verdecken. Sie sind feste
  Voreinstellungen; Umbelegen ist, wie bei den wm-Kürzeln, eine spätere Spec.
- **FR-017**: Das Kopieren von Benutzername und Passwort per Menü oder Kürzel MUSS wie das
  Kopieren aus dem Eintrag die Einstellung für die Zwischenablage aus 034 (automatisches
  Leeren) einhalten; das Passwort DARF dabei nie sichtbar werden.
- **FR-018**: Ein **Kontextmenü** MUSS sich per Rechtsklick (oder Menüknopf an der Zeile, wo
  es keine rechte Maustaste gibt) öffnen für einen Eintrag (Öffnen, Benutzername kopieren,
  Passwort kopieren, Ausschneiden, Kopieren, Löschen; im Papierkorb Wiederherstellen und
  Endgültig löschen), für einen Ordner (Öffnen, Bearbeiten, Neuer Unterordner, Ausschneiden,
  Kopieren, Einfügen in diesen Ordner, Löschen; im Papierkorb Wiederherstellen und Endgültig
  löschen), für den Ordner in der Seitenleiste (dieselben) und für die leere Fläche der Liste
  (Neuer Eintrag, Neuer Ordner, Einfügen). Der Papierkorb in der Seitenleiste MUSS „Papierkorb
  leeren“ bieten. Ein Rechtsklick auf eine Zeile außerhalb der Auswahl MUSS die Auswahl auf
  diese Zeile setzen; innerhalb der Auswahl MUSS das Menü für die ganze Auswahl gelten.
- **FR-019**: Jede Aktion eines Menüs oder Kürzels MUSS auch ohne Maus und Tastatur über die
  Auswahlleiste oder die Seite des Eintrags erreichbar sein.
- **FR-020**: Ziehen MUSS wie in 034 Einträge und Ordner auf Ordner der Liste und der
  Seitenleiste und auf die Wurzel ablegen können und zusätzlich auf die Teile der Brotkrumen.
  Gehört der gezogene Eintrag zu einer Auswahl, MUSS die ganze Auswahl verschoben werden.
  Ein Ablegen, das einen Ordner in sich selbst verschieben würde, MUSS abgelehnt werden.
- **FR-021**: Die Ablage MUSS nur Kennungen und die Art (Ausschneiden oder Kopieren) halten,
  nie ein Geheimnis. Sie MUSS für alle Fenster des Passwortmanagers im Prozess gelten, nicht
  mit der Sitzung wiederhergestellt, nicht synchronisiert und mit dem letzten geschlossenen
  Fenster oder dem Wechsel der Vault verworfen werden. Ausschneiden und Kopieren ganzer
  Einträge und Ordner DARF nichts in die Zwischenablage des Betriebssystems legen und sie nicht
  verändern; diese gehört den einzelnen Werten (FR-017).
- **FR-022**: Alle Aktionen aus FR-012 bis FR-020 MÜSSEN für mehrere hundert Einträge als
  eine Änderung laufen, ein Ergebnis melden und bei einem Fehler nichts halb ausgeführt
  lassen.

**Passkeys (Stufe b)**

- **FR-023**: Der Dienst MUSS drei Funktionen für Passkeys bieten: **anlegen**, **bestätigen**
  (die Aufgabe der Gegenstelle mit dem privaten Schlüssel signieren) und **auflisten**. Sie
  gehen wie alle Zugriffe durch die Zugriffsprüfung aus 034 (FR-024) mit Aufrufer und
  Bereich; die Oberfläche des Nutzers DARF sie nicht zum Anlegen eines Passkeys anbieten
  (kein Anlegen von Hand), ruft aber Auflisten, Umbenennen und Löschen selbst auf.
- **FR-024**: Beim **Anlegen** MUSS der Dienst ein neues Schlüsselpaar erzeugen, eine
  zufällige, eindeutige Credential-ID vergeben, den Passkey mit Zähler 0 speichern und dem
  Aufrufer die Angaben zurückgeben, die die Gegenstelle zum Registrieren braucht
  (Credential-ID, öffentlicher Schlüssel, Algorithmus, die Beglaubigung über die Aufgabe).
  Kennung und Name der Gegenstelle, Benutzerkennung, Benutzername und die Aufgabe der
  Gegenstelle MÜSSEN Pflicht sein; fehlt eine, wird abgelehnt. Eine Liste ausgeschlossener
  Credential-IDs MUSS dazu führen, dass kein neuer Passkey entsteht, wenn einer davon für
  dieselbe Gegenstelle an einem Eintrag im Bereich des Aufrufers existiert (Passkeys außerhalb
  des Bereichs verraten sich so nicht, FR-030). Der Aufruf MUSS den **Eintrag** nennen, an dem der Passkey
  hängt; ohne Eintrag wird abgelehnt. `userHandle` darf nach Base64URL-Dekodierung höchstens
  64 Byte lang sein; andernfalls wird der Aufruf abgelehnt. Einen neuen Eintrag legt der Aufrufer vorher mit den
  gewöhnlichen Funktionen aus 034 an.
- **FR-025**: Beim **Bestätigen** MUSS der Dienst den passenden Passkey finden (über die Liste
  zugelassener Credential-IDs und die Kennung der Gegenstelle, sonst über einen auffindbaren
  Passkey dieser Gegenstelle), `signCount` als 0 setzen, „zuletzt benutzt“ setzen und die
  Aufgabe samt Zähler signieren. Ohne einen erfolgreichen, zeremoniegebundenen Presence-Nachweis
  werden UP und UV nicht gesetzt. Die **Herkunft** der Anfrage MUSS zur Kennung der Gegenstelle
  des Passkeys passen (gleiche Domäne oder eine Oberdomäne der Herkunft); andernfalls MUSS
  abgelehnt und nichts signiert werden. Das schließt die feste Annahme von haex-vault aus,
  die Herkunft sei immer `https://<Kennung>`.
- **FR-026**: Der **Zähler** eines synchronisierten Passkeys MUSS bei Anlegen und Bestätigen 0
  bleiben. Der Dienst führt keine unzuverlässige globale Nicht-Null-Reihenfolge über offline
  bestätigende Geräte ein.
- **FR-027**: Beim **Auflisten** MUSS der Dienst nur Kopfdaten liefern (Kennung, Gegenstelle
  samt Name, Benutzerkennung und -name, Spitzname, Algorithmus, auffindbar ja oder nein,
  Anlege- und Zuletzt-benutzt-Zeit, Kennung des Eintrags), filterbar nach
  Gegenstelle, Eintrag und „nur auffindbare“; nie den privaten Schlüssel.
- **FR-028**: Der private Schlüssel MUSS im Dienst bleiben: weder eine Antwort, ein Fehler,
  ein Protokoll noch ein Verlauf noch der Kontext des eingebauten Agenten DARF ihn enthalten.
- **FR-029**: Der Nutzer MUSS die Passkeys eines Eintrags im Tab Extra sehen und verwalten
  (Spitzname, Gegenstellenname oder -kennung, Benutzer, angelegt, zuletzt benutzt,
  „auffindbar“-Marke; Umbenennen und Löschen mit Bestätigung). Eine eigene Ansicht für alle
  Passkeys und das Verschieben eines Passkeys zu einem anderen Eintrag gibt es nicht; ein
  Passkey ist immer Teil eines gewöhnlichen Eintrags.
- **FR-030**: Ein Passkey MUSS durch denselben Bereich gedeckt sein wie sein Eintrag: Er gilt
  für einen Aufrufer als im Bereich, wenn der Eintrag es ist (er trägt ein Tag aus dem
  Bereich, oder die Freigabe gilt für „alle“). Wer einen Eintrag sehen darf, sieht auch seine
  Passkeys (nach Art der Freigabe: Kopfdaten beim Lesen, Benutzen beim Bestätigen). Ein Passkey
  außerhalb des Bereichs MUSS wie ein nicht vorhandener behandelt werden (034 FR-029).
- **FR-031**: **Anlegen** MUSS die Art „Lesen und Schreiben“ verlangen und dass der Eintrag, an
  dem der Passkey entsteht, im Bereich des Aufrufers liegt (034 FR-028). **Bestätigen** MUSS
  mindestens „Lesen“ im Bereich des Eintrags verlangen; **Auflisten** auch.
- **FR-032**: Der eingebaute Agent im Chat MUSS von den Passkeys höchstens erfahren, dass ein
  Eintrag welche hat (der Hinweis in den Kopfdaten aus 034); er DARF keine der drei Funktionen
  aufrufen. Externe Agenten über MCP sind Aufrufer wie Erweiterungen: Mit einer Freigabe für
  ein Tag des Eintrags DÜRFEN sie auflisten, anlegen und bestätigen, soweit die Art der Freigabe
  es deckt (FR-031).
- **FR-033**: **Umbenennen und Löschen** eines Passkeys MUSS dem Nutzer
  vorbehalten bleiben (wie 034 für das Löschen eines Passkeys); andere Aufrufer MÜSSEN dafür
  abgelehnt werden.
- **FR-034**: Passkeys mit einem anderen Algorithmus als dem, den holzi beim Anlegen erzeugt
  (Import, Sync), MÜSSEN anzeigbar, auflistbar und, wenn holzi den Algorithmus kennt,
  bestätigbar sein; sonst lehnt Bestätigen mit einer klaren Meldung ab.
- **FR-035**: Ein Passkey an einem Eintrag im Papierkorb MUSS für andere Aufrufer als den
  Nutzer wie nicht vorhanden sein (Auflisten, Bestätigen), wie der Eintrag selbst (034
  FR-015), und kommt mit dem Eintrag zurück. Das endgültige Löschen des Eintrags löscht auch
  seine Passkeys (034 FR-015); der Dialog nennt in diesem Fall die Zahl der Passkeys. Das
  Löschen eines einzelnen Passkeys am Eintrag ist endgültig und braucht die Bestätigung aus
  Story 5.
- **FR-036**: Die Funktionen MÜSSEN so gebaut sein, dass die External Bridge später als
  weiterer Aufrufer ohne Änderung von Datenmodell oder Freigabe-Semantik andocken kann
  (034 FR-032). Eine Browser-Erweiterung, Autofill und das Annehmen von Anfragen aus dem
  Browser sind nicht Teil dieser Spec.

**Anhänge als Karten**

- **FR-037**: Die Anhänge eines Eintrags MÜSSEN im Tab Extra als Karten in einem Raster
  erscheinen (drei, zwei oder eine Spalte je nach Breite). Jede Karte MUSS zeigen: bei Bildern
  ein Vorschaubild, sonst ein Symbol nach Typ (PDF, Text, anderes), den Namen, die Größe und
  den Typ; und Knöpfe für Speichern unter, Umbenennen und Entfernen. Die Größe MUSS bei jedem
  Anhang stehen, auch bei bereits gespeicherten.
- **FR-038**: Ein Tipp auf ein Bild MUSS eine **Lightbox** öffnen: das Bild groß, zoombar und
  verschiebbar (Tippen, Zwicken, Mausrad), mit Vor und Zurück (Pfeile, Wischen,
  Pfeiltasten) durch alle Bilder des Eintrags in der Reihenfolge der Karten, mit Schließen
  (Knopf und Escape), dem Namen und der Position („2 von 5“) und Speichern unter. Der Fokus
  MUSS in der Lightbox bleiben und beim Schließen auf die Karte zurückkehren.
- **FR-039**: Ein Tipp auf einen Anhang ohne Vorschau (PDF, Text, anderes) MUSS Speichern
  unter anbieten, nichts in der App öffnen. Eine PDF-Vorschau ist nicht Teil dieser Spec.
- **FR-040**: Das Hinzufügen (Dateidialog und Ablegen), die Grenze von 25 MiB, die Prüfsumme,
  die Verwaltung verwaister Binärdaten und das Umbenennen und Entfernen MÜSSEN sich wie in
  034 verhalten; die Karten ändern nur die Darstellung.
- **FR-041**: Vorschaubilder MÜSSEN verkleinert erzeugt und nur für den sichtbaren Bereich
  geladen werden; die Lightbox lädt das Bild in voller Größe erst beim Öffnen. Lässt sich ein
  Bild nicht darstellen, MUSS die Karte das Typ-Symbol und die Lightbox eine Meldung zeigen.

**Verweise zwischen Einträgen**

- **FR-044**: Der Text eines Werts MUSS **Verweise** auf Werte anderer Einträge als Platzhalter
  enthalten können, in der Form `{$<Kennung des Eintrags>:username}`,
  `{$<Kennung>:password}` und `{$<Kennung>:extra:<Schlüssel des eigenen Felds>}`, auch
  mehrere im selben Wert und zusammen mit gewöhnlichem Text. Die Felder, in denen Verweise
  gelten, sind Benutzername, Passwort, Adresse, Notiz und die Werte eigener Felder; die
  verweisbaren Werte sind zunächst Benutzername, Passwort und eigene Felder. Weitere
  Wertarten DÜRFEN später dazukommen, ohne dass sich die Form oder die Semantik ändert.
- **FR-045**: Beim Anzeigen, beim Kopieren in die Zwischenablage und beim Benutzen MUSS holzi
  jeden Platzhalter durch den Wert der Quelle ersetzen; gespeichert bleibt der Platzhalter,
  nie der aufgelöste Wert. Änderungen der Quelle MÜSSEN sofort in allen Zielen wirken. Der
  Editor MUSS einen Verweis als Marke mit dem Namen der Quelle und der Art des Werts zeigen
  (ein Tipp öffnet die Quelle), das Einfügen eines Verweises anbieten (Eintrag suchen, Wert
  wählen) und von Hand getippte Platzhalter erkennen. Ein Platzhalter, der nicht aufgelöst
  werden kann (Quelle fehlt, Kreis oder zu tief), MUSS als solcher gekennzeichnet sein und DARF beim
  Kopieren oder Benutzen nicht als Text verwendet werden; es gibt eine Meldung.
- **FR-046**: Aufgelöst wird bis zur eigentlichen Quelle: Enthält ein verwiesener Wert selbst
  Verweise, werden sie ebenfalls aufgelöst, bis zu **12 Stufen** (wie in KeePass). Ein Kreis
  MUSS beim Speichern abgelehnt werden. Kommt dennoch ein Kreis an (Sync) oder ist eine Kette
  länger als 12 Stufen, MUSS das Feld „Verweiskreis oder zu tief“ zeigen; es DARF nie leerer
  Text oder der Platzhalter als Text herauskommen (KeePass liefert ab der zwölften Stufe leeren
  Text, ohne Kreise zu erkennen). Ein **Passkey** ist kein Text: Er wird beim
  Kopieren (FR-015) per Verbindung zur Quelle verwiesen, erscheint im Ziel mit seinen
  Kopfdaten, Bestätigen über das Ziel signiert mit dem Schlüssel der Quelle, und der Zähler
  gehört der Quelle. Ein Passkey wird nie als Wert kopiert.
- **FR-047**: Ein Verweis DARF kein Zugriffsrecht geben: Ein Aufrufer außer dem Nutzer MUSS
  einen Wert über einen Verweis nur erhalten, wenn seine Freigabe auch die Quelle deckt; sonst
  ist das ganze Feld für ihn nicht vorhanden, ohne dass erkennbar ist, dass ein Verweis
  besteht (034 FR-029). Listen MÜSSEN Verweise nie auflösen (034 FR-026): Ein Feld mit Verweis
  erscheint dort für Aufrufer von außen leer und in der Oberfläche als Marke; sonst könnte ein
  Passwort-Verweis im Benutzernamen ein Geheimnis in die Liste tragen. Der eingebaute Agent
  erhält nie ein aufgelöstes Geheimnis (034 FR-027).
- **FR-048**: Das endgültige Löschen einer Quelle MUSS vorher mit der Zahl der Ziele warnen und
  anbieten, die Verweise in eigene Werte umzuwandeln (der Platzhalter wird durch den heutigen
  Wert ersetzt); Verweise von Passkeys fallen dabei weg, und die Warnung sagt es. Der
  Papierkorb löst keine Verweise auf (für den Nutzer bleibt der Wert sichtbar).
- **FR-049**: Der Verlauf MUSS Platzhalter unverändert speichern, nie den aufgelösten Wert (so
  gelangt kein Geheimnis der Quelle in den Verlauf des Ziels); Wiederherstellen stellt den
  Platzhalter wieder her.
- **FR-050**: Der KeePass-Import (034 FR-023) MUSS KeePass-Verweise der Form
  `{REF:<Feld>@<Suche>:<Text>}` in Verweise dieses Systems umwandeln, wenn das gewünschte Feld
  Benutzername oder Passwort ist und die Quelle sich in der importierten Datenbank eindeutig
  findet (über die KeePass-Kennung oder über genau einen Treffer der Suche); die Kennung der
  Quelle ist dann die des importierten Eintrags. Alle anderen Verweise (anderes Feld, kein oder
  mehrere Treffer) bleiben Text, und der Importbericht nennt ihre Zahl.

**Allgemein**

- **FR-042**: Alles Neue MUSS ohne waagerechtes Scrollen bei 360 px Breite benutzbar sein
  (034 SC-011), per Tastatur erreichbar sein, die Farben der Darstellung (035) benutzen und
  in Deutsch und Englisch vorliegen.
- **FR-043**: Die vorhandenen Abläufe des Passwortmanagers (Anlegen, Suchen, TOTP, Papierkorb,
  Zwei-Geräte-Sync, schmales Fenster, Sitzung ohne Werte, Import) MÜSSEN weiter funktionieren;
  ihre bestehenden Szenarien laufen grün.

### Key Entities

- **Eintrag-Ansicht**: die Darstellung eines Eintrags mit drei Tabs und dem gewählten Tab;
  gehört zum Ort des Fenster-Tabs, enthält nie ein Geheimnis im Ort.
- **Stand (Snapshot)**: unverändert aus 034; die Zeitleiste ist nur eine neue Darstellung.
- **Ablage**: die Kennungen ausgeschnittener oder kopierter Einträge und Ordner mit der Art;
  gilt für alle Fenster des Passwortmanagers im Prozess, nicht gespeichert, nicht
  synchronisiert.
- **Auswahl**: die markierten Einträge und Ordner einer Liste; endet beim Ordnerwechsel.
- **Passkey**: unverändert aus 034 (Tabelle der Passkeys), jetzt auch mit Funktionen zum
  Anlegen, Bestätigen und Auflisten; gehört immer zu einem Eintrag und folgt dessen Tags;
  `signCount` bleibt bei synchronisierten Passkeys 0.
- **Verweis**: ein Platzhalter im Text eines Werts (`{$<Eintrag>:username}`, `…:password`,
  `…:extra:<Schlüssel>`), der beim Lesen durch den Wert des anderen Eintrags ersetzt wird; er
  wird nie aufgelöst gespeichert und folgt der Quelle sofort. Für Passkeys eine Verbindung zur
  Quelle.
- **Anhangskarte**: die Darstellung eines Anhangs (Vorschau oder Symbol, Name, Größe, Typ).

## Success Criteria _(mandatory)_

### Measurable Outcomes

- **SC-001**: Ein Nutzer findet in einem Eintrag mit drei Ständen den Verlauf und stellt den
  ältesten Stand wieder her, ohne Anleitung, in unter 30 Sekunden.
- **SC-002**: Ein Nutzer verschiebt 20 Einträge aus einem Ordner in einen anderen Ordner, in
  der Liste mit Auswahl, Ausschneiden und Einfügen, in unter 20 Sekunden, auf dem Rechner
  und auf einem Telefon.
- **SC-003**: Bei 360 px Breite gibt es in keinem neuen Teil (Tabs, Verlauf, Brotkrumen,
  Auswahlleiste, Passkeys im Tab Extra, Anhangskarten, Lightbox) waagerechtes Scrollen der Seite,
  und jede Aktion eines Kontextmenüs ist ohne rechte Maustaste erreichbar.
- **SC-004**: In 100 Versuchen, einen Ordner in sich oder einen Unterordner einzufügen oder
  zu ziehen, verschiebt holzi nie etwas; in 100 Versuchen mit gelöschten Zielen geht kein
  Eintrag verloren.
- **SC-005**: Eine Signatur, die der Dienst für einen angelegten Passkey erzeugt, prüft mit dem
  öffentlichen Schlüssel erfolgreich, für jeden Algorithmus, den holzi anlegt; `signCount`
  bleibt bei jeder Bestätigung und in jedem Sync-Szenario 0.
- **SC-006**: In keiner Antwort, keinem Fehler, keiner Liste und keinem Protokoll des Dienstes
  steht ein privater Passkey-Schlüssel; ein Aufrufer mit einem Tag sieht, ändert und
  bestätigt in 100 Versuchen keinen Passkey an einem Eintrag außerhalb seines Bereichs und
  erfährt nicht, dass es ihn gibt.
- **SC-007**: Eine Anfrage mit falscher Herkunft wird in allen Versuchen abgelehnt, ohne zu
  signieren oder Nutzungszustand zu ändern.
- **SC-008**: Die Lightbox öffnet ein 5-MiB-Bild in unter einer Sekunde; an einem Eintrag mit
  30 Bildern lädt der Tab Extra nur die sichtbaren Vorschaubilder.
- **SC-009**: Ändert der Nutzer das Passwort einer Quelle, zeigt jedes Ziel mit Verweis es
  beim nächsten Öffnen ohne weiteres Zutun; in 100 Versuchen erhält ein Aufrufer ohne Bereich
  für die Quelle über einen Verweis nie einen Wert und nie einen Hinweis auf den Verweis, und
  keine Liste löst je einen Verweis auf.
- **SC-010**: Die vorhandenen Szenarien des Passwortmanagers (034 SC-012) laufen unverändert
  grün; neue Szenarien decken Tabs mit Wischgeste, Verlauf, Ablage mit Ordnerkopie,
  Kontextmenü, Passkey-Anlegen und -Bestätigen und die Lightbox ab.

## Assumptions

- Die **Tab-Wischgeste** bekommt eine eigene Bibliothek für Gesten und Übergänge; welche, wird
  im Plan entschieden. haex-vault hat dort nur Tippen, die Wischgeste ist neu.
- Der **Kopier-Dialog** erscheint beim Einfügen, denn dann entsteht die Kopie; die Auswahl beim
  Kopieren selbst bleibt ohne Dialog.
- **Verweise** sind zunächst auf Benutzername, Passwort und eigene Felder beschränkt, wie die
  Beispiele zeigen; weitere Wertarten lassen sich später ohne Änderung der Form zulassen. Der
  Aufbau gleicht dem von KeePass (Auflösen beim Lesen, bis zu 12 Stufen); anders als dort
  erkennt holzi Kreise beim Speichern und liefert nie leeren Text.
- Die **Kürzel** sind feste Voreinstellungen; das Umbelegen kommt mit dem Umbelegen der
  wm-Kürzel in einer Folge-Spec (Befehle und Tastenkürzel; Spec 020 nimmt es ausdrücklich aus). Die Auswahl der Zeile mit Pfeiltasten gilt nur im Passwortmanager.
- Ein **Passkey ohne Eintrag** kann in der Tabelle von 034 (und in haex-vault) vorkommen; holzi
  legt nie einen an und behandelt einen vorgefundenen als nicht vorhanden (siehe Randfälle).
- Wer **Passkeys anlegen und bestätigen** darf, sind die Aufrufer mit Freigabe für ein Tag des
  Eintrags (Erweiterungen, externe Agenten über MCP, später die External Bridge); der
  eingebaute Agent darf gar nichts. Die Freigaben selbst vergeben die Specs 017–019 und 021.
- Ein Passkey an einem **Eintrag im Papierkorb** bleibt an ihm hängen und wird mit ihm
  wiederhergestellt (FR-035).
- Eine **Vorschau** gibt es nur für Bilder (PNG, JPEG, GIF, WebP wie in 034); SVG, PDF und
  Text bekommen nur ein Symbol.
- Die Tastaturkürzel und Menüs gelten für die **Liste**; im Editor bleiben Eingabe, Escape und
  die Standardkürzel der Eingabefelder wie bisher.
- Der `signCount` eines synchronisierten Passkeys bleibt beim Sync **0**; eine globale
  Nicht-Null-Reihenfolge ist über offline bestätigende Geräte nicht garantiert.

## Nicht im Umfang

- Die **External Bridge**: Browser-Erweiterung, Autofill, Annehmen von Passkey-Anfragen aus
  dem Browser, Anbindung von `apps/haex-pass-browser` in haextension.
- Das **Anlegen eines Passkeys von Hand** in der Oberfläche (er entsteht über die Funktionen
  des Dienstes oder durch Import und Sync).
- **Export** (KeePass, Bitwarden, CSV), **PDF-Vorschau**, eine **Inaktivitätssperre** des
  Passwortmanagers und weitere Folgearbeiten aus `plans/README.md` („Passwortmanager:
  Folgearbeiten“).
- **Umbelegbare Kürzel** und Kürzel außerhalb der Liste.
- **Freigaben verwalten** (Specs 017–019, 021) und das **Teilen** von Einträgen mit anderen.
- Eine **Änderung der Tabellen** aus 034. Neu hinzukommen dürfen nur Tabellen, die die
  Verbindung eines Passkeys zur Quelle (FR-046) braucht; der synchronisierte Zähler benötigt
  keine zusätzliche Tabelle.
