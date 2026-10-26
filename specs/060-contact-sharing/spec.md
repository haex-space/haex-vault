# Feature Specification: Kontakte teilen und der Kontakt „Ich“

**Feature Branch**: `060-contact-sharing`

**Created**: 2026-10-26

**Status**: Draft

**Input**: User description: "Das Handling von Kontakten verbessern. (1) Es gibt einen Kontakt „Ich“, der
dem Nutzer der Vault entspricht und in dem der Nutzer seine Daten pflegt. (2) Genau diesen Kontakt kann
der Nutzer teilen, wählt aber VOR dem Teilen per Checkbox, welche Daten er teilen möchte (E-Mail-Adressen,
Telefonnummern, Anschriften, Geburtsdatum, iroh- oder Nostr-Adressen und so weiter). (3) JEDER Kontakt
lässt sich gleichermaßen mit anderen Nutzern teilen, ob der Nutzer seinen eigenen oder einen anderen aus
seinem Adressbuch teilt.“

## Begriffe

- **Kontakt**, **Angabe**, **Art einer Angabe**, **Visitenkarte**, **holzi-Adressen**, **bestätigt**,
  **unbestätigt**, **blockierte Vault**: wie in Spec 051. Eine **Angabe** ist eine Zeile eines Kontakts
  (Telefonnummer, E-Mail-Adresse, Anschrift, Webseite, Messenger-Kennung, Ereignis wie ein Geburtstag,
  Beziehung, eigene Angabe oder eine Angabe unbekannter Art).
- **Vault-Identität**, **Geräteliste**, **Gerät der Vault**: wie in Spec 024. **iroh-Adresse**: die
  Netzwerkkennung eines Geräts auf einer Visitenkarte.
- **Ich-Kontakt**: der eine Kontakt jeder Vault, der den Nutzer selbst beschreibt. In der Oberfläche
  heißt er immer „Ich“.
- **Teilen-Dialog**: der Dialog, in dem der Nutzer für einen Kontakt festlegt, was weitergegeben wird.
  Er ist für jeden Kontakt derselbe, auch für den Ich-Kontakt.
- **Weitergabe**: das Ergebnis des Teilen-Dialogs, ein Text („Umschlag“), den der Nutzer als Link, über
  die Zwischenablage oder als QR-Code weitergibt. Er trägt den Namen, die gewählten Angaben und, wenn
  gewählt, die holzi-Adressen mit ihrem Echtheitsnachweis.
- **Link**: die Textform der Weitergabe, ein Deep Link der App. Der QR-Code und die Zwischenablage
  enthalten genau denselben Text. Ein Tippen auf den Link in einer anderen App oder ein Scannen des
  QR-Codes mit der Kamera des Systems öffnet holzi mit der Vorschau.
- **Echtheitsnachweis der holzi-Adressen**: der mit der Vault-Identität des Kontakts unterschriebene
  Teil einer Weitergabe, der belegt, dass Vault-Identität, Geräte und Nostr-Relays zusammengehören.
  Intern ist das die Visitenkarte aus Spec 051. In der Oberfläche heißt die Zeile dazu „holzi-Adressen
  (bestätigt)“; das Wort „Karte“ erscheint dort nicht.
- **Eingelesene Weitergabe**: eine Weitergabe, die der Empfänger über Link, Zwischenablage oder Kamera
  in holzi öffnet.

## Beziehung zu bestehenden Specs

- **Spec 051** (Adressbuch): Diese Spec baut auf ihm auf und ändert drei Dinge. **FR-009** (die eigene
  Visitenkarte mit den Angaben, die der Nutzer dafür in den Einstellungen wählt, Standard nur der Name)
  entfällt: Was weitergegeben wird, bestimmt der Nutzer bei jedem Teilen im Teilen-Dialog. **FR-011**
  (Karte einlesen, Vorschau, neuer oder vorhandener Kontakt) gilt auch für die Weitergabe dieser Spec.
  **FR-015a** verliert den Punkt „Inhalt der eigenen Visitenkarte“. **FR-013** und **FR-014** gelten
  unverändert; FR-014 gilt auch für den Ich-Kontakt. **FR-012** („eine neuere Karte ersetzt die
  bestätigten Adressen“) gilt weiter für eine Visitenkarte im Format `holzi:card:` und für die anderen
  Wege dort, **nicht** für eine Weitergabe dieser Spec: Eine Weitergabe entfernt und ersetzt an einem
  vorhandenen Kontakt nie etwas von selbst (FR-024a bis FR-025).
- **Spec 020 und 051 FR-030** (Aktionen und Agent-Freigaben): Ein Agent kann einen Kontakt nur über eine
  Aktion mit der Wirkung `destructive` teilen, die jedes Mal der Nutzer bestätigen muss (FR-029 bis
  FR-032).
- **Spec 054** (Umbenennung in haex-vault): Das Schema des Deep Links trägt von Anfang an den neuen
  Namen (`haex-vault://`), damit 054 es nicht mehr umbenennen muss. Die bisherige Visitenkarte
  (`holzi:card:`) benennt 054 weiterhin um. `haex-vault://` ist nicht das Schema `haexvault://` der
  Legacy-App (haex-vault 2.x), beide unterscheiden sich im Bindestrich.
- **Spec 056** (Dateifreigaben): Eine Dateifreigabe geht nur an Kontakte mit bestätigter fremder
  Vault-Identität. Der Ich-Kontakt hat keine; er wird dort nie als Empfänger angeboten.
- **Spätere Messenger-Spec**: Der Direktversand eines Kontakts an einen anderen Kontakt entsteht dort
  als Nachricht mit Anhang. Die Weitergabe dieser Spec ist dafür der Anhang; der Teilen-Dialog bekommt
  dann ein weiteres Ziel „Im Chat senden“. Diese Spec baut keinen Versandweg zwischen Vaults.
- **Späteres Ziel vCard**: Export und Import von `.vcf`-Dateien ist ein weiteres Ziel desselben
  Teilen-Dialogs (Spec 051 hat es ausdrücklich ausgeklammert). Diese Spec bleibt bei ihrem eigenen Format.

## Clarifications

### Session 2026-10-26 (Brainstorming mit dem Betreiber)

- Q: Über welchen Weg kommt ein geteilter Kontakt beim Empfänger an? → A: In dieser Spec über Link,
  Zwischenablage und QR-Code. Direktversand an einen holzi-Kontakt gibt es erst mit der Messenger-Spec;
  dort ist die Weitergabe der Anhang.
- Q: Was geschieht mit den holzi-Adressen eines fremden Kontakts, den der Nutzer weitergibt? → A: Die
  ursprüngliche, vom Kontakt unterschriebene Karte geht unverändert mit, damit der Empfänger die Adressen
  selbst prüfen kann. Sie lässt sich nur als Ganzes mitschicken oder weglassen (eine Checkbox), denn jede
  Änderung bräche die Unterschrift. Alle anderen Angaben sind einzeln abwählbar.
- Q: Gilt dieselbe Auswahl für den eigenen Kontakt? → A: Ja, derselbe Teilen-Dialog. Beim Ich-Kontakt
  unterschreibt holzi pro Teilen eine frische Karte; dadurch sind Geräte (iroh) und Nostr-Relays einzeln
  abwählbar.
- Q: Was ist vorgewählt, und wird die Auswahl gemerkt? → A: Alles ist vorgewählt, jede Zeile lässt sich
  abwählen. Die Auswahl wird nicht gespeichert; beim nächsten Teilen ist wieder alles gewählt.
- Q: Wie wird der Ich-Kontakt angelegt, und wie heißt er? → A: Er ist ein fester Kontakt mit bekannter
  Kennung, der mit dem Schema entsteht (kein Gerät muss ihn anlegen). Er lässt sich nicht löschen und
  heißt in der Oberfläche immer „Ich“, auch wenn Namensteile eingetragen sind.
- Q: Welchen Namen sieht der Empfänger? → A: Die Namensteile des Kontakts; fehlen sie, die `npub`
  seiner Vault-Identität (beim Ich-Kontakt die eigene); gibt es auch die nicht, bleibt der Name leer. Das
  gilt für jeden Kontakt. Anzeigename, Spitzname, Organisation, Telefonnummer und E-Mail-Adresse sind
  kein Namens-Ersatz beim Teilen, denn sie sind jeweils eigene, abwählbare Zeilen.
- Q: Wird die Weitergabe als vCard abgebildet? → A: Nein, vorerst bleibt es beim eigenen Format. vCard
  ist ein späteres, eigenes Ziel.

### Session 2026-10-26 (Korrekturen des Betreibers zum ersten Entwurf)

