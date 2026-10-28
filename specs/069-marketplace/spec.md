# Feature Specification: Marketplace für Erweiterungen

**Feature Branch**: `069-marketplace`

**Created**: 2026-10-28

**Status**: Draft

**Input**: „ich möchte gerne noch den marketplace in haex-vault einbinden, so wie wir das in
haex-vault-legacy auch gemacht haben“ (Betreiber, 2026-10-28). HaexVault soll Erweiterungen aus einem
Marketplace durchsuchen und installieren können, wie haex-vault 2.x (Legacy) es tut, aber auf holzi
angepasst. Referenzen: haex-vault 2.x @ `8dce379d94e18fcd42c3b73686a06f984ca3f574`
(`src/composables/useMarketplaces.ts`, `src/composables/useMarketplaceCatalog.ts`,
`src/components/haex/system/marketplace*.vue`, `src/components/haex/system/settings/extensions/marketplaces.vue`,
`src/stores/extensions/index.ts` `downloadAndPreviewAsync`).

## Clarifications

### Session 2026-10-28

- Q: Wo läuft der Netzwerkverkehr zum Marketplace? → A: Im Host (Rust), nicht im Webview: Katalog,
  Kategorien, Download-Adresse und das Laden der Installationsdatei samt Prüfsumme. Die geladenen Bytes
  gehen direkt in die bestehende Installations-Vorschau (Spec 017), nicht über das Webview. Es gibt keinen
  zweiten HTTP-Pfad im Frontend (eine Implementierung pro Regel).
- Q: Wo liegen die Zugangsdaten eines eigenen (privaten) Marketplace? → A: Im Passwortmanager. Der
  Marketplace verweist auf einen Eintrag (Muster aus Spec 052); in der Marketplace-Liste steht kein
  Geheimnis. Auth-Arten in dieser Spec: keine, Bearer-Token, Basic. DID-Anmeldung wie im Legacy kommt später.
- Q: Wie wird vorgegangen und wo sitzt die Oberfläche? → A: Spec zuerst als reiner Docs-PR, danach
  gestufte PRs. Der Katalog ist eine eigene App im Window Manager (wm), die Verwaltung der Marketplaces
  sitzt unter Einstellungen → Erweiterungen.

### Session 2026-10-28 (Betreiber, nach dem Plan)

- Q: Wie fest ist der Standard-Marketplace? → A: Gar nicht. Die Konfiguration ist **generisch**: Jeder
  Marketplace ist eine gleichrangige Zeile aus Name, Adresse und optionalen Zugangsdaten; HaexVault
  behandelt keine Adresse und keinen Betreiber besonders. Der Betreiber stellt einen öffentlichen
  Marketplace bereit; der Eintrag dafür ist nur eine **Voreinstellung** (eine gewöhnliche, änderbare und
  löschbare Zeile, die ein neuer Vault mitbringt).
- Q: Mehrere Marketplaces? → A: Beliebig viele, **gleichzeitig** genutzt. Nutzer dürfen auch selbst einen
  Marketplace hosten. Deshalb ist das Marketplace-Protokoll ein dokumentierter, offener Vertrag dieses
  Repositories (FR-034), nicht mehr nur „die Schnittstelle von haex-space“.
- Q: Was passiert, wenn zwei Marketplaces dieselbe Erweiterung führen? → A: Beide Einträge erscheinen,
  mit Quelle; es wird nichts zusammengelegt, damit ein selbst gehosteter Marketplace den Eintrag eines
  anderen nicht verdecken kann.

### Session 2026-10-28 (Plan)

- Q: Liefert der Marketplace Prüfsumme und Größe verlässlich? → A: Ja. Der Dienst
  `marketplace.haex.space` liefert zu jeder Fassung `bundleHash` (SHA-256, hex) und `bundleSize` und
  zu jedem Download eine kurzlebige signierte Adresse auf einem anderen Host. Die Prüfsumme wird
  deshalb zur Pflicht (FR-013); fehlt sie, wird nicht installiert.
- Q: Gibt es eine Größengrenze für `.xt`? → A: Ja, 64 MiB (Spec 017, `limits::ARCHIVE_BYTES`). Der
  Download hält dieselbe Grenze ein (FR-014).

## Begriffe

- **Marketplace**: ein Katalogdienst, der das **Marketplace-Protokoll** spricht
  ([contracts/marketplace-api.md](contracts/marketplace-api.md)) und Erweiterungen (Spec 017) mit
  Beschreibung, Herausgeber, Kategorie, Fassungen und einer Download-Adresse anbietet. Ein Marketplace ist
  für HaexVault nur eine Zeile aus Name, Adresse und optionalen Zugangsdaten; alle sind gleichrangig.
  Betreiben kann ihn jeder: das Projekt (öffentlich), eine Firma oder der Nutzer selbst.
- **Voreinstellung**: der Eintrag für den öffentlichen Marketplace des Projekts, den ein neuer Vault
  mitbringt. Er ist eine gewöhnliche Zeile und lässt sich ändern, ausschalten und löschen.
