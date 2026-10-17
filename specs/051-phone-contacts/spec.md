# Feature Specification: Adressbuch in holzi mit Abgleich zum Telefon-Adressbuch (Android)

**Feature Branch**: `051-phone-contacts`

**Created**: 2026-10-13

**Status**: Draft

**Input**: User description: "Android-Telefonkontakte in der Vault: Holzi auf Android übernimmt alle
Kontakte aus dem Telefon-Adressbuch in die Vault-Datenbank und hält beide Seiten in beide Richtungen
synchron (Zweiwege: Änderungen am Telefon kommen in die Vault, Änderungen in holzi – auch von anderen
Geräten über den Vault-Sync – werden ins Telefon-Adressbuch zurückgeschrieben). Die Kontakte sind Teil
der synchronisierten Vault-Daten (auch auf dem Desktop sichtbar), holzi bekommt eine eigene
Kontaktansicht, Erweiterungen dürfen Kontakte per Berechtigung lesen, und externe Agenten können sie
über holzis MCP-Server mit Grant abfragen." Ergänzungen des Betreibers (2026-10-13): "ich möchte in
holzi ein eigenes Kontaktebuch haben und mit Android syncen. Bei den Kontakten auf holzi kommen noch
Nostr-Relay und iroh-Adressen dazu und vielleicht noch andere Infos." – "ich möchte aber ein
Adressbuch" – "ich möchte später auch noch Infos von den verschiedenen Messengern speichern,
PGP-Schlüssel, Serveradressen und was sonst noch so kommen wird."

## Clarifications

### Session 2026-10-13

- Q: Sollen Kontakte nur aus dem Telefon übernommen oder in beide Richtungen abgeglichen werden? → A:
  In beide Richtungen; Änderungen in holzi, auch von anderen Geräten, landen im Telefon-Adressbuch.
- Q: Wofür werden die Kontakte genutzt? → A: Sync auf alle eigenen Geräte, eigene Ansicht in holzi,
  Lesen durch Erweiterungen mit Berechtigung, Abfrage durch Agenten mit Freigabe.
- Q: Ist das Telefon die Quelle oder holzi? → A: holzi hat ein eigenes Adressbuch; Android wird damit
  abgeglichen. Kontakte in holzi haben zusätzlich holzi-Adressen (Nostr-Relays, iroh-Adressen).
- Q: Wofür sind die holzi-Adressen gedacht? → A: Sie kommen bestätigt aus der Visitenkarte der anderen
  Vault (QR-Code oder Link), und Einladungen in Spaces (027) und Datenfreigaben (028) können Empfänger
  direkt aus dem Adressbuch wählen (Option C).
- Q: Welche weiteren Angaben? → A: Später Messenger-Kennungen, PGP-Schlüssel, Serveradressen und was
  noch kommt. Diese Spec macht die Angaben eines Kontakts dafür erweiterbar; die konkreten Arten
  bringen spätere Specs.
- Q: Wie führt holzi beim ersten Abgleich Telefonkontakte mit vorhandenen Kontakten zusammen? → A:
  Eindeutige Treffer (gleicher Name und mindestens eine gleiche Telefonnummer oder E-Mail-Adresse)
  automatisch; unsichere Treffer (nur der Name oder nur eine Nummer beziehungsweise E-Mail-Adresse
  gleich) zeigt die Übersicht, und der Nutzer entscheidet je Paar.
- Q: Darf der eingebaute Agent Kontakte ohne eigene Freigabe lesen? → A: Nein, nur nach Freigabe wie
  externe Agenten; beim ersten Zugriff fragt holzi, die Antwort wird gemerkt.
- Q: Was passiert mit Einladungen von Vaults, die nicht bestätigt im Adressbuch stehen? → A: Sie
  erscheinen getrennt unter „Von Unbekannten“, ohne Benachrichtigung, mit „Annehmen“, „Verwerfen“
  und „Absender blockieren“; eine Einstellung zum Verwerfen gibt es nicht.
- Q: Wo schaltet man den Abgleich ein und wählt das Zielkonto? → A: In einer neuen Kategorie
  „Kontakte“ der Einstellungen-App (Abgleich, Zielkonto, Stand, blockierte Vaults, Inhalt der eigenen
  Visitenkarte); die App „Kontakte“ verlinkt dorthin.
- Q: Gehen alle holzi-Kontakte ins Telefon? → A: Nein. Jeder Kontakt hat eine Herkunft (woher der
  Eintrag kommt, auch bei vom Telefon übernommenen) und eigene Abgleichsziele (auf welche Telefone und
  in welches Telefonkonto er geht). In holzi angelegte Kontakte gehen standardmäßig auf kein Telefon;
  beim Anlegen wird der Nutzer gefragt. Vom Telefon übernommene haben ihr Herkunftstelefon als Ziel.

## Begriffe

- **Adressbuch**: die Kontakte einer Vault. Es gehört zu den Vault-Daten, kommt mit dem Datensync auf
  alle eigenen Geräte (Spec 024) und ist auf jedem Gerät nutzbar, auch ohne Telefon. „Kontakte“ meint
  in holzi ab dieser Spec immer das Adressbuch.
- **Kontakt**: eine Person oder Organisation im Adressbuch, beschrieben durch ihre Angaben.
- **Angabe**: ein einzelner Wert eines Kontakts mit einer **Art** (Telefonnummer, E-Mail-Adresse,
  Anschrift, Nostr-Relay …) und optional einer Bezeichnung (privat, Arbeit, eigene). Ein Kontakt kann
  mehrere Angaben derselben Art haben. Die Menge der Arten ist offen: Spätere Specs fügen weitere hinzu
  (Messenger-Kennungen, PGP-Schlüssel, Serveradressen).
- **Eigene Angabe**: eine Angabe mit frei gewählter Bezeichnung und einem Text, für alles, wofür es
  noch keine eigene Art gibt.
- **holzi-Adressen**: die Angaben, unter denen die Vault eines Kontakts erreichbar ist: ihre
  Vault-Identität (Spec 024), die Geräte ihrer Geräteliste mit deren Netzwerkkennungen (iroh-Adressen)
  und ihre Nostr-Relays. **Bestätigt** sind sie, wenn sie aus einer gültigen Visitenkarte stammen;
  von Hand eingetragene sind unbestätigt.
- **Visitenkarte**: ein Datensatz, den eine Vault über sich selbst herausgibt: Vault-Identität,
  aktuelle Geräteliste, Nostr-Relays, ein Name und auf Wunsch weitere Angaben des eigenen Kontakts,
  mit Erstellungszeit und unterschrieben mit der Vault-Identität. holzi bietet sie als Link und als
  QR-Code an.
- **Telefon-Adressbuch**: das Adressbuch des Android-Systems mit allen **Telefonkonten**, aus denen es
  Kontakte bezieht (zum Beispiel ein Google-Konto, die SIM-Karte, ein lokales Gerätekonto, ein
  Firmenkonto).
- **Abgleich**: das Übernehmen von Änderungen in beide Richtungen zwischen Adressbuch und
  Telefon-Adressbuch eines Telefons. Er ist je Gerät eingeschaltet oder aus.
- **Zuordnung**: das Wissen eines Telefons, welcher Kontakt im Adressbuch zu welchem Kontakt im
  Telefon-Adressbuch gehört. Jedes Telefon hat seine eigene.
