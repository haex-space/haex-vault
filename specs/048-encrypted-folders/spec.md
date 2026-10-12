# Feature Specification: Verschlüsselte Ordner in Speichern

**Feature Branch**: `048-encrypted-folders`

**Created**: 2026-10-10

**Status**: Draft

**Input**: User description: "Verschlüsselte Ordner in S3-Speichern (Spec 038) für den Dateibrowser
(Spec 044). Nicht der ganze Speicher, sondern einzelne Ordner werden transparent verschlüsselt; jedes
Lesen und Schreiben von Dateien und Ordnern darin läuft durch eine Verschlüsselungsschicht (Modell
Cryptomator/gocryptfs, je Datei, kein Container wie VeraCrypt/LVM). Vorbild ist haex-vault
`file_sync/crypto`. Schlüsselhierarchie Vault → Ordner → Datei, damit ein Ordner später geteilt werden
kann, ohne den Vault-Schlüssel preiszugeben; alle eigenen Geräte lesen ohne Passwort. Die Zuordnung
Pfad → Objekt liegt im Bucket. Erweiterungen und Agents brauchen für einen verschlüsselten Ordner immer
eine eigene, ausdrückliche Freigabe. Teilen mit anderen Nutzern ist nicht im Umfang, das Format muss es
aber zulassen. Die Sync-Regeln und der Umbau 027/029 sollen dasselbe Format nutzen."

## Begriffe

- **Speicher**, **Speicherverbindung**, **Bereich einer Erweiterung**: wie in Spec 038.
- **Dateibrowser**, **Quelle**, **Viewer**, **Transfer**, **Agent**, **Agent-Berechtigung**: wie in
  Spec 044.
- **Verschlüsselter Ordner**: ein Ordner in einem Speicher, dessen Inhalt holzi verschlüsselt ablegt.
  Im Dateibrowser verhält er sich wie ein gewöhnlicher Ordner mit Unterordnern und Dateien; beim
  Anbieter ist er ein Präfix mit zufälligem Namen, darunter liegen nur Objekte mit zufälligen Namen und
  unlesbarem Inhalt. Sichtbar ist nur, in welchem gewöhnlichen Ordner er liegt; sein Name steht
  verschlüsselt im Kopf des Ordners.
- **Eintrag**: eine Datei oder ein Unterordner in einem verschlüsselten Ordner.
- **Inhaltsobjekt**: der verschlüsselte Inhalt einer Datei als ein Objekt im Speicher, in Blöcken
  verschlüsselt, sodass holzi beliebige Teilbereiche lesen kann.
- **Begleitdatei**: ein kleines verschlüsseltes Objekt je Eintrag mit dem, was der Dateibrowser über
  ihn wissen muss (Elterneintrag und Name, Art, Größe, Änderungszeit, Inhaltstyp, Prüfsumme) und dem
  verschlüsselten Dateischlüssel. Aus den Begleitdateien baut holzi die Ordneransicht auf; sie sind
  die einzige Quelle für die Zuordnung von Pfaden zu Inhaltsobjekten.
- **Kopf des Ordners**: ein Objekt im verschlüsselten Ordner, das ihn als solchen kennzeichnet und
  die Formatversion, den verschlüsselten Ordnerschlüssel und den verschlüsselten Namen des Ordners
  trägt.
- **Schlüssel**: drei Stufen.
  - **Dateischlüssel**: zufällig, einer je Datei; er verschlüsselt den Inhalt und liegt nur
    verschlüsselt mit dem Ordnerschlüssel vor.
  - **Ordnerschlüssel**: zufällig, einer je verschlüsselten Ordner; er verschlüsselt Begleitdateien
    und Dateischlüssel. Wer ihn hat, kann den Ordner lesen. Teilen heißt später, ihn für jemanden
    zusätzlich zu verschlüsseln.
  - **Dateiverschlüsselungsschlüssel der Vault**: aus dem Inhaltsschlüssel der Vault (Spec 024, auf
    jedem eigenen Gerät, in Generationen) abgeleitet und für nichts anderes verwendet. Er verschlüsselt die
    Ordnerschlüssel der Vault und verlässt die Vault nie.
- **Freigabe für einen verschlüsselten Ordner**: die eigene, ausdrückliche Berechtigung eines Agents
  oder einer Erweiterung für genau einen verschlüsselten Ordner, „Lesen“ oder „Lesen und Schreiben“.
  Sie entsteht nur durch eine Antwort des Nutzers in der Frage von holzi, nie durch das Manifest einer
  Erweiterung. Mit dem Haken „Erlaubnis merken“ bleibt sie gespeichert, sonst gilt sie nur für den
  laufenden Vorgang (FR-031).

## Beziehung zu bestehenden Specs

- **Spec 038** (Speicherverbindungen): liefert die Speicher. Ihr „Nicht im Umfang“ schließt die
  Verschlüsselung von Objekten durch holzi aus; diese Spec ergänzt sie für verschlüsselte Ordner, ohne
  die Funktionen für Erweiterungen zu ändern. Was eine Erweiterung in ihrem Bereich ablegt, bleibt so,
  wie sie es schickt.
- **Spec 044** (Dateibrowser): Ein verschlüsselter Ordner ist ein Ordner in der Quelle „Speicher“.
  Navigation, Viewer, Freigabe-URLs, Transfers, Suche und Agent-Aktionen gelten darin wie dort, mit den
  Abweichungen dieser Spec. Das Lesen von Teilbereichen (Video, Audio) bleibt Pflicht.
- **Spec 024** (Sync der eigenen Geräte) und der Sync-Entwurf
  [`docs/plans/2026-09-28-sync-architecture.md`](../../docs/plans/2026-09-28-sync-architecture.md):
  Die eigenen Geräte teilen die Identität der Vault. Daraus kommt der Dateiverschlüsselungsschlüssel;
  es gibt keinen eigenen Schlüsseltausch für verschlüsselte Ordner.
- **Spec 032 und 021** (eingebauter Agent, MCP-Server): Die Dateiaktionen aus 044 bekommen eine
  zusätzliche Berechtigungsebene je verschlüsseltem Ordner (FR-030 bis FR-034).
- **Spec 017** (Erweiterungs-Host) und **038** FR-009, FR-010: Speicher bleiben eine eigene
  Berechtigungsart (`remoteStorage`), getrennt vom Dateizugriff auf dem Gerät; ohne sie erreicht eine
  Erweiterung keinen Speicher. Mit ihr sieht sie weiter nur ihren eigenen Bereich. Einzige Ausnahme
  ist ein verschlüsselter Ordner mit Freigabe (FR-035 bis FR-039); diese Spec ändert 038 FR-010
  dafür.