- **Katalog**: die gemeinsame Liste der Erweiterungen aller eingeschalteten Marketplaces, gleichzeitig.
- **Katalogeintrag**: eine Erweiterung im Katalog mit den Angaben des Marketplace (Name, Kurzbeschreibung,
  Symbol, Herausgeber, Kategorie, Bewertung, Zahl der Downloads, verfügbare Fassungen).
- **Quelle**: der Marketplace, aus dem ein Katalogeintrag stammt.
- **Zugangsdaten-Eintrag**: der Eintrag im Passwortmanager (Spec 034), auf den ein eigener Marketplace
  verweist.
- **Installations-Vorschau**: die bestehende Prüfung und Bestätigung beim Installieren einer
  Erweiterung (Spec 017 US1, `InstallDialog`): Herausgeber, erklärte Berechtigungen, Update oder
  Downgrade. Sie ist für Dateien und Marketplace dieselbe.

## Beziehung zu bestehenden Specs

- **Spec 017** (Erweiterungs-Host): Diese Spec ändert nichts an Signatur, Berechtigungen und
  Installations-Vorschau. Eine Erweiterung aus dem Marketplace ist ein gewöhnliches `.xt`-Bundle; ob sie
  vertrauenswürdig ist, entscheidet die Signatur im Bundle und die Bestätigung des Nutzers, **nicht** die
  Angabe „geprüft“ des Marketplace. Das Installieren aus einer Datei bleibt unverändert.
- **Spec 034** (Passwortmanager) und **Spec 052** (Zugangsdaten wählen): Die Auswahl des
  Zugangsdaten-Eintrags eines Marketplace benutzt dieselbe Auswahl wie die Speicherverbindungen; die
  Regeln für Löschwarnung und Freigaben aus 034 und 052 gelten entsprechend.
- **Spec 024** (Sync) und **Spec 023** (Einstellungen): Die Marketplace-Liste sind Vault-Daten und kommen
  auf alle eigenen Geräte; in den Einstellungen gibt es keine Speichern-Schaltfläche, Änderungen gelten
  sofort (Ausnahme: das Formular eines Marketplace, siehe FR-021).
- **Spec 021** (MCP-Server) und **Spec 018/047** (Agenten-Werkzeuge): Agenten und Erweiterungen können
  weder den Katalog abfragen noch installieren (siehe FR-030).
- **ADR-0004**: Der Katalog ist eine holzi-eigene Funktion, keine Host-Funktion für Erweiterungen.

## User Scenarios & Testing _(mandatory)_

### User Story 1 - Erweiterung im Katalog finden und installieren (Priority: P1)

Anna öffnet in HaexVault die App „Erweiterungen entdecken“. Sie sieht den gemeinsamen Katalog
ihrer Marketplaces als Kacheln mit Symbol, Name, Kurzbeschreibung, Herausgeber und Bewertung. Sie
filtert nach der Kategorie „Produktivität“, sucht nach „Kalender“, öffnet die Details einer Erweiterung
und tippt „Installieren“. HaexVault lädt die Installationsdatei und zeigt dieselbe Installations-Vorschau
wie beim Installieren aus einer Datei: Herausgeber, Berechtigungen, Bestätigung. Nach „Installieren“
erscheint die Erweiterung unter ihren Apps, und die Kachel zeigt „Installiert“.

**Why this priority**: Das ist der Kern: ohne Katalog muss der Nutzer `.xt`-Dateien selbst finden,
herunterladen und von Hand installieren.

**Independent Test**: Mit einem Test-Marketplace (lokaler Server mit zwei Erweiterungen, einer
signierten `.xt`) den Katalog öffnen, eine Erweiterung wählen, installieren, in der Installations-Vorschau
bestätigen; die Erweiterung läuft als App, die Kachel zeigt „Installiert“.

**Acceptance Scenarios**:

1. **Given** mindestens ein eingeschalteter Marketplace ist erreichbar, **When** Anna die App öffnet,
   **Then** sieht sie die erste Seite des gemeinsamen Katalogs, sortiert nach Beliebtheit (Downloads), mit Kategorien zum Filtern.
2. **Given** der Katalog ist offen, **When** Anna in das Suchfeld tippt, **Then** aktualisiert sich die
   Liste nach einer kurzen Pause (nicht nach jedem Buchstaben) und die Seite springt auf Seite 1.
3. **Given** Anna wählt eine Kategorie, **When** sie zusätzlich sucht, **Then** gelten beide Filter
   zusammen; „Alle“ hebt den Kategoriefilter auf.
4. **Given** der Katalog hat mehr Treffer als eine Seite, **When** Anna blättert, **Then** lädt die
   nächste Seite, ohne dass der Rest der App blockiert.
