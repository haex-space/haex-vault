# Feature Specification: Dateizugriff zwischen eigenen Geräten und Dateifreigaben an Kontakte

**Feature Branch**: `056-file-access-sharing`

**Created**: 2026-10-21

**Status**: Draft

**Input**: User description: "Spec 056: Dateizugriff zwischen eigenen Geräten und Freigaben an
Kontakte. Grundlage ist das abgestimmte Design docs/plans/2026-10-21-file-access-and-sharing-design.md
(alle Entscheidungen dort sind vom Operator bestätigt). Ersetzt die Specs 027 (Spaces) und 029
(eigener S3 pro Space), die gelöscht werden."

## Begriffe

- **Vault-Identität**, **Geräteschlüssel**, **Geräteliste**, **Gerät der Vault**, **Präsenzmeldung**:
  wie in Spec 024. Ein Gerät gehört zu einer Vault, solange es auf deren aktueller Geräteliste steht.
- **Kontakt**, **Visitenkarte**, **holzi-Adressen**: wie in Spec 051. **Bestätigt** sind die
  holzi-Adressen eines Kontakts, wenn sie aus einer gültigen Visitenkarte stammen.
- **Dateibrowser**, **Quelle**, **Viewer**, **Transfer**, **Agent**, **Agent-Berechtigung**,
  **Freigabestufe**, **eigene Daten von holzi**: wie in Spec 044. **Speicher**: wie in den Specs 038
  und 050. **Verschlüsselter Ordner**: wie in Spec 048.
- **Bedienendes Gerät**: das Gerät, dessen Dateien ein anderes Gerät gerade liest oder ändert. Nur
  dieses Gerät beantwortet Anfragen zu seinen Dateien; es prüft jede Anfrage selbst.
- **Anfragendes Gerät**: das Gerät, das auf Dateien eines bedienenden Geräts zugreift, ein eigenes
  oder eines einer fremden Vault.
- **Fremde Vault**: eine Vault, deren Vault-Identität ein Kontakt mit bestätigten holzi-Adressen
  trägt und nicht die eigene ist.
- **Geräte-Dateizugriff**: der Zugriff eines eigenen Geräts auf das ganze Dateisystem eines anderen
  eigenen Geräts, lesend und schreibend. Er braucht keine Dateifreigabe.
- **Schalter „Geräte-Dateizugriff“**: eine Einstellung je Gerät, die nur auf diesem Gerät gilt und
  nicht synchronisiert wird. Ist sie aus, bedient das Gerät keine anderen eigenen Geräte.
- **Dateifreigabe**: ein Ordner oder eine einzelne Datei, die der Nutzer auf einem seiner Geräte
  Kontakten zugänglich macht. Sie hat einen Namen, eine **Quelle** (Gerät oder Speicher und Pfad), ein
  bedienendes Gerät und **Zugriffsrechte**.
- **Wurzel**: der freigegebene Ordner oder die freigegebene Datei selbst. Eine fremde Vault sieht
  nichts oberhalb oder neben ihr.
- **Zugriffsrecht**: die Zuordnung einer Vault-Identität (eines Kontakts) zu einer Dateifreigabe mit
  der Stufe **Lesen** oder **Lesen und Schreiben**. „Schreiben“ heißt in einem Ordner: anlegen,
  ändern, umbenennen, verschieben und löschen unterhalb der Wurzel; bei einer einzelnen Datei nur ihren
  Inhalt ändern.
- **Entziehen**: ein Zugriffsrecht entfernen. Es verteilt keine Schlüssel neu; das bedienende Gerät
  beantwortet die Anfragen dieser Vault-Identität für diese Dateifreigabe nicht mehr.
- **Empfangene Dateifreigabe**: was die Vault eines Empfängers über eine Dateifreigabe eines Kontakts
  weiß: Kontakt, Name, Stufe und bedienendes Gerät.
- **Anklopfen**: eine kurzlebige, verschlüsselte Nachricht eines anfragenden Geräts über die
  Nostr-Relays aus der Visitenkarte eines Kontakts, mit der es die aktuelle Adresse eines Geräts
  dieses Kontakts erfragt. Ein Gerät antwortet nur, wenn es dem anfragenden Gerät etwas zugänglich
  macht.
- **Versionsstand**: die Kennung des Stands einer Datei (etwa aus Größe und Änderungszeit), so wie das
  anfragende Gerät sie zuletzt gesehen hat. Ein Schreibvorgang auf eine vorhandene Datei nennt ihn; stimmt er nicht mehr, ist das
  ein **Schreibkonflikt**.

_Nicht verwechseln_: Eine **Dateifreigabe** ist keine **Freigabe** im Sinne von Spec 034 (Grant für
Aufrufer des Passwortmanagers), keine **Freigabe-URL** und keine **Freigabestufe** aus Spec 044, und
ein **Zugriffsrecht** ist keine **Berechtigung** einer Erweiterung (Spec 017) und keine
**Agent-Berechtigung**.

## Beziehung zu bestehenden Specs

- **Design**:
  [`docs/plans/2026-10-21-file-access-and-sharing-design.md`](../../docs/plans/2026-10-21-file-access-and-sharing-design.md)
  hält die Entscheidungen, den Datenfluss und den Testansatz fest.
- **Specs 027 (Spaces) und 029 (eigener S3-Speicher für Spaces)**: entfallen. Diese Spec ersetzt das
  Teilen von Dateien über Spaces vollständig. Beide Specs werden gelöscht; Verweise in anderen
  Dokumenten zeigen auf diese Spec.
- **Spec 025** (Dateisync zwischen eigenen Geräten): bleibt Entwurf für die späteren Sync-Regeln
  (Spiegeln, Cloud-Sicherung). Diese Spec spiegelt nichts; sie gibt Live-Zugriff.
- **Spec 024** (Sync der eigenen Geräte): liefert Vault-Identität, Geräteschlüssel, Geräteliste,
  Präsenz und die direkte, verschlüsselte Verbindung zwischen Geräten. Dateifreigaben und empfangene
  Dateifreigaben sind gewöhnliche Vault-Daten und kommen mit dem Datensync auf alle eigenen Geräte.