- **Spec 050** (WebDAV- und SFTP-Speicher): bringt Speicher mit diesen Protokollen und verlangt, dass
  verschlüsselte Ordner dort genauso funktionieren (050 FR-024). `ObjectInfo.etag` führt die Spec ein,
  die zuerst landet (050 research R8).
- **Umbau 027/029** (Spaces als Rahmen mit Einträgen) und **Sync-Regeln** (ersetzt 025, noch ohne
  Nummer): sollen dasselbe Format nutzen. Ein Space verschlüsselt den Ordnerschlüssel zusätzlich für
  seine Mitglieder; die Sync-Regeln legen „Cloud verschlüsselt“ als verschlüsselten Ordner an. Beides
  ist nicht Teil dieser Spec, das Format MUSS es aber zulassen (FR-040, FR-041).
- **Vorlage haex-vault**: Repository `https://github.com/haex-space/haex-vault`, Revision
  `8dce379d94e18fcd42c3b73686a06f984ca3f574`, Pfad `src-tauri/src/file_sync/crypto/` (Envelope `HXFE`,
  Blöcke zu 1 MiB mit XChaCha20-Poly1305, zufällige Objektnamen, Begleitdateien `.m`, Dateischlüssel
  verschlüsselt mit dem Schlüssel der Stufe darüber). holzi übernimmt das Modell und schließt seine
  Lücken (FR-022 bis FR-025); haex-vault ist kein Maßstab für Oberfläche.

## Clarifications

### Session 2026-10-10

- Q: Ganzer Speicher oder einzelne Ordner? → A: Einzelne Ordner. Der übrige Speicher bleibt, wie er
  ist (Betreiber).
- Q: Container (VeraCrypt, LVM) oder je Datei? → A: Je Datei nach dem Modell Cryptomator/gocryptfs; ein
  Container müsste bei jeder Änderung ganz neu hochgeladen werden, weil S3 Objekte nicht teilweise
  überschreibt.
- Q: Gefährdet der Vault-Schlüssel ein späteres Teilen? → A: Nein, mit drei Stufen: Teilen verschlüsselt
  den Ordnerschlüssel für den Empfänger; der Dateiverschlüsselungsschlüssel der Vault verlässt die
  Vault nie (Betreiber, mit dem Vorschlag einverstanden).
- Q: Wo liegt die Zuordnung Pfad → Objekt? → A: Im Bucket, in den Begleitdateien. Ihr Lesen muss
  schnell sein (Betreiber).
- Q: Wie erreichen Erweiterungen und Agents verschlüsselte Ordner? → A: Nur mit einer ausdrücklichen
  Freigabe je Ordner; die Berechtigung für den Speicher reicht nicht (Betreiber).
- Q: Müssen bestehende Ordner umgewandelt werden? → A: Nein, es gibt keine (Betreiber).
- Q: Dürfen Erweiterungen verschlüsselte Ordner erreichen? → A: Ja, aber nur über Speicher als
  eigene Berechtigung (`remoteStorage`, getrennt vom Dateizugriff) und nur nach einer Frage an den
  Nutzer vor dem Zugriff; mit dem Haken „Erlaubnis merken“ fragt holzi danach nicht mehr. Die
  Freigabe DARF NICHT im Manifest stehen. Sonst sehen Erweiterungen nur ihre eigenen Daten. Für
  Agents gilt derselbe Dialog (Betreiber).
- Q: Wie weit reicht eine Erlaubnis ohne „Erlaubnis merken“? → A: Für den laufenden Vorgang: eine
  Erweiterung, bis sie beendet oder neu geladen wird; ein Agent bis zum Ende seiner Aufgabe (Turn im
  Chat, MCP-Sitzung); höchstens bis zum Sperren der Vault (Betreiber).
- Q: Darf der Anbieter den Namen des verschlüsselten Ordners sehen? → A: Nein. Das Präfix ist
  zufällig, der Name steht verschlüsselt im Kopf; Umbenennen ändert nur den Kopf (Betreiber).
- Q: Dürfen Agents mit Freigabe Inhalte an Cloud-Modelle geben? → A: Ja, aber die Frage nennt Modell
  und ob es lokal oder in der Cloud läuft; eine Freigabe gilt nur lokal oder ausdrücklich auch für
  Cloud-Modelle, und beim Wechsel auf ein Cloud-Modell fragt holzi neu (Betreiber).
- Q: Soll „Mit System-App öffnen“ in verschlüsselten Ordnern gehen? → A: Ja, mit Hinweis (Haken
  „Nicht mehr fragen“ je Ordner), Kopie in einem eigenen Ordner von holzi, gelöscht beim Sperren oder
  Schließen der Vault und nach einem Absturz beim nächsten Start. Auf Android dürfen andere Apps nicht
  in holzis Speicher schauen; dort nur über eine befristete Leseberechtigung für genau diese Datei
  (Betreiber).
- Q: Braucht ein eigenes Gerät ein Passwort für den Ordner? → A: Nein. Jedes Gerät der Vault liest
  ohne Rückfrage.

## User Scenarios & Testing _(mandatory)_

### User Story 1 - Verschlüsselten Ordner anlegen und nutzen (Priority: P1)

Anna hat einen Speicher bei ihrem S3-Anbieter. Im Dateibrowser legt sie dort den Ordner „Unterlagen“
als verschlüsselten Ordner an. Er trägt ein Schloss-Symbol. Sie zieht ein PDF, ein Foto und einen
Ordner mit Rechnungen hinein, öffnet danach das PDF und das Foto im Viewer und arbeitet in dem Ordner
wie in jedem anderen.

**Why this priority**: Ohne Anlegen, Hineinlegen und Öffnen gibt es keinen verschlüsselten Ordner;
alles andere baut darauf auf.

**Independent Test**: In einem Testspeicher einen verschlüsselten Ordner anlegen, Dateien und einen
Unterordner hineinkopieren, die Liste prüfen (Namen, Größen, Typen, Vorschaubild) und Text, PDF und
Bild öffnen.

**Acceptance Scenarios**:

1. **Given** ein Speicher im Dateibrowser, **When** Anna „Neuer verschlüsselter Ordner“ wählt und
   einen Namen eingibt, **Then** erscheint der Ordner mit Schloss-Symbol, und holzi weist einmal
   darauf hin, dass der Inhalt ohne diese Vault nicht lesbar ist.