- **Herkunft**: woher ein Kontakt stammt: in holzi angelegt (auf welchem Gerät, von wem: Nutzer oder
  Agent), vom Telefon übernommen (welches Telefon, welches Telefonkonto) oder aus einer Visitenkarte.
  Sie wird beim Anlegen festgelegt und ändert sich nicht.
- **Abgleichsziel**: ein Telefon und ein Telefonkonto darauf, in das ein Kontakt geschrieben und mit
  dem er abgeglichen wird. Ein Kontakt hat keines, eines oder mehrere. Nur Kontakte mit einem
  Abgleichsziel auf einem Telefon kommen in dessen Telefon-Adressbuch.
- **Zielkonto**: das Telefonkonto, das holzi auf einem Telefon vorschlägt, wenn ein Kontakt dieses
  Telefon als Abgleichsziel bekommt.
- Die „Kontakte“ aus haex-vault 2.x (Legacy) waren Identitäten zum Teilen (Schlüsselpaare anderer Vaults). Ihren
  Zweck übernimmt hier die Visitenkarte; die Daten aus haex-vault 2.x werden nicht übernommen.

## Beziehung zu bestehenden Specs

- **Spec 024** (Sync der eigenen Geräte): Das Adressbuch sind gewöhnliche Vault-Daten. Anlegen, Ändern
  und Löschen kommen auf allen Geräten an, gleichzeitige Änderungen desselben Felds enden überall
  gleich (FR-016, FR-017 dort). Was ein Telefon aus seinem Adressbuch übernimmt, ist für den Sync eine
  Änderung dieses Geräts. Vault-Identität, Geräteliste und Geräteschlüssel sind die aus 024; die
  Visitenkarte gibt sie nur weiter.
- **Spec 043** (Android): Das Telefon synchronisiert nur im Vordergrund und holt beim Zurückkehren nach
  (FR-018 dort). Der Abgleich folgt derselben Regel. Android-Berechtigungen werden beim ersten Gebrauch
  angefragt und eine Ablehnung wird erklärt, wie beim Mikrofon (FR-029 dort). Auf Geräten ohne
  Telefon-Adressbuch meldet holzi den Abgleich als „Auf diesem Gerät nicht verfügbar“ (FR-016 dort);
  das Adressbuch selbst gibt es dort trotzdem.
- **Spec 027** (Spaces) und **Spec 028** (Datenfreigaben): Beide schlossen ein Adressbuch und das
  Einladen ohne Link aus (027 FR-008: „holzi DARF keinen Weg anbieten, eine Einladung an eine
  eingegebene Vault-Identität zu schicken“; 027 „Nicht im Umfang“; 028 Annahmen und Klärung vom
  2026-10-01). Diese Spec führt das Adressbuch ein und erlaubt eine Einladung an einen Kontakt mit
  bestätigten holzi-Adressen (US7, FR-033 bis FR-040). Eine Einladung an eine bloß eingegebene
  Vault-Identität bleibt verboten. Inhalt, Annahme, Bestätigung durch den Admin beziehungsweise
  Eigentümer und Schlüsselverteilung bleiben wie in 027 und 028; nur der Weg der Einladung zum
  Empfänger ist neu. 027 und 028 verweisen an den betroffenen Stellen auf diese Spec.
- **Spec 026** (Sync-Server): Hat der Empfänger ein Postfach auf einem Sync-Server, darf die Zustellung
  einer Einladung es nutzen; nötig ist es nicht.
- **Spec 017** (Erweiterungs-Host): Erweiterungen lesen Kontakte über eine neue Berechtigungsart mit
  Art, Aktion und Ziel (FR-015, FR-016 dort), nicht über SQL; Kerntabellen bleiben Erweiterungen
  verschlossen (FR-025 dort).
- **Spec 034** (Passwortmanager): Vorbild für den Zugriff von Aufrufern: ein Zugang mit einer Prüfung
  für alle Aufrufer, Freigaben mit Art und Bereich, Ablehnung ohne zu verraten, ob ein Eintrag existiert
  (FR-024 bis FR-031 dort).
- **Spec 020** (Aktionen) und die geplante **Spec 021** (MCP-Server, Freigaben je Agent): Das
  Adressbuch bekommt eigene Berechtigungsbereiche für Lesen und Ändern (FR-031 in 020). Speichern,
  Erteilen und Entziehen von Freigaben für Agenten gehört zu 021; diese Spec liefert die Bereiche und
  die Prüfung.
- **Spec 023** (Einstellungen): Einstellungen speichern sich bei der Auswahl, ohne Speichern-Knopf
  (FR-021 dort).

## Reihenfolge

US1 bis US3 bilden die erste Stufe und sind allein nutzbar: das Adressbuch und der Abgleich mit dem
Telefon. US4 (Visitenkarten) braucht nur Spec 024. US5 und US6 hängen von Spec 017 beziehungsweise 021
ab, US7 von der Umsetzung von 027 oder 028; sie werden erst umgesetzt, wenn diese im Einsatz sind
(Grundsatz der Phasen in der Verfassung).

## User Scenarios & Testing _(mandatory)_

### User Story 1 - Adressbuch in holzi nutzen (Priority: P1)

Mara öffnet in holzi die App „Kontakte“, eine eingebaute App wie „Passwörter“. Sie legt einen Kontakt
an, trägt Telefonnummer, E-Mail und Anschrift ein, ergänzt eine eigene Angabe „Kundennummer“, sucht
später nach dem Namen, ändert die Nummer und löscht einen alten Kontakt. Auf ihrem Desktop und ihrem
Telefon sieht sie dasselbe Adressbuch.

**Why this priority**: Das Adressbuch ist die Grundlage für alles andere und schon allein auf dem
Desktop nützlich.

**Independent Test**: Auf einem Desktop ohne Telefon Kontakte anlegen, suchen, ändern und löschen; mit
einem zweiten eigenen Gerät prüfen, dass alles über den Datensync ankommt.

**Acceptance Scenarios**:

1. **Given** eine offene Vault, **When** Mara die App „Kontakte“ öffnet, **Then** sieht sie die Liste
   aller Kontakte alphabetisch nach Anzeigenamen mit einem Suchfeld.
2. **Given** die App „Kontakte“, **When** Mara einen Kontakt mit Name, zwei Telefonnummern (mobil,
   privat), einer E-Mail-Adresse und einer eigenen Angabe „Kundennummer: 4711“ anlegt, **Then**
   erscheint er sofort in der Liste und bleibt nach einem Neustart erhalten.
3. **Given** 500 Kontakte, **When** Mara „ann“ ins Suchfeld tippt, **Then** zeigt die Liste alle
   Kontakte, deren Name, Organisation, Telefonnummer oder E-Mail-Adresse „ann“ enthält.
4. **Given** zwei verbundene eigene Geräte, **When** Mara auf dem einen einen Kontakt ändert oder
   löscht, **Then** zeigt das andere die Änderung nach dem nächsten Sync.
5. **Given** ein geöffneter Kontakt, **When** Mara auf eine Telefonnummer, E-Mail-Adresse oder Anschrift
   tippt, **Then** bietet holzi an, sie zu kopieren, und auf dem Telefon zusätzlich, sie mit der
   passenden App des Systems zu öffnen (anrufen, Nachricht, E-Mail, Karte).
6. **Given** ein Gerät mit einer neueren holzi-Version hat bei einem Kontakt eine Angabe einer Art
   gespeichert, die Maras ältere Version nicht kennt, **When** Mara diesen Kontakt auf dem älteren
   Gerät ändert, **Then** bleibt die unbekannte Angabe erhalten und wird mit Bezeichnung und Wert
   angezeigt.

