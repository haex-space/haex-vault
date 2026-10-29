# Feature Specification: Erweiterungs-Host für haextensions

**Feature Branch**: `017-extension-host`
**Created**: 2026-10-02
**Status**: Draft
**Input**: holzi soll haextensions so einbinden können, wie haex-vault 2.x (Legacy) es heute tut, aber auf
holzi angepasst: signierte Web-Bundles laufen in abgeschotteten iframes als Apps im Window
Manager (wm), bekommen Berechtigungen, die der Nutzer bei der Installation bestätigt und zur
Laufzeit erteilt oder verweigert, und dürfen SQL-Abfragen auf ihre eigenen Tabellen absetzen
(Drizzle über das vault-sdk), auf Tabellen anderer Erweiterungen nur mit Berechtigung. Dazu
kommen alle weiteren Host-Funktionen, die haex-vault 2.x Erweiterungen anbietet (Netzwerk,
Dateien, Benachrichtigungen, Passwörter, entfernter Speicher, Mail, Shell, Protokolle,
Schlüssel-Wert-Speicher). Die Entscheidungen von ADR-0004 gelten. Referenzen: haex-vault 2.x @
`8dce379d94e18fcd42c3b73686a06f984ca3f574` (`src-tauri/src/extension/`,
`src/composables/extensionMessageHandler.ts`, `src/composables/usePermissionPrompt.ts`,
`src/stores/extensions/broadcast.ts`), vault-sdk @
`502593e84b8d289b0986a2777754d6bd8f52da5e` (v3.7.0; `src/messages.ts`, `src/client/`,
`src/commands/`), haextension @ `db48f9a948522c18a00331aac232718825cc9317` (`apps/haex-notes`,
`apps/haex-calendar`, `apps/haex-pass`).

## Begriffe

- **Erweiterung**: eine haextension im Sinne von ADR-0004, ein signiertes Web-Bundle mit
  Manifest. Eine Erweiterung ist eindeutig durch **Herausgeberschlüssel** (der öffentliche
  Ed25519-Schlüssel im Manifest) und **Name**. Zwei Bundles mit demselben Paar sind Fassungen
  derselben Erweiterung.
- **Bundle**: die Installationsdatei (`.xt`) mit Manifest, Signatur und den Dateien
  der Erweiterung.
- **Manifest**: die Angaben der Erweiterung zu Name, Fassung, Herausgeberschlüssel, Signatur,
  Einstiegsdatei, Symbol, Beschreibung, Darstellung (eine oder mehrere Instanzen),
  Migrationen und den **erklärten Berechtigungen**.
- **Eigene Tabellen**: die Tabellen mit dem Präfix der Erweiterung,
  `<Herausgeberschlüssel>__<Name>__`. Das Präfix ist dasselbe wie in haex-vault 2.x
  (`src-tauri/src/extension/utils.rs`, `get_extension_table_prefix`) und im vault-sdk
  (`src/types.ts`, `getTableName`) und wird schon von Spec 028 vorausgesetzt.
- **Kerntabellen**: alle Tabellen, die keine Tabellen einer Erweiterung sind: die Tabellen von
  holzi selbst (Chat, Einstellungen, Geräte, Passwortmanager und weitere), die Tabellen von
  haex-crdt und von SQLite.
- **Host-Funktion**: eine Operation, die holzi einer Erweiterung anbietet (SQL, Dateien,
  Netzwerk und so weiter), entsprechend der Richtung B in ADR-0004.
- **Prüfstelle**: die eine Stelle in holzi, die jede Anfrage einer Erweiterung entgegennimmt,
  die Erweiterung erkennt, die Berechtigung prüft und die Host-Funktion ausführt (ADR-0004).
- **Berechtigung**: die Erlaubnis für eine Erweiterung, eine Art Host-Funktion auf einem Ziel
  auszuführen (etwa „Lesen“ auf den Tabellen einer anderen Erweiterung, „Lesen und Schreiben“
  auf einem Ordner, Anfragen an `https://example.org/*`). Spec 034 nennt sie „Freigabe“. Eine
  Berechtigung hat einen Zustand: **erteilt**, **verweigert** oder **fragen**. Eine
  **gemerkte** Berechtigung bleibt bestehen; eine **vorläufige** gilt nur, bis die Vault
  geschlossen wird. Eine gemerkte Berechtigung gilt entweder **vault-weit** (auf allen eigenen
  Geräten) oder **geräteeigen** (nur auf dem Gerät, auf dem sie erteilt wurde). Die Shell ist
  die einzige **gerätebezogene Art**: Eine gemerkte Shell-Berechtigung gilt immer geräteeigen,
  jede andere immer vault-weit. Der Nutzer wählt das nicht.
- **Erweiterungsrahmen**: das abgeschottete iframe, in dem eine Erweiterung in einem Tab läuft,
  und der Kanal, über den es mit holzi spricht. holzi erzeugt beide und weiß daher, welcher
  Erweiterung ein Rahmen gehört.
- **Entwicklermodus**: das Laden einer noch nicht signierten Erweiterung von einem lokalen
  Entwicklungsserver.

## Beziehung zu bestehenden Specs

- [ADR-0004](../../docs/adr/0004-extension-protocol-split.md): Diese Spec ist die dort
  geplante Spec 017, die Richtung B (Erweiterung → holzi) mit der einen Prüfstelle. Sie
  übernimmt das Protokoll des vault-sdk unverändert, damit dieselben Erweiterungen in
  haex-vault 2.x und holzi laufen. Die Richtung A (Werkzeuge einer Erweiterung für den Agenten
  über MCP) ist Spec 018, das Ergänzen von Werkzeugen in den bestehenden haextensions ist
  Spec 019.
- [ADR-0008](../../docs/adr/0008-extension-bundle-signature-and-sql-authorizer.md): hält das
  Bundle-Format `haextension-bundle/2` mit Signatur je Datei und die zwei unabhängigen Prüfungen
  des SQL von Erweiterungen (Vorprüfung und SQLite-Authorizer) fest, die über holzi hinaus in
  vault-sdk und haex-crdt wirken.
- [`015-workspace-shell`](../015-workspace-shell/spec.md) und
  [`030-app-multi-instance`](../030-app-multi-instance/spec.md): Eine Erweiterung ist eine App
  im wm wie Chat und Einstellungen und nutzt die Tab-Schnittstelle aus
  `contracts/shell-app-contract.md` unverändert. Die App-Kennungen `extension.*` sind dort
  schon vorgesehen. Ob eine Erweiterung mehrere Instanzen hat, sagt ihr Manifest.
- [`020-tab-navigation`](../020-tab-navigation/spec.md): FR-034 und FR-035 dort gelten für
  jeden Erweiterungsrahmen. Orte einer Erweiterung kommen über die Brücke in den Verlauf des
  Tabs (dort Research R17), nicht über die Browser-Historie.
- [`022-session-restore`](../022-session-restore/spec.md): Ein Tab mit einer Erweiterung kommt
  mit Ort und Verlauf zurück, wenn die Erweiterung noch installiert und aktiv ist.
- [`023-settings-app`](../023-settings-app/spec.md): Die Verwaltung der Erweiterungen und
  ihrer Berechtigungen ist eine Kategorie „Erweiterungen“ in der Einstellungs-App. Auswahlen
  werden sofort gespeichert, ohne Knopf zum Übernehmen.
- [`024-own-device-sync`](../024-own-device-sync/spec.md): Bundles, Registrierung,
  gemerkte Berechtigungen und die synchronisierten eigenen Tabellen sind Vault-Daten und
  gehen über den Sync der eigenen Geräte. Diese Spec baut keinen eigenen Sync.
  Geräteeigene Daten (angewendete Migrationen, vorläufige Berechtigungen,
  Schlüssel-Wert-Speicher, Protokolle) folgen [ADR-0001](../../docs/adr/0001-device-scoped-data-convention.md).
- [`057-extension-peer-channels`](../057-extension-peer-channels/spec.md): Kanäle zwischen
  derselben Erweiterung auf Geräten anderer Vaults. Die Berechtigung `peers`, acht Methoden
  `extension_peers_*` und fünf Meldungen `peers:*` kommen über diese Brücke
  ([contracts/bridge.md](contracts/bridge.md)); der Rest ist dort geregelt.
- [`025-own-device-file-sync`](../025-own-device-file-sync/spec.md): Die Dateifunktionen dieser
  Spec arbeiten auf dem Dateisystem des Geräts. Ob ein Ordner synchronisiert wird, ändert an
  den Berechtigungen nichts.
- [`027-spaces`](../027-spaces/spec.md) und [`028-data-shares`](../028-data-shares/spec.md):
  In holzi sind Spaces Netzwerkordner nur für Dateien, und SQLite-Daten teilt man über
  Datenfreigaben. Die Space-Funktionen des vault-sdk, die Datenzeilen einem Space zuordnen,
  bildet diese Spec deshalb bewusst nicht nach (FR-061). Spec 028 baut auf dieser Spec auf:
  Sie ergänzt das Manifest um Freigabetypen und die Brücke um zwei Anfragen.
- [`038-storage-connections`](../038-storage-connections/spec.md): Die Funktionen für entfernten
  Speicher nutzen die Speicher aus 038, mit einer Berechtigung `remoteStorage` je Speicher
  (`backendId`) oder für `*`. Zugangsdaten gibt nur der Nutzer in holzi ein, keine Erweiterung
  bekommt oder schickt sie. [`056-file-access-sharing`](../056-file-access-sharing/spec.md) baut für
  Dateifreigaben auf denselben Speicherverbindungen auf (Review 2026-10-06).
- [`032-model-operates-holzi`](../032-model-operates-holzi/spec.md): Kein Weg aus einer
  Erweiterung erreicht den eingebauten Agenten oder ein Modell (ADR-0004, FR-009).
- [`034-password-manager`](../034-password-manager/spec.md): Diese Spec erteilt, speichert,
  zeigt und widerruft die Freigaben für Passwörter, die 034 definiert (dort FR-030). Die
  Passwort-Funktionen einer Erweiterung gehen ausschließlich über die Zugriffsprüfung des
  Passwortmanagers mit dem Aufrufer „Erweiterung“. Die Tabellen des Passwortmanagers sind
  Kerntabellen und per SQL nie erreichbar (034 FR-024).
- Geplante Spec **021** (MCP-Server, Freigaben je Agent): eigener Freigabeweg für Agenten.
  Diese Spec gibt Agenten keinen Weg, Berechtigungen von Erweiterungen zu ändern.

## Clarifications

### Session 2026-10-02

- Q: Welche Host-Funktionen deckt die Spec neben SQL ab? → A: Alle, die haex-vault 2.x
  Erweiterungen anbietet: Datenbank, Kontext, Schlüssel-Wert-Speicher, Dateisystem, Netzwerk,
  Benachrichtigungen, Passwörter, entfernter Speicher, Mail, Shell und Protokolle. Bewusst
  ausgenommen sind die Space-Zuordnung von Datenzeilen (holzi teilt Daten über 028, nicht
  über Spaces), die External Bridge (034) und Funktionen, die auch haex-vault 2.x Erweiterungen
  nicht zugänglich macht (LocalSend, Identitäten, Lesezeichen, Sync-Server).