- **Spec 051** (Adressbuch): liefert die Kontakte mit bestätigten holzi-Adressen (Vault-Identität,
  Geräte, Nostr-Relays). Zugriffsrechte gibt es nur für solche Kontakte.
- **Spec 044** (Dateibrowser): Eigene Geräte und empfangene Dateifreigaben erscheinen dort als weitere
  Quellen. Viewer, Transfers, Namenskonflikte, Abbruch beim Sperren und die Regeln für Agents gelten
  für sie wie für einen Speicher. Ein eigenes Gerät sieht die eigenen Daten von holzi des bedienenden
  Geräts wie dessen Dateibrowser: nur lesend (044 FR-037), für Agents gesperrt (044 FR-033); für
  Kontakte bleiben sie gesperrt. Ein geöffneter Ordner auf dem Gerät aktualisiert sich live wie in
  044 FR-006.
- **Specs 038, 050 und 048**: Eine Dateifreigabe darf jede Quelle haben, die der Dateibrowser zeigt,
  auch einen Speicher oder einen verschlüsselten Ordner. Zugangsdaten und Schlüssel verlassen das
  bedienende Gerät dabei nie. Speicher selbst kommen nach Spec 038 ohnehin auf alle eigenen Geräte;
  der Geräte-Dateizugriff leitet sie deshalb nicht über ein anderes Gerät.
- **Spec 026** (Blind Relay): wird für das Teilen von Dateien nicht gebraucht. Ob es aus anderen
  Gründen bleibt, entscheidet diese Spec nicht.
- **Spec 028** (Datenfreigaben von Erweiterungen): bleibt unberührt.
- **Spec 017** (Erweiterungs-Host): Erweiterungen bekommen über diese Spec keinen Zugriff auf eigene
  Geräte oder empfangene Dateifreigaben.

## Clarifications

### Session 2026-10-21 (Brainstorming mit dem Betreiber)

- Q: Braucht holzi Spaces? → A: Nein. Ordner und Dateien werden einzelnen Kontakten freigegeben, an
  mehrere Kontakte zugleich und für jeden einzeln entziehbar, ohne Schlüssel neu zu verteilen.
- Q: Muss eine Datei ankommen, wenn der Eigentümer offline ist? → A: Nein. Zugriff nur, solange beide
  Vaults geöffnet sind.
- Q: Gespiegelte Kopie oder Live-Zugriff beim Empfänger? → A: Live-Zugriff wie ein Netzlaufwerk.
- Q: Welche Stufen? → A: Lesen, oder Lesen und Schreiben (Schreiben schließt Löschen ein).
- Q: Welches eigene Gerät bedient eine Dateifreigabe? → A: Genau das Gerät, auf dem die Quelle liegt.
  Die Dateifreigabe selbst kommt auf alle eigenen Geräte und lässt sich dort verwalten.
- Q: Wie erfährt der Empfänger davon? → A: Durch Abfrage beim Kontakt, keine Ankündigung.
- Q: Was darf freigegeben werden? → A: Alles, was der Dateibrowser zeigt.
- Q: Eigene Geräte? → A: Jedes eigene Gerät liest und schreibt das ganze Dateisystem jedes anderen;
  pro Gerät abschaltbar, standardmäßig an. Fremde Vaults nur ihre Wurzeln mit ihrer Stufe.
- Q: Wie findet ein Kontakt das bedienende Gerät? → A: Durch Anklopfen über Nostr; nur wer etwas
  freigegeben bekommen hat, erfährt die Adresse.

### Session 2026-10-22

- Q: Sieht ein eigenes Gerät auch die eigenen Daten von holzi des anderen Geräts? → A: Ja, alles, was
  der Dateibrowser des anderen Geräts selbst zeigt, mit denselben Regeln (lesen ja, ändern nein, für
  Agents gesperrt). Für Kontakte bleiben sie gesperrt.
- Q: Dürfen Agents Dateifreigaben verwalten? → A: Ja, wenn der Nutzer es dem Agent erlaubt (eigene
  Agent-Berechtigung, Standard aus). Freigeben und Rechte erweitern bestätigt der Nutzer jedes Mal in
  einem Dialog, unabhängig von der Freigabestufe; Entziehen und Herabstufen ohne Rückfrage.
- Q: Aktualisieren sich Ordner anderer Geräte live? → A: Ja. Ändert sich ein geöffneter Ordner auf dem
  bedienenden Gerät, sieht das anfragende Gerät es sofort, wie lokal (044 FR-006).

## User Scenarios & Testing _(mandatory)_

### User Story 1 - Auf die Dateien eines anderen eigenen Geräts zugreifen (Priority: P1)

Anna hat ihre Vault auf dem Laptop und dem Telefon. Unterwegs öffnet sie auf dem Telefon den
Dateibrowser. Unter „Meine Geräte“ steht „Laptop“ mit dem Status „erreichbar“. Sie öffnet darin
`Dokumente`, sieht sich ein PDF an, spielt ein Video ab, das sofort startet, und legt ein Foto vom
Telefon in einen Ordner auf dem Laptop. Später benennt sie auf dem Laptop eine Datei vom Telefon aus
um und löscht eine alte.

**Why this priority**: Der Zugriff auf die eigenen Geräte ist der häufigste Fall, braucht keine
anderen Personen und legt die Verbindung, die Prüfung jeder Anfrage und die Anbindung an den
Dateibrowser, auf denen alles Weitere aufbaut.

**Independent Test**: Zwei Geräte derselben Vault, beide mit geöffneter Vault. Auf Gerät A Ordner
auflisten, eine Datei im Viewer öffnen, eine Datei hochladen, umbenennen und löschen. Danach auf
Gerät B prüfen, dass alles angekommen ist.

**Acceptance Scenarios**:

1. **Given** zwei eigene Geräte mit geöffneter Vault, **When** Anna auf dem Telefon den Dateibrowser
   öffnet, **Then** zeigt „Meine Geräte“ den Laptop mit Namen und dem Status „erreichbar“.
2. **Given** der Laptop ist erreichbar, **When** Anna ihn öffnet, **Then** sieht sie dasselbe
   Dateisystem wie der Dateibrowser auf dem Laptop selbst, mit Laufwerken und bekannten Orten.