---

### User Story 2 - Telefonkontakte in holzi übernehmen und abgleichen (Priority: P1)

Mara schaltet auf ihrem Android-Telefon in holzi den Abgleich mit dem Telefon-Adressbuch ein. holzi
fragt nach der Berechtigung für Kontakte, zeigt eine Übersicht, was passieren wird, und übernimmt nach
ihrer Bestätigung alle Telefonkontakte aus allen Telefonkonten ins Adressbuch. Danach bleiben beide
Seiten abgeglichen: Ändert sie einen Kontakt im Telefon-Adressbuch (in der Telefon-App oder weil ihr
Google-Konto etwas nachgeladen hat), steht die Änderung beim nächsten Öffnen von holzi im Adressbuch;
ändert sie ihn in holzi, steht die Änderung im Telefon-Adressbuch.

**Why this priority**: Das ist der Kern des Wunsches: alle Kontakte des Telefons in der Vault, ohne sie
von Hand pflegen zu müssen.

**Independent Test**: Auf einem Telefon (Emulator) mit vorbereiteten Kontakten in zwei Telefonkonten
den Abgleich einschalten, Anzahl und Inhalt vergleichen, dann auf beiden Seiten je einen Kontakt
ändern, anlegen und löschen und das Ergebnis auf der jeweils anderen Seite prüfen.

**Acceptance Scenarios**:

1. **Given** ein Telefon mit 300 Kontakten in einem Google-Konto und 20 auf der SIM, **When** Mara den
   Abgleich einschaltet und die Berechtigung erteilt, **Then** zeigt holzi vor dem ersten Abgleich, wie
   viele Kontakte neu ins Adressbuch kommen, wie viele neu ins Telefon (Kontakte, die dieses Telefon
   schon als Abgleichsziel haben) und wie viele zusammengeführt
   werden, listet unsichere Paare zur Entscheidung auf und führt ihn erst nach ihrer Bestätigung aus.
2. **Given** der erste Abgleich ist bestätigt, **When** er fertig ist, **Then** enthält das Adressbuch
   alle 320 Kontakte mit allen Angaben, die das Telefon-Adressbuch für sie hat und die holzi kennt.
3. **Given** ein laufender Abgleich, **When** Mara im Telefon-Adressbuch eine Nummer ändert und holzi
   danach wieder in den Vordergrund holt, **Then** zeigt das Adressbuch die neue Nummer.
4. **Given** ein laufender Abgleich, **When** Mara in holzi eine E-Mail-Adresse ergänzt, **Then** steht
   sie im Telefon-Adressbuch beim selben Kontakt in dessen Telefonkonto.
5. **Given** ein laufender Abgleich, **When** Mara im Telefon-Adressbuch einen Kontakt löscht, **Then**
   ist er nach dem nächsten Abgleich auch im Adressbuch gelöscht (und damit auf allen eigenen Geräten).
6. **Given** Mara verweigert die Berechtigung, **When** sie den Abgleich einschalten will, **Then**
   bleibt er aus, und holzi erklärt, wie sie die Berechtigung später in den Android-Einstellungen
   erteilt.
7. **Given** ein laufender Abgleich, **When** Mara ihn ausschaltet, **Then** bleiben alle Kontakte auf
   beiden Seiten erhalten; holzi übernimmt nur keine Änderungen mehr.

---

### User Story 3 - Änderungen von anderen Geräten landen im Telefon (Priority: P1)

Mara ändert am Desktop die Anschrift eines Kontakts, der von ihrem Telefon stammt, und legt zwei
neue an. Beim ersten fragt holzi, ob er auch aufs Telefon soll; sie wählt ihr Telefon, und holzi
schlägt dessen Zielkonto vor. Den zweiten lässt sie, wie vorgeschlagen, nur in holzi. Sobald ihr
Telefon die Änderungen über den Datensync bekommen hat, schreibt holzi die geänderte Anschrift und den
ersten neuen Kontakt ins Telefon-Adressbuch; der zweite bleibt nur in holzi.

**Why this priority**: Ohne diese Richtung wäre das Telefon nur eine Quelle, und das Adressbuch in
holzi wäre nicht das führende Adressbuch, das der Betreiber möchte.

**Independent Test**: Desktop und Telefon verbunden, Abgleich auf dem Telefon an; am Desktop einen
Kontakt ändern und zwei anlegen (mit und ohne Abgleichsziel); auf dem Telefon nach dem Sync im
Telefon-Adressbuch prüfen.

**Acceptance Scenarios**:

1. **Given** Abgleich an, Zielkonto „Google (mara@…)“, **When** Mara am Desktop einen Kontakt anlegt,
   als Abgleichsziel ihr Telefon mit dem vorgeschlagenen Konto wählt und das Telefon synchronisiert,
   **Then** steht der Kontakt im Telefon-Adressbuch im Google-Konto.
2. **Given** Abgleich an, **When** Mara in holzi einen Kontakt anlegt und die Frage nach dem Telefon
   unverändert lässt, **Then** bleibt er nur in holzi, und als Herkunft steht „in holzi angelegt“ mit
   dem Gerät.
3. **Given** ein Kontakt, der im Telefon in einem Firmenkonto liegt, **When** Mara ihn am Desktop
   ändert, **Then** schreibt holzi die Änderung in dieses Firmenkonto, nicht ins Zielkonto.
4. **Given** ein Kontakt mit Abgleichsziel Telefon, **When** Mara das Ziel entfernt, **Then** fragt
   holzi, ob der Kontakt im Telefon gelöscht oder dort nur nicht mehr abgeglichen werden soll.
5. **Given** 40 Kontakte ohne Abgleichsziel, **When** Mara sie in der Liste auswählt (oder eine Gruppe
   wählt) und ihnen ihr Telefon als Ziel gibt, **Then** schreibt holzi alle 40 beim nächsten Abgleich
   ins Telefon.
6. **Given** das Telefon war während der Änderungen am Desktop aus, **When** Mara holzi auf dem Telefon
   wieder öffnet, **Then** holt holzi Sync und Abgleich nach, ohne dass sie etwas tun muss.

---

### User Story 4 - Visitenkarten austauschen (Priority: P2)

Ben nutzt ebenfalls holzi. Er öffnet in der App „Kontakte“ seine Visitenkarte und zeigt Mara den
QR-Code. Mara scannt ihn mit ihrem Telefon, sieht eine Vorschau (Name, Vault-Identität, Zahl der
Geräte) und fügt die Karte ihrem vorhandenen Kontakt „Ben“ hinzu, den sie aus dem Telefon kennt. Bens
Kontakt hat danach bestätigte holzi-Adressen; Mara sieht sie auf allen ihren Geräten, ins
Telefon-Adressbuch kommen sie nicht. Fügt Ben später ein Gerät hinzu, aktualisiert Maras holzi die
Adressen, sobald es eine neuere Karte von Ben erhält.

**Why this priority**: Bestätigte holzi-Adressen sind die Voraussetzung für Einladungen aus dem
Adressbuch (US7) und für alles, was spätere Specs mit ihnen vorhaben. Für US1 bis US3 sind sie nicht
nötig.

**Independent Test**: Zwei Vaults; die eine zeigt ihre Visitenkarte als Link, die andere liest ihn ein,
legt einen Kontakt an beziehungsweise ergänzt einen vorhandenen; danach eine veränderte (falsch
unterschriebene) und eine ältere Karte einlesen und prüfen, dass beide abgewiesen werden.