- Q: Darf eine Weitergabe einen vorhandenen Kontakt überschreiben oder verkleinern? → A: Nie. Was am
  vorhandenen Kontakt steht, bleibt, auch wenn die Weitergabe es nicht enthält (eine Handynummer
  verschwindet nicht, nur weil jemand den Kontakt mit einer E-Mail-Adresse teilt). Für jede neue Angabe
  fragt holzi, ob sie hinzugefügt oder ein vorhandener Wert damit aktualisiert werden soll; Standard ist
  Hinzufügen. Es gehen keine Angaben verloren.
- Q: Gilt das auch für holzi-Adressen? → A: Ja. Eine Weitergabe entfernt oder ersetzt keine bestätigten
  Adressen eines Kontakts, auch nicht durch eine neuere Karte derselben Vault; sie fügt neue Geräte und
  Relays nur hinzu. Die bestätigte Vault-Identität eines vorhandenen Kontakts ersetzt eine Weitergabe nie,
  weil an ihr Zugriffsrechte hängen. Veraltete Adressen räumt später ein eigener Weg auf, auf dem holzi
  die aktuellen Adressen vom Kontakt selbst erfährt (Spec 051 FR-012, Weg „Geräteliste“); der ist nicht
  Teil dieser Spec.
- Q: Darf der Nutzer beim Einlesen auch einzelne Angaben ablehnen, auch holzi-Adressen? → A: Ja. Jede
  neue Angabe lässt sich ablehnen („Nicht übernehmen“), und holzi-Adressen zählen wie alle anderen
  Angaben. Weil eine unterschriebene Karte unteilbar ist, gilt die Wahl für die holzi-Adressen einer
  Karte als Ganzes. Vor dem Speichern prüft holzi jede Karte: gültige Unterschrift der Vault, die sie
  nennt, und ihre Kennung ist aus ihrem unterschriebenen Inhalt abgeleitet.
- Q: Lassen sich Kontakte ohne iroh- und Nostr-Adressen teilen (zum Beispiel ein Handykontakt, der kein
  holzi nutzt)? → A: Ja, genauso wie jeder andere. Eine Weitergabe ohne Karte ist der Normalfall für
  solche Kontakte und kein Fehler. Nur eine mitgeschickte, aber veränderte Karte weist holzi ab.
- Q: Dürfen Agenten Kontakte teilen? → A: Ja, aber als zerstörerische Aktion: Jeder Aufruf braucht die
  Bestätigung des Nutzers, der dabei sieht und ändern kann, was weitergegeben wird.

### Session 2026-10-26 (Rückfragen mit `speckit-clarify`)

- Q: Was zeigt die Vorschau zur Identität einer Karte? → A: Die Kurzform der `npub`. Bei einem
  vorhandenen Kontakt mit gleicher bestätigter `npub` den Satz „passt zur Vault dieses Kontakts“, bei einer
  erstmals bestätigten Identität den Hinweis, dass „bestätigt“ nur die Zugehörigkeit der Adressen zur
  Vault belegt, nicht, wer die Person ist. Keine zusätzliche Rückfrage.
- Q: Welches URL-Schema trägt der Link? → A: `haex-vault://`, der Name der App nach Spec 054, von
  Anfang an. Ein später umbenanntes Schema machte alle bis dahin verschickten Links ungültig.
- Q: Was sehen Agenten und Erweiterungen vom Ich-Kontakt? → A: Den echten Namen aus den Namensteilen
  und eine Markierung „ist der Nutzer selbst“, unter der bestehenden Freigabe „Kontakte lesen“. Das Wort
  „Ich“ ist nur Anzeige der Oberfläche.
- Q: Was ist der „Link“, und ist er derselbe Text wie der QR-Code? → A: Ja, derselbe Text: Link, Zwischen-
  ablage und QR-Code enthalten genau dieselbe Weitergabe. Der Link ist ein Deep Link: Ein Tippen darauf
  oder ein Scannen des QR-Codes mit der Kamera des Systems öffnet holzi mit der Vorschau.
- Q: Geht das Foto eines Kontakts mit in die Weitergabe? → A: Nein. Das Foto bleibt, wie es ist, und
  wird niemals über Link, Zwischenablage oder QR-Code weitergegeben. Erst das Teilen eines Kontakts im
  Chat (spätere Messenger-Spec) nimmt es in voller Größe mit.

## User Scenarios & Testing _(mandatory)_

### User Story 1 - Meine eigenen Daten im Kontakt „Ich“ pflegen (Priority: P1)

Der Nutzer öffnet die Kontakte und findet ganz oben „Ich“. Er trägt dort seinen Namen, seine
Telefonnummern, E-Mail-Adressen, Anschrift, seinen Geburtstag und weitere Angaben ein, so wie bei jedem
anderen Kontakt. Der Kontakt ist von Anfang an da, auf jedem Gerät derselbe, und lässt sich nicht
löschen.

**Why this priority**: Ohne einen festen Platz für die eigenen Daten gibt es nichts, was der Nutzer
teilen könnte. Die übrigen Geschichten bauen darauf auf.

**Independent Test**: In einer neuen Vault die Kontakte öffnen, „Ich“ finden, Angaben eintragen und den
Kontakt auf einem zweiten Gerät derselben Vault wiederfinden. Löschen ist nirgends möglich.

**Acceptance Scenarios**:

1. **Given** eine neu angelegte Vault ohne weitere Kontakte, **When** der Nutzer die Kontakte öffnet,
   **Then** steht dort „Ich“ ganz oben, leer und bereit zum Bearbeiten.
2. **Given** der Ich-Kontakt mit Vorname „Anna“ und Nachname „Beispiel“, **When** der Nutzer die Liste, die
   Detailansicht oder eine Auswahl von Kontakten ansieht, **Then** steht überall „Ich“, nicht „Anna
   Beispiel“; im Editor sind die Namensteile änderbar, der Anzeigename ist gesperrt.
3. **Given** der Ich-Kontakt mit dem Nachnamen „Beispiel“, **When** der Nutzer in der Liste nach
   „Beispiel“ sucht, **Then** erscheint „Ich“ als Treffer.
4. **Given** ein zweites Gerät derselben Vault, **When** es synchronisiert hat, **Then** zeigt es
   denselben einen Ich-Kontakt mit denselben Angaben, nicht zwei.
5. **Given** der Ich-Kontakt, **When** der Nutzer, ein Agent, eine Erweiterung oder der Abgleich mit dem
   Telefon versucht, ihn zu löschen, **Then** lehnt holzi das ab, und die Oberfläche bietet Löschen für
   ihn gar nicht erst an.
6. **Given** ein Android-Gerät mit eingeschaltetem Abgleich, **When** der Abgleich läuft, **Then** wird der
   Ich-Kontakt nicht ins Telefon-Adressbuch geschrieben, und kein Telefonkontakt wird ihm zugeordnet.
7. **Given** ein Agent mit der Freigabe „Kontakte lesen“ und der Ich-Kontakt mit Vorname „Anna“ und
   Nachname „Beispiel“, **When** der Agent Kontakte abfragt, **Then** bekommt er den Kontakt mit dem Namen
   „Anna Beispiel“ und der Markierung „ist der Nutzer selbst“, nicht mit dem Namen „Ich“.

---

### User Story 2 - Meinen Kontakt teilen und vorher wählen, was mit rausgeht (Priority: P1)

Der Nutzer wählt bei „Ich“ „Teilen“. Der Dialog zeigt alle Angaben mit Checkboxen: Anzeigename, Spitzname,
Telefonnummern,
E-Mail-Adressen, Anschriften, Geburtstag, Webseiten, Messenger-Kennungen, Organisation, Notiz und die
holzi-Adressen, also jedes Gerät mit seiner iroh-Adresse und jedes Nostr-Relay. Alles ist angehakt, der
Nutzer wählt ab, was er nicht weitergeben will, und bekommt darunter Link, QR-Code und eine
Kopieren-Schaltfläche.

**Why this priority**: Das ist der Kern der Anfrage: Der Nutzer entscheidet bei jedem Teilen, was
verlassen darf.

**Independent Test**: „Ich“ mit mehreren Angaben füllen, im Dialog zwei Angaben und ein Nostr-Relay
abwählen, den Text in einer zweiten Vault einlesen und prüfen, dass genau die gewählten Angaben
ankommen.

**Acceptance Scenarios**:

1. **Given** der Ich-Kontakt mit zwei Telefonnummern, einer E-Mail-Adresse, einer Anschrift und einem
   Geburtstag, **When** der Nutzer „Teilen“ wählt, **Then** zeigt der Dialog jede Angabe als eigene
   Zeile, alle angehakt, mit „Alle“ und „Keine“ als Abkürzung.