5. **Given** Anna tippt in den Details auf „Installieren“, **When** die Datei geladen ist, **Then**
   erscheint die Installations-Vorschau mit Herausgeber, Berechtigungen und der Angabe, aus welchem
   Marketplace die Datei kam; erst „Installieren“ dort installiert.
6. **Given** Anna bricht die Installations-Vorschau ab, **When** sie den Dialog schließt, **Then** ist
   nichts installiert und nichts auf der Platte geblieben.
7. **Given** die Installation war erfolgreich, **When** Anna zur Kachel zurückkehrt, **Then** zeigt sie
   „Installiert“ mit der installierten Fassung.
8. **Given** Anna hat zwei eingeschaltete Marketplaces, **When** sie den Katalog öffnet, **Then** sieht sie
   die Einträge beider gleichzeitig, jede Kachel nennt ihren Marketplace, und ein Filter „Quelle“ schränkt
   auf einen ein.
9. **Given** beide Marketplaces führen eine Erweiterung gleichen Namens, **When** Anna den Katalog
   ansieht, **Then** erscheinen beide Kacheln mit ihrer Quelle, und Anna wählt, aus welcher sie
   installiert.

---

### User Story 2 - Sicher: nur geprüfte Bytes werden installiert (Priority: P1)

Der Marketplace ist ein Dritter. Was er liefert, darf HaexVault nicht blind installieren. HaexVault
vergleicht die Prüfsumme, die der Marketplace zu einer Fassung angibt, mit der geladenen Datei; stimmt
sie nicht, oder fehlt sie, wird abgebrochen. Unabhängig davon prüft die Installations-Vorschau wie immer die Signatur im
Bundle. Behauptet der Marketplace einen Herausgeber „geprüft“, ist das nur eine Anzeige.

**Why this priority**: Der Marketplace-Betreiber (oder ein Angreifer im Netz) darf nie ohne Wissen des
Nutzers Code in seinen Vault bringen.

**Independent Test**: Test-Marketplace mit (a) falscher oder fehlender Prüfsumme, (b) unsigniertem Bundle, (c) Bundle
mit anderem Herausgeber als angezeigt, (d) viel zu großer Datei: In allen Fällen wird nichts installiert und
der Nutzer sieht den Grund.

**Acceptance Scenarios**:

1. **Given** die Prüfsumme des Marketplace stimmt nicht mit der geladenen Datei überein, **When** der
   Download endet, **Then** bricht HaexVault ab, installiert nichts und meldet „Die geladene Datei
   stimmt nicht mit der Angabe des Marketplace überein“; fehlt die Prüfsumme, meldet es „Der Marketplace liefert keine Prüfsumme“.
2. **Given** das Bundle ist ungültig oder unsigniert, **When** die Installations-Vorschau es prüft,
   **Then** zeigt sie denselben Fehler wie beim Installieren aus einer Datei (Spec 017) und erlaubt keine
   Installation.
3. **Given** der Herausgeberschlüssel im Bundle weicht vom Herausgeber im Katalog ab, **When** die Vorschau
   erscheint, **Then** zeigt sie den Schlüssel aus dem Bundle als Herausgeber und weist auf die Abweichung zum
   Katalog hin.
4. **Given** der Marketplace liefert eine Datei, die größer ist als 64 MiB (FR-014) oder mehr Bytes liefert als angegeben,
   **When** der Download läuft, **Then** bricht er ab, sobald die Grenze überschritten ist.
5. **Given** der Marketplace antwortet mit einer Weiterleitung auf einen anderen Host, **When** HaexVault
   der Weiterleitung folgt, **Then** sendet es dorthin keine Zugangsdaten des Marketplace.

---

### User Story 3 - Aktualisieren und Entfernen (Priority: P2)

Anna hat „Kalender“ in Fassung 1.2.0 installiert; der Katalog führt 1.3.0. Die Kachel zeigt „Update
verfügbar“. Ein Tipp öffnet die Installations-Vorschau im Modus „Update“ (Daten bleiben erhalten, die
Berechtigungsänderungen werden gezeigt). Über „Entfernen“ in den Details startet Anna die bestehende
Entfernung (Spec 017) samt der Frage, ob die Daten der Erweiterung behalten werden.

**Why this priority**: Ohne Update-Hinweis bleibt der Nutzer auf alten Fassungen, aber die erste
Installation (US1) trägt die Funktion bereits.

**Independent Test**: Test-Marketplace mit zwei Fassungen einer bereits installierten Erweiterung:
Kachel zeigt „Update verfügbar“, Update läuft über die Vorschau, danach „Installiert“ mit neuer Fassung;
„Entfernen“ entfernt die Erweiterung nach Bestätigung.

**Acceptance Scenarios**:

1. **Given** die installierte Fassung ist älter als die neueste Fassung im Katalog, **When** Anna den
   Katalog öffnet, **Then** zeigt die Kachel „Update verfügbar“ mit beiden Fassungen.
2. **Given** die installierte Fassung ist gleich oder neuer, **When** Anna die Kachel sieht, **Then**
   erscheint kein Update-Hinweis.
