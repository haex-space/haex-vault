# Feature Specification: Kanäle zwischen Erweiterungen auf verschiedenen Vaults

**Feature Branch**: `057-extension-peer-channels`

**Created**: 2026-10-22

**Status**: Draft

**Input**: User description: "Kanäle zwischen Erweiterungsrahmen auf Geräten verschiedener Vaults,
zuerst für den Mehrspielermodus von haex-doom. Eine neue Berechtigungsart `peers` (Aktion `connect`).
Eine Erweiterung mit dieser Berechtigung kann (1) einen Kontakt einladen: Die Einladung geht an alle
erreichbaren Geräte des Kontakts zugleich, die erste Annahme gewinnt, auf den anderen wird sie
zurückgezogen; die empfangende Vault nimmt Einladungen nur von eigenen Kontakten an, nur für dieselbe
installierte und aktivierte Erweiterung, und der Nutzer muss annehmen; ist kein Rahmen der
Erweiterung offen, öffnet eine Benachrichtigung der Vault sie. (2) Eine Lobby mit der Sichtbarkeit
„nur Kontakte“ oder „öffentlich“ anmelden und einen Beitrittscode erhalten (Geheimnis je Lobby, gültig
nur, solange die Lobby offen ist); Beitretende legen den Code vor; Lobbys „nur Kontakte“ lassen nur
Kontakte zu, öffentliche jeden mit Code (als nicht bestätigt angezeigt); Nicht-Kontakte werden nur
angenommen, solange eine öffentliche Lobby offen ist, und müssen den Code binnen weniger Sekunden
vorlegen; der Gastgeber kann einen Spieler entfernen, der dieser Lobby nicht wieder beitreten kann.
(3) Über einen offenen Kanal Nachrichten austauschen, schnell und verlusttolerant (Spielpakete) und
gesichert (Lobby-Nachrichten), Ende-zu-Ende-verschlüsselt, direkt zwischen den Geräten, wo möglich,
sonst über das Relay der Vault. (4) Erfahren, wenn ein Kanal schließt. Kanäle verbinden nur Rahmen
derselben Erweiterung. Die Zustellung an die Rahmen nutzt die bestehenden Meldungen an
Erweiterungsrahmen. Nicht im Umfang: Erkennung im lokalen Netz (mDNS), Durchsuchen offener Spiele
anderer, Übertragung von Dateien."

## Begriffe

- **Erweiterung**, **Erweiterungsrahmen**, **Berechtigung**, **Prüfstelle**, **Meldung** (holzi →
  Erweiterung): wie in Spec 017. Eine Erweiterung ist eindeutig durch Herausgeberschlüssel und Name.
- **Vault-Identität**, **Geräteschlüssel**, **Geräteliste**: wie in Spec 024.
- **Kontakt**, **Visitenkarte**, **bestätigte holzi-Adressen**, **blockierte Vault**: wie in Spec 051.
- **Anklopfen**: wie in Spec 056.
- **Kanal**: eine Ende-zu-Ende-verschlüsselte Verbindung zwischen genau zwei Geräten, auf beiden
  Seiten an dieselbe Erweiterung gebunden. Ein Kanal trägt Nachrichten in beide Richtungen und
  entsteht nur aus einer angenommenen Einladung oder einem zugelassenen Beitritt.
- **Schnelle Nachricht**: eine kleine Nachricht ohne Wiederholung. Sie kann verloren gehen oder in
  anderer Reihenfolge ankommen, wird dafür ohne Wartezeit zugestellt (Spielpakete).
- **Gesicherte Nachricht**: eine Nachricht, die vollständig und in der Reihenfolge des Sendens
  ankommt, oder der Kanal schließt (Lobby- und Steuernachrichten).
- **Einladung**: das Angebot einer Erweiterung an einen Kontakt, einen Kanal zu derselben
  Erweiterung zu öffnen, mit einem kleinen Inhalt der Erweiterung (etwa Spiel, Modus, Karte). Sie lebt
  nur im Arbeitsspeicher und läuft ab.
- **Einladende Seite**, **eingeladene Seite**: das Gerät, dessen Erweiterung einlädt, und die Geräte
  des Kontakts, die die Einladung erhalten.
- **Einladungsabfrage**: die Abfrage von holzi selbst „[Kontakt] lädt dich zu [Erweiterung] ein –
  Annehmen / Ablehnen“, im Fenster von holzi oder als Benachrichtigung von holzi. Nur hier nimmt der
  Nutzer eine Einladung an oder lehnt sie ab.
- **Lobby**: eine offene Runde, die eine Erweiterung auf ihrem Gerät anmeldet, damit andere mit einem
  Beitrittscode beitreten können. Sie hat eine **Sichtbarkeit**, „nur Kontakte“ (Standard) oder
  „öffentlich“, und ist **offen**, bis die Erweiterung sie schließt.
- **Gastgeber**: das Gerät, auf dem die Lobby angemeldet ist. **Beitretender**: ein Gerät, dessen
  Erweiterung mit einem Beitrittscode beitritt.
- **Beitrittscode**: ein Text, den der Gastgeber weitergibt. Er macht das gastgebende Gerät erreichbar
  und enthält ein zufälliges Geheimnis, das nur für diese eine Lobby gilt. Für die Erweiterung ist er
  undurchsichtig.
- **Bestätigt**: Die Gegenseite hat ihren Geräteschlüssel nachgewiesen und gehört zu einer Vault,
  deren Vault-Identität ein Kontakt mit bestätigten holzi-Adressen trägt oder die die eigene Vault ist.
  Sonst ist sie **nicht bestätigt**; ihr Anzeigename ist dann selbst gewählt und ungeprüft.
- **Entfernen**: Der Gastgeber schließt den Kanal eines Beitretenden und sperrt ihn für diese Lobby.

_Nicht verwechseln_: Eine **Einladung** hier ist keine Einladung aus dem Adressbuch zu einem Space
oder einer Datenfreigabe (ADR-0014, Spec 051 US7); diese wird gespeichert über Nostr zugestellt, eine
Einladung hier nur live. Eine **Lobby** ist kein Space. Ein **Kanal** ist weder eine Sync-Sitzung
zwischen eigenen Geräten (Spec 024) noch ein Dateizugriff (Spec 056) und gibt keinen Zugang zu
Vault-Daten oder Dateien.

## Beziehung zu bestehenden Specs

- **Anlass**: der Mehrspielermodus der Erweiterung haex-doom. Ihr abgestimmter Entwurf für diese
  Host-Funktion steht in `haex-space/haextension` bei Commit
  `a076b69321d0cfd63baa23d9b628620e1866e4f3` unter `specs/020-haex-doom/` (`spec.md` US3, FR-027 bis
  FR-035, FR-040 bis FR-045; `contracts/peers-api.md`; `data-model.md`; `research.md` R-10 bis
  R-13). Diese Spec legt die Seite von holzi fest; wo der Entwurf von holzis Stand abweicht, gilt diese
  Spec, und der Plan von haex-doom wird angepasst.
