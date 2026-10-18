# Feature Specification: Passwortmanager

**Feature Branch**: `034-password-manager`
**Created**: 2026-10-02
**Status**: Draft
**Input**: holzi bekommt einen festen Passwortmanager, der Zugangsdaten und andere
Geheimnisse in der Vault verwahrt. Vorlage ist der Passwortmanager von haex-vault 2.x (Legacy)
(Einträge, Ordner, Tags, eigene Felder, TOTP, Passkeys als Daten, Passwortgenerator,
Verlauf, Anhänge, Papierkorb, Import aus KeePass, Bitwarden und LastPass, Zugriff für
Erweiterungen über Tag-Berechtigungen). Das Datenmodell wird möglichst 1:1 übernommen;
die Oberfläche wird ein App-Fenster im Window Manager (wm) im Stil von holzi. Die
Anbindung der External Bridge (Autofill, Passkey-Signieren, Browser-Erweiterung) ist
ausdrücklich nicht Teil dieser Spec. Referenz: haex-vault 2.x @
`8dce379d94e18fcd42c3b73686a06f984ca3f574`, `src/database/schemas/passwords.ts`,
`src/components/haex/system/passwords/`, `src-tauri/src/passwords/` und
`src-tauri/src/extension/permissions/manager/check/passwords.rs`.

## Beziehung zu bestehenden Specs

- [`015-workspace-shell`](../015-workspace-shell/spec.md) und
  [`030-app-multi-instance`](../030-app-multi-instance/spec.md): Der Passwortmanager ist
  eine App im Window Manager wie Chat und Einstellungen; Fenster, Tabs und mehrere
  Instanzen folgen diesen Specs.
- [`020-tab-navigation`](../020-tab-navigation/spec.md): Jeder Ort im Passwortmanager
  (Ordner, Eintrag, Papierkorb) ist ein Ort im Tab mit Vor, Zurück und Verlaufsliste.
  Seine agentenfähigen Aktionen stehen im Aktionskatalog.
- [`022-session-restore`](../022-session-restore/spec.md): Ein Passwortmanager-Tab kommt
  mit Ort und Verlauf zurück, aber nie mit entsperrten oder aufgedeckten Geheimnissen.
- [`024-own-device-sync`](../024-own-device-sync/spec.md): Alle Tabellen des
  Passwortmanagers sind Vault-Daten und gehen wie alle Vault-Daten über den Sync der
  eigenen Geräte (iroh direkt, Relay optional). Diese Spec baut keinen eigenen Sync.
- [`025-own-device-file-sync`](../025-own-device-file-sync/spec.md): Dateisync betrifft
  Ordner im Dateisystem, nicht die Anhänge der Einträge; sie liegen in der Vault-Datenbank.
- [`029-own-s3-storage`](../029-own-s3-storage/spec.md): Diese Spec liefert den
  Passwortmanager, den 029 voraussetzt. Die Hauptzugangsdaten des Admins und die
  Zugangsschlüssel liegen dort als gewöhnliche Einträge (FR-033 bis FR-035).
- [`032-model-operates-holzi`](../032-model-operates-holzi/spec.md): Dessen Regel, dass
  Zugangsdaten und Schlüssel nie im Ergebnis einer Aktion an ein Modell gelangen
  (FR-010 dort), gilt hier für den eingebauten Agenten ohne Ausnahme (FR-027).
- Geplante Specs **017–019 (haextensions)** und **021 (MCP-Server, Freigaben je Agent)**:
  Sie vergeben und verwalten Freigaben und zeigen Anfragen. Diese Spec legt fest, was
  eine Freigabe für Passwörter bedeutet und wie der Passwortmanager sie prüft
  (FR-024 bis FR-032).

## Clarifications

### Session 2026-10-02

- Q: Werden Geheimnisse zusätzlich zur Vault-Verschlüsselung verschlüsselt? → A: Nein.
  Passwörter, TOTP-Secrets und private Passkey-Schlüssel liegen unverschlüsselt in der
  Vault-Datenbank (wie in haex-vault 2.x); geschützt sind sie durch die Verschlüsselung der
  Vault und durch Berechtigungen für jeden Zugriff von außen.
- Q: Gehört die External Bridge (Browser-Erweiterung, Autofill, Passkey-Signieren) zum
  ersten Wurf? → A: Nein, sie kommt später. Der Passwortmanager wird so gebaut, dass sie
  als weiterer Aufrufer an dieselbe Zugriffsprüfung andocken kann, ohne das Datenmodell
  zu ändern.
- Q: Wie liegen Dateianhänge in der Datenbank? → A: Wie in haex-vault 2.x in der Vault-Datenbank
  und per SHA-256 dedupliziert, aber als Binärdaten (BLOB) statt als Base64-Text. Das ist
  ein bewusster Bruch mit haex-vault 2.x: Die Tabelle der Binärdaten ist nicht mit der von
  haex-vault 2.x austauschbar.
- Q: Über welchen Weg synchronisieren Passwörter und Anhänge? → A: Wie alle Vault-Daten
  über den Datensync von Spec 024 (iroh direkt zwischen den eigenen Geräten, Relay
  optional). Das Sync-Postfach eines Relays ist nur ein zusätzlicher Weg; ein zu großer
  Anhang kommt dann weiter direkt an (Spec 026).
- Q: Wie groß darf ein einzelner Anhang höchstens sein? → A: 25 MiB. Der Wert schützt die
  Größe der Vault-Datenbank, nicht den Sync: Ein Anhang über der Obergrenze des Relay-Postfachs
  (Spec 026) käme weiter direkt über iroh an.
- Q: (Planung) Welche Abweichungen vom Datenmodell von haex-vault 2.x sind nötig? → A: Keine
  UNIQUE-Constraints (ein Konflikt hält den Sync an; abgeleitete Kennungen für Tags,
  Tag-Zuordnungen und Passkeys), zwei Spalten für den früheren Ort im Papierkorb, `RESTRICT` bei
  den Verweisen auf Binärdaten. Der Verlauf speichert den neuen Zustand; Aufräumen mit Karenzzeit
  von sieben Tagen ab dem Verlust des letzten bekannten Verweises (geprüft beim nächsten Öffnen); PDFs nur zum Herunterladen; der eingebaute Agent sieht nur Titel, Tags und
  Ordnernamen (Plan, R2–R6, R18).
- Q: (Analyse) Müssen Passkeys im ersten Wurf auch aus Importen kommen? → A: Ja (Bitwarden
  und KeePassXC). haex-vault 2.x importiert keine Passkeys; es erzeugt sie selbst über die Bridge und
  hat deshalb beide Schlüssel. Importquellen liefern nur den privaten Schlüssel, der öffentliche
  wird abgeleitet (ES256, EdDSA, RS256; Details im Plan).
- Q: (Analyse) Läuft die Oberfläche am Dienst vorbei? → A: Nein. Alles läuft über den Dienst; die
  Oberfläche ist der Aufrufer „Nutzer“.
- Q: (Analyse) Dürfen ungültige Secrets angelegt werden? → A: Nein, das wird abgelehnt und
  fehlende Angaben bekommen Standardwerte; was per Sync oder Import ungültig ankommt, muss
  erkannt und behebbar sein.
- Q: (Klärung) Verschiebt Löschen durch eine holzi-Funktion, Erweiterung oder einen Agenten in den
  Papierkorb oder entfernt es endgültig? → A: In den Papierkorb, wie bei jedem Aufrufer. Endgültig
  entfernt wird erst, wenn der Nutzer einen Eintrag im Papierkorb löscht.
- Q: (Klärung) Sind „Work“ und „work“ derselbe Tag? → A: Ja, Tagnamen sind ohne Rücksicht auf
  Groß-/Kleinschreibung und Umlautform eindeutig; die Schreibweise des ersten Anlegers bleibt.
- Q: (Klärung) Sieht ein Aufrufer mit Freigabe für „s3“ auch die anderen Tags eines Eintrags? →
  A: Ja, er darf sie sehen, aber Tags außerhalb seines Bereichs darf er auf keinen Fall ändern
  oder löschen; sie bleiben bei jeder seiner Änderungen unverändert.
- Q: (Klärung) Ist das Tag-Präfix „holzi:“ für holzi-Funktionen reserviert? → A: Nein, es gibt
  keine reservierten Tags. Die Warnung vor dem Löschen beruht darauf, dass eine holzi-Funktion
  ihre genutzten Einträge selbst meldet.