3. **Given** Anna tippt „Aktualisieren“, **When** die Datei geladen ist, **Then** zeigt die Vorschau
   „Update von A auf B“ und welche Berechtigungen neu sind; die Daten der Erweiterung bleiben.
4. **Given** der Katalog führt mehrere Fassungen, **When** Anna in den Details eine ältere wählt,
   **Then** zeigt die Vorschau die Bestätigung für ein Downgrade wie in Spec 017.
5. **Given** eine Erweiterung ist installiert, **When** Anna in den Details „Entfernen“ wählt, **Then**
   läuft die bestehende Entfernung mit der Frage nach den Daten.
6. **Given** der Nutzer öffnet die Liste der installierten Erweiterungen (Einstellungen), **When** der
   Katalog zuvor eine neuere Fassung gemeldet hat, **Then** zeigt die Zeile „Update verfügbar“; ein Tipp
   öffnet den Katalogeintrag.

---

### User Story 4 - Marketplaces verwalten (Priority: P2)

Unter Einstellungen → Erweiterungen → Marketplaces sieht Anna eine Liste ihrer Marketplaces, bei einem
neuen Vault mit der Voreinstellung. Alle Zeilen sind gleich: Sie schaltet jede ein oder aus (gilt sofort),
ändert die Reihenfolge, legt einen neuen mit Name und Adresse an, bearbeitet und löscht jeden. Wer einen
eigenen Marketplace hostet (Firma, Verein, Heimserver), trägt dessen Adresse ein, und er arbeitet neben
den anderen, ohne dass HaexVault etwas darüber wissen muss.

**Why this priority**: Wichtig für Firmen und Selbst-Hoster, aber die Voreinstellung reicht für
den ersten Nutzen (US1).

**Independent Test**: Zweiten Marketplace anlegen, ausschalten, einschalten, nach oben sortieren,
löschen; Katalog und Reihenfolge der Quellen folgen jeweils sofort.

**Acceptance Scenarios**:

1. **Given** ein neuer Vault, **When** Anna die Liste öffnet, **Then** steht dort die Voreinstellung als
   gewöhnliche Zeile, eingeschaltet, ohne dass sie etwas einrichten musste.
2. **Given** Anna legt einen Marketplace mit Name und Adresse an, **When** sie bestätigt, **Then**
   prüft HaexVault die Erreichbarkeit der Adresse und zeigt das Ergebnis („erreichbar, N Erweiterungen“
   oder die Fehlerursache); sie kann trotzdem speichern.
3. **Given** ein Marketplace ist ausgeschaltet, **When** Anna den Katalog öffnet, **Then** kommen von
   ihm weder Einträge noch Kategorien noch Fehler.
4. **Given** zwei Marketplaces, **When** Anna einen nach oben schiebt, **Then** ändert sich ihre
   Anordnung im Katalog sofort.
5. **Given** die Voreinstellung, **When** Anna sie bearbeitet, ausschaltet oder löscht, **Then** geht das wie
   bei jedem anderen Marketplace; HaexVault legt sie nie von allein wieder an oder schaltet sie wieder ein.
6. **Given** Anna löscht einen Marketplace, **When** sie bestätigt, **Then** verschwindet er
   aus der Liste und aus dem Katalog; bereits installierte Erweiterungen bleiben unberührt.
7. **Given** Anna hat auf Gerät A einen Marketplace angelegt, **When** Gerät B synchronisiert hat,
   **Then** steht er dort ebenfalls in der Liste (Zugangsdaten über den Eintrag im Passwortmanager).
8. **Given** Ben betreibt einen eigenen Marketplace nach dem Marketplace-Protokoll auf einer beliebigen
   Adresse (auch mit Pfadanteil, auch `http://` im lokalen Netz), **When** Anna die Adresse einträgt,
   **Then** erscheinen seine Erweiterungen im Katalog neben denen der anderen, ohne Änderung an
   HaexVault.
9. **Given** Anna hat keinen Marketplace (Vault vor dieser Funktion angelegt oder alle gelöscht), **When**
   sie den Katalog öffnet, **Then** erklärt der Leerzustand das und bietet „Öffentlichen Marketplace
   hinzufügen“ (die Voreinstellung) und „Eigenen Marketplace hinzufügen“ an.

---

### User Story 5 - Privater Marketplace mit Zugangsdaten (Priority: P3)

Ein Team betreibt einen eigenen Marketplace hinter einer Anmeldung (Bearer-Token oder Benutzername und
Passwort). Beim Anlegen wählt Ben statt eines Tokens im Formular einen Eintrag aus dem Passwortmanager
(oder legt dort einen neuen ab). Der Marketplace verweist auf diesen Eintrag; in der Liste und in den
Vault-Daten steht kein Geheimnis.

**Why this priority**: Nur für geschlossene Marketplaces nötig; ein öffentlicher Marketplace braucht keine
Anmeldung.