- **Spec 017** (Erweiterungs-Host): Die neue Berechtigungsart `peers` folgt dem bestehenden
  Berechtigungsmodell (Zustände, Abfrage zur Laufzeit, vault-weit). Angenommene Einladungen,
  Beitritte, Nachrichten und das Ende eines Kanals erreichen die Rahmen als Meldungen wie alle anderen (017 FR-042: vor
  „bereit“ gepuffert). Deaktivieren und Entfernen beenden alles dieser Erweiterung (017 FR-039). Der
  Plan ergänzt die Methoden und Meldungen in `specs/017-extension-host/contracts/bridge.md`.
- **Spec 024** (Sync der eigenen Geräte): liefert den Netzwerkendpunkt der Vault-Sitzung, die
  konfigurierten Relays und Geräteschlüssel. Kanäle gibt es nur, solange die Vault geöffnet ist. Sie
  laufen nie über die Sync-Verbindung und tauschen nie Vault-Daten aus.
- **Spec 051** (Adressbuch): liefert Kontakte mit bestätigten holzi-Adressen und blockierte Vaults.
  Welche Kontakte eine Erweiterung einladen darf, bestimmt ihre Berechtigung `contacts` (051 FR-029);
  wie dort erfährt eine Erweiterung nie, ob ein Kontakt außerhalb ihrer Berechtigung existiert
  (051 FR-031).
- **Spec 056 und ADR-0015** (Dateizugriff): Fremde Geräte weisen sich dort mit Geräteschlüssel und
  unterschriebener Geräteliste aus, und ein Gerät eines Kontakts wird über Anklopfen gefunden, ohne
  öffentliche Adressauflösung (056 FR-002, FR-003, FR-008, FR-010). Diese Spec weist Geräte auf
  dieselbe Weise aus. Einladungen erreichen die Geräte eines Kontakts zunächst über die Adressen aus
  seiner Visitenkarte und nutzen Anklopfen, sobald es mit Spec 056 da ist (Clarifications 2026-10-22).
  Sie geht über ADR-0015 hinaus: Für öffentliche Lobbys nimmt ein Gerät auch Geräte fremder Vaults an,
  die kein Kontakt sind. Das hält ein eigener ADR bei der Planung fest.
- **Spec 043** (Mobil): Wo das System keine Benachrichtigungen zeigt, zeigt holzi sie im eigenen
  Fenster (043 FR-023); das gilt auch für die Einladungsabfrage.
- **vault-sdk**: Die Schnittstelle `client.peers` für Erweiterungen ist eine eigene Änderung in
  `haex-space/vault-sdk`.

## Clarifications

### Session 2026-10-22

Die ersten vier Entscheidungen übernimmt diese Spec aus dem Entwurf von haex-doom; die letzten beiden
hat der Betreiber für diese Spec getroffen.

- Q: Von wem nimmt eine Vault Einladungen und Beitritte an? → A: Der Gastgeber legt die Sichtbarkeit
  jeder Lobby fest. „Nur Kontakte“ (Standard): Nur Kontakte des Gastgebers treten bei, per Einladung
  oder Beitrittscode. „Öffentlich“: Jeder mit dem Beitrittscode tritt bei. Einladungen gehen nur an
  Kontakte, und die empfangende Vault nimmt sie nur von eigenen Kontakten an. Der Beitrittscode trägt
  ein Geheimnis je Lobby und gilt nur, solange die Lobby offen ist.
- Q: Auf welchen Geräten erscheint eine Einladung, wenn der Kontakt mehrere Geräte mit geöffneter
  Vault hat? → A: Auf allen erreichbaren zugleich; die erste Annahme gewinnt, auf den anderen wird die
  Einladung zurückgezogen.
- Q: Gibt es Erkennung im lokalen Netz oder eine Liste offener Spiele der Kontakte? → A: Nein. Geräte
  im selben Netz verbinden sich ohnehin direkt; eine Liste würde Abfragen bei allen Kontakten
  verlangen und verraten, wer online ist.
- Q: Was erfährt ein Mitspieler in einer öffentlichen Lobby? → A: Bei direkter Verbindung die
  Netzwerkadresse des Gastgebers; das wird in Kauf genommen und dem Nutzer gesagt.
- Q: Wo nimmt der Nutzer eine Einladung an? → A: Immer in einer Abfrage von holzi selbst („[Kontakt]
  lädt dich zu [Erweiterung] ein – Annehmen / Ablehnen“): im Fenster, wenn ein Rahmen der
  Erweiterung offen ist, sonst als Benachrichtigung von holzi. Die Erweiterung erfährt von der
  Einladung erst nach der Annahme und erhält dann den Kanal; eine Ablehnung erfährt die einladende
  Seite als „abgelehnt“. Erweiterungen brauchen keinen eigenen Einladungsdialog, und es gibt für sie
  keine Funktion zum Annehmen oder Ablehnen (Option B).
- Q: Wie erreicht eine Einladung die Geräte eines Kontakts? → A: Die erste Fassung wählt die
  Adressen aus der gespeicherten Visitenkarte (Netzwerkkennung und Relay je Gerät). Scheitert das,
  erhält die einladende Seite „nicht erreichbar“ mit dem Hinweis, dass der Kontakt vielleicht eine
  aktuelle Visitenkarte teilen muss. Sobald es das Anklopfen aus Spec 056 gibt, nutzen Einladungen es.
  Beitrittscodes sind nicht betroffen; sie tragen die aktuelle Adresse des Gastgebers.

## User Scenarios & Testing _(mandatory)_

### User Story 1 - Einen Kontakt einladen und verbunden werden (Priority: P1)

Anna öffnet DOOM, eröffnet ein Spiel und lädt ihren Kontakt Ben ein. Ben hat holzi auf Laptop und
Telefon offen, DOOM ist dort installiert. Auf dem Telefon läuft DOOM gerade; dort fragt holzi im
Fenster: „Anna lädt dich zu DOOM ein – Annehmen / Ablehnen“. Auf dem Laptop ist DOOM nicht geöffnet;
dort zeigt holzi dieselbe Frage als Benachrichtigung. Ben nimmt auf dem Laptop an; holzi öffnet DOOM,
und DOOM erhält den Kanal zu Anna mit dem Inhalt der Einladung. Auf dem Telefon verschwindet die
Frage, DOOM dort hat von der Einladung nie erfahren. Anna sieht „angenommen“.

**Why this priority**: Die Einladung unter Kontakten ist der sichere Standardweg in ein gemeinsames
Spiel. Sie legt das Ausweisen, das Erreichen der Geräte eines Kontakts, die Zustellung an die richtige
Erweiterung und die Zustimmung des Nutzers fest, auf denen alles Weitere aufbaut.

**Independent Test**: Zwei Vaults, die sich gegenseitig als Kontakt mit Visitenkarte führen, die
eingeladene mit zwei Geräten, eines mit offenem Rahmen der Testerweiterung, eines ohne. Eine
Testerweiterung mit der Berechtigung lädt ein; auf beiden Geräten erscheint holzis Einladungsabfrage,
die Rahmen der eingeladenen Seite erhalten nichts. Nach der Annahme auf einem Gerät erhält die
Erweiterung dort den Kanal, auf dem anderen verschwindet die Abfrage, und die einladende Seite erhält
genau einen Status.

**Acceptance Scenarios**:

1. **Given** Anna und Ben sind gegenseitig Kontakte, Bens Vault ist offen und dieselbe Erweiterung
   installiert und aktiviert, **When** Annas Erweiterung Ben einlädt, **Then** zeigt holzi auf jedem
   erreichbaren Gerät Bens die Einladungsabfrage mit Annas Namen, wie Bens Adressbuch ihn führt, und
   dem Namen der Erweiterung, und kein Rahmen der Erweiterung erfährt davon.
2. **Given** auf einem Gerät Bens ist ein Rahmen der Erweiterung offen, **When** die Einladung
   ankommt, **Then** zeigt holzi die Einladungsabfrage im eigenen Fenster; ist keiner offen, zeigt
   holzi sie als Benachrichtigung.
3. **Given** Ben nimmt in holzis Einladungsabfrage an und auf dem Gerät ist kein Rahmen der
   Erweiterung offen, **When** die Annahme gilt, **Then** öffnet holzi die Erweiterung, und sie
   erhält den Kanal mit Annas Kontakt und dem Inhalt der Einladung, sobald ihr Rahmen bereit ist.
4. **Given** die Einladungsabfrage liegt auf Bens Laptop und Telefon, **When** Ben auf dem Laptop
   annimmt, **Then** entsteht genau ein Kanal zwischen Annas Gerät und dem Laptop, auf dem Telefon
   verschwindet die Abfrage, und eine spätere Annahme dort scheitert.
5. **Given** Ben lehnt in der Einladungsabfrage auf allen Geräten ab, auf denen sie erschien, **When**
   die letzte Ablehnung eintrifft, **Then** erhält Annas Erweiterung einmal „abgelehnt“, und Bens
   Erweiterung erfährt nichts.
6. **Given** Bens Vault führt Anna nicht als Kontakt oder hat Annas Vault blockiert, **When** Annas
   Erweiterung Ben einlädt, **Then** erscheint bei Ben nichts, und Anna erhält „nicht erreichbar“, ohne
   zu erfahren, warum.
7. **Given** Bens Geräte haben seit dem Austausch der Visitenkarte ihr Relay gewechselt, **When** Anna
   einlädt, **Then** erhält Anna „nicht erreichbar“ mit dem Hinweis, dass Ben vielleicht eine aktuelle
   Visitenkarte teilen muss.
8. **Given** auf Bens Geräten ist die Erweiterung nicht installiert oder deaktiviert, **When** Anna
   einlädt, **Then** erscheint bei Ben nichts, und Anna erhält „nicht installiert“.
9. **Given** niemand nimmt an, **When** fünf Minuten vergangen sind, **Then** läuft die Einladung ab,
   die Einladungsabfrage verschwindet auf Bens Geräten, und Anna erhält „abgelaufen“.

---

### User Story 2 - Nachrichten über einen Kanal austauschen und sein Ende erfahren (Priority: P1)

Annas und Bens DOOM tauschen über den Kanal 35-mal pro Sekunde kleine Spielpakete aus, schnelle
Nachrichten, bei denen ein verlorenes Paket nicht aufhält. Lobby-Nachrichten wie „Spiel startet auf
Karte E1M1“ gehen als gesicherte Nachrichten. Liegen beide im selben Netz, laufen die Nachrichten
direkt; sonst über ein Relay, ohne dass die Erweiterung etwas tun muss. Schließt Ben das Spiel, erfährt
Annas DOOM, dass der Kanal geschlossen ist; bricht Bens Verbindung ab, erfährt sie, dass er verloren ist.

**Why this priority**: Ohne Nachrichten ist ein Kanal nutzlos. Die Zeit- und Mengenziele entscheiden,
ob ein Echtzeitspiel spielbar ist.

**Independent Test**: Zwei Testerweiterungen mit einem Kanal senden zehn Minuten lang je Richtung 35
schnelle Nachrichten pro Sekunde und zwischendurch gesicherte Nachrichten, einmal im selben Netz,
einmal mit erzwungenem Relay. Gemessen werden Ankunft, Verzögerung und Reihenfolge; danach schließt
eine Seite, die andere erhält das Ende mit Grund.

**Acceptance Scenarios**:

1. **Given** ein offener Kanal, **When** eine Seite eine schnelle Nachricht sendet, **Then** erhalten
   die Rahmen derselben Erweiterung auf der Gegenseite sie ohne Wartezeit auf frühere Nachrichten.
2. **Given** ein offener Kanal, **When** eine Seite gesicherte Nachrichten sendet, **Then** kommen alle
   vollständig und in der Reihenfolge des Sendens an, oder der Kanal schließt und beide Seiten erfahren
   es.
3. **Given** zwei Geräte ohne direkten Weg zueinander, **When** ein Kanal entsteht, **Then** laufen die
   Nachrichten über ein Relay, und das Relay sieht nur verschlüsselte Daten.
4. **Given** eine Nachricht über der Obergrenze ihrer Art, **When** die Erweiterung sie senden will,
   **Then** wird nichts gesendet, und sie erhält einen Fehler „zu groß“.
5. **Given** ein offener Kanal, **When** eine Seite ihn schließt, die Verbindung abbricht oder 30
   Sekunden lang nichts mehr ankommt, **Then** erhalten beide Erweiterungen das Ende mit dem Grund
   „geschlossen“, „verloren“ oder „Zeitüberschreitung“, und Senden darauf scheitert danach.
6. **Given** ein offener Kanal, **When** die Vault einer Seite geschlossen oder die Erweiterung dort
   deaktiviert wird, **Then** schließt der Kanal, und die Gegenseite erfährt es.

---

### User Story 3 - Der Nutzer erlaubt Kanäle, und Erweiterungen bleiben getrennt (Priority: P1)

Bei der Installation von DOOM sieht der Nutzer, dass die Erweiterung Verbindungen zu anderen Vaults
öffnen möchte und dass Mitspieler dabei seine Netzwerkadresse sehen können. Er kann das erlauben,
später fragen lassen oder verweigern. Eine andere Erweiterung, etwa ein Notizbuch, erfährt nichts von
DOOMs Kanälen und kann über sie nichts senden oder empfangen.

**Why this priority**: Kanäle öffnen das Gerät nach außen. Ohne Berechtigung und strikte Trennung
wären sie ein Weg, an holzi oder an anderen Erweiterungen vorbei Daten auszutauschen.

**Independent Test**: Zwei Testerweiterungen A und B, nur A mit Berechtigung. A öffnet einen Kanal
zu einem Kontakt; B versucht, mit den Kennungen von A zu senden, zu schließen oder Meldungen zu
erhalten, und scheitert; B ohne Berechtigung kann nichts einladen und keine Lobby öffnen.

**Acceptance Scenarios**:

1. **Given** eine Erweiterung mit der Berechtigung `peers` im Zustand „verweigert“, **When** sie
   einlädt, eine Lobby öffnet oder beitritt, **Then** geschieht nichts außerhalb des Geräts, und sie
   erhält einen Berechtigungsfehler.
2. **Given** der Zustand „fragen“, **When** die Erweiterung das erste Mal einlädt, eine Lobby öffnet
   oder beitritt, **Then** fragt holzi den Nutzer wie bei anderen Berechtigungen, bevor etwas
   geschieht.
