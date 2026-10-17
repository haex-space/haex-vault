# Feature Specification: Passwörter aus haex-vault 2.x (Legacy) übernehmen

**Feature Branch**: `037-haex-vault-import`
**Created**: 2026-10-04
**Status**: Draft
**Input**: Einmalige Migration: Passwörter aus haex-vault 2.x (eingebauter Passwortmanager,
Tabellen `haex_passwords_*`) in den Passwortmanager von holzi (`system.passwords`, Spec 034)
übernehmen. Umsetzung als vierte Importquelle „haex-vault 2.x“ im bestehenden Import: Der Nutzer
wählt die Vault-Datei von haex-vault 2.x, gibt deren Vault-Passwort ein, sieht die bestehende
Vorschau (Anzahl, Doppelte überspringen oder anlegen, Warnungen) und danach den Bericht.
Übernommen wird verlustfrei alles, was der Passwortmanager von haex-vault 2.x speichert. Die
Datei wird nur gelesen, das Vault-Passwort nicht gespeichert. haex-vault 2.x hat keinen Export;
holzi liest die Vault-Datei deshalb direkt. Die Tabellen der alten Erweiterung haex-pass sind
nicht Teil dieser Spec. Referenz: haex-vault 2.x @ `8dce379d94e18fcd42c3b73686a06f984ca3f574`,
`src/database/schemas/passwords.ts`, `src-tauri/database/migrations/0000_jazzy_chat.sql`,
`src/utils/passwords/snapshots.ts`, `src/stores/passwords/groups.ts` und
`src-tauri/src/database/core/init.rs`.

## Beziehung zu bestehenden Specs

- [`034-password-manager`](../034-password-manager/spec.md): Diese Spec erweitert dessen
  Import (User Story 7, FR-023) um eine vierte Quelle. Vorschau, Frage nach Doppelten,
  Bericht, Abbruch mit Aufräumen, Grenze für Anhänge (FR-020) und die Regel „nichts geht still
  verloren“ gelten unverändert. Das Datenmodell von 034 wurde aus haex-vault 2.x übernommen; diese
  Spec ändert es nicht.
- [`036-password-redesign`](../036-password-redesign/spec.md): Die Oberfläche des Imports
  folgt dem Stand von 036; diese Spec fügt dort nur die Quelle hinzu.
- [`024-own-device-sync`](../024-own-device-sync/spec.md): Übernommene Einträge sind
  gewöhnliche Vault-Daten und gehen wie alle anderen über den Sync der eigenen Geräte. Die
  Sync-Angaben aus haex-vault 2.x (Uhrstände, Löschprotokoll) werden nicht übernommen.
- [`014-portable-mode`](../014-portable-mode/spec.md): Der Import liest eine Datei, die der
  Nutzer selbst auswählt; er sucht nicht in den Datenordnern von haex-vault 2.x.

## Clarifications

### Session 2026-10-04 (Planung)

- Q: Lässt sich „falsches Passwort“ von „keine haex-vault-2.x-Datei“ unterscheiden? → A: Nur
  teilweise. Eine verschlüsselte Datei sieht ohne den richtigen Schlüssel wie Zufallsdaten aus;
  holzi meldet dann beides zusammen. Erkennbar sind nur unverschlüsselte Datenbanken und leere
  oder zu kleine Dateien (FR-005, User Story 2).
- Q: Gilt das Wiederverwenden vorhandener Ordner nur für haex-vault 2.x? → A: Nein, für alle Quellen.
  Bisher legte ein wiederholter Import aus KeePass, Bitwarden oder LastPass die Ordner ein
  zweites Mal an (FR-017).
- Q: (Umsetzung) Gilt „0 neue Einträge“ beim zweiten Import auch für den Papierkorb? → A: Nein.
  034 zählt Einträge im Papierkorb bewusst nicht als vorhanden (wer einen Eintrag gelöscht hat,
  soll ihn neu importieren können). Einträge aus dem Papierkorb der Quelle kommen deshalb bei
  jedem Import erneut in den Papierkorb (SC-003, User Story 3).

## User Scenarios & Testing _(mandatory)_

### User Story 1 - Alle Passwörter aus haex-vault 2.x in holzi holen (Priority: P1)