- Q: (Klärung) Darf ein Eintrag ohne Titel gespeichert werden? → A: Ja, auch in der Oberfläche;
  der Titel ist nicht Pflicht. Die Oberfläche zeigt dann den Platzhalter „(ohne Titel)“.
- Q: (Klärung) Was darf beim Import wegfallen? → A: Nichts, der Import übernimmt immer alles:
  Papierkorb, Verlauf, Symbole, alle Passkeys, ungültige TOTP-Secrets, alle Typen und Felder von
  Bitwarden (was kein eigenes Feld hat, kommt als eigenes Feld oder Tag). Anhänge über 25 MiB werden
  abgelehnt, aber im Bericht genannt; Fehler im Import stehen im Bericht, damit der Nutzer von
  Hand nacharbeiten kann. Die Exportdatei wird vor dem Parsen höchstens 512 MiB gepuffert; größere
  Dateien werden mit einer verständlichen Meldung abgelehnt. Eine KeePass-Schlüsseldatei wird nach
  den KeePass-Regeln verarbeitet und nicht mit diesem Exportlimit verwechselt. Der Import schreibt
  in mehreren Schritten und macht bei einem
  Gesamtfehler oder Abbruch alles rückgängig (Option A).
- Q: (Analyse 2) Was dürfen Aufrufer von außen mit Einträgen im Papierkorb tun? → A: Nichts, sie
  sind für sie nicht vorhanden; sonst könnte ein zweites „Löschen“ endgültig entfernen. Benutzte
  eigene Symbole aus dem Import zählen beim Aufräumen als benutzt; ein Passkey mit schon vorhandener
  Credential-ID steht im Importbericht.
- Q: (Analyse) Wie hart sind Zeitvorgaben? → A: Grobe Zielgrenzen genügen; es gibt keine
  Messaufgaben für Zeiten.
- Q: Soll das Datenmodell von haex-vault 2.x 1:1 übernommen werden? → A: Ja, Tabellen- und
  Spaltennamen und deren Bedeutung bleiben, mit der einen Ausnahme der Binärdaten.

## User Scenarios & Testing _(mandatory)_

### User Story 1 - Zugangsdaten ablegen, finden und benutzen (Priority: P1)

Ein Nutzer öffnet den Passwortmanager, legt einen Eintrag für ein Konto an (Titel,
Benutzername, Passwort, Adresse, Notiz, Symbol und Farbe, optional ein
Ablaufdatum und ein TOTP-Secret) und findet ihn später über die Suche wieder. Er kopiert
Benutzername oder Passwort, deckt das Passwort bei Bedarf auf, sieht den aktuellen
TOTP-Code mit Restzeit und kopiert ihn. Eigene Felder wie „Wiederherstellungscode“ oder
„PIN“ kann er hinzufügen.

**Why this priority**: Ohne Einträge, die sich anlegen, finden und benutzen lassen, gibt
es keinen Passwortmanager; alle weiteren Stories bauen darauf auf.

**Independent Test**: Einen Eintrag mit Passwort und TOTP-Secret anlegen, per Suche finden,
das Passwort kopieren, den TOTP-Code anzeigen und mit einem Referenzwert für den
Zeitpunkt vergleichen. Das Ergebnis ist der Kern des Passwortmanagers.

**Acceptance Scenarios**:

1. **Given** die Vault ist offen, **When** der Nutzer einen Eintrag mit Titel und Passwort
   speichert, **Then** erscheint er in der Liste und nach einem Neustart von holzi wieder.
2. **Given** ein Eintrag mit Passwort, **When** der Nutzer ihn öffnet, **Then** ist das
   Passwort verdeckt, bis er es aufdeckt (Halten mit der Maus oder Tippen auf Mobilgeräten),
   und es wird nie in der Verlaufsliste des Tabs oder im Fenstertitel angezeigt.
3. **Given** ein Eintrag mit TOTP-Secret, **When** der Nutzer ihn öffnet, **Then** zeigt er
   den aktuellen Code und die Restzeit und wechselt zum nächsten Code, ohne dass der Nutzer
   etwas tut.
4. **Given** der Nutzer hat ein Passwort kopiert, **When** die eingestellte Zeit abläuft,
   **Then** wird die Zwischenablage geleert, sofern sie noch dasselbe Passwort enthält.
