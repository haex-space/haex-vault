# Feature Specification: Umbenennung von holzi in haex-vault

**Feature Branch**: `054-haex-vault-rename`

**Created**: 2026-10-16

**Status**: Draft

**Input**: User description: „ich möchte holzi auf haex-vault umbenennen und auch in das
entsprechende repo https://github.com/haex-space/haex-vault umziehen“ (Betreiber, 2026-10-16),
mit den Entscheidungen aus der Planung vom selben Tag (siehe Clarifications). Das Eichhörnchen-Logo
aus haex-vault ist als Vorarbeit mit PR #474 eingebaut.

## Clarifications

### Session 2026-10-16

- Q: Wie zieht das Repo um? → A: Das bisherige `haex-space/haex-vault` wird zu
  `haex-space/haex-vault-legacy` umbenannt und archiviert. `haexmas/holzi` wird nach `haex-space`
  transferiert und zu `haex-vault` umbenannt; Historie, PRs und Issues bleiben erhalten, GitHub
  leitet die alten Adressen weiter. Der Default-Branch ist `main` (Betreiber, 2026-10-16).
- Q: Was wird umbenannt? → A: Alles, was „holzi“ heißt, ohne Alias und ohne Ausnahme für
  „historische“ Namen: Crate und Bibliothek, Typen, das Android-Plugin samt Kotlin-Paket, die
  Umgebungsvariablen, Produktname, Fenstertitel, Oberflächentexte, die festen Texte in der
  Kryptografie (Schlüsselableitung, Signatur-Domänen, Netzwerk-Protokollnamen), die Kennungen
  (UUID-Namensräume, Visitenkarten-Präfix, Datenbank-Tabellen, Manifest-Schlüssel, MCP-Name) und das
  Schema der Erweiterungen (`holzi-ext://` wird `haex-ext://`) sowie Specs und Doku. Der Identifier
  der App wird `space.haex.vault`, auf allen Plattformen einschließlich Android (Betreiber,
  2026-10-16).
- Q: Gibt es eine Migration der Daten? → A: Nein, weder im Code noch beim Start. Es werden keine
  alten Installationen gesucht oder übernommen, denn holzi hat keine Nutzer außer dem Betreiber. Die
  einzige Übernahme ist von Hand: Die alte Vault-Datei `test.db` wird über den vorhandenen
  Datei-Import geöffnet, um die Passwörter zu behalten. Modelle, Erweiterungen und sonstige Daten
  entfallen (Betreiber, 2026-10-16).
- Q: Welche Versionsnummer trägt die umbenannte App? → A: 3.0.0 (Betreiber, 2026-10-16). Die alte
  App stand auf 2.17.0.
- Q: Wie wird der Name „haex-vault“ eindeutig, wenn er bisher die alte App meint? → A: Jede
  Erwähnung der alten App wird zuerst als „haex-vault 2.x (Legacy)“ markiert, im Code wird aus dem
  Importmodul `haex_vault` das Modul `legacy_haex_vault`; erst danach ersetzt „haex-vault“ den Namen
  „holzi“ (Betreiber, 2026-10-16).
- Q: Was geschieht mit Daten der alten App haex-vault 2.x, die unter `space.haex.vault` auf einem
  Gerät liegen, auf dem haex-vault 3 startet? → A: Sie sind nicht wichtig. Der Betreiber löscht den
  alten Datenordner von Hand vor dem ersten Start; es gibt keine Code-Logik dafür (Betreiber,
  2026-10-16).

## Begriffe

- **holzi**: der bisherige Name dieser App und dieses Repos, bis zum Tag `holzi-final`.
- **haex-vault 2.x (Legacy)**: die alte App, bisher im Repo `haex-space/haex-vault`, Version 2.17.0,
  Identifier `space.haex.vault`. Sie wird nicht weiterentwickelt.