2. **Given** ein verschlüsselter Ordner, **When** Anna Dateien und Ordner vom Gerät hineinzieht,
   **Then** zeigt der Dateibrowser sie mit ihren Namen, Größen und Änderungszeiten, und der Transfer
   zeigt Fortschritt und lässt sich abbrechen wie in 044.
3. **Given** ein PDF und ein Foto im verschlüsselten Ordner, **When** Anna sie öffnet, **Then** zeigt
   der Viewer sie, und das Foto hat in der Liste ein Vorschaubild.
4. **Given** ein verschlüsselter Ordner mit Unterordnern, **When** Anna navigiert, über die
   Pfadleiste zurückspringt oder den Tab nach einem Neustart wiederherstellt, **Then** verhält sich
   alles wie in einem gewöhnlichen Ordner.

---

### User Story 2 - Der Anbieter erfährt nichts über den Inhalt (Priority: P1)

Anna öffnet die Weboberfläche ihres Anbieters und sieht sich den Bucket an. Statt „Unterlagen“ findet
sie einen Ordner mit zufälligem Namen und darin nur Objekte mit zufälligen Namen. Keine Datei, kein Ordnername und keine Struktur aus dem Ordner
ist zu erkennen. Ein Objekt, das sie dort von Hand verändert, öffnet holzi nicht mehr, sondern meldet
es als beschädigt.

**Why this priority**: Das ist der Zweck der Funktion. Was hier nicht stimmt, macht alles andere
wertlos.

**Independent Test**: Nach dem Befüllen den Bucket direkt beim Anbieter auflisten und lesen: kein Name,
Pfad, Inhalt oder Inhaltstyp aus dem Ordner kommt in Objektnamen, Objektinhalten oder Metadaten vor.
Dann einzelne Objekte verändern, kürzen, vertauschen und durch ältere ersetzen und das Verhalten von
holzi prüfen.

**Acceptance Scenarios**:

1. **Given** ein verschlüsselter Ordner mit `Rechnungen/2026/strom.pdf`, **When** jemand den Bucket
   beim Anbieter auflistet, **Then** kommen weder „Unterlagen“, „Rechnungen“, „2026“, „strom“ noch „pdf“ in einem
   Objektnamen vor, und alle Objekte liegen flach unter dem Ordner, ohne die innere Struktur
   nachzubilden.
2. **Given** ein Inhaltsobjekt, **When** ein Byte darin verändert wird, **Then** meldet holzi die
   Datei beim Öffnen als beschädigt und zeigt keinen Teil des veränderten Inhalts.
3. **Given** zwei Dateien im Ordner, **When** ihre Inhaltsobjekte beim Anbieter vertauscht oder eines
   am Ende gekürzt wird, **Then** erkennt holzi das beim Lesen und meldet beide als beschädigt, statt
   den falschen oder gekürzten Inhalt zu zeigen.
4. **Given** eine beschädigte Datei, **When** Anna den Ordner öffnet, **Then** sind alle übrigen
   Einträge weiter lesbar.

---

### User Story 3 - Auf einem anderen eigenen Gerät lesen (Priority: P1)

Anna öffnet ihre Vault auf dem Telefon. Der Speicher ist über den Datensync schon da. Sie öffnet
„Unterlagen“ und sieht dieselben Dateien wie am Desktop, ohne ein Passwort einzugeben. Eine Datei, die
sie unterwegs fotografiert und hineinlegt, sieht sie danach am Desktop.

**Why this priority**: Verschlüsselte Ordner, die nur auf einem Gerät lesbar sind, wären für eine
Vault mit mehreren Geräten nutzlos.

**Independent Test**: Zwei Geräte derselben Vault (Rig von Spec 033): auf Gerät 1 einen verschlüsselten
Ordner anlegen und befüllen; auf Gerät 2 ohne Rückfrage auflisten und öffnen; auf Gerät 2 eine Datei
ergänzen; auf Gerät 1 nach dem Neuladen sehen.

**Acceptance Scenarios**:

1. **Given** ein verschlüsselter Ordner von Gerät 1, **When** Gerät 2 derselben Vault ihn zum ersten
   Mal öffnet, **Then** zeigt es alle Einträge ohne Passwort oder sonstige Rückfrage.
2. **Given** eine andere Vault mit Zugang zum selben Bucket, **When** sie den Ordner öffnet, **Then**
   sieht sie ihn als verschlüsselten Ordner einer anderen Vault, ohne Inhalt, und kann darin nichts
   ändern.
3. **Given** beide Geräte schreiben gleichzeitig eine Datei gleichen Namens in denselben Ordner,
   **When** beide neu laden, **Then** sind beide Fassungen erhalten, eine davon als Konfliktkopie.

---

### User Story 4 - Videos und große Dateien abspielen (Priority: P2)

Anna legt ein 4-GB-Urlaubsvideo in den verschlüsselten Ordner. Auf dem Desktop und auf dem Telefon
startet es nach wenigen Sekunden und springt an jede Stelle, ohne dass holzi die ganze Datei lädt.

**Why this priority**: Fotos und Videos sind ein Hauptgrund für eigenen Speicher; ohne Springen ist
der Ordner für Medien unbrauchbar. Es baut auf US1 auf.

**Independent Test**: Ein Video (Fixture aus 044, plus ein großes, im Test erzeugtes) hochladen,
abspielen, in die Mitte springen; prüfen, dass holzi nur die nötigen Teilbereiche beim Anbieter liest
und der Speicherbedarf nicht mit der Dateigröße wächst.

**Acceptance Scenarios**:

1. **Given** ein Video im verschlüsselten Ordner, **When** Anna es abspielt und in die Mitte springt,
   **Then** liest holzi nur die Blöcke um diese Stelle und entschlüsselt sie, nie die ganze Datei.
2. **Given** ein Upload einer großen Datei, **When** Anna ihn abbricht oder das Netz abreißt,
   **Then** erscheint im Ordner kein halber Eintrag, und auf Dauer bleibt beim Anbieter kein
   Inhaltsobjekt ohne Begleitdatei zurück.

---

### User Story 5 - Im verschlüsselten Ordner aufräumen (Priority: P2)

Anna benennt den Unterordner „Rechnungen“ in „Belege“ um, verschiebt einige Dateien, kopiert eine,
löscht eine andere und sucht nach „strom“. Alles geht wie in einem gewöhnlichen Ordner. Umbenennen und
Verschieben im Ordner laden keinen Inhalt neu hoch.