**Independent Test**: Test-Marketplace, der ohne gültigen Authorization-Header 401 liefert: ohne
Zugangsdaten zeigt der Katalog „Anmeldung nötig“ für diese Quelle, mit gewähltem Eintrag lädt er.

**Acceptance Scenarios**:

1. **Given** Ben wählt als Auth-Art „Bearer“, **When** er das Formular öffnet, **Then** kann er einen
   passenden Passwort-Eintrag wählen (Passwort-Feld ist das Token); bei „Basic“ ist es Benutzername und
   Passwort.
2. **Given** Ben bestätigt das Formular, **When** HaexVault die Verbindung prüft, **Then** sendet es die
   Zugangsdaten und meldet Erfolg oder „Anmeldung abgelehnt“.
3. **Given** der Zugangsdaten-Eintrag wurde im Passwortmanager gelöscht, **When** der Katalog lädt,
   **Then** zeigt die Quelle „Zugangsdaten fehlen“ mit Sprung zum Formular; die anderen Quellen laden
   weiter.
4. **Given** der Zugangsdaten-Eintrag ist auf diesem Gerät noch nicht synchronisiert, **When** der Katalog
   lädt, **Then** zeigt die Quelle „wird synchronisiert“ statt eines Fehlers (Spec 052 Randfall).
5. **Given** Ben will einen Eintrag im Passwortmanager löschen, den ein Marketplace nutzt, **When** er
   löscht, **Then** warnt HaexVault und nennt den Marketplace (Spec 034 FR-034).
6. **Given** die Adresse beginnt mit `http://` und es sind Zugangsdaten gewählt, **When** Ben bestätigt,
   **Then** verweigert HaexVault das Speichern mit dem Hinweis, dass Zugangsdaten nur über `https://`
   gehen.

---

### Edge Cases

- **Offline oder Marketplace nicht erreichbar**: Der Katalog zeigt für die Quelle eine Fehlerzeile mit
  Ursache und „Erneut versuchen“; andere Quellen laden. Sind alle Quellen ausgefallen, steht ein
  Leerzustand mit „Keine Verbindung“, nicht „Keine Erweiterungen“.
- **Alle Marketplaces ausgeschaltet**: Leerzustand mit Verweis auf die Einstellungen.
- **Kein Marketplace eingerichtet**: Leerzustand mit „Öffentlichen Marketplace hinzufügen“ und „Eigenen
  Marketplace hinzufügen“ (US4-9).
- **Dieselbe Erweiterung in zwei Marketplaces**: Beide Einträge erscheinen mit ihrer Quelle, nichts wird
  zusammengelegt (FR-012). Die Vorschau zeigt den Herausgeberschlüssel aus dem Bundle, sodass ein
  untergeschobener Eintrag erkennbar ist.
- **Marketplace liefert Müll** (kein gültiges Format, fehlende Felder): Die Quelle zeigt „Antwort nicht
  verständlich“; ein Eintrag mit fehlenden Pflichtfeldern wird ausgelassen, nicht der ganze Katalog.
- **Langsame Quelle**: Eine Quelle, die nicht innerhalb einer festen Frist antwortet, gilt als
  ausgefallen und hält die anderen nicht auf.
- **Seite und Gesamtzahl bei mehreren Quellen**: Die Gesamtzahl ist die Summe der Quellen; die Blätterung
  bleibt stabil (kein Eintrag verschwindet beim Blättern).
- **Katalogeintrag bereits installiert, aber anderer Herausgeber**: Die Installations-Vorschau weist wie in
  Spec 017 auf „gleicher Name, anderer Herausgeber“ hin.
- **Kein Platz / Schreibfehler beim Download**: Abbruch ohne Rest; Fehlermeldung mit Ursache.
- **Download wird abgebrochen** (Nutzer schließt die App oder die Verbindung reißt): Nichts installiert,
  kein halbes Bundle; erneutes Tippen startet neu.
- **Marketplace ändert zwischen Details und Download die Fassung**: Die Vorschau zeigt die tatsächlich
  geladene Fassung.
- **Zwei Installationen gleichzeitig** (zwei Kacheln schnell nacheinander): Es läuft eine Vorschau; die
  zweite wartet oder wird abgelehnt mit Hinweis, nie zwei Dialoge übereinander.
- **Vault gesperrt oder geschlossen**: Während des Downloads gesperrt: Abbruch ohne Installation.
- **Mobil (Android, iOS)**: Einspaltige Kacheln, Touch-Ziele; der Datei-Install bleibt daneben möglich.
- **Windows**: Schlüssel- und Pfadbehandlung wie beim Datei-Install; der Download nutzt keinen
  Dateipfad im Webview.

## Requirements _(mandatory)_

### Functional Requirements

**Katalog**

- **FR-001**: HaexVault MUSS eine App „Erweiterungen entdecken“ im wm anbieten, die den Katalog aller
  eingeschalteten Marketplaces zeigt (Kacheln mit Symbol, Name, Kurzbeschreibung, Herausgeber, Kategorie,
  Bewertung, Zahl der Downloads).