3. **Given** Erweiterung A hat einen Kanal, **When** Erweiterung B dessen Kennung benutzt, **Then**
   scheitert B wie bei einer unbekannten Kennung, und Nachrichten des Kanals erreichen nie Rahmen von B
   oder die Oberfläche von holzi.
4. **Given** ein Kanal zwischen Annas und Bens DOOM, **When** Annas Seite Nachrichten sendet, **Then**
   erreichen sie auf Bens Gerät nur Rahmen derselben Erweiterung (Herausgeberschlüssel und Name).
5. **Given** der Nutzer entzieht die Berechtigung, **When** Kanäle, Lobbys oder Einladungen dieser
   Erweiterung offen sind, **Then** schließt holzi sie sofort, und die Gegenseiten erfahren es.

---

### User Story 4 - Einer Lobby mit Beitrittscode beitreten (Priority: P2)

Anna öffnet in DOOM eine Lobby „nur Kontakte“ und kopiert den Beitrittscode in einen Chat mit Ben und
Carla, die beide in ihrem Adressbuch stehen. Ben und Carla fügen den Code in ihr DOOM ein und treten
bei. Annas DOOM sieht beide mit ihrem Namen aus Annas Adressbuch als bestätigt. Carla benimmt sich
daneben; Anna entfernt sie. Carla kann mit demselben Code nicht zurück. Als das Spiel startet, schließt
Annas DOOM die Lobby; der Code wirkt nicht mehr, der Kanal zu Ben bleibt.

**Why this priority**: Ein Code erreicht mehrere Mitspieler auf einmal und braucht keine Einladung je
Person. Er baut auf Kanälen (US2) und dem Ausweisen (US1) auf.

**Independent Test**: Eine Testerweiterung öffnet eine Lobby „nur Kontakte“. Zwei Kontakte treten mit
dem Code bei, ein Nicht-Kontakt wird abgewiesen, ein entfernter Kontakt kommt nicht zurück, nach dem
Schließen scheitert jeder weitere Beitritt mit derselben Antwort wie ein falscher Code.

**Acceptance Scenarios**:

1. **Given** eine Erweiterung mit Berechtigung, **When** sie eine Lobby öffnet, **Then** erhält sie
   einen Beitrittscode, der ab sofort und bis zum Schließen der Lobby wirkt.
2. **Given** eine offene Lobby „nur Kontakte“, **When** ein bestätigter Kontakt mit dem Code beitritt,
   **Then** entsteht ein Kanal, und die Erweiterung des Gastgebers erfährt den selbst gewählten
   Anzeigenamen, den Kontakt aus dem eigenen Adressbuch, „bestätigt“ und den Inhalt des Beitretenden.
3. **Given** eine offene Lobby „nur Kontakte“, **When** jemand mit dem Code beitritt, der kein Kontakt
   des Gastgebers ist, **Then** wird er mit der Antwort „nicht zugelassen“ abgewiesen, und es entsteht
   kein Kanal.
4. **Given** ein falsch abgetippter Code, ein Code einer geschlossenen Lobby oder ein Code mit falschem
   Geheimnis, **When** jemand damit beitritt, **Then** lautet die Antwort in allen Fällen gleich
   „nicht mehr offen“, und er erfährt nichts weiter über den Gastgeber.
5. **Given** ein Beitretender in einer Lobby, **When** der Gastgeber ihn entfernt, **Then** schließt
   sein Kanal, und jeder weitere Beitritt desselben Geräts oder derselben Vault zu dieser Lobby wird
   abgewiesen, bis die Lobby schließt.
6. **Given** eine offene Lobby mit Kanälen, **When** die Erweiterung des Gastgebers die Lobby schließt,
   **Then** wirkt der Code nicht mehr, und die bestehenden Kanäle bleiben offen.

---

### User Story 5 - Eine öffentliche Lobby für Mitspieler ohne Kontakt (Priority: P3)

Anna will auf einem Treffen mit Leuten spielen, die nicht in ihrem Adressbuch stehen. Sie öffnet eine
öffentliche Lobby und zeigt den Code. Dave, kein Kontakt, tritt mit dem Code und dem Namen „Dave“ bei;
Annas DOOM zeigt ihn als „nicht bestätigt“. Bevor Anna die Lobby öffnete, hätte Daves Gerät keine
Verbindung zu ihrem bekommen, und nach dem Schließen bekommt es wieder keine.

**Why this priority**: Spielen ohne Adressbuch ist nützlich für Vorführungen, öffnet das Gerät aber
für Unbekannte. Es kommt nach dem Standardweg über Kontakte.

**Independent Test**: Eine Testerweiterung öffnet eine öffentliche Lobby. Ein Gerät einer fremden
Vault, die kein Kontakt ist, tritt bei und wird als nicht bestätigt gemeldet. Ohne offene öffentliche
Lobby wird dasselbe Gerät gleich nach dem Ausweisen abgewiesen; ein Gerät, das keinen Code vorlegt,
wird nach 5 Sekunden getrennt.

**Acceptance Scenarios**:

1. **Given** eine offene öffentliche Lobby, **When** jemand ohne Kontakt mit gültigem Code beitritt,
   **Then** entsteht ein Kanal, und die Erweiterung des Gastgebers erfährt den selbst gewählten Namen
   und „nicht bestätigt“.
2. **Given** auf dem Gerät ist keine öffentliche Lobby offen, **When** ein Gerät einer Vault, die kein
   Kontakt ist, sich verbindet, **Then** trennt holzi gleich nach dem Ausweisen, ohne zu verraten, ob
   es Lobbys gibt.
3. **Given** eine offene öffentliche Lobby, **When** ein fremdes Gerät sich verbindet und binnen
   5 Sekunden keinen Beitritt vorlegt, **Then** trennt holzi die Verbindung.
4. **Given** eine offene öffentliche Lobby, **When** ein Gerät einer blockierten Vault mit gültigem
   Code beitritt, **Then** wird es abgewiesen wie mit einem falschen Code.
5. **Given** ein Kontakt tritt einer öffentlichen Lobby bei, **When** die Erweiterung des Gastgebers
   den Beitritt erhält, **Then** ist er als bestätigt mit dem Kontakt gemeldet.

---

### Edge Cases

- Der eingeladene Kontakt hat kein Gerät mit geöffneter Vault. Die einladende Seite erhält nach
  höchstens 15 Sekunden „nicht erreichbar“; die Einladung wird nicht für später aufgehoben.
- Ein Gerät des Kontakts hat seit dem Austausch der Visitenkarte sein Relay gewechselt. In der ersten
  Fassung ist es über die Visitenkarte nicht erreichbar, bis der Kontakt eine aktuelle Visitenkarte
  teilt; die einladende Seite erhält „nicht erreichbar“ mit dem entsprechenden Hinweis. Sobald es
  Anklopfen gibt, findet holzi die aktuelle Adresse darüber. Beitrittscodes betrifft das nicht.
- Ein Gerät steht noch auf einer alten Visitenkarte, ist aber aus seiner Vault entfernt worden. Kennt
  die andere Seite eine neuere Geräteliste dieser Vault, wird es abgewiesen (wie 056 FR-003).
- Ein Kontakt lehnt auf einem Gerät ab und nimmt auf einem anderen an. Die Annahme gewinnt; der Status
  an die einladende Seite ist „angenommen“.