**Acceptance Scenarios**:

1. **Given** Bens Vault, **When** Ben „Meine Visitenkarte“ öffnet, **Then** zeigt holzi Link und
   QR-Code mit seinem Namen und den Angaben, die er für die Karte ausgewählt hat (Standard: nur der
   Name).
2. **Given** Bens Karte, **When** Mara sie einliest, **Then** prüft holzi die Unterschrift und zeigt eine
   Vorschau mit „Neuer Kontakt“ und „Zu Kontakt hinzufügen …“; erst nach ihrer Wahl wird gespeichert.
3. **Given** eine veränderte oder falsch unterschriebene Karte, **When** Mara sie einliest, **Then**
   weist holzi sie mit einem Hinweis ab und speichert nichts.
4. **Given** Bens Kontakt mit bestätigten holzi-Adressen, **When** Maras holzi eine neuere gültige Karte
   derselben Vault-Identität erhält, **Then** ersetzt sie die holzi-Adressen; eine ältere Karte ändert
   nichts.
5. **Given** ein Kontakt mit von Hand eingetragenen holzi-Adressen, **When** Mara ihn ansieht, **Then**
   sind sie als „unbestätigt“ gekennzeichnet.
6. **Given** ein abgeglichener Kontakt mit holzi-Adressen, **When** Mara ihn im Telefon-Adressbuch
   ändert, **Then** bleiben seine holzi-Adressen im Adressbuch erhalten.

---

### User Story 5 - Erweiterungen lesen Kontakte mit Berechtigung (Priority: P2)

Die Erweiterung haex-mail möchte beim Schreiben einer Mail Empfänger aus dem Adressbuch vorschlagen.
Mara erteilt ihr die Berechtigung, Kontakte zu lesen. haex-mail bekommt Kontakte, kann sie aber nicht
ändern; eine Erweiterung ohne Berechtigung bekommt nichts.

**Why this priority**: Erst die Erweiterungen machen das Adressbuch im Alltag nützlich (Mail,
Kalender), aber das Adressbuch selbst funktioniert ohne sie.

**Independent Test**: Eine Test-Erweiterung fragt Kontakte ab, einmal ohne, einmal mit erteilter
Berechtigung, einmal mit Berechtigung nur für eine Gruppe.

**Acceptance Scenarios**:

1. **Given** eine Erweiterung, die das Lesen von Kontakten im Manifest erklärt, **When** Mara sie
   installiert, **Then** zeigt der Installationsdialog diese Berechtigung wie jede andere.
2. **Given** eine Erweiterung ohne erteilte Berechtigung, **When** sie Kontakte abfragt, **Then** fragt
   holzi Mara oder lehnt ab, je nach Zustand der Berechtigung (Spec 017), und die Erweiterung erfährt
   bei einer Ablehnung nichts über das Adressbuch.
3. **Given** eine Berechtigung nur für die Gruppe „Arbeit“, **When** die Erweiterung Kontakte abfragt,
   **Then** bekommt sie nur Kontakte dieser Gruppe.

---

### User Story 6 - Agenten fragen Kontakte ab (Priority: P3)

Maras externer Agent fragt über holzis MCP-Server nach der Telefonnummer von Ben. Hat Mara diesem
Agenten Lesen von Kontakten freigegeben, bekommt er die Antwort; sonst nicht. Der eingebaute Agent im
Chat braucht dieselbe Freigabe.

**Why this priority**: Hängt vom MCP-Server (Spec 021) ab, den es noch nicht gibt. Diese Spec stellt die
Bereiche und die Prüfung bereit, damit 021 sie ohne Umbau anbieten kann.

**Independent Test**: Die Kontakt-Aktionen über den Weg des eingebauten Agenten aufrufen, mit und ohne
Freigabe; die Aktionen sind so beschrieben, dass 021 sie veröffentlichen kann (Spec 032 FR-001).

**Acceptance Scenarios**:

1. **Given** ein Agent ohne Freigabe für Kontakte, **When** er Kontakte sucht, **Then** lehnt holzi ab,
   ohne zu verraten, ob es passende Kontakte gibt.
2. **Given** ein Agent mit Freigabe „Lesen“, **When** er nach „Ben“ sucht, **Then** bekommt er die
   passenden Kontakte; einen Kontakt ändern darf er nicht.
3. **Given** ein Agent mit Freigabe „Lesen und Schreiben“, **When** er eine Telefonnummer ändert,
   **Then** wird sie wie eine Änderung in holzi auf alle Geräte und ins Telefon-Adressbuch übernommen.

---

### User Story 7 - Aus dem Adressbuch einladen (Priority: P3)

Mara will Ben ihren Kalender „Familie“ freigeben (Spec 028). Statt einen Link zu kopieren, wählt sie
im Einladen-Dialog „Aus dem Adressbuch“ und dort Ben, dessen holzi-Adressen bestätigt sind. holzi
stellt Ben die Einladung zu; bei Ben erscheint sie wie eine über einen Link geöffnete Einladung, mit
„Annehmen“ und „Verwerfen“. Der Rest läuft wie bisher: Ben nimmt an, Mara bestätigt, Ben bekommt
Zugang. Für Spaces (Spec 027) gilt dasselbe.

**Why this priority**: Der Betreiber wünscht es (Option C), es hängt aber von der Umsetzung von 027
oder 028 ab.

**Independent Test**: Zwei Vaults, die ihre Visitenkarten getauscht haben; eine lädt die andere aus
dem Adressbuch zu einer Datenfreigabe ein; Zustellung, Annahme und Bestätigung prüfen, auch wenn der
Empfänger beim Senden offline war.

**Acceptance Scenarios**:

1. **Given** Ben mit bestätigten holzi-Adressen, **When** Mara ihn im Einladen-Dialog aus dem Adressbuch
   wählt und Name, Fähigkeiten und Gültigkeit festlegt, **Then** schickt holzi die Einladung an Bens
   Geräte und zeigt bei Mara „zugestellt“, sobald eines sie erhalten hat.
2. **Given** ein Kontakt ohne oder mit nur unbestätigten holzi-Adressen, **When** Mara den Dialog
   öffnet, **Then** ist er dort nicht wählbar, und holzi erklärt, dass seine Visitenkarte fehlt.
3. **Given** Ben ist beim Senden offline, **When** eines seiner Geräte vor Ablauf der Einladung online
   kommt, **Then** erhält es die Einladung, und sie erscheint auf allen seinen Geräten.
4. **Given** eine Einladung von einer Vault, die nicht mit bestätigter Visitenkarte in Bens Adressbuch
   steht, **When** sie ankommt, **Then** zeigt holzi sie getrennt unter „Von Unbekannten“, ohne
   Benachrichtigung, mit „Annehmen“, „Verwerfen“ und „Absender blockieren“.
5. **Given** Ben hat eine Vault blockiert, **When** sie ihn erneut einlädt, **Then** verwirft holzi die
   Einladung ohne Anzeige und ohne Antwort an den Absender.
6. **Given** eine zugestellte Einladung, **When** Mara sie zurückzieht, **Then** verschwindet sie bei Ben
   wie eine zurückgezogene Link-Einladung (027 FR-013).

---

### Edge Cases