- **haex-vault 3**: diese App nach der Umbenennung, Version 3.0.0.
- **Alte Vault-Datei**: die Vault-Datenbank `test.db` des Betreibers aus der Zeit unter dem Namen
  holzi. Eine Sicherung liegt außerhalb des Repos vor.
- **Ausnahmeliste**: die Orte, an denen „holzi“ danach noch vorkommen darf (FR-004).

## Beziehung zu bestehenden Specs

- [`034-password-manager`](../034-password-manager/spec.md) und
  [`037-haex-vault-import`](../037-haex-vault-import/spec.md): Beide nennen „haex-vault“ und meinen
  die alte App. Sie werden in dieser Spec zuerst eindeutig gemacht (User Story 4), bevor der Name
  neu vergeben wird.
- [`017-extension-host`](../017-extension-host/spec.md): Das Schema der Erweiterungen und die
  Verweise im vault-sdk ändern sich (FR-009, FR-021).
- [`024-own-device-sync`](../024-own-device-sync/spec.md): Die festen Texte der Kryptografie und
  der Netzwerk-Protokollnamen gehören zum Sync. Mit den neuen Texten verstehen sich Geräte nur noch,
  wenn alle den neuen Namen tragen (FR-010).
- [`043-android-build`](../043-android-build/spec.md): Der Identifier und das Android-Plugin ändern sich.

## User Scenarios & Testing _(mandatory)_

### User Story 1 - Ein Name überall (Priority: P1)

Der Betreiber öffnet das Repo, die App, die Doku und die Werkzeuge und trifft nur noch auf den
Namen haex-vault. Wer den Namen später aus Versehen wieder einführt, bekommt das von der CI gesagt.

**Why this priority**: Das ist der Kern der Aufgabe. Ohne ihn gibt es keine umbenannte App.

**Independent Test**: Die Suche nach „holzi“ (ohne Beachtung der Groß- und Kleinschreibung) über
das Repo liefert außerhalb der Ausnahmeliste keinen Treffer; ein Test-PR, der „holzi“ in eine
Quelldatei schreibt, scheitert in der CI.

**Acceptance Scenarios**:

1. **Given** der Stand nach der Umbenennung, **When** der Betreiber nach „holzi“ sucht, **Then**
   gibt es Treffer nur an den Orten der Ausnahmeliste.
2. **Given** ein PR, der „holzi“ in Quellcode, Skripte oder CI-Konfiguration einführt, **When** die
   CI läuft, **Then** schlägt eine Prüfung fehl und nennt die Fundstellen.
3. **Given** die gestartete App, **When** der Betreiber Fenstertitel, Programmname, Einstellungen
   und Meldungen ansieht, **Then** steht überall „haex-vault“.

---

### User Story 2 - Die Passwörter aus der alten Datei übernehmen (Priority: P1)

Der Betreiber hat Passwörter in der alten Vault-Datei und will sie in haex-vault 3 weiter nutzen.
Er öffnet eine Kopie der Datei über den vorhandenen Datei-Import, gibt die Passphrase ein und sieht
seine Einträge.

**Why this priority**: Es ist die einzige Übernahme von Bestandsdaten, und sie enthält die
Passwörter des Betreibers. Scheitert sie, geht Wesentliches verloren.

**Independent Test**: Eine Kopie von `test.db` wird in haex-vault 3 importiert; die Zahl und der
Inhalt der Passwort-Einträge stimmen mit der alten App überein.

**Acceptance Scenarios**:

1. **Given** eine Kopie der alten Vault-Datei und ihre Passphrase, **When** der Betreiber sie über
   den Datei-Import öffnet, **Then** erscheint der Vault, und seine Passwort-Einträge sind lesbar
   und vollständig.
2. **Given** die alte Datei enthält Sync-Daten, die mit den alten Texten signiert wurden (Geräteliste,
   Zulassungen, Schlüssel-Umschläge), **When** der Vault geöffnet wird, **Then** blockiert das weder
   das Öffnen noch das Lesen der Passwörter; was sich nicht mehr prüfen lässt, ist als solches
   erkennbar, und die Geräte werden neu verknüpft.