Ein Nutzer hat seine Passwörter bisher in haex-vault 2.x verwaltet und steigt auf holzi um. Er
kopiert die Vault-Datei von haex-vault 2.x auf das Gerät, öffnet in holzi den Import des
Passwortmanagers, wählt die Quelle „haex-vault 2.x“ und die Datei und gibt das Vault-Passwort von
haex-vault 2.x ein. holzi zeigt die Vorschau: wie viele Einträge, Ordner, Tags, Anhänge,
Verlaufsstände und Passkeys gefunden wurden, wie viele davon im Papierkorb liegen und wie viele
es in holzi schon gibt. Der Nutzer startet den Import. Danach findet er jeden Eintrag mit allen
Angaben am gleichen Ort wie in haex-vault 2.x und kann ihn sofort benutzen (Passwort kopieren,
TOTP-Code ablesen, Anhang öffnen).

**Why this priority**: Das ist der eigentliche Zweck der Spec. haex-vault 2.x hat keinen Export;
ohne diesen Weg müsste der Nutzer jeden Eintrag von Hand abtippen.

**Independent Test**: Eine verschlüsselte haex-vault-2.x-Vault-Datei mit bekannten Einträgen
(alle Felder gefüllt, Ordner in mehreren Ebenen, Tags, eigene Felder, TOTP mit abweichenden
Einstellungen, Ablaufdatum, Farbe, eigenes Symbol, Anhang, Verlauf, Passkey, Eintrag im
Papierkorb) importieren und Feld für Feld mit der Quelle vergleichen.

**Acceptance Scenarios**:

1. **Given** eine haex-vault-2.x-Vault-Datei und ihr richtiges Vault-Passwort, **When** der Nutzer
   die Vorschau öffnet, **Then** nennt holzi die Zahl der Einträge, Ordner, Tags, Anhänge,
   Verlaufsstände, Passkeys und der Einträge im Papierkorb, und noch ist nichts geschrieben.
2. **Given** die Vorschau, **When** der Nutzer den Import startet, **Then** gibt es danach jeden
   Eintrag der Quelle mit Titel, Benutzername, Passwort, Adresse, Notiz, Symbol, Farbe,
   TOTP-Secret samt Ziffern, Periode und Algorithmus, Ablaufdatum, Autofill-Aliasen,
   Erstellungs- und Änderungszeit, eigenen Feldern, Tags (mit Farbe), Anhängen (mit
   Dateiname), Verlaufsständen (mit ihren Anhängen) und Passkeys, im Ordner mit demselben Pfad
   wie in haex-vault 2.x.
3. **Given** Ordner in haex-vault 2.x mit Beschreibung, Symbol, Farbe und Reihenfolge, **When** der
   Nutzer importiert, **Then** haben die Ordner in holzi dieselben Angaben und dieselbe
   Verschachtelung.
4. **Given** Einträge im Papierkorb von haex-vault 2.x, **When** der Nutzer importiert, **Then**
   liegen sie im Papierkorb von holzi und nicht in der Liste der aktiven Einträge.
5. **Given** ein Eintrag mit eigenem Symbol (Bild), **When** der Nutzer importiert, **Then**
   zeigt der Eintrag in holzi dasselbe Bild.
6. **Given** ein Passkey in haex-vault 2.x, **When** der Nutzer importiert, **Then** hat er in holzi
   dieselbe Relying Party, denselben Nutzer, privaten und öffentlichen Schlüssel, Algorithmus,
   Zähler, Spitzname und Zeitpunkt der letzten Nutzung, am selben Eintrag oder ohne Eintrag,
   wenn er in haex-vault 2.x keinen hatte.
7. **Given** Voreinstellungen des Passwortgenerators in haex-vault 2.x, **When** der Nutzer
   importiert, **Then** stehen sie im Generator von holzi zur Auswahl.
8. **Given** der Import ist fertig, **When** der Nutzer den Bericht öffnet, **Then** nennt er die
   Zahl der übernommenen Einträge, Ordner, Anhänge und Passkeys, die übersprungenen Doppelten
   und jede Stelle, die nicht verlustfrei ankam; Geheimnisse stehen nicht darin.

---

### User Story 2 - Falsche Datei, falsches Passwort oder Abbruch ohne Schaden (Priority: P1)

Der Nutzer vertippt sich beim Vault-Passwort, wählt aus Versehen eine andere Datei oder bricht
den Import ab. holzi sagt verständlich, was los ist, und weder die Datei von haex-vault 2.x noch
die Vault von holzi werden verändert.

**Why this priority**: Es geht um alle Passwörter des Nutzers. Ein Fehler darf weder die alte
Quelle beschädigen noch die neue Vault halb gefüllt zurücklassen.