**Why this priority**: Ein Ordner, in dem man nur hinzufügen kann, ist ein Archiv. Es baut auf US1
auf.

**Independent Test**: In einem verschlüsselten Ordner mit 1 000 Dateien in Unterordnern umbenennen,
verschieben, kopieren, löschen und suchen; beim Anbieter prüfen, dass Umbenennen und Verschieben kein
Inhaltsobjekt neu schreiben.

**Acceptance Scenarios**:

1. **Given** ein Unterordner mit 1 000 Dateien, **When** Anna ihn umbenennt, **Then** ändert holzi nur
   Begleitdateien, kein Inhaltsobjekt.
2. **Given** eine Datei im verschlüsselten Ordner, **When** Anna sie in einen gewöhnlichen Ordner
   desselben oder eines anderen Speichers kopiert oder verschiebt, **Then** fragt holzi vorher, weil
   sie dort unverschlüsselt liegen wird; aufs Gerät fragt es nicht.
3. **Given** eine Suche nach „strom“ im verschlüsselten Ordner, **When** sie läuft, **Then** findet sie
   `Belege/2026/strom.pdf` über die entschlüsselten Namen.
4. **Given** Anna löscht den verschlüsselten Ordner selbst, **When** sie die Frage bestätigt, **Then**
   entfernt holzi alle seine Objekte beim Anbieter.

---

### User Story 6 - Agents und Erweiterungen nur mit ausdrücklicher Freigabe (Priority: P2)

Annas Agent hat „Lesen“ für ihren Speicher. Als er eine Datei aus „Unterlagen“ lesen soll, fragt
holzi Anna, ob der Agent diesen verschlüsselten Ordner lesen darf. Erst nach ihrem „Ja“ liest er.
Später widerruft sie die Freigabe in den Einstellungen.

**Why this priority**: Verschlüsselte Inhalte sind besonders schützenswert; ohne eigene Freigabe würde
die Berechtigung für den Speicher sie Agents (und Cloud-Modellen) öffnen. Es baut auf US1 auf.

**Independent Test**: Ein Agent mit Speicherberechtigung listet den Speicher, versucht den
verschlüsselten Ordner zu listen, zu lesen und zu durchsuchen: ohne Freigabe kommt nichts aus dem
Ordner zurück und holzi fragt; nach „Lesen“ liest er, Schreiben bleibt verweigert; nach dem Widerruf
wieder nichts.

**Acceptance Scenarios**:

1. **Given** ein Agent mit „Lesen“ für den Speicher und ohne Freigabe für den Ordner, **When** er den
   Speicher listet, **Then** sieht er den Ordner nur als verschlüsselten Ordner ohne Namen und Inhalt.
2. **Given** derselbe Agent, **When** er etwas im Ordner listet, liest oder sucht, **Then** fragt holzi
   den Nutzer (Lesen erlauben, Lesen und Schreiben erlauben, Ablehnen), nennt dabei das Modell und ob
   es lokal oder in der Cloud läuft, und merkt sich die Antwort nur mit „Erlaubnis merken“.
3. **Given** eine gespeicherte Freigabe „nur lokal“, **When** Anna im Chat auf ein Cloud-Modell
   wechselt und der Agent wieder auf den Ordner zugreift, **Then** fragt holzi neu.
4. **Given** eine Freigabe „Lesen“, **When** der Agent schreiben will, **Then** lehnt holzi ab.
5. **Given** eine Suche des Agents über den ganzen Speicher, **When** er keine Freigabe für den Ordner
   hat, **Then** enthält das Ergebnis nichts aus dem Ordner.
6. **Given** eine Erweiterung mit der Berechtigung `remoteStorage` für den Speicher, **When** sie auf
   „Unterlagen“ zugreifen will, **Then** fragt holzi Anna zuerst; ohne Haken bei „Erlaubnis merken“
   gilt die Antwort, bis die Erweiterung beendet oder neu geladen oder die Vault gesperrt wird, danach
   fragt holzi wieder; mit Haken fragt es nicht mehr.
7. **Given** eine Erweiterung, deren Manifest eine Freigabe für verschlüsselte Ordner mitbringt,
   **When** Anna sie installiert, **Then** lehnt holzi die Installation mit einer verständlichen
   Meldung ab.
8. **Given** eine Erweiterung ohne `remoteStorage` für den Speicher, **When** sie auf den Ordner
   zugreifen will, **Then** fragt holzi nicht einmal, sondern lehnt ab wie heute in 038.

### Edge Cases

- **Vault verloren**: Ohne die Vault (oder ein Backup von ihr) sind die Inhalte nicht mehr lesbar. Es
  gibt keinen Weg daran vorbei; holzi sagt das beim Anlegen (US1, Szenario 1).
- **Neuere Formatversion**: Ein Ordner, den eine neuere holzi-Version mit einem neueren Format
  angelegt hat, wird nicht geöffnet; holzi sagt, dass ein Update nötig ist, und schreibt nichts hinein.
- **Kopf fehlt oder ist beschädigt**: Der Ordner wird als beschädigter verschlüsselter Ordner gezeigt,
  ohne Inhalt; holzi schreibt nichts hinein und legt keinen neuen Kopf über einen alten.
- **Begleitdatei ohne Inhaltsobjekt**: Der Eintrag erscheint als beschädigt; die übrigen bleiben
  lesbar.
- **Inhaltsobjekt ohne Begleitdatei** (abgebrochener Upload, abgestürztes Gerät): Es erscheint nicht.
  holzi entfernt es frühestens 24 Stunden nach seiner Änderungszeit, damit kein laufender Upload eines
  anderen Geräts getroffen wird.
- **Zwei Begleitdateien für denselben Pfad** (gleichzeitiges Schreiben): Beide Einträge bleiben; der
  ältere erscheint als Konfliktkopie mit eigenem Namen. Nichts geht still verloren.
- **Datei wird überschrieben, während ein anderes Gerät sie liest**: Der Leser bekommt entweder die
  alte oder die neue Fassung ganz, nie eine Mischung.
- **Leerer Unterordner**: Er bleibt sichtbar, auch auf anderen Geräten.
- **Verschlüsselter Ordner in einem verschlüsselten Ordner**: Nicht möglich; ein Unterordner darin ist
  einfach ein verschlüsselter Unterordner.
- **Verschlüsselter Ordner im Bereich einer Erweiterung** (038 FR-010): Nicht möglich; holzi bietet es
  dort nicht an.
- **Den verschlüsselten Ordner selbst umbenennen**: holzi ändert nur seinen Kopf; kein anderes
  Objekt wird geschrieben.