2. **Given** der Dialog, **When** der Nutzer eine Telefonnummer und den Geburtstag abwählt, **Then**
   enthält die Weitergabe diese beiden Angaben nicht, und der Link und der QR-Code ändern sich mit der
   Auswahl.
3. **Given** ein Gerät der Vault mit iroh-Adresse und zwei Nostr-Relays, **When** der Dialog für „Ich“
   offen ist, **Then** stehen unter „holzi-Adressen“ das Gerät und jedes Relay als eigene Zeilen, jede
   einzeln abwählbar; die Vault-Identität ist immer dabei und nicht abwählbar.
4. **Given** der Nutzer hat im Dialog etwas abgewählt und ihn geschlossen, **When** er ihn wieder öffnet,
   **Then** ist wieder alles angehakt.
5. **Given** der Ich-Kontakt ohne Vorname und ohne Nachname, **When** der Nutzer teilt, **Then** zeigt
   der Empfänger als Namen die `npub` der eigenen Vault-Identität.
6. **Given** ein verknüpftes Gerät, das das Vault-Geheimnis nicht hält und deshalb nicht unterschreiben
   kann, **When** der Nutzer „Ich“ teilt, **Then** sagt der Dialog, dass auf diesem Gerät keine
   holzi-Adressen mitgehen können, und die Weitergabe enthält nur Namen und gewählte Angaben.
7. **Given** der Nutzer wählt alles ab, was sich abwählen lässt, und der Name ist leer, **When** der
   Dialog das Ergebnis zeigen soll, **Then** zeigt er einen Hinweis, dass nichts zum Weitergeben übrig
   ist, statt eines leeren Links.

---

### User Story 3 - Jeden Kontakt aus dem Adressbuch genauso teilen (Priority: P1)

Der Nutzer öffnet einen beliebigen Kontakt aus seinem Adressbuch, etwa einen Handwerker, und wählt
„Teilen“. Es öffnet sich derselbe Dialog wie bei „Ich“. Alle Angaben des Kontakts sind angehakt und
einzeln abwählbar. Hat der Kontakt bestätigte holzi-Adressen, steht dort eine zusätzliche Zeile „holzi-
Adressen (bestätigt)“, die den ursprünglichen, vom Kontakt unterschriebenen Nachweis mitschickt.

**Why this priority**: Der Nutzer wollte ausdrücklich, dass jeder Kontakt gleich teilbar ist. Ein
zweiter, anderer Ablauf für fremde Kontakte wäre für ihn schwer zu verstehen.

**Independent Test**: Einen Kontakt mit Telefonnummer, E-Mail-Adresse, Notiz und bestätigten holzi-
Adressen teilen, die Notiz und die E-Mail-Adresse abwählen, in einer zweiten Vault einlesen und prüfen,
dass Notiz und E-Mail-Adresse fehlen, die holzi-Adressen aber als bestätigt ankommen.

**Acceptance Scenarios**:

1. **Given** ein Kontakt aus dem Adressbuch, **When** der Nutzer in dessen Menü „Teilen“ wählt, **Then**
   öffnet derselbe Teilen-Dialog wie bei „Ich“, mit allen Angaben des Kontakts, alle angehakt.
2. **Given** ein Kontakt mit bestätigten holzi-Adressen, **When** der Dialog offen ist, **Then** zeigt er
   genau eine Zeile „holzi-Adressen (bestätigt)“ mit dem Hinweis, dass der ursprüngliche Nachweis
   unverändert mitgeht und auch die Angaben enthält, die der Kontakt damals selbst freigegeben hat;
   Geräte und Relays sind hier nicht einzeln abwählbar.
3. **Given** ein Kontakt ohne bestätigte holzi-Adressen, **When** der Dialog offen ist, **Then** fehlt
   diese Zeile.
4. **Given** der Nutzer wählt „holzi-Adressen (bestätigt)“ ab, **When** er die Weitergabe einliest,
   **Then** kommen Name und gewählte Angaben ohne Vault-Identität, Geräte und Relays an.
5. **Given** ein Kontakt, der von Hand eingetragene (unbestätigte) holzi-Adressen hat, **When** der Dialog
   offen ist, **Then** erscheinen sie als gewöhnliche Zeilen, einzeln abwählbar, und kommen beim
   Empfänger als unbestätigt an.
6. **Given** ein Kontakt ohne Vorname und Nachname mit bestätigter Vault-Identität, **When** der Nutzer
   teilt, **Then** zeigt der Empfänger als Namen die `npub` dieser Vault-Identität.
7. **Given** ein Kontakt mit einer Angabe einer Art, die diese holzi-Version nicht kennt, **When** der
   Dialog offen ist, **Then** erscheint sie als Zeile mit Bezeichnung und Wert und geht unverändert mit,
   wenn sie angehakt bleibt (Spec 051 FR-007).
8. **Given** ein Kontakt, der vom Telefon stammt und keine holzi-Adressen hat (kein holzi-Nutzer),
   **When** der Nutzer ihn teilt, **Then** funktioniert das genauso: derselbe Dialog ohne die Zeile „holzi-
   Adressen (bestätigt)“, eine Weitergabe ohne Karte, und beim Empfänger weist die Vorschau darauf hin,
   dass der Kontakt nicht über holzi erreichbar ist. Das ist kein Fehler.

---

### User Story 4 - Eine geteilte Weitergabe einlesen, ohne etwas zu verlieren (Priority: P1)

Der Empfänger öffnet einen Link, fügt Text aus der Zwischenablage ein oder scannt einen QR-Code. holzi
prüft die Weitergabe und zeigt eine Vorschau mit Namen, Angaben und holzi-Adressen, mit „bestätigt“ oder
„unbestätigt“ gekennzeichnet. Erst nach der Wahl des Nutzers wird gespeichert: als neuer Kontakt oder
zu einem vorhandenen. Kommt die Weitergabe zu einem vorhandenen Kontakt, bleibt dort alles erhalten, was
steht. Für jede neue Angabe entscheidet der Nutzer, ob sie hinzukommt (Standard) oder einen vorhandenen
Wert aktualisiert.

**Why this priority**: Ohne Einlesen wäre Teilen eine Einbahnstraße. Und wer Kontakte von mehreren
Menschen bekommt, darf nie Daten verlieren, nur weil eine Version unvollständig ist.

**Independent Test**: Eine Weitergabe aus Geschichte 2 und eine aus Geschichte 3 einlesen, in der
Vorschau prüfen und einmal als neuen, einmal zu einem vorhandenen Kontakt speichern. Einen Kontakt mit
Handynummer um eine Weitergabe nur mit E-Mail-Adresse ergänzen und prüfen, dass die Handynummer bleibt.

**Acceptance Scenarios**:

1. **Given** eine gültige Weitergabe, **When** der Nutzer sie einliest, **Then** zeigt holzi eine
   Vorschau, ohne etwas zu speichern; bis zur Wahl des Nutzers entsteht kein Kontakt und ändert sich
   keiner.
2. **Given** die Vorschau, **When** der Nutzer „Neuer Kontakt“ wählt, **Then** entsteht ein Kontakt mit
   dem Namen und den Angaben der Weitergabe; sind holzi-Adressen mit gültigem Nachweis dabei, sind sie
   bestätigt.
3. **Given** ein vorhandener Kontakt mit einer Handynummer und eine Weitergabe desselben Menschen von
   jemand anderem, die nur eine E-Mail-Adresse trägt, **When** der Nutzer „Zu vorhandenem hinzufügen“
   wählt, **Then** kommt die E-Mail-Adresse dazu, und die Handynummer bleibt unverändert.
4. **Given** ein vorhandener Kontakt mit der E-Mail-Adresse a@x.de und eine Weitergabe mit b@y.de,
   **When** die Vorschau die Angabe zeigt, **Then** steht dort die Wahl „Hinzufügen“ (vorgewählt) oder
   „Aktualisieren“; bei „Aktualisieren“ ersetzt b@y.de die gewählte vorhandene E-Mail-Adresse (bei
   mehreren wählt der Nutzer, welche), und nur dann geht a@x.de verloren, weil der Nutzer es so wählte.
5. **Given** eine Weitergabe mit mehreren neuen Angaben, **When** die Vorschau sie zeigt, **Then** hat
   jede neue Angabe ihre eigene Wahl, und ein einziger Bestätigen-Klick mit den Standardwerten fügt alles
   hinzu.
6. **Given** eine Weitergabe, deren Angaben der Kontakt schon mit gleicher Art und gleichem Wert hat,
   **When** die Vorschau sie zeigt, **Then** fragt holzi dazu nichts und legt nichts doppelt an.