- **FR-002**: Der Katalog MUSS nach Freitext suchen, nach Kategorie filtern und seitenweise blättern
  können; Filter wirken gemeinsam, eine Änderung des Filters springt auf Seite 1, Eingaben im Suchfeld
  werden kurz gebündelt.
- **FR-003**: Der Katalog MUSS standardmäßig nach Downloads absteigend sortieren.
- **FR-004**: Eine Kachel MUSS den Zustand „Nicht installiert“, „Installiert (Fassung)“ oder „Update
  verfügbar“ zeigen. Der Abgleich mit installierten Erweiterungen nutzt die Erweiterungs-Kennung des
  Marketplace, soweit er sie liefert, sonst den Namen; endgültig entscheidet die Installations-Vorschau
  anhand von Herausgeberschlüssel und Name aus dem Bundle (Spec 017).
- **FR-005**: Die Details einer Erweiterung MÜSSEN Beschreibung, Herausgeber (mit der Angabe „geprüft“
  des Marketplace als Anzeige), Bewertung, Fassungen, Kategorie, Tags und Quelle zeigen und von dort
  „Installieren“, „Aktualisieren“ und „Entfernen“ anbieten.
- **FR-006**: Texte und Symbole aus dem Marketplace MÜSSEN als reiner Text bzw. als Bild dargestellt
  werden, nie als aktiver Inhalt (kein HTML, keine Skripte). Symbole werden vom Host geladen und
  eingebettet, das Webview ruft keine Adresse eines Marketplace selbst auf.
- **FR-007**: Der Marketplace-Katalog MUSS auch ohne gewählte Installation nutzbar sein (nur ansehen);
  eine Anfrage an einen Marketplace sendet weder Vault- noch Geräte-Kennungen noch den Namen des Nutzers.

**Installieren, Aktualisieren, Entfernen**

- **FR-010**: „Installieren“ und „Aktualisieren“ MÜSSEN die Installationsdatei im Host laden und die Bytes
  direkt an die Installations-Vorschau (Spec 017) übergeben; es gibt keine zweite Installationsroutine.
- **FR-011**: Die Installations-Vorschau MUSS bei Marketplace-Herkunft zusätzlich die Quelle nennen, sonst
  aber unverändert Herausgeber, Berechtigungen, Update/Downgrade und die Bestätigungen aus Spec 017
  zeigen. Es wird nie ohne Bestätigung des Nutzers installiert.
- **FR-012**: Der Katalog MUSS die Einträge aller eingeschalteten Marketplaces gleichzeitig zeigen. Jeder
  Eintrag nennt seine Quelle. Einträge verschiedener Quellen werden **nie** zusammengelegt, auch nicht bei
  gleicher Erweiterungs-Kennung oder gleichem Namen (eine Quelle darf den Eintrag einer anderen nicht
  verdecken). Ein Filter „Quelle“ schränkt auf einen Marketplace ein; die Reihenfolge der Marketplaces
  bestimmt die Anordnung.
- **FR-013**: HaexVault MUSS die Prüfsumme (SHA-256), die der Marketplace zur Fassung angibt, vor der
  Vorschau gegen die geladenen Bytes prüfen und bei Abweichung abbrechen. Liefert der Marketplace keine
  Prüfsumme, MUSS die Installation abgelehnt werden. Zusätzlich MUSS die geladene Größe der
  angegebenen Größe entsprechen. Die Signatur im Bundle prüft die Vorschau unabhängig davon (Spec 017).
- **FR-014**: Der Download MUSS die Grenze von 64 MiB für eine Installationsdatei (Spec 017) einhalten
  und abbrechen, sobald sie überschritten ist, ohne die ganze Datei zu laden. Es bleibt nichts auf der
  Platte, wenn nicht installiert wird.
- **FR-015**: Der Download MUSS dem Nutzer Fortschritt zeigen und abbrechbar sein.
- **FR-016**: „Entfernen“ im Katalog MUSS die bestehende Entfernung aus Spec 017 nutzen (einschließlich
  der Frage nach den Daten).
- **FR-017**: Der Katalog MUSS beim Öffnen und auf Wunsch („Aktualisieren“-Knopf) nach Updates suchen;
  es gibt keine Abfragen im Hintergrund oder beim Start. Die Liste der installierten Erweiterungen
  (Einstellungen) MUSS einen bekannten Update-Hinweis zeigen, ohne selbst zu fragen.

- **FR-018**: Gibt eine Fassung eine Mindest- oder Höchstfassung von HaexVault an, MUSS der Katalog
  eine nicht passende Fassung als „Benötigt HaexVault ab/bis x“ kennzeichnen und die Installation
  dieser Fassung sperren; eine passende ältere Fassung bleibt wählbar. Die Anfragen nennen die
  Plattform und die Fassung von HaexVault, sonst nichts über Nutzer oder Gerät (FR-007).