- **Den verschlüsselten Ordner in einen anderen gewöhnlichen Ordner verschieben**: Sein zufälliges
  Präfix liegt unter dem Pfad des Elternordners; holzi kopiert dabei alle seine Objekte beim Anbieter
  und löscht die alten, wie bei einem gewöhnlichen Ordner in 044. Inhalte werden nicht neu
  verschlüsselt.
- **Gleicher Name zweimal**: Weil die Präfixe zufällig sind, prüft holzi beim Anlegen und Umbenennen
  die entschlüsselten Namen der verschlüsselten Ordner und die gewöhnlichen Ordner und Dateien im
  selben Elternordner und lehnt einen doppelten Namen ab. Legen zwei Geräte gleichzeitig denselben
  Namen an, erscheinen beide, einer als Konfliktkopie.
- **Gewöhnlicher Name, der wie ein verschlüsselter Ordner aussieht** (26 Zeichen Base32 und
  `.hxef`): holzi legt einen solchen gewöhnlichen Ordner oder eine solche Datei nicht an und meldet
  einen ungültigen Namen, damit er nicht als beschädigter verschlüsselter Ordner erscheint.
- **Ordner einer anderen Vault**: Weil holzi seinen Namen nicht entschlüsseln kann, zeigt es ihn mit
  einer neutralen Bezeichnung („Verschlüsselter Ordner einer anderen Vault“) und der Änderungszeit
  seines Kopfes beim Anbieter.
- **Gerät entfernt, neue Schlüsselgeneration** (Spec 024, D22): Bestehende Ordner bleiben über ihre
  Generation lesbar, neue nutzen die aktuelle. Ein entferntes Gerät, das einen Ordnerschlüssel schon
  kannte, behält ihn; Entzug ist nicht im Umfang.
- **Gerät noch ohne Inhaltsschlüssel** (vor dem ersten Datensync): holzi kann keinen verschlüsselten
  Ordner anlegen oder öffnen und sagt das.
- **Viele Einträge**: Ein Ordner mit 10 000 Einträgen muss sich auf einem neuen Gerät öffnen lassen,
  ohne dass holzi alle Begleitdateien nacheinander lädt (SC-003).
- **Sperren der Vault während eines Transfers oder einer Wiedergabe**: Transfer und Freigabe-URL enden
  wie in 044; danach liegt nichts Entschlüsseltes mehr im Speicher von holzi.
- **„Mit System-App öffnen“**: Dafür entsteht eine entschlüsselte Kopie auf dem Gerät. holzi weist
  vorher darauf hin und entfernt sie spätestens beim Sperren oder Schließen der Vault (FR-028). Auf
  Android sieht keine andere App holzis privaten Speicher; dort bekommt nur die gewählte App eine
  befristete Leseberechtigung für genau diese Datei (FR-028a).
- **Speicher hat Versionierung eingeschaltet**: Alte Fassungen bleiben beim Anbieter liegen, auch
  nach dem Löschen. holzi nennt das im Hinweis beim Löschen, wenn es die Versionierung erkennt.
- **Mehr als die Größengrenze des Anbieters für ein Objekt**: wie in 044 (mehrteiliger Upload); die
  Verschlüsselung in Blöcken DARF das nicht verhindern.

## Requirements _(mandatory)_

### Functional Requirements

**Anlegen und Erkennen**

- **FR-001**: Der Nutzer MUSS im Dateibrowser in jedem Ordner eines Speichers einen neuen
  verschlüsselten Ordner anlegen können, außer innerhalb eines verschlüsselten Ordners und im Bereich
  einer Erweiterung.
- **FR-002**: holzi MUSS beim Anlegen einen zufälligen Ordnerschlüssel erzeugen, ihn mit dem
  Dateiverschlüsselungsschlüssel der Vault verschlüsseln und mit Formatversion und dem mit dem
  Ordnerschlüssel verschlüsselten Namen im Kopf des Ordners ablegen. Das Präfix des Ordners beim
  Anbieter MUSS zufällig sein und DARF seinen Namen nicht verraten. Vor dem ersten Anlegen MUSS holzi einmal darauf hinweisen, dass der Inhalt ohne diese
  Vault nicht lesbar ist.
- **FR-003**: holzi MUSS einen verschlüsselten Ordner an seinem Kopf erkennen und im Dateibrowser mit
  einem eigenen Symbol und dem Zustand zeigen: lesbar, gehört einer anderen Vault, neuere
  Formatversion, beschädigt.
- **FR-004**: Ein gewöhnlicher Ordner DARF NICHT nachträglich in einen verschlüsselten umgewandelt
  werden und umgekehrt. Wer Inhalte verschlüsseln will, verschiebt sie in einen verschlüsselten
  Ordner.

**Schlüssel**

- **FR-005**: Der Dateiverschlüsselungsschlüssel der Vault MUSS aus dem Inhaltsschlüssel der Vault
  abgeleitet werden, den jedes eigene Gerät hat, und nur für verschlüsselte Ordner verwendet werden.
  Der Kopf eines Ordners MUSS die Generation nennen, mit der er verpackt ist; ein Gerät MUSS jeden Ordner
  mit jeder Generation lesen können, die es hat. Er DARF die Vault nie verlassen, weder im Speicher noch an Agents, Erweiterungen,
  Logs oder Freigaben.
- **FR-006**: Jede Datei MUSS einen eigenen zufälligen Dateischlüssel haben. Eine neue Fassung einer
  Datei MUSS einen neuen Dateischlüssel bekommen.
- **FR-007**: Jedes eigene Gerät der Vault MUSS jeden verschlüsselten Ordner der Vault ohne Passwort
  oder sonstige Eingabe lesen und schreiben können.

**Was beim Anbieter liegt**

- **FR-009**: Den Namen eines verschlüsselten Ordners MUSS holzi aus seinem Kopf entschlüsseln und
  im Dateibrowser, in der Pfadleiste, in Sitzungen und in Antworten an Agents mit Freigabe zeigen.
  Umbenennen MUSS nur den Kopf ändern. Ein Agent oder eine Erweiterung ohne Freigabe erfährt den Namen
  nicht (FR-032, FR-039).
- **FR-010**: Unter einem verschlüsselten Ordner DÜRFEN beim Anbieter nur der Kopf, Inhaltsobjekte
  und Begleitdateien liegen. Ihre Namen MÜSSEN zufällig sein und DÜRFEN weder Namen, Pfade, Struktur,
  Inhaltstyp, Größe noch Prüfsummen der Einträge verraten; die Objekte bilden die innere Struktur
  nicht nach.