- Q: Wie verhalten sich Erweiterungen über mehrere eigene Geräte? → A: Eine Installation gilt
  für die Vault. Bundle, Registrierung und gemerkte Berechtigungen sind Vault-Daten und gehen
  per Sync auf alle eigenen Geräte. Jedes Gerät prüft die Signatur selbst und legt die
  Tabellen der Erweiterung an, bevor es Daten für sie übernimmt. Der Sync anderer Daten
  bleibt dabei nie hängen.
- Q: Auf welche Tabellen darf eine Erweiterung per SQL zugreifen? → A: Auf eigene Tabellen
  immer, auf Tabellen anderer Erweiterungen nur mit Berechtigung („Lesen“ oder „Lesen und
  Schreiben“), auf Kerntabellen nie. Kerndaten erreicht sie nur über typisierte
  Host-Funktionen wie die Passwort-Funktionen aus 034.
- Q: Gelten gemerkte Berechtigungen für Shell und Dateisystem auch auf allen eigenen Geräten?
  → A: Überholt durch die Klärung vom 2026-10-06 (unten). Ursprünglich: standardmäßig nur auf
  dem Gerät, auf dem sie erteilt wurden, mit der Wahl „für alle Geräte merken“.
- Q: Übernimmt holzi das Signaturformat von haex-vault 2.x? → A: Nein. haex-vault 2.x signiert nur die
  aneinandergehängten Dateiinhalte, ohne Pfade und Dateigrenzen; Inhalte lassen sich so ohne
  Schlüssel zwischen Dateien verschieben, Dateien umbenennen oder leere Dateien ergänzen. holzi
  verlangt ein eigenes, strenges Format, in dem Pfad, Länge und Inhalt jeder Datei sowie das
  kanonische Manifest signiert sind. Das Werkzeug `haex` im vault-sdk bekommt dieses Format;
  Bundles im alten Format lehnt holzi ab, auch im Entwicklermodus gibt es dafür keine
  Ausnahme (dort wird ohnehin nicht signiert).
- Q: Wo werden die Schwachstellen behoben, die die Analyse von haex-vault 2.x gefunden hat? → A:
  Nur in holzi. holzi löst haex-vault 2.x ab; haex-vault 2.x wird dafür nicht geändert. Jede gefundene
  Schwachstelle ist in holzi von Anfang an ausgeschlossen und hat einen Test (SC-002), bevor
  eine Lieferung von 017 als fertig gilt.
- Q: (Planung) Wie wird festgehalten, welche Migrationen auf einem Gerät angewendet sind? → A: In
  einer `_no_sync`-Tabelle, die den Zustand der Vault-Datei beschreibt (wie das Migrationsjournal von
  haex-crdt), nicht nach ADR-0001: Eine kopierte Vault-Datei trägt ihr Schema mit (Plan, R8).
- Q: (Planung) Wo liegt der Schlüssel-Wert-Speicher? → A: Als gerätebezogene Zeile in der Vault nach
  ADR-0001; er gilt nur für das Gerät, das ihn geschrieben hat (Plan, R20).
- Q: (Planung) Laufen Erweiterungen auf Android und iOS? → A: Ja. holzi bekommt Android- und iOS-Ziele, und
  Erweiterungen müssen dort laufen. Auf Android gibt Tauri die IPC heute jedem Rahmen; das wird in wry behoben,
  bevor Erweiterungen dort laufen (Plan, R25).
- Q: (Planung) Was passiert mit einem Rahmen, wenn sein Tab in ein anderes Fenster wandert oder der
  Arbeitsbereich wechselt? → A: Er lädt neu und kehrt an seinen Ort zurück; ungespeicherter Zustand im
  Rahmen geht dabei verloren (Plan, R17).

### Session 2026-10-06

- Q: Für welche Geräte gilt eine gemerkte Dateisystem-Berechtigung, und darf der Nutzer das
  wählen? → A: Für alle eigenen Geräte, ohne Wahl. Nur die Shell bleibt geräteeigen, ebenfalls
  ohne Wahl: Programme unterscheiden sich je Gerät, und Ausführen ist das größte Risiko. Die
  Dialoge sagen, wo eine Berechtigung gilt, statt danach zu fragen (Betreiber, bei der Prüfung
  von haex-notes).
- Q: Was passiert, wenn ein Bundle dieselbe Version wie ein vorhandenes hat, aber anderen
  Inhalt? → A: holzi warnt im Installationsdialog und ersetzt das vorhandene nur, wenn der
  Nutzer das ausdrücklich bestätigt; das ersetzte Bundle wird zurückgezogen. Ohne Bestätigung
  installiert holzi nicht (Betreiber, nachdem bei der Prüfung von haex-notes ein neu gebautes
  0.1.7 neben dem alten lag und das alte lief).

## User Scenarios & Testing _(mandatory)_

### User Story 1 - Eine Erweiterung installieren und als App öffnen (Priority: P1)

Ein Nutzer hat eine Datei `haex-notes.xt`. Er öffnet in den Einstellungen die
Kategorie „Erweiterungen“ und wählt „Aus Datei installieren“. holzi zeigt ihm Name, Fassung,
Beschreibung und Herausgeber (eine kurze, gut vergleichbare Form des Herausgeberschlüssels),
ob die Signatur gültig ist, und alle Berechtigungen, die die Erweiterung erklärt. Er kann
einzelne Berechtigungen vor der Installation abwählen. Nach dem Bestätigen erscheint die
Erweiterung im Launcher und im „+“-Menü. Er öffnet sie, sie läuft in einem Tab wie jede andere
App, und ihre Orte erscheinen im Verlauf des Tabs.

**Why this priority**: Ohne Installation und Einbettung gibt es keine Erweiterung, und
alle anderen Stories bauen darauf auf.

**Independent Test**: Ein signiertes Test-Bundle installieren, das nur seinen Namen und das
Theme anzeigt. Es im Launcher finden, öffnen, in einen zweiten Tab ziehen und neu starten.
Ein manipuliertes Bundle (eine Datei geändert) wird abgelehnt.

**Acceptance Scenarios**:

1. **Given** ein Bundle mit gültiger Signatur, **When** der Nutzer die Installation
   bestätigt, **Then** erscheint die Erweiterung im Launcher und im „+“-Menü und öffnet sich
   als Tab.
2. **Given** ein Bundle, bei dem eine Datei oder das Manifest nach dem Signieren verändert
   wurde, **When** der Nutzer es installieren will, **Then** lehnt holzi es mit einem
   verständlichen Grund ab und installiert nichts.
3. **Given** der Installationsdialog, **When** er erscheint, **Then** zeigt er jede erklärte
   Berechtigung aller Arten, und jede lässt sich abwählen; eine abgewählte Berechtigung steht
   danach auf „fragen“.
4. **Given** eine offene Erweiterung, **When** sie ihren Ort ändert, **Then** steht der neue
   Ort im Verlauf des Tabs, und Zurück führt dorthin zurück, ohne dass die Erweiterung den Tab
   oder ein anderes Fenster von holzi steuern kann.
5. **Given** eine Erweiterung mit „eine Instanz“ im Manifest, **When** der Nutzer sie ein
   zweites Mal öffnet, **Then** springt holzi zum vorhandenen Tab.
6. **Given** ein offener Tab mit einer Erweiterung, **When** holzi neu startet und die
   Sitzung wiederhergestellt wird, **Then** öffnet sich der Tab wieder an seinem Ort.
7. **Given** eine installierte Erweiterung, **When** der Nutzer ein Bundle mit demselben
   Herausgeberschlüssel und Namen, aber einer anderen Fassung installiert, **Then** gilt das
   als Update dieser Erweiterung (US7), nicht als zweite Erweiterung.
8. **Given** der Installationsdialog einer Erweiterung mit vielen Berechtigungen, **When** er
   erscheint, **Then** stehen die Berechtigungen nach Art gruppiert da (die Werkzeuge für den
   Agenten als eigene Gruppe), jede Gruppe als Ziehharmonika, die der Nutzer aufklappt, um ihre
   einzelnen Berechtigungen zu sehen und abzuwählen; eine Gruppe, die eine weitreichende
   Berechtigung enthält (ein Ziel `*` oder die Web-Aktion `*`), trägt ein Warndreieck, auch
   zugeklappt.

---

### User Story 2 - Eigene Daten per SQL speichern und lesen (Priority: P1)

Die Erweiterung haex-notes legt beim ersten Start ihre Tabellen über ihre Migrationen an und
speichert danach Notizbücher und Seiten mit Drizzle. Sie liest, ändert und löscht ihre
eigenen Daten ohne jede Rückfrage. Was sie schreibt, behandelt holzi wie alle anderen
Vault-Daten: Es ist verschlüsselt gespeichert, mit Zeitstempeln für den Sync versehen, und
Löschungen hinterlassen eine Löschmarke.

**Why this priority**: SQL auf eigene Tabellen ist der Kern dessen, was haextensions heute
tun. Ohne ihn läuft keine der bestehenden Erweiterungen.

**Independent Test**: Ein Test-Bundle mit zwei Migrationen installieren. Prüfen, dass seine
Tabellen mit Präfix angelegt sind, dass es Zeilen einfügt, mit RETURNING zurückbekommt,
ändert, liest und löscht, dass eine Transaktion aus drei Anweisungen ganz oder gar nicht
wirkt und dass ein zweiter Start die Migrationen nicht erneut anwendet.

**Acceptance Scenarios**:

1. **Given** eine frisch installierte Erweiterung mit Migrationen, **When** sie startet,
   **Then** wendet holzi die noch nicht angewendeten Migrationen in ihrer Reihenfolge an,
   und die Erweiterung sieht danach ihre Tabellen.
2. **Given** eine Erweiterung mit eigenen Tabellen, **When** sie Zeilen einfügt, ändert,
   liest oder löscht, **Then** gelingt das ohne Rückfrage, und das Ergebnis hat die Form, die
   Drizzle über das vault-sdk erwartet.
3. **Given** eine Transaktion aus mehreren Anweisungen, **When** eine davon scheitert,
   **Then** wirkt keine, und die Erweiterung erhält den Fehler der scheiternden Anweisung.
4. **Given** eine Migration, die eine Kerntabelle oder eine Tabelle einer anderen Erweiterung
   anlegt, ändert, liest oder beschreibt, oder die eine Sicht, einen Trigger, eine weitere
   Datenbank oder eine nicht erlaubte PRAGMA-Anweisung enthält, **When** holzi sie anwenden
   soll, **Then** lehnt holzi die ganze Migration ab, wendet nichts davon an und startet die
   Erweiterung nicht, bis eine korrigierte Fassung kommt.