- Zwei Geräte des Kontakts nehmen fast gleichzeitig an. Genau eine Annahme gewinnt; auf dem anderen
  Gerät meldet holzis Einladungsabfrage „Einladung nicht mehr offen“, und die Erweiterung dort erfährt
  nichts.
- Die einladende Erweiterung zieht die Einladung zurück, oder ihre Vault schließt, während die
  Einladung offen ist. Auf allen Geräten des Kontakts verschwindet die Einladungsabfrage; eine spätere
  Annahme scheitert.
- Zwischen Anzeige und Annahme wird die Erweiterung auf der eingeladenen Seite deaktiviert oder
  entfernt, oder ihre Berechtigung wird verweigert. Die Annahme scheitert, es entsteht kein Kanal, und
  die einladende Seite erhält „nicht installiert“.
- Anna und Ben laden sich gleichzeitig gegenseitig ein. Es sind zwei unabhängige Einladungen; nimmt
  jeder an, entstehen zwei Kanäle.
- Der letzte Rahmen der Erweiterung auf einem Gerät schließt. holzi schließt ihre Kanäle und Lobbys
  auf diesem Gerät und zieht ihre offenen Einladungen zurück, wie bei der Shell (Spec 017). Lädt ein
  Rahmen nur neu, bleiben sie bestehen; Meldungen bis zur Bereitschaft werden gepuffert (017 FR-042).
- Ein Rahmen nimmt Meldungen lange nicht ab. Läuft der Puffer über, verwirft holzi zuerst die ältesten
  schnellen Nachrichten; müsste eine gesicherte Nachricht verworfen werden, schließt holzi stattdessen
  den Kanal. Andere Rahmen und holzis Oberfläche stocken dabei nicht.
- Mehrere Rahmen derselben Erweiterung sind auf einem Gerät offen. Jeder erhält die Meldungen; welcher
  sie verarbeitet, entscheidet die Erweiterung.
- Ein Beitrittscode wird öffentlich verbreitet, und viele versuchen beizutreten. holzi begrenzt Versuche
  und gleichzeitige Verbindungen; über der Grenze lautet die Antwort wie bei einem geschlossenen Code.
  Wie viele Mitspieler eine Lobby aufnimmt, entscheidet die Erweiterung, indem sie Beitretende
  entfernt oder die Lobby schließt.
- Ein Beitrittscode der Erweiterung A wird in Erweiterung B eingegeben. Die Antwort lautet „nicht mehr
  offen“; Lobbys gelten nur für ihre eigene Erweiterung.
- Das gastgebende Gerät wechselt das Netz, während die Lobby offen ist. Bestehende Kanäle wechseln den
  Weg, ohne zu schließen, wenn das möglich ist; ein schon weitergegebener Code kann danach unerreichbar
  sein, und Beitretende erhalten „nicht erreichbar“.
- Der Nutzer hat alle Relays in den Einstellungen abgeschaltet. Kanäle entstehen nur auf direktem Weg;
  sonst gilt die Gegenseite als nicht erreichbar.
- Ein mobiles Gerät geht in den Hintergrund oder sperrt. Verliert holzi dort die Verbindung, schließt
  der Kanal mit „verloren“ oder „Zeitüberschreitung“; ein neuer Kanal braucht eine neue Einladung oder
  einen neuen Beitritt.
- Ein Kontakt hat dieselbe Erweiterung in einer anderen Fassung. holzi stellt trotzdem zu; ob die
  Fassungen zusammenpassen, prüft die Erweiterung anhand ihres Inhalts.
- Ein Gerät der eigenen Vault tritt mit dem Code einer Lobby „nur Kontakte“ bei. Es ist bestätigt und
  wird zugelassen.

## Requirements _(mandatory)_

### Functional Requirements

**Berechtigung und Trennung**

- **FR-001**: holzi MUSS eine Berechtigungsart `peers` mit der Aktion `connect` und dem Ziel `*`
  kennen. Sie folgt dem bestehenden Berechtigungsmodell (Spec 017): Zustände „erteilt“, „verweigert“
  und „fragen“, Abfrage zur Laufzeit, Erklärung im Manifest, Geltung vault-weit.
- **FR-002**: Ohne erteilte Berechtigung `peers` MUSS jede Funktion dieser Spec abgelehnt werden,
  bevor etwas das Gerät verlässt; im Zustand „fragen“ MUSS zuerst die übliche Abfrage kommen.
- **FR-003**: Die Abfrage und die Anzeige bei der Installation MÜSSEN sagen, dass die Erweiterung
  Verbindungen zu Geräten anderer Vaults öffnen kann und dass Gegenseiten bei direkter Verbindung die
  Netzwerkadresse des Geräts erfahren.
- **FR-004**: Jeder Kanal, jede Einladung und jede Lobby MUSS an genau eine Erweiterung gebunden sein,
  die holzi aus ihrem Rahmen kennt, nie aus der Anfrage. Auf beiden Seiten MUSS es dieselbe Erweiterung
  sein (Herausgeberschlüssel und Name).
- **FR-005**: holzi MUSS angenommene Einladungen, Beitritte, Nachrichten und das Ende eines Kanals
  nur an Rahmen der gebundenen Erweiterung auf dem Gerät zustellen, als Meldungen nach Spec 017.
  Andere Erweiterungen und holzis Oberfläche DÜRFEN keine Inhalte erhalten.
- **FR-006**: Eine Erweiterung DARF nur ihre eigenen Kanäle, Einladungen und Lobbys bedienen; eine
  fremde und eine unbekannte Kennung MÜSSEN dieselbe Antwort ergeben.
- **FR-007**: Ein Kanal DARF nur die Nachrichten der Erweiterung tragen. Er DARF keinen Zugang zu
  Vault-Daten, Dateien, Geräten oder anderen Funktionen von holzi geben.
- **FR-008**: Verweigern oder Entziehen der Berechtigung, Deaktivieren oder Entfernen der Erweiterung,
  das Schließen ihres letzten Rahmens auf dem Gerät und das Schließen der Vault MÜSSEN alle Kanäle und
  Lobbys der Erweiterung auf diesem Gerät schließen und ihre offenen Einladungen zurückziehen; die
  Gegenseiten MÜSSEN das Ende erfahren.

**Einladungen**

- **FR-009**: Eine Erweiterung MUSS einen Kontakt einladen können, den ihre Berechtigung `contacts`
  umfasst und der bestätigte holzi-Adressen hat. Für jeden anderen Kontakt, auch einen, der nicht
  existiert, MUSS die Antwort dieselbe sein (051 FR-031).
- **FR-010**: Eine Einladung MUSS die Erweiterung (Herausgeberschlüssel und Name), die Vault-Identität
  der einladenden Seite und einen Inhalt der Erweiterung von höchstens 4 KiB tragen. Ein größerer Inhalt
  MUSS mit „zu groß“ abgelehnt werden.
- **FR-011**: holzi MUSS die Einladung an alle Geräte des Kontakts zugleich senden, die über die
  Adressen aus seiner gespeicherten Visitenkarte (Netzwerkkennung und Relay je Gerät) erreichbar sind.
  Geräteadressen DÜRFEN nicht öffentlich verzeichnet oder erfragt werden (056 FR-010). Sobald es
  Anklopfen gibt (056 FR-008), MUSS holzi Geräte, die über die Visitenkarte nicht erreichbar sind,
  darüber suchen.