**Independent Test**: Mit falschem Passwort, mit einer Datei, die keine haex-vault-2.x-Vault ist,
mit einer haex-vault-2.x-Vault ohne Passwortmanager-Daten und mit einem Abbruch mitten im Import
jeweils prüfen: Meldung erscheint, die Quelldatei ist Byte für Byte unverändert, die Vault
von holzi ist wie vorher.

**Acceptance Scenarios**:

1. **Given** eine haex-vault-2.x-Vault-Datei, **When** der Nutzer ein falsches Vault-Passwort
   eingibt, **Then** sagt holzi, dass das Passwort nicht passt oder die Datei keine
   haex-vault-2.x-Vault ist, lässt ihn es erneut versuchen und schreibt nichts.
2. **Given** eine Datei, die erkennbar keine haex-vault-2.x-Vault ist (unverschlüsselte Datenbank,
   leere oder zu kleine Datei), **When** der Nutzer sie wählt, **Then** sagt holzi, dass das
   Format nicht unterstützt wird, und schreibt nichts.
3. **Given** eine haex-vault-2.x-Vault ohne Passwortmanager-Tabellen, **When** der Nutzer sie wählt,
   **Then** sagt holzi, dass diese Vault keine Passwörter enthält, und schreibt nichts.
4. **Given** ein laufender Import, **When** der Nutzer abbricht oder ein Fehler den Import
   insgesamt stoppt, **Then** entfernt holzi alles, was dieser Import angelegt hat.
5. **Given** ein beliebiger Ablauf dieser Story oder von User Story 1, **When** er endet,
   **Then** ist die gewählte Datei von haex-vault 2.x (samt zugehöriger Begleitdateien)
   unverändert, und das Vault-Passwort von haex-vault 2.x ist nirgends gespeichert.

---

### User Story 3 - Import wiederholen ohne Doppelte (Priority: P2)

Der Nutzer hat in haex-vault 2.x nach dem ersten Import noch Einträge angelegt, oder der erste
Import wurde unterbrochen. Er importiert dieselbe Datei erneut und bekommt nur, was noch fehlt.

**Why this priority**: Ein Umstieg geschieht selten an einem Tag. Wiederholen muss gefahrlos
sein, ist für den ersten Umstieg aber nicht nötig.

**Independent Test**: Dieselbe Datei zweimal importieren; beim zweiten Mal mit „Doppelte
überspringen“ entstehen außerhalb des Papierkorbs keine neuen Einträge, nach einem in
haex-vault 2.x ergänzten Eintrag genau einer.

**Acceptance Scenarios**:

1. **Given** eine schon einmal importierte Datei, **When** der Nutzer die Vorschau öffnet,
   **Then** zählt holzi die schon vorhandenen Einträge als Doppelte und bietet wie bei den
   anderen Quellen „überspringen“ (Vorgabe) oder „trotzdem anlegen“ an.
2. **Given** „überspringen“, **When** der Nutzer importiert, **Then** entstehen nur Einträge, die
   es noch nicht gibt, und Ordner und Tags werden nicht doppelt angelegt. Einträge im Papierkorb
   zählen nach 034 nicht als vorhanden; Einträge aus dem Papierkorb der Quelle landen deshalb
   erneut im Papierkorb.
3. **Given** ein Passkey, dessen Credential-ID es in holzi schon gibt, **When** der Nutzer
   importiert, **Then** legt holzi ihn nicht doppelt an und nennt ihn im Bericht.

---

### Edge Cases

- **haex-vault 2.x lief beim Kopieren noch oder wurde nicht sauber beendet.** Dann stehen die
  letzten Änderungen in einer Begleitdatei neben der Vault-Datei. Liegt sie daneben, übernimmt
  holzi den Stand einschließlich dieser Änderungen; holzi verändert und löscht sie nie. Ob eine
  Begleitdatei fehlt, lässt sich nicht erkennen (nach sauberem Beenden gibt es keine); der Import
  weist deshalb bei dieser Quelle immer darauf hin, haex-vault 2.x vor dem Kopieren zu schließen.
- **Die Datei liegt auf einem schreibgeschützten Ort** (USB-Stick, Freigabe ohne Schreibrecht).
  Der Import funktioniert trotzdem.
- **Ein Anhang oder ein eigenes Symbol ist größer als das Limit aus FR-020 von 034** oder lässt
  sich nicht lesen (beschädigte Daten). holzi übernimmt den Eintrag ohne ihn und nennt Eintrag,
  Dateiname und Größe im Bericht.