3. **Given** ein Video auf dem Laptop, **When** Anna es auf dem Telefon öffnet, **Then** beginnt die
   Wiedergabe, bevor die Datei ganz übertragen ist, und Springen im Video funktioniert.
4. **Given** ein Ordner auf dem Laptop, **When** Anna ein Foto vom Telefon hineinkopiert, **Then**
   liegt es danach vollständig auf dem Laptop; bricht sie ab, liegt dort keine halbe Datei.
5. **Given** eine Datei auf dem Laptop, **When** Anna sie vom Telefon aus löscht, **Then** landet sie
   im Papierkorb des Laptops.
6. **Given** der Laptop ist aus, **When** Anna „Meine Geräte“ öffnet, **Then** steht der Laptop dort
   als „nicht erreichbar“ und lässt sich nicht öffnen.
7. **Given** der Laptop, **When** Anna auf dem Telefon das Datenverzeichnis von holzi auf dem Laptop
   öffnet, **Then** sieht sie es wie auf dem Laptop selbst, nur lesend.
8. **Given** Anna hat auf dem Telefon `Downloads` des Laptops offen, **When** auf dem Laptop eine Datei
   dort ankommt, **Then** erscheint sie auf dem Telefon ohne Aktualisieren.

---

### User Story 2 - Einen Ordner an einen Kontakt freigeben und dort öffnen (Priority: P1)

Anna möchte Ben die Urlaubsfotos zeigen. Ben steht in ihrem Adressbuch, seine Visitenkarte hat sie
übernommen. Im Dateibrowser auf ihrem Laptop wählt sie beim Ordner `Fotos/Urlaub 2026` „Freigeben …“,
wählt Ben aus und lässt die Stufe auf „Lesen“. Ben öffnet bei sich den Dateibrowser, klappt unter „Von
Kontakten“ Anna auf und sieht „Urlaub 2026“. Er blättert durch die Fotos und kopiert zwei davon in
einen eigenen Ordner.

**Why this priority**: Das ist der Kern des Teilens mit anderen Menschen, und Lesen ist die häufigste
und ungefährlichste Stufe.

**Independent Test**: Zwei Vaults, die sich gegenseitig als Kontakt mit bestätigten holzi-Adressen
führen. In Vault A einen Ordner für Vault B mit „Lesen“ freigeben; in Vault B den Kontakt aufklappen,
die Dateifreigabe öffnen, eine Datei ansehen und herauskopieren; prüfen, dass Vault B nichts außerhalb
der Wurzel sieht und nichts schreiben kann.

**Acceptance Scenarios**:

1. **Given** ein Ordner auf dem Laptop, **When** Anna „Freigeben …“ wählt, **Then** zeigt der Dialog
   nur Kontakte mit bestätigten holzi-Adressen zur Auswahl, mit der Stufe „Lesen“ als Vorgabe und dem
   Ordnernamen als Vorgabe für den Namen der Dateifreigabe.
2. **Given** Annas Dateifreigabe für Ben, **When** Ben bei sich unter „Von Kontakten“ Anna aufklappt,
   während Annas Laptop die Vault offen hat, **Then** erscheint „Urlaub 2026“ als Quelle mit der
   Angabe „Lesen“.
3. **Given** Ben hat „Urlaub 2026“ geöffnet, **When** er nach oben navigieren will, **Then** endet die
   Pfadleiste an der Wurzel der Dateifreigabe; Ordner darüber oder daneben sind für ihn nicht sichtbar.
4. **Given** Stufe „Lesen“, **When** Ben die Dateifreigabe ansieht, **Then** bietet holzi keine
   Aktion zum Anlegen, Ändern, Umbenennen, Verschieben oder Löschen an, und das bedienende Gerät
   lehnt solche Anfragen ab, auch wenn sie trotzdem kommen.
5. **Given** Ben hat die Dateifreigabe einmal gesehen, **When** Annas Laptop aus ist, **Then** steht
   „Urlaub 2026“ weiter unter Anna, ausgegraut mit „nicht erreichbar“.
6. **Given** Ben hat zwei Geräte, **When** er die Dateifreigabe auf dem zweiten Gerät öffnet, **Then**
   funktioniert das ohne neue Freigabe durch Anna.
7. **Given** Anna gibt einen Ordner in einem S3-Speicher oder einen verschlüsselten Ordner frei,
   **When** Ben ihn öffnet, **Then** sieht er die Dateien im Klartext; Zugangsdaten und Schlüssel
   erreichen ihn nicht.
8. **Given** Anna gibt eine einzelne Datei frei, **When** Ben die Dateifreigabe öffnet, **Then** sieht
   er genau diese Datei.

---

### User Story 3 - Gemeinsam an einem Ordner arbeiten (Priority: P2)

Anna gibt Ben und Clara den Ordner `Verein/Protokolle` mit „Lesen und Schreiben“ frei. Ben legt ein
neues Protokoll an, Clara korrigiert eines. Als Ben und Anna gleichzeitig dieselbe Datei speichern,
fragt holzi Ben, ob er überschreiben oder seine Fassung als Kopie speichern will.

**Why this priority**: Schreiben macht die Dateifreigabe zur Zusammenarbeit, setzt aber Lesen voraus
und bringt Konflikte, die erst danach sinnvoll prüfbar sind.

**Independent Test**: Dateifreigabe mit „Lesen und Schreiben“ an zwei Kontakte; beide legen an,
ändern, benennen um und löschen; eine Datei wird beim Eigentümer geändert, bevor ein Kontakt seine
Änderung speichert.

**Acceptance Scenarios**:

1. **Given** Stufe „Lesen und Schreiben“, **When** Ben eine Datei anlegt, umbenennt, verschiebt oder
   löscht, **Then** wirkt das sofort auf Annas Gerät, und Clara sieht es beim nächsten Auflisten.
2. **Given** eine Datei, die Anna nach Bens letztem Öffnen geändert hat, **When** Ben seine Fassung
   speichert, **Then** meldet holzi einen Schreibkonflikt und Ben wählt „Ersetzen“ oder „Beide
   behalten“; ohne Wahl ändert sich nichts.
3. **Given** Ben löscht eine Datei, **When** Annas Laptop sie entfernt, **Then** liegt sie auf dem
   Desktop im Papierkorb des Laptops.