**Marketplaces verwalten**

- **FR-020**: Einstellungen → Erweiterungen MUSS die Marketplace-Liste mit Name, Adresse, Ein/Aus und
  Rangfolge zeigen; Ein/Aus und Reihenfolge gelten sofort ohne Speichern-Schaltfläche (Spec 023).
- **FR-021**: Das Formular eines Marketplace (Name, Adresse, Auth-Art, Zugangsdaten-Eintrag) MUSS die
  Verbindung beim Bestätigen testen (Erreichbarkeit, Anmeldung) und das Ergebnis zeigen, bevor es speichert;
  der Nutzer kann auch bei einem Fehlschlag bewusst speichern.
- **FR-022**: Ein neuer Vault MUSS die Voreinstellung (öffentlicher Marketplace des Projekts) als
  gewöhnliche Zeile enthalten. Hat ein Vault keinen Marketplace, MUSS der Katalog „Öffentlichen
  Marketplace hinzufügen“ anbieten. HaexVault legt nie ohne Handlung des Nutzers eine Zeile an und
  schaltet nie eine wieder ein.
- **FR-023**: Jeder Marketplace MUSS angelegt, bearbeitet, ein-/ausgeschaltet, umsortiert und gelöscht
  werden können, auch die Voreinstellung. Das Löschen entfernt keine installierte Erweiterung.
- **FR-024**: Adressen MÜSSEN `https://` oder `http://` sein und dürfen einen Pfadanteil haben
  (`https://example.org/market`); `http://` ist erlaubt (selbst gehostete Marketplaces im lokalen Netz),
  das Formular weist darauf hin.
- **FR-025**: Die Marketplace-Liste MUSS Vault-Daten sein und über Spec 024 auf alle eigenen Geräte
  kommen (auch der Zustand Ein/Aus und die Reihenfolge).

**Zugangsdaten**

- **FR-026**: Auth-Arten sind „Keine“, „Bearer“ und „Basic“. Bei „Bearer“ und „Basic“ MUSS der
  Marketplace auf einen Zugangsdaten-Eintrag im Passwortmanager verweisen (Auswahl wie in Spec 052; neu
  eingegebene Werte werden als sichtbarer Eintrag dort abgelegt). Die Marketplace-Liste enthält nie
  Token oder Passwort.
- **FR-027**: Zugangsdaten DÜRFEN nur an `https://`-Adressen gesendet werden und nur an den Host des
  Marketplace, nie an eine Weiterleitung auf einen anderen Host und nie an die Adresse, von der eine
  Installationsdatei geladen wird, wenn sie auf einem anderen Host liegt.
- **FR-028**: Fehlen die Zugangsdaten (Eintrag gelöscht, nicht synchronisiert, unvollständig), MUSS die
  Quelle dies benennen, ohne andere Quellen zu stören.
- **FR-029**: Beim Löschen eines Passwort-Eintrags, den ein Marketplace nutzt, MUSS der Passwortmanager
  warnen und den Marketplace nennen (Spec 034 FR-034).

**Offenes Protokoll**

- **FR-034**: Das Marketplace-Protokoll MUSS als Vertrag in diesem Repository stehen
  ([contracts/marketplace-api.md](contracts/marketplace-api.md)), so dass jeder einen kompatiblen
  Marketplace betreiben kann. HaexVault DARF über einen Marketplace nichts voraussetzen außer diesem
  Protokoll: keine feste Adresse, keine Sonderbehandlung nach Name oder Adresse (außer der
  Voreinstellung als Eintrag), keine Felder, die nur ein Betreiber liefert.
- **FR-035**: Ein Marketplace, der weitere Felder liefert, DARF HaexVault nicht stören (unbekannte Felder
  werden ignoriert); ein Marketplace ohne optionale Felder MUSS nutzbar bleiben.

**Abgrenzung und Fehler**

- **FR-030**: Weder Erweiterungen noch Agenten (Spec 021, 018, 047) DÜRFEN den Katalog abfragen,
  Marketplaces verwalten oder eine Installation anstoßen. Die Installation geschieht nur durch eine
  bewusste Handlung des Nutzers in der Oberfläche.
- **FR-031**: Jede Quelle MUSS einzeln ausfallen dürfen: Fehlerzeile mit Ursache (nicht erreichbar,
  Zeitüberschreitung, Anmeldung abgelehnt, Antwort nicht verständlich, Zugangsdaten fehlen) und
  „Erneut versuchen“, ohne den Katalog der anderen Quellen zu stören.
- **FR-032**: Alle Texte MÜSSEN deutsch und englisch vorliegen, der Produktname ist „HaexVault“; die
  Oberfläche folgt dem Stil aus Spec 035/036 (Werkzeugleiste, große Überschrift, Gruppen und Zeilen).