- **FR-012**: Sobald es Anklopfen gibt, DARF ein Gerät darauf für diese Funktion nur Geräten der
  eigenen Vault und von Vaults antworten, deren Vault-Identität es als Kontakt mit bestätigten
  holzi-Adressen führt und nicht blockiert hat.
- **FR-013**: Die eingeladene Seite MUSS eine Einladung dem Nutzer nur vorlegen, wenn die einladende
  Seite bestätigt und ihre Vault nicht blockiert ist und wenn dieselbe Erweiterung installiert und
  aktiviert ist und ihre Berechtigung `peers` nicht „verweigert“ ist. Ist die einladende Seite nicht bestätigt
  oder blockiert, MUSS die Einladung ohne Anzeige verworfen werden. Fehlt die Erweiterung, ist sie
  deaktiviert oder die Berechtigung verweigert, MUSS die Antwort „nicht installiert“ lauten.
- **FR-014**: holzi MUSS jede Einladung auf der eingeladenen Seite in seiner eigenen
  Einladungsabfrage vorlegen: „[Kontakt] lädt dich zu [Erweiterung] ein – Annehmen / Ablehnen“, mit
  dem Kontakt, wie das Adressbuch der eingeladenen Seite ihn führt. Die Abfrage MUSS unabhängig von
  der Berechtigung `notifications` der Erweiterung erscheinen.
- **FR-015**: Ist auf dem Gerät ein Rahmen der Erweiterung offen, MUSS holzi die Einladungsabfrage im
  eigenen Fenster zeigen, sonst als eigene Benachrichtigung mit Annehmen und Ablehnen. Kann das System
  keine Knöpfe zeigen, MUSS das Antippen der Benachrichtigung die Abfrage in holzis Fenster öffnen;
  zeigt das System gar keine Benachrichtigung, MUSS holzi sie im eigenen Fenster zeigen (043 FR-023).
  Verschwindet die Einladung, MUSS auch die Abfrage verschwinden.
- **FR-016**: Ein Kanal DARF aus einer Einladung erst entstehen, nachdem der Nutzer in holzis
  Einladungsabfrage angenommen hat. Erweiterungen DÜRFEN keine Funktion zum Annehmen oder Ablehnen
  haben, und vor der Annahme DARF keine Erweiterung der eingeladenen Seite von der Einladung erfahren.
  Nach der Annahme MUSS die Erweiterung den Kanal mit dem einladenden Kontakt und dem Inhalt der
  Einladung erhalten; ist kein Rahmen offen, MUSS holzi die Erweiterung öffnen und ihr den Kanal
  melden, sobald der Rahmen bereit ist.
- **FR-017**: Die erste Annahme MUSS gewinnen. holzi MUSS die Einladungsabfrage danach auf allen
  anderen Geräten des Kontakts zurückziehen; eine spätere Annahme dort MUSS mit „Einladung nicht mehr
  offen“ scheitern.
- **FR-018**: Die einladende Erweiterung MUSS je Einladung genau einen Endstatus erhalten:
  „angenommen“ (mit dem Kanal), „abgelehnt“, „nicht installiert“, „nicht erreichbar“ oder „abgelaufen“.
  „Abgelehnt“ und „nicht installiert“ DÜRFEN nur kommen, wenn alle erreichten Geräte so geantwortet
  haben; eine Ablehnung in der Einladungsabfrage zählt als „abgelehnt“. „Nicht erreichbar“ MUSS
  gelten, wenn kein Gerät über die Adressen erreichbar war, und auch, wenn die eingeladene Seite die
  einladende nicht als Kontakt führt oder blockiert hat, ohne das zu unterscheiden. Zu „nicht
  erreichbar“ MUSS der Hinweis gehören, dass der Kontakt vielleicht eine aktuelle Visitenkarte teilen
  muss.
- **FR-019**: Eine Einladung MUSS nach 5 Minuten ohne Annahme ablaufen. Die einladende Erweiterung MUSS
  sie vorher zurückziehen können. Ablauf und Rückzug MÜSSEN alle Geräte des Kontakts erreichen, die sie
  erhalten haben.

**Lobbys und Beitrittscodes**

- **FR-020**: Eine Erweiterung MUSS eine Lobby mit der Sichtbarkeit „nur Kontakte“ (Standard) oder
  „öffentlich“ öffnen können und dafür einen Beitrittscode erhalten.
- **FR-021**: Ein Beitrittscode MUSS ein zufälliges Geheimnis von mindestens 128 Bit enthalten, das nur
  für diese Lobby gilt, und genug, um das gastgebende Gerät zu erreichen. Er DARF keine Vault-Daten,
  keine Namen und keine Kontaktdaten enthalten.
- **FR-022**: Ein Beitrittscode MUSS genau so lange wirken, wie die Lobby offen ist. Schließt die
  Erweiterung die Lobby, MUSS der Code sofort unwirksam werden; bestehende Kanäle MÜSSEN offen bleiben.
- **FR-023**: Eine Erweiterung MUSS mit einem Beitrittscode, einem selbst gewählten Anzeigenamen und
  einem Inhalt von höchstens 4 KiB beitreten können.
- **FR-024**: Der Gastgeber MUSS einen Beitritt nur zulassen, wenn die Lobby offen ist, das Geheimnis
  stimmt, dieselbe Erweiterung beitritt, der Beitretende nicht aus dieser Lobby entfernt wurde, seine
  Vault nicht blockiert ist und, bei „nur Kontakte“, der Beitretende bestätigt ist.
- **FR-025**: Ein unlesbarer oder unbekannter Code, ein Code einer geschlossenen Lobby, ein falsches
  Geheimnis, eine andere Erweiterung und eine blockierte Vault MÜSSEN dieselbe Antwort „nicht mehr
  offen“ ergeben, auch in der Antwortzeit nicht unterscheidbar. Ein Nicht-Kontakt in einer Lobby „nur
  Kontakte“ und ein entfernter Beitretender MÜSSEN die Antwort „nicht zugelassen“ erhalten.
- **FR-026**: Bei einem zugelassenen Beitritt MUSS die Erweiterung des Gastgebers die Lobby, den Kanal,
  den selbst gewählten Anzeigenamen, ob der Beitretende bestätigt ist, bei einem Kontakt diesen
  Kontakt und den Inhalt des Beitretenden erhalten.
- **FR-027**: Der Gastgeber MUSS einen Beitretenden entfernen können. Sein Kanal MUSS schließen, und
  weitere Beitritte desselben Geräts und, wenn er bestätigt ist, derselben Vault zu dieser Lobby MÜSSEN
  abgewiesen werden, bis die Lobby schließt.
- **FR-028**: holzi DARF offene Lobbys niemandem auflisten oder ankündigen, auch nicht im lokalen Netz;
  eine Lobby ist nur über ihren Code erreichbar.

**Annahme von Verbindungen**

- **FR-029**: Verbindungen dieser Funktion MÜSSEN von der Sync-Verbindung (Spec 024) und vom
  Dateizugriff (Spec 056) getrennt sein; eine Verbindung dieser Funktion DARF nie als eine von diesen
  gelten.