4. **Given** eine Dateifreigabe einer einzelnen Datei mit „Lesen und Schreiben“, **When** Ben sie
   umbenennen oder löschen will, **Then** lehnt holzi das ab; ändern kann er nur den Inhalt.
5. **Given** die Wurzel eines Ordners, **When** Ben sie umbenennen, verschieben oder löschen will,
   **Then** lehnt holzi das ab.
6. **Given** das bedienende Gerät hat keinen Papierkorb, **When** Ben eine Datei löscht, **Then** fragt
   holzi vor dem Löschen nach, und ohne Bestätigung bleibt die Datei unverändert.

---

### User Story 4 - Dateifreigaben verwalten und entziehen (Priority: P2)

Clara ist aus dem Verein ausgetreten. Anna öffnet auf dem Telefon „Meine Dateifreigaben“, sieht
„Protokolle“ mit Ben und Clara und entzieht Clara das Zugriffsrecht. Ben stuft sie auf „Lesen“
herunter. Weil der Laptop gerade aus ist, zeigt holzi „Entzug wartet auf Laptop“, bis der Laptop die
Änderung erhalten hat.

**Why this priority**: Entziehen ist das Versprechen dieser Spec gegenüber Spaces; ohne verlässlichen
Entzug ist Teilen nicht vertretbar. Es setzt Story 2 voraus.

**Independent Test**: Ein Zugriffsrecht entziehen, während der Kontakt gerade eine große Datei liest;
prüfen, dass der Transfer abbricht und keine weitere Anfrage beantwortet wird. Danach dasselbe vom
anderen eigenen Gerät aus, während das bedienende Gerät aus ist.

**Acceptance Scenarios**:

1. **Given** Dateifreigaben auf mehreren Geräten, **When** Anna „Meine Dateifreigaben“ öffnet,
   **Then** sieht sie jede Dateifreigabe mit Quelle, bedienendem Gerät, Kontakten, Stufen und
   Zustand.
2. **Given** Clara liest gerade eine Datei, **When** Anna auf dem bedienenden Gerät Claras
   Zugriffsrecht entzieht, **Then** bricht Claras Transfer ab, und das bedienende Gerät beantwortet
   keine weitere Anfrage von Clara zu dieser Dateifreigabe.
3. **Given** das bedienende Gerät ist aus, **When** Anna auf einem anderen Gerät entzieht, **Then**
   zeigt holzi „Entzug wartet auf ‹Gerät›“, bis das bedienende Gerät die Änderung erhalten hat.
4. **Given** Ben hat „Lesen und Schreiben“, **When** Anna ihn auf „Lesen“ stellt, **Then** lehnt das
   bedienende Gerät ab der nächsten Anfrage jede Änderung durch Ben ab.
5. **Given** eine Dateifreigabe, **When** Anna ihren Quellordner auf dem Gerät umbenennt oder löscht,
   **Then** zeigt „Meine Dateifreigaben“ den Zustand „Quelle fehlt“, und Kontakte erhalten „nicht
   verfügbar“.
6. **Given** der Kontakt Ben, **When** Anna seine Kontaktseite öffnet, **Then** sieht sie dort die
   Dateifreigaben, die Ben von ihr bekommen hat, und kann sie dort ändern oder entziehen.

---

### User Story 5 - Ein Gerät vom Geräte-Dateizugriff ausnehmen (Priority: P3)

Annas Arbeitslaptop hat ebenfalls ihre Vault. Sie will nicht, dass ihr Telefon die Arbeitsdateien
erreicht, und schaltet auf dem Arbeitslaptop den Geräte-Dateizugriff aus.

**Why this priority**: Begrenzt das Risiko des vollen Zugriffs zwischen eigenen Geräten; der Normalfall
kommt ohne aus.

**Independent Test**: Schalter auf Gerät A ausschalten; von Gerät B aus Gerät A öffnen; Dateifreigaben
an Kontakte auf Gerät A weiter prüfen.

**Acceptance Scenarios**:

1. **Given** ein neues Gerät der Vault, **When** Anna seine Einstellungen öffnet, **Then** ist der
   Geräte-Dateizugriff eingeschaltet.
2. **Given** der Schalter ist auf dem Arbeitslaptop aus, **When** Anna auf dem Telefon den
   Arbeitslaptop öffnen will, **Then** zeigt holzi „Dateizugriff auf diesem Gerät ausgeschaltet“ und
   keine Datei.
3. **Given** der Schalter ist aus, **When** ein Kontakt eine Dateifreigabe dieses Geräts öffnet,
   **Then** funktioniert sie weiter; der Schalter betrifft nur eigene Geräte.
4. **Given** der Schalter ist auf dem Arbeitslaptop aus, **When** Anna die Einstellungen auf einem
   anderen Gerät ansieht, **Then** ändert sich dort nichts; der Schalter gilt nur für das Gerät, auf
   dem er steht.

---

### User Story 6 - Agents arbeiten mit Dateien anderer Geräte und Dateifreigaben (Priority: P3)

Anna bittet den Chat-Agent auf dem Telefon, im Ordner `Projekte` auf dem Laptop die neueste
Präsentation zu finden und zusammenzufassen, und später, Notizen in Bens Dateifreigabe „Rezepte“
abzulegen.

**Why this priority**: Nützlich, aber nur eine Erweiterung der Agent-Regeln aus Spec 044 auf neue
Quellen.

**Independent Test**: Einem Agent die Berechtigung für das Gerät „Laptop“ und für eine empfangene
Dateifreigabe geben bzw. verweigern; Lesen und Schreiben über Agent-Aktionen prüfen.

**Acceptance Scenarios**:

1. **Given** ein Agent ohne Berechtigung für „Laptop“, **When** er dort lesen will, **Then** lehnt
   holzi ab.
2. **Given** ein Agent mit „Lesen“ für eine empfangene Dateifreigabe, **When** er dort schreiben will,
   **Then** lehnt holzi ab, auch wenn das Zugriffsrecht des Kontakts „Lesen und Schreiben“ wäre.
3. **Given** ein Agent mit „Lesen und Schreiben“, **When** er eine Datei ändert, **Then** gilt die
   Freigabestufe wie bei jeder anderen Quelle.