- **FR-011**: Objektinhalte und Metadaten beim Anbieter DÜRFEN keinen Klartext der Einträge enthalten.
  Inhaltstyp und Metadaten der Objekte MÜSSEN neutral sein.
- **FR-012**: Begleitdateien MÜSSEN Elterneintrag und Name (nicht den ganzen Pfad, damit Umbenennen
  eines Unterordners nur seine eigene Begleitdatei ändert), Art (Datei oder Ordner), Größe, Änderungszeit,
  Inhaltstyp und Prüfsumme des Klartexts sowie den verschlüsselten Dateischlüssel tragen, alles
  verschlüsselt mit dem Ordnerschlüssel. Leere Unterordner MÜSSEN eine eigene Begleitdatei haben.
- **FR-013**: Die Ansicht eines verschlüsselten Ordners MUSS sich allein aus dem Bucket aufbauen
  lassen. holzi DARF dafür auf dem Gerät einen Zwischenspeicher halten, der nur in den verschlüsselten
  Daten der Vault liegt, nicht synchronisiert wird und jederzeit aus dem Bucket neu entstehen kann.

**Lesen und Schreiben**

- **FR-014**: Alle Aktionen des Dateibrowsers aus 044 (Navigieren, Liste und Raster, Vorschaubilder,
  Viewer, Freigabe-URLs, Hochladen, Herunterladen, Anlegen, Umbenennen, Kopieren, Verschieben, Löschen,
  Suchen, Drag & Drop ins Fenster, Wiederherstellen der Sitzung) MÜSSEN in einem verschlüsselten
  Ordner funktionieren; holzi ver- und entschlüsselt dabei ohne Zutun des Nutzers.
- **FR-015**: Inhalte MÜSSEN in Blöcken verschlüsselt sein, sodass holzi einen beliebigen
  Teilbereich liest, indem es nur die betroffenen Blöcke beim Anbieter lädt. Wiedergabe und Springen
  in Video und Audio MÜSSEN so funktionieren wie in 044; holzi DARF eine Datei dafür nie ganz in den
  Arbeitsspeicher laden.
- **FR-016**: Ein Eintrag MUSS erst erscheinen, wenn sein Inhaltsobjekt vollständig geschrieben ist.
  Abgebrochene oder gescheiterte Uploads DÜRFEN keinen sichtbaren Eintrag hinterlassen; verwaiste
  Inhaltsobjekte MUSS holzi frühestens 24 Stunden nach ihrer Änderungszeit entfernen.
- **FR-017**: Beim Überschreiben MUSS holzi die neue Fassung vollständig ablegen, bevor die alte
  verschwindet; ein Leser bekommt immer eine Fassung ganz.
- **FR-018**: Umbenennen und Verschieben innerhalb eines verschlüsselten Ordners DÜRFEN kein
  Inhaltsobjekt neu schreiben. Kopieren innerhalb desselben verschlüsselten Ordners SOLL ohne
  Herunterladen beim Anbieter geschehen.
- **FR-019**: Kopieren oder Verschieben aus einem verschlüsselten Ordner in einen gewöhnlichen Ordner
  eines Speichers MUSS vorher fragen, weil der Inhalt dort unverschlüsselt liegt. Aufs Gerät fragt
  holzi nicht.
- **FR-020**: Kopieren zwischen zwei verschlüsselten Ordnern derselben Vault MUSS den Inhalt im
  Zielordner mit dessen Ordnerschlüssel lesbar machen.
- **FR-021**: Gleichzeitiges Schreiben desselben Pfads von zwei Geräten DARF keine Fassung verlieren;
  holzi MUSS beide zeigen und eine als Konfliktkopie benennen.

**Unversehrtheit**

- **FR-022**: holzi MUSS jede Veränderung von Kopf, Begleitdatei oder Inhaltsobjekt beim Lesen
  erkennen und den betroffenen Eintrag als beschädigt melden, ohne veränderten Klartext zu zeigen oder
  an Viewer, Agents oder Transfers zu geben.
- **FR-023**: holzi MUSS erkennen, wenn ein Inhaltsobjekt gekürzt, verlängert, gegen das einer
  anderen Datei vertauscht oder Blöcke darin umgestellt wurden.
- **FR-024**: holzi MUSS erkennen, wenn eine Begleitdatei auf ein Inhaltsobjekt zeigt, das nicht zu
  ihr gehört.
- **FR-025**: Eine beschädigte Datei DARF das Lesen anderer Einträge desselben Ordners nicht
  verhindern.

**Spuren auf dem Gerät**

- **FR-026**: Klartext von Inhalten, Namen oder Pfaden aus verschlüsselten Ordnern DARF auf dem Gerät
  nur in den verschlüsselten Daten der Vault liegen. Vorschaubilder MÜSSEN entweder nur im
  Arbeitsspeicher oder in den verschlüsselten Daten der Vault liegen, nie im gewöhnlichen
  Zwischenspeicher von 044.
- **FR-027**: Logs und Fehlermeldungen DÜRFEN keine Namen, Pfade oder Inhalte aus verschlüsselten
  Ordnern enthalten, auch nicht auf Debug-Stufe.
- **FR-028**: „Mit System-App öffnen“ MUSS für Dateien aus verschlüsselten Ordnern vor dem Öffnen
  darauf hinweisen, dass eine entschlüsselte Kopie auf dem Gerät entsteht, die die andere App behalten
  kann; mit dem Haken „Nicht mehr fragen“ gilt das für diesen Ordner. Die Kopie MUSS in einem eigenen
  Ordner von holzi liegen, außerhalb des gewöhnlichen Zwischenspeichers aus 044, und MUSS spätestens
  beim Sperren oder Schließen der Vault entfernt werden, nach einem Absturz beim nächsten Start.
- **FR-028a**: Auf Android MUSS holzi die Kopie in seinem privaten Speicher halten und der gewählten
  App nur über eine befristete Leseberechtigung für genau diese eine Datei geben; sie DARF nie in einen
  gemeinsamen Ordner (Downloads, Dokumente) gelegt werden. Solange 044 „Mit System-App öffnen“ auf
  Android nicht anbietet (heute „noch nicht auf dieser Plattform“), gilt das auch für verschlüsselte
  Ordner.

**Agents und Erweiterungen**