- **Massenlöschung**: Verschwinden bei einem Abgleich auf einmal viele Kontakte aus dem
  Telefon-Adressbuch (Konto entfernt, Telefon zurückgesetzt, Berechtigung entzogen, andere App räumt
  auf), löscht holzi sie nicht still aus dem Adressbuch, sondern fragt Mara erst (FR-016). Dasselbe
  gilt in der anderen Richtung, wenn über den Sync viele Löschungen auf einmal ankommen.
- **Telefonkonto entfernt**: Entfernt Mara ein Google-Konto vom Telefon, verschwinden dessen Kontakte
  aus dem Telefon-Adressbuch; das ist keine Löschung im Adressbuch. Die Kontakte bleiben, ihre
  Zuordnung auf diesem Telefon entfällt.
- **Zweites Telefon mit demselben Google-Konto**: Schaltet Mara den Abgleich auf einem zweiten Telefon
  ein, auf dem dieselben Kontakte über das Google-Konto schon liegen, führt holzi sie mit den
  vorhandenen Kontakten zusammen, statt Doppel anzulegen. Ändert Google danach einen Kontakt auf beiden
  Telefonen, kommt dieselbe Änderung zweimal an und ergibt dasselbe Ergebnis.
  Zusammengeführte Kontakte haben danach beide Telefone als Abgleichsziel.
- **Zwei Telefone mit verschiedenen Konten**: Ein Kontakt vom ersten Telefon kommt nicht von selbst
  auf das zweite; dafür muss Mara ihm das zweite Telefon als Abgleichsziel geben.
- **Ziel auf einem Telefon ohne Abgleich**: Schaltet Mara auf einem Telefon den Abgleich aus, bleiben
  die Abgleichsziele der Kontakte bestehen, und das Telefon ist für neue Ziele nicht mehr wählbar.
  Schaltet sie ihn wieder ein, gelten die alten Ziele wieder.
- **Gleichzeitige Änderung**: Ändert Mara dasselbe Feld im Telefon-Adressbuch und an einem anderen
  Gerät in holzi, endet es auf allen Geräten und im Telefon gleich (Regel von Spec 024). Verschiedene
  Felder desselben Kontakts bleiben beide erhalten.
- **Löschen gegen Ändern**: Löscht Mara einen Kontakt auf einem Gerät, während er auf einem anderen
  geändert wird, gilt die Löschung überall (Spec 024 FR-017), auch im Telefon-Adressbuch.
- **Nur lesbares Telefonkonto**: Manche Telefonkonten lassen keine Änderungen durch andere Apps zu.
  Eine Änderung aus holzi an so einem Kontakt bleibt im Adressbuch; holzi zeigt beim Kontakt, dass sie
  im Telefon nicht ankommen konnte.
- **Vom Telefon zusammengeführte Kontakte**: Android zeigt eine Person, die in mehreren Telefonkonten
  vorkommt (zum Beispiel Google-Konto und ein Messenger), als einen Kontakt. In holzi ist sie ebenfalls
  ein Kontakt; Angaben, die nur ein Messenger-Konto mit eigenem Format beisteuert, übernimmt holzi in
  dieser Spec nicht.
- **Unbekannte Angaben im Telefon**: Angaben im Telefon-Adressbuch, die holzi nicht kennt (eigene Felder
  anderer Apps), bleiben dort unverändert; holzi überschreibt oder löscht sie beim Zurückschreiben
  nicht.
- **Unbekannte Arten in holzi**: Eine Angabe einer Art, die erst eine neuere holzi-Version kennt, bleibt
  auf älteren Geräten bei jeder Änderung des Kontakts erhalten und wird dort schreibgeschützt mit
  Bezeichnung und Wert gezeigt.
- **Große Fotos**: Ein Kontaktfoto über der Grenze aus FR-005 verkleinert holzi beim Übernehmen; das
  Original im Telefon bleibt, solange niemand in holzi das Foto ändert.
- **Vault zu, holzi im Hintergrund**: Solange holzi nicht im Vordergrund ist oder die Vault geschlossen
  ist, passiert nichts. Änderungen aus dieser Zeit holt holzi beim nächsten Öffnen nach.
- **Mehrere Vaults auf einem Telefon**: Jede Vault hat ihren eigenen Abgleich mit eigener Zuordnung.
  Schaltet Mara ihn in zwei Vaults ein, landen Kontakte der einen über das Telefon-Adressbuch auch in der
  anderen; holzi weist beim Einschalten darauf hin, wenn auf diesem Telefon schon eine andere Vault
  abgleicht.
- **Abgleich aus und wieder an**: Beim erneuten Einschalten führt holzi wie beim ersten Mal zusammen und
  zeigt wieder die Übersicht; es entstehen keine Doppel.
- **Berechtigung später entzogen**: holzi meldet beim nächsten Abgleich, dass die Berechtigung fehlt,
  und hält den Abgleich an; das Adressbuch bleibt unverändert (keine Massenlöschung).
- **Desktop**: Den Abgleich gibt es dort nicht; die Einstellung zeigt „Auf diesem Gerät nicht
  verfügbar“. Adressbuch, Visitenkarten und Einladen funktionieren voll.
- **Visitenkarte auf einem verknüpften Gerät**: Nur Hauptgeräte können mit der Vault-Identität
  unterschreiben (Spec 024). Ein verknüpftes Gerät zeigt die neueste Karte, die ein Hauptgerät erstellt
  hat; gibt es noch keine, erklärt es, dass ein Hauptgerät sie erstellen muss.
- **Eigene Karte eingelesen**: Liest Mara ihre eigene Visitenkarte ein, weist holzi sie mit einem Hinweis
  ab.
- **Dieselbe Vault-Identität bei zwei Kontakten**: holzi lässt eine bestätigte Vault-Identität nur bei
  einem Kontakt zu und bietet beim Einlesen an, die Karte dem Kontakt hinzuzufügen, der sie schon hat.
- **Veraltete Karte beim Einladen**: Stehen auf Bens Karte nur noch Geräte, die Ben inzwischen entfernt
  hat, kann holzi die Einladung nicht zustellen; Mara sieht „nicht zustellbar“ mit dem Hinweis, Bens
  aktuelle Karte einzulesen, und kann weiter einen Link verwenden.
- **Sync über das Telefon und den Desktop gleichzeitig**: Liest Mara eine Karte am Desktop ein, während
  ihr Telefon abgleicht, kommen die holzi-Adressen über den Datensync aufs Telefon und bleiben dort
  im Adressbuch, ohne ins Telefon-Adressbuch zu wandern.

## Requirements _(mandatory)_

### Functional Requirements

**Adressbuch**

- **FR-001**: holzi MUSS ein Adressbuch als Teil der Vault-Daten führen, das über den Datensync auf alle
  eigenen Geräte kommt (Spec 024) und auf jeder Plattform von holzi nutzbar ist.
- **FR-002**: holzi MUSS eine eingebaute App „Kontakte“ im Window Manager anbieten, mit Liste, Suche,
  Ansicht, Anlegen, Ändern und Löschen von Kontakten. Löschen MUSS bestätigt werden.
- **FR-003**: holzi MUSS mindestens diese Arten von Angaben kennen: Namensteile (Vorname, Nachname,
  weitere Namen, Präfix, Suffix, Spitzname) und Anzeigename; Telefonnummern, E-Mail-Adressen,
  Postanschriften, Webseiten, Messenger-Kennungen und SIP-Adressen, jeweils mit Bezeichnung (privat,
  Arbeit, mobil, andere oder eigene); Organisation und Position; Geburtstag und weitere Daten mit
  Bezeichnung; Beziehungen zu anderen Personen; Notiz; Foto; Gruppen; holzi-Adressen (FR-008); eigene
  Angaben. Mehrere Angaben derselben Art MÜSSEN möglich sein, wo es für die Art sinnvoll ist.