5. **Given** mehrere Einträge, **When** der Nutzer in der Suche einen Teil von Titel,
   Benutzername oder Adresse eingibt, **Then** bleiben genau die passenden Einträge übrig.
   _Geändert 2026-10-06 (PR #290):_ auch mit einem Tippfehler, die besten Treffer zuerst
   (FR-007).
6. **Given** ein Eintrag, **When** der Nutzer ein eigenes Feld hinzufügt, ändert oder löscht,
   **Then** bleibt das nach dem Speichern erhalten und gehört zu diesem Eintrag.
7. **Given** ein Eintrag mit Ablaufdatum in der Vergangenheit, **When** die Liste angezeigt
   wird, **Then** ist er als abgelaufen gekennzeichnet.

---

### User Story 2 - Einträge ordnen mit Ordnern und Tags (Priority: P1)

Ein Nutzer legt Ordner an (auch verschachtelt), verschiebt Einträge hinein und vergibt Tags
(„Arbeit“, „Bank“). Er wählt mehrere Einträge zugleich aus und verschiebt, taggt oder löscht
sie gemeinsam. Tags sind frei vergebene Bezeichnungen und der Maßstab für die Reichweite von
Berechtigungen (US6).

**Why this priority**: Ab ein paar Dutzend Einträgen ist der Passwortmanager ohne Ordnung
nicht mehr benutzbar, und Tags tragen das Berechtigungsmodell.

**Independent Test**: Zwei verschachtelte Ordner anlegen, drei Einträge verschieben, zwei
Tags vergeben, per Mehrfachauswahl zwei Einträge in einen anderen Ordner verschieben und
nach den Tags filtern.

**Acceptance Scenarios**:

1. **Given** ein Eintrag, **When** der Nutzer ihn in einen Ordner verschiebt, **Then** liegt
   er in genau diesem einen Ordner und nicht mehr im vorherigen.
2. **Given** ein Ordner mit Unterordnern, **When** der Nutzer den Ordner umbenennt, ihm ein
   Symbol oder eine Farbe gibt oder die Reihenfolge ändert, **Then** bleibt das erhalten.
3. **Given** Einträge mit Tags, **When** der Nutzer nach einem Tag filtert, **Then** sieht er
   alle Einträge mit diesem Tag, auch aus verschiedenen Ordnern.
4. **Given** ein Tag, **When** der Nutzer ihn umbenennt oder löscht, **Then** gilt das für
   alle Einträge, die ihn tragen; ein Tagname kommt in der Vault nur einmal vor.
5. **Given** mehrere ausgewählte Einträge, **When** der Nutzer sie löscht, verschiebt oder
   mit einem Tag versieht, **Then** gilt die Aktion für alle, und der Nutzer sieht vorher,
   wie viele betroffen sind.
6. **Given** ein Ordner mit Einträgen, **When** der Nutzer ihn löscht, **Then** landet er samt
   Inhalt im Papierkorb (US4) und nicht endgültig gelöscht.

---

### User Story 3 - Starke Passwörter erzeugen (Priority: P1)

Beim Anlegen oder Ändern eines Eintrags öffnet der Nutzer den Passwortgenerator, wählt Länge,
Zeichenarten, ausgeschlossene Zeichen oder ein Muster, sieht das Ergebnis und übernimmt es.
Er speichert eine Konfiguration als Voreinstellung, macht eine zur Standardvoreinstellung und
benutzt sie beim nächsten Mal sofort.

**Why this priority**: Ein Passwortmanager, der keine Passwörter erzeugt, wird zum Ablageort
für schwache Passwörter. Der Generator ist klein und liefert sofortigen Nutzen.

**Independent Test**: Voreinstellung mit Länge 24 ohne Sonderzeichen anlegen, als Standard
setzen, im Editor ein Passwort erzeugen und prüfen, dass es die Regeln einhält.

**Acceptance Scenarios**:

1. **Given** Länge 20 mit Groß- und Kleinbuchstaben und Ziffern, **When** der Nutzer ein
   Passwort erzeugt, **Then** hat es genau 20 Zeichen aus diesen Klassen, und jede gewählte
   Klasse kommt mindestens einmal vor.
2. **Given** ausgeschlossene Zeichen, **When** der Nutzer erzeugt, **Then** kommt keines
   davon im Ergebnis vor.
3. **Given** ein Muster, **When** der Nutzer erzeugt, **Then** entspricht das Ergebnis dem Muster.
4. **Given** eine gespeicherte Standardvoreinstellung, **When** der Nutzer den Generator
   öffnet, **Then** ist sie vorausgewählt; genau eine Voreinstellung ist Standard.
5. **Given** der Nutzer hat ein erzeugtes Passwort übernommen, **When** er den Eintrag
   speichert, **Then** ist genau dieses Passwort gespeichert.
6. **Given** die Auswahl lässt keine gültige Ausgabe zu (etwa Länge kleiner als die Zahl der
   gewählten Klassen), **When** der Nutzer erzeugt, **Then** sagt holzi, was zu ändern ist,
   und erzeugt nichts.

---

### User Story 4 - Papierkorb und Verlauf: nichts geht versehentlich verloren (Priority: P2)

Löscht der Nutzer einen Eintrag oder Ordner, landet er im Papierkorb, aus dem er ihn
wiederherstellt oder endgültig entfernt; löscht er etwas im Papierkorb, ist es endgültig weg.
Nach jeder Änderung eines Eintrags legt holzi einen Verlaufsstand mit dem neuen Zustand an; die
früheren Stände bleiben. Der Nutzer sieht den Verlauf eines Eintrags, vergleicht Stände und
stellt einen früheren Stand wieder her.

**Why this priority**: Bei Geheimnissen ist ein versehentliches Überschreiben oder Löschen
nicht reparabel, wenn man das Passwort nicht anderswo hat. Es ist aber nach der Grundfunktion
nachrangig.

**Independent Test**: Passwort eines Eintrags ändern, im Verlauf den alten Stand finden und
wiederherstellen; Eintrag löschen, im Papierkorb finden, wiederherstellen, erneut löschen und
endgültig entfernen.

**Acceptance Scenarios**:

1. **Given** ein Eintrag, **When** der Nutzer ihn löscht, **Then** erscheint er im
   Papierkorb mit seinem Ordnerpfad und taucht in Liste und Suche nicht mehr auf.
2. **Given** ein Eintrag im Papierkorb, **When** der Nutzer ihn wiederherstellt, **Then**
   erscheint er wieder in seinem früheren Ordner, oder an der Wurzel, wenn der Ordner nicht
   mehr existiert.
3. **Given** ein Eintrag im Papierkorb, **When** der Nutzer ihn endgültig löscht (einzeln
   oder „Papierkorb leeren“ nach Bestätigung), **Then** sind Eintrag, Verlauf, Passkeys,
   Verknüpfungen entfernt; nur noch von ihm genutzte Anhänge und Symbole sind danach nicht mehr
   erreichbar; beim nächsten Öffnen geben sie ihren Platz frei, sobald seit dem Verlust des
   letzten bekannten Verweises mindestens sieben Tage vergangen sind (US5, Szenario 6).
4. **Given** ein Eintrag, **When** der Nutzer eine Änderung speichert, **Then** gibt es einen
   neuen Eintrag im Verlauf mit Zeitpunkt und dem neuen Stand samt Anhängen, und der Stand
   davor bleibt als früherer Eintrag erhalten.
5. **Given** der Verlauf eines Eintrags, **When** der Nutzer einen Stand wählt, **Then** sieht
   er, was sich geändert hat, ohne dass Passwörter im Klartext aufgedeckt werden, bevor er es
   verlangt.
6. **Given** ein früherer Stand, **When** der Nutzer ihn wiederherstellt, **Then** wird er der
   aktuelle Stand, und der bisherige Stand bleibt als neuer Verlaufsstand erhalten.

---

### User Story 5 - Dateien an Einträge hängen (Priority: P2)

Ein Nutzer hängt Dateien an einen Eintrag (Wiederherstellungsschlüssel als Textdatei, einen
Scan, ein Zertifikat), sieht Bilder als Vorschau, benennt Anhänge um, lädt sie in
eine Datei herunter und entfernt sie. Dieselbe Datei an mehreren Einträgen belegt nur einmal
Platz.

**Why this priority**: Anhänge sind in haex-vault 2.x vorhanden und gehören für viele
Nutzer zu den Daten, die sie sonst nirgends sicher ablegen. Sie sind nicht nötig, damit der
Passwortmanager nützlich ist.

**Independent Test**: Dieselbe Datei an zwei Einträge hängen und prüfen, dass nur ein Satz
Binärdaten gespeichert ist; Datei herunterladen und byteweise mit dem Original vergleichen.

**Acceptance Scenarios**:

1. **Given** ein Eintrag, **When** der Nutzer eine Datei hinzufügt, **Then** zeigt der Eintrag
   Name und Größe, und der Download liefert byteweise die Originaldatei.
2. **Given** dieselbe Datei an zwei Einträgen, **When** der Nutzer sie von einem entfernt,
   **Then** bleibt sie am anderen erhalten; entfernt er sie auch dort, ist sie nicht mehr erreichbar, und
   ihre Binärdaten geben beim nächsten Öffnen den Platz frei, sobald sie mindestens sieben Tage
   ohne bekannten Verweis waren (Szenario 6).
3. **Given** eine Datei über dem Größenlimit, **When** der Nutzer sie hinzufügen will,
   **Then** lehnt holzi sie mit einer verständlichen Meldung ab und ändert den Eintrag nicht.
4. **Given** ein Bild, **When** der Nutzer den Anhang öffnet, **Then** sieht er eine Vorschau in
   holzi; andere Dateien, auch PDFs, lädt er herunter.
5. **Given** ein Anhang, **When** der Nutzer ihn umbenennt, **Then** ändert sich nur der Name
   an diesem Eintrag.
6. **Given** nicht mehr referenzierte Binärdaten, deren `orphaned_at` mindestens sieben Tage
   zurückliegt (etwa nach einer Sync-Zusammenführung oder nach dem Entfernen des letzten
   Anhangs), **When** holzi die Vault öffnet, **Then** werden sie entfernt; eine alte Binärzeile,
   die erst jetzt verwaist wird, bleibt für die Karenzzeit, damit ein noch nicht eingetroffener
   Verweis eines anderen Geräts sie noch erreichen kann.

---

### User Story 6 - Zugriff für Erweiterungen, Agenten und holzi-Funktionen mit Berechtigungen (Priority: P1)

Eine haextension, ein externer Agent über MCP oder eine Funktion von holzi (etwa der eigene
S3-Speicher aus Spec 029) braucht Zugangsdaten aus dem Passwortmanager. Der Nutzer erlaubt
das nicht pauschal, sondern für Einträge mit bestimmten Tags und für Lesen oder Lesen und
Schreiben. Wer anfragt, bekommt in Listen nur Kopfdaten ohne Geheimnisse; das Geheimnis eines
einzelnen Eintrags gibt es nur auf eine einzelne Anfrage und nur, wenn eine Freigabe es
deckt. Der eingebaute Agent im Chat bekommt nie Geheimnisse.

**Why this priority**: Spec 029 setzt diesen Zugriff voraus, und ohne ihn wäre die Aussage
„der Schutz der Geheimnisse sind die Berechtigungen“ leer. Die Verwaltung der Freigaben
selbst kommt mit den Specs 017–019 und 021.

**Independent Test**: Mit einer Test-Freigabe für das Tag „s3“ (Lesen) eine Liste abfragen,
das Geheimnis eines Eintrags mit diesem Tag lesen, das eines Eintrags ohne dieses Tag
anfordern (abgelehnt) und einen Eintrag schreiben wollen (abgelehnt).

**Acceptance Scenarios**:

1. **Given** eine Freigabe „Lesen“ für das Tag „s3“, **When** ein Aufrufer die Liste abfragt,
   **Then** sieht er nur Einträge mit diesem Tag, und keine Antwort der Liste enthält
   Passwort, TOTP-Secret, private Passkey-Schlüssel, eigene Felder oder Notiz.
2. **Given** dieselbe Freigabe, **When** er das Geheimnis eines Eintrags mit dem Tag „s3“
   liest, **Then** erhält er es; **When** er ein Geheimnis eines Eintrags ohne dieses Tag
   liest, **Then** wird er abgelehnt.
3. **Given** eine Freigabe „Lesen“, **When** der Aufrufer schreiben will, **Then** wird er
   abgelehnt und nichts ändert sich.
4. **Given** eine Freigabe „Lesen und Schreiben“ für das Tag „s3“, **When** er einen Eintrag
   mit diesem Tag anlegt oder ändert, **Then** gelingt das; **When** der Eintrag nach der
   Änderung kein Tag aus seinem Bereich mehr trägt, **Then** wird die Änderung abgelehnt.
5. **Given** mehrere Freigaben mit verschiedenen Tags, **When** der Aufrufer abfragt,
   **Then** gilt die Vereinigung der Tags; eine Freigabe für „alle“ deckt alle Einträge.
6. **Given** keine passende Freigabe, **When** ein Aufrufer etwas abfragt, **Then** wird er
   abgelehnt und erfährt nicht, ob ein bestimmter Eintrag existiert.
7. **Given** der eingebaute Agent im Chat, **When** er den Passwortmanager benutzt, **Then**
   kann er höchstens Kopfdaten ohne Geheimnisse sehen; kein Aktionsergebnis, keine
   Fehlermeldung und kein Eintrag im Chatverlauf enthält je ein Geheimnis.
8. **Given** eine holzi-Funktion, die einen Eintrag selbst angelegt hat (029), **When** der
   Nutzer ihn im Passwortmanager öffnet, **Then** sieht er ihn wie jeden anderen Eintrag und
   kann ihn ändern oder löschen.
9. **Given** ein Eintrag im Papierkorb, der noch ein Tag aus dem Bereich einer Freigabe trägt,
   **When** der Aufrufer ihn liest, ändert oder löscht, **Then** wird er abgelehnt wie bei einem
   nicht vorhandenen Eintrag, und der Eintrag bleibt unverändert im Papierkorb; er wird durch
   keinen Aufruf von außen endgültig entfernt.

---

### User Story 7 - Import aus anderen Passwortmanagern (Priority: P2)

Ein Nutzer wechselt von KeePass, Bitwarden oder LastPass zu holzi. Er wählt die Exportdatei
(bei KeePass die Datei der Datenbank mit Passwort oder Schlüsseldatei), sieht eine Vorschau,
wie viele Einträge, Ordner und Anhänge gefunden wurden, und startet den Import. holzi
übernimmt **alles**, was die Quelle enthält: Einträge samt Ordnerstruktur, Papierkorb, Verlauf,
Symbole, TOTP (auch ungültige), eigene Felder, Tags, Anhänge und Passkeys jedes Algorithmus. Was
holzi kein eigenes Feld hat, kommt als eigenes Feld oder Tag mit. Danach zeigt holzi, was
importiert wurde und wo der Nutzer von Hand nacharbeiten muss (etwa Anhänge über dem Limit).

**Why this priority**: Ohne Import startet jeder Nutzer bei null und bleibt bei seinem alten
Manager. Importieren ist aber nicht nötig, um den Passwortmanager zu benutzen.

**Independent Test**: Je eine Beispieldatei pro Format importieren und die Zahl der Einträge,
die Ordnerstruktur und je ein Passwort, TOTP-Secret und Anhang mit der Quelle vergleichen.

**Acceptance Scenarios**:

1. **Given** eine gültige KeePass-Datei und das richtige Passwort, **When** der Nutzer
   importiert, **Then** erscheinen Einträge und Ordner wie in der Quelle, der Papierkorb der
   Quelle ist unser Papierkorb mit seinem Inhalt, der Verlauf der Einträge steht als
   Verlaufsstände da, Symbole sind gesetzt, und der Nutzer sieht die Zahl der importierten
   Einträge.
2. **Given** ein falsches Passwort oder eine beschädigte Datei, **When** der Nutzer
   importiert, **Then** nennt holzi den Grund und importiert nichts.
3. **Given** eine Bitwarden- oder LastPass-Exportdatei, **When** der Nutzer importiert,
   **Then** erscheinen die Einträge mit Ordnern, eigenen Feldern und TOTP.
4. **Given** ein Eintrag der Quelle mit einem Feld, einem Typ oder einer Angabe, für die holzi
   kein eigenes Gegenstück hat (etwa Karten- und Identitätsdaten, Auto-Type, Farben,
   Sammlungen, Favoriten), **When** der Nutzer importiert, **Then** steht die Angabe als eigenes
   Feld oder Tag am Eintrag; nichts wird still verworfen.
5. **Given** ein Import wurde gestartet, **When** ein Fehler ihn insgesamt stoppt (Datei nicht
   lesbar, Speicherfehler) oder der Nutzer ihn abbricht, **Then** entfernt holzi wieder alles, was
   der Import angelegt hat, und die Vault ist wie vor dem Import.
6. **Given** ein Import hat Einträge angelegt, **When** der Nutzer ihn wiederholt, **Then**
   fragt holzi, ob doppelte Einträge übersprungen oder angelegt werden sollen.
7. **Given** ein Eintrag der Quelle mit einem Passkey (ES256, EdDSA oder RS256), **When** der
   Nutzer importiert, **Then** erscheint der Passkey am Eintrag mit Relying Party und Nutzer, und
   sein öffentlicher Schlüssel passt zum privaten; ein Passkey mit einem anderen Algorithmus wird
   ebenfalls übernommen, sein öffentlicher Schlüssel bleibt leer und der Bericht nennt ihn.
8. **Given** ein Eintrag der Quelle mit ungültigem TOTP-Secret, **When** der Nutzer importiert,
   **Then** wird das Secret wie es ist übernommen und am Eintrag als ungültig angezeigt (ersetzen
   oder entfernen möglich), und der Bericht nennt den Eintrag.
9. **Given** ein Anhang der Quelle über dem Limit aus FR-020, **When** der Nutzer importiert,
   **Then** lehnt holzi diesen Anhang ab, importiert den Eintrag ohne ihn, und der Bericht nennt
   Eintrag, Dateiname und Größe, damit der Nutzer ihn von Hand nachholen kann.
10. **Given** ein Eintrag, bei dem beim Import ein Fehler auftritt (etwa ein unlesbarer
    Passkey-Schlüssel), **When** der Import weiterläuft, **Then** wird der Rest des Eintrags
    übernommen, und der Bericht nennt den Eintrag, den Ordnerpfad und was fehlt, ohne
    Geheimnisse zu enthalten; der Nutzer kann den Bericht als Textdatei speichern.

---

### User Story 8 - Dieselben Passwörter auf allen eigenen Geräten (Priority: P2)

Ein Nutzer ändert auf einem Gerät ein Passwort und sieht es kurz darauf auf seinem anderen
Gerät, ohne etwas zu tun. Änderungen auf zwei Geräten gehen nicht verloren, wenn sie
verschiedene Felder betreffen.

**Why this priority**: Die Zusammenführung leisten die CRDT-Tabellen und der Sync aus Spec 024. Diese Story prüft, dass der Passwortmanager so aufgebaut ist, dass sie für ihn gilt.

**Independent Test**: Auf zwei verbundenen Geräten Titel und Passwort desselben Eintrags
gleichzeitig ändern; beide Änderungen sind nach dem Abgleich auf beiden Geräten da.

**Acceptance Scenarios**:

1. **Given** zwei verbundene Geräte, **When** der Nutzer auf dem einen einen Eintrag anlegt,
   **Then** erscheint er auf dem anderen, ohne dass die Ansicht neu geöffnet werden muss.
2. **Given** zwei Geräte ändern gleichzeitig verschiedene Felder desselben Eintrags,
   **When** sie sich abgleichen, **Then** haben beide beide Änderungen.
3. **Given** ein Eintrag wird auf einem Gerät gelöscht oder in den Papierkorb verschoben,
   **When** die Geräte sich abgleichen, **Then** gilt das auf beiden; ein später eintreffender
   alter Stand erweckt ihn nicht wieder.
4. **Given** zwei Geräte legen denselben Anhang an, **When** sie sich abgleichen, **Then**
   gibt es die Binärdaten nur einmal.
5. **Given** zwei Geräte legen ein Tag mit demselben Namen an, **When** sie sich abgleichen,
   **Then** gibt es dieses Tag danach nur einmal, und alle Einträge behalten ihr Tag.

---

### Edge Cases

- **Nutzer verlässt den Editor mit ungespeicherten Änderungen.** holzi fragt, ob er speichern,
  verwerfen oder bleiben will.
- **Ein Eintrag wird gleichzeitig auf einem anderen Gerät gelöscht**, während der Nutzer ihn
  bearbeitet. Beim Speichern sagt holzi, dass der Eintrag gelöscht wurde, und bietet an, ihn
  neu anzulegen; nichts wird still verworfen.
- **Ein TOTP-Secret ist ungültig** (falsches Format, unbekannter Algorithmus, Ziffernzahl oder
  Periode außerhalb des Zulässigen). Beim Anlegen und Ändern lehnt holzi das mit einer Meldung
  am Feld ab; fehlende Angaben bekommen die Standardwerte. Kommt ein ungültiger Wert dennoch
  an (Sync von einem anderen Gerät, ein anderes Programm), zeigt der Eintrag statt eines Codes
  eine Meldung und lässt zu, das Secret zu ersetzen oder zu entfernen; holzi stürzt nicht ab
  und verändert nichts still. Beim Import wird ein ungültiges Secret wie es ist übernommen
  und im Bericht genannt.
- **Die Uhr eines Geräts geht falsch.** Der TOTP-Code weicht dann ab; holzi weist an der
  Anzeige darauf hin, dass der Code von der Systemzeit abhängt.
- **Ein Anhang ist leer, sehr groß oder hat einen ungewöhnlichen Namen** (Sonderzeichen,
  Pfadtrenner, sehr lang). holzi behandelt den Namen als Text, nie als Pfad.
- **Ein Ordner wird in seinen eigenen Unterordner verschoben.** holzi verbietet es.
- **Ein in Benutzung befindlicher Eintrag** (etwa die Zugangsdaten des eigenen S3-Speichers
  nach Spec 029) wird gelöscht. holzi warnt vorher, dass eine Funktion ihn nutzt, und
  verhindert das Löschen nicht.
- **Der Nutzer legt zwei Tags mit gleichem Namen an** oder benennt ein Tag um in einen
  bestehenden Namen. holzi verhindert die Doppelung.
- **Ein importierter Passkey hat dieselbe Credential-ID wie ein vorhandener.** holzi legt ihn
  nicht doppelt an, überspringt ihn nicht still, sondern nennt ihn im Bericht.
- **Die Vault ist groß** (mehrere tausend Einträge). Liste und Suche bleiben benutzbar.
- **Zwei Fenster des Passwortmanagers sind offen** (Spec 030). Eine Änderung im einen
  erscheint ohne Zutun im anderen; ein Eintrag, der in beiden bearbeitet wird, überschreibt
  nicht still den anderen.
- **Das Fenster ist schmal** (360 px). Alle Funktionen sind ohne waagerechtes Scrollen
  erreichbar; die Seitenleiste ist ausblendbar.
- **Ein Aufrufer fragt in sehr kurzer Folge viele Geheimnisse ab.** Jede Abfrage wird einzeln
  geprüft; eine Liste liefert nie Geheimnisse mit.

## Requirements _(mandatory)_

### Functional Requirements

**Einträge, Felder und Darstellung**

- **FR-001**: Das System MUSS Einträge mit Titel, Benutzername, Passwort, Notiz, Adresse,
  Symbol, Farbe, Ablaufdatum, Erstellungs- und Änderungszeitpunkt und den Autofill-Aliasen
  (Zuordnung von Feldnamen zu alternativen Bezeichnungen für den späteren Browser-Abgleich)
  speichern; alle Felder außer der Kennung DÜRFEN leer sein, auch der Titel. Die Listen und die
  Ansicht MÜSSEN einen Eintrag ohne Titel erkennbar zeigen (Platzhalter „(ohne Titel)“, als
  Text der Oberfläche, nicht als gespeicherter Wert) und ihn über Benutzername oder Adresse
  auffindbar machen.
- **FR-002**: Das System MUSS je Eintrag beliebig viele eigene Felder aus Name und Wert
  erlauben, die zum Eintrag gehören und mit ihm gelöscht werden.
- **FR-003**: Das System MUSS je Eintrag TOTP (Secret, Ziffernzahl, Periode, Algorithmus,
  Standard 6 Ziffern, 30 Sekunden, SHA-1) speichern, daraus den aktuellen Code berechnen,
  die Restzeit zeigen und den Code ohne Zutun des Nutzers erneuern; `otpauth://`-Adressen
  und reine Secrets MÜSSEN als Eingabe akzeptiert werden. Ungültige Secrets, Ziffernzahlen,
  Perioden und Algorithmen MÜSSEN beim Anlegen und Ändern abgelehnt werden; fehlende Angaben
  MÜSSEN die Standardwerte erhalten. Ein ungültiger Wert, der per Sync oder Import eintrifft,
  MUSS erkannt, am Eintrag angezeigt und vom Nutzer behebbar sein.
- **FR-004**: Das System MUSS Passkeys eines Eintrags (Credential-ID, Relying Party, Nutzer,
  Schlüsselpaar, Algorithmus, Zähler, Erkennbarkeit, Symbol, Farbe, Spitzname, letzte
  Nutzung) als Daten speichern, anzeigen, umbenennen (Spitzname) und löschen können. Das Anlegen
  durch die Oberfläche und das Benutzen von Passkeys (Signieren, Autofill) ist nicht Teil dieser
  Spec. Passkeys kommen durch Import (FR-023) oder Sync hinein und können jeden Algorithmus haben.
  Eine Credential-ID MUSS in der Vault eindeutig sein.
- **FR-005**: Passwörter, TOTP-Secrets, eigene Felder und Passkey-Schlüssel MÜSSEN
  standardmäßig verdeckt sein; Aufdecken MUSS eine bewusste Handlung des Nutzers sein (Halten
  mit der Maus, Tippen auf Mobilgeräten) und MUSS beim Verlassen des Eintrags enden.
  _Geändert 2026-10-05:_ Die Werte eigener Felder zeigt die Ansicht unverdeckt; im Editor
  stehen Passwort und eigene Felder als gespeicherte Werte (Platzhalter unaufgelöst) in
  normalen Eingabefeldern, ohne „Ersetzen“. Aufgelöste Platzhalter bleiben verdeckt. Der
  Verlauf zeigt eigene Felder weiter verdeckt (Review 2026-10-06: bewusst, ein alter Stand
  wird nur auf Wunsch aufgedeckt).
- **FR-006**: Das System MUSS Benutzername, Passwort und TOTP-Code in die Zwischenablage
  kopieren können und sie nach einer einstellbaren Zeit leeren (Standard 30 Sekunden), sofern
  sie noch den kopierten Wert enthält; die Einstellung speichert bei Auswahl, ohne Knopf zum
  Übernehmen.
  _Erweitert 2026-10-05:_ Jedes Feld hat einen Kopierknopf, in Ansicht, Editor und Verlauf;
  jede Kopie läuft durch Rust und wird gleich geleert, auch die des Generators. Platzhalter
  löst die Kopie auf (036 FR-045), auch aus Editor und Verlauf. Kopiert wird der TOTP-Code;
  das TOTP-Secret nur aus Editor und Verlauf, unter seinem eigenen Namen „TOTP-Secret“.
- **FR-007**: Das System MUSS Einträge nach Titel, Benutzername, Adresse und Tag-Namen
  durchsuchen und in einer Liste und einem Baum mit Ordnern darstellen; Geheimnisse und
  Notizen DÜRFEN nicht Gegenstand der Suche sein.
  _Erweitert 2026-10-06:_ Die Suche ist unscharf (Fuse.js): Jedes Wort muss in einem der vier
  Felder vorkommen, ab vier Buchstaben mit einem Tippfehler; Treffer erscheinen nach Güte.
  _Erweitert 2026-10-06:_ Ein Wort trifft auch als Abkürzung: seine Buchstaben der Reihe nach
  in einem Wort eines Feldes, beginnend mit dessen erstem Buchstaben („itms“ findet „itemis“).
  Eine Suche durchsucht alle Ordner, nicht nur den geöffneten.
- **FR-008**: Das System MUSS Einträge als abgelaufen kennzeichnen, wenn ihr Ablaufdatum
  überschritten ist.

**Ordnung**

- **FR-009**: Das System MUSS verschachtelte Ordner mit Name, Beschreibung, Symbol, Farbe und
  Reihenfolge erlauben; ein Eintrag liegt in höchstens einem Ordner, ohne Ordner liegt er an
  der Wurzel. Der Nutzer MUSS die Reihenfolge der Ordner einer Ebene ändern können, auch ohne
  Maus (Aktionen „nach oben“ und „nach unten“); ohne eigene Reihenfolge gilt die alphabetische.
  _Geändert 2026-10-06 (PR #287):_ Einen Ordner legt der Knopf „Neu“ der Werkzeugleiste an
  (Auswahl „Schlüssel“ oder „Ordner“), im gerade offenen Ordner, sonst an der Wurzel; die
  Seitenleiste hat keinen eigenen Knopf dafür mehr. Unterordner gehen weiter über das Menü
  eines Ordners.
- **FR-010**: Das System MUSS verhindern, dass ein Ordner in sich selbst oder einen seiner
  Unterordner verschoben wird.
- **FR-011**: Das System MUSS Tags (Name eindeutig, Farbe) verwalten und Einträgen beliebig
  viele zuordnen; Umbenennen und Löschen eines Tags MUSS für alle Einträge gelten. Ein Tagname
  ist eindeutig ohne Rücksicht auf Groß-/Kleinschreibung und auf die Schreibweise von Umlauten
  (zusammengesetzt oder zerlegt): „Work“ und „work“ sind derselbe Tag, die Schreibweise des
  ersten Anlegers bleibt sichtbar.
  _Geändert 2026-10-06 (PR #287):_ Nach einem Tag filtert ein Auswahlmenü neben der Suche
  (Tags mit Farbe und Anzahl, „Alle Tags“ hebt den Filter auf); die Seitenleiste listet die
  Tags nicht mehr, „Tags verwalten“ bleibt dort.
- **FR-012**: Das System MUSS Mehrfachauswahl für Verschieben, Taggen und Löschen
  unterstützen und vor einer Massenaktion die Zahl der betroffenen Einträge nennen.

**Generator**

- **FR-013**: Das System MUSS Passwörter nach Länge, Zeichenklassen (Groß, Klein, Ziffern,
  Sonderzeichen), ausgeschlossenen Zeichen oder einem Muster erzeugen, mit einer
  kryptografisch sicheren Zufallsquelle; jede gewählte Klasse MUSS im Ergebnis vorkommen.
- **FR-014**: Das System MUSS Voreinstellungen (Name, die genannten Optionen) speichern,
  ändern und löschen und genau eine als Standard führen; es MUSS bei einer nicht erfüllbaren
  Auswahl erklären, was zu ändern ist, statt etwas zu erzeugen.

**Papierkorb und Verlauf**

- **FR-015**: Löschen eines Eintrags oder Ordners MUSS ihn in den Papierkorb verschieben, den
  die Vault als besonderen Ordner mit fester Kennung führt; Löschen eines Elements im
  Papierkorb MUSS es endgültig entfernen, samt allem, was nur von ihm abhängt. „Papierkorb
  leeren“ MUSS eine Bestätigung verlangen. Das gilt für **jeden** Aufrufer: Löschen durch eine
  holzi-Funktion, Erweiterung oder einen Agenten verschiebt ebenfalls nur in den Papierkorb;
  endgültig entfernt ein Eintrag nur dadurch, dass der Nutzer ihn **im Papierkorb** löscht.
  Einträge im Papierkorb sind für andere Aufrufer als den Nutzer nicht vorhanden (sie lesen,
  ändern und löschen sie nicht); so führt ein Aufruf von außen nie zum endgültigen Löschen.
- **FR-016**: Wiederherstellen aus dem Papierkorb MUSS den früheren Ort wiederherstellen,
  oder die Wurzel, wenn der Ordner nicht mehr existiert.
- **FR-017**: Das System MUSS nach jeder Änderung eines Eintrags den neuen Zustand samt der
  Verknüpfung zu seinen Anhängen als Verlaufsstand sichern (Zeitpunkt, Stand als
  eigenständig lesbares Dokument; kein neuer Eintrag, wenn sich nichts geändert hat), den
  Verlauf anzeigen, Stände vergleichen und einen Stand wiederherstellen, ohne frühere Stände
  zu verlieren.
- **FR-018**: Der Verlauf MUSS Anhänge behalten, die am Eintrag selbst inzwischen ersetzt
  wurden; sie MÜSSEN erst entfernt werden, wenn kein Eintrag und kein Verlaufsstand sie mehr
  braucht.

**Anhänge**

- **FR-019**: Das System MUSS Dateien an Einträge hängen und Name und Größe anzeigen; Binärdaten
  MÜSSEN als Binärdaten (nicht als Text) in der Vault-Datenbank liegen, durch den
  SHA-256-Wert ihres Inhalts eindeutig, und MÜSSEN je Eintrag unter einem eigenen Namen
  verknüpft sein.
- **FR-020**: Das System MUSS das Größenlimit pro Anhang durchsetzen, bevor etwas gespeichert
  wird, und die Ablehnung begründen. Das Limit beträgt
  25 MiB je Anhang; es schützt Größe und Speicherbedarf der Vault-Datenbank, in der die
  Anhänge liegen und die auf alle Geräte synchronisiert wird.
- **FR-021**: Das System MUSS Anhänge herunterladen (byteweise gleich dem Original), umbenennen
  und entfernen sowie Bilder in einer Vorschau zeigen; andere Dateien, auch PDFs, MÜSSEN sich
  herunterladen lassen. Dateinamen MÜSSEN als Text behandelt werden, nie als Pfad.
- **FR-022**: Das System MUSS Binärdaten entfernen, die kein Eintrag, kein Ordner, kein Passkey und
  kein Verlaufsstand mehr braucht (als Anhang oder als Symbol) und deren `orphaned_at` beim
  Öffnen der Vault mindestens sieben Tage zurückliegt; der Zeitpunkt wird gesetzt, sobald der
  letzte bekannte Verweis entfällt, und eine neue Referenz setzt ihn zurück. Benutzte Symbole
  bleiben.

**Import**

- **FR-023**: Das System MUSS Einträge aus KeePass-Datenbanken (kdbx, mit Passwort und
  optional Schlüsseldatei), Bitwarden-Exporten und LastPass-Exporten importieren und dabei
  **alles** übernehmen, was die Quelle enthält:
  - Einträge mit allen Feldern, Ordnerstruktur, Tags, eigenen Feldern, TOTP, Anhängen, Zeitstempeln
    und Symbolen (Standardsymbole der Quelle als Symbol von holzi, eigene Symbole als Bild);
  - den **Papierkorb** der Quelle (KeePass-Papierkorb, gelöschte Einträge von Bitwarden) als
    Inhalt unseres Papierkorbs, und den **Verlauf** der Einträge (KeePass-Verlauf, Bitwarden
    `passwordHistory`) als Verlaufsstände samt ihrer Anhänge;
  - **alle Passkeys**, jeden Algorithmus. Weil die Quellen nur den privaten Schlüssel liefern,
    MUSS der Import den öffentlichen Schlüssel ableiten, wo der Algorithmus es zulässt (ES256,
    EdDSA, RS256); sonst bleibt er leer und der Bericht nennt den Passkey;
  - **ungültige TOTP-Secrets** wie sie sind (am Eintrag als ungültig angezeigt, FR-003);
  - alle Eintragstypen und Felder von Bitwarden (Anmeldung, Notiz, Karte, Identität, SSH-Schlüssel,
    unbekannte Typen) und alles, wofür holzi kein eigenes Feld hat (Auto-Type, Farben, Sammlungen,
    Favoriten, Wiederholungsabfrage, URL-Zuordnungen und Ähnliches), als **eigene Felder** oder
    Tags am Eintrag. Nichts darf still verloren gehen.

  Der Import MUSS vorher eine Vorschau (Zahl der Einträge, Ordner, Anhänge, Verlaufsstände,
  Doppelte) zeigen und bei erkannten Doppelten fragen. Anhänge über dem Limit aus FR-020 MÜSSEN
  abgelehnt werden. Jeder Fehler und jede Auffälligkeit, die der Import nicht selbst auflöst
  (abgelehnter Anhang mit Dateiname und Größe, unlesbarer Schlüssel, ein Passkey, dessen
  Credential-ID schon vorhanden ist, ein nicht speicherbarer Wert), MUSS in einem Bericht stehen, der Eintrag, Ordnerpfad und das Fehlende nennt, damit der
  Nutzer weiß, wo er nacharbeiten muss; der Bericht enthält keine Geheimnisse und lässt sich als
  Textdatei speichern. Der Import schreibt in mehreren Schritten (jeder Anhang für sich); ein
  Fehler, der den Import insgesamt stoppt, und ein Abbruch durch den Nutzer MÜSSEN alles wieder
  entfernen, was der Import angelegt hat.

**Zugriff von außen und Berechtigungen**

- **FR-024**: Jeder Zugriff auf Passwortmanager-Daten (Oberfläche, Erweiterungen, externe
  Agenten, der eingebaute Agent, holzi-Funktionen) MUSS über denselben Dienst und dessen
  Zugriffsprüfung gehen; kein Weg DARF die Tabellen am Passwortmanager vorbei lesen oder
  schreiben. Die Oberfläche läuft dort als Aufrufer „Nutzer“ ohne Freigabe (FR-031); alle
  Funktionen jenseits von Eintrag lesen, anlegen, ändern und löschen stehen anderen Aufrufern
  nicht offen.
- **FR-025**: Eine Freigabe MUSS eine Art (Lesen oder Lesen und Schreiben) und einen Bereich
  (alle Einträge oder Einträge mit einem bestimmten Tag) haben; mehrere Freigaben eines
  Aufrufers gelten als Vereinigung ihrer Bereiche, „alle“ deckt alles, „Lesen und Schreiben“
  deckt auch Lesen.
- **FR-026**: Listen MÜSSEN nur Kopfdaten liefern (Kennung, Titel, Benutzername, Adresse,
  Symbol, Farbe, Tags, Ablaufdatum, Hinweis, ob TOTP oder Passkeys vorhanden sind) und nie
  Passwort, TOTP-Secret, Passkey-Schlüssel, eigene Felder, Notiz oder Anhänge. Geheimnisse
  gibt es nur auf eine Einzelabfrage je Eintrag.
- **FR-027**: Der eingebaute Agent im Chat (Spec 032) MUSS vom Passwortmanager höchstens eine
  engere Auswahl der Kopfdaten erhalten (Kennung, Titel, Tags, Ordnername, Hinweis auf TOTP;
  ohne Benutzername und Adresse, damit nicht die Liste der Konten an einen Cloud-Anbieter geht)
  und DARF Geheimnisse nie lesen, auch nicht mit Zustimmung im
  Chat und auch nicht indirekt über Fehlermeldungen, Protokolle oder Chatverlauf; die
  Aktionen für Geheimnisse MÜSSEN für ihn nicht aufrufbar sein.
- **FR-028**: Schreibzugriffe MÜSSEN verlangen, dass der Eintrag nach der Änderung mindestens
  ein Tag aus dem Bereich des Aufrufers trägt; andernfalls MUSS die Änderung abgelehnt werden,
  damit ein Aufrufer Einträge nicht aus seinem Bereich herausschreibt. Das Löschen eines
  Eintrags (Verschieben in den Papierkorb, FR-015) MUSS einen Eintrag mit einem Tag aus seinem
  Bereich voraussetzen; das endgültige Löschen im Papierkorb steht anderen Aufrufern nicht
  offen. Ein Aufrufer DARF alle Tags eines Eintrags sehen, den er sehen darf, aber ein Tag
  außerhalb seines Bereichs MUSS bei jeder Änderung unverändert bleiben: er darf es weder
  entfernen noch hinzufügen noch umbenennen oder löschen (Tags umbenennen und löschen kann nur
  der Nutzer). Hinzufügen darf er nur Tags seines Bereichs, damit er einen Eintrag nicht in
  den Bereich eines anderen Aufrufers hebt.
- **FR-029**: Fehlt eine passende Freigabe, MUSS die Anfrage abgelehnt werden, ohne preiszugeben,
  ob ein bestimmter Eintrag außerhalb des Bereichs existiert.
- **FR-030**: Wo Freigaben gespeichert, erteilt, angezeigt und widerrufen werden, regeln die
  Specs 017–019 und 021. Diese Spec MUSS die Prüfung unabhängig davon über eine Schnittstelle
  bieten, die ein Aufrufer-Kennzeichen und die Freigaben des Aufrufers entgegennimmt, damit
  sie ohne diese Specs automatisch geprüft werden kann.
- **FR-031**: Der Zugriff der Oberfläche des Nutzers selbst auf eigene Einträge MUSS ohne
  Freigabe möglich sein.
- **FR-032**: Die Zugriffsprüfung MUSS so gebaut sein, dass die External Bridge später als
  weiterer Aufrufer ohne Änderung von Datenmodell oder Freigabe-Semantik andocken kann; sie
  selbst ist nicht Teil dieser Spec.

**Nutzung durch holzi-Funktionen**

- **FR-033**: holzi-Funktionen (zunächst Spec 029) MÜSSEN Einträge im Passwortmanager als
  gewöhnliche Einträge mit einem frei gewählten Tag anlegen, lesen und ändern können; es gibt
  keine für holzi reservierten Tags oder Tagnamen. Der Nutzer MUSS sie in der Oberfläche sehen,
  ändern und löschen können.
- **FR-034**: Das System MUSS vor dem Löschen eines Eintrags, den eine holzi-Funktion nutzt,
  warnen und die Funktion nennen; das Löschen MUSS erlaubt bleiben. Welche Einträge eine
  Funktion nutzt, meldet die Funktion selbst (sie kennt ihre Verweise auf Einträge); der
  Passwortmanager erkennt das nicht an Tags oder Namen.
- **FR-035**: Geheimnisse MÜSSEN mit der Vault und deren Sync wandern wie alle Einträge; sie
  DÜRFEN nur für Geräte der Vault sichtbar sein (Spec 024).

**Daten, Sync und Fenster**

- **FR-036**: Tabellen- und Spaltennamen des Datenmodells MÜSSEN die von haex-vault 2.x
  übernehmen (`haex_passwords_item_details`, `_item_key_values`, `_groups`, `_group_items`,
  `_binaries`, `_item_binaries`, `_item_snapshots`, `_snapshot_binaries`,
  `_generator_presets`, `_tags`, `_item_tags`, `_passkeys`), mit gleicher Bedeutung. Die
  Abweichungen sind abschließend: die Spalte der Binärdaten ist Binär statt Base64-Text
  (Clarifications); zwei zusätzliche nullbare Spalten merken den früheren Ort für das
  Wiederherstellen (FR-016); die Verweise auf Binärdaten löschen nicht mit (`RESTRICT`);
  `orphaned_at` merkt den Beginn der Karenzzeit nach dem Verlust des letzten bekannten
  Verweises; Eindeutigkeit entsteht nicht durch UNIQUE-Constraints (FR-037).
- **FR-037**: Alle Tabellen MÜSSEN CRDT-synchronisiert sein und je Zeile zusammengeführt
  werden; Eindeutigkeitsregeln (Tagname, Credential-ID, Hash der Binärdaten) MÜSSEN
  bestehen bleiben, ohne dass ein Sync-Abgleich Daten verliert oder anhält. Sie gelten durch
  abgeleitete Kennungen und Prüfung in der Anwendung, nicht durch UNIQUE-Constraints, weil
  ein Konflikt dort den Abgleich anhält; Löschen MUSS Löschmarker
  hinterlassen, und Löschen auf einem Gerät MUSS auf allen gelten.
- **FR-038**: Der Passwortmanager MUSS sich bei Änderungen der Tabellen durch ein anderes
  Fenster, einen Agenten oder einen Sync live aktualisieren, ohne eine laufende Bearbeitung
  zu überschreiben.
- **FR-039**: Der Passwortmanager MUSS eine App im Window Manager sein (Spec 015) mit Orten
  im Tab nach Spec 020, deren Wiederherstellung (Spec 022) nie Geheimnisse im Verlauf oder in
  der gespeicherten Sitzung ablegt, und MUSS dem Stil der anderen holzi-Oberflächen
  folgen (Werkzeugleiste, große Überschrift, Listen in Boxen, ausblendbare Seitenleiste).
- **FR-040**: Geheimnisse DÜRFEN weder in Protokolle, Fehlermeldungen, Fenstertitel, die
  Verlaufsliste der Tabs, die gespeicherte Sitzung noch in die Aktionsprotokolle des Agenten
  gelangen.
- **FR-041**: Die Oberfläche MUSS bei jeder Breite ab 360 px ohne waagerechtes Scrollen
  bedienbar sein und alle Texte auf Deutsch zeigen, mit Texten auch auf Englisch (wie Spec
  023, FR-020).

### Key Entities

- **Eintrag**: ein Geheimnis oder Konto mit Titel, Benutzername, Passwort, Notiz, Adresse,
  Symbol, Farbe, Ablaufdatum, TOTP-Angaben und Autofill-Aliasen; liegt in höchstens einem
  Ordner, trägt beliebig viele Tags.
- **Eigenes Feld**: Name und Wert, gehört zu genau einem Eintrag.
- **Ordner**: hierarchischer Behälter mit Name, Beschreibung, Symbol, Farbe und Reihenfolge; der
  Papierkorb ist ein Ordner mit fester Kennung.
- **Tag**: eindeutig benannte Bezeichnung mit Farbe; zugleich Maßstab für den Bereich einer
  Freigabe.
- **Passkey**: gespeichertes Schlüsselpaar mit Credential-ID und Relying Party, gehört zu
  einem Eintrag; jeder Algorithmus; nur als Daten.
- **Importbericht**: Ergebnis eines Imports mit Zahlen und der Liste der Stellen, an denen der
  Nutzer nacharbeiten muss (Eintrag, Ordnerpfad, Fehlendes, ohne Geheimnisse).
- **Binärdaten**: Inhalt einer Datei, durch den SHA-256-Wert eindeutig, mit Größe und Art
  (Anhang oder Symbol).
- **Anhang**: Verknüpfung eines Eintrags (oder eines Verlaufsstands) mit Binärdaten unter einem
  Dateinamen.
- **Verlaufsstand**: Sicherung eines Eintrags mit Zeitpunkt und den damals verknüpften
  Anhängen.
- **Voreinstellung des Generators**: Name, Länge, Zeichenklassen, ausgeschlossene Zeichen,
  Muster, Standardmarkierung.
- **Freigabe**: Aufrufer, Art (Lesen, Lesen und Schreiben) und Bereich (alle oder ein Tag);
  Verwaltung in den Specs 017–019 und 021.
- **Kopfdaten**: die Sicht eines Eintrags ohne Geheimnisse, die Listen liefern.
- **Aufrufer**: Oberfläche des Nutzers, eingebauter Agent, Erweiterung, externer Agent oder
  holzi-Funktion.

## Success Criteria _(mandatory)_

### Measurable Outcomes

- **SC-001**: Ein Nutzer legt einen Eintrag mit Titel, Benutzername und Passwort in höchstens
  30 Sekunden an und findet ihn danach über die Suche in höchstens 10 Sekunden.
- **SC-002**: In einer Vault mit 5.000 Einträgen erscheint das Ergebnis der Suche ohne
  spürbare Verzögerung (grob unter einer Sekunde nach der Eingabe).
- **SC-003**: 100 % der TOTP-Codes stimmen mit den Referenzwerten aus der Spezifikation des
  Verfahrens für die geprüften Eingaben (alle Algorithmen, 6 und 8 Ziffern) überein.
- **SC-004**: Von 1.000 erzeugten Passwörtern erfüllen 100 % die gewählten Regeln.
- **SC-005**: In den automatischen Prüfungen enthält keine Antwort einer Liste und keine
  Antwort an den eingebauten Agenten ein Geheimnis (0 Fundstellen bei allen Abfragearten).
- **SC-006**: Jede Anfrage ohne passende Freigabe wird abgelehnt (100 % der Prüffälle,
  einschließlich Schreiben außerhalb des Bereichs und Herausschreiben aus dem Bereich).
- **SC-007**: Ein Eintrag, den ein Gerät ändert, erscheint auf einem verbundenen Gerät in
  wenigen Sekunden (grob unter 10) ohne manuelles Neuladen.
- **SC-008**: Bei gleichzeitiger Änderung verschiedener Felder auf zwei Geräten gehen 0 Änderungen
  verloren.
- **SC-009**: Eine heruntergeladene Datei ist in 100 % der Fälle byteweise gleich dem Original.
- **SC-010**: Beim Import der Beispieldateien aller drei Formate stimmen Zahl der Einträge und
  Ordner zu 100 % mit der Quelle überein (der Papierkorb der Quelle inbegriffen), jede Stelle,
  die nicht verlustfrei ankam, steht im Bericht (0 stille Verluste), und ein abgebrochener Import
  hinterlässt 0 angelegte Einträge.
- **SC-011**: Bei 360 px Breite sind alle Funktionen ohne waagerechtes Scrollen bedienbar.
- **SC-012**: Die in der Quickstart-Anleitung (§12) benannten Abläufe laufen als
  End-to-End-Test gegen die gebaute App und bestehen: Eintrag anlegen, suchen, TOTP-Code,
  Papierkorb; Synchronisation auf zwei eigenen Geräten; schmales Fenster; Sitzungswiederherstellung
  ohne Werte. Die übrigen Abläufe (Ordnen, Verlauf, Generator, Anhänge, Import) deckt die
  Quickstart-Anleitung durch Integrationstests und manuelle Schritte ab; die Dateidialoge von
  Anhängen und Import sind nicht Teil der End-to-End-Tests.

## Assumptions

- holzi hat nur einen Nutzer pro Vault; es gibt keine Freigabe von Einträgen an andere
  Nutzer. Das Teilen mit anderen Personen folgt später über Spaces (Spec 027), nicht über
  einzelne Einträge.
- Die Vault-Verschlüsselung im Ruhezustand genügt; es gibt keine zweite Hülle oder
  Hauptpasswort des Passwortmanagers. Eine zusätzliche Sperre des Passwortmanagers nach
  Leerlauf ist nicht Teil dieser Spec.
- Die Tabellennamen mit dem Präfix `haex_` weichen vom Muster der übrigen holzi-Tabellen ab
  (zum Beispiel `chat_threads`). Das ist bewusst, damit Datenmodell und Werkzeuge mit
  haex-vault 2.x vergleichbar bleiben; der Plan prüft, dass das Präfix in haex-crdt keine
  Sonderbehandlung auslöst.
- Passkeys entstehen in dieser Spec nur durch Import, Sync oder später durch die External
  Bridge; die Oberfläche kann sie anzeigen, umbenennen und löschen, aber nicht neu anlegen.
- Der Aufrufer „Oberfläche“ ist der Nutzer selbst; er braucht keine Freigabe (FR-031).
- Standard der Zwischenablage-Löschung sind 30 Sekunden, die Auswahl reicht von „aus“ bis
  2 Minuten; der Plan legt die Auswahl fest.
- Der Papierkorb wird nicht automatisch geleert; der Nutzer leert ihn selbst.
- Der Verlauf eines Eintrags wird nicht automatisch gekürzt; eine Obergrenze für die Zahl der
  Stände je Eintrag legt der Plan fest, falls Messungen sie nötig machen.
- Für Aufrufer von außen bestimmt Spec 021 (und 017–019), wie eine Freigabe vergeben und
  angezeigt wird. Bis dahin gibt es keinen Weg für Erweiterungen und Agenten, Freigaben zu
  erhalten; die internen holzi-Funktionen (Spec 029) tragen ihren Bereich fest.
- Die Datei- und Anhang-Aufnahme geht über den Dateidialog des Systems; Ziehen und Ablegen
  ist eine Verbesserung, keine Bedingung.
- Beim Import werden auch der Papierkorb der Quelle und der Verlauf der Einträge übernommen. Ein
  verschlüsselter Bitwarden-Export lässt sich ohne dessen Kennwort nicht lesen und wird mit einer
  Meldung abgelehnt; sonst gilt: Was nicht ankommt, steht im Bericht.
- Alle neuen Texte entstehen auf Deutsch und Englisch; weitere Sprachen sind nicht Teil dieser
  Spec.

## Nicht im Umfang

- Die External Bridge: Browser-Erweiterung, Autofill, das Signieren und Anlegen von Passkeys.
  Später, mit eigener Spec; FR-032 hält den Weg frei.
- Eine eigene Verschlüsselung der Geheimnisse über die der Vault hinaus.
- Teilen einzelner Einträge mit anderen Personen oder Spaces.
- Notfallzugriff, Passwort-Gesundheitsprüfung (Wiederverwendung, Datenlecks, Stärkeanzeige als
  Auswertung) und automatisches Ändern von Passwörtern.
- Ein Export der Einträge (nach KeePass, Bitwarden oder CSV); er kommt mit einer späteren
  Spec.
- Die Verwaltung der Freigaben für Erweiterungen und Agenten (Anlegen, Anzeigen, Widerrufen,
  Anfragedialog): Specs 017–019 und 021.
- Dateisync der Anhänge als Dateien im Dateisystem (Spec 025 betrifft Ordner).
- Eine Obergrenze oder Bereinigung des Verlaufs sowie ein zeitgesteuerter Papierkorb.