7. **Given** ein vorhandener Kontakt mit dem Nachnamen „Müller“ und eine Weitergabe mit dem Nachnamen
   „Mueller“, **When** die Vorschau das Feld zeigt, **Then** steht dort die Wahl „Behalten“ (vorgewählt)
   oder „Aktualisieren“; ist das Feld am Kontakt leer, wird es ohne Frage gefüllt.
8. **Given** eine Weitergabe ohne Vault-Identität, die eine Telefonnummer oder E-Mail-Adresse trägt, die
   schon bei einem vorhandenen Kontakt steht, **When** der Nutzer einliest, **Then** nennt die Vorschau
   diesen Kontakt als Hinweis, ohne ihn vorzuwählen; holzi fügt nie von selbst zusammen.
9. **Given** die Vault-Identität der Weitergabe steht schon bei einem anderen Kontakt bestätigt, **When**
   der Nutzer einliest, **Then** bietet die Vorschau nur an, zu diesem Kontakt hinzuzufügen, statt einen
   zweiten Kontakt mit derselben Vault-Identität anzulegen (Spec 051 FR-013).
10. **Given** ein vorhandener Kontakt mit bestätigter Vault-Identität A und eine Weitergabe mit einer
    Karte der Vault-Identität B, **When** der Nutzer einliest, **Then** ersetzt holzi die bestätigte
    Identität A in keinem Fall; die Vorschau weist darauf hin, und B kommt nur in einen neuen Kontakt.
11. **Given** ein vorhandener Kontakt mit bestätigten holzi-Adressen und eine Weitergabe mit einer Karte
    derselben Vault, die weniger Geräte trägt, **When** der Nutzer hinzufügt, **Then** bleiben alle
    bisherigen bestätigten Adressen erhalten.
12. **Given** ein vorhandener Kontakt, dessen gespeicherte Adressen veraltet sind (der Kontakt hat seine
    Relays gewechselt und die alten gelöscht), und eine Weitergabe desselben Kontakts von jemand anderem
    mit einer neueren Karte derselben Vault, **When** der Nutzer hinzufügt, **Then** stehen die neuen
    Geräte und Relays neben den alten, und der Kontakt ist wieder erreichbar; keine alte Adresse geht
    verloren.
13. **Given** eine Weitergabe mit neuen holzi-Adressen für einen vorhandenen Kontakt, **When** der
    Nutzer in der Vorschau „Nicht übernehmen“ wählt, **Then** bleiben die holzi-Adressen des Kontakts
    unverändert, und die übrigen Angaben der Weitergabe werden nach ihrer eigenen Wahl übernommen.
14. **Given** eine Weitergabe ohne Karte (der Kontakt ist kein holzi-Nutzer), **When** der Nutzer
    einliest, **Then** funktioniert alles wie oben, und die Vorschau zeigt, dass der Kontakt nicht über
    holzi erreichbar ist.
15. **Given** eine Weitergabe mit einer mitgeschickten, aber veränderten oder ungültig unterschriebenen
    Karte, **When** der Nutzer einliest, **Then** weist holzi sie mit einem Hinweis ab und speichert
    nichts.
16. **Given** die Weitergabe der eigenen Vault, **When** der Nutzer sie einliest, **Then** weist holzi sie
    mit einem Hinweis ab (wie bei der eigenen Karte in Spec 051 FR-011).
17. **Given** eine Visitenkarte im Format `holzi:card:` (zum Beispiel aus einer Einladung), **When** der
    Nutzer sie einliest, **Then** liest holzi sie weiter wie bisher (Spec 051).
18. **Given** ein Kontakt mit Foto, **When** der Nutzer ihn teilt und die Weitergabe einliest, **Then**
    trägt die Weitergabe kein Foto, und der Empfänger bekommt den Kontakt ohne Foto.
19. **Given** eine gespeicherte Karte, deren Zeile nicht zu ihrem Inhalt passt (die Unterschrift stimmt
    nicht, die Kennung ist nicht aus dem Inhalt abgeleitet oder die Zeile ordnet sie einer anderen Vault
    zu), **When** holzi Adressen eines Kontakts braucht, **Then** verwendet es diese Zeile nicht und
    liefert keine Adressen daraus.
20. **Given** ein Link der Weitergabe in einer Nachricht oder Mail und eine geöffnete Vault, **When** der
    Nutzer darauf tippt, **Then** kommt holzi in den Vordergrund und zeigt die Vorschau; es ist noch
    nichts gespeichert.
21. **Given** derselbe Link, aber holzi ist beendet oder die Vault gesperrt, **When** der Nutzer darauf
    tippt, **Then** startet holzi, verlangt das Entsperren und zeigt danach die Vorschau zu genau diesem
    Link.
22. **Given** der QR-Code einer Weitergabe auf dem Bildschirm eines anderen Geräts, **When** der Nutzer ihn
    mit der Kamera des Systems scannt und den Hinweis antippt, **Then** öffnet sich holzi mit der
    Vorschau, wie bei einem Link.
23. **Given** ein Link mit beschädigtem oder abgeschnittenem Inhalt, **When** der Nutzer darauf tippt,
    **Then** öffnet holzi mit einem Hinweis, dass der Link ungültig ist, und speichert nichts.
24. **Given** der Kontakt Alice mit bestätigter `npub` und eine Weitergabe von Bob mit einer Karte
    derselben `npub`, **When** die Vorschau erscheint, **Then** steht dort die Kurzform der `npub` und
    „passt zur Vault dieses Kontakts“, und die neuen Adressen lassen sich zu Alice hinzufügen.
25. **Given** derselbe Kontakt Alice und eine Weitergabe von Bob mit einer Karte einer **anderen**
    `npub`, **When** die Vorschau erscheint, **Then** lässt sie das Hinzufügen zu Alice nicht zu und
    bietet nur einen neuen Kontakt an.
26. **Given** eine Weitergabe „Alice“ mit gültiger Karte einer bisher unbekannten Vault, **When** die
    Vorschau erscheint, **Then** zeigt sie die Kurzform der `npub` und den Hinweis, dass „bestätigt“
    nur die Zugehörigkeit der Adressen zur Vault belegt, nicht die Identität der Person.

---

### User Story 5 - Ein Agent teilt einen Kontakt, und ich bestätige (Priority: P2)

Der Nutzer bittet den eingebauten Agenten im Chat oder einen externen Agenten: „Gib mir den Kontakt von
Ben zum Weiterschicken.“ Der Agent ruft die Aktion zum Teilen auf. Weil dabei Daten die Vault verlassen
und nicht zurückzuholen sind, gilt sie als zerstörerisch: holzi zeigt dem Nutzer denselben Teilen-Dialog
mit dem, was der Agent weitergeben will, und der Agent bekommt den Text erst, wenn der Nutzer bestätigt.

**Why this priority**: Ein Agent soll im Gespräch mit Kontakten umgehen können, ohne dass er Daten
unbemerkt herausgeben kann. Die Bestätigung ist der Schutz dagegen.

**Independent Test**: Über den Weg des eingebauten Agenten die Aktion aufrufen, bestätigen, ändern
und ablehnen, und in jedem Fall prüfen, was der Agent zurückbekommt.

**Acceptance Scenarios**:

1. **Given** ein Agent mit der Freigabe „Kontakte lesen“, **When** er einen Kontakt teilen will, **Then**
   zeigt holzi dem Nutzer den Teilen-Dialog für diesen Kontakt, und der Agent hat bis zur Bestätigung
   nichts bekommen.
2. **Given** der Dialog aus der Agent-Anfrage, **When** der Nutzer eine Telefonnummer abwählt und
   bestätigt, **Then** bekommt der Agent einen Text ohne diese Telefonnummer.
3. **Given** der Dialog aus der Agent-Anfrage, **When** der Nutzer ablehnt, **Then** bekommt der Agent
   eine Ablehnung ohne Inhalt des Kontakts.
4. **Given** ein Agent, der dem Nutzer schon früher einen Kontakt zum Teilen vorgelegt hat und bestätigt
   bekam, **When** er die Aktion erneut aufruft, **Then** fragt holzi erneut; es gibt keine dauerhafte
   Erlaubnis für diese Aktion.
5. **Given** ein Agent ohne die Freigabe „Kontakte lesen“, **When** er die Aktion aufruft, **Then**
   lehnt holzi ab, ohne zu verraten, ob es den Kontakt gibt (Spec 051 FR-031).
6. **Given** eine Erweiterung, **When** sie versucht, einen Kontakt zu teilen, **Then** gibt es dafür
   keine Schnittstelle (Spec 051 FR-029).

---

### Edge Cases

- Der Ich-Kontakt wird auf zwei Geräten gleichzeitig bearbeitet: Er folgt den Sync-Regeln jedes Kontakts;
  es gibt nie zwei Ich-Kontakte.