- **FR-030**: Ein Agent DARF einen verschlüsselten Ordner nur mit einer Freigabe für genau diesen
  Ordner erreichen: „Lesen“ oder „Lesen und Schreiben“, ab Werk für keinen Agent, zusätzlich zur
  Berechtigung für den Speicher (044 FR-031a). Die Freigabe für den Ordner DARF NICHT mehr erlauben als
  die für den Speicher.
- **FR-030a**: Die Frage MUSS nennen, wohin gelesene Inhalte gehen: das Modell des Agents und ob es
  auf dem Gerät oder in der Cloud läuft. Externe Agents (Spec 021) und Delegates, deren Modell holzi
  nicht kennt, MÜSSEN wie Cloud-Modelle behandelt und so benannt werden. Eine Freigabe gilt entweder
  nur für lokale Modelle oder ausdrücklich auch für Cloud-Modelle; ab Werk ist „nur lokal“ gewählt,
  wenn der Agent gerade ein lokales Modell nutzt. Wechselt ein Agent mit einer Freigabe „nur lokal“
  auf ein Cloud-Modell, MUSS holzi vor dem nächsten Zugriff neu fragen.
- **FR-031**: Fehlt die Freigabe, MUSS holzi den Nutzer fragen (Lesen erlauben, Lesen und Schreiben
  erlauben, Ablehnen) mit einem Haken „Erlaubnis merken“, ab Werk nicht gesetzt. Nur mit Haken MUSS
  holzi die Antwort speichern. Ohne Haken gilt sie nur für den laufenden Vorgang: bei einer
  Erweiterung, bis sie beendet oder neu geladen wird, bei einem Agent bis zum Ende seiner Aufgabe
  (ein Turn im Chat, eine Sitzung über MCP), in jedem Fall höchstens bis zum Sperren der Vault.
- **FR-032**: Ohne Freigabe DARF ein Agent aus dem Ordner nichts erfahren außer dass es ihn gibt und
  dass er verschlüsselt ist, unter einer neutralen Bezeichnung, mit der er nach der Freigabe fragen
  kann; den Namen nennt holzi nur dem Nutzer in der Frage. Er erfährt keinen Namen, keine Einträge, keine Anzahl, keine Suchtreffer, keine Objekte beim Anbieter,
  auch nicht verschlüsselt.
- **FR-033**: Der Nutzer MUSS die Freigaben jedes Agents für verschlüsselte Ordner bei den
  Datei-Berechtigungen des Agents sehen und widerrufen können (044 FR-031b). Ein Widerruf MUSS sofort
  gelten.
- **FR-034**: Externe Agents (Spec 021) MÜSSEN denselben Regeln folgen.
- **FR-035**: Eine Erweiterung DARF Speicher nur mit der Berechtigung `remoteStorage` erreichen,
  getrennt vom Dateizugriff auf dem Gerät (038 FR-009). Sonst sieht sie nur ihren eigenen Bereich (038
  FR-010); einzige Ausnahme sind verschlüsselte Ordner nach FR-036.
- **FR-036**: Will eine Erweiterung auf einen verschlüsselten Ordner zugreifen, MUSS holzi den Nutzer
  zuerst fragen, mit denselben Antworten und demselben Haken wie in FR-031, und zwar in einem Dialog
  von holzi, den die Erweiterung weder gestalten noch auslösen kann, ohne zuzugreifen. Ohne
  `remoteStorage` für den Speicher MUSS holzi ablehnen, ohne zu fragen. Die Erweiterung bekommt
  entschlüsselte Inhalte, nie rohe Objekte, Schlüssel oder Begleitdateien.
- **FR-037**: Eine Freigabe für verschlüsselte Ordner DARF NICHT im Manifest einer Erweiterung stehen.
  holzi MUSS ein Manifest, das sie verlangt, bei Installation und Update mit einer verständlichen
  Meldung ablehnen. Auch Entwicklerversionen und Updates DÜRFEN keine gespeicherte Freigabe erben,
  die der Nutzer nicht für genau diese Erweiterung gegeben hat.
- **FR-038**: Gespeicherte Freigaben einer Erweiterung MUSS der Nutzer bei ihren Berechtigungen sehen
  und widerrufen können (Spec 017); ein Widerruf MUSS sofort gelten. Entfernen der Erweiterung MUSS
  ihre Freigaben entfernen.
- **FR-039**: Ohne Freigabe DARF eine Erweiterung aus einem verschlüsselten Ordner nichts erfahren
  außer der Ablehnung.

**Format für später**

- **FR-040**: Das Format MUSS zulassen, den Ordnerschlüssel zusätzlich für weitere Empfänger zu
  verschlüsseln (Spaces, Umbau 027/029), ohne Inhaltsobjekte, Dateischlüssel oder Begleitdateien neu
  zu schreiben und ohne den Dateiverschlüsselungsschlüssel der Vault weiterzugeben.
- **FR-041**: Das Format MUSS eine Versionsangabe tragen und so beschrieben sein, dass die
  Sync-Regeln und der Umbau 027/029 es ohne zweite Umsetzung nutzen. Ver- und Entschlüsselung MÜSSEN
  an genau einer Stelle in holzi umgesetzt sein.
- **FR-042**: Das Format MUSS in einem Dokument im Repository beschrieben sein (Aufbau aller drei
  Objektarten, Schlüsselableitung, Verfahren, Versionen), damit Ordner auch ohne holzi geprüft werden
  können, und es MUSS Testvektoren geben.

### Key Entities

- **Verschlüsselter Ordner**: Speicher, Elternordner, zufälliges Präfix, verschlüsselter Name,
  Formatversion, verschlüsselter Ordnerschlüssel (je Empfänger, in dieser Spec nur die eigene Vault), Zustand für die Anzeige.
- **Eintrag**: Elterneintrag, Name, Art, Größe, Änderungszeit, Inhaltstyp, Prüfsumme; nur in seiner
  Begleitdatei, nie im Klartext beim Anbieter.
- **Inhaltsobjekt**: zufälliger Name, verschlüsselter Inhalt in Blöcken, gehört zu genau einer
  Begleitdatei.
- **Begleitdatei**: zufälliger Name, verschlüsselte Angaben des Eintrags und sein verschlüsselter
  Dateischlüssel.
- **Freigabe für einen verschlüsselten Ordner**: Agent oder Erweiterung, Speicher, Ordner, Stufe
  (Lesen, Lesen und Schreiben, abgelehnt), bei Agents die Reichweite (nur lokale Modelle, auch
  Cloud-Modelle); nur gespeichert, wenn der Nutzer „Erlaubnis merken“
  angehakt hat; nie aus einem Manifest.