4. **Given** ein Agent ohne die Berechtigung „Dateifreigaben verwalten“, **When** Anna ihn bittet, einen
   Ordner an Ben freizugeben, **Then** lehnt holzi ab und nennt die fehlende Berechtigung.
5. **Given** ein Agent mit dieser Berechtigung, **When** er einen Ordner an Ben freigeben will, **Then**
   zeigt holzi einen Dialog mit Ordner, Gerät, Kontakt und Stufe, und erst Annas Bestätigung legt die
   Dateifreigabe an, auch im Modus „Auto“.
6. **Given** ein Agent mit dieser Berechtigung, **When** er Ben ein Zugriffsrecht entzieht, **Then**
   geschieht das ohne Rückfrage.

---

### Edge Cases

- **Bedienendes Gerät wird mitten im Transfer gesperrt, beendet oder verliert die Verbindung**: Der
  Transfer bricht ab, ohne halbe Datei am Ziel; das anfragende Gerät meldet „nicht erreichbar“.
- **Anfragendes Gerät wird gesperrt**: Laufende Transfers brechen ab wie in Spec 044 FR-026.
- **Pfad führt aus der Wurzel hinaus** (`..`, symbolischer Link, Verknüpfung, Groß-/Kleinschreibung
  auf Dateisystemen ohne Unterscheidung): Das bedienende Gerät lehnt ab, beim Lesen, Schreiben,
  Umbenennen und Verschieben gleichermaßen. Das gilt auch für Ziele von Verschiebungen.
- **Verschieben zwischen zwei Dateifreigaben oder aus einer Dateifreigabe auf ein eigenes Gerät**:
  Das ist ein Transfer aus Kopieren und Löschen wie zwischen Quellen in Spec 044; die Stufe beider
  Seiten gilt.
- **Kontakt wird im Adressbuch gelöscht oder verliert seine bestätigten holzi-Adressen**: Seine
  Zugriffsrechte fallen weg; der Dialog zum Löschen nennt die betroffenen Dateifreigaben vorher.
- **Kontakt legt eine ältere Geräteliste vor als zuletzt gesehen**: Das bedienende Gerät lehnt ab.
  Ein vom Kontakt entferntes Gerät verliert so den Zugriff, sobald eine neuere Liste gesehen wurde.
- **Kontakt hat ein neues Gerät**: Erst nach einer aktualisierten Visitenkarte kann das Gerät über
  deren Geräteschlüssel anklopfen; danach legt es die aktuelle Geräteliste seiner Vault vor und
  bekommt Zugriff, ohne dass der Eigentümer eine neue Freigabe anlegt.
- **Anklopfen ohne Zugriffsrecht oder von Unbekannten**: Das Gerät antwortet nicht und verrät nicht,
  ob es etwas freigibt.
- **Entzug von einem anderen Gerät, während das bedienende Gerät aus ist**: Der Entzug wirkt, sobald
  das bedienende Gerät ihn per Datensync erhalten hat. Bis dahin zeigt holzi „Entzug wartet auf
  ‹Gerät›“. Startet das bedienende Gerät und beantwortet eine Anfrage, bevor der Datensync gelaufen
  ist, kann der Kontakt in diesem Fenster noch zugreifen; das ist eine bekannte Grenze.
- **Quelle der Dateifreigabe nicht verfügbar** (Speicher gestört, verschlüsselter Ordner nicht
  lesbar, Laufwerk ausgehängt): Kontakte erhalten „beim Eigentümer nicht verfügbar“ ohne interne
  Angaben; der Eigentümer sieht den Grund in „Meine Dateifreigaben“.
- **Dieselbe Quelle zweimal freigeben**: holzi öffnet die vorhandene Dateifreigabe zum Bearbeiten statt
  eine zweite anzulegen.
- **Verschachtelte Dateifreigaben** (ein Ordner an Ben, sein Unterordner an Clara): erlaubt; jede ist
  eine eigene Wurzel mit eigenen Zugriffsrechten. Hat ein Kontakt beide, sieht er beide getrennt.
- **Dateifreigabe der eigenen Daten von holzi oder eines Ordners darüber**: Die eigenen Daten von holzi
  bleiben auch innerhalb einer Dateifreigabe für Kontakte gesperrt.
- **Agent liest eine präparierte Datei, die zum Freigeben auffordert**: Ohne Bestätigung des Nutzers
  entsteht keine Dateifreigabe und kein erweitertes Zugriffsrecht (FR-045).
- **Beide Geräte im Ruhezustand oder Android im Hintergrund**: Zugriff nur, solange auf beiden
  Geräten holzi läuft und die Vault geöffnet ist; ein Hintergrunddienst ist nicht Teil dieser Spec.
- **Gleichzeitiger Zugriff vieler Kontakte**: Das bedienende Gerät bedient alle; die eigene Bedienung
  von holzi auf diesem Gerät bleibt bedienbar (SC-006).

## Requirements _(mandatory)_

### Functional Requirements

**Verbindung und Prüfung**

- **FR-001**: Geräte MÜSSEN Dateien über die direkte, verschlüsselte Verbindung zwischen Geräten aus
  Spec 024 austauschen, ohne dass diese Funktion eine zusätzliche Kopie der Dateiinhalte oder
  Freigabe-Chiffretexte bei Dritten speichert. Ist die Quelle selbst ein S3-, WebDAV- oder
  SFTP-Speicher, bleiben dessen Quelldaten dort liegen.
- **FR-002**: Ein anfragendes Gerät MUSS sich beim Verbinden mit seinem Geräteschlüssel ausweisen und
  die mit der Vault-Identität unterschriebene Geräteliste seiner Vault vorlegen. Das bedienende Gerät
  MUSS die Unterschrift prüfen und dass das anfragende Gerät auf dieser Liste steht.
- **FR-003**: Das bedienende Gerät MUSS eine Geräteliste ablehnen, deren Generation kleiner ist als die
  höchste, die es von derselben Vault-Identität kennt.
- **FR-004**: Das bedienende Gerät MUSS jede Verbindung einordnen: eigene Vault mit eingeschaltetem
  Schalter „Geräte-Dateizugriff“ → Geräte-Dateizugriff; Vault-Identität eines Kontakts mit mindestens
  einem Zugriffsrecht auf eine Dateifreigabe dieses Geräts → Zugriff auf genau diese Wurzeln; sonst
  ablehnen, ohne preiszugeben, ob es Dateifreigaben gibt.