- Die Weitergabe ist zu groß für einen QR-Code (mehr als 2.048 Zeichen): Der Dialog zeigt keinen QR-Code,
  sondern den Hinweis „zu groß für QR“. Link und Zwischenablage funktionieren.
- Die Weitergabe überschreitet auch die Höchstgröße für Link und Zwischenablage (zum Beispiel durch
  sehr viele Angaben oder eine sehr lange Notiz): Der Dialog nennt den Grund und was sich abwählen lässt, und gibt keinen abgeschnittenen Text aus.
- Der Nutzer wählt bei „Ich“ ein Gerät und alle Relays ab: Die Weitergabe trägt nur die Vault-Identität
  und die gewählten Angaben. Der Empfänger kann den Nutzer so nicht über holzi erreichen, und der Dialog
  weist darauf hin.
- Der Nutzer teilt einen Kontakt, dessen Originalkarte älter ist als die aktuelle Geräteliste des
  Kontakts: Die Weitergabe trägt die gespeicherte Karte unverändert. Beim Empfänger entfernt sie nie
  Adressen eines vorhandenen Kontakts, ob sie älter oder neuer ist als dessen gespeicherte Karte.
- Der Ich-Kontakt wird bewusst mit weniger Geräten oder Relays geteilt: Beim Empfänger bleiben alle dort
  schon bestätigten Adressen derselben Vault erhalten, die verkleinerte Weitergabe kann sie nicht kürzen.
  Dass ein Gerät aus der Geräteliste verschwunden ist, erfährt der Empfänger auf den bisherigen Wegen
  (Spec 051 FR-012), nicht durch eine Weitergabe.
- Zwei Menschen teilen denselben Kontakt mit unterschiedlichen Angaben: Beide Weitergaben lassen sich
  nacheinander zum selben Kontakt hinzufügen; keine entfernt etwas der anderen.
- Eine Weitergabe trägt mehrere Angaben derselben Art, der Kontakt hat keine davon: Es gibt keinen
  Wert zum Aktualisieren, die Vorschau bietet nur „Hinzufügen“ an.
- Der Nutzer wählt bei einer Angabe „Aktualisieren“, aber der Kontakt hat mehrere Werte derselben Art:
  Der Nutzer wählt, welcher ersetzt wird; ohne diese Wahl bleibt es beim Hinzufügen.
- Der Nutzer bestätigt die Vorschau mit den Standardwerten: Alles Neue wird hinzugefügt, nichts ersetzt,
  nichts entfernt.
- Ein Kontakt hat keine holzi-Adressen (Handykontakt, kein holzi-Nutzer): Er lässt sich vollständig teilen
  und einlesen; es fehlt nur der Weg, ihn über holzi zu erreichen.
- Ein Agent ruft die Aktion zum Teilen auf, während der Nutzer das Fenster nicht offen hat oder nicht
  antwortet: Die Aktion wartet auf die Bestätigung oder läuft ab; der Agent bekommt nie Inhalte ohne
  sie.
- Die Notiz eines Kontakts ist vorgewählt: Sie kann Privates über den Kontakt enthalten. Der Dialog zeigt
  sie wie jede Zeile mit ihrem Inhalt; abgewählt geht sie nicht mit.
- Ein Kontakt hat weder Namensteile noch eine Vault-Identität: Der Name geht leer mit, und der Empfänger
  leitet ihn nach den Regeln aus Spec 051 aus Telefonnummer oder E-Mail-Adresse ab, falls die angehakt
  sind.
- Der Nutzer liest eine Weitergabe ein, deren Name leer ist und die keine Angaben trägt: holzi zeigt
  einen Hinweis und speichert nichts.
- Eine andere App kürzt einen langen Link beim Anzeigen oder Weitergeben: Der Link ist dann ungültig
  (Szenario 23), das Einfügen aus der Zwischenablage mit dem vollständigen Text funktioniert weiter.
- Der Nutzer tippt auf einen Link, während holzi eine andere Abfrage zeigt (etwa die Bestätigung einer
  Agent-Anfrage): Die Vorschau erscheint danach oder darüber, ohne die Abfrage zu verwerfen; keine Abfrage
  wird dadurch bestätigt.
- Ein Umschlag aus einer neueren holzi-Version mit unbekannter Versionsnummer: holzi weist ihn mit einem
  Hinweis ab, ohne etwas zu speichern.
- Die Weitergabe trägt eine Angabe einer Art, die der Empfänger nicht kennt: Sie bleibt unverändert und
  wird schreibgeschützt angezeigt (Spec 051 FR-007).
- Eine blockierte Vault (Spec 051 FR-039): Die Weitergabe eines Kontakts, dessen Nachweis eine blockierte
  Vault-Identität nennt, behandelt holzi wie eine Visitenkarte dieser Vault.
- Gruppen, Abgleichsziele und Herkunft eines geteilten Kontakts gehen nicht mit, denn sie gehören dem
  Absender.

## Requirements _(mandatory)_

### Functional Requirements

**Der Kontakt „Ich“**

- **FR-001**: Jede Vault MUSS genau einen Ich-Kontakt haben. Er MUSS eine feste, bekannte Kennung haben
  und mit dem Schema der Vault entstehen, sodass kein Gerät ihn anlegen muss, und er MUSS auf allen
  Geräten derselben Vault derselbe Kontakt sein.
- **FR-002**: Der Ich-Kontakt DARF NICHT gelöscht werden können, von keinem Aufrufer: nicht vom Nutzer,
  nicht von einem Agenten, nicht von einer Erweiterung, nicht durch den Abgleich mit dem Telefon. Die
  Oberfläche DARF Löschen für ihn nicht anbieten.
- **FR-003**: Die Oberfläche MUSS den Ich-Kontakt immer als „Ich“ (in der Sprache der Oberfläche)
  anzeigen: in der Liste, in der Detailansicht, in Auswahlen und in Suchtreffern, unabhängig von
  eingetragenen Namen. Der Anzeigename des Ich-Kontakts MUSS gesperrt sein; die Namensteile MÜSSEN
  änderbar bleiben. Die Suche MUSS ihn weiter über seine eingetragenen Namen, Telefonnummern und
  E-Mail-Adressen finden.
- **FR-003a**: Agenten und Erweiterungen, die Kontakte lesen dürfen (Spec 051 FR-029, FR-030), MÜSSEN den
  Ich-Kontakt mit dem Namen bekommen, den die Namensteile ergeben (nicht mit dem Wort „Ich“), und mit
  einer Markierung, dass es der Kontakt des Nutzers selbst ist. Sind keine Namensteile eingetragen, ist
  der Name leer, und die Markierung bleibt. Es gibt dafür keine eigene Berechtigung; die Freigabe
  „Kontakte lesen“ gilt auch für ihn. Wer Kontakte ändern darf, darf ihn ändern, aber nicht löschen
  (FR-002).
- **FR-004**: Der Ich-Kontakt MUSS in der ungefilterten Kontaktliste ganz oben stehen, unabhängig von
  Sortierung und Gruppierung.
- **FR-005**: Der Nutzer MUSS im Ich-Kontakt alle Angaben pflegen können, die jeder Kontakt haben kann
  (Spec 051 FR-003), einschließlich Foto und Notiz. Die holzi-Adressen der eigenen Vault (Vault-
  Identität, Geräte, Nostr-Relays) MUSS der Editor schreibgeschützt anzeigen; holzi DARF sie nicht als
  Angaben im Kontakt speichern.
- **FR-006**: Der Ich-Kontakt DARF NICHT ins Telefon-Adressbuch geschrieben werden, kein Abgleich DARF ihn
  verändern, und er DARF keine Abgleichsziele haben (Spec 051 FR-014).
- **FR-007**: holzi DARF den Ich-Kontakt nirgends als Empfänger anbieten, in dem ein Kontakt mit
  bestätigter fremder Vault-Identität gebraucht wird (zum Beispiel für Dateifreigaben).

**Der Teilen-Dialog**

- **FR-008**: holzi MUSS bei jedem Kontakt, auch beim Ich-Kontakt, die Aktion „Teilen“ anbieten. Sie MUSS
  für jeden Kontakt denselben Dialog öffnen.
- **FR-009**: Der Dialog MUSS jede Angabe des Kontakts als eigene Zeile mit Checkbox zeigen: Anzeigename,
  Spitzname, Telefon-
  nummern, E-Mail-Adressen, Anschriften, Webseiten, Messenger-Kennungen, SIP-Adressen, Ereignisse wie
  den Geburtstag, Beziehungen, eigene Angaben und Angaben unbekannter Art, dazu Organisation, Position
  und Notiz, soweit der Kontakt sie hat. Alle Zeilen MÜSSEN vorgewählt sein, jede MUSS abwählbar
  sein. Der Dialog MUSS „Alle“ und „Keine“ anbieten. Das Foto DARF nicht in der Weitergabe sein und
  keine Zeile haben; ein Hinweis MUSS sagen, dass Fotos nicht per Link, Zwischenablage oder QR-Code
  weitergegeben werden.