- **FR-030**: Beide Seiten MÜSSEN ihren Geräteschlüssel nachweisen und die unterschriebene Geräteliste
  ihrer Vault vorlegen. Eine Seite gilt nur als bestätigt, wenn sie die Prüfungen von 056 FR-002 und
  FR-003 besteht und ihre Vault-Identität die eigene ist oder einem Kontakt mit bestätigten
  holzi-Adressen gehört.
- **FR-031**: Ein Gerät DARF Verbindungen nicht bestätigter Gegenseiten nur annehmen, solange auf ihm
  mindestens eine öffentliche Lobby offen ist. Sonst MUSS es sie gleich nach dem Ausweisen trennen,
  ohne zu verraten, ob es Lobbys gibt.
- **FR-032**: Legt eine Gegenseite binnen 5 Sekunden nach dem Ausweisen weder einen Beitritt noch eine
  Einladung oder eine Antwort auf eine Einladung vor, MUSS holzi die Verbindung trennen.
- **FR-033**: holzi MUSS Verbindungsversuche und Beitrittsversuche je Gegenseite und insgesamt
  begrenzen, sodass das Durchprobieren von Codes und das Überlasten des Geräts scheitern, ohne dass
  gültige Beitritte von Kontakten ausgesperrt werden.
- **FR-034**: Blockierte Vaults (Spec 051) MÜSSEN immer abgewiesen werden, auch in öffentlichen Lobbys.

**Nachrichten und Ende eines Kanals**

- **FR-035**: Über einen offenen Kanal MÜSSEN beide Seiten schnelle Nachrichten von mindestens
  1.000 Byte und gesicherte Nachrichten bis 64 KiB senden können. Größere Nachrichten MÜSSEN mit „zu
  groß“ abgelehnt werden, ohne dass etwas gesendet wird.
- **FR-036**: Gesicherte Nachrichten MÜSSEN vollständig und in der Reihenfolge des Sendens ankommen,
  oder der Kanal MUSS schließen. Schnelle Nachrichten DÜRFEN verloren gehen oder die Reihenfolge
  wechseln, DÜRFEN aber nicht auf frühere Nachrichten warten.
- **FR-037**: Nachrichten MÜSSEN zwischen den beiden Geräten Ende-zu-Ende-verschlüsselt sein. Sie
  MÜSSEN direkt laufen, wo das möglich ist, sonst über ein Relay aus den Einstellungen der Vault
  (Spec 024); ein Relay DARF nur verschlüsselte Daten sehen. Ein Wechsel des Wegs DARF den Kanal nicht
  schließen.
- **FR-038**: Senden DARF nicht auf die Gegenseite warten. Ein langsamer oder nicht bereiter Rahmen
  DARF weder holzis Oberfläche noch andere Kanäle aufhalten; muss holzi Meldungen verwerfen, dann nur
  schnelle Nachrichten, sonst schließt es den Kanal.
- **FR-039**: Ein Kanal MUSS schließen, wenn eine Seite ihn schließt, die Verbindung abbricht oder
  30 Sekunden lang nichts ankommt. Solange beide Seiten offen sind, MUSS holzi den Kanal ohne Zutun der
  Erweiterung am Leben halten.
- **FR-040**: Beide Erweiterungen MÜSSEN das Ende eines Kanals genau einmal mit dem Grund
  „geschlossen“, „verloren“ oder „Zeitüberschreitung“ erfahren. Senden oder Schließen auf einem
  geschlossenen Kanal MUSS mit „Kanal geschlossen“ scheitern.

**Grenzen, Speicher und Fehler**

- **FR-041**: holzi MUSS die Zahl gleichzeitiger Kanäle, Lobbys und offener Einladungen je
  Erweiterung begrenzen. Eine Erweiterung MUSS mindestens 8 Kanäle, 2 Lobbys und 8 offene Einladungen
  zugleich haben können.
- **FR-042**: holzi DARF über Kanäle, Einladungen, Lobbys und Nachrichten nichts dauerhaft speichern,
  weder Inhalte noch Metadaten; nichts davon DARF synchronisiert oder in Protokolle geschrieben werden.
  Gespeichert bleibt nur die Berechtigung.
- **FR-043**: Fehler MÜSSEN unterscheidbar sein in: Berechtigung fehlt oder Abfrage nötig (wie
  Spec 017), Kontakt unbekannt, Einladung nicht mehr offen, Kanal geschlossen, zu groß, Lobby nicht mehr
  offen, nicht zugelassen und Grenze erreicht. Sie DÜRFEN keine Adressen, Schlüssel oder Daten anderer
  Erweiterungen enthalten.
- **FR-044**: Die Funktion MUSS auf allen Plattformen von holzi (Linux, Windows, macOS, Android, iOS)
  verfügbar sein, solange die Vault geöffnet ist. Ist der Netzwerkendpunkt der Vault nicht verfügbar,
  MÜSSEN Einladen, Öffnen einer Lobby und Beitreten mit einem eindeutigen Fehler scheitern, und die
  übrige Vault MUSS nutzbar bleiben.

### Key Entities

- **Peer-Berechtigung**: die Berechtigung `peers`/`connect`/`*` einer Erweiterung, vault-weit, im
  Zustand erteilt, verweigert oder fragen.
- **Kanal**: Kennung, gebundene Erweiterung, Gegenseite (Gerät, Vault-Identität, bestätigt oder
  nicht, bei einem Kontakt dieser Kontakt), Herkunft (Einladung oder Lobby), Zustand (offen oder
  geschlossen mit Grund). Nur im Arbeitsspeicher.
- **Einladung**: Kennung, Erweiterung, einladende Vault-Identität, eingeladener Kontakt, Inhalt der
  Erweiterung, erreichte Geräte, Zustand (offen, angenommen, abgelehnt, nicht installiert, nicht
  erreichbar, abgelaufen, zurückgezogen), Ablaufzeit. Nur im Arbeitsspeicher.
- **Lobby**: Kennung der Erweiterung, Erweiterung, Sichtbarkeit, Geheimnis, offen oder geschlossen,
  gesperrte Geräte und Vaults. Nur im Arbeitsspeicher des gastgebenden Geräts.
- **Beitrittscode**: Erreichbarkeit des gastgebenden Geräts, Lobby und Geheimnis; undurchsichtig für
  die Erweiterung.
- **Beitritt**: Lobby, Kanal, selbst gewählter Anzeigename, bestätigt oder nicht, Kontakt (falls
  bestätigt), Inhalt der Erweiterung.

## Success Criteria _(mandatory)_

### Measurable Outcomes

- **SC-001**: holzis Einladungsabfrage erscheint auf jedem Gerät eines Kontakts mit geöffneter Vault,
  dessen Adresse aus der Visitenkarte stimmt, in 95 % der Versuche innerhalb von 5 Sekunden. Ist kein
  Gerät erreichbar, erhält die einladende Seite in 100 % der Versuche innerhalb von 15 Sekunden „nicht
  erreichbar“ mit dem Hinweis auf eine aktuelle Visitenkarte.
- **SC-002**: Nach der Annahme in holzis Einladungsabfrage oder dem Absenden eines Beitritts ist der
  Kanal auf beiden Seiten in 95 % der Versuche innerhalb von 3 Sekunden bereit.