5. **Given** eine Erweiterung, **When** sie zur Laufzeit außerhalb einer Migration eine
   Tabelle anlegen, ändern oder löschen will, **Then** wird das abgelehnt.
6. **Given** eine Erweiterung, die eine Zeile in einer synchronisierten eigenen Tabelle
   löscht, **When** der Sync läuft, **Then** ist die Zeile auch auf den anderen eigenen
   Geräten gelöscht. Eine Zeile in einer eigenen Tabelle mit der Endung `_no_sync` bleibt
   auf dem Gerät und hinterlässt keine Löschmarke.

---

### User Story 3 - Berechtigungen zur Laufzeit erteilen, merken und verwalten (Priority: P1)

Die Erweiterung haex-calendar will einen Termin an einen CalDAV-Server senden, und ihre
Berechtigung für diese Adresse steht auf „fragen“. holzi zeigt eine Anfrage: welche
Erweiterung, welche Art Zugriff, welches Ziel und ob die Erweiterung das in ihrem Manifest
erklärt hat. Der Nutzer erlaubt und setzt „Merken“. Später öffnet er in den Einstellungen die
Erweiterung, sieht alle Berechtigungen mit Zustand und ändert eine auf „verweigert“.

**Why this priority**: Das Berechtigungsmodell ist die Sicherheitsgrenze zwischen fremdem
Code und den Daten des Nutzers. ADR-0004 verlangt es für jede Host-Funktion.

**Independent Test**: Ein Test-Bundle fragt eine Berechtigung an, die auf „fragen“ steht.
Einmal ohne „Merken“ erlauben (gilt bis zum Schließen der Vault), einmal mit „Merken“ (gilt
nach einem Neustart weiter), einmal verweigern (die Anfrage scheitert). Danach in den
Einstellungen widerrufen und prüfen, dass die nächste Anfrage wieder fragt.

**Acceptance Scenarios**:

1. **Given** eine erteilte Berechtigung, **When** die Erweiterung die passende Anfrage stellt,
   **Then** führt holzi sie ohne Rückfrage aus.
2. **Given** eine Berechtigung auf „fragen“ oder keine passende Berechtigung, **When** die
   Erweiterung anfragt, **Then** zeigt holzi eine Anfrage mit Erweiterung, Art, Ziel und dem
   Hinweis, ob die Erweiterung das erklärt hat; die Erweiterung wartet auf die Entscheidung
   und erfährt sie danach.
3. **Given** eine Anfrage, **When** der Nutzer mit „Merken“ entscheidet, **Then** gilt die
   Entscheidung dauerhaft, bei der Shell nur auf diesem Gerät, bei allen anderen Arten auf
   allen eigenen Geräten; ohne „Merken“ gilt sie nur auf diesem Gerät bis zum Schließen der
   Vault.
4. **Given** eine Anfrage, **When** „Merken“ gewählt ist, **Then** sagt die Anfrage, wo die
   Entscheidung gelten wird (bei der Shell „nur auf diesem Gerät“, sonst „auf allen deinen
   Geräten“), und bietet keine Wahl des Geltungsbereichs an.
5. **Given** eine verweigerte Berechtigung und eine erteilte, die beide passen, **When** die
   Erweiterung anfragt, **Then** gilt die Verweigerung.
6. **Given** mehrere gleichartige Anfragen kurz hintereinander, **When** holzi sie zeigt,
   **Then** sieht der Nutzer eine Anfrage, und seine Entscheidung gilt für alle wartenden
   gleichen Anfragen; verschiedene Anfragen stehen nacheinander in einer Warteschlange.
7. **Given** die Einstellungen einer Erweiterung, **When** der Nutzer eine Berechtigung
   ändert oder widerruft, **Then** gilt das sofort, auch für bereits offene Tabs der
   Erweiterung.
8. **Given** ein Update, das neue Berechtigungen erklärt, **When** der Nutzer es installiert,
   **Then** muss er die neuen Berechtigungen bestätigen; die bisherigen Entscheidungen
   bleiben, außer für erklärte Berechtigungen, die das Update nicht mehr erklärt; diese
   entfallen.
9. **Given** eine Erweiterung, **When** sie versucht, sich selbst eine Berechtigung zu geben,
   eine Anfrage selbst zu beantworten oder ihre Grenzwerte zu ändern, **Then** gibt es dafür
   keine Host-Funktion, und der Versuch scheitert.

---

### User Story 4 - Dieselbe Erweiterung auf allen eigenen Geräten (Priority: P1)

Ein Nutzer installiert haex-calendar auf dem Laptop. Nach dem nächsten Sync steht die
Erweiterung auch auf dem Desktop und dem Telefon im Launcher, mit denselben Terminen und
denselben gemerkten Berechtigungen. Er muss das Bundle dort nicht noch einmal installieren.
Löscht er auf dem Telefon einen Termin, ist er nach dem Sync auch auf dem Laptop weg.

**Why this priority**: holzi ist auf mehrere eigene Geräte ausgelegt (Spec 024), und Spec
028 setzt synchronisierte Tabellen von Erweiterungen voraus. Ohne diese Story würde der Sync
auf einem Gerät ohne die Erweiterung an deren Tabellen hängen bleiben.

**Independent Test**: Mit dem Mehrgeräte-Aufbau aus Spec 033 eine Erweiterung auf Gerät A
installieren, auf Gerät B nach dem Sync öffnen, auf beiden Seiten Zeilen schreiben und
vergleichen. Danach auf A ein Update installieren und prüfen, dass B es übernimmt.

**Acceptance Scenarios**:

1. **Given** eine Erweiterung, die auf Gerät A installiert wird, **When** Gerät B die
   Änderungen bekommt, **Then** prüft B die Signatur des Bundles selbst und bietet die
   Erweiterung danach im Launcher an, ohne weitere Installation.
2. **Given** Zeilen einer Erweiterung, die bei Gerät B ankommen, bevor B deren Tabellen
   angelegt hat, **When** B sie übernimmt, **Then** legt B zuerst die Tabellen über die
   Migrationen an. Daten anderer Erweiterungen und Kerndaten übernimmt B unabhängig davon
   weiter.
3. **Given** ein Bundle, das bei Gerät B ankommt, dessen Signatur dort aber nicht stimmt,
   **When** B es prüft, **Then** startet B die Erweiterung nicht, zeigt dem Nutzer den Grund
   und übernimmt alle anderen Daten weiter.
4. **Given** ein Update auf Gerät A, **When** Gerät B die Änderungen bekommt, **Then** läuft
   die Erweiterung auf B in der neuen Fassung, nachdem B deren neue Migrationen angewendet
   hat. Ein Tab, der auf B noch offen ist, lädt neu oder sagt, dass er neu laden muss.
5. **Given** eine vault-weit gemerkte Berechtigung, die auf Gerät A erteilt wurde, **When**
   die Erweiterung auf Gerät B dieselbe Anfrage stellt, **Then** gilt die Berechtigung auch
   dort. Eine vault-weite Berechtigung für Dateien gilt für einen Pfad, den es auf B nicht
   geben muss (FR-047).
6. **Given** eine geräteeigene Berechtigung für die Shell auf Gerät A, **When**
   die Erweiterung auf Gerät B dieselbe Anfrage stellt, **Then** fragt holzi auf B erneut.
7. **Given** eine Erweiterung, die auf einem Gerät deinstalliert wird, **When** die anderen
   Geräte die Änderungen bekommen, **Then** ist sie auch dort deinstalliert, mit oder ohne
   ihre Daten, wie der Nutzer gewählt hat (US7).

---

### User Story 5 - Ansichten bleiben aktuell, und die Erweiterung passt zu holzi (Priority: P2)

Der Nutzer hat haex-notes auf zwei Geräten offen. Er ändert auf dem einen eine Seite, und die
Erweiterung auf dem anderen zeigt die Änderung, ohne dass er neu lädt. Er schaltet in holzi auf
dunkles Theme oder eine andere Sprache um, und die Erweiterung folgt. Sie merkt sich
Ansichtseinstellungen in ihrem Schlüssel-Wert-Speicher, die auch nach einem Neustart noch da
sind.