- **FR-010**: holzi DARF die Auswahl des Dialogs NICHT speichern. Jedes Öffnen beginnt mit allem
  gewählt.
- **FR-011**: Der Name des Kontakts MUSS immer mitgehen und DARF nicht abwählbar sein. Als Name MUSS holzi
  die Namensteile des Kontakts verwenden; fehlen sie, die `npub` seiner Vault-Identität (beim
  Ich-Kontakt die eigene); gibt es auch diese nicht, MUSS der Name leer bleiben. Anzeigename, Spitzname,
  Organisation, Telefonnummer und E-Mail-Adresse DÜRFEN nicht als Namens-Ersatz dienen; Anzeigename und
  Spitzname MÜSSEN aber als eigene, abwählbare Angaben teilbar sein.
- **FR-012**: Beim Ich-Kontakt MUSS der Dialog die holzi-Adressen der eigenen Vault zeigen: jedes Gerät
  mit seiner iroh-Adresse und jedes Nostr-Relay als eigene, abwählbare Zeile. Wenn eine Karte mitgeht,
  MUSS die Vault-Identität darin immer mitgehen. holzi MUSS dafür bei jedem Teilen eine frisch unterschriebene
  Karte erzeugen, die nur die gewählten Geräte und Relays enthält und keine Angaben des Kontakts. Kann das
  Gerät nicht unterschreiben, MUSS die Karte mitsamt Identität entfallen und FR-016 den Nutzer informieren.
- **FR-013**: Bei einem fremden Kontakt mit bestätigten holzi-Adressen MUSS der Dialog genau eine Zeile
  „holzi-Adressen (bestätigt)“ zeigen. Ist sie gewählt, MUSS holzi die ursprüngliche, vom Kontakt
  unterschriebene Karte unverändert mitschicken. Die Zeile MUSS darauf hinweisen, dass die Karte auch die
  Angaben enthält, die der Kontakt selbst dafür freigegeben hat, und dass Geräte und Relays nur
  gemeinsam mitgehen. Hat der Kontakt keine bestätigten holzi-Adressen, MUSS die Zeile fehlen.
- **FR-014**: Von Hand eingetragene, unbestätigte holzi-Adressen eines Kontakts MÜSSEN im Dialog als
  gewöhnliche Zeilen erscheinen und beim Empfänger als unbestätigt ankommen.
- **FR-015**: Die Weitergabe DARF nichts enthalten, was der Nutzer abgewählt hat. Ausgenommen sind die
  Angaben im ursprünglichen Nachweis eines fremden Kontakts (FR-013), die der Empfänger-Teil von holzi
  nicht übernimmt (FR-023).
- **FR-016**: Kann ein Gerät keine Karte unterschreiben, weil es das Vault-Geheimnis nicht hält, MUSS der
  Dialog beim Ich-Kontakt sagen, dass auf diesem Gerät keine holzi-Adressen mitgehen können, und die
  Weitergabe ohne sie erzeugen. holzi DARF sie nicht stillschweigend weglassen.
- **FR-017**: Gibt es nach der Auswahl nichts Weiterzugebendes (leerer Name, keine Angabe, keine
  Adressen), MUSS der Dialog das mit einem Hinweis sagen und keinen Link und keinen QR-Code zeigen.
- **FR-017a**: Ein Kontakt ohne holzi-Adressen (etwa vom Telefon, ohne holzi-Nutzer) MUSS sich genauso
  teilen lassen wie jeder andere. Die Weitergabe trägt dann keine Karte, der Dialog zeigt die Zeile „holzi-
  Adressen (bestätigt)“ nicht, und die Vorschau beim Empfänger MUSS darauf hinweisen, dass der Kontakt
  nicht über holzi erreichbar ist. Eine fehlende Karte DARF nie ein Fehler sein.

**Das Format der Weitergabe**

- **FR-018**: Die Weitergabe MUSS ein Text in Form eines Deep Links der App sein, mit dem Schema
  `haex-vault://` (dem Namen der App nach Spec 054). Als Link, über die
  Zwischenablage und, wenn er passt, als QR-Code MUSS holzi immer genau denselben Text ausgeben. Er MUSS
  eine Versionsnummer, den Namen, die gewählten Angaben und optional eine unterschriebene Karte tragen
  und sich am Anfang von der bisherigen Visitenkarte (`holzi:card:`) unterscheiden.
- **FR-018a**: Öffnet der Nutzer einen Link der Weitergabe außerhalb von holzi (Tippen in einer anderen
  App, Scannen des QR-Codes mit der Kamera des Systems), MUSS das Betriebssystem holzi starten oder in den
  Vordergrund holen und holzi MUSS die Vorschau nach FR-021 zeigen. Das Öffnen eines Links DARF nichts
  speichern und nichts ändern, nur die Vorschau zeigen. Ist die Vault gesperrt oder noch nicht geöffnet,
  MUSS holzi die Vorschau nach dem Entsperren zeigen; der Link darf dabei nicht verloren gehen. Das gilt
  auf allen Plattformen, auf denen holzi läuft (Android und Desktop). Läuft auf dem Desktop schon ein Prozess
  mit geöffneter Vault (holzi nutzt je Vault einen Prozess), MUSS der Link in diesem Prozess erscheinen und
  das Fenster nach vorn kommen, statt einen zweiten Prozess zu öffnen, in dem die Vault nicht noch einmal zu
  öffnen wäre. Läuft keiner, MUSS der neue Prozess den Link halten, bis der Nutzer eine Vault geöffnet hat.
  Laufen mehrere Vaults, MUSS die Vorschau den Namen der Vault nennen, in die gespeichert würde.
- **FR-019**: Der Dialog MUSS einen QR-Code nur zeigen, wenn die Weitergabe in höchstens 2.048 Zeichen
  passt; sonst MUSS er den Hinweis „zu groß für QR“ zeigen. Link und Zwischenablage MÜSSEN darüber hinaus
  funktionieren, bis zu einer Höchstgröße, die der Plan festlegt. Wird sie überschritten, MUSS der
  Dialog den Grund nennen und darauf hinweisen, was sich abwählen lässt.
- **FR-020**: holzi MUSS Visitenkarten im Format `holzi:card:` weiter einlesen können (Einladungen und
  andere Stellen, die sie verwenden). Der Teilen-Dialog MUSS nur noch das neue Format erzeugen.

**Einlesen**

- **FR-021**: holzi MUSS eine Weitergabe als Link (auch durch Tippen, FR-018a), über die
  Zwischenablage und per Kamera als QR-Code einlesen können und vor dem Speichern eine Vorschau mit Namen, Angaben und holzi-Adressen zeigen. Die
  Vorschau MUSS bestätigte und unbestätigte holzi-Adressen unterscheiden. holzi DARF vor der Wahl des
  Nutzers nichts speichern.
- **FR-021a**: Trägt die Weitergabe eine gültige Karte, MUSS die Vorschau die Kurzform der `npub` der
  Vault-Identität zeigen. Gehört die Karte zu einem vorhandenen Kontakt mit bestätigter Vault-Identität
  (gleiche `npub`), MUSS sie das sagen („passt zur Vault dieses Kontakts“). Würde die Weitergabe eine
  Vault-Identität erstmals bestätigen (neuer Kontakt oder vorhandener Kontakt ohne bestätigte Identität),
  MUSS die Vorschau darauf hinweisen, dass „bestätigt“ nur belegt, dass die Adressen zu dieser Vault
  gehören, nicht, wer die Person ist. Eine eigene zusätzliche Rückfrage gibt es nicht.
- **FR-022**: Trägt die Weitergabe eine Karte, MUSS holzi deren Unterschrift prüfen. Eine mitgeschickte,
  aber ungültige, veränderte oder zur eigenen Vault gehörende Karte MUSS holzi mit einem Hinweis
  abweisen und die ganze Weitergabe nicht speichern. Dasselbe gilt für einen Umschlag mit unbekannter
  Version, einen beschädigten oder zu großen Umschlag. Eine Weitergabe ohne Karte MUSS holzi dagegen
  einlesen (FR-017a).
- **FR-023**: Aus einer Karte in einer Weitergabe MUSS holzi nur Vault-Identität, Geräte und Nostr-Relays
  übernehmen, nie ihre Angaben. Die Angaben des Kontakts MÜSSEN aus der Weitergabe selbst stammen und
  als gewöhnliche, unbestätigte Angaben ankommen; die Herkunft MUSS wie bei einem Kontakt aus einer
  Visitenkarte gesetzt sein.