- **FR-004**: Die Suche MUSS Anzeigename, Namensteile, Organisation, Telefonnummern und E-Mail-Adressen
  durchsuchen, ohne Groß- und Kleinschreibung zu unterscheiden; Telefonnummern MÜSSEN auch ohne
  Leerzeichen, Klammern und Bindestriche gefunden werden.
- **FR-005**: holzi MUSS Kontaktfotos auf eine feste Höchstgröße begrenzen, damit das Adressbuch den
  Sync nicht mit großen Bildern belastet; der Plan legt die Grenze fest.
- **FR-006**: Telefonnummern, E-Mail-Adressen und Anschriften MÜSSEN sich kopieren und auf dem Telefon
  mit der passenden App des Systems öffnen lassen.

**Erweiterbare Angaben**

- **FR-007**: Die Arten von Angaben MÜSSEN erweiterbar sein: Eine spätere holzi-Version MUSS neue Arten
  (zum Beispiel Messenger-Kennungen mit Dienst und Prüfstatus, PGP-Schlüssel, Serveradressen) einführen
  können, ohne vorhandene Kontakte umzuschreiben. Eine Angabe einer Art, die das Gerät nicht kennt, MUSS
  bei jeder Änderung, jedem Sync und jedem Abgleich unverändert erhalten bleiben und schreibgeschützt
  mit Bezeichnung und Wert angezeigt werden. Eigene Angaben (frei gewählte Bezeichnung und Text) MÜSSEN
  sich an jedem Kontakt anlegen lassen.

**holzi-Adressen und Visitenkarten**

- **FR-008**: Ein Kontakt MUSS holzi-Adressen haben können: höchstens eine Vault-Identität, die Geräte
  ihrer Geräteliste mit Netzwerkkennungen (iroh-Adressen) und beliebig viele Nostr-Relays. holzi MUSS
  sie als bestätigt (aus einer gültigen Visitenkarte) oder unbestätigt (von Hand eingetragen)
  kennzeichnen und das Format von Hand eingetragener Angaben beim Speichern prüfen.
- **FR-009**: holzi MUSS für die eigene Vault eine Visitenkarte als Link und als QR-Code anbieten. Sie
  MUSS Vault-Identität, aktuelle Geräteliste, Nostr-Relays, Namen, Erstellungszeit und die Angaben des
  eigenen Kontakts enthalten, die der Nutzer für die Karte gewählt hat (Standard: nur der Name), und
  MUSS mit der Vault-Identität unterschrieben sein. Eine Karte DARF keinen Schlüssel und kein Geheimnis
  enthalten.
- **FR-010**: Ändern sich Geräteliste oder Nostr-Relays der eigenen Vault, MUSS ein Hauptgerät eine
  neue Visitenkarte erstellen; ältere bleiben gültig, werden aber von der neueren ersetzt.
- **FR-011**: holzi MUSS eine Visitenkarte als Link, über die Zwischenablage und per Kamera als QR-Code
  einlesen können. Vor jeder Anzeige MUSS es die Unterschrift prüfen; eine ungültige, veränderte oder
  eigene Karte MUSS es mit einem Hinweis abweisen. Nach einer Vorschau MUSS der Nutzer wählen: neuer
  Kontakt oder Hinzufügen zu einem vorhandenen Kontakt; erst dann wird gespeichert.
- **FR-012**: Erhält holzi auf irgendeinem Weg (erneutes Einlesen, Einladung, Annahme, Mitgliederliste
  aus 027 oder 028) eine gültige Karte oder Geräteliste derselben Vault-Identität, die neuer ist als die
  gespeicherte, MUSS es die bestätigten holzi-Adressen des Kontakts damit ersetzen. Eine ältere DARF
  nichts ändern.
- **FR-013**: Eine bestätigte Vault-Identität DARF nur bei einem Kontakt stehen.
- **FR-014**: holzi-Adressen und eigene Angaben DÜRFEN NICHT ins Telefon-Adressbuch geschrieben werden,
  und ein Abgleich DARF sie NICHT verändern oder löschen. Für Arten, die spätere Specs einführen, legen
  diese fest, ob sie ins Telefon gehen; ohne Festlegung gehen sie nicht dorthin.

**Abgleich mit dem Telefon-Adressbuch**

- **FR-015**: Auf Android MUSS sich der Abgleich je Gerät und Vault ein- und ausschalten lassen; die
  Auswahl speichert sich sofort (Spec 023 FR-021). Standard ist „Aus“. Auf Geräten ohne
  Telefon-Adressbuch MUSS er als „Auf diesem Gerät nicht verfügbar“ erscheinen.
- **FR-015a**: Die Einstellungen-App MUSS eine Kategorie „Kontakte“ bekommen mit: Abgleich (an/aus,
  Zielkonto, Stand des letzten Abgleichs, nicht angekommene Änderungen), Inhalt der eigenen
  Visitenkarte (FR-009) und Liste der blockierten Vaults (FR-039). Die App „Kontakte“ MUSS dorthin
  verlinken.
- **FR-016**: Würde ein Abgleich mehr als 10 Kontakte und zugleich mehr als 10 % der mit diesem Telefon
  abgeglichenen Kontakte auf einmal löschen (in welcher Richtung auch immer), MUSS holzi das vorher anzeigen und den Nutzer entscheiden lassen: löschen oder die
  betroffenen Kontakte behalten (im Adressbuch beziehungsweise neu ins Telefon schreiben).
- **FR-017**: Beim Einschalten MUSS holzi die Android-Berechtigung zum Lesen und Schreiben von Kontakten
  anfragen; bei Ablehnung bleibt der Abgleich aus, und holzi erklärt, wie sie später erteilt wird (wie
  Spec 043 FR-029).
- **FR-018**: Beim Einschalten MUSS der Nutzer das Zielkonto aus den beschreibbaren Telefonkonten
  wählen; holzi schlägt das Standardkonto des Telefons vor. Das Zielkonto lässt sich später ändern;
  bereits geschriebene Kontakte bleiben dabei in ihrem Konto.
- **FR-018a**: Jeder Kontakt MUSS eine Herkunft haben, die holzi beim Anlegen festlegt und in der
  Ansicht des Kontakts zeigt: in holzi angelegt (Gerät; Nutzer oder Agent), vom Telefon übernommen
  (Telefon und Telefonkonto) oder aus einer Visitenkarte. Sie DARF sich danach nicht ändern, auch nicht
  durch Zusammenführen (der Kontakt behält die Herkunft des älteren Eintrags).
- **FR-018b**: Jeder Kontakt MUSS eigene Abgleichsziele haben. Vom Telefon übernommene Kontakte haben
  ihr Herkunftstelefon mit ihrem Telefonkonto als Ziel; beim Zusammenführen bekommt ein Kontakt das
  Telefon, auf dem er schon liegt, als weiteres Ziel. In holzi angelegte, aus einer Visitenkarte oder
  von einem Agenten angelegte Kontakte haben standardmäßig KEIN Ziel.