- **FR-005**: Das bedienende Gerät MUSS bei jeder einzelnen Anfrage erneut prüfen, ob das
  Zugriffsrecht besteht, welche Stufe es hat und ob der Pfad innerhalb der Wurzel liegt; eine Prüfung
  nur beim Verbinden reicht nicht.
- **FR-006**: Das bedienende Gerät MUSS jeden Pfad vor dem Zugriff auflösen und ablehnen, wenn er über
  `..`, symbolische Links, Verknüpfungen oder Schreibvarianten aus der Wurzel herausführt; das gilt
  auch für das Ziel von Umbenennen und Verschieben.
- **FR-007**: Die eigenen Daten von holzi des bedienenden Geräts MÜSSEN für Kontakte gesperrt sein,
  auch innerhalb einer Dateifreigabe. Ein eigenes Gerät MUSS sie sehen wie der Dateibrowser des
  bedienenden Geräts: nur lesend (Spec 044 FR-037), für Agents gesperrt (Spec 044 FR-033).
- **FR-008**: Um ein Gerät eines Kontakts zu erreichen, MUSS das anfragende Gerät anklopfen: eine
  kurzlebige, verschlüsselte und mit seinem Geräteschlüssel unterschriebene Nachricht an die
  Geräteschlüssel des Kontakts über die Nostr-Relays aus dessen Visitenkarte.
- **FR-009**: Ein Gerät DARF auf ein Anklopfen nur antworten, wenn es dem anfragenden Gerät nach FR-004
  etwas zugänglich macht; nur dann DARF es seine aktuelle Adresse nennen. Andernfalls MUSS es
  schweigen.
- **FR-010**: holzi DARF Adressen von Geräten nicht öffentlich verzeichnen lassen (keine öffentliche
  Adressauflösung); eigene Geräte finden sich weiter über die Präsenz aus Spec 024.

**Geräte-Dateizugriff (eigene Geräte)**

- **FR-011**: Der Dateibrowser MUSS jedes andere Gerät der eigenen Vault unter „Meine Geräte“ als
  Quelle zeigen, mit Gerätename und dem Zustand „erreichbar“, „nicht erreichbar“ oder
  „Dateizugriff auf diesem Gerät ausgeschaltet“.
- **FR-012**: Ein eigenes Gerät MUSS über den Geräte-Dateizugriff dasselbe sehen und dieselben
  Aktionen ausführen können wie der Dateibrowser auf dem bedienenden Gerät für dessen Gerät-Quelle
  (Spec 044 FR-002, FR-017 bis FR-024), einschließlich verschlüsselter Ordner auf dem Gerät
  (Spec 048), die das bedienende Gerät entschlüsselt anzeigt.
- **FR-013**: Speicher (Specs 038, 050) MUSS ein Gerät direkt erreichen, nicht über ein anderes eigenes
  Gerät.
- **FR-014**: Jedes Gerät MUSS einen Schalter „Geräte-Dateizugriff“ in seinen Einstellungen haben,
  standardmäßig an, der nur auf diesem Gerät gilt und nicht synchronisiert wird. Ist er aus, MUSS das
  Gerät Anfragen eigener Geräte ablehnen; Dateifreigaben an Kontakte bleiben davon unberührt.

**Dateifreigaben (Eigentümer)**

- **FR-015**: Der Nutzer MUSS im Dateibrowser jeden Ordner und jede Datei jeder Quelle des aktuellen
  Geräts über „Freigeben …“ freigeben können, auch in Speichern und verschlüsselten Ordnern. Quellen
  anderer Geräte (über den Geräte-Dateizugriff) DÜRFEN NICHT freigegeben werden; dafür gibt der
  Nutzer auf dem Gerät frei, auf dem die Quelle liegt. Anlegen DARF der Nutzer eine Dateifreigabe nur
  auf dem Gerät, auf dem die Quelle liegt; eine bestehende Dateifreigabe ändern, entziehen und löschen
  DARF er auf jedem eigenen Gerät (FR-019).
- **FR-016**: Der Dialog „Freigeben …“ MUSS zur Auswahl nur Kontakte mit bestätigten holzi-Adressen
  anbieten, je Kontakt die Stufe „Lesen“ (Vorgabe) oder „Lesen und Schreiben“, einen änderbaren Namen
  (Vorgabe: Name des Ordners bzw. der Datei) und die vorhandenen Zugriffsrechte zum Ändern und
  Entziehen.
- **FR-017**: Für dieselbe Quelle auf demselben Gerät DARF es nur eine Dateifreigabe geben; „Freigeben
  …“ auf einer schon freigegebenen Quelle MUSS die vorhandene öffnen.
- **FR-018**: Bedienendes Gerät einer Dateifreigabe MUSS das Gerät sein, auf dem sie angelegt wurde;
  nur dieses Gerät DARF Anfragen dazu beantworten.
- **FR-019**: Dateifreigaben und ihre Zugriffsrechte MÜSSEN mit dem Datensync (Spec 024) auf alle
  eigenen Geräte kommen und sich auf jedem eigenen Gerät ändern und entziehen lassen.
- **FR-020**: holzi MUSS eine Übersicht „Meine Dateifreigaben“ bieten mit Name, Quelle, bedienendem
  Gerät, Kontakten mit Stufe und Zustand: „aktiv“, „Quelle fehlt“, „Quelle nicht verfügbar“ oder
  „Änderung wartet auf ‹Gerät›“.
- **FR-021**: Die Kontaktseite (Spec 051) MUSS die Dateifreigaben an diesen Kontakt zeigen und Ändern
  und Entziehen dort erlauben.
- **FR-022**: Fällt die Quelle einer Dateifreigabe weg (umbenannt, verschoben, gelöscht), MUSS die
  Dateifreigabe mit „Quelle fehlt“ bestehen bleiben, bis der Nutzer sie löscht; holzi DARF sie nicht
  stillschweigend auf einen anderen Pfad umlenken.