- **FR-033**: Alle Funktionen MÜSSEN auf Linux, Windows, macOS, Android und iOS laufen; die Logik, die
  plattformabhängig scheint (Download, Pfade), MUSS auf Linux testbar sein.

### Key Entities

- **Marketplace**: Name, Adresse, Ein/Aus, Rangfolge, Auth-Art und Verweis auf einen
  Zugangsdaten-Eintrag (kein Geheimnis). Vault-Daten, synchronisiert.
- **Katalogeintrag**: flüchtig, nicht gespeichert; Angaben des Marketplace plus Quelle und lokaler
  Installationszustand.
- **Fassung**: eine Version einer Erweiterung im Katalog mit Download-Adresse, Prüfsumme und Größe.
- **Zugangsdaten-Eintrag**: Eintrag im Passwortmanager (Spec 034); der Marketplace verweist darauf.

## Success Criteria _(mandatory)_

### Measurable Outcomes

- **SC-001**: Ein Nutzer findet und installiert eine Erweiterung aus einem Marketplace in unter
  einer Minute vom Öffnen der App bis zur laufenden Erweiterung (ohne Dateien zu suchen).
- **SC-002**: In keinem Fall wird eine Erweiterung ohne die Bestätigung in der Installations-Vorschau
  installiert; Dateien mit falscher Prüfsumme, ungültiger Signatur oder über der Größengrenze werden zu
  100 % abgelehnt (automatisierter Test mit Test-Marketplace).
- **SC-003**: Fällt eine von mehreren Quellen aus, zeigt der Katalog die übrigen innerhalb derselben
  Zeit wie ohne den Ausfall plus höchstens der festen Frist für eine Quelle, und benennt die ausgefallene
  Quelle.
- **SC-004**: In der Vault-Datenbank, in Protokollen und in Synchronisationsdaten steht an keiner Stelle
  ein Token oder Passwort eines Marketplace (automatisierter Test).
- **SC-005**: Zugangsdaten verlassen das Gerät nie an einen anderen Host als den des Marketplace und nie
  über `http://` (automatisierter Test mit Weiterleitung und `http`-Adresse).
- **SC-006**: Ein neuer Vault enthält die Voreinstellung ohne Einrichtung; sie lässt sich löschen und kommt
  nicht von allein wieder; ein Vault ohne Marketplace bietet sie mit einem Tipp an, ohne dass Einträge
  doppelt entstehen.
- **SC-007**: Ein Marketplace, der auf Gerät A angelegt wurde, erscheint nach der Synchronisation auf
  Gerät B (E2E mit zwei Geräten).
- **SC-009**: Zwei Test-Marketplaces auf verschiedenen Adressen (einer mit Pfadanteil, einer mit
  Anmeldung) liefern gleichzeitig Einträge in einen Katalog; ohne eine Zeile Code, die eine der Adressen
  kennt (e2e, Rust-Test).
- **SC-008**: Der Ablauf Katalog → Installieren läuft im E2E auf Desktop und im Android-Emulator durch.

## Assumptions

- Der **öffentliche Marketplace des Projekts** liegt unter `https://marketplace.haex.space` (Betreiber,
  2026-10-29); diese Adresse ist die Voreinstellung und steht an einer einzigen Stelle im Code. Das Protokoll ist das, das
  haex-vault 2.x und das `@haex-space/marketplace-sdk` 0.1.6 benutzen: Liste mit Suche, Kategorie, Seite
  und Sortierung, Kategorien mit Anzahl, Details und Download-Adresse zu einer Fassung. Ab dieser Spec ist
  es ein Vertrag dieses Repositories (FR-034); der Betreiber-Dienst richtet sich danach. Im Listenaufruf
  fehlt der Herausgeberschlüssel (er steht nur in den Details); fehlende optionale Felder werden als „nicht
  geliefert“ behandelt, mit Ausnahme der Prüfsumme (FR-013).
- Die Antwortformate werden in einem Vertrag (Plan, `contracts/`) festgehalten und im Host gegen
  Testdaten geprüft; ob das bestehende `@haex-space/marketplace-sdk` nur als Typquelle dient oder in
  Rust nachgebaut wird, entscheidet der Plan.
- Die feste Frist pro Quelle liegt in der Größenordnung von zehn Sekunden; der Plan legt den Wert fest.
- Es gibt keinen Offline-Katalog und keinen Zwischenspeicher über das Schließen der App hinaus;
  Bewertungen und Rezensionen werden angezeigt, aber nicht abgegeben.
- Es gibt keine automatischen Updates und keine Hintergrundabfragen (siehe FR-017); das kann eine
  spätere Spec ergänzen, wenn der Nutzer es will.
- Nicht in dieser Spec: Veröffentlichen von Erweiterungen, bezahlte Erweiterungen, DID-Anmeldung,
  Rezensionen schreiben, Installation durch Agenten oder Erweiterungen.
- Der Entwicklermodus (Spec 017) und das Installieren aus einer Datei bleiben unverändert daneben
  bestehen.