- **FR-024**: Nach der Vorschau MUSS der Nutzer wählen: neuer Kontakt oder Hinzufügen zu einem
  vorhandenen Kontakt (Spec 051 FR-011). Hat die Weitergabe dieselbe Vault-Identität wie ein
  vorhandener Kontakt, MUSS die Vorschau diesen Kontakt vorwählen (Spec 051 FR-013). Hat ein vorhandener
  Kontakt dieselbe Telefonnummer oder E-Mail-Adresse, MUSS die Vorschau ihn als Hinweis nennen, ohne ihn
  vorzuwählen. Den Ich-Kontakt DARF die Vorschau nicht als Ziel zum Hinzufügen anbieten. holzi DARF nie
  von selbst zusammenführen.
- **FR-024a**: Eine Weitergabe DARF an einem vorhandenen Kontakt nichts löschen, ersetzen oder kürzen, es
  sei denn, der Nutzer hat es für genau diese Angabe gewählt. Was am Kontakt steht und in der
  Weitergabe fehlt, MUSS unverändert bleiben (eine vorhandene Handynummer bleibt, auch wenn die
  Weitergabe nur eine E-Mail-Adresse trägt).
- **FR-024b**: Für jede Angabe der Weitergabe, die der Kontakt noch nicht mit gleicher Art und gleichem
  Wert hat, MUSS die Vorschau eine eigene Wahl zeigen: „Hinzufügen“ (vorgewählt), „Aktualisieren“ oder
  „Nicht übernehmen“. „Aktualisieren“ ersetzt einen vorhandenen Wert derselben Art; hat der Kontakt mehrere, wählt der
  Nutzer den zu ersetzenden, und hat er keinen, bietet die Vorschau nur „Hinzufügen“ an. Eine Angabe,
  die der Kontakt schon mit gleicher Art und gleichem Wert hat, MUSS holzi ohne Frage und ohne
  Duplikat übergehen. Mit den Standardwerten fügt ein einziger Bestätigen-Klick alles Neue hinzu.
- **FR-024c**: Für Felder, die ein Kontakt nur einmal hat (Namensteile, Anzeigename, Spitzname, Organisation,
  Position, Notiz),
  MUSS holzi ein leeres Feld ohne Frage füllen. Ist es belegt und weicht der Wert der Weitergabe
  ab, MUSS die Vorschau „Behalten“ (vorgewählt) oder „Aktualisieren“ anbieten.
- **FR-024d**: Die Vorschau für ein vorhandenes Ziel MUSS eine undurchsichtige Revision des Zielkontakts
  ausgeben. `contacts_share_accept` MUSS diese Revision zurückgeben und die Annahme ablehnen, wenn sich
  der Zielkontakt seit der Vorschau geändert hat; holzi MUSS dann eine frische Vorschau liefern und darf
  nichts schreiben. Die separate Prüfung, ob ein `value_id` zum Zielkontakt gehört, bleibt zusätzlich nötig.
- **FR-025**: Eine Weitergabe DARF bestätigte holzi-Adressen eines vorhandenen Kontakts nie entfernen,
  ersetzen oder verkürzen, auch nicht durch eine neuere Karte derselben Vault. Neue Geräte und Relays
  einer gültig unterschriebenen Karte derselben Vault-Identität MUSS die Vorschau als eine Angabe
  „holzi-Adressen (bestätigt)“ mit den neuen Geräten und Relays zeigen: „Hinzufügen“ (vorgewählt) oder
  „Nicht übernehmen“; „Aktualisieren“ gibt es für holzi-Adressen nicht. Die Wahl gilt für die
  Adressen einer Karte als Ganzes, denn die unterschriebene Karte ist unteilbar. Ein Gerät oder Relay,
  das der Kontakt schon hat, MUSS holzi ohne Frage übergehen. Die bestätigte
  Vault-Identität eines vorhandenen Kontakts DARF eine Weitergabe nie ersetzen, auch nicht auf Wunsch des
  Nutzers, denn an ihr hängen Zugriffsrechte (Spec 056); eine abweichende Vault-Identität kommt nur in
  einen neuen Kontakt. Spec 051 FR-013 (eine bestätigte Vault-Identität steht nur bei einem Kontakt) und
  FR-039 (blockierte Vaults) gelten auch hier. Das Entfernen veralteter Adressen gehört nicht zu dieser
  Spec; Spec 051 FR-012 gilt weiter für Karten im Format `holzi:card:`.
- **FR-025a**: holzi MUSS jede Karte prüfen, bevor es ihre Adressen speichert, und bei jeder
  Verwendung gespeicherter Adressen erneut: Format und Grenzen, die Unterschrift gegen die Vault-
  Identität, die die Karte selbst nennt, und dass ihre gespeicherte Kennung aus ihrem unterschriebenen
  Inhalt abgeleitet ist und der Vault-Zuordnung der Zeile entspricht. Eine Karte oder gespeicherte
  Zeile, die das nicht besteht, DARF holzi weder speichern noch für Adressen verwenden. Eine Karte,
  deren Prüfung scheitert, DARF auch keine andere Angabe der Weitergabe ins Adressbuch bringen
  (FR-022).
- **FR-026**: Foto, Gruppen, Abgleichsziele, Herkunft und Abgleichsstand eines Kontakts DÜRFEN NICHT in
  die Weitergabe gehen.

**Was entfällt**

- **FR-027**: Der Einstellungsblock für die eigene Visitenkarte (Wahl des eigenen Kontakts und der
  Angaben auf der Karte), die Seite „Eigene Visitenkarte“ und die zugehörigen Vault-Einstellungen MÜSSEN
  entfallen. Die Kategorie „Kontakte“ in den Einstellungen behält den Abgleich und die Liste der
  blockierten Vaults. Eine Migration vorhandener Wahlen entfällt, holzi hat noch keine Nutzer.
- **FR-028**: Ändern sich Geräteliste oder Nostr-Relays der Vault, MUSS die gespeicherte eigene Karte
  weiter erneuert werden, soweit andere Teile von holzi sie brauchen (Spec 051 FR-010). Der Teilen-Dialog
  hängt nicht von ihr ab.

**Teilen durch Agenten**

- **FR-029**: Agenten (der eingebaute Agent und externe Agenten über den MCP-Server) MÜSSEN einen
  Kontakt, auch den Ich-Kontakt, über eine Aktion teilen können. Sie MUSS die Wirkung `destructive`
  haben (Spec 020), weil Daten die Vault verlassen und nicht zurückzuholen sind. Voraussetzung ist die
  Freigabe „Kontakte lesen“ (Spec 051 FR-030); ohne sie lehnt holzi ab, ohne zu verraten, ob der Kontakt
  existiert (Spec 051 FR-031).
- **FR-030**: Jeder Aufruf der Aktion MUSS der Nutzer einzeln bestätigen. Es DARF keine dauerhafte
  Erlaubnis („immer erlauben“) für diese Aktion geben, auch nicht für einen Agenten, der Kontakte lesen
  darf. Der Agent DARF bis zur Bestätigung keinen Inhalt des Kontakts und keine Weitergabe bekommen.
- **FR-031**: Die Bestätigung MUSS den Teilen-Dialog des Kontakts zeigen, mit der Auswahl, die der Agent
  vorschlägt (ohne Vorschlag: alles gewählt). Der Nutzer MUSS die Auswahl ändern können; der Agent
  bekommt nur die Weitergabe mit genau der bestätigten Auswahl. Ein Agent kann die Auswahl nur
  einschränken, nie um etwas erweitern, das der Dialog nicht anbietet.
- **FR-032**: Lehnt der Nutzer ab, bricht die Anfrage ab oder läuft sie ohne Antwort ab, MUSS der Agent
  eine Ablehnung ohne Inhalt bekommen. Für Erweiterungen DARF es keine Möglichkeit geben, einen Kontakt
  zu teilen (Spec 051 FR-029).

### Key Entities

- **Ich-Kontakt**: der eine Kontakt der Vault mit fester Kennung. Hat die Angaben eines Kontakts; Anzeige
  immer „Ich“; nicht löschbar; ohne Abgleichsziele.
- **Teilen-Auswahl**: die Menge der Zeilen, die der Nutzer im Dialog angehakt lässt. Lebt nur so lange,
  wie der Dialog offen ist.
- **Weitergabe**: der Text mit Version, Name, gewählten Angaben und optional einer unterschriebenen Karte.
  Nicht gespeichert; er entsteht pro Teilen neu.
- **Einlese-Entscheidung**: die Wahl des Nutzers je neuer Angabe beim Hinzufügen zu einem vorhandenen
  Kontakt: „Hinzufügen“ (Standard), „Aktualisieren“ oder „Nicht übernehmen“ (bei einfeldigen Angaben
  „Behalten“ oder „Aktualisieren“, bei holzi-Adressen „Hinzufügen“ oder „Nicht übernehmen“). Lebt nur
  in der Vorschau.