**Why this priority**: Ohne Live-Aktualisierung sehen Nutzer veraltete Daten (wie vor PR
#183 in holzi selbst). Theme und Sprache gehören zum Gesamteindruck. Beides ist wichtig, aber
keine Grundfunktion.

**Independent Test**: Ein Test-Bundle meldet jede erhaltene Änderungsmeldung. Ändern auf
einem zweiten Gerät, in einem zweiten Tab, durch eine andere Erweiterung mit Lesezugriff auf
diese Tabellen und durch eine Erweiterung ohne Lesezugriff. Nur die ersten drei erreichen die
Erweiterung. Theme umschalten, Wert speichern, neu starten, Wert lesen.

**Acceptance Scenarios**:

1. **Given** eine offene Erweiterung, **When** sich eine ihrer eigenen Tabellen ändert
   (lokal, auf einem anderen Gerät, durch ihren zweiten Tab), **Then** erhält sie eine
   Meldung mit den geänderten Tabellen.
2. **Given** eine Erweiterung, **When** sich eine Tabelle ändert, die sie nicht lesen darf,
   **Then** erfährt sie davon nichts, auch nicht den Namen der Tabelle.
3. **Given** eine offene Erweiterung, **When** der Nutzer Theme oder Sprache ändert, **Then**
   erhält sie den neuen Kontext (Theme, Sprache, Plattform, Gerätekennung).
4. **Given** ein Wert im Schlüssel-Wert-Speicher, **When** der Tab geschlossen und neu
   geöffnet wird oder holzi neu startet, **Then** liest die Erweiterung denselben Wert.
   Andere Erweiterungen sehen ihn nicht.
5. **Given** eine Erweiterung, **When** sie den Kontext von holzi ändern will (Theme,
   Sprache), **Then** gibt es dafür keine Host-Funktion.

---

### User Story 6 - Daten einer anderen Erweiterung mitnutzen (Priority: P2)

Eine Erweiterung „Wochenübersicht“ zeigt Termine aus haex-calendar und Aufgaben aus einer
Aufgaben-Erweiterung. Sie erklärt in ihrem Manifest, dass sie die Tabellen von haex-calendar
lesen will. Der Nutzer bestätigt das bei der Installation, und die Wochenübersicht liest die
Termine per SQL, kann sie aber nicht ändern.

**Why this priority**: haex-vault 2.x erlaubt das, und es macht Erweiterungen kombinierbar. Für
die erste Erweiterung braucht es niemand, deshalb P2.

**Independent Test**: Zwei Test-Bundles installieren. Das zweite liest mit Berechtigung
„Lesen“ eine Tabelle des ersten, will schreiben (abgelehnt), liest eine Tabelle ohne
Berechtigung (Anfrage) und eine Kerntabelle (abgelehnt, ohne Anfrage).

**Acceptance Scenarios**:

1. **Given** eine Berechtigung „Lesen“ auf die Tabellen einer anderen Erweiterung, **When**
   die Erweiterung sie per SQL liest, auch in Joins mit eigenen Tabellen, **Then** gelingt
   das.
2. **Given** dieselbe Berechtigung, **When** sie in diese Tabellen schreiben will, **Then**
   fragt holzi nach „Lesen und Schreiben“; ohne Zustimmung scheitert die Anfrage.
3. **Given** eine beliebige Berechtigung, **When** die Erweiterung eine Tabelle einer anderen
   Erweiterung anlegen, ändern oder löschen will, **Then** wird das abgelehnt.
4. **Given** eine Anfrage, die eine Kerntabelle liest oder schreibt, auf welchem Weg auch
   immer (direkt, in einer Unterabfrage, in einem `WITH`-Ausdruck, mit Schemaangabe, in
   `EXISTS`, in einem Join), **When** holzi sie prüft, **Then** lehnt holzi sie ab, ohne den
   Nutzer zu fragen. Keine Berechtigung, auch keine vorläufige, kann das ändern.
5. **Given** die Erweiterung, deren Tabellen gelesen werden, wird deinstalliert, **When** die
   lesende Erweiterung sie abfragt, **Then** erhält sie einen Fehler wie bei einer nicht
   vorhandenen Tabelle.

---

### User Story 7 - Erweiterungen aktualisieren, deaktivieren und entfernen (Priority: P2)

Der Nutzer installiert eine neue Fassung von haex-notes aus einer Datei. holzi zeigt, was neu
ist, und fragt nur nach den neu erklärten Berechtigungen. Eine Erweiterung, die er gerade
nicht braucht, deaktiviert er; sie verschwindet aus dem Launcher, ihre Daten bleiben. Eine
andere entfernt er und wählt, ob ihre Daten mitgehen.

**Why this priority**: Ohne Update und Entfernen sammelt eine Vault alte Fassungen und
Daten an. Für den ersten Test reicht die Installation.

**Independent Test**: Fassung 1 installieren, Daten schreiben, Fassung 2 mit einer weiteren
Migration und einer neuen Berechtigung installieren, Daten prüfen, deaktivieren, aktivieren,
deinstallieren mit „Daten behalten“, neu installieren (Daten sind wieder da), deinstallieren
mit „Daten löschen“.

**Acceptance Scenarios**:

1. **Given** eine installierte Erweiterung, **When** der Nutzer ein Bundle mit demselben
   Herausgeberschlüssel und Namen installiert, **Then** ersetzt es die bisherige Fassung,
   ihre neuen Migrationen werden angewendet, und die Daten bleiben.
2. **Given** ein Bundle mit demselben Namen, aber einem anderen Herausgeberschlüssel, **When**
   der Nutzer es installiert, **Then** ist es eine andere Erweiterung mit eigenem Präfix und
   ohne Zugriff auf die Daten der ersten. holzi weist auf den gleichen Namen hin.
3. **Given** ein Bundle mit einer älteren Fassung als der installierten, **When** der Nutzer
   es installieren will, **Then** warnt holzi und installiert nur nach ausdrücklicher
   Bestätigung. Bereits angewendete Migrationen werden nicht zurückgenommen.
   Ein Bundle derselben Version mit anderem Inhalt behandelt holzi ebenso: Warnung, und erst
   nach Bestätigung ersetzt es das vorhandene auf allen Geräten.
4. **Given** eine deaktivierte Erweiterung, **When** irgendein Tab, eine alte Seite oder ein
   anderer Weg eine Host-Funktion für sie aufruft, **Then** lehnt holzi ab. Sie erscheint
   nicht im Launcher, ihre Tabs schließen sich, und ihre Daten bleiben.
5. **Given** das Entfernen mit „Daten löschen“, **When** es ausgeführt ist, **Then** sind ihre
   eigenen Tabellen, ihre Berechtigungen, ihr Schlüssel-Wert-Speicher und ihre Protokolle
   auf allen eigenen Geräten entfernt; Berechtigungen anderer Erweiterungen auf ihre
   Tabellen sind gegenstandslos.
6. **Given** das Entfernen mit „Daten behalten“, **When** es ausgeführt ist, **Then** bleiben
   ihre Tabellen, und eine spätere Installation derselben Erweiterung findet sie wieder.
   Die Einstellungen zeigen behaltene Daten entfernter Erweiterungen mit ihrer Größe und
   bieten an, sie zu löschen.

---

### User Story 8 - Netzwerk und Benachrichtigungen (Priority: P2)

haex-calendar spricht mit einem CalDAV-Server und erinnert an Termine mit einer
Systembenachrichtigung. Eine andere Erweiterung lädt Symbole von einem Symboldienst und öffnet
eine Adresse im Browser. Jeder Netzzugriff einer Erweiterung geht über holzi und braucht eine
Berechtigung für die Zieladresse.

**Why this priority**: Kalender und Passwortmanager brauchen das. Die Kernfunktion SQL
kommt ohne Netz aus.

**Independent Test**: Ein Test-Bundle sendet Anfragen an einen lokalen Testserver: mit
Berechtigung (gelingt), ohne (Anfrage), direkt am Host vorbei (scheitert immer). Es zeigt
eine Benachrichtigung und erhält das Klicken darauf.

**Acceptance Scenarios**:

1. **Given** eine Berechtigung für `https://dav.example.org/*`, **When** die Erweiterung dort
   eine Anfrage stellt, **Then** führt holzi sie aus und gibt Status, Kopfzeilen und Inhalt
   zurück.
2. **Given** keine passende Berechtigung, **When** die Erweiterung eine Adresse anfragt,
   **Then** fragt holzi den Nutzer.
3. **Given** eine Erweiterung, **When** sie am Host vorbei eine Netzverbindung öffnen will
   (Abruf aus dem iframe, Bild, Formular, WebSocket, eingebettete Seite), **Then** scheitert
   das immer, unabhängig von Berechtigungen.
4. **Given** eine Erweiterung, **When** sie eine Adresse im Browser öffnen will, **Then**
   öffnet holzi sie im Standardbrowser des Systems, nach der Berechtigung für diese Adresse.
5. **Given** eine Berechtigung für Benachrichtigungen, **When** die Erweiterung eine zeigt
   und der Nutzer darauf klickt, **Then** erhält die Erweiterung die Meldung, und holzi
   bringt ihren Tab nach vorn.

---

### User Story 9 - Dateien des Geräts lesen und schreiben (Priority: P2)

Eine Erweiterung speichert einen Anhang als Datei, die der Nutzer im Speichern-Dialog auswählt. Eine
Bildbetrachter-Erweiterung liest einen Ordner, den der Nutzer ihr freigegeben hat, und wird
benachrichtigt, wenn darin eine Datei hinzukommt.

**Why this priority**: Viele Erweiterungen brauchen Dateien. Der Zugriff auf das Dateisystem
ist aber heikel und für SQL-Erweiterungen nicht nötig.

**Independent Test**: Ein Test-Bundle speichert über den Dialog (keine weitere Rückfrage),
liest in einem freigegebenen Ordner (gelingt), außerhalb (Anfrage), versucht über `..` aus
dem Ordner herauszukommen (abgelehnt) und beobachtet den Ordner.

**Acceptance Scenarios**:

1. **Given** der Nutzer wählt im Dialog zum Öffnen oder Speichern eine Datei oder einen
   Ordner, **When** die Erweiterung diese eine Datei oder diesen Ordner liest oder schreibt,
   solange ihr Rahmen offen ist, **Then** braucht sie keine weitere Berechtigung.
2. **Given** eine Berechtigung „Lesen“ für einen Ordner, **When** die Erweiterung darin oder
   in Unterordnern liest, **Then** gelingt das; **When** sie schreiben will, **Then** fragt
   holzi.
3. **Given** ein Pfad, der über `..`, einen symbolischen Link oder eine andere Schreibweise aus
   dem erlaubten Ordner herausführt, **When** die Erweiterung ihn benutzt, **Then** prüft
   holzi gegen das tatsächliche Ziel und fragt oder lehnt ab.
4. **Given** ein beobachteter Ordner, **When** darin eine Datei entsteht, sich ändert oder
   verschwindet, **Then** erhält die Erweiterung eine Meldung mit Pfad und Art der Änderung.
5. **Given** die Vault-Datei, die Bundles anderer Erweiterungen oder Konfigurationsdateien von
   holzi, **When** eine Erweiterung sie lesen oder schreiben will, **Then** lehnt holzi ab,
   auch mit einer Berechtigung für einen übergeordneten Ordner.

---

### User Story 10 - Passwörter und entfernter Speicher über holzi (Priority: P3)

haex-calendar legt die Zugangsdaten eines CalDAV-Kontos im Passwortmanager von holzi ab und
liest sie später wieder. Eine Backup-Erweiterung lädt Dateien in einen S3-Speicher aus
Spec 038. Beide arbeiten nur über die Funktionen von holzi und nur innerhalb ihrer
Berechtigungen.

**Why this priority**: Nur einzelne Erweiterungen brauchen das. Der Passwortmanager (034) ist
gebaut, die Speicherverbindungen (038) noch nicht; die Story bindet sie an, sobald es sie gibt.

**Independent Test**: Mit Passwortmanager eine Berechtigung für Einträge mit dem Tag
„haex-calendar“ erteilen, einen Eintrag anlegen, lesen, ändern und löschen. Einen Eintrag mit
einem anderen Tag lesen (abgelehnt). Ohne Passwortmanager meldet die Funktion „nicht
verfügbar“. Mit S3-Speicher eine Datei hoch- und herunterladen.

**Acceptance Scenarios**:

1. **Given** eine Freigabe für Passwörter mit einem Tag, **When** die Erweiterung die
   Passwort-Funktionen nutzt, **Then** gelten genau die Regeln von 034 (FR-024 bis FR-029
   dort), mit der Erweiterung als Aufrufer.
2. **Given** holzi ohne Passwortmanager oder ohne S3-Speicher, **When** eine Erweiterung die
   jeweilige Funktion aufruft, **Then** erhält sie die Antwort „nicht verfügbar“ mit einer
   eigenen Fehlerart, und holzi bleibt stabil.
3. **Given** eine Berechtigung für einen bestimmten entfernten Speicher, **When** die
   Erweiterung dort auflistet, hochlädt, herunterlädt oder löscht, **Then** gelingt das, aber
   nie bei einem anderen Speicher.
4. **Given** eine Erweiterung, **When** sie einen entfernten Speicher anlegen, ändern oder
   entfernen will, **Then** muss der Nutzer das jedes Mal in einem Dialog von holzi
   bestätigen. Zugangsdaten gibt nur der Nutzer in holzi ein; die Erweiterung schickt und sieht
   sie nie (038 FR-013a, Review 2026-10-06).

---

### User Story 11 - Mail und Shell für spezialisierte Erweiterungen (Priority: P3)

haex-mail liest und sendet Mails über IMAP und SMTP mit den Funktionen von holzi und erfährt
von neuen Nachrichten. haex-code öffnet eine Shell, schreibt Eingaben hinein und zeigt die
Ausgabe. Beides ist mächtig und braucht ausdrückliche Berechtigungen.

**Why this priority**: Nur einzelne Erweiterungen brauchen das. Die Shell ist die stärkste
Berechtigung überhaupt.

**Independent Test**: Gegen einen lokalen Test-Mailserver Postfächer auflisten, eine
Nachricht abrufen, Flags setzen und eine Mail senden, mit und ohne Berechtigung für den
Server. Eine Shell mit Berechtigung starten, `echo` ausführen, Größe ändern, schließen;
ohne Berechtigung fragt holzi.

**Acceptance Scenarios**:

1. **Given** eine Berechtigung für einen Mailserver, **When** die Erweiterung Postfächer,
   Nachrichten und Anhänge abruft, Flags setzt, Nachrichten verschiebt, ablegt oder sendet,
   **Then** führt holzi das gegen genau diesen Server aus.
2. **Given** eine beobachtete Mailbox, **When** eine neue Nachricht eintrifft, **Then** erhält
   die Erweiterung eine Meldung.
3. **Given** die Anfrage, eine Shell zu starten, **When** holzi fragt, **Then** nennt die
   Anfrage das Programm und warnt deutlich, dass die Erweiterung damit alles tun kann, was der
   Nutzer auf dem Gerät tun kann.
4. **Given** eine laufende Shell, **When** der Tab der Erweiterung geschlossen, die
   Erweiterung deaktiviert oder die Vault geschlossen wird, **Then** beendet holzi die Shell.
5. **Given** ein Gerät ohne Shell (Mobilgerät), **When** eine Erweiterung eine starten will,
   **Then** erhält sie „nicht verfügbar“.

---

### User Story 12 - Erweiterungen entwickeln (Priority: P2)

Eine Entwicklerin schaltet in den Einstellungen den Entwicklermodus ein und lädt ihre
Erweiterung vom lokalen Entwicklungsserver. Änderungen im Code erscheinen sofort im Tab. Sie
sieht die Konsolenausgabe der Erweiterung in holzi. Die Berechtigungen prüft holzi wie bei
einer installierten Erweiterung, damit sie das echte Verhalten sieht.

**Why this priority**: Ohne Entwicklermodus lässt sich keine Erweiterung für holzi bauen oder
anpassen, auch nicht die Werkzeuge aus 019.

**Independent Test**: Entwicklermodus einschalten, eine Erweiterung von `localhost`
laden, eine Datei ändern (der Tab aktualisiert), eine Berechtigung anfragen (Anfrage
erscheint), Entwicklermodus ausschalten (Erweiterung verschwindet).

**Acceptance Scenarios**:

1. **Given** der Entwicklermodus ist aus, **When** jemand eine Erweiterung von einem
   Entwicklungsserver laden will, **Then** bietet holzi das nicht an.
2. **Given** der Entwicklermodus ist an, **When** die Entwicklerin eine Adresse auf
   `localhost` angibt, **Then** lädt holzi die Erweiterung ohne Signatur und kennzeichnet ihren
   Tab dauerhaft als Entwicklungsfassung.
3. **Given** eine Erweiterung im Entwicklermodus, **When** sie Host-Funktionen nutzt, **Then**
   gelten dieselben Berechtigungen, Anfragen und Grenzen wie bei einer installierten. Die
   erklärten Berechtigungen bestätigt die Entwicklerin beim Laden.
4. **Given** eine installierte Erweiterung mit demselben Herausgeberschlüssel und Namen,
   **When** die Entwicklerin dieselbe Erweiterung im Entwicklermodus lädt, **Then** lehnt holzi
   ab und nennt den Grund. Eine Erweiterung im Entwicklermodus übernimmt nie Registrierung,
   Daten oder Berechtigungen einer installierten.
5. **Given** eine Erweiterung im Entwicklermodus, **When** sie Tabellen anlegt, **Then**
   bleiben diese Tabellen, ihre Registrierung und ihre Berechtigungen auf diesem Gerät und
   gehen nicht in den Sync.

---

### Edge Cases

- **Die Vault wird geschlossen, während eine Anfrage wartet**: Wartende Anfragen werden
  abgelehnt, laufende Abfragen enden mit Fehler, Shells und Dateibeobachtungen enden. Nichts
  wird nach dem Schließen ausgeführt (ADR-0003).
- **Eine Erweiterung sendet viele Anfragen in kurzer Zeit**: holzi begrenzt die Zahl
  gleichzeitiger Anfragen je Erweiterung. Was darüber hinausgeht, wartet oder scheitert,
  ohne holzi oder andere Erweiterungen zu bremsen.
- **Eine Abfrage liefert sehr viele Zeilen oder läuft sehr lange**: holzi begrenzt Zeilenzahl,
  Laufzeit und Größe einer Anfrage und bricht mit einem eindeutigen Fehler ab, auch bei
  `SELECT` über den Weg „ausführen“.
- **Eine Erweiterung antwortet beim Start nicht**: Kommt der Kanal nicht zustande, zeigt der
  Tab eine Fehlermeldung mit „Neu laden“ statt einer leeren Fläche.
- **Eine Erweiterung versucht, das Fenster von holzi zu steuern** (Navigation, Fokus, Vollbild,
  Pop-ups, Tastenkürzel abfangen): Das bleibt ohne Wirkung auf holzi (Spec 020 FR-034).
  Tastenkürzel von holzi funktionieren auch, wenn der Fokus im Rahmen liegt; die Brücke leitet
  sie weiter, aber nur, solange der Rahmen den Fokus hat und das Fenster von holzi aktiv ist.
  Eine Erweiterung kann damit nicht mehr auslösen als ein Tastendruck des Nutzers in diesem
  Moment.
- **Zwei Geräte installieren gleichzeitig verschiedene Fassungen**: Nach dem Sync gilt auf
  allen Geräten die höhere Fassung. Migrationen, die ein Gerät schon angewendet hat, bleiben
  angewendet; die höhere Fassung muss sie enthalten.
- **Ein synchronisiertes Bundle ist größer als das Postfach des Sync-Servers erlaubt**: Es
  kommt direkt zwischen den Geräten an (wie große Anhänge in 034). Bis dahin ist die
  Erweiterung auf dem anderen Gerät als „wird übertragen“ gekennzeichnet.
- **Eine Migration scheitert auf einem Gerät, auf dem anderen nicht** (etwa wegen
  unterschiedlicher Daten): Die Erweiterung startet auf diesem Gerät nicht, die Migration ist
  dort nicht angewendet, der Nutzer sieht den Fehler. Der Sync der übrigen Daten läuft
  weiter, Daten für ihre Tabellen werden aufbewahrt, bis die Tabellen da sind.
- **Eine Anfrage wartet auf die Entscheidung, und der Tab wird geschlossen**: Die Anfrage
  verschwindet aus der Warteschlange, wenn kein anderer Tab derselben Erweiterung auf sie
  wartet.
- **Die Prüfung einer SQL-Anweisung kann eine Form nicht sicher zuordnen** (neue Syntax,
  Funktion mit Tabellenargument, virtuelle Tabelle): holzi lehnt die Anweisung ab. Im
  Zweifel gilt Ablehnung, nie Durchlassen.
- **Zwei Erweiterungen desselben Herausgebers mit Namen, deren Präfixe sich überschneiden
  könnten** (`a` und `a__b`): Namen mit `__` sind ungültig. Präfixe sind damit eindeutig.

## Requirements _(mandatory)_

### Functional Requirements

**Installation, Signatur und Fassungen**

- **FR-001**: holzi MUSS Erweiterungen aus einer Bundle-Datei installieren können, die der
  Nutzer auswählt. Vor der Installation MUSS es Name, Fassung, Beschreibung, Symbol,
  Herausgeber (eine kurze, vergleichbare Form des Herausgeberschlüssels), Gültigkeit der
  Signatur und alle erklärten Berechtigungen aller Arten zeigen.
- **FR-002**: holzi MUSS die Signatur über den Inhalt des Bundles und das Manifest mit dem
  Herausgeberschlüssel prüfen und ein Bundle mit fehlender oder ungültiger Signatur ablehnen.
  Die Signatur MUSS Pfad, Länge und Inhalt jeder Datei des Bundles und das kanonische
  Manifest decken, so dass weder Inhalte zwischen Dateien verschoben noch Dateien umbenannt,
  ergänzt oder entfernt werden können, ohne dass die Prüfung scheitert. Das Format MUSS eine
  Kennung seiner Fassung tragen. Bundles im Signaturformat von haex-vault 2.x MUSS holzi mit dem
  Hinweis ablehnen, dass sie mit dem aktuellen Werkzeug `haex` neu signiert werden müssen.
- **FR-003**: holzi MUSS die Signatur vor jedem Start der Erweiterung erneut prüfen, nicht nur
  bei der Installation, und eine Erweiterung mit veränderten Dateien nicht starten.
- **FR-004**: Name und Herausgeberschlüssel MÜSSEN beim Installieren geprüft werden: Der Name
  besteht nur aus Kleinbuchstaben, Ziffern und Bindestrichen, beginnt mit einem Buchstaben und
  enthält kein `__`; der Herausgeberschlüssel ist ein gültiger Ed25519-Schlüssel. Ein Update
  MUSS denselben Herausgeberschlüssel und Namen wie die installierte Erweiterung haben.
- **FR-005**: Die Installation MUSS für die Vault gelten. Bundle, Registrierung und gemerkte
  Berechtigungen MÜSSEN Vault-Daten sein, die über den Sync aus Spec 024 auf alle eigenen
  Geräte gelangen. Jedes Gerät MUSS ein empfangenes Bundle selbst prüfen (FR-002), bevor es
  die Erweiterung anbietet.
- **FR-006**: Bei einem Update MUSS holzi nur die neu erklärten Berechtigungen zur Bestätigung
  vorlegen. Bisherige Entscheidungen bleiben, außer für erklärte Berechtigungen, die das neue
  Manifest nicht mehr erklärt: diese MÜSSEN entfernt werden. Ein Downgrade MUSS eine ausdrückliche
  Bestätigung verlangen, ebenso ein Bundle derselben Version mit anderem Inhalt; bestätigt,
  MUSS es das vorhandene ersetzen (zurückziehen), damit alle Geräte dasselbe Bundle wählen.
- **FR-007**: Der Nutzer MUSS eine Erweiterung deaktivieren und wieder aktivieren können. Eine
  deaktivierte Erweiterung DARF keine Host-Funktion mehr ausführen; ihre Tabs schließen sich,
  ihre Daten bleiben.
- **FR-008**: Der Nutzer MUSS eine Erweiterung entfernen können, mit der Wahl „Daten behalten“
  oder „Daten löschen“. „Daten löschen“ MUSS eigene Tabellen, Berechtigungen,
  Schlüssel-Wert-Speicher und Protokolle der Erweiterung auf allen eigenen Geräten entfernen.
  Behaltene Daten MÜSSEN in den Einstellungen sichtbar und löschbar sein. Solange Daten
  behalten werden, MÜSSEN die Migrationen der Erweiterung als Vault-Daten bleiben, damit jedes
  Gerät deren Tabellen weiter anlegen und Sync-Daten dafür übernehmen kann (FR-036, FR-037).

**Einbettung und Abschottung**

- **FR-009**: Eine Erweiterung MUSS in einem abgeschotteten iframe in einem Tab des wm laufen,
  in einem Ursprung nur für diese Erweiterung, getrennt von holzi und von jeder anderen
  Erweiterung. Sie DARF keinen Zugriff auf Dokument, Speicher, Cookies oder Funktionen von
  holzi oder anderen Erweiterungen haben. Kein Weg aus einer Erweiterung DARF den Chat, den
  eingebauten Agenten oder ein Modell erreichen. Ein Vertragstest MUSS alle Host-Funktionen
  aufzählen und scheitern, wenn eine davon Chat- oder Modellcode erreicht (ADR-0004).
- **FR-010**: Ein Erweiterungsrahmen DARF keine Netzverbindung am Host vorbei aufbauen und
  keine Inhalte von außerhalb seines Bundles laden. Netzzugriff gibt es nur über FR-050.
- **FR-011**: holzi MUSS die Erweiterung hinter einer Anfrage aus dem Rahmen bestimmen, über
  den sie kam, nie aus Angaben in der Anfrage. Der Kanal MUSS an genau einen Rahmen gebunden
  sein, und die Einrichtung des Kanals MUSS sicherstellen, dass nur dieser Rahmen ihn erhält.
- **FR-012**: Eine Erweiterung MUSS die Tab-Schnittstelle aus Spec 015 nutzen können:
  Titel setzen, Aufmerksamkeit anfordern, sich schließen, beim Schließen nachfragen. Ihre
  Orte MÜSSEN über die Brücke in den Verlauf des Tabs gelangen (Spec 020, R17), und
  Tastenkürzel von holzi MÜSSEN auch bei Fokus im Rahmen wirken.
- **FR-013**: holzi MUSS das Protokoll des vault-sdk v3.7.0 (Revision oben) sprechen:
  Aufbau des Kanals, Form von Anfragen, Antworten und Meldungen, Fehlercodes (unter anderem
  „Anfrage nötig“ und „verweigert“) und die Namen der Host-Funktionen. Erweiterungen aus dem
  haextension-Repository MÜSSEN ohne Änderung ihres Codes laufen, soweit sie Funktionen dieser
  Spec nutzen.
- **FR-014**: holzi DARF keine eigenen nativen Fenster für Erweiterungen öffnen (ADR-0004).
  Ein Manifest, das das verlangt, läuft im Tab.

**Berechtigungen**

- **FR-015**: Jede Host-Funktion außer Kontext, eigenen Tabellen, eigenem
  Schlüssel-Wert-Speicher und eigenen Protokollen MUSS eine Berechtigung verlangen. Eine
  Berechtigung MUSS Art, Aktion und Ziel haben. Ziele MÜSSEN ein genauer Wert, ein Präfix mit
  `*` oder `*` für alles sein.
- **FR-016**: Die erklärten und bei der Installation bestätigten Berechtigungen MÜSSEN als
  „erteilt“ gelten, abgewählte als „fragen“. Bestätigte Shell-Berechtigungen MÜSSEN geräteeigen
  für das installierende Gerät gelten, alle anderen vault-weit; der Installationsdialog MUSS
  sagen, dass die Erweiterung in der ganzen Vault installiert wird, und bei der Shell, dass sie
  nur auf diesem Gerät gilt. Eine Berechtigung, die nicht erklärt war, MUSS
  zur Laufzeit erfragt und in der Anfrage als „nicht erklärt“ gekennzeichnet werden.
- **FR-017**: Bei mehreren passenden Berechtigungen MUSS „verweigert“ vor „erteilt“ und
  „erteilt“ vor „fragen“ gelten. „Lesen und Schreiben“ deckt „Lesen“. Bei Passwörtern gilt das je
  Tag: Ein verweigertes Tag nimmt die Freigabe für dasselbe Tag und verbirgt seine Einträge vor
  einer Freigabe für `*`; ein Eintrag, der zusätzlich ein erteiltes Tag trägt, bleibt über dieses
  sichtbar (eine Freigabe für `private` zeigt alle Einträge mit `private`, auch wenn sie zusätzlich
  das verweigerte `calendar` tragen).
- **FR-018**: Eine Anfrage MUSS Erweiterung, Art, Aktion, Ziel und die Kennzeichnung aus
  FR-016 zeigen und „Erlauben“ und „Verweigern“ mit der Wahl „Merken“ bieten. Gemerkte
  Entscheidungen MÜSSEN Vault-Daten sein und vault-weit gelten, außer bei der Shell: Dort
  MÜSSEN sie geräteeigen gelten (ADR-0001). Eine Wahl des Geltungsbereichs gibt es nicht; die
  Anfrage MUSS sagen, wo die gemerkte Entscheidung gelten wird. Nicht gemerkte gelten nur auf
  diesem Gerät bis zum Schließen der Vault. Die Einstellungs-App MUSS bei jeder Berechtigung
  zeigen, ob sie vault-weit oder geräteeigen ist und für welches Gerät. Ändert der Nutzer dort
  den Zustand einer Zeile, MUSS sie ihren Geltungsbereich behalten, wenn er zu ihrer Art passt
  (eine Shell-Berechtigung eines anderen Geräts bleibt auf diesem Gerät); passt er nicht (aus der
  Zeit vor der Klärung vom 2026-10-06), MUSS holzi sie durch eine Zeile im Geltungsbereich der Art
  ersetzen. Eine vault-weite „Erlauben“-Zeile der Shell aus dieser Zeit DARF nirgends gelten, jedes
  Gerät fragt neu; eine vault-weite „Verweigern“-Zeile gilt weiter.
- **FR-019**: Anfragen MÜSSEN in einer Warteschlange nacheinander erscheinen, gleiche
  Anfragen zusammengefasst. Die Erweiterung MUSS die Entscheidung erfahren, damit das
  vault-sdk die Anfrage wiederholen oder aufgeben kann. Eine Anfrage, auf die niemand mehr
  wartet, MUSS aus der Warteschlange verschwinden.
- **FR-020**: Die Einstellungs-App MUSS in der Kategorie „Erweiterungen“ jede Erweiterung mit
  ihren Berechtigungen (Art, Aktion, Ziel, Zustand, erklärt oder nicht) zeigen und Ändern und
  Widerrufen erlauben, sowie die vorläufigen Berechtigungen dieses Geräts. Änderungen MÜSSEN
  sofort gelten, auch für offene Tabs, ohne Knopf zum Übernehmen.
- **FR-021**: Eine Erweiterung DARF keine Host-Funktion haben, mit der sie Berechtigungen
  erteilt, Anfragen beantwortet, ihre Grenzwerte liest oder ändert oder den Kontext von holzi
  ändert. Dasselbe gilt für Agenten (Specs 018, 021).
- **FR-022**: Gespeicherte Berechtigungen mit unbekannter Art, Aktion oder Ziel MÜSSEN als
  nicht vorhanden gelten, nie als eine andere Berechtigung.

**SQL**

- **FR-023**: Eine Erweiterung MUSS SQL auf ihre eigenen Tabellen ohne Berechtigung absetzen
  können: lesen, einfügen (auch mit RETURNING), ändern und löschen, als einzelne Anweisung
  oder als Transaktion aus mehreren Anweisungen, die ganz oder gar nicht wirkt.
- **FR-024**: Lesen in Tabellen einer anderen Erweiterung MUSS die Berechtigung „Lesen“,
  Schreiben die Berechtigung „Lesen und Schreiben“ auf diese Tabellen verlangen. Ziele sind
  eine Tabelle oder alle Tabellen einer Erweiterung. Schemaänderungen an Tabellen anderer
  Erweiterungen MÜSSEN immer abgelehnt werden.
- **FR-025**: Jede Anweisung, die eine Kerntabelle liest oder schreibt, MUSS abgelehnt
  werden, ohne zu fragen. Keine Berechtigung, ob gemerkt oder vorläufig, DARF das ändern.
- **FR-026**: holzi MUSS jede Tabelle prüfen, die eine Anweisung berührt, an jeder Stelle:
  Haupttabelle, Joins, Unterabfragen in jeder Klausel, `WITH`-Ausdrücke, `EXISTS`,
  Funktionsargumente, Namen mit Schemaangabe und Aliasnamen. Eine Form, die holzi nicht sicher
  zuordnen kann, MUSS abgelehnt werden. Ein Name aus einem `WITH`-Ausdruck DARF nie als
  eigene Tabelle gelten.
- **FR-027**: Erlaubt sind Lesen, Einfügen, Ändern und Löschen. Alles andere MUSS zur
  Laufzeit abgelehnt werden: Schemaänderungen, `ATTACH`, `PRAGMA`, Sichten, Trigger,
  virtuelle Tabellen, Transaktionssteuerung durch die Erweiterung selbst, mehrere
  Anweisungen in einer Zeichenkette. Das Schreiben in die Spalten für den Sync MUSS
  abgelehnt werden.
- **FR-028**: Jede Schreibanweisung einer Erweiterung MUSS über denselben Weg laufen wie
  Schreibvorgänge von holzi selbst: mit Zeitstempeln für den Sync, Löschmarken für
  synchronisierte Tabellen und derselben Größengrenze je Transaktion.
- **FR-029**: Eigene Tabellen mit der Endung `_no_sync` MÜSSEN auf dem Gerät bleiben. Ihre
  Änderungen und Löschungen DÜRFEN NIE in den Sync gelangen.
- **FR-030**: Ergebnisse MÜSSEN die Form haben, die das vault-sdk erwartet (Zeilen als
  Wertlisten, Spaltennamen, Zahl der geänderten Zeilen), und `SELECT` MUSS auch über den Weg
  „ausführen“ funktionieren, wie es Drizzle verlangt.
- **FR-031**: holzi MUSS je Erweiterung Grenzen für Zeilenzahl, Laufzeit, Größe einer Anfrage
  und gleichzeitige Anfragen durchsetzen, auf jedem Weg, der Zeilen liefert. Der Nutzer MUSS
  die Grenzen je Erweiterung in den Einstellungen sehen und ändern können.

**Migrationen**

- **FR-032**: holzi MUSS Migrationen einer Erweiterung aus ihrem Bundle und über die
  Registrierung zur Laufzeit (vault-sdk) annehmen, in ihrer Reihenfolge, jede genau einmal je
  Gerät. Welche Migration auf einem Gerät angewendet ist, MUSS geräteeigen in einer Tabelle
  festgehalten werden, die nicht synchronisiert wird und den Zustand der Vault-Datei beschreibt.
- **FR-033**: Eine Migration DARF nur eigene Tabellen und deren Indizes anlegen, ändern und
  löschen und Daten in eigenen Tabellen ändern. Für jede Anweisung MÜSSEN dieselben Prüfungen
  wie zur Laufzeit gelten (FR-025, FR-026), dazu die Regeln für Schemaänderungen. Sichten,
  Trigger, virtuelle Tabellen und `ATTACH` MÜSSEN abgelehnt werden. `PRAGMA` ist nur in den
  Formen erlaubt, die Drizzle für Tabellenumbauten erzeugt.
- **FR-034**: Eine Migration MUSS ganz oder gar nicht angewendet werden. Scheitert sie, DARF
  die Erweiterung auf diesem Gerät nicht starten, und der Nutzer MUSS den Grund sehen.
- **FR-035**: Neue oder geänderte eigene Tabellen MÜSSEN danach vollständig am Sync
  teilnehmen (Zeitstempel je Zeile und Spalte, Löschmarken), außer `_no_sync`-Tabellen.
- **FR-036**: Migrationen MÜSSEN Teil der Vault-Daten der Erweiterung sein, damit jedes Gerät
  die Tabellen anlegen kann, auch eines, auf dem die Erweiterung noch nie lief.

**Mehrere Geräte**

- **FR-037**: Ein Gerät MUSS die Tabellen einer Erweiterung über ihre Migrationen anlegen,
  bevor es Zeilen für diese Tabellen übernimmt. Zeilen für Tabellen, die es noch nicht hat,
  MUSS es aufbewahren und später übernehmen, ohne den Sync anderer Daten aufzuhalten.
  Zeilen für eine Tabelle oder Spalte, die eine bereits angewendete Migration entfernt oder
  umbenannt hat, stammen von einem Gerät mit älterer Fassung: Sie MÜSSEN warten, bis dieses
  Gerät aktualisiert hat, und dann in der Form übernommen werden, die seine Migration ihnen
  gegeben hat. Sie DÜRFEN den Sync der Erweiterung nicht dauerhaft anhalten, auch nicht,
  wenn das Gerät sie aufbewahrt hat, bevor es selbst diese Migration anwandte. Die
  Einstellungen der Erweiterung MÜSSEN je Gerät dessen Fassung zeigen und ein Gerät mit
  älterer Fassung als hier so kennzeichnen, dass erkennbar ist: Daten von diesem Gerät kommen
  teils erst nach seinem Update an.
- **FR-038**: Kommt ein Update an, MUSS ein Gerät dessen neue Migrationen anwenden, bevor es
  die neue Fassung startet. Offene Tabs MÜSSEN neu laden oder einen Hinweis zeigen.
- **FR-039**: Entfernen und Deaktivieren MÜSSEN auf allen eigenen Geräten wirken.

**Änderungsmeldungen und Kontext**

- **FR-040**: holzi MUSS jeder offenen Erweiterung die Änderungen an Tabellen melden, die sie
  lesen darf, gleich wer sie geändert hat (sie selbst, ein anderer Tab, eine andere
  Erweiterung, holzi, ein anderes Gerät). Die Liste MUSS an der Prüfstelle nach ihren
  Berechtigungen gekürzt werden. Kerntabellen DÜRFEN nie darin stehen.
- **FR-041**: holzi MUSS der Erweiterung den Kontext geben (Theme, Sprache, Plattform,
  Gerätekennung) und ihr Änderungen melden.
- **FR-042**: Meldungen, die eintreffen, bevor der Kanal steht, MÜSSEN gepuffert und danach
  zugestellt werden.

**Schlüssel-Wert-Speicher und Protokolle**

- **FR-043**: Jede Erweiterung MUSS einen eigenen Schlüssel-Wert-Speicher haben, der über
  Tabs, Fenster und Neustarts erhalten bleibt, für andere Erweiterungen unsichtbar ist, nur für
  das Gerät gilt, auf dem er geschrieben wurde (ADR-0001), und beim Entfernen mit „Daten löschen“
  verschwindet.
- **FR-044**: Eine Erweiterung MUSS Protokolleinträge schreiben und nur ihre eigenen lesen
  können. Sie bleiben auf dem Gerät und sind in der Größe begrenzt (älteste fallen weg). Der
  Nutzer MUSS sie in den Einstellungen sehen können.
- **FR-045**: Im Entwicklermodus MUSS holzi die Konsolenausgabe der Erweiterung anzeigen.

**Dateisystem**

- **FR-046**: holzi MUSS die Dateifunktionen des vault-sdk anbieten: Dateien lesen und
  schreiben, Ordner lesen und anlegen, entfernen, umbenennen, kopieren, Existenz und Angaben
  prüfen, Dateien öffnen, Dialoge zum Öffnen und Speichern, bekannte Orte (etwa
  „Dokumente“), Ordner beobachten.
- **FR-047**: Dateiberechtigungen MÜSSEN für einen Pfad mit Unterordnern und für „Lesen“
  oder „Lesen und Schreiben“ gelten. Geprüft MUSS immer das tatsächliche Ziel werden, nach
  Auflösen von `..` und symbolischen Links. Eine gemerkte Dateiberechtigung gilt vault-weit auf
  jedem Gerät für denselben Pfad, wenn es ihn dort gibt.
- **FR-048**: Was der Nutzer in einem Dialog zum Öffnen oder Speichern selbst auswählt, MUSS
  die Erweiterung ohne weitere Berechtigung lesen (Öffnen) oder lesen und schreiben (Speichern)
  dürfen, solange der Rahmen offen ist, aus dem der Dialog kam; danach nicht mehr.
- **FR-049**: Vault-Dateien, Bundles, Daten und Konfiguration von holzi MÜSSEN für
  Erweiterungen immer gesperrt sein, auch mit einer Berechtigung für einen übergeordneten
  Ordner.

**Netzwerk und Benachrichtigungen**

- **FR-050**: holzi MUSS HTTP-Anfragen für eine Erweiterung ausführen (Methode, Adresse,
  Kopfzeilen, Inhalt, Zeitlimit) und Status, Kopfzeilen, Inhalt und endgültige Adresse
  zurückgeben. Berechtigungen MÜSSEN für eine Adresse oder ein Adressmuster und optional für
  bestimmte Methoden gelten. Folgt eine Anfrage einer Umleitung, MUSS auch das neue Ziel
  gedeckt sein.
- **FR-051**: holzi MUSS eine Adresse im Standardbrowser öffnen können, nach Berechtigung.
- **FR-052**: holzi MUSS Systembenachrichtigungen für eine Erweiterung zeigen und entfernen,
  nach Berechtigung. Beim Klicken MUSS es die Erweiterung benachrichtigen und ihren Tab nach
  vorn holen.

**Passwörter und entfernter Speicher**

- **FR-053**: Die Passwort-Funktionen des vault-sdk (auflisten, lesen, anlegen, ändern,
  löschen) MÜSSEN über die Zugriffsprüfung des Passwortmanagers (034) laufen, mit der
  Erweiterung als Aufrufer und den Freigaben dieser Spec als ihren Freigaben. Gibt es den
  Passwortmanager noch nicht, MÜSSEN sie „nicht verfügbar“ antworten.
- **FR-054**: Die Funktionen für entfernten Speicher (auflisten, hochladen, herunterladen,
  löschen) MÜSSEN die Speicher aus 038 nutzen, mit einer Berechtigung je Speicher. Gibt es
  038 noch nicht, MÜSSEN sie „nicht verfügbar“ antworten.
- **FR-055**: Anlegen, Ändern, Prüfen und Entfernen eines Speichers durch eine Erweiterung
  MUSS jedes Mal eine Bestätigung in einem Dialog von holzi verlangen. Zugangsdaten DÜRFEN nur
  in holzi eingegeben werden und eine Erweiterung nie erreichen; ein Aufruf einer Erweiterung,
  der Zugangsdaten enthält, MUSS abgelehnt werden (038 FR-012, FR-013a; Review 2026-10-06).

**Mail und Shell**

- **FR-056**: holzi MUSS die Mail-Funktionen des vault-sdk anbieten (Postfächer, Umschläge,
  Nachrichten, Anhänge, Flags, Verschieben, Ablegen, Senden, Nachricht bauen, Beobachten) mit
  einer Berechtigung je Mailserver oder für alle Mailserver (`*`; ein Mailprogramm kennt die
  Server seiner Nutzer nicht im Voraus). Neue Nachrichten in einer beobachteten Mailbox MUSS
  holzi melden.
- **FR-057**: holzi MUSS die Shell-Funktionen des vault-sdk anbieten (verfügbare Shells
  auflisten, starten, schreiben, Größe ändern, schließen, Ausgabe und Ende melden) mit einer
  Berechtigung je Programm. Die Anfrage MUSS deutlich warnen.
- **FR-058**: Shells und Mail-Beobachtungen MÜSSEN enden, wenn der letzte Tab der
  Erweiterung schließt, die Erweiterung deaktiviert wird oder die Vault schließt.
- **FR-059**: Auf Plattformen ohne Shell MÜSSEN die Shell-Funktionen „nicht verfügbar“
  antworten.

**Abgrenzung und Fehler**

- **FR-060**: Jede Host-Funktion, die holzi nicht kennt oder die diese Spec nicht anbietet,
  MUSS mit einer eigenen Fehlerart „nicht unterstützt“ beantwortet werden (Ablehnung als
  Grundeinstellung). Eine Erweiterung MUSS danach weiterlaufen können.
- **FR-061**: Die Space-Funktionen des vault-sdk (Zeilen einem Space zuordnen, Zuordnungen
  lesen, Spaces und Mitglieder auflisten) MÜSSEN „nicht unterstützt“ antworten. Daten teilen
  Erweiterungen in holzi über Spec 028.
- **FR-062**: Fehlermeldungen an eine Erweiterung DÜRFEN nicht verraten, ob eine Tabelle, ein
  Eintrag oder eine Datei außerhalb ihrer Berechtigungen existiert.
- **FR-066**: Erweiterungen MÜSSEN auf Linux, macOS, Windows, Android und iOS mit denselben Regeln laufen. Auf
  keiner Plattform DARF ein Erweiterungsrahmen Zugriff auf die interne Schnittstelle von holzi haben. Funktionen,
  die eine Plattform nicht bietet (Shell, Beobachten von Ordnern, freie Dateipfade auf Mobilgeräten), MÜSSEN
  „nicht verfügbar“ antworten.
- **FR-067**: Der Installationsdialog MUSS die erklärten Berechtigungen nach Art gruppiert zeigen, die
  Werkzeuge für den Agenten (Spec 018) als eigene Gruppe. Jede Gruppe MUSS sich auf- und zuklappen lassen und
  ist zu Beginn zugeklappt; ihre Kopfzeile nennt den Namen der Art und wie viele ihrer Berechtigungen angehakt
  sind. Enthält eine Gruppe eine weitreichende Berechtigung (ein Ziel `*`, außer bei
  `notifications` und `peers`, oder die Web-Aktion `*`), MUSS ihre Kopfzeile ein Warndreieck tragen, auch
  zugeklappt; das Warndreieck steht nur für Platzhalter, nicht für ändernde oder löschende Werkzeuge. Die
  Werkzeuge stehen mit ihrem Titel da, nie mit dem technischen Namen der Funktion. Die Vorgabe (alles angehakt), die Abwahl einzelner Berechtigungen und das Ergebnis der Installation bleiben
  unverändert.

**Entwicklermodus**

- **FR-063**: Den Entwicklermodus MUSS der Nutzer in den Einstellungen einschalten. Er gilt
  für dieses Gerät.
- **FR-064**: Im Entwicklermodus MUSS holzi eine Erweiterung von einer Adresse auf `localhost`
  ohne Signatur laden, ihren Tab dauerhaft kennzeichnen und ihre erklärten Berechtigungen beim
  Laden bestätigen lassen. Sonst gelten alle Regeln dieser Spec, mit einer Ausnahme: Für Antworten des
  Entwicklungsservers setzt holzi keine eigene Inhaltsrichtlinie, die Netzsperre aus FR-010 gilt dort also
  nicht. Host-Funktionen prüft holzi weiter wie bei einer installierten Erweiterung. Ihr Tab verhält
  sich wie der einer installierten Erweiterung: Titel, Zurück/Vor, Schließen-Schutz, `window.close()` und
  holzis Tastenkürzel wirken auch dort.
- **FR-065**: Eine Erweiterung im Entwicklermodus DARF keine installierte Erweiterung mit
  demselben Herausgeberschlüssel und Namen ersetzen oder deren Daten oder Berechtigungen
  nutzen. Ihre Registrierung, Berechtigungen und Tabellen MÜSSEN auf dem Gerät bleiben und
  DÜRFEN nicht in den Sync gelangen.

### Key Entities

- **Erweiterung**: Herausgeberschlüssel, Name, installierte Fassung, aktiv oder deaktiviert,
  Herkunft (installiert oder Entwicklungsserver), Zeitpunkt der Installation. Vault-Daten
  (außer im Entwicklermodus).
- **Bundle**: die Dateien einer Fassung mit Manifest und Signatur. Vault-Daten, damit jedes
  Gerät die Erweiterung hat.
- **Manifest**: Angaben der Erweiterung, darunter Einstiegsdatei, Darstellung, Migrationen
  und erklärte Berechtigungen. Teil des Bundles und von der Signatur gedeckt.
- **Berechtigung**: Erweiterung, Art (Datenbank, Dateisystem, Netzwerk, Benachrichtigungen,
  Passwörter, entfernter Speicher, Mail, Shell), Aktion, Ziel, Zustand, erklärt oder nicht.
  Gemerkte Berechtigungen sind Vault-Daten mit einem Geltungsbereich: vault-weit oder
  geräteeigen mit Bezug auf ein Gerät (ADR-0001); **vorläufige** gelten nur auf einem Gerät
  bis zum Schließen der Vault.
- **Migration**: Name, Reihenfolge und SQL einer Erweiterung. Vault-Daten. Die **Anwendung
  auf einem Gerät** ist geräteeigen und wird nicht synchronisiert.
- **Eigene Tabelle**: Tabelle mit dem Präfix der Erweiterung, synchronisiert oder mit
  `_no_sync` geräteeigen.
- **Grenzwerte**: je Erweiterung Zeilenzahl, Laufzeit, Größe und gleichzeitige Anfragen.
  Nur der Nutzer ändert sie.
- **Schlüssel-Wert-Eintrag**: je Erweiterung und Gerät (ADR-0001). **Protokolleintrag**: je
  Erweiterung, geräteeigen, nicht synchronisiert.
- **Erweiterungsrahmen**: ein laufender Tab einer Erweiterung mit seinem Kanal. Hält keine
  Daten über das Ende des Tabs hinaus.

## Success Criteria _(mandatory)_

### Measurable Outcomes

- **SC-001**: haex-notes und haex-calendar aus dem haextension-Repository (Revision oben), gebaut und signiert mit dem Kommandozeilenwerkzeug `haex` des vault-sdk in der
  Fassung mit dem neuen Signaturformat (FR-002), lassen sich ohne Änderung ihres Codes
  installieren und benutzen. Ausgenommen sind nur Funktionen, die Spaces (FR-061) oder die
  External Bridge brauchen. Ihre Hinweise darauf bleiben verständlich, und die Erweiterung
  läuft weiter. haex-pass ist kein Prüfgegenstand mehr: der eingebaute Passwortmanager (Spec 034)
  hat es abgelöst.
- **SC-002**: Eine Sammlung von Umgehungsversuchen (mindestens: `WITH`-Ausdrücke, `EXISTS`,
  Unterabfragen in jeder Klausel, Schemaangaben, Aliasnamen, Sichten, Trigger und `ATTACH` in
  Migrationen, mehrere Anweisungen in einer Zeichenkette, Schreiben in Sync-Spalten,
  Groß-/Kleinschreibung und Anführungszeichen in Namen, `..` und Links in Pfaden, verschobene,
  umbenannte, ergänzte oder entfernte Dateien in einem signierten Bundle, Umleitung
  im Netzwerk, Abruf am Host vorbei) wird in 100 % der Fälle abgelehnt, ohne dass Daten
  außerhalb der Berechtigungen gelesen oder geändert werden. Jede Lieferung von 017 enthält die
  Fälle für die Host-Funktionen, die sie bringt.
- **SC-003**: Nach der Installation auf einem Gerät steht die Erweiterung auf jedem anderen
  eigenen Gerät nach dessen nächstem erfolgreichen Sync im Launcher, ohne Schritt des
  Nutzers, und in 100 % der Testfälle bleibt der Sync anderer Daten dabei nicht hängen.
- **SC-004**: Ein Nutzer installiert eine Erweiterung aus einer Datei und öffnet sie in unter
  einer Minute, ohne Anleitung.
- **SC-005**: Eine Änderung an einer Tabelle erreicht eine offene Erweiterung mit
  Leseberechtigung innerhalb einer Sekunde nach dem Speichern auf demselben Gerät.
- **SC-006**: Eine offene Erweiterung, die viele Anfragen stellt oder eine teure Abfrage
  absetzt, macht holzi nicht unbedienbar: Tabs wechseln und Chat bleiben flüssig, und die
  Abfrage endet spätestens an ihrer Zeitgrenze.
- **SC-007**: Für jede Host-Funktion gibt es mindestens einen Test mit und einen ohne
  Berechtigung, und der Vertragstest aus FR-009 findet keinen Weg zu Chat oder Modell.

## Assumptions

- Zielplattformen sind Linux, macOS, Windows, Android und iOS. Die mobilen Builds von holzi entstehen in
  einer eigenen Spec; diese Spec baut den Host so, dass er dort läuft.
- Installiert wird nur aus Dateien. Ein Marktplatz oder ein Katalog von Erweiterungen kommt
  später mit eigener Spec.
- Es gibt keine Liste vertrauenswürdiger Herausgeber. Die Signatur beweist, dass das Bundle
  unverändert ist und von diesem Herausgeberschlüssel stammt, nicht, dass er vertrauenswürdig
  ist. Die Installation zeigt den Herausgeber, damit der Nutzer vergleichen kann.
- Das Präfix `<Herausgeberschlüssel>__<Name>__` und das Protokoll werden aus haex-vault 2.x und
  vault-sdk übernommen, damit der Code der Erweiterungen unverändert bleibt. Das
  Signaturformat übernimmt holzi nicht (FR-002); die Änderung am Werkzeug `haex` ist Arbeit im
  Repository `vault-sdk`, auf die holzi mit voller Revision verweist. Wo haex-vault 2.x
  von dieser Spec abweicht (Prüfung von Migrationen, Tabellen in `WITH` und Unterabfragen,
  vorläufige Berechtigungen für Kerntabellen, Abruf am Host vorbei, gemeinsamer Ursprung,
  Speicher je Fenster, Funktionen deaktivierter Erweiterungen, Löschmarken aus
  `_no_sync`-Tabellen), gilt diese Spec. Das sind Schwächen von haex-vault 2.x, die holzi nicht
  übernimmt. haex-vault 2.x selbst wird nicht korrigiert, weil holzi es ablöst; Code aus
  haex-vault 2.x ist Vorlage für Verhalten; seine Prüfungen in Rust werden übernommen, wo sie tragen, und dabei um
  die gefundenen Lücken korrigiert.
- Die Bundle-Datei bleibt ein Archiv wie in haex-vault 2.x; nur die Signatur ändert sich.
- Kerntabellen sind per SQL nie erreichbar. Für Kerndaten gibt es typisierte Funktionen
  (Passwörter aus 034); weitere kommen mit eigenen Specs.
- „Daten löschen“ beim Entfernen ist ein Löschen von Vault-Daten und wirkt über den Sync auf
  allen eigenen Geräten.
- Die Bundles sind klein genug für die Vault-Datenbank (wie Anhänge in 034, mit einer Grenze,
  die der Plan festlegt). Übertragen werden sie über den Sync aus 024.
- Wie in haex-vault 2.x gibt eine Mail-Erweiterung die Zugangsdaten des Mailservers bei jedem
  Aufruf mit; die Berechtigung gilt für den Server. Wo die Erweiterung die Zugangsdaten
  aufbewahrt (eigene Tabellen oder Passwortmanager über FR-053), ist ihre Sache.
- Mail und Shell setzt holzi selbst um, wie haex-vault 2.x; die Spec 017 nimmt dafür keine
  weiteren holzi-Funktionen (wie ein Mail-Programm) an.
- Agenten erreichen Erweiterungen erst mit Spec 018 (Werkzeuge über MCP). Die Meldung
  „Aktion angefordert“ des vault-sdk beantwortet holzi bis dahin nicht.
- Diese Spec darf vorab geschrieben werden. Wann sie umgesetzt wird, bestimmt die
  Phasenfolge des Projekts (`plans/README.md`). Der Plan schneidet sie in Lieferungen in der
  Reihenfolge der Prioritäten.

## Nicht im Umfang

- Marktplatz, Katalog, automatische Updates aus dem Netz.
- Werkzeuge einer Erweiterung für den Agenten über MCP (Spec 018) und das Ergänzen solcher
  Werkzeuge in den bestehenden haextensions (Spec 019).
- Freigaben je externem Agenten (Spec 021).
- Die External Bridge (Browser-Erweiterung, Autofill, Passkeys), auch die Anfragen von außen
  an eine Erweiterung (034).
- Zeilen einem Space zuordnen. Daten teilen geht über Spec 028.
- LocalSend, Identitäten, Lesezeichen und Sync-Server als Funktionen für Erweiterungen.
  haex-vault 2.x macht sie Erweiterungen auch nicht zugänglich.
- Native Fenster für Erweiterungen (ADR-0004).
- Aufrufe von einer Erweiterung zu einer anderen außer über SQL auf deren Tabellen.