- **SC-003**: Ein Gastgeber mit 3 Mitspielern auf getrennten Kanälen tauscht zehn Minuten lang je Kanal
  und Richtung 35 schnelle Nachrichten pro Sekunde zu 1.000 Byte aus; auf einer stabilen Verbindung
  kommen mindestens 99 % an, und holzis Oberfläche reagiert dabei in höchstens 100 ms auf Eingaben.
- **SC-004**: Die Zeit vom Senden einer schnellen Nachricht in einem Rahmen bis zu ihrer Meldung im
  Rahmen der Gegenseite übersteigt die einfache Laufzeit der Verbindung im Median um höchstens 5 ms und
  im 95. Perzentil um höchstens 15 ms.
- **SC-005**: Ohne direkten Weg zwischen den Geräten entsteht der Kanal über ein Relay in 100 % der
  Testfälle, und bei einer Umlaufzeit bis 150 ms gilt SC-003 weiter.
- **SC-006**: In einer Testsammlung entsteht kein Kanal ohne eine in holzis Einladungsabfrage
  angenommene Einladung oder einen gültigen Code einer offenen Lobby: 0 Fälle. Keine Erweiterung der
  eingeladenen Seite erfährt vor der Annahme von einer Einladung: 0 Fälle. Ohne offene öffentliche Lobby werden 100 % der Verbindungen nicht
  bestätigter Geräte abgewiesen, und 100 % der Verbindungen ohne Beitritt sind nach höchstens
  6 Sekunden getrennt.
- **SC-007**: Für unlesbare, unbekannte, geschlossene und mit falschem Geheimnis versehene Codes ist
  die Antwort in 100 % der Versuche gleich, und über 1.000 Versuche je Fall unterscheiden sich die
  Mediane der Antwortzeit um weniger als 1 ms.
- **SC-008**: In einer Testsammlung erreicht keine Nachricht, Einladung oder Meldung eines Kanals einen
  Rahmen einer anderen Erweiterung oder holzis Oberfläche: 0 Fälle.
- **SC-009**: Bei einem Kontakt mit zwei Geräten verschwindet die Einladungsabfrage nach der Annahme
  auf dem einen auf dem anderen innerhalb von 2 Sekunden; eine Annahme dort scheitert in 100 % der Versuche.
- **SC-010**: Schließt eine Seite den Kanal, ihre Vault oder die Erweiterung, erfährt die Gegenseite
  das Ende innerhalb von 2 Sekunden, wenn die Verbindung besteht, und sonst innerhalb von 30 Sekunden.
- **SC-011**: Ein entfernter Beitretender kommt in 100 % der Versuche nicht in dieselbe Lobby zurück.
- **SC-012**: Nach einer Sitzung mit Einladungen, Lobbys und Kanälen findet sich in den gespeicherten
  Daten und Protokollen von holzi keine Spur von Inhalt oder Metadaten dieser Funktion; nur die
  Berechtigung bleibt.

## Assumptions

- Erster Nutzer ist haex-doom: ein Stern mit einem Gastgeber und bis zu 3 Mitspielern, je Mitspieler
  ein Kanal, 35 kleine Spielpakete pro Sekunde und Richtung. Die Zahlen in den Erfolgskriterien sind
  daraus abgeleitet und gelten als Mindestmaß.
- Die Schnittstelle für Erweiterungen (`client.peers` in `haex-space/vault-sdk`) ist eine eigene
  Änderung; ihre Form folgt dem Vertrag im Entwurf von haex-doom, soweit diese Spec nichts anderes
  sagt. Abweichend davon gibt es keine Funktionen zum Annehmen und Ablehnen und keine Meldungen über
  eingehende oder zurückgezogene Einladungen; die Erweiterung der eingeladenen Seite erhält nach der
  Annahme nur den Kanal mit Kontakt und Inhalt. Die Statusmeldung an die einladende Seite bleibt. Der
  Plan legt Methoden, Meldungen und Fehlercodes im Bridge-Vertrag von Spec 017 fest, mit den
  Zahlencodes von holzi.
- Kanäle nutzen den Netzwerkendpunkt und die Relays, die jede geöffnete Vault für den Sync ihrer Geräte
  ohnehin betreibt (Spec 024). Ist die Vault geschlossen, gibt es keine Kanäle; ein Hintergrunddienst
  kommt mit einer eigenen Spec.
- Die erste Fassung erreicht die Geräte eines Kontakts nur über die Adressen aus seiner gespeicherten
  Visitenkarte (Clarifications 2026-10-22). Bekannte Grenze: Ein Gerät, das sein Relay gewechselt hat,
  ist so nicht erreichbar, bis der Kontakt eine aktuelle Visitenkarte teilt. Einladungen wechseln auf
  Anklopfen, sobald Spec 056 es gebaut hat; davon hängt diese Spec ab, nicht umgekehrt. Beitrittscodes
  sind nicht betroffen, weil sie die aktuelle Adresse des Gastgebers tragen.
- Geräte der eigenen Vault gelten als bestätigt. Einladen lässt sich nur ein Kontakt; mit einem Code
  kann auch ein eigenes Gerät beitreten.
- Die Berechtigung `peers` wird wie andere Arten aus dem Manifest bei der Installation bestätigt oder
  bei der ersten Nutzung erfragt; sie ist nicht gerätebezogen.
- Ob die Mitspieler dieselbe Fassung der Erweiterung und dieselben Spieldaten haben, prüft die
  Erweiterung über ihren Inhalt; holzi prüft nur Herausgeberschlüssel und Name.
- Der Hinweis auf die sichtbare Netzwerkadresse steht in der Abfrage der Berechtigung (FR-003); die
  Erweiterung darf ihn in ihrer öffentlichen Lobby wiederholen.
- Dass Geräte fremder Vaults, die kein Kontakt sind, sich für öffentliche Lobbys verbinden dürfen, geht
  über ADR-0015 hinaus; ein neuer ADR hält das bei der Planung fest.
- Die Werte 4 KiB, 64 KiB, 5 Minuten, 5 Sekunden und 30 Sekunden stammen aus dem Entwurf von haex-doom;
  die Obergrenzen nach FR-041 legt der Plan fest.

## Nicht im Umfang

- Erkennung von Geräten oder Lobbys im lokalen Netz (mDNS) und jede Liste offener Lobbys von Kontakten
  oder Fremden.
- Übertragung von Dateien: holzi bietet über Kanäle keine Dateiübertragung, und die Größengrenzen der
  Nachrichten bleiben bestehen. Dateien zwischen Vaults regelt Spec 056.
- Einladungen an Nicht-Kontakte und Einladungen, die zugestellt werden, wenn der Empfänger später
  online kommt.
- Kanäle zu mehr als einem Gerät (Gruppen, Vermaschung) und Weiterleitung über ein drittes Gerät; eine
  Erweiterung baut eine Runde aus mehreren Kanälen selbst.
- Kanäle zwischen verschiedenen Erweiterungen oder zwischen einer Erweiterung und holzi selbst.
- Audio- oder Videoströme und eine Anzeige von Kanälen in holzis Oberfläche.
- Betrieb eigener Relays für diese Funktion; es gelten die Relays aus den Einstellungen der Vault.