- **Teilen-Anfrage eines Agenten**: ein Aufruf der zerstörerischen Aktion zum Teilen mit Kontakt und
  optionalem Auswahlvorschlag; wartet auf die Bestätigung des Nutzers und liefert danach die Weitergabe
  oder eine Ablehnung.
- **Karte (Echtheitsnachweis der holzi-Adressen)**: wie in Spec 051 die unterschriebene Selbstauskunft
  einer Vault. Hier mit gewählten Geräten und Relays frisch unterschrieben (Ich-Kontakt) oder unverändert
  aus dem Adressbuch (fremder Kontakt).

## Success Criteria _(mandatory)_

### Measurable Outcomes

- **SC-001**: Ein Nutzer findet in einer neuen Vault den Kontakt „Ich“ ohne Einrichtung und kann seinen
  Namen und drei Angaben darin in unter 2 Minuten eintragen.
- **SC-002**: In 100 % der Fälle enthält eine Weitergabe keine abgewählte Angabe; die Angaben in einer
  mitgeschickten Originalkarte sind die einzige Ausnahme, und der Dialog sagt das vorher.
- **SC-003**: Ein Nutzer teilt einen Kontakt mit zwei abgewählten Angaben in höchstens 4 Schritten (Teilen
  wählen, abwählen, kopieren oder QR-Code zeigen).
- **SC-004**: Ein Empfänger speichert eine Weitergabe in höchstens 3 Schritten (einlesen, Vorschau
  prüfen, „neu“ oder „hinzufügen“ wählen).
- **SC-005**: Ohne Wahl des Nutzers legt das Einlesen in 100 % der Fälle keinen Kontakt an und ändert
  keinen.
- **SC-005a**: Beim Hinzufügen zu einem vorhandenen Kontakt geht in 100 % der Fälle keine Angabe verloren,
  außer einer, für die der Nutzer „Aktualisieren“ gewählt hat; bestätigte holzi-Adressen und die bestätigte
  Vault-Identität eines vorhandenen Kontakts ändert eine Weitergabe in keinem Fall.
- **SC-005b**: Jeder Kontakt ohne holzi-Adressen lässt sich teilen und einlesen; kein solcher Kontakt
  scheitert an einer fehlenden Karte.
- **SC-006**: In 100 % der Fälle mit bekannter Vault-Identität sieht der Empfänger einen nichtleeren
  Namen, auch wenn der Absender keine Namensteile gespeichert hat.
- **SC-007**: Jede Weitergabe mit Karte, deren Unterschrift nicht stimmt, weist holzi in 100 % der Fälle
  ab; kein Kontakt entsteht.
- **SC-008**: Jede Vault hat nach beliebig vielen Starts und Syncs auf jedem Gerät genau einen
  Ich-Kontakt, und kein Weg im Programm löscht ihn.
- **SC-009**: Jeder angezeigte QR-Code liefert beim Einlesen denselben Text wie der Link; passt der Text
  nicht, erscheint der Hinweis „zu groß für QR“ statt eines unlesbaren Codes.
- **SC-010**: In 100 % der Fälle bekommt ein Agent eine Weitergabe erst nach der Bestätigung des Nutzers
  und nur mit der bestätigten Auswahl; bei Ablehnung oder Zeitablauf bekommt er keinen Inhalt.
- **SC-011**: Tippt der Nutzer auf einen gültigen Link, sieht er in höchstens 2 Schritten (bei gesperrter
  Vault: Entsperren) die Vorschau; in 100 % der Fälle ist dabei nichts gespeichert.

## Assumptions

- Die Weitergabe ist unverschlüsselt. Wer den Text hat, kann ihn lesen und weitergeben. holzi verhindert
  die Weitergabe von Inhalten nicht, denn geteilte Daten gehören dem Empfänger.
- Angaben in einer Weitergabe haben keinen Echtheitsnachweis; nur die holzi-Adressen haben ihn, durch die
  Unterschrift der Karte.
- Die Notiz eines Kontakts ist wie alle anderen Zeilen vorgewählt, auf ausdrücklichen Wunsch („default
  sollte alles angewählt sein“). Der Dialog zeigt den Inhalt jeder Zeile.
- Die Höchstgröße für Link und Zwischenablage legt der Plan fest. Ohne Foto bestimmen die Zahl der Angaben
  und die Länge der Notiz die Größe.
- Das Foto eines Kontakts geht nie über Link, Zwischenablage oder QR-Code. Erst ein späterer Chat kann es
  in voller Größe mitgeben.
- Teilen durch einen Agenten läuft über dieselbe Weitergabe und denselben Dialog wie das Teilen durch den
  Nutzer; der Agent braucht dafür keine eigene Berechtigung außer „Kontakte lesen“ und der Bestätigung.
  Erweiterungen können nicht teilen. Einlesen und Ändern von Kontakten durch Agenten bleibt wie in
  Spec 051.
- „Aktualisieren“ ist eine bewusste Wahl des Nutzers; dass dabei ein vorhandener Wert verschwindet, ist
  gewollt und kein Verlust im Sinne von FR-024a. Vorgewählt ist immer die Wahl, die nichts entfernt.
- Wie holzi die bestätigten Adressen eines Kontakts speichert, wenn eine Weitergabe neue Geräte oder
  Relays bringt, ohne die bisherigen zu entfernen, legt der Plan fest (die Karte selbst ist
  unterschrieben und lässt sich nicht verändern).
- Veraltete Adressen bleiben nach einer Weitergabe stehen. Beim Verbinden mit einem Gerät des Kontakts
  prüft holzi heute dessen Geräteliste, speichert sie aber nicht in den Adressen des Kontakts. Das
  Auffrischen der Adressen aus dem Kontakt selbst, mit Löschen der überholten, ist ein eigenes Vorhaben
  (Spec 051 FR-012, Weg „Geräteliste“).
- Der Ich-Kontakt gilt für die ganze Vault, nicht je Gerät; alle eigenen Geräte teilen dieselbe Person.
- Die Namens-Regel („Namensteile, sonst `npub`, sonst leer“) gilt für jede Weitergabe, nicht nur für den
  Ich-Kontakt.
- Ein Deep Link braucht Unterstützung des Betriebssystems (Android: Intent-Filter, Desktop: Registrierung
  des Schemas bei der Installation, nur eine laufende Instanz). Wie zuverlässig ein Tippen funktioniert,
  hängt auch von der App ab, in der der Link steht; sehr lange Links kürzen manche Apps. Das Einfügen aus
  der Zwischenablage funktioniert immer.
- Die Legacy-App (haex-vault 2.x, Repository `haex-space/haex-vault-legacy`) hat schon Deep-Link-Unterstützung
  (`tauri-plugin-deep-link`, Schema `haexvault://`, Weiterleitung beim Zweitstart in `lib.rs`) und dient
  dem Plan als Vorlage. Läuft sie neben der neuen App, bleiben beide Schemas getrennt, sehen sich aber
  ähnlich.
- Der Weg „Im Chat senden“ und der Austausch von Dateien in Chats kommen mit einer späteren
  Messenger-Spec; die Weitergabe dieser Spec bleibt dort als Anhang gültig.

## Nicht im Umfang

- Direktversand eines Kontakts an einen anderen holzi-Kontakt (Zustellung zwischen Vaults, Posteingang,
  Annehmen-Abfrage): geht in die spätere Messenger-Spec auf.
- Export und Import von vCard-Dateien (`.vcf`) und CardDAV-Abgleich: ein späteres, eigenes Ziel des
  Teilen-Dialogs.
- Verschlüsselung oder Ablaufdatum einer Weitergabe.
- Ein Link, der für Personen ohne holzi eine Webseite oder einen Store öffnet (Web-Links über https).
- Das Teilen mehrerer Kontakte auf einmal oder ganzer Gruppen.
- Ein Ich-Profil im Telefon-Adressbuch.
- Teilen von Kontakten durch Erweiterungen sowie Einlesen einer Weitergabe durch Agenten.
- Das Foto eines Kontakts in der Weitergabe: Es geht erst mit dem Teilen im Chat (spätere Messenger-Spec)
  in voller Größe mit.
- Das Nachverfolgen, wem ein Kontakt weitergegeben wurde, und das Zurückrufen einer Weitergabe.
- Das automatische Auffrischen der holzi-Adressen eines Kontakts aus dem Kontakt selbst beim Verbinden
  und das Entfernen überholter Adressen (Spec 051 FR-012, Weg „Geräteliste“): ein eigenes Vorhaben.