- **Ein Eintrag verweist auf ein Bild oder einen Anhang, das in haex-vault 2.x fehlt.** holzi
  übernimmt den Eintrag mit Standardsymbol bzw. ohne den Anhang und nennt es im Bericht.
- **Ein Symbol ist ein Name, den holzi nicht kennt.** holzi setzt sein Standardsymbol und nennt
  den Eintrag im Bericht.
- **Ein TOTP-Secret oder eine TOTP-Einstellung ist ungültig.** holzi übernimmt den Wert wie er
  ist und nennt ihn im Bericht (wie FR-023 von 034).
- **Ein Verlaufsstand ist kein lesbarer Stand von haex-vault 2.x** (beschädigtes oder fremdes
  Format). holzi übernimmt den Eintrag ohne diesen Stand und nennt ihn im Bericht.
- **Ein Ordner in haex-vault 2.x verweist auf einen übergeordneten Ordner, den es nicht gibt, oder
  die Ordner bilden einen Kreis.** holzi hängt den betroffenen Ordner oben ein und nennt ihn im
  Bericht.
- **Ein Ordner im Papierkorb von haex-vault 2.x.** Er landet mit seinem Inhalt im Papierkorb von
  holzi. haex-vault 2.x merkt sich nicht, woher etwas in den Papierkorb kam; beim Wiederherstellen
  landet es deshalb oben.
- **Zwei Tags unterscheiden sich nur in der Schreibweise.** holzi führt sie nach seiner
  Tag-Regel zu einem zusammen und nennt das im Bericht.
- **Die Vault-Datei stammt aus einer anderen haex-vault-2.x-Version** und hat zusätzliche Spalten
  oder Tabellen im Passwortmanager. holzi übernimmt, was es kennt, und nennt im Bericht, welche
  unbekannten Angaben nicht übernommen wurden. Fehlt eine Tabelle oder Spalte, die holzi
  braucht, bricht holzi vor dem Schreiben mit einer Meldung ab.
- **Die Vault ist groß** (mehrere tausend Einträge, viele Anhänge). Die Vorschau zeigt den
  Fortschritt, der Import bleibt abbrechbar.
- **Der Nutzer hat in haex-vault 2.x das Vault-Passwort geändert.** Es gilt das aktuelle
  Vault-Passwort der Datei; ein altes schlägt wie ein falsches fehl.

## Requirements _(mandatory)_

### Functional Requirements

**Quelle und Zugang**

- **FR-001**: Der Import des Passwortmanagers MUSS „haex-vault 2.x“ als weitere Quelle neben
  KeePass, Bitwarden und LastPass anbieten, auf allen Plattformen, auf denen es den Import gibt.
- **FR-002**: Für diese Quelle MUSS der Nutzer eine Vault-Datei von haex-vault 2.x auswählen und
  ihr Vault-Passwort eingeben; holzi MUSS die Datei damit so öffnen, wie haex-vault 2.x es tut
  (Verschlüsselung der Vault-Datei mit dem Vault-Passwort, Standardeinstellungen von haex-vault 2.x
  @ `8dce379`).
- **FR-003**: holzi DARF die gewählte Datei und ihre Begleitdateien nie verändern, anlegen oder
  löschen; das MUSS auch gelten, wenn der Ort beschreibbar ist. Liegt eine Begleitdatei mit noch
  nicht übernommenen Änderungen neben der Vault-Datei, MUSS holzi deren Stand mit einlesen.
- **FR-004**: Das Vault-Passwort von haex-vault 2.x DARF weder gespeichert noch protokolliert noch
  in den Bericht geschrieben werden und MUSS nach dem Import aus dem Speicher entfernt werden,
  wie die Zugangsdaten der KeePass-Quelle in 034.
- **FR-005**: holzi MUSS vor jedem Schreiben unterscheiden und dem Nutzer verständlich melden:
  Vault-Passwort passt nicht oder die Datei ist keine haex-vault-2.x-Vault (eine verschlüsselte Datei
  verrät ohne den richtigen Schlüssel nicht, was sie ist), Datei ist erkennbar keine
  haex-vault-2.x-Vault, Vault enthält keine Passwortmanager-Daten, Vault hat einen Aufbau, den holzi
  nicht lesen kann (fehlende Tabelle oder Spalte). In keinem dieser Fälle DARF etwas geschrieben
  werden.