3. **Given** die Datei lässt sich nicht öffnen, **When** der Betreiber den Fehler meldet, **Then**
   gibt es vor dem Umzug des Repos einen Export der Passwörter, den haex-vault 3 importieren kann
   (FR-013).

---

### User Story 3 - Die App läuft unter dem neuen Namen auf allen Geräten (Priority: P1)

Der Betreiber baut und startet haex-vault 3 auf dem Desktop und auf Android. Die Funktionen sind
dieselben wie vorher; neu sind der Name, der Identifier und die Texte im Netz.

**Why this priority**: Eine Umbenennung, die etwas kaputt macht, ist keine. Alle bisherigen
Funktionen müssen unter dem neuen Namen funktionieren.

**Independent Test**: Die vorhandene Testsuite einschließlich der End-to-End-Tests auf dem Desktop
und auf Android läuft ohne Fehler; zwei Geräte lassen sich unter dem neuen Namen verknüpfen und
synchronisieren; eine Erweiterung lädt über `haex-ext://`.

**Acceptance Scenarios**:

1. **Given** der umbenannte Stand, **When** die CI läuft, **Then** sind alle bisherigen Prüfungen
   grün, auch die Builds für Android und iOS.
2. **Given** zwei frische Installationen von haex-vault 3, **When** der Betreiber sie verknüpft,
   **Then** synchronisieren sie Vault-Daten wie zuvor.
3. **Given** eine installierte Erweiterung, **When** sie geöffnet wird, **Then** lädt sie über das
   neue Schema und erreicht ihre Daten.
4. **Given** die Android-App, **When** sie installiert wird, **Then** trägt sie den Identifier
   `space.haex.vault`, das Logo des Eichhörnchens und den Namen „haex-vault“.

---

### User Story 4 - Der alte Name haex-vault bleibt nicht doppeldeutig (Priority: P2)

Bevor der Name neu vergeben wird, ist überall, wo er die alte App meint, „haex-vault 2.x (Legacy)“
zu lesen. Erst danach steht „haex-vault“ für diese App.

**Why this priority**: Ohne diesen Schritt entstehen Sätze wie „haex-vault wird durch haex-vault
abgelöst“, und die Doku wird unlesbar. Er muss aber nur vor der Umbenennung geschehen, nicht davor
sichtbar sein.

**Independent Test**: Nach dem Schritt führt jede Fundstelle von „haex-vault“ in Specs, Doku, Code
und Kommentaren entweder auf die alte App (mit Legacy-Markierung) oder gar nicht mehr auf die alte
App; eine Stichprobe von Specs 034 und 037 bestätigt es.

**Acceptance Scenarios**:

1. **Given** der Stand vor der Umbenennung, **When** der Betreiber nach „haex-vault“ sucht, **Then**
   trägt jede Fundstelle, die die alte App meint, die Markierung „2.x“ (beim ersten Auftreten je
   Datei „2.x (Legacy)“) oder den Zusatz `legacy`.
2. **Given** das Importmodul für die alte App, **When** der Betreiber es im Code sucht, **Then**
   heißt es `legacy_haex_vault`.

---

### User Story 5 - Das Repo zieht in haex-vault um (Priority: P2)

Der Betreiber findet die App unter `github.com/haex-space/haex-vault`. Die alte App liegt
archiviert unter `haex-vault-legacy`. Alle alten PRs, Issues und Links funktionieren weiter.

**Why this priority**: Der Umzug ist das Ziel, aber er kommt zuletzt, damit Einstellungen und
Geheimnisse des Repos nur einmal unter dem endgültigen Namen eingerichtet werden müssen.