- **FR-023**: Wird ein Kontakt gelöscht oder verliert er seine bestätigten holzi-Adressen, MÜSSEN seine
  Zugriffsrechte entfallen; vor dem Löschen MUSS holzi die betroffenen Dateifreigaben nennen.

**Entziehen und Ändern**

- **FR-024**: Entziehen oder Herabstufen MUSS auf dem bedienenden Gerät ab der nächsten Anfrage gelten;
  laufende Transfers des betroffenen Kontakts zu dieser Dateifreigabe MÜSSEN abbrechen, ohne halbe
  Datei.
- **FR-025**: Entziehen DARF keine Schlüssel neu verteilen und keine Mitwirkung anderer Kontakte
  verlangen.
- **FR-026**: Wurde eine Änderung an einem anderen Gerät vorgenommen, MUSS holzi bis zu ihrem Eintreffen
  auf dem bedienenden Gerät „Änderung wartet auf ‹Gerät›“ zeigen.
- **FR-027**: Was ein Kontakt vor dem Entziehen kopiert hat, behält er; holzi DARF das nicht
  verhindern oder zurückfordern.

**Empfangene Dateifreigaben (Empfänger)**

- **FR-028**: Der Dateibrowser MUSS unter „Von Kontakten“ die Kontakte mit bestätigten holzi-Adressen
  zeigen. Klappt der Nutzer einen Kontakt auf, MUSS holzi bei dessen Geräten anklopfen und die
  Dateifreigaben abfragen, die dieser für die eigene Vault hat.
- **FR-029**: holzi DARF nicht ohne Zutun des Nutzers bei Kontakten anklopfen, außer um eine schon
  bekannte empfangene Dateifreigabe zu öffnen, die der Nutzer aufruft.
- **FR-030**: Eine abgefragte Dateifreigabe MUSS als empfangene Dateifreigabe gespeichert werden und
  mit dem Datensync auf alle eigenen Geräte kommen; jedes eigene Gerät MUSS sie öffnen können.
- **FR-031**: Eine bekannte empfangene Dateifreigabe MUSS sichtbar bleiben, wenn der Kontakt nicht
  erreichbar ist, ausgegraut mit „nicht erreichbar“. Meldet das bedienende Gerät des Kontakts sie bei
  einer Abfrage nicht mehr, MUSS holzi sie entfernen; Freigaben anderer Geräte desselben Kontakts
  bleiben davon unberührt.
- **FR-032**: Eine empfangene Dateifreigabe MUSS sich im Dateibrowser wie eine Quelle verhalten:
  Auflisten, Viewer mit Wiedergabe vor vollständiger Übertragung und Springen (Spec 044 FR-012),
  Transfers mit Fortschritt und Abbruch, Kopieren heraus in eigene Quellen.
- **FR-033**: Die Pfadleiste MUSS an der Wurzel einer empfangenen Dateifreigabe enden.
- **FR-034**: Bei Stufe „Lesen“ DARF holzi keine schreibenden Aktionen anbieten.
- **FR-046**: Ist auf dem anfragenden Gerät ein Ordner eines anderen Geräts oder einer Dateifreigabe
  geöffnet, der auf dem bedienenden Gerät liegt, MUSS die Ansicht Änderungen dort ohne Zutun des
  Nutzers zeigen, wie Spec 044 FR-006 lokal; endet die Verbindung, zeigt sie „nicht erreichbar“.

**Schreiben und Konflikte**

- **FR-035**: Ein Schreibvorgang MUSS am Ziel vollständig oder gar nicht wirken; ein Abbruch DARF keine
  halbe Datei hinterlassen.
- **FR-036**: Ein Schreibvorgang auf eine vorhandene Datei MUSS den Versionsstand nennen. Weicht er ab,
  MUSS das bedienende Gerät mit „Schreibkonflikt“ ablehnen, und das anfragende Gerät MUSS „Ersetzen“ oder
  „Beide behalten“ anbieten (wie Spec 044 FR-021).
- **FR-037**: Die Wurzel einer Dateifreigabe DARF ein Kontakt weder umbenennen noch verschieben noch
  löschen; bei einer freigegebenen Datei DARF er nur den Inhalt ändern.
- **FR-038**: Löschen durch ein anderes Gerät MUSS auf einem Desktop-Gerät in dessen Papierkorb gehen,
  wo das Gerät einen hat (wie Spec 044 FR-023); sonst MUSS das anfragende Gerät vorher nachfragen.
- **FR-039**: Namenskonflikte am Ziel MUSS holzi behandeln wie in Spec 044 FR-021.

**Agents**

- **FR-040**: Agents DÜRFEN ein anderes eigenes Gerät oder eine empfangene Dateifreigabe nur mit einer
  Agent-Berechtigung für genau diese Quelle erreichen, „Lesen“ oder „Lesen und Schreiben“, wie für
  einen Speicher (Spec 044 FR-031a, FR-031b).
- **FR-041**: Wirksam ist für einen Agent die engere der beiden Stufen: seine Agent-Berechtigung und
  das Zugriffsrecht der eigenen Vault. Ändern läuft über die Freigabestufe (Spec 044 FR-032).
- **FR-045**: Ein Agent DARF Dateifreigaben nur mit der Agent-Berechtigung „Dateifreigaben verwalten“
  anlegen, ändern oder entziehen; sie ist je Agent, standardmäßig aus und nur vom Nutzer zu erteilen.
  Anlegen, einen Kontakt hinzufügen und eine Stufe erhöhen MUSS der Nutzer jedes Mal in einem Dialog
  mit Quelle, Gerät, Kontakt und Stufe bestätigen, unabhängig von der Freigabestufe und ohne
  „merken“. Entziehen, Herabstufen und Löschen DARF ein berechtigter Agent ohne Rückfrage. Für Agents
  gelten dieselben Grenzen wie für den Nutzer (FR-015, FR-016). Den Schalter „Geräte-Dateizugriff“
  DARF kein Agent umlegen.

**Fehler und Grenzen**

- **FR-042**: Ist das bedienende Gerät nicht erreichbar, gesperrt oder die Quelle dort nicht
  verfügbar, MUSS das anfragende Gerät das klar melden, ohne Pfade, Zugangsdaten oder interne
  Fehlertexte des bedienenden Geräts zu zeigen.