**Was übernommen wird**

- **FR-006**: Der Import MUSS jeden Eintrag mit allen Feldern übernehmen: Titel, Benutzername,
  Passwort, Adresse, Notiz, Symbol, Farbe, TOTP-Secret, Ziffern, Periode, Algorithmus,
  Ablaufdatum, Autofill-Aliase, Erstellungs- und Änderungszeit, sowie seine eigenen Felder in
  derselben Reihenfolge.
- **FR-007**: Der Import MUSS die Ordner mit Name, Beschreibung, Symbol, Farbe, Reihenfolge und
  Verschachtelung übernehmen und jeden Eintrag in den Ordner mit demselben Pfad legen. Einträge
  ohne Ordner landen oben.
- **FR-008**: Einträge und Ordner im Papierkorb von haex-vault 2.x MÜSSEN im Papierkorb von holzi
  landen.
- **FR-009**: Der Import MUSS Tags mit Namen und Farbe übernehmen und den Einträgen zuordnen;
  vorhandene Tags gleichen Namens MÜSSEN wiederverwendet werden.
- **FR-010**: Der Import MUSS Anhänge mit Dateinamen und Inhalt übernehmen und eigene Symbole
  (Bilder) von Einträgen und Ordnern als Bild; der Inhalt MUSS Byte für Byte gleich sein.
  Anhänge über dem Limit aus FR-020 von 034 MÜSSEN wie bei den anderen Quellen abgelehnt und
  im Bericht genannt werden.
- **FR-011**: Der Import MUSS den Verlauf jedes Eintrags übernehmen: jeden Verlaufsstand mit
  seinen Feldern, Tags, eigenen Feldern, Anhängen und seinem Zeitpunkt.
- **FR-012**: Der Import MUSS jeden Passkey mit allen Angaben übernehmen (Credential-ID, Relying
  Party mit Name, Nutzerkennung, Nutzername, Anzeigename, privater und öffentlicher Schlüssel,
  Algorithmus, Zähler, auffindbar ja/nein, Symbol, Farbe, Spitzname, Erstellungszeit, letzte
  Nutzung), am zugehörigen Eintrag oder ohne Eintrag. Weil haex-vault 2.x den öffentlichen Schlüssel
  mitliefert, MUSS holzi ihn übernehmen statt ihn abzuleiten.
- **FR-013**: Der Import MUSS die Voreinstellungen des Passwortgenerators übernehmen; eine
  Voreinstellung mit gleichem Namen wie eine vorhandene wird nicht doppelt angelegt.
- **FR-014**: Was die Quelle enthält und holzi nicht abbilden kann (unbekanntes Symbol,
  unbekannte Spalten oder Tabellen im Passwortmanager, unlesbarer Verlaufsstand, fehlender
  Verweis, zusammengeführte Tags, umgehängter Ordner), MUSS im Bericht mit Eintrag oder Ordner
  und dem Fehlenden stehen. Nichts DARF still verloren gehen.
- **FR-015**: Sync-Angaben von haex-vault 2.x (Uhrstände je Zeile und Spalte, Signaturen,
  Löschprotokoll) und Daten außerhalb des Passwortmanagers DÜRFEN NICHT übernommen werden;
  übernommene Daten sind in holzi gewöhnliche, neu angelegte Vault-Daten.

**Ablauf (wie die anderen Quellen)**

- **FR-016**: Vorschau, Frage nach Doppelten (Vorgabe „überspringen“), Fortschritt, Abbruch mit
  Entfernen alles Angelegten, Bericht ohne Geheimnisse und Speichern des Berichts MÜSSEN für
  diese Quelle genauso funktionieren wie für die anderen Quellen nach FR-023 von 034. Die
  Vorschau MUSS zusätzlich die Zahl der Tags, Passkeys und Generator-Voreinstellungen nennen.
- **FR-017**: Doppelte MÜSSEN nach derselben Regel wie bei den anderen Quellen erkannt werden;
  Passkeys mit vorhandener Credential-ID MÜSSEN nach 034 behandelt werden. Ein Ordner der
  Quelle MUSS einen vorhandenen Ordner mit demselben Namen am selben Ort wiederverwenden, statt
  einen zweiten anzulegen; das gilt für alle Importquellen, nicht nur für haex-vault 2.x.
  Voreinstellungen des Generators mit vorhandenem Namen werden nicht doppelt angelegt.

### Key Entities