**Independent Test**: Die Adresse `haexmas/holzi` leitet auf `haex-space/haex-vault` weiter; ein
alter PR (#474) ist dort auffindbar; die CI läuft dort grün.

**Acceptance Scenarios**:

1. **Given** der Umzug ist abgeschlossen, **When** der Betreiber `haex-space/haex-vault` öffnet,
   **Then** zeigt es die umbenannte App auf `main` mit der kompletten Historie.
2. **Given** das archivierte Legacy-Repo, **When** der Betreiber es öffnet, **Then** weist es auf
   das neue Repo hin und ist schreibgeschützt.
3. **Given** das neue Repo, **When** ein PR auf `main` geöffnet wird, **Then** gelten dieselben
   Schutzregeln und Pflichtprüfungen wie vorher.

---

### User Story 6 - Die Geschwister-Repos passen zum neuen Namen (Priority: P3)

Das vault-sdk, haextension, die atoms, die spaex-Konfiguration und haex-crdt tragen den neuen Namen
und verweisen auf den neuen Stand dieser App.

**Why this priority**: Sie sind nicht Teil der App, aber ein halber Umzug lässt Verweise und
Werkzeuge ins Leere laufen.

**Independent Test**: Nach den Änderungen baut jedes betroffene Repo, und keines verweist mehr auf
den alten Namen oder auf Typen, die es nicht mehr gibt.

**Acceptance Scenarios**:

1. **Given** das vault-sdk, **When** eine Erweiterung gegen die neue Hauptversion gebaut wird,
   **Then** erscheinen die umbenannten Typen, und es gibt keine Alias-Namen für die alten.
2. **Given** das Projekt mit dem spaex-Werkzeug, **When** die Konfiguration installiert wird,
   **Then** trägt die Projektidentität den neuen Namen und die Constitution wird mit dem neuen
   Molecule gebaut.

---

### Edge Cases

- Was geschieht mit offenen PRs und Worktrees während der Umbenennung? Sie werden vorher
  abgeschlossen oder verworfen; ein eingefrorener Stand verhindert Konflikte mit der mechanischen
  Änderung.
- Wie sieht der Betreiber, dass ein Gerät noch den alten Namen im Netz verwendet? Es verbindet sich
  nicht mehr; ein altes holzi und ein neues haex-vault 3 verstehen sich bewusst nicht (FR-010).
- Was passiert, wenn auf demselben Rechner noch die alte App haex-vault 2.x mit Daten unter dem
  Identifier `space.haex.vault` liegt? Beide nutzen denselben Datenordner. Der Betreiber entfernt
  den alten Ordner von Hand, bevor er haex-vault 3 dort zum ersten Mal startet; die App prüft und
  behandelt das nicht (FR-013).
- Was geschieht mit Dateien, die den alten Namen im Dateinamen tragen (zum Beispiel temporäre
  `.holzi-part-…`-Dateien nicht abgeschlossener Übertragungen)? Sie werden nicht gesucht und nicht
  angefasst; neue Dateien tragen den neuen Namen.
- Wie lässt sich die alte Vault-Datei prüfen, ohne sie zu gefährden? Es wird nur mit einer Kopie
  gearbeitet; das Original und die Sicherung bleiben unverändert.
- Was geschieht, wenn die Umbenennung in der Mitte abbricht? Jeder Schritt ist ein eigener PR; der
  Tag `holzi-final` hält den letzten Stand unter dem alten Namen fest.

## Requirements _(mandatory)_

### Functional Requirements

**Der Name**

- **FR-001**: Der Name „haex-vault“ ersetzt „holzi“ in jeder Schreibweise (`holzi`, `Holzi`,
  `HOLZI`, `holzi_…`, `holzi-…`, `Holzi…`), in Code, Typen, Dateinamen, Konfiguration, Skripten,
  CI, Texten der Oberfläche, Kommentaren, Specs und Doku. Dabei folgt jede Schreibweise der
  Konvention der Umgebung (Rust-Crate `haex_vault`, Typ `VaultError`, Umgebungsvariable
  `HAEX_VAULT_…`).
- **FR-002**: Es gibt keinen Alias für einen alten Namen: kein Re-Export, keine zweite Variable,
  kein zusätzliches Schema.
- **FR-003**: Version, Produktname, Fenstertitel und Paketnamen der App stehen auf „haex-vault“
  und 3.0.0.
- **FR-004**: Die Ausnahmeliste ist abschließend: Git-Historie und der Tag `holzi-final`, das
  Verzeichnis dieser Spec (sie beschreibt die Umbenennung und braucht den alten Namen), und die
  Textstellen, an denen der alte Name ausdrücklich als Vergangenheit benannt wird (Begriffe,
  README-Hinweis). Alles andere enthält den Namen nicht mehr.
- **FR-005**: Eine Prüfung in der CI scheitert, wenn „holzi“ (ohne Beachtung der Schreibweise)
  außerhalb der Ausnahmeliste vorkommt. Sie nennt die Fundstellen und ist eine Pflichtprüfung
  für `main`.

**Identität der App**

- **FR-006**: Der Identifier der App ist `space.haex.vault` auf Desktop, Android und iOS.
- **FR-007**: Das Android-Plugin und sein Kotlin-Paket tragen den neuen Namen; Android baut, startet
  und führt die Funktionen der App aus.
- **FR-008**: Die Namen der Umgebungsvariablen, Systemeigenschaften und Prüfpfade der Testwerkzeuge
  (End-to-End-Rig, Skripte, CI) tragen den neuen Namen und sind überall gleich geändert.
- **FR-009**: Das Schema der Erweiterungen ist `haex-ext://` (und die `.localhost`-Fassung dazu),
  in der Sicherheitsrichtlinie der App, im Erweiterungs-Host und in den Tests.

**Kryptografie und Netz**

- **FR-010**: Alle festen Texte, die in Schlüsselableitung, Signaturen, Hashes, Protokollnamen und
  Kennungen eingehen, tragen den neuen Namen: das HKDF-Salz und die Info-Texte der Vault-Identität,
  alle Signatur-Domänen, die Schlüssel-ID, die Gerätenamen- und Presence-Ableitung, die
  Pairing-Texte, die Netzwerk-Protokollnamen (`…-sync/1`, `…-link/1`), die UUID-Namensräume der
  lokalen Provider, das Visitenkarten-Präfix und das Hugging-Face-Präfix. Ein Gerät mit den
  neuen Texten verbindet sich bewusst nicht mit einem Gerät mit den alten.
- **FR-011**: Die Tabelle `holzi_maintenance_no_sync`, der Manifest-Schlüssel `x-holzi`, der
  MCP-Name `holzi-approve`, die Android-Systemeigenschaft für die Plattformprobe, der
  User-Agent der Netzwerkaufrufe und alle weiteren Kennungen tragen den neuen Namen.
- **FR-012**: Für jede der geänderten Texte aus FR-010 gibt es einen Test, der den neuen Wert
  festschreibt, damit eine spätere Änderung nicht unbemerkt geschieht (die bestehenden Tests werden
  mit dem neuen Wert aktualisiert; kein Test prüft einen alten Namen).

**Übernahme**

- **FR-013**: Die App enthält keine Migration alter Daten und sucht nirgends nach alten
  Installationen. Der einzige Weg ist der vorhandene Datei-Import (`import_instance`). Liegen auf
  einem Gerät noch Daten der alten App haex-vault 2.x im Datenordner `space.haex.vault`, löscht der
  Betreiber ihn von Hand vor dem ersten Start; die App prüft das nicht.
- **FR-014**: Vor dem Umzug des Repos wird eine Kopie der alten Vault-Datei in der umbenannten App
  geöffnet. Sind die Passwörter lesbar, ist der Weg bestätigt. Sind sie es nicht, kommt vorher
  ein einfacher Export der Passwörter (CSV oder Bitwarden-JSON), den die vorhandenen Importer
  lesen können; er wird in dieser Spec geplant und hier nur ausgelöst, wenn die Prüfung scheitert.
- **FR-015**: Das Original der alten Datei und ihre Sicherung bleiben unverändert; alle Versuche
  laufen auf Kopien.
- **FR-016**: Kann die App eine Signatur aus der alten Datei nicht mehr prüfen, zeigt sie das als
  Zustand an, statt abzubrechen; die Passwörter bleiben lesbar.

**Der alte Name haex-vault**

- **FR-017**: Vor FR-001 wird jede Erwähnung der alten App in Specs, Doku, Kommentaren und Code als
  „haex-vault 2.x (Legacy)“ gekennzeichnet, in einer Datei beim ersten Auftreten mit dem Zusatz
  „(Legacy)“, danach als „haex-vault 2.x“; Bezeichner und Dateinamen tragen `legacy`; das
  Importmodul heißt `legacy_haex_vault`. Verweise auf das Repo der alten App zeigen auf
  `haex-space/haex-vault-legacy`.
- **FR-018**: Verzeichnisnamen der Specs (`specs/0xx-…`) bleiben, auch wenn sie den alten Namen
  tragen, weil Verweise und PR-Titel auf sie zeigen; die Dateien darin folgen FR-001.

**Repo und Umzug**

- **FR-019**: Das Repo `haex-space/haex-vault` (2.x) wird zu `haex-space/haex-vault-legacy`
  umbenannt, bekommt einen Hinweis auf das neue Repo und wird archiviert.
- **FR-020**: Das Repo `haexmas/holzi` wird nach `haex-space` transferiert und zu `haex-vault`
  umbenannt. Default-Branch ist `main`; Branch-Schutz, Pflichtprüfungen, Geheimnisse (darunter der
  Android-Signaturschlüssel, sobald er besteht) und automatische Abläufe sind danach neu
  eingerichtet.
- **FR-021**: Alle Verweise im Repo auf die eigene Adresse (Paket-Metadaten, README, Flake,
  Abzeichen, Links in der Doku) zeigen auf `haex-space/haex-vault`.
- **FR-022**: Die lokalen Checkouts sind umbenannt (der alte Ordner `haex-vault` wird zu
  `haex-vault-legacy`, `holzi` wird zu `haex-vault`), und die Remote-Adressen und Worktrees
  stimmen.

**Geschwister-Repos**

- **FR-023**: vault-sdk erhält eine neue Hauptversion: Typen mit dem alten Namen (zum Beispiel für
  Kontaktadressen und Kontaktgeräte) werden umbenannt, ohne Alias; Kommentare und Verweise auf die
  Specs dieser App folgen FR-001.
- **FR-024**: haextension (haex-ui) und haex-crdt folgen dem neuen Namen und der neuen Version des
  vault-sdk und bauen und testen grün.
- **FR-025**: Das Molecule `com.github.haexmas.atoms.holzi` und die Projektidentität
  `com.github.haexmas.holzi` im spaex-Werkzeug tragen den neuen Namen; die Constitution wird mit
  einem gepinnten Stand des atoms-Repos neu gebaut, und das Ergebnis ist geprüft, bevor es
  übernommen wird.

**Reihenfolge**

- **FR-026**: Die Arbeit läuft in dieser Reihenfolge, jeder Schritt in einem eigenen PR oder
  Handgriff: Sicherung der alten Datei und Tag `holzi-final`; Logo (erledigt, #474); Einfrieren
  der offenen Arbeit; Legacy-Markierung (FR-017); die Umbenennung in kleinen, mechanischen PRs
  (Rust und Build, Kotlin und Plugin, Skripte und CI, Specs und Doku, Identifier und Schema); die
  CI-Prüfung (FR-005); die Prüfung der alten Datei (FR-014); die Geschwister-Repos (FR-023 bis
  FR-025); zuletzt der Umzug (FR-019 bis FR-022).

### Key Entities

- **Alter Name (holzi)**: die Zeichenfolge in jeder Schreibweise, die verschwindet.
- **Ausnahmeliste**: die abschließende Liste der Orte, an denen der alte Name bleibt (FR-004).
- **Alte Vault-Datei**: die Datenbank des Betreibers mit seinen Passwörtern; Quelle der einzigen
  Übernahme.
- **Feste Texte der Kryptografie**: Texte, die in Ableitungen, Signaturen und Protokollnamen
  eingehen und die Verträglichkeit zwischen Geräten bestimmen.
- **Repo-Adresse**: Ort der Quellen und ihrer Historie, bisher `haexmas/holzi`, künftig
  `haex-space/haex-vault`.

## Success Criteria _(mandatory)_

### Measurable Outcomes

- **SC-001**: Die Suche nach „holzi“ über das Repo liefert außerhalb der Ausnahmeliste null
  Treffer, und ein PR, der den Namen neu einführt, scheitert in der CI.
- **SC-002**: Nach der Umbenennung sind alle Pflichtprüfungen auf `main` grün, einschließlich der
  End-to-End-Tests auf dem Desktop und der Builds für Android und iOS, ohne dass ein Test gelöscht
  oder abgeschwächt wurde.
- **SC-003**: Der Betreiber öffnet die Kopie der alten Vault-Datei in haex-vault 3 und findet alle
  Passwort-Einträge (gleiche Anzahl wie in holzi) lesbar.
- **SC-004**: Zwei Geräte mit haex-vault 3 verknüpfen sich und gleichen eine Änderung ab; die
  Mehrgeräte-Tests aus [Spec 033](../033-multi-device-e2e/spec.md) laufen unverändert in ihren
  bisherigen Wartezeiten durch.
- **SC-005**: Keine Fundstelle von „haex-vault“ in Specs, Doku und Code ist doppeldeutig: Jede, die
  die alte App meint, trägt die Markierung „2.x“ (beim ersten Auftreten je Datei „2.x (Legacy)“)
  oder `legacy`; bewusst ausgenommen sind Dateinamen und der Verzeichnisname
  `037-haex-vault-import` (FR-018).
- **SC-006**: Die Adresse `haexmas/holzi` und ein alter PR (#474) führen nach dem Umzug auf das
  neue Repo; das Legacy-Repo ist archiviert und verweist auf das neue.
- **SC-007**: Jedes betroffene Geschwister-Repo baut und testet nach der Umstellung grün, und
  keines verweist noch auf den alten Namen.

## Assumptions

- holzi hat keine Nutzer außer dem Betreiber; deshalb gibt es keine Übergangszeit, keinen Alias
  und keine Rückwärtsverträglichkeit mit Geräten unter dem alten Namen.
- Die alte Vault-Datei ist gesichert (Sicherung außerhalb des Repos, geprüft mit gleicher
  Prüfsumme); der Tag `holzi-final` steht auf `main` am 2026-10-16.
- Die Übernahme der Passwörter braucht nur die Datenbank; Modelle, Erweiterungen und andere Daten
  werden nicht übernommen.
- Die Version 3.0.0 gilt für die App; das vault-sdk bekommt eine eigene neue Hauptversion nach
  seiner Zählung.
- Die Ausnahmeliste hält diese Spec selbst heraus: Sie beschreibt die Umbenennung und kann den
  alten Namen nicht ersetzen, ohne unlesbar zu werden.
- Die Spec-Verzeichnisnamen bleiben, weil PR-Titel, Verweise und Gedächtnisstützen auf die Nummern
  zeigen.
- Der Umzug des Repos braucht Administratorrechte in der Organisation `haex-space`; die Übertragung
  startet der Account, dem das Repo gehört.
- Der Umbau der Quellen ist mechanisch; über die Größe jedes PR entscheidet die Aufteilung nach
  Gebieten, nicht ein Zeilenlimit (mechanische Umbenennungen sind eine anerkannte Ausnahme der
  Constitution).