- **FR-018c**: Beim Anlegen eines Kontakts in holzi MUSS der Nutzer gefragt werden, ob er auf Telefone
  der Vault soll (Auswahl der Telefone, je Telefon dessen Zielkonto vorgeschlagen); vorausgewählt ist
  „Nur in holzi“. Abgleichsziele MÜSSEN sich später je Kontakt und für mehrere ausgewählte Kontakte
  oder eine Gruppe auf einmal ändern lassen. Wählbar sind alle Telefone der Vault, die den Abgleich
  eingeschaltet haben. Hat kein Telefon der Vault den Abgleich eingeschaltet, entfällt die Frage.
- **FR-018d**: Entfernt der Nutzer ein Abgleichsziel, MUSS holzi fragen, ob der Kontakt in diesem
  Telefon gelöscht oder dort nur nicht mehr abgeglichen werden soll.
- **FR-019**: Vor dem ersten Abgleich nach dem Einschalten MUSS holzi zusammenführen und eine Übersicht
  zeigen (neu ins Adressbuch, neu ins Telefon, zusammengeführt, unsicher) und erst nach Bestätigung
  schreiben. Automatisch zusammengeführt werden Kontakte mit gleichem Namen und mindestens einer
  gleichen Telefonnummer oder E-Mail-Adresse. Unsichere Treffer (nur der Name gleich, oder nur eine
  Telefonnummer beziehungsweise E-Mail-Adresse gleich) MUSS die Übersicht als Paare zeigen, und der
  Nutzer entscheidet je Paar „zusammenführen“ oder „getrennt lassen“ (Standard: getrennt lassen). Wie
  Namen und Nummern für den Vergleich vereinheitlicht werden, legt der Plan fest.
- **FR-020**: Bei eingeschaltetem Abgleich MUSS holzi Änderungen im Telefon-Adressbuch (Anlegen, Ändern,
  Löschen, aus jedem Telefonkonto) ins Adressbuch übernehmen, solange holzi im Vordergrund und die
  Vault offen ist, und Änderungen aus der Zeit davor beim nächsten Öffnen oder Zurückkehren nachholen
  (Spec 043 FR-018).
- **FR-021**: Bei eingeschaltetem Abgleich MUSS holzi Änderungen im Adressbuch, egal von welchem Gerät
  oder Aufrufer, ins Telefon-Adressbuch schreiben, aber nur für Kontakte mit diesem Telefon als
  Abgleichsziel: Änderungen und Löschungen in das Telefonkonto, in dem der Kontakt liegt; ein Kontakt,
  der das Telefon neu als Ziel bekommt, in das Telefonkonto dieses Ziels. Kontakte ohne Ziel auf diesem
  Telefon DARF holzi dort nicht schreiben.
- **FR-022**: Beim Zurückschreiben DARF holzi nur Angaben ändern, die es kennt und die sich geändert
  haben; andere Angaben des Kontakts im Telefon-Adressbuch MÜSSEN unverändert bleiben.
- **FR-023**: Ein Abgleich DARF keine Angabe verlieren: Ein Kontakt, der aus dem Telefon ins Adressbuch
  und wieder zurück kommt, MUSS im Telefon alle Angaben behalten, die er vorher hatte.
- **FR-024**: Jedes Telefon MUSS seine eigene Zuordnung haben; dass ein Kontakt auf einem Telefon
  zugeordnet ist, DARF NICHT beeinflussen, wie ein anderes Telefon ihn zuordnet.
- **FR-025**: Kann eine Änderung nicht ins Telefon geschrieben werden (Konto nur lesbar, Konto entfernt,
  Berechtigung entzogen), MUSS sie im Adressbuch bleiben, und holzi MUSS beim Kontakt beziehungsweise
  beim Abgleich anzeigen, was nicht ankam und warum.
- **FR-026**: Ausschalten des Abgleichs DARF auf keiner Seite Kontakte löschen, und das Entfernen eines
  Telefonkontos vom Telefon DARF NICHT als Löschung seiner Kontakte im Adressbuch gelten.
- **FR-027**: Gleicht auf diesem Telefon schon eine andere Vault ab, MUSS holzi beim Einschalten darauf
  hinweisen, dass Kontakte dadurch zwischen den Vaults wandern.

**Zugriff durch Erweiterungen und Agenten**

- **FR-028**: Alle Aufrufer (eigene Oberfläche, Erweiterungen, eingebauter Agent, externe Agenten,
  holzi-Funktionen) MÜSSEN über denselben Zugang mit derselben Prüfung auf Kontakte zugreifen (wie
  Spec 034 FR-024). Die eigene Oberfläche braucht keine Freigabe (wie 034 FR-031).
- **FR-029**: Für Erweiterungen MUSS es eine Berechtigungsart für Kontakte geben, mit der Aktion „Lesen“
  und dem Ziel „alle Kontakte“ oder „Kontakte einer Gruppe“; sie folgt den Regeln von Spec 017
  (erklärt, erteilt, verweigert, fragen). Erweiterungen DÜRFEN Kontakte in dieser Spec NICHT ändern.
- **FR-030**: Für Agenten MUSS es Berechtigungsbereiche „Kontakte lesen“ und „Kontakte ändern“ geben
  (Spec 020 FR-031), mit demselben Ziel wie FR-029. Ohne Freigabe hat kein Agent Zugriff, auch der
  eingebaute nicht. Greift der eingebaute Agent ohne Freigabe zu, MUSS holzi den Nutzer fragen
  (einmal erlauben, immer erlauben, ablehnen) und eine dauerhafte Antwort merken; sie lässt sich
  später entziehen.
- **FR-031**: Eine Ablehnung DARF NICHT verraten, ob es passende Kontakte gibt (wie Spec 034 FR-029).
- **FR-032**: Die Kontakt-Aktionen MÜSSEN so beschrieben sein, dass Spec 021 sie ohne Umbau über den
  MCP-Server anbieten kann (Spec 032 FR-001).

**Einladen aus dem Adressbuch**

- **FR-033**: Wo Spec 027 oder 028 einen Einladungslink anbietet, MUSS holzi zusätzlich „Aus dem
  Adressbuch“ anbieten. Wählbar sind nur Kontakte mit bestätigter Vault-Identität; eine Einladung an
  eine von Hand eingetragene oder eingegebene Vault-Identität DARF es weiterhin NICHT geben.
- **FR-034**: Eine Einladung aus dem Adressbuch MUSS dieselben Angaben, dieselbe Unterschrift und
  dieselbe Gültigkeit haben wie ein Einladungslink (027 FR-008); Annahme, Bestätigung durch Admin
  beziehungsweise Eigentümer, Zurückziehen und Schlüsselverteilung MÜSSEN wie bei einem Link ablaufen.
  Das Einmalgeheimnis gilt nur für den gewählten Empfänger.
- **FR-035**: holzi MUSS die Einladung an jedes Gerät der zuletzt bekannten Geräteliste des Empfängers
  zustellen, verschlüsselt für dieses Gerät, über die Nostr-Relays des Empfängers oder, wenn vorhanden,
  sein Postfach (Spec 026). Ein Nostr-Relay oder Sync-Server DARF dabei weder den Inhalt noch die
  Vault-Identität des Absenders erfahren.
- **FR-036**: Die Einladung MUSS ankommen, wenn eines der Geräte des Empfängers vor ihrem Ablauf online
  ist, auch wenn beim Senden keines online war. Der Absender MUSS den Stand sehen: „ausstehend“,
  „zugestellt“, „nicht zustellbar“ (mit Grund) und danach die Stände aus 027 und 028.
- **FR-037**: Beim Empfänger MUSS eine zugestellte Einladung wie eine über einen Link geöffnete
  erscheinen (027 FR-010) und dieselben Wahlmöglichkeiten bieten.