- **Zwischenspeicher der Ordneransicht**: je Gerät, in den verschlüsselten Daten der Vault, nicht
  synchronisiert, aus dem Bucket wiederherstellbar.

## Success Criteria _(mandatory)_

### Measurable Outcomes

- **SC-001**: In 100 % der Prüfungen eines Katalogs (Namen mit Umlauten und Sonderzeichen, tiefe
  Pfade, leere Ordner, alle Inhaltstypen aus 044) kommt kein Name, Pfadteil, Inhaltstyp oder
  Klartextausschnitt in Objektnamen, Objektinhalten oder Metadaten beim Anbieter vor.
- **SC-002**: In 100 % der geprüften Manipulationen (verändertes Byte, gekürztes, verlängertes,
  vertauschtes Objekt, umgestellte Blöcke, fremde Begleitdatei) meldet holzi den Eintrag als beschädigt
  und gibt keinen veränderten Klartext heraus.
- **SC-003**: Ein verschlüsselter Ordner mit 1 000 Einträgen ist auf einem Gerät, das ihn zum ersten
  Mal öffnet, bei guter Verbindung in unter 5 Sekunden vollständig sichtbar, beim nächsten Öffnen in
  unter 1 Sekunde; einer mit 10 000 Einträgen in unter 30 Sekunden. Ein gewöhnlicher Ordner mit 20
  verschlüsselten Ordnern zeigt deren Namen bei guter Verbindung in unter 2 Sekunden.
- **SC-004**: Ein 4-GB-Video in einem verschlüsselten Ordner beginnt bei guter Verbindung in unter
  5 Sekunden zu spielen, ein Sprung an eine beliebige Stelle ist in unter 5 Sekunden sichtbar, und der
  Speicherbedarf von holzi wächst dabei um weniger als 100 MB.
- **SC-005**: Hoch- und Herunterladen in einen verschlüsselten Ordner sind höchstens 20 % langsamer als
  in einen gewöhnlichen Ordner desselben Speichers; das Volumen beim Anbieter ist höchstens 1 % größer
  als der Klartext (Dateien ab 1 MB).
- **SC-006**: Umbenennen eines Unterordners mit 1 000 Dateien schreibt 0 Inhaltsobjekte und ist bei
  guter Verbindung in unter 30 Sekunden fertig.
- **SC-007**: Ein zweites Gerät derselben Vault öffnet einen verschlüsselten Ordner in 100 % der Fälle
  ohne jede Eingabe.
- **SC-008**: In 100 % der Fälle eines Prüfkatalogs (Auflisten, Lesen, Suchen über den ganzen
  Speicher, direkte Pfade, rohe Objektnamen) erfährt ein Agent ohne Freigabe nichts aus einem
  verschlüsselten Ordner außer seinem Dasein und Zustand, und eine Erweiterung ohne Freigabe nichts außer der
  Ablehnung; eine Erweiterung mit Freigabe im Manifest lässt sich in 100 % der Fälle nicht
  installieren.
- **SC-009**: Nach dem Schließen der Vault liegt in 100 % der geprüften Fälle (Viewer, Vorschaubilder,
  Transfer, „Mit System-App öffnen“, Logs) kein Klartext aus verschlüsselten Ordnern außerhalb der
  verschlüsselten Daten der Vault auf dem Gerät.
- **SC-010**: Die Testvektoren aus FR-042 laufen in der CI bei jedem PR auf allen Plattformen von holzi
  durch, auch auf Android.

## Assumptions

- Jedes eigene Gerät hat den Inhaltsschlüssel der Vault mit allen Generationen (Spec 024, D22, D30);
  das Identitätsgeheimnis der Vault liegt dagegen nur auf Hauptgeräten (D27) und taugt deshalb nicht
  (research R1). Ein Backup der Vault enthält die Inhaltsschlüssel.
- Der Anbieter darf sehen: dass es einen verschlüsselten Ordner gibt und in welchem gewöhnlichen
  Ordner er liegt, die Zahl der Objekte, wann sie entstanden sind und wann auf sie zugegriffen wird.
  Daraus folgen auch: die genaue Größe jeder Datei (Länge des Inhaltsobjekts), die ungefähre Länge
  der Namen (Länge von Kopf und Begleitdateien), die Zahl der Dateien und Unterordner (Begleitdateien
  ohne Inhaltsobjekt), welche Begleitdatei zu welchem Inhaltsobjekt gehört (zeitliche Nähe beim
  Schreiben) und dass zwei Inhaltsobjekte Kopien voneinander sind. Das ist dieselbe Grenze wie bei
  Cryptomator, ohne dessen sichtbaren Ordnernamen; Auffüllen auf feste Größen ist nicht vorgesehen.
- Ein Anbieter, der ein altes Objekt wiederherstellt (Zurückspielen einer früheren Fassung), wird
  nicht in jedem Fall erkannt; holzi erkennt Veränderung und Vertauschen, nicht jedes Zurückspielen
  einer ganzen früheren Fassung. Ebenso wenig erkennt holzi, wenn der Anbieter ganze Einträge
  (Begleitdatei samt Inhaltsobjekt) entfernt; es gibt kein signiertes Verzeichnis des Ordners.
- Verfahren und Blockgröße legt der Plan fest (research R2, R4: XChaCha20-Poly1305, Blöcke zu
  64 KiB); das Modell kommt aus haex-vault, das Format nicht (research R3).
- Ein Speicher hat keine Benachrichtigung über Änderungen; Änderungen anderer Geräte erscheinen wie in
  044 beim Neuladen oder Zurückkehren.

## Nicht im Umfang

- Teilen eines verschlüsselten Ordners mit anderen Nutzern und Spaces (Umbau 027/029); nur das Format
  muss es zulassen (FR-040).
- Entzug und Erneuern von Schlüsseln (etwa nach einem verlorenen Gerät) und neues Verschlüsseln eines
  Ordners.
- Umwandeln bestehender Ordner in verschlüsselte und zurück (FR-004).
- Verschlüsselte Ordner auf dem Gerät selbst oder auf anderen Quellen als Speichern aus 038.
- Verschlüsselte Bereiche von Erweiterungen (ihr eigener Bereich bleibt, wie sie ihn schreiben).
- Ein Passwort je Ordner oder eine Wiederherstellung ohne Vault.
- Die Sync-Regeln, die verschlüsselte Ordner als Ziel nutzen werden.
- Auffüllen von Objekten auf feste Größen, um Dateigrößen zu verbergen.