- **haex-vault-2.x-Vault-Datei**: Verschlüsselte Datenbankdatei von haex-vault 2.x, geöffnet mit dem
  Vault-Passwort; liegt beim Nutzer, wird nur gelesen. Kann eine Begleitdatei mit noch nicht
  übernommenen Änderungen haben.
- **Quelldaten des Passwortmanagers**: Einträge, eigene Felder, Ordner (mit Papierkorb als
  festem Ordner), Tags, Binärdaten (Anhänge und Bilder), Verlaufsstände, Passkeys und
  Generator-Voreinstellungen von haex-vault 2.x; sie entsprechen fast 1:1 dem Datenmodell von 034.
- **Importbericht**: wie in 034; ergänzt um die Fälle aus FR-014.

## Success Criteria _(mandatory)_

### Measurable Outcomes

- **SC-001**: Beim Import einer Beispiel-Vault, in der jedes Feld aus FR-006 bis FR-013 belegt
  ist, stimmen 100 % der Felder, Anhänge, Verlaufsstände und Passkeys mit der Quelle überein;
  jede Abweichung steht im Bericht (0 stille Verluste).
- **SC-002**: Nach jedem Ablauf, auch nach falschem Passwort, Abbruch und Fehler, ist die
  Quelldatei samt Begleitdateien Byte für Byte unverändert.
- **SC-003**: Ein zweiter Import derselben Datei mit „überspringen“ legt außerhalb des Papierkorbs
  0 neue Einträge und insgesamt 0 neue Ordner, Tags, Passkeys und Voreinstellungen an.
- **SC-004**: Der Nutzer kommt von „Import öffnen“ bis zum fertigen Bericht in höchstens fünf
  Schritten (Quelle wählen, Datei wählen, Passwort eingeben, Vorschau bestätigen, Bericht).
- **SC-005**: Eine Vault mit 1.000 Einträgen ist auf einem gewöhnlichen Laptop in unter einer
  Minute importiert; die Oberfläche bleibt dabei bedienbar und der Import abbrechbar.

## Assumptions

- Der Nutzer hat in haex-vault 2.x den eingebauten Passwortmanager benutzt (seit 2026-04). Daten
  der alten Erweiterung haex-pass (Tabellen mit Präfix der Erweiterung) sind nicht Teil dieser
  Spec; liegen solche Tabellen in der Datei, nennt die Vorschau sie, übernimmt sie aber nicht.
- Der Nutzer kopiert die Vault-Datei selbst auf das Gerät, auf dem holzi läuft; holzi sucht
  nicht nach haex-vault-2.x-Installationen und greift nicht übers Netz auf ein anderes Gerät zu.
- Maßgeblich ist der Aufbau des Passwortmanagers von haex-vault 2.x @ `8dce379d94e1`; haex-vault 2.x
  wird durch holzi abgelöst, weitere Versionen sind nicht zu erwarten.
- Die Quelle bleibt dauerhaft im Import, damit jeder Umsteiger von haex-vault 2.x sie nutzen kann;
  „einmalig“ heißt, dass ein Nutzer sie in der Regel einmal braucht.
- Übernommene Einträge bekommen neue Kennungen in holzi; Doppelte erkennt holzi wie bei den
  anderen Quellen am Inhalt. Wer auf zwei eigenen Geräten gleichzeitig importiert, bevor der
  Sync sie abgleicht, kann Doppelte bekommen; dafür genügt ein Import auf einem Gerät.
- Symbolnamen aus haex-vault 2.x, die holzi kennt, bleiben erhalten; was holzi nicht kennt, ersetzt
  das Standardsymbol (FR-014).
- Einträge, die haex-vault 2.x selbst angelegt hat (etwa Zugangsdaten eines S3-Speichers mit Titel
  `iam-admin:…`), sind gewöhnliche Einträge und werden wie alle anderen übernommen.
- Hat die Vault in haex-vault 2.x eine Farbe für ein Tag und das gleichnamige Tag in holzi keine,
  bekommt es die Farbe der Quelle; eine vorhandene Farbe in holzi bleibt.

## Nicht im Umfang

- Tabellen der alten Erweiterung haex-pass.
- Übernahme anderer haex-vault-2.x-Daten (Erweiterungen, Einstellungen, Dateien, Geräte, Spaces).
- Export aus holzi oder Rückweg nach haex-vault 2.x.
- Automatisches Auffinden der Vault-Datei oder Import über das Netz von einem anderen Gerät.