- **FR-038**: Einladungen von Vaults, die nicht mit bestätigter Vault-Identität im Adressbuch des
  Empfängers stehen, MUSS holzi getrennt unter „Von Unbekannten“ zeigen, ohne Benachrichtigung, mit der
  zusätzlichen Wahl „Absender blockieren“.
- **FR-039**: Einladungen blockierter Vaults MUSS holzi ohne Anzeige und ohne Antwort verwerfen. Die
  Liste der blockierten Vaults gehört zu den Vault-Daten und lässt sich ansehen und bearbeiten.
- **FR-040**: Das Verwerfen einer zugestellten Einladung DARF wie bei einem Link nichts an den Absender
  schicken (027 FR-012).

### Key Entities

- **Kontakt**: eine Person oder Organisation im Adressbuch mit Herkunft und Abgleichszielen; gehört
  zur Vault und wird synchronisiert.
- **Abgleichsziel**: Telefon und Telefonkonto, in das ein Kontakt geschrieben wird.
- **Angabe**: ein Wert eines Kontakts mit Art und optional Bezeichnung; die Menge der Arten ist offen
  (FR-007).
- **holzi-Adressen**: Vault-Identität, Geräte mit Netzwerkkennungen und Nostr-Relays eines Kontakts,
  bestätigt oder unbestätigt, mit Erstellungszeit der Karte, aus der sie stammen.
- **Visitenkarte**: die unterschriebene Selbstauskunft einer Vault (FR-009); die eigene wird erstellt,
  fremde werden eingelesen.
- **Gruppe**: ein Name, dem Kontakte zugeordnet sind; Ziel von Berechtigungen und Freigaben.
- **Abgleich eines Telefons**: je Gerät und Vault: an oder aus, Zielkonto, Stand des letzten Abgleichs
  und Liste der nicht angekommenen Änderungen.
- **Zuordnung**: je Telefon, welcher Kontakt im Adressbuch zu welchem Kontakt (und Telefonkonto) im
  Telefon-Adressbuch gehört.
- **Zugestellte Einladung**: eine Einladung aus 027 oder 028 mit Empfänger-Kontakt und Zustellstand.
- **Blockierte Vault**: eine Vault-Identität, deren Einladungen verworfen werden.

## Success Criteria _(mandatory)_

### Measurable Outcomes

- **SC-001**: Der erste Abgleich von 2.000 Telefonkontakten ist auf einem Mittelklasse-Telefon nach der
  Bestätigung in unter 60 Sekunden fertig.
- **SC-002**: Eine Änderung im Telefon-Adressbuch steht spätestens 5 Sekunden, nachdem holzi wieder im
  Vordergrund ist, im Adressbuch.
- **SC-003**: Eine Änderung an einem anderen Gerät steht spätestens 5 Sekunden, nachdem das Telefon sie
  über den Sync bekommen hat, im Telefon-Adressbuch.
- **SC-004**: Ein Hin- und Rückweg eines Kontakts (Telefon → Adressbuch → Telefon) verliert in Tests mit
  allen Angaben aus FR-003 keine einzige Angabe.
- **SC-005**: Zwei Telefone mit denselben 500 Kontakten im selben Google-Konto ergeben nach dem
  Einschalten auf beiden genau 500 Kontakte im Adressbuch, ohne Doppel.
- **SC-006**: In jedem Testfall mit Massenlöschung (Konto entfernt, Berechtigung entzogen, mehr als 10 %
  gelöscht) geht ohne Bestätigung kein Kontakt im Adressbuch verloren.
- **SC-007**: Die Suche in 5.000 Kontakten zeigt Ergebnisse beim Tippen ohne spürbare Verzögerung (unter
  200 ms je Eingabe).
- **SC-008**: Ohne Berechtigung bekommt keine Erweiterung und kein Agent in den Tests einen Kontakt oder
  einen Hinweis auf seine Existenz.
- **SC-009**: Eine Visitenkarte ist in unter 10 Sekunden ausgetauscht (QR-Code zeigen, scannen,
  Vorschau, speichern), und jede veränderte Karte wird in den Tests abgewiesen.
- **SC-010**: Eine Einladung aus dem Adressbuch an einen Empfänger, der online ist, steht spätestens 10
  Sekunden nach dem Senden bei ihm; ist er offline, spätestens 10 Sekunden nachdem eines seiner Geräte
  vor Ablauf der Einladung online kommt.

## Assumptions

- Das Telefon-Adressbuch gibt es in dieser Spec nur auf Android; iOS ist noch kein Ziel von holzi
  (Spec 043, „iOS (eigene Spec)“). Der Abgleich ist so beschrieben, dass ein späteres iOS-Gegenstück
  möglich bleibt.
- Abgeglichen werden alle Telefonkonten (wie gewünscht „alle Kontakte“); eine Auswahl einzelner Konten
  ist nicht vorgesehen.
- Der Abgleich läuft wie der Sync nur im Vordergrund (Spec 043 FR-018).
- Gruppen des Telefon-Adressbuchs werden als Gruppen übernommen und zurückgeschrieben, soweit das
  Telefonkonto Gruppen kennt.
- Die Regel für gleichzeitige Änderungen ist die des Datensyncs (Spec 024); diese Spec führt keine
  eigene Konfliktlösung ein.
- Kontakte sind Kerndaten von holzi, keine Daten einer Erweiterung; Erweiterungen kommen nur über die
  Berechtigung aus FR-029 an sie heran.
- Die Grenze von 10 Kontakten und 10 % in FR-016 ist ein Startwert; der Plan darf ihn begründet ändern.
- Eine Visitenkarte, die als QR-Code nicht mehr gut lesbar ist (viele Geräte, viele Nostr-Relays), darf
  der Plan im QR-Code kürzen, solange Link und Unterschrift vollständig bleiben; wie, legt der Plan
  fest.
- Wer Bens Visitenkarte hat, kann Ben einladen. Weil Einladungen von Unbekannten getrennt, ohne
  Benachrichtigung und blockierbar ankommen (FR-038, FR-039), reicht das als Schutz vor unerwünschten
  Einladungen; weitergehender Spamschutz ist nicht vorgesehen.
- Die Zustellung über Nostr-Relays nutzt deren Speicherung bis zum Ablauf der Einladung; die
  Nachrichtenart und den Schutz der Absender-Identität legt der Plan fest.

## Nicht im Umfang

- Konkrete Arten für Messenger-Dienste (mit Prüfung oder Verknüpfung zur App), PGP-Schlüssel und
  Serveradressen: spätere Specs, auf Grundlage von FR-007. Bis dahin lassen sie sich als eigene Angaben
  speichern; Messenger-Kennungen und SIP-Adressen aus dem Telefon-Adressbuch nur als einfacher Text.
- Arten von Angaben, die Erweiterungen selbst einführen.
- Import und Export von vCard-Dateien und ein CardDAV-Abgleich (zum Beispiel mit Nextcloud): eigene
  Spec.
- Ändern von Kontakten durch Erweiterungen.
- Doppel im Adressbuch von Hand zusammenführen (über die Zusammenführung beim Einschalten hinaus).
- Kontakte selbst mit anderen Vaults teilen und Nachrichten an Kontakte schicken.
- Abgleich im Hintergrund, solange holzi nicht im Vordergrund ist.
- Übernahme der Identitäten aus haex-vault 2.x („Kontakte“ dort).