- **FR-043**: Zugangsdaten von Speichern und Schlüssel verschlüsselter Ordner DÜRFEN das bedienende
  Gerät nie verlassen.
- **FR-044**: Wird die Vault auf einem der beiden Geräte gesperrt, MÜSSEN laufende Transfers zwischen
  ihnen abbrechen, ohne halbe Datei (wie Spec 044 FR-026).

### Key Entities

- **Dateifreigabe**: Kennung, Name, Quelle (Gerät oder Speicher, Pfad; Art Ordner oder Datei),
  bedienendes Gerät, Zeitpunkt der Anlage; gehört der Vault, synchronisiert auf alle eigenen Geräte.
- **Zugriffsrecht**: Dateifreigabe, Vault-Identität des Kontakts, Stufe („Lesen“ oder „Lesen und
  Schreiben“); synchronisiert wie die Dateifreigabe.
- **Empfangene Dateifreigabe**: Kontakt (Vault-Identität), Kennung der Dateifreigabe beim Eigentümer,
  Name, Stufe, Art, bedienendes Gerät, zuletzt erfolgreich erreicht; synchronisiert auf alle eigenen
  Geräte.
- **Schalter „Geräte-Dateizugriff“**: ein Wert je Gerät, nur lokal.
- **Agent-Berechtigung „Dateifreigaben verwalten“**: je Agent erteilt oder nicht; synchronisiert wie
  die anderen Agent-Berechtigungen für Dateien (Spec 044).
- **Höchste gesehene Geräteliste**: je fremder Vault-Identität die höchste geprüfte Generation, für
  FR-003.

## Success Criteria _(mandatory)_

### Measurable Outcomes

- **SC-001**: Im selben Netz zeigt ein anderes eigenes Gerät oder eine Dateifreigabe einen Ordner mit
  1 000 Einträgen nach dem Öffnen in höchstens 2 Sekunden an.
- **SC-002**: Im selben Netz beginnt ein Video aus einer Dateifreigabe oder von einem anderen eigenen
  Gerät in höchstens 3 Sekunden zu spielen, unabhängig von der Dateigröße.
- **SC-003**: Ein Nutzer gibt einen Ordner in unter 30 Sekunden an einen Kontakt frei, gemessen vom
  Kontextmenü bis zur bestätigten Dateifreigabe.
- **SC-004**: Nach dem Entziehen auf dem bedienenden Gerät beantwortet es keine Anfrage des Kontakts zu
  dieser Dateifreigabe mehr, und laufende Transfers enden innerhalb von 2 Sekunden.
- **SC-005**: Jeder Fall aus einer Testsammlung von Pfadausbrüchen (`..`, symbolische Links,
  Verknüpfungen, Schreibvarianten, Ziele von Verschieben und Umbenennen) wird abgelehnt: 100 %.
- **SC-006**: Während fünf Kontakte gleichzeitig große Dateien aus Dateifreigaben eines Geräts lesen,
  öffnet holzi auf diesem Gerät einen lokalen Ordner mit 1 000 Einträgen und wechselt Tabs jeweils in
  höchstens 1 Sekunde.
- **SC-007**: Kein Geräte- oder Vault-Zugriff gelingt ohne gültige Geräteliste: Sobald das bedienende
  Gerät die neue Liste akzeptiert und verbreitet hat, erhält ein Gerät, das nicht auf der höchsten
  akzeptierten Liste seiner Vault steht, in 100 % der Versuche keine Datei. Bis diese Liste das
  bedienende Gerät erreicht, gilt die dokumentierte Übergangsfrist.
- **SC-008**: Ein abgebrochener oder gescheiterter Transfer hinterlässt in 100 % der Fälle keine halbe
  Datei am Ziel.
- **SC-009**: Im selben Netz erscheint eine neue, geänderte oder gelöschte Datei in einem geöffneten
  Ordner eines anderen Geräts oder einer Dateifreigabe auf dem Gerät des Betrachters in höchstens
  2 Sekunden.
- **SC-010**: Kein Agent legt eine Dateifreigabe an oder erweitert ein Zugriffsrecht ohne Bestätigung
  des Nutzers: 0 Fälle in der Testsammlung, auch im Modus „Auto“.

## Assumptions

- Beide Seiten haben holzi geöffnet und die Vault entsperrt; ein Hintergrunddienst kommt mit einer
  eigenen Spec.
- Kontakte entstehen wie in Spec 051 aus einer Visitenkarte; eine andere Art, eine fremde Vault
  auszuweisen, gibt es nicht.
- Die Geschwindigkeitsziele gelten im selben Netz; über das Internet hängen sie von der Verbindung ab.
- Live-Aktualisierung (FR-046) gilt für Ordner, die auf dem bedienenden Gerät liegen; Ordner in
  Speichern (S3, WebDAV, SFTP) aktualisieren sich wie lokal erst beim Auflisten oder Aktualisieren.
- Die Suche (Spec 044 FR-027) läuft auch in anderen Geräten und Dateifreigaben, über die gleichen
  Anfragen wie das Auflisten.
- Das Risiko des Geräte-Dateizugriffs ist bewusst akzeptiert: Ein verlorenes oder kompromittiertes
  eigenes Gerät erreicht das Dateisystem jedes anderen eigenen Geräts, dessen Vault offen ist und
  dessen Schalter an ist. Ein ADR hält das bei der Planung fest.
- Wer eine Dateifreigabe bekommt, darf mit den Inhalten alles tun; holzi verhindert Weitergabe nicht.

## Nicht im Umfang

- Gespiegelte Kopien, Offline-Verfügbarkeit und Cloud-Sicherung (spätere Sync-Regeln, Spec 025).
- Zugriff, wenn eine Seite offline ist; Zwischenspeichern von Inhalten bei Dritten.
- Ablaufdatum von Dateifreigaben, Kontaktgruppen, Benachrichtigung über neue Dateifreigaben.
- Links für Personen ohne holzi.
- Zugriff von Erweiterungen auf andere Geräte oder empfangene Dateifreigaben.
- Weitergabe einer empfangenen Dateifreigabe an Dritte als Dateifreigabe (ein Kontakt kann Inhalte
  kopieren und selbst freigeben).
- Hintergrunddienst auf Android und Desktop.
