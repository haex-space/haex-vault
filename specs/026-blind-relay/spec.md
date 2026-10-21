# Feature Specification: Blind Relay: Sync über einen nicht vertrauenswürdigen Server

**Feature Branch**: `026-blind-relay`
**Created**: 2026-09-28
**Status**: Draft. Teilweise überholt (2026-10-21): Dateien teilt Spec 056 direkt zwischen den
Geräten, ohne Relay, Postfächer oder Inhaltsschlüssel; Spec 029 (Speicher-Backend B) ist gelöscht,
Spec 027 nur noch Grundlage für 028. Die Verweise auf 027 und 029 unten beschreiben den alten
Entwurf. Ob und wofür 026 bleibt, ist offen.
**Input**: Zeile 026 des Spec-Schnitts im Sync-Design
([`docs/plans/2026-09-28-sync-architecture.md`](../../docs/plans/2026-09-28-sync-architecture.md)
§14): ein Relay mit Postfächern, Sequenznummern als Lesestand, vom Client
erstellten Momentaufnahmen, signierten Mitgliederlisten, einem mitbetriebenen
Relay für den NAT-Durchgang und dem Speicher-Backend A. Das Relay ist ein
eigener, kopfloser, selbst betreibbarer Dienst für viele Nutzer. Es ist nicht
vertrauenswürdig: Es sieht keine Inhalte und kann nur die Verfügbarkeit
beeinträchtigen. Betreibervorgaben aus dem Design: Transport über iroh,
Anmeldung mit Nostr-Schlüsseln (Challenge-Response nach NIP-42), optional ein
Nostr-Relay für Signalisierung, Objektspeicher S3-kompatibel. Auf der Seite von
holzi kommen dazu: Relays in den Einstellungen eintragen, ein Postfach für die
eigene Vault, damit eigene Geräte auch ohne gleichzeitige Verbindung gleich
werden, das Erkennen zurückgehaltener Daten und klare Anzeigen, wenn ein Relay
nicht erreichbar ist oder ablehnt.

## Begriffe

- **Vault-Identität**: das secp256k1-Schlüsselpaar einer Vault (Spec 024). Ihr
  öffentlicher Schlüssel ist die feste Adresse der Vault; Zulassung, Kontingent
  und Mitgliedschaft am Relay hängen an ihr, nie an einem Gerät. Ihr privater
  Schlüssel liegt nur auf Hauptgeräten.
- **Gerät**: eine holzi-Instanz der Vault (Spec 024). Ein **Hauptgerät** hat den
  privaten Schlüssel der Vault-Identität und darf Geräte hinzufügen und
  entfernen; es kann mehrere geben. Ein **verknüpftes Gerät** liest und
  schreibt alle Daten der Vault und darf Spaces und Datenfreigaben verwalten,
  aber keine Geräte hinzufügen oder entfernen.
- **Geräteschlüssel**: das eigene Schlüsselpaar jedes Geräts (Spec 024). Es
  verlässt das Gerät nie. Damit meldet sich ein Gerät am Relay an und
  signiert alles, was es schreibt.
- **Geräteliste**: die von der Vault-Identität signierte Liste aller aktuellen
  Geräte einer Vault, je Gerät mit öffentlichem Geräteschlüssel, Rolle
  (Hauptgerät oder verknüpft), Name und Netzkennung, mit einer Generation
  (Spec 024). Es gelten dieselben Regeln wie bei der Mitgliederliste: Eine
  höhere Generation ersetzt eine niedrigere, bei gleicher Generation gilt die
  Liste mit dem kleinsten Hash, im Zweifel wird abgelehnt. Das Relay hält je
  Vault die neueste und nimmt ein Gerät nur an, wenn es darauf steht (FR-049).
- **Bereich**: das, was gemeinsam synchronisiert wird: die Vault selbst, ein
  Space oder eine Datenfreigabe. Das Relay kennt einen Bereich nur als
  undurchsichtige Kennung.
- **Admin eines Bereichs**: die Vault, die ihn angelegt hat, und nur sie. Beim
  Bereich „Vault“ ist das die Vault selbst. Die Admin-Rolle lässt sich in v1
  nicht übertragen. Handeln kann für den Admin jedes Gerät auf seiner
  aktuellen Geräteliste (D29).
- **Beenden**: eine von einem Gerät des Admins signierte, endgültige Erklärung,
  dass ein Bereich endet, etwa „Datenfreigabe beenden“ (Spec 028 FR-036).
  Danach nimmt das Relay für den Bereich nichts mehr an (FR-021).
- **Änderungspaket**: ein verschlüsselter, signierter Stapel von Änderungen
  eines Bereichs (Spec 024). Das Relay sieht davon nur Bereichskennung,
  Schlüsselkennung und Länge.
- **Inhaltsschlüssel** und **Schlüsselgeneration**: der symmetrische Schlüssel,
  mit dem ein Bereich verschlüsselt wird, und seine Generation (Spec 024, 027).
  Das Relay hat keinen davon.
- **Relay**: der nicht vertrauenswürdige Server dieser Spec. Der Dienst des
  Relays synchronisiert nur SQLite-Daten, also Änderungspakete und
  Momentaufnahmen in Postfächern; Dateien kommen nie in ein Postfach. Stellt
  der Betreiber zusätzlich einen S3-kompatiblen Speicher bereit
  (Speicher-Backend A), prüft der Dienst jeden Zugriff auf eine Datei und
  überträgt jedes verschlüsselte Objekt selbst zwischen diesem Speicher und
  dem Gerät (D24); jedes Objekt liegt dort genau einmal. Nutzt ein Bereich
  eigenen S3-Speicher (Speicher-Backend B), hat das Relay mit Dateien nichts
  zu tun. Dazu betreibt das Relay ein Relay für den NAT-Durchgang, optional
  die Signalisierung und optional die Ablage von Wiederherstellungspaketen.
- **Postfach**: die Ablage eines Bereichs auf einem Relay. Es nimmt
  Änderungspakete nur an und vergibt jedem angenommenen Paket eine fortlaufende
  **Sequenznummer**. Ein Gerät holt „alles nach Nummer n“; die höchste
  verarbeitete Nummer ist sein **Lesestand**.
- **Momentaufnahme**: ein verschlüsselter Stand eines Bereichs bis zu einer
  Sequenznummer, von einem Client erstellt, mit den ursprünglichen Signaturen
  jeder Änderung, in vollständigen Transaktionen (Spec 024). Danach verwirft das Relay die älteren Pakete (Kompaktierung).
- **Mitgliederliste**: eine vom Admin signierte Liste {Vault-Identität →
  Fähigkeiten} je Bereich, mit Generation, Ausstellungs- und Ablaufzeit. Das
  Relay hält die jeweils gültige Liste; eine höhere Generation ersetzt eine
  niedrigere, bei gleicher Generation gilt die Liste mit dem kleinsten Hash
  (FR-018); fehlt die Liste, ist sie abgelaufen oder ungültig, wird
  abgelehnt.
- **Fähigkeiten**: **Lesen**, **Schreiben**, **Löschen**; die Rolle **Admin**
  hat nur der Admin des Bereichs. Das Relay prüft davon nur, was es sehen kann
  (Lesen gegen Schreiben, beim Speicher auch Löschen).
- **Zulassung**: die Aufnahme einer Vault-Identität bei einem Relay über einen
  **Einladungscode** des Betreibers. Sie trägt ein **Kontingent** an
  Speicherplatz.
- **Objekt**: verschlüsselter, unveränderlicher Dateiinhalt, benannt nach dem
  Hash seines Chiffrats (Spec 025).
- **Speicher-Backend A**: optionaler, S3-kompatibler Objektspeicher, den der
  Betreiber des Relays zusätzlich stellt. Das Relay überträgt die Objekte
  selbst zwischen diesem Speicher und dem anfragenden Gerät, über seinen
  eigenen Endpunkt und dieselbe Protokollfamilie wie beim Postfach; es ist
  auch für Dateien ein blinder Teilnehmer. **Speicher-Backend B** ist der
  eigene S3-Speicher des Nutzers (Spec 029), ohne das Relay.
- **Wiederherstellungsschlüssel**: ein zufälliger Schlüssel hoher Entropie,
  den holzi beim Einrichten der Wiederherstellung einmal als Code und QR-Code
  zeigt und den der Nutzer offline aufbewahrt (FR-050). holzi und das Relay
  speichern ihn nicht.
- **Wiederherstellungspaket**: verschlüsselt auf dem Relay hinterlegt; enthält
  den privaten Schlüssel der Vault-Identität, die Generationen des
  Inhaltsschlüssels des Bereichs „Vault“ und die Liste der Relays (FR-052).
  Entschlüsseln kann es nur, wer den Wiederherstellungsschlüssel hat.
- **Fortschrittsstand** (Versionsvektor): je Ursprungsgerät die höchste gesehene
  Änderung (Spec 024). Er zeigt, ob einem Gerät etwas fehlt, egal über welchen
  Weg es kam.

## Beziehung zu bestehenden Specs

- Sync-Design
  [`docs/plans/2026-09-28-sync-architecture.md`](../../docs/plans/2026-09-28-sync-architecture.md):
  Quelle dieser Spec. Maßgeblich sind die Entscheidungen D1, D6, D7, D9, D11
  und D12 (§2) sowie D23, D24 und D26 bis D32 (Klärungen unten), die Lehren aus haex-sync-server (§3.4), die Bausteine in §5,
  das Relay in §10, Speicher-Backend A in §12 und das Bedrohungsmodell in §13.
  Offene Punkte 5 und 6 aus §15 sind unten als Klärungsbedarf markiert.
- Früheres Design
  [`docs/plans/2026-09-07-cross-user-sharing-deferred-design.md`](../../docs/plans/2026-09-07-cross-user-sharing-deferred-design.md):
  Das Relay dieser Spec ist das dortige „Buffer Relay“ (§3): kein Peer, keine
  Passphrase, kein Anwenden von Änderungen. Die Mitgliederliste ist die dortige
  vom Relay lesbare, signierte Projektion der Mitgliedschaft (§4), mit
  Generation, Ablaufzeit und Ablehnung bei fehlender Liste. Die dortige
  Ablehnung gleicher Generation mit anderem Inhalt ersetzt diese Spec durch
  eine feste Regel, weil mehrere Geräte des Admins Listen veröffentlichen
  (FR-018). Die Mindest-Generation, die ein Client
  verlangen darf (§4 dort), übernimmt FR-020.
- Referenz haex-sync-server, Repository
  `https://github.com/haex-space/haex-sync-server` @
  `e20abeb26991d187fe1a4a8f1bc69029e5b12177`: Übernommen wird die Form (ein
  Dienst für viele Nutzer, der Chiffrat speichert und ein iroh-Relay
  mitbetreibt). Nicht übernommen werden die Zellen mit Klartext-Metadaten
  (`src/db/schema.ts`), das Zusammenführen auf dem Server (`src/routes/sync.ts`),
  Konten mit E-Mail-Adresse, UCAN ohne Widerruf (`src/middleware/ucanAuth.ts`)
  und ein Speicher-Bucket je Nutzer mit vollen Zugangsdaten beim Server
  (`src/routes/storage.ts`).
- **Spec 024** (Vault-Identität, Geräteschlüssel, direkter Sync eigener Geräte):
  liefert Vault-Identität, Geräteschlüssel, Hauptgeräte und verknüpfte Geräte,
  die Geräteliste mit Verknüpfen und Entfernen von Geräten, das Format der
  Änderungspakete, Fortschrittsstände und die Nur-direkt-Daten; das ist nur
  noch der private Schlüssel der Vault-Identität (D30). Diese Spec erweitert
  den Sync eigener Geräte um das Postfach der Vault auf einem Relay und bringt
  die Wiederherstellung einer Vault über das Relay mit (User Story 9). Der
  direkte Sync bleibt der erste Weg; das Relay ist ein zusätzlicher.
- **Spec 025** (Dateisync eigener Geräte): Die Objekte eigener
  synchronisierter Ordner aus 025 dürfen mit dieser Spec über das
  Speicher-Backend A laufen (FR-048), sodass auch Dateien zwischen eigenen
  Geräten ankommen, die nie gleichzeitig online sind. Der Dateiindex reist im
  Postfach der Vault (FR-032).
- **Spec 023** (Einstellungs-App): Die Relays bekommen die Unteransicht „Relays“
  in der Kategorie „Föderation“ (FR-038). Es gelten die Regeln von 023: Werte
  gelten für die Vault, keine Knöpfe zum Speichern (FR-021 und FR-024 dort).
- Vorwärtsverweise: **Spec 027** (Spaces) nutzt Postfächer, Mitgliederlisten
  und Speicher-Backend A, und bringt die Verwaltung der Mitglieder, Einladungen
  und neue Generationen der Inhaltsschlüssel mit. **Spec 028** (Datenfreigaben)
  nutzt Postfächer und Mitgliederlisten. Spec 027 legt auch fest, wie
  Einladungen transportiert werden. **Spec 029** (eigener S3-Speicher)
  läuft ohne das Relay. Dessen Zugangsdaten liegen im Passwortmanager ([Spec 034](../034-password-manager/spec.md)) und reisen als gewöhnliche Daten der Vault nur verschlüsselt im
  Postfach der Vault (D30); lesbar bekommt das Relay sie nie.

## Clarifications

### Session 2026-09-27

- Q: Wer identifiziert sich am Relay, ein Gerät oder eine Vault? → A: Ein
  Nostr-Schlüssel steht für ein Gerät; dieselbe Vault läuft auf mehreren
  Geräten mit derselben Vault-Identität (D1). Ein Gerät meldet sich mit seinem
  Geräteschlüssel an; für welche Vault es handelt, zeigt die Geräteliste der
  Vault, auf der es stehen muss (überholt durch D27 in diesem Teil). Rechte,
  Zulassung und Kontingent hängen an der Vault.
- Q: Wer darf in einem Space oder einer Datenfreigabe einladen und Rechte
  ändern? → A: Nur der Admin, also die Vault, die den Bereich angelegt hat
  (D6). Empfänger können
  nicht weiterteilen (D7). Darum gibt es je Bereich genau eine Stelle, die eine
  Mitgliederliste signiert.

### Session 2026-09-28

- Q: Was darf der Betreiber des Relays sehen? → A: Für v1 gilt „der Betreiber
  sieht keine Inhalte“ (D9). Er sieht IP-Adressen, Zeitpunkte, Größen,
  Bereichskennungen, die Mitgliederlisten (Vault-Identitäten und Fähigkeiten)
  und über die Gerätelisten die Geräte jeder Vault mit Geräteschlüssel, Rolle,
  Name und Netzkennung (überholt durch D27 in diesem Teil). Wer den zweiten
  Faktor der Wiederherstellung per E-Mail wählt, gibt dem Relay seine Adresse
  (D31). Pseudonyme je Bereich, die auch die Teilnehmer verbergen, sind nicht
  v1.
- Q: Wie weit wird dem Relay vertraut? → A: Gar nicht (D11). Es kann Daten
  löschen, zurückhalten oder veraltete Stände liefern, aber weder lesen noch
  fälschen. Es erhält die S3-Zugangsdaten eines Nutzers nie lesbar; im
  Postfach der Vault reisen sie nur verschlüsselt (überholt durch D30 in diesem
  Teil).
- Q: Welche Speicher gibt es in v1? → A: Beide (D12): den vom Relay-Betreiber
  gestellten (Speicher-Backend A, diese Spec) und den eigenen S3-Speicher des
  Nutzers (Speicher-Backend B, Spec 029).
- Q: Warum braucht das Relay keine UCAN, wie haex-sync-server sie nutzt? → A:
  Der Wert von UCAN sind prüfbare Ketten von Weitergaben. Weil nur der Admin
  einlädt und niemand weitergibt (D6, D7), hat jede Kette genau ein Glied
  (Admin → Mitglied), und eine UCAN mit einem Glied ist nichts anderes als eine
  signierte Freigabe. Hält das Relay die aktuelle, vom Admin signierte Liste,
  wirkt ein Entzug sofort, sobald der Admin eine neue Liste hochlädt; eine UCAN
  bliebe bis zu ihrem Ablauf gültig und bräuchte eine eigene Widerrufsliste.
  Die Liste nutzt dieselben secp256k1-Schlüssel wie Nostr statt eines zweiten
  Schlüsselsystems. Es bleibt ein System von Fähigkeiten, nur ohne Ketten.
- Q: Kann holzi den haex-sync-server übernehmen? → A: Die Form ja, das
  Datenmodell nein (Design §3.4). Der Server darf weder das Schema der App noch
  die Aktivität einzelner Geräte sehen und führt nichts zusammen.
- Q: Braucht ein Relay Konten? → A: Nein. Keine Konten, keine E-Mail-Adresse;
  eine Identität ist ein Schlüssel. Aufgenommen wird in v1 über einen
  Einladungscode des Betreibers, Bezahlung kommt vielleicht später (Design
  §10.2). Einzige Ausnahme ist die freiwillige E-Mail-Adresse als zweiter
  Faktor der Wiederherstellung (überholt durch D31 in diesem Teil).
- Q: Darf ein Bereich gleichzeitig auf mehreren Relays liegen? → A: Nein, nicht in v1. Jeder Bereich hat ein Heimat-Relay; weitere Relays dienen dem NAT-Durchgang (FR-040). Das gilt auch für Spaces (Spec 027).
- Q: Wer darf eine Momentaufnahme hochladen? → A: Nur der Admin des Bereichs, also jedes Gerät auf seiner Geräteliste (D29); beim Bereich „Vault“ jedes eigene Gerät (FR-022).
- Q: Kann die Admin-Rolle übertragen werden? → A: Nein, in v1 gar nicht, und
  nichts hängt von der Zustimmung der Mitglieder ab (D23). Das Relay bindet
  einen Bereich nie an eine andere Vault-Identität (FR-021). Der frühere Teil
  dieser Antwort zum Beenden und Verlassen aller Bereiche bei einem Wechsel
  der Vault-Identität ist überholt durch D26.
- Q: Wie kommen Geräte bei Speicher-Backend A an die Objekte? → A: Das Relay
  überträgt die verschlüsselten Objekte selbst; es gibt keine zeitlich
  begrenzten Links. Das Relay sieht dabei nur Ciphertext (D24).
- Q: Gibt es in v1 ein Rotieren der Vault-Identität? → A: Nein (D26). Ein
  verlorenes Gerät ist kein Problem, solange eine Kopie oder das Relay existiert
  und die Passphrase hält; ausgesperrt wird ein Gerät über die Geräteliste
  (D27).
- Q: Wer darf Geräte hinzufügen und entfernen? → A: Nur Hauptgeräte, die den
  privaten Schlüssel der Vault-Identität haben; es kann mehrere geben. Beim
  Verknüpfen fragt holzi, ob der Schlüssel mit übertragen wird (Standard:
  nein). Verknüpfen ist der bevorzugte Weg, Kopieren der Datei bleibt möglich
  (D27).
- Q: Wie kommt ein Nutzer, der alle Kopien verloren hat, wieder an seine
  Vault? → A: Über ein optionales, verschlüsselt auf dem Relay hinterlegtes
  Wiederherstellungspaket. Abrufen erfordert den Besitznachweis des
  Wiederherstellungsschlüssels, ohne ihn zu übertragen, und einen zweiten
  Faktor (TOTP oder E-Mail-Link) (D31).
- Q: Was synchronisiert das Relay, und wo liegen Dateien? → A: Der Dienst des
  Relays synchronisiert nur SQLite-Daten in Postfächern; Dateien kommen nie in
  ein Postfach. Optional stellt der Betreiber zusätzlich S3-kompatiblen
  Speicher (Speicher-Backend A); dann prüft das Relay jeden Zugriff und
  überträgt jedes verschlüsselte Objekt selbst, und jedes Objekt liegt dort
  genau einmal. Mit eigenem S3 (Speicher-Backend B) ist das Relay an Dateien
  nicht beteiligt (D32).

## User Scenarios & Testing _(mandatory)_

### User Story 1 - Eigene Geräte gleichen sich über das Relay ab (Priority: P1)

Eine Nutzerin hat ihre Vault auf dem Laptop und dem Desktop. Der Laptop ist
tagsüber an, der Desktop nur abends, und nie sind beide gleichzeitig online.
Beide Geräte kennen dasselbe Relay. Was sie tagsüber auf dem Laptop ändert, hat
der Desktop am Abend, und umgekehrt, ohne dass die Geräte sich je direkt sehen.

**Why this priority**: Das ist der Grund, warum es das Relay für einen
einzelnen Nutzer überhaupt braucht. Der direkte Sync aus Spec 024 hilft nur,
wenn beide Geräte gleichzeitig laufen.

**Independent Test**: Zwei Geräte derselben Vault, nie gleichzeitig online, mit
demselben Relay. Auf A eine Änderung machen, A beenden, B starten: B zeigt die
Änderung. Danach die Relay-Ablage durchsuchen: Sie enthält keinen Klartext.

**Acceptance Scenarios**:

1. **Given** Gerät A und Gerät B derselben Vault kennen dasselbe Relay, **When**
   A eine Änderung macht, während B aus ist, und B später startet, während A
   aus ist, **Then** hat B die Änderung, sobald es das Relay erreicht hat.
2. **Given** A ist ohne Netz und sammelt Änderungen, **When** A wieder ein Relay
   erreicht, **Then** lädt A alle gesammelten Änderungen hoch, und kein anderes
   Gerät verliert dabei eine eigene Änderung.
3. **Given** A und B sind gleichzeitig online und direkt verbunden, **When** A
   eine Änderung macht, **Then** kommt sie direkt bei B an; dass sie zusätzlich
   im Postfach liegt, führt auf B nicht zu einer doppelten Anwendung oder
   einem zweiten Eintrag.
4. **Given** ein drittes, neu verknüpftes Gerät C, das ein Hauptgerät auf die
   Geräteliste gesetzt hat (Spec 024), **When** C das Relay zum ersten Mal
   abfragt, **Then** holt es die letzte Momentaufnahme und die Pakete danach
   und hat danach denselben Stand wie A und B.
5. **Given** ein Postfach der Vault, **When** jemand die Ablage des Relays
   untersucht, **Then** findet er weder den privaten Schlüssel der
   Vault-Identität noch einen privaten Geräteschlüssel, auch nicht
   verschlüsselt, in einem Paket, einer Momentaufnahme oder einem Objekt
   (FR-032); den privaten Schlüssel der Vault-Identität gibt es auf dem Relay
   nur verschlüsselt im Wiederherstellungspaket, wenn der Nutzer die
   Wiederherstellung eingerichtet hat (FR-052).
6. **Given** ein Hauptgerät hat Gerät B von der Geräteliste entfernt (Spec
   024), **When** das Relay die neue Geräteliste angenommen hat und B danach
   eine Anfrage stellt, **Then** lehnt das Relay sie ab, auch auf einer schon
   offenen Verbindung, und liefert B nichts mehr aus (FR-049).

---

### User Story 2 - Ein Relay in den Einstellungen eintragen (Priority: P1)

Der Nutzer bekommt vom Betreiber eines Relays eine Adresse und einen
Einladungscode. In den Einstellungen unter „Föderation“ öffnet er „Relays“,
wählt „Relay hinzufügen“, gibt beides ein und sieht danach das Relay in der
Liste mit dem Zustand „Verbunden“ und dem belegten Anteil seines Kontingents.
Auf seinen anderen Geräten erscheint das Relay von selbst.

**Why this priority**: Ohne eingetragenes Relay gibt es kein Postfach. Der
Schritt muss ohne Konto, E-Mail-Adresse oder Passwort gehen.

**Independent Test**: Ein laufendes Relay mit einem frischen Einladungscode;
in holzi Adresse und Code eintragen. Das Relay erscheint als verbunden; auf
einem zweiten Gerät der Vault steht es ohne weitere Eingabe in der Liste; ein
zweiter Versuch mit demselben Code aus einer anderen Vault wird abgelehnt.

**Acceptance Scenarios**:

1. **Given** keine Relays sind eingetragen, **When** der Nutzer „Relays“
   öffnet, **Then** sieht er eine leere Liste mit einer Zeile, die erklärt,
   dass eigene Geräte dann nur direkt synchronisieren, und den Knopf „Relay
   hinzufügen“.
2. **Given** eine gültige Adresse und ein unbenutzter Einladungscode, **When**
   der Nutzer beides einträgt und bestätigt, **Then** ist die Vault am Relay
   zugelassen, das Postfach der Vault ist angelegt, und das Relay steht mit
   Zustand und Kontingent in der Liste.
3. **Given** ein falscher, abgelaufener oder schon benutzter Einladungscode,
   **When** der Nutzer ihn einträgt, **Then** wird das Relay nicht hinzugefügt,
   und das Feld nennt den Grund.
4. **Given** das Relay ist auf einem Gerät eingetragen, **When** ein anderes
   Gerät derselben Vault synchronisiert, **Then** kennt es das Relay ebenfalls
   und meldet sich ohne Einladungscode an.
5. **Given** ein eingetragenes Relay, **When** der Nutzer es entfernt und
   bestätigt, **Then** nutzt keines seiner Geräte es mehr, und das Postfach
   der Vault auf diesem Relay wird gelöscht, soweit das Relay erreichbar ist.
6. **Given** die Vault ist zugelassen, **When** sich später die Identität des
   Relays hinter derselben Adresse ändert, **Then** synchronisiert holzi nicht
   still weiter, sondern meldet das Relay als „Identität geändert“.

---

### User Story 3 - Ein Relay betreiben, ohne Inhalte zu sehen (Priority: P1)

Eine Betreiberin stellt ein Relay für ihre Familie und einen Verein auf. Sie
startet den Dienst auf einem eigenen Server, erzeugt Einladungscodes mit einem
Kontingent und gibt sie weiter. Sie sieht, welche Vault-Identitäten zugelassen
sind und wie viel Platz sie belegen, aber nicht, was darin steht, wie Tabellen
oder Dateien heißen oder wer was geändert hat.

**Why this priority**: Ohne betreibbares Relay gibt es für holzi-Nutzer keinen
Weg zwischen Geräten, die nie gleichzeitig online sind. Dass der Betreiber
keine Inhalte sieht, ist die zentrale Zusage (D9).

**Independent Test**: Ein Relay aufsetzen, zwei Vaults zulassen, in beiden
Daten mit bekannten Markierungswörtern in Inhalten, Tabellen und Dateinamen
synchronisieren. Die gesamte Ablage des Relays und des Objektspeichers
durchsuchen: kein Treffer.

**Acceptance Scenarios**:

1. **Given** ein frischer Server, **When** die Betreiberin das Relay mit seiner
   Konfiguration startet, **Then** laufen Postfächer, Relay für den
   NAT-Durchgang und, falls eingeschaltet, Speicher-Backend A und
   Signalisierung, ohne dass ein Konto, eine E-Mail-Adresse oder ein fremder
   Dienst nötig ist.
2. **Given** ein laufendes Relay, **When** die Betreiberin einen Einladungscode
   mit einem Kontingent erzeugt, **Then** kann damit genau eine Vault-Identität
   zugelassen werden.
3. **Given** zugelassene Vaults, **When** die Betreiberin die Übersicht
   aufruft, **Then** sieht sie je Vault-Identität den belegten Platz und das
   Kontingent, und nichts über Inhalte.
4. **Given** eine zugelassene Vault, **When** die Betreiberin ihre Zulassung
   entzieht, **Then** werden weitere Uploads für die Bereiche dieser Vault
   abgelehnt, und die betroffenen Geräte zeigen den Grund an.
5. **Given** Daten mehrerer Vaults, **When** die Ablage untersucht wird,
   **Then** enthält sie keinen Klartext, keine Tabellen- oder Spaltennamen,
   keine Primärschlüssel, keine Zeitstempel der Änderungen und keine
   Dateinamen (SC-002).

---

### User Story 4 - Zugriff am Relay über die Mitgliederliste steuern (Priority: P2)

Der Admin eines Bereichs (später ein Space aus Spec 027 oder eine
Datenfreigabe aus Spec 028) lädt eine signierte Mitgliederliste zum Relay. Nur
die Vaults darin dürfen das Postfach lesen, nur die mit Schreiben dürfen
hineinschreiben. Entfernt der Admin eine Vault aus der Liste, wird sie beim
nächsten Zugriff abgewiesen.

**Why this priority**: Der Zugang am Relay ist die Grundlage für die geteilten
Bereiche aus 027 und 028. Er ist nur eine zusätzliche Hürde; die eigentlichen
Regeln prüfen die Empfänger. Für den Bereich „Vault“ allein würde eine Liste
mit nur der eigenen Vault genügen.

**Independent Test**: Mit einem Test-Bereich und drei Vaults: Liste mit A
(Admin), B (Lesen) und C (Lesen, Schreiben) hochladen. B kann lesen, aber
nicht schreiben; C kann beides. Neue Liste ohne C hochladen: C wird beim
nächsten Zugriff abgewiesen. Eine alte Liste, in der C noch steht, erneut
hochladen: wird abgelehnt.

**Acceptance Scenarios**:

1. **Given** eine gültige Mitgliederliste, in der eine Vault Lesen hat, **When**
   ein Gerät dieser Vault, das auf ihrer aktuellen Geräteliste steht, Pakete
   abruft,
   **Then** liefert das Relay sie aus.
2. **Given** dieselbe Vault ohne Schreiben, **When** ihr Gerät ein Paket
   hochladen will, **Then** lehnt das Relay ab und nennt „keine Berechtigung“.
3. **Given** der Admin lädt eine Liste mit höherer Generation ohne diese Vault
   hoch, **When** das Gerät der Vault danach irgendeine Anfrage für den Bereich
   stellt, **Then** wird sie abgelehnt, ohne Wartezeit.
4. **Given** eine Liste der Generation 5 gilt, **When** jemand eine signierte
   Liste der Generation 4 hochlädt, **Then** lehnt das Relay sie ab, und die
   Liste der Generation 5 gilt weiter.
5. **Given** die gültige Liste ist abgelaufen und der Admin hat keine neue
   hochgeladen, **When** irgendein Gerät auf den Bereich zugreift, **Then**
   lehnt das Relay Lesen und Schreiben ab, bis eine gültige Liste vorliegt.
6. **Given** eine Liste, die kein Gerät auf der aktuellen Geräteliste des
   Admins signiert hat, **When** sie hochgeladen wird, **Then** lehnt das Relay
   sie ab, auch wenn für den Bereich noch keine Liste existiert.
7. **Given** eine Liste der Generation 5 gilt, **When** ein anderes Gerät des
   Admins eine andere gültige Liste der Generation 5 hochlädt, **Then** ersetzt
   das Relay die geltende Liste nur, wenn der Hash der neuen kleiner ist, und
   lehnt sie sonst ab; ein Gerät des Admins, das beide Listen sieht,
   veröffentlicht danach eine Liste der Generation 6, die beide Änderungen
   vereint (FR-018, FR-041).
8. **Given** der Admin hat den Bereich mit einer signierten Erklärung beendet,
   **When** danach irgendein Gerät eine Liste, ein Paket oder eine
   Momentaufnahme für den Bereich hochlädt, auch ein Gerät des Admins,
   **Then** lehnt das Relay mit „vom Admin beendet“ ab; Mitglieder der letzten
   gültigen Liste können bis zu deren Ablauf noch lesen, danach löscht das
   Relay das Postfach (FR-021).

---

### User Story 5 - Postfächer wachsen nicht endlos (Priority: P2)

Die Vault eines Nutzers ändert täglich dieselben Einträge. Nach einiger Zeit
lädt eines seiner Geräte eine Momentaufnahme des Bereichs hoch; das Relay
verwirft die älteren Pakete. Ein neues Gerät muss danach nicht mehr jede
einzelne Änderung seit Anbeginn holen.

**Why this priority**: Ohne Kompaktierung füllt jedes Postfach sein Kontingent
irgendwann mit überholten Änderungen, und neue Geräte brauchen immer länger.

**Independent Test**: 10.000 Änderungen an denselben 100 Einträgen über das
Relay synchronisieren, dann eine Momentaufnahme auslösen. Der belegte Platz des
Postfachs sinkt deutlich; ein neues Gerät erreicht aus Momentaufnahme und
restlichen Paketen denselben Stand.

**Acceptance Scenarios**:

1. **Given** ein Postfach mit vielen Paketen, **When** die Menge der Pakete eine
   Schwelle überschreitet, **Then** erstellt ein berechtigtes Gerät eine
   Momentaufnahme bis zu seinem Lesestand und lädt sie hoch.
2. **Given** eine angenommene Momentaufnahme bis Nummer n, **When** ein Gerät
   mit Lesestand kleiner n abruft, **Then** bekommt es die Momentaufnahme und
   danach die Pakete nach n.
3. **Given** eine Momentaufnahme, **When** ein Empfänger sie prüft, **Then**
   prüft er jede darin enthaltene Transaktion (Spec 024) mit den ursprünglichen
   Signaturen ihrer Änderungen wie bei einem Paket; enthält eine Transaktion
   eine Änderung, die das Gerät, das die Momentaufnahme erstellt hat,
   verfälscht oder für ein anderes Mitglied erfunden hat, wird die ganze
   Transaktion verworfen, und die übrigen Transaktionen bleiben erhalten.
4. **Given** ein Gerät besitzt Änderungen, die weder in der neuen
   Momentaufnahme noch in späteren Paketen stehen, **When** es die
   Momentaufnahme sieht, **Then** lädt es diese Änderungen erneut hoch.
5. **Given** während der Erstellung einer Momentaufnahme kommen neue Pakete an,
   **When** die Momentaufnahme angenommen wird, **Then** bleiben alle Pakete
   nach ihrer Nummer erhalten.

---

### User Story 6 - Sehen, wenn das Relay nicht mitspielt (Priority: P2)

Ein Relay ist nicht erreichbar, lehnt ab, hat ein Postfach verloren oder
liefert Pakete nur lückenhaft. holzi bleibt voll nutzbar, synchronisiert direkt
weiter, wo das geht, und zeigt beim Relay in den Einstellungen, was los ist.
Es übernimmt nichts, was nicht geprüft ist, und verliert nichts, was die
eigenen Geräte schon haben.

**Why this priority**: Das Relay kann Daten zurückhalten, verhindern lässt sich
das nicht (Design §13). Der Nutzer muss es aber merken, und holzi muss sich
selbst wieder heilen können.

**Independent Test**: Mit einem Test-Relay, das auf Befehl nicht antwortet,
Pakete auslässt, ein Postfach löscht oder eine alte Mitgliederliste verwendet:
holzi zeigt jeweils den passenden Zustand beim Relay, arbeitet lokal weiter und
füllt ein gelöschtes Postfach wieder.

**Acceptance Scenarios**:

1. **Given** das Relay ist nicht erreichbar, **When** der Nutzer weiterarbeitet,
   **Then** gibt es keinen blockierenden Dialog; Änderungen sammeln sich lokal,
   und die Relay-Liste zeigt „Nicht erreichbar seit …“.
2. **Given** das Relay liefert nach Nummer 10 die Nummer 12, **When** holzi die
   Lücke bemerkt, **Then** übernimmt es die geprüften Pakete, hält seinen
   Lesestand vor der Lücke, fragt erneut an und zeigt „Unvollständige Daten“,
   wenn die Lücke bleibt.
3. **Given** das Relay meldet ein Postfach als unbekannt oder mit neu
   begonnener Nummerierung, **When** holzi einen höheren Lesestand hatte,
   **Then** meldet es „Daten auf dem Relay verloren“ und lädt eine neue
   Momentaufnahme hoch, wenn es dazu berechtigt ist.
4. **Given** der Fortschrittsstand eines Pakets nennt Änderungen, die dieses
   Gerät weder hat noch im Postfach findet, **When** das länger als eine Frist
   so bleibt, **Then** zeigt holzi, dass das Relay Daten zurückhält.
5. **Given** das Relay lehnt ab (nicht zugelassen, keine Berechtigung, Liste
   fehlt oder abgelaufen, Kontingent erschöpft, Version nicht unterstützt),
   **When** holzi das erfährt, **Then** zeigt die Relay-Liste genau diesen
   Grund und, wo möglich, was der Nutzer tun kann.

---

### User Story 7 - Dateien über den Speicher des Relays (Priority: P3)

Die verschlüsselten Objekte aus dem Dateisync (Spec 025) und später aus Spaces
(Spec 027) liegen beim Speicher-Backend A, das der Betreiber des Relays
zusätzlich stellt. Das Relay prüft bei jeder Anfrage die geltende
Mitgliederliste und überträgt das Objekt selbst zwischen diesem Speicher und
dem Gerät; es sieht dabei nur Chiffrat. Jedes Objekt liegt dort genau einmal;
in ein Postfach kommen Dateien nie, dort reist nur der Dateiindex. Löschen darf
ein Objekt nur, wer es hochgeladen hat, wer Löschen hat, oder der Admin.

**Why this priority**: Dateien zwischen Geräten, die nie gleichzeitig online
sind, brauchen einen Speicher. Speicher-Backend B (Spec 029) ist die
Alternative für Nutzer mit eigenem S3.

**Independent Test**: Ein Objekt über Speicher-Backend A hochladen, auf einem
zweiten Gerät herunterladen und den Hash prüfen. Mit einer Vault ohne Löschen,
die das Objekt nicht hochgeladen hat, das Objekt löschen: abgelehnt. Eine
Vault aus der Liste entfernen und mit ihrem Gerät das Objekt erneut anfordern:
sofort abgelehnt.

**Acceptance Scenarios**:

1. **Given** eine Vault mit Schreiben im Bereich, **When** ihr Gerät ein neues
   Objekt hochladen will, **Then** nimmt das Relay genau dieses Objekt über
   seinen Endpunkt an, legt es in seinem Speicher ab und vermerkt die
   hochladende Vault.
2. **Given** eine Vault mit Lesen, **When** ihr Gerät ein Objekt anfordert,
   **Then** prüft das Relay die geltende Liste und überträgt das Objekt aus
   seinem Speicher an das Gerät.
3. **Given** ein Objekt existiert schon, **When** jemand es unter derselben
   Kennung erneut hochladen will, **Then** wird es nicht überschrieben.
4. **Given** eine Vault, die das Objekt nicht hochgeladen hat, weder Löschen
   hat noch Admin ist, **When** sie es löschen will, **Then** lehnt das Relay
   ab.
5. **Given** ein Gerät lädt gerade ein Objekt herunter, **When** das Relay
   eine Liste annimmt, die seine Vault entfernt, **Then** bricht das Relay die
   laufende Übertragung ab und bedient keine weitere (FR-023).
6. **Given** eine Vault, die inzwischen nicht mehr in der Liste steht, **When**
   ihr Gerät ein Objekt hoch- oder herunterladen will, **Then** lehnt das
   Relay sofort ab; es gibt keinen vorher ausgestellten Zugang, der noch
   weiter funktioniert (FR-026).

---

### User Story 8 - Kontingente und volle Postfächer (Priority: P3)

Das Kontingent einer Vault ist fast voll. holzi versucht zuerst, Platz durch
Momentaufnahmen zurückzugewinnen, und zeigt dann in der Relay-Liste
„Kontingent fast voll“ oder „voll“. Lesen geht weiter, neue Uploads nicht.

**Why this priority**: Kontingente schützen den Betreiber; der Nutzer soll ein
volles Kontingent nicht als unerklärlichen Sync-Ausfall erleben.

**Independent Test**: Eine Vault mit kleinem Kontingent zulassen und füllen:
holzi zeigt den Stand, eine Momentaufnahme wird trotz vollem Kontingent
angenommen, wenn sie Platz frei macht, weitere Pakete werden mit „Kontingent
erschöpft“ abgelehnt, andere Vaults auf demselben Relay sind nicht betroffen.

**Acceptance Scenarios**:

1. **Given** der belegte Platz überschreitet 80 % des Kontingents, **When** die
   Relay-Liste angezeigt wird, **Then** steht dort „Kontingent fast voll“ mit
   dem Stand.
2. **Given** das Kontingent ist erschöpft, **When** ein Gerät ein Paket
   hochladen will, **Then** lehnt das Relay mit „Kontingent erschöpft“ ab,
   Abrufen funktioniert weiter.
3. **Given** das Kontingent ist erschöpft, **When** eine Momentaufnahme
   hochgeladen wird, die nach dem Verwerfen der alten Pakete weniger Platz
   belegt, **Then** nimmt das Relay sie an.
4. **Given** eine Vault hat ihr Kontingent erschöpft, **When** eine andere Vault
   auf demselben Relay hochlädt, **Then** ist sie davon nicht betroffen.

---

### User Story 9 - Vault wiederherstellen, wenn alle Kopien verloren sind (Priority: P2)

Ein Nutzer hat seine Vault nur auf dem Laptop, und der Laptop wird gestohlen.
Beim Einrichten hatte er auf einem Hauptgerät die Wiederherstellung
eingeschaltet, den Wiederherstellungsschlüssel als Code ausgedruckt und eine
Authenticator-App als zweiten Faktor eingerichtet. Auf einem neuen Rechner
installiert er holzi, wählt „Vault wiederherstellen“, gibt den Code ein und
bestätigt mit einem Code aus der App. holzi entschlüsselt das Paket auf dem
neuen Rechner, holt den Stand aus dem Postfach der Vault, lässt ihn eine neue
Passphrase setzen und entfernt den gestohlenen Laptop aus der Geräteliste.

**Why this priority**: Ohne diesen Weg ist eine Vault verloren, sobald alle
Kopien weg sind; das Postfach allein hilft nicht, weil nur Geräte der Vault es
entschlüsseln können. Die Funktion ist freiwillig und setzt ein Relay voraus,
darum nach dem Sync (User Story 1 bis 3).

**Independent Test**: Auf einem Hauptgerät mit Relay die Wiederherstellung mit
TOTP einrichten, Daten ändern, eine neue Generation des Inhaltsschlüssels
auslösen, dann alle Geräte löschen. Auf einem frischen Gerät mit Code und
TOTP-Code wiederherstellen: Die Vault hat den letzten Stand des Postfachs, das
neue Gerät ist Hauptgerät, und das Relay lehnt die alten Geräte ab. Danach die
Ablage des Relays durchsuchen: kein Wiederherstellungsschlüssel und nichts,
womit sich das Paket entschlüsseln ließe. Abrufe ohne zweiten Faktor oder mit
falschem Code schlagen fehl, und nach wiederholten Fehlversuchen sperrt das
Relay.

**Acceptance Scenarios**:

1. **Given** ein Hauptgerät mit einem Relay, **When** der Nutzer in den
   Einstellungen „Wiederherstellung einrichten“ wählt, **Then** erklärt holzi,
   was hinterlegt wird, erzeugt einen Wiederherstellungsschlüssel, zeigt ihn
   genau einmal als Code und QR-Code und schaltet die Wiederherstellung erst
   ein, wenn der Nutzer den zweiten Faktor eingerichtet und bestätigt hat
   (FR-050).
2. **Given** das Einrichten, **When** der Nutzer den zweiten Faktor wählt,
   **Then** ist TOTP vorausgewählt und wird erst aktiv, wenn er einen gültigen
   Code aus seiner App eingibt; wählt er stattdessen E-Mail, sagt holzi vorher,
   dass das Relay dann seine Adresse kennt (FR-051).
3. **Given** die Wiederherstellung ist eingerichtet, **When** ein Hauptgerät
   eine neue Generation des Inhaltsschlüssels des Bereichs „Vault“ erzeugt oder
   sich die Liste der Relays ändert, **Then** verschlüsselt es das Paket neu und
   lädt es hoch, ohne den Nutzer zu fragen und ohne den
   Wiederherstellungsschlüssel zu kennen (FR-052).
4. **Given** ein frisches Gerät ohne Vault, **When** der Nutzer „Vault
   wiederherstellen“ wählt und den Code eingibt, **Then** weist das Gerät dem
   Relay den Besitz des Schlüssels mit einer Signatur über eine Challenge
   nach, ohne etwas Geheimes zu senden, und verlangt danach den zweiten Faktor
   (FR-053).
5. **Given** ein richtiger Besitznachweis, **When** der zweite Faktor fehlt,
   falsch oder abgelaufen ist, **Then** liefert das Relay das Paket nicht aus;
   nach wiederholten Fehlversuchen sperrt es weitere Versuche für eine
   wachsende Frist und zeigt holzi, bis wann (FR-053).
6. **Given** Besitznachweis und zweiter Faktor sind richtig, **When** das
   Relay das Paket ausliefert, **Then** entschlüsselt holzi es nur auf dem
   Gerät, lässt den Nutzer eine neue Passphrase setzen, holt die letzte
   Momentaufnahme und die Pakete danach aus dem Postfach der Vault und hat
   danach deren Stand (FR-054).
7. **Given** die wiederhergestellte Instanz, **When** holzi die Geräteliste
   veröffentlicht, **Then** ist die neue Instanz darin ein Hauptgerät, die
   bisherigen Geräte sind zum Entfernen vorausgewählt, der Nutzer entscheidet
   je Gerät, und das Relay lehnt jedes entfernte Gerät danach ab (FR-049,
   FR-054).
8. **Given** jemand hat den Code, aber nicht den zweiten Faktor, oder den
   zweiten Faktor, aber nicht den Code, **When** er abzurufen versucht,
   **Then** erhält er weder das Paket noch eine Aussage darüber, ob es zu dem
   Code ein Paket gibt (FR-053).

---

### Edge Cases

- **Das Relay löscht ein Postfach oder Pakete.** Die Geräte merken es an der
  Nummerierung (FR-043) und füllen das Postfach aus ihrem eigenen Stand wieder
  (FR-044). Die Wahrheit liegt in den Vaults; das Relay ist nur Zwischenablage.
  Was nur im Postfach lag und noch auf keinem Gerät angekommen ist, kann
  verloren sein; das Gerät, das es hochgeladen hat, lädt es erneut hoch, wenn
  es das Fehlen bemerkt (FR-042).
- **Das Relay hält Daten zurück oder zeigt verschiedenen Geräten verschiedene
  Stände.** Lücken in der Nummerierung und Fortschrittsstände in den Paketen
  zeigen es an (FR-043); spätestens beim direkten Sync eigener Geräte fallen
  abweichende Stände auf. Verhindern lässt es sich nicht (Design §13).
- **Jemand spielt eine alte Mitgliederliste ein.** Eine niedrigere Generation
  wird abgelehnt (FR-018). Ein Client verlangt die Generation, die er kennt, als
  Mindestwert (FR-020); bedient ein Relay mit einer älteren Liste, erfährt der
  Client das. Ein Relay, das selbst eine alte Liste verwendet, kann nur einem
  entfernten Mitglied Chiffrat ausliefern; neue Inhalte kann es damit nicht
  lesen, weil der Admin nach dem Entfernen eine neue Generation des
  Inhaltsschlüssels erzeugt (Spec 027).
- **Die Mitgliederliste läuft ab.** Das Relay lehnt dann jeden Zugriff auf den
  Bereich ab (FR-019). holzi erneuert die Listen der Bereiche, deren Admin die
  Vault ist, rechtzeitig vorher (FR-041). Ist kein Gerät des Admins lange
  genug online, ist der Bereich am Relay nicht erreichbar, bis eines wieder
  online kommt; direkter Sync bleibt möglich.
- **Kontingent voll.** Siehe User Story 8. Postfächer geteilter Bereiche
  zählen beim Admin (FR-028); ein Mitglied kann das Kontingent des Admins durch
  Hochladen füllen, aber nur, solange es Schreiben hat.
- **Das Relay ist kompromittiert.** Ein Angreifer erhält Chiffrat,
  Mitgliederlisten, Gerätelisten, Wiederherstellungspakete samt der
  Einrichtung des zweiten Faktors, IP-Adressen und Zeitpunkte. Er kann nichts
  lesen, nichts fälschen, was ein Empfänger annimmt, keine S3-Zugangsdaten
  eines Nutzers lesen (sie liegen nur verschlüsselt im Postfach der Vault) und
  keine Inhaltsschlüssel erhalten. Den zweiten Faktor prüft das Relay selbst,
  also kann es ihn übergehen; das Paket bleibt trotzdem verschlüsselt, und
  wegen der hohen Entropie des Wiederherstellungsschlüssels lässt es sich
  nicht durch Raten öffnen (FR-050). Er kann Daten löschen, zurückhalten,
  Anfragen ablehnen oder unberechtigte Anfragen durchlassen; durchgelassene
  Änderungen verwerfen die Empfänger (FR-025).
- **Ein Änderungspaket ist größer als die Obergrenze des Relays.** Pakete teilen
  nie eine zusammengehörige Änderungsgruppe (Spec 024). Ist schon eine einzelne
  Gruppe zu groß, lädt holzi sie nicht hoch, zeigt es beim Relay an und
  synchronisiert sie weiter direkt.
- **Ein Gerät geht verloren oder wird gestohlen.** Ein Hauptgerät entfernt es
  aus der Geräteliste und erzeugt eine neue Generation des Inhaltsschlüssels
  des Bereichs „Vault“ (Spec 024). Sobald das Relay die neue Geräteliste
  angenommen hat, lehnt es jede Anfrage des Geräts ab, auch auf offenen
  Verbindungen (FR-049), und neue Änderungen kann es nicht mehr
  entschlüsseln. Was schon auf dem Gerät liegt, bleibt dort; ein Löschen aus
  der Ferne gibt es nicht. Bis ein Hauptgerät die neue Liste hochlädt, kann
  das Gerät weiter synchronisieren; das ist hingenommen. War das Gerät ein
  Hauptgerät und kennt der Dieb die Passphrase, ist die Vault verloren
  (Spec 024); das Relay kann ein ehrliches Hauptgerät nicht von einem
  kompromittierten unterscheiden.
- **Zwei Hauptgeräte veröffentlichen gleichzeitig verschiedene Gerätelisten
  derselben Generation.** Das Relay nimmt wie bei Mitgliederlisten die mit
  dem kleinsten Hash (FR-049); ein Hauptgerät, das beide sieht, veröffentlicht
  eine Liste der nächsten Generation, die beide Änderungen vereint (Spec 024).
- **Der Wiederherstellungsschlüssel ist verloren.** Solange ein Hauptgerät
  existiert, erzeugt der Nutzer dort einen neuen; der alte funktioniert danach
  nicht mehr (FR-055). Ist auch das letzte Gerät weg, gibt es keinen Weg
  zurück; das ist gewollt, weil sonst das Relay oder ein Dritter die Vault
  öffnen könnte.
- **Der zweite Faktor ist verloren** (Telefon mit der TOTP-App weg). Ein
  Hauptgerät richtet ihn neu ein (FR-055). Sind zweiter Faktor und alle
  Geräte weg, ist die Wiederherstellung nicht möglich.
- **Das Relay liefert ein älteres Wiederherstellungspaket oder hat es
  gelöscht.** Verhindern lässt sich das nicht (Design §13). Das Paket nennt
  seinen Stand; Änderungen im Postfach, die eine neuere Schlüsselgeneration
  brauchen, kann holzi dann nicht entschlüsseln und meldet das. Ein Hauptgerät
  prüft, ob das Paket auf dem Relay aktuell ist, und lädt es sonst neu hoch
  (FR-052).
- **Das Relay mit dem Wiederherstellungspaket wird entfernt.** holzi sagt vor
  der Bestätigung, dass die Wiederherstellung damit endet, und bietet an, sie
  auf dem neuen Heimat-Relay neu einzurichten (FR-046, FR-055).
- **Zwei Geräte des Admins veröffentlichen gleichzeitig verschiedene Listen
  derselben Generation.** Relay und Empfänger nehmen beide dieselbe, die mit
  dem kleinsten Hash (FR-018); ein Gerät des Admins, das den Konflikt sieht,
  veröffentlicht eine Liste der nächsten Generation, die beide Änderungen
  vereint (FR-041).
- **Zwei Geräte der Vault legen gleichzeitig eine Momentaufnahme an.** Das Relay
  nimmt nur eine an, deren Nummer über der geltenden liegt; die andere wird
  abgelehnt, das Gerät holt die neue Momentaufnahme und prüft FR-042.
- **Ein Relay wird entfernt, während es nicht erreichbar ist.** holzi nutzt es
  nicht mehr; das Postfach bleibt dort liegen, bis der Betreiber es aufräumt
  (FR-012) oder der Nutzer das Relay erneut einträgt und entfernt.
- **Das Relay ist Heimat eines Bereichs, dessen Admin diese Vault ist**
  (Space oder Datenfreigabe), und der Nutzer will es entfernen. holzi nennt die
  betroffenen Bereiche und verlangt eine Bestätigung; ein Umzug auf ein anderes
  Relay gehört zu Spec 027 bzw. 028.
- **Uhrzeiten weichen ab.** Ausstellungs- und Ablaufzeit der Liste prüft das
  Relay mit seiner Uhr und einer kleinen Toleranz; Sequenznummern hängen nicht
  an Uhrzeiten.

## Requirements _(mandatory)_

### Functional Requirements

**Relay-Server**

- **FR-001**: Das Relay MUSS ein eigenständiger Dienst ohne Oberfläche sein,
  den jeder selbst betreiben kann, und MUSS die Bereiche beliebig vieler,
  voneinander unabhängiger Vaults gleichzeitig bedienen.
- **FR-002**: Das Relay DARF NICHT als Teilnehmer eines Bereichs handeln: Es hat
  keine Vault, keine Passphrase und keinen Inhaltsschlüssel, entschlüsselt
  nichts und wendet keine Änderungen an.
- **FR-003**: Das Relay MUSS ohne Konten auskommen. Es DARF weder eine
  E-Mail-Adresse noch ein Passwort noch einen anderen Kontaktweg verlangen;
  eine Vault ist für das Relay ihre Vault-Identität. Einzige Ausnahme ist die
  E-Mail-Adresse, die ein Nutzer freiwillig als zweiten Faktor der
  Wiederherstellung angibt (FR-051); sie DARF zu nichts anderem dienen.
- **FR-004**: Ein Gerät MUSS sich am Relay anmelden, indem es den Besitz seines
  Geräteschlüssels in einer Challenge-Response beweist und die Vault nennt,
  für die es handelt. Das Relay MUSS prüfen, dass der Geräteschlüssel auf der
  aktuellen Geräteliste dieser Vault steht (FR-049), und das Gerät danach als
  Gerät dieser Vault behandeln. Eine Anfrage ohne gültige Anmeldung oder von
  einem Gerät, das nicht auf der aktuellen Geräteliste steht, MUSS abgelehnt
  werden.
- **FR-005**: Eine Vault-Identität MUSS über einen Einladungscode des
  Betreibers zugelassen werden. Ein Einladungscode MUSS genau eine
  Vault-Identität zulassen können, DARF eine Gültigkeitsdauer haben und trägt
  das Kontingent, das die Zulassung erhält. Ist der Code verbraucht, abgelaufen
  oder unbekannt, MUSS das Relay mit diesem Grund ablehnen.
- **FR-006**: Eine Zulassung MUSS nur nötig sein, um eigene Bereiche anzulegen
  (einschließlich des Postfachs der Vault). Ein Mitglied eines fremden
  Bereichs MUSS diesen ohne eigene Zulassung nutzen können.
- **FR-007**: Der Betreiber MUSS Einladungscodes erzeugen und zurückziehen,
  Kontingente ändern, Zulassungen entziehen und je Vault-Identität den
  belegten Platz sehen können. Diese Verwaltung DARF keine Inhalte, Namen von
  Tabellen oder Dateien und keine Zuordnung von Änderungen zu Geräten zeigen,
  weil das Relay sie nicht kennt.
- **FR-008**: Das Relay MUSS ein Relay für den NAT-Durchgang mitbetreiben,
  über das sich Geräte verbinden, die sich nicht direkt erreichen. Es DARF es
  auf angemeldete Geräte von Vaults beschränken, die zugelassen oder Mitglied
  eines Bereichs dort sind. Die Verbindungen darüber bleiben Ende-zu-Ende
  verschlüsselt.
- **FR-009**: Das Relay DARF optional eine Signalisierung für Anwesenheit und
  Einladungen anbieten (Spec 024, 027); wie Einladungen transportiert werden,
  legt Spec 027 fest, und holzi DARF NICHT voraussetzen, dass ein Relay sie
  anbietet. Ist sie eingeschaltet, MUSS sie
  schreibend nur angemeldeten Geräten offenstehen, und sie DARF NIE
  Änderungspakete tragen.
- **FR-010**: Relay und Client MÜSSEN beim Verbindungsaufbau ihre
  Protokollversion austauschen. Bei einer nicht unterstützten Version MUSS die
  Verbindung mit genau diesem Grund enden.
- **FR-011**: Das Relay MUSS die Bereiche verschiedener Vaults voneinander
  trennen: Eine Anfrage für einen Bereich DARF nie Daten, Listen oder
  Kennungen anderer Bereiche liefern oder deren Existenz verraten.
- **FR-012**: Der Betreiber DARF Postfächer löschen, deren Mitgliederliste seit
  einer von ihm festgelegten Frist abgelaufen ist, und Pakete, deren
  Zulassung entzogen wurde. Das betrifft nur die Verfügbarkeit; die Clients
  behandeln es wie Datenverlust (FR-043, FR-044).
- **FR-013**: Das Relay SOLLTE Anfragen je Gerät und je Vault-Identität
  begrenzen können, damit eine Vault die anderen nicht ausbremst.

**Postfächer**

- **FR-014**: Jeder Bereich MUSS auf einem Relay höchstens ein Postfach haben.
  Das Postfach MUSS angenommene Änderungspakete nur anhängen und jedem eine
  Sequenznummer geben, die je Postfach lückenlos um eins steigt. Das Relay MUSS
  dem hochladenden Gerät die vergebene Nummer zurückgeben.
- **FR-015**: Ein Gerät MUSS Pakete „nach Nummer n“ abrufen können, in
  aufsteigender Reihenfolge. Das Relay MUSS mit jeder Antwort eine Kennung der
  Nummerierung liefern, die sich ändert, wenn es die Nummerierung des
  Postfachs neu beginnt (etwa nach Datenverlust).
- **FR-016**: Das Relay DARF NICHT zusammenführen, umsortieren oder den Inhalt
  eines Pakets deuten. Es MUSS ein Paket nur als Bereichskennung,
  Schlüsselkennung und Chiffrat sehen; Tabellen- und Spaltennamen,
  Primärschlüssel, Zeitstempel der Änderungen, Autoren und Dateinamen liegen
  nur im Chiffrat.
- **FR-017**: Ein Gerät mit der nötigen Fähigkeit (FR-022) MUSS eine
  Momentaufnahme eines Bereichs bis zu einer Sequenznummer n hochladen können.
  Das Relay MUSS sie nur annehmen, wenn n über der Nummer der geltenden
  Momentaufnahme liegt und nicht über der höchsten vergebenen Nummer. Hat es sie
  angenommen, MUSS es die Pakete bis n verwerfen, die Pakete nach n behalten und
  einem Gerät mit Lesestand unter n zuerst die Momentaufnahme liefern. Die
  Nummerierung läuft danach unverändert weiter.

**Berechtigung**

- **FR-018**: Das Relay MUSS je Bereich die geltende Mitgliederliste halten.
  Eine Liste MUSS Bereichskennung, Generation, Einträge {Vault-Identität →
  Fähigkeiten}, Ausstellungszeit, Ablaufzeit und die Signatur eines Geräts des
  Admins tragen (D29). Eine gültige Liste mit höherer Generation MUSS die
  geltende ersetzen;
  eine mit niedrigerer Generation MUSS abgelehnt werden. Weil mehrere Geräte
  des Admins Listen veröffentlichen, können zwei verschiedene gültige Listen
  dieselbe Generation tragen. Dann gilt nach derselben Regel wie bei jedem
  Empfänger (Spec 024) die Liste mit dem lexikografisch kleinsten Hash der
  vollständigen, signierten Liste: Das Relay MUSS die geltende Liste durch eine
  andere gültige Liste derselben Generation genau dann ersetzen, wenn deren
  Hash kleiner ist, und MUSS sie sonst ablehnen.
- **FR-019**: Fehlt die Liste eines Bereichs, ist sie abgelaufen, noch nicht
  gültig oder ungültig signiert, MUSS das Relay jeden Zugriff auf den Bereich
  ablehnen (fail closed). Die Laufzeit einer Liste DARF die Höchstdauer, die
  der Betreiber festlegt, nicht überschreiten.
- **FR-020**: Ein Gerät DARF mit einer Anfrage die Generation der
  Mitgliederliste nennen, die es mindestens erwartet. Hält das Relay eine
  niedrigere, MUSS es das in der Antwort melden, statt die Anfrage mit der
  älteren Liste zu bedienen.
- **FR-021**: Die Bereichskennung MUSS an die Vault-Identität ihres Admins
  gebunden sein, sodass das Relay ohne Vorwissen prüfen kann, dass eine
  Liste vom Admin des Bereichs stammt: Sie MUSS von einem Geräteschlüssel
  signiert sein, der auf der aktuellen Geräteliste dieser Vault-Identität
  steht (FR-049, D29). Eine andere Signatur MUSS abgelehnt werden, auch für
  einen Bereich, der auf dem Relay noch nicht existiert. Die erste gültige
  Liste legt das Postfach an und verlangt die Zulassung des Admins (FR-006).
  Das Relay DARF einen Bereich nie an eine andere Vault-Identität binden; die
  Admin-Rolle lässt sich am Relay nicht übertragen. Der Admin MUSS einen
  Bereich mit einer signierten Erklärung beenden können, etwa „Datenfreigabe
  beenden“ (Spec 028 FR-036); eine Erklärung, die kein Gerät auf der
  aktuellen Geräteliste des Admins signiert hat, MUSS das Relay ablehnen. Das
  Beenden ist endgültig: Danach MUSS das Relay für diesen Bereich nichts mehr
  annehmen, keine Liste, kein Paket, keine Momentaufnahme und keinen Upload
  eines Objekts, auch nicht von einem Gerät des Admins. Abrufe der Mitglieder
  mit Lesen in der letzten gültigen Liste MUSS es weiter bedienen, solange
  diese Liste nicht abgelaufen ist (FR-019), und dabei die Erklärung
  mitliefern; jedes Mitglied prüft ihre Signatur selbst, zeigt „vom Admin
  beendet“, behält seine lokale Kopie und beendet den Sync des Bereichs. Läuft
  die letzte Liste ab, MUSS das Relay Postfach und Objekte des Bereichs
  löschen. Die Bereichskennung und die Erklärung MUSS es danach behalten,
  damit niemand den Bereich unter derselben Kennung neu anlegen kann, und MUSS
  jede weitere Anfrage dafür mit „vom Admin beendet“ ablehnen.
- **FR-049**: Das Relay MUSS je Vault-Identität, die bei ihm zugelassen ist
  oder in einer geltenden Mitgliederliste steht, die neueste gültige
  Geräteliste halten (Spec 024, D27). Ein Gerät MUSS sie bei der Anmeldung
  vorlegen oder eine neuere hochladen können. Das Relay MUSS eine Geräteliste
  nur annehmen, wenn die Vault-Identität sie signiert hat; eine höhere
  Generation MUSS die geltende ersetzen, eine niedrigere MUSS abgelehnt werden,
  Gerätenamen sieht das Relay nicht, sie sind in der Liste verschlüsselt
  (Spec 024 FR-005),
  und bei gleicher Generation gilt wie in FR-018 die mit dem kleinsten Hash.
  Hat das Relay eine Geräteliste angenommen, MUSS es jede weitere Anfrage
  eines Geräts, das nicht mehr darauf steht, sofort ablehnen, auch auf schon
  bestehenden Verbindungen und bei laufenden Übertragungen von Objekten, wie
  beim Entzug nach FR-023. Die Rolle eines Geräts (Hauptgerät oder verknüpft)
  MUSS das Relay nur dort prüfen, wo diese Spec es verlangt (FR-052); alle
  anderen Rechte hängen an der Vault. Ein Gerät DARF wie in FR-020 die
  Generation der Geräteliste nennen, die es mindestens erwartet; hält das
  Relay eine niedrigere, MUSS es das melden.
- **FR-022**: Das Relay MUSS Abrufe nur Vaults mit Lesen und Uploads von
  Paketen nur Vaults mit Schreiben erlauben, jeweils nach der geltenden Liste.
  Eine Momentaufnahme DARF nur hochladen: der Admin des Bereichs, also jedes Gerät auf seiner aktuellen Geräteliste (D29). Damit kann kein Mitglied mit einer unvollständigen Momentaufnahme ältere Pakete verdrängen. Beim Bereich
  „Vault“ ist das in beiden Fällen jedes eigene Gerät.
- **FR-023**: Ein Entzug MUSS sofort wirken: Sobald das Relay eine neue Liste
  angenommen hat, MUSS jede weitere Anfrage einer nicht mehr berechtigten Vault abgelehnt werden,
  auch auf schon bestehenden Verbindungen. Eine Widerrufsliste DARF dafür
  nicht nötig sein. „Sofort“ gilt für das Postfach und für Objekte
  gleichermaßen: Eine laufende Übertragung eines Objekts an oder von einer
  nicht mehr berechtigten Vault MUSS das Relay abbrechen (FR-026).
- **FR-024**: Mitgliederlisten, Gerätelisten, Anmeldungen und der
  Besitznachweis der Wiederherstellung (FR-053) MÜSSEN dasselbe
  secp256k1-Schlüsselsystem nutzen wie Vault-Identität und Geräteschlüssel;
  ein zweites Schlüsselsystem nur für das Relay DARF es nicht geben.
- **FR-025**: Die Prüfung am Relay ist nur eine zusätzliche Hürde. Jeder
  Empfänger MUSS jedes Paket und jede Momentaufnahme selbst prüfen (Signatur,
  Autor-Gerät auf der aktuellen Geräteliste seiner Vault, Fähigkeit der
  Autor-Vault im Bereich, Spec 024 und 027);
  was das Relay zu Unrecht durchlässt, DARF bei keinem Empfänger angewendet
  werden.

**Speicher-Backend A**

- **FR-026**: Das Relay DARF Speicher-Backend A anbieten, einen zusätzlichen,
  S3-kompatiblen Objektspeicher des Betreibers für die Objekte der Bereiche.
  Postfächer tragen nur SQLite-Daten (Pakete und Momentaufnahmen); ein Objekt
  DARF NIE in ein Postfach gelangen. Nutzt ein Bereich eigenen S3-Speicher
  (Speicher-Backend B, Spec 029), ist das Relay an seinen Dateien nicht
  beteiligt. Bietet es Speicher-Backend A an, MUSS es die
  Objekte selbst zwischen seinem Speicher und dem anfragenden Gerät
  übertragen, über seinen eigenen Endpunkt und dieselbe Protokollfamilie wie
  beim Postfach. Bei jeder Anfrage für ein Objekt MUSS es die geltende Liste
  prüfen: Lesen zum Herunterladen, Schreiben zum Hochladen, eine Berechtigung
  zum Löschen nach FR-028 zum Löschen. Zeitlich begrenzte Links oder andere
  Zugänge, mit denen ein Gerät am Relay vorbei auf den Speicher zugreift, DARF
  es NICHT ausstellen. Das Relay überträgt nur Chiffrat und DARF NICHT
  versuchen, es zu entschlüsseln. Ein Entzug wirkt damit auch für Objekte
  sofort (FR-023).
- **FR-027**: Objekte MÜSSEN unveränderlich sein: Ein Upload MUSS an Kennung und
  Größe des Objekts gebunden sein, und ein bestehendes Objekt DARF NICHT
  überschrieben werden. Das Relay MUSS einen Upload ablehnen, dessen Hash nicht
  zur Kennung passt; unabhängig davon prüft jeder Empfänger
  den Hash (Spec 025). Jedes Objekt MUSS im Speicher genau einmal liegen, nicht
  zusätzlich in einem Postfach und nicht je Mitglied kopiert.
- **FR-028**: Das Relay MUSS je Objekt die hochladende Vault-Identität
  vermerken. Ein Objekt löschen DÜRFEN nur diese Vault, Vaults mit Löschen und
  der Admin des Bereichs; das Relay prüft das bei jeder Anfrage zum Löschen
  selbst. Damit DARF ein Gerät des Admins alte Objekte
  (überholte Fassungen, gelöschte Dateien) löschen, sobald der Dateiindex sie
  nicht mehr nennt (Spec 027); im Bereich „Vault“ ist das jedes eigene Gerät.
  Postfächer und Objekte eines Bereichs MÜSSEN auf das Kontingent des Admins
  zählen.
- **FR-029**: Objekte MÜSSEN im Speicher unter undurchsichtigen Namen liegen,
  die aus Bereichskennung und Objektkennung bestehen. Dateinamen, Pfade,
  Dateitypen und Klartextgrößen DÜRFEN dort nicht vorkommen.
- **FR-030**: Das Relay DARF NIE Zugangsdaten für einen Speicher eines Nutzers
  lesbar annehmen, speichern oder benutzen (D11); Speicher-Backend B (Spec 029)
  läuft ohne das Relay. Als gewöhnliche Daten der Vault reisen die
  Zugangsdaten nur verschlüsselt im Postfach der Vault (FR-032, D30).

**holzi-Client: Postfach der Vault und Sync**

- **FR-031**: Sobald mindestens ein Relay eingetragen ist, MUSS holzi für den
  Bereich „Vault“ ein Postfach auf dem Relay führen, eine Mitgliederliste mit
  nur der eigenen Vault-Identität (Lesen, Schreiben) hochladen, eigene
  Änderungen als Änderungspakete hochladen und fremde abrufen und anwenden.
  Eigene Geräte, die nie gleichzeitig online sind, MÜSSEN so denselben Stand
  erreichen. Der direkte Sync aus Spec 024 bleibt daneben bestehen.
- **FR-032**: Der private Schlüssel der Vault-Identität, das einzige
  Nur-direkt-Datum (Spec 024, D30), DARF NICHT in ein Paket, eine
  Momentaufnahme oder ein Objekt für ein Relay gelangen, auch nicht
  verschlüsselt und auch nicht im Postfach der Vault; zum Relay gelangt er nur
  verschlüsselt im Wiederherstellungspaket (FR-052). Private Geräteschlüssel
  DÜRFEN ihr Gerät nie verlassen. Alles andere der Vault DARF über das
  Postfach der Vault reisen, verschlüsselt mit dem Inhaltsschlüssel des
  Bereichs „Vault“, ausdrücklich auch die Schlüsselumschläge an die eigenen
  Geräte, die entpackten Schlüssel von Spaces und Datenfreigaben, die die
  Vault empfangen hat (D28), Zugangsdaten wie die für eigenen S3-Speicher
  (D30) und der Dateiindex eigener synchronisierter Ordner mit den Schlüsseln
  der Dateien (Spec 025). Das
  Postfach der Vault gehört zum Bereich „Vault“, nicht zu einem anderen
  Bereich. Tabellen mit `_no_sync` bleiben lokal.
- **FR-033**: holzi DARF nur verschlüsselte Pakete und Momentaufnahmen zu einem
  Relay senden. Pakete SOLLTEN auf feste Größenstufen aufgefüllt werden, damit
  ihre Länge wenig über den Inhalt verrät.
- **FR-034**: Eine Änderung, die über das Postfach und direkt ankommt, MUSS
  genau einmal wirken. holzi MUSS je Postfach seinen Lesestand und die
  Kennung der Nummerierung festhalten.
- **FR-035**: Ohne erreichbares Relay MUSS holzi voll nutzbar bleiben. Eigene
  Änderungen MÜSSEN lokal vorgemerkt und hochgeladen werden, sobald das Relay
  wieder erreichbar ist; holzi MUSS es in wachsenden Abständen erneut
  versuchen. Es DARF keinen blockierenden Dialog geben.
- **FR-036**: holzi MUSS eine Momentaufnahme des Bereichs „Vault“ erstellen und
  hochladen, wenn die Pakete im Postfach eine Schwelle an Anzahl oder Größe
  überschreiten, und bevor das Kontingent voll ist. Die Momentaufnahme MUSS
  die ursprünglichen Signaturen jeder Änderung enthalten und jede Transaktion
  (Spec 024 FR-013) vollständig.
- **FR-037**: holzi MUSS Pakete und Momentaufnahmen nach FR-025 prüfen, mit den
  Regeln von Spec 024 zur Unteilbarkeit: Ein Änderungspaket ist unteilbar; ist
  eine Änderung darin ungültig, MUSS holzi das ganze Paket verwerfen. Eine
  Momentaufnahme MUSS holzi dagegen je vollständiger Transaktion prüfen, also
  je Gruppe der Änderungen mit gemeinsamem HLC-Zeitstempel (Spec 024 FR-013):
  Ist eine Änderung einer Transaktion ungültig, etwa mit fehlender oder
  falscher Signatur, MUSS holzi die ganze Transaktion verwerfen und die
  übrigen Transaktionen behalten. Eine Transaktion DARF nie zum Teil
  angewendet werden.
- **FR-048**: Eigene synchronisierte Ordner (Spec 025) DÜRFEN ihre Objekte im
  Speicher-Backend A des Relays ablegen, das das Postfach der Vault führt
  (FR-040), sofern es Speicher-Backend A anbietet. Die Objekte gehören dann zum
  Bereich „Vault“: Das Relay prüft Anfragen dafür gegen die Mitgliederliste
  dieses Bereichs (FR-026), sie zählen auf das Kontingent der Vault
  (FR-028), und eigene Geräte räumen Objekte, die der Dateiindex nicht mehr
  nennt, als Admin auf. Ohne ein solches Relay laufen die Objekte nur direkt
  zwischen eigenen Geräten (Spec 025).

**holzi-Client: Einstellungen**

- **FR-038**: Die Kategorie „Föderation“ der Einstellungen (Spec 023) MUSS die
  Unteransicht „Relays“ bekommen, neben den Unteransichten „Geräte“ (Spec 024),
  „Ordner“ (Spec 025), „Spaces“ (Spec 027) und „Datenfreigaben“ (Spec 028).
  „Relays“ MUSS
  jedes eingetragene Relay mit Adresse, Zustand (Verbunden, Nicht erreichbar
  seit …, Abgelehnt mit Grund, Unvollständige Daten, Daten verloren,
  Identität geändert) und belegtem Anteil des Kontingents zeigen.
- **FR-039**: Der Nutzer MUSS ein Relay über Adresse und Einladungscode
  hinzufügen können. holzi MUSS dabei die Zulassung einlösen, die Identität des
  Relays festhalten und das Postfach der Vault anlegen; ein Fehler MUSS am Feld
  erklärt werden. Der Einladungscode DARF nach der Zulassung nicht gespeichert
  werden. Ändert sich die Identität des Relays später, MUSS holzi den Sync mit
  ihm anhalten und „Identität geändert“ zeigen, bis der Nutzer das Relay
  entfernt oder neu hinzufügt.
- **FR-040**: Die Liste der Relays MUSS für die Vault gelten und über den Sync
  zu allen eigenen Geräten kommen (Spec 023 FR-024). Ein weiteres eigenes
  Gerät MUSS sich ohne Einladungscode anmelden können. Hinzufügen und Entfernen
  MÜSSEN Aktionen im Katalog von Spec 020 sein, die nur der Nutzer auslösen
  kann; die Liste mit Zuständen MUSS für Agenten mit Leserecht auf die
  Einstellungen abrufbar sein. Mehrere Relays DÜRFEN eingetragen sein; welche
  davon das Postfach der Vault führen: Ein Bereich liegt in v1 auf genau einem Relay, seinem Heimat-Relay: das Postfach der Vault auf dem zuerst eingetragenen Relay, ein Space oder eine Datenfreigabe auf dem Relay ihres Admins. Weitere Relays dienen nur dem NAT-Durchgang und als Heimat anderer Bereiche.
- **FR-041**: holzi MUSS die Mitgliederliste jedes Bereichs, dessen Admin die
  Vault ist, erneuern, bevor die Hälfte ihrer Laufzeit verstrichen ist, sofern
  ein Gerät der Vault online ist und das Relay erreicht. Sieht ein Gerät des
  Admins zwei verschiedene gültige Listen derselben Generation (FR-018), MUSS
  es eine Liste der nächsten Generation veröffentlichen, die die Änderungen
  beider vereint (Spec 024).
- **FR-042**: holzi MUSS prüfen, ob die eigenen hochgeladenen Pakete später
  im Postfach auftauchen, und ob nach einer Momentaufnahme eigene Änderungen
  fehlen. Fehlt etwas, das dieses Gerät hat, MUSS es das erneut hochladen.
- **FR-043**: holzi MUSS zurückgehaltene oder verlorene Daten erkennen: eine
  Lücke in den Sequenznummern, eine geänderte Kennung der Nummerierung, eine
  höchste Nummer unter dem eigenen Lesestand, eine ältere Mitgliederliste als
  verlangt (FR-020) und Änderungen, die Fortschrittsstände nennen, die aber
  innerhalb einer Frist weder direkt noch über das Postfach ankommen. Geprüfte
  Pakete MUSS es trotzdem anwenden, den Lesestand aber nicht über eine Lücke
  hinaus setzen. Der Zustand MUSS beim Relay angezeigt werden (FR-038).
- **FR-044**: Hat ein Relay Daten eines Bereichs verloren, für den dieses Gerät
  eine Momentaufnahme hochladen darf, MUSS holzi das Postfach aus dem eigenen
  Stand neu füllen, damit neue Geräte wieder aufschließen können.
- **FR-045**: Lehnt ein Relay ab, MUSS holzi den Grund unterscheiden und zeigen:
  nicht zugelassen, Einladungscode ungültig, keine Berechtigung, Liste fehlt
  oder abgelaufen, Kontingent fast voll (ab 80 %) oder erschöpft, Version nicht
  unterstützt, Gerät nicht auf der Geräteliste (FR-049), Bereich vom Admin
  beendet (FR-021), mit dem Hinweis, dass er bis zum Ablauf der letzten Liste
  nur noch lesbar ist bzw. gelöscht wurde, und Beenden abgelehnt, weil kein
  Gerät des Admins die Erklärung signiert hat (FR-021). Wo der Nutzer etwas
  tun kann (neuen Code eingeben, holzi aktualisieren, Platz schaffen, das Gerät
  von einem Hauptgerät neu verknüpfen lassen, beim beendeten Bereich den Admin
  um eine neue Einladung bitten), MUSS die Anzeige das nennen.
- **FR-046**: Entfernt der Nutzer ein Relay, MUSS holzi nach Bestätigung das
  Postfach der Vault dort löschen, soweit das Relay erreichbar ist, und das
  Relay auf keinem eigenen Gerät mehr nutzen. Ist das Relay Heimat von
  Bereichen, deren Admin die Vault ist, MUSS holzi diese vor der Bestätigung
  nennen. Liegt dort das Wiederherstellungspaket, MUSS holzi vor der
  Bestätigung sagen, dass die Wiederherstellung damit endet, und danach das
  Einrichten auf dem neuen Heimat-Relay anbieten (FR-055).
- **FR-047**: Alle neuen Texte der Einstellungen und Zustände MÜSSEN auf Deutsch
  und Englisch vorliegen (Spec 023 FR-020).

**Wiederherstellung**

- **FR-050**: holzi MUSS eine freiwillige Wiederherstellung anbieten, die der
  Nutzer ausdrücklich einschaltet; ohne sein Zutun DARF nichts davon beim
  Relay liegen. Einrichten DARF nur ein Hauptgerät, und nur, wenn das
  Postfach der Vault auf einem Relay liegt. holzi MUSS dabei einen
  Wiederherstellungsschlüssel mit mindestens 128 Bit Zufall erzeugen und ihn
  genau einmal als Code und als QR-Code zeigen, mit dem Hinweis, ihn offline
  aufzubewahren. holzi DARF den Wiederherstellungsschlüssel weder in der Vault
  noch sonst speichern, anzeigen oder übertragen, nachdem der Nutzer den Dialog
  verlassen hat. Der Code MUSS die Adresse des Relays enthalten, auf dem das
  Paket liegt.
- **FR-051**: Zum Einrichten gehört ein zweiter Faktor. Voreingestellt MUSS
  TOTP sein: holzi zeigt das TOTP-Geheimnis als QR-Code für eine
  Authenticator-App und schaltet die Wiederherstellung erst ein, wenn der
  Nutzer einen gültigen Code eingegeben hat. Wahlweise DARF der Nutzer
  stattdessen einen Link per E-Mail wählen, sofern das Relay ihn anbietet;
  holzi MUSS vorher sagen, dass das Relay dann seine E-Mail-Adresse kennt
  (FR-003), und die Adresse mit einem ersten Link bestätigen lassen.
- **FR-052**: Aus dem Wiederherstellungsschlüssel MUSS holzi auf dem Gerät
  ableiten: ein Schlüsselpaar zum Verschlüsseln des
  Wiederherstellungspakets, ein Schlüsselpaar zur Authentisierung und eine
  daraus abgeleitete Suchkennung. Auf den Geräten der Vault DARF danach nur
  der öffentliche Schlüssel zum Verschlüsseln bleiben, damit ein Hauptgerät
  das Paket neu verschlüsseln kann, ohne den Wiederherstellungsschlüssel zu
  kennen. Das Paket MUSS den privaten Schlüssel der Vault-Identität, alle
  Generationen des Inhaltsschlüssels des Bereichs „Vault“ und die Liste der
  Relays (Adressen und festgehaltene Identitäten) enthalten und eine Nummer
  seines Stands tragen. Ein Hauptgerät MUSS es neu verschlüsseln und
  hochladen, sobald eine neue Generation des Inhaltsschlüssels des Bereichs
  „Vault“ entsteht oder sich die Liste der Relays ändert, und bei jeder
  Verbindung prüfen, ob das Relay den aktuellen Stand hat. Das Relay MUSS je
  Vault genau ein Paket halten, ein Paket nur von einem Gerät annehmen, das
  auf der aktuellen Geräteliste der Vault ein Hauptgerät ist (FR-049), ein
  Paket mit niedrigerer Nummer ablehnen und das Paket auf das Kontingent der
  Vault zählen.
- **FR-053**: Das Relay MUSS von der Wiederherstellung nur speichern: die
  Suchkennung, den öffentlichen Schlüssel zur Authentisierung, das
  verschlüsselte Paket mit seiner Nummer, die Einrichtung des zweiten Faktors
  (TOTP-Geheimnis oder E-Mail-Adresse) und die Zähler der Fehlversuche. Den
  Wiederherstellungsschlüssel, den Schlüssel zum Entschlüsseln des Pakets und
  den privaten Schlüssel zur Authentisierung DARF es nie erhalten. Ein Abruf
  MUSS ohne Anmeldung als Gerät und ohne Zulassung möglich sein und beides
  verlangen: (1) den Besitznachweis, bei dem das Gerät eine Challenge des
  Relays mit dem privaten Schlüssel zur Authentisierung signiert, ohne etwas
  Geheimes zu senden, und (2) danach den zweiten Faktor, also einen gültigen
  TOTP-Code oder das Öffnen des Links aus der E-Mail. Erst wenn beides
  gelungen ist, DARF das Relay das Paket ausliefern. Vor dem gelungenen
  Besitznachweis DARF das Relay nicht verraten, ob es zu einer Suchkennung ein
  Paket gibt, und DARF keine E-Mail senden. Das Relay MUSS Versuche je
  Suchkennung und je Netzadresse begrenzen und nach wiederholten
  Fehlversuchen weitere für eine wachsende Frist sperren; holzi MUSS die
  Sperre mit ihrem Ende anzeigen.
- **FR-054**: holzi MUSS das Paket nur auf dem Gerät entschlüsseln. Danach MUSS
  es eine neue Instanz der Vault mit neuem Geräteschlüssel anlegen, den
  Nutzer eine neue Passphrase setzen lassen, die letzte Momentaufnahme und die
  Pakete danach aus dem Postfach der Vault holen und anwenden (FR-017,
  FR-037) und eine neue Geräteliste mit höherer Generation als die auf dem
  Relay veröffentlichen, in der die neue Instanz ein Hauptgerät ist. Die
  bisherigen Geräte MUSS holzi dabei zeigen, zum Entfernen vorausgewählt; der
  Nutzer entscheidet je Gerät. Entfernt er Geräte, gilt dasselbe wie bei jedem
  Entfernen (Spec 024): eine neue Generation des Inhaltsschlüssels des Bereichs
  „Vault“ und danach ein neues Paket (FR-052).
- **FR-055**: Die Unteransicht „Relays“ (FR-038) MUSS den Zustand der
  Wiederherstellung zeigen (aus, eingerichtet mit Art des zweiten Faktors,
  Stand des Pakets auf dem Relay, Paket fehlt oder veraltet). Auf einem
  Hauptgerät MUSS der Nutzer dort die Wiederherstellung einrichten, einen neuen
  Wiederherstellungsschlüssel erzeugen, den zweiten Faktor neu einrichten und
  die Wiederherstellung ausschalten können; ein neuer Schlüssel ersetzt Paket,
  Suchkennung und Schlüssel zur Authentisierung auf dem Relay, sodass der alte
  nichts mehr öffnet, und Ausschalten löscht alles davon auf dem Relay.
  Verknüpfte Geräte zeigen den Zustand nur an. „Vault wiederherstellen“ MUSS
  dort angeboten werden, wo holzi eine Vault anlegt oder öffnet.
- **FR-056**: Einrichten, Ändern, Ausschalten und Abrufen der Wiederherstellung
  MÜSSEN Aktionen im Katalog von Spec 020 sein, die nur der Nutzer auslösen
  kann. Kein Agent und keine Erweiterung DARF den Wiederherstellungsschlüssel,
  das TOTP-Geheimnis oder das entschlüsselte Paket sehen.

### Key Entities

- **Relay**: ein Dienst unter einer Adresse mit einer festen Identität
  (Schlüssel). Bietet Postfächer, NAT-Durchgang, optional Speicher-Backend A
  und Signalisierung. Hat eine Protokollversion, eine Höchstlaufzeit für
  Listen, eine Obergrenze für Paketgrößen.
- **Einladungscode**: vom Betreiber erzeugt; lässt genau eine Vault-Identität
  zu; trägt Kontingent und optional eine Gültigkeitsdauer; verbraucht oder
  zurückgezogen.
- **Zulassung**: Vault-Identität, Kontingent, belegter Platz, aktiv oder
  entzogen.
- **Postfach**: gehört zu genau einem Bereich; Kennung der Nummerierung,
  höchste vergebene Sequenznummer, geltende Momentaufnahme (mit ihrer Nummer)
  und die Pakete danach.
- **Änderungspaket (Sicht des Relays)**: Bereichskennung, Schlüsselkennung,
  Chiffrat, Länge, Sequenznummer, Zeitpunkt des Eingangs.
- **Momentaufnahme (Sicht des Relays)**: Bereichskennung, Sequenznummer, bis zu
  der sie reicht, Chiffrat, hochladende Vault.
- **Mitgliederliste**: Bereichskennung, Generation, Einträge {Vault-Identität →
  Fähigkeiten}, Ausstellungszeit, Ablaufzeit, Signatur eines Geräts des Admins.
- **Geräteliste (Sicht des Relays)**: Vault-Identität, Generation, Einträge
  {Geräteschlüssel → Rolle (Hauptgerät oder verknüpft), Name, Netzkennung},
  Signatur der Vault-Identität (Spec 024); je Vault nur die neueste gültige.
- **Objekt (Sicht des Relays)**: Bereichskennung, Objektkennung (Hash des
  Chiffrats), Größe des Chiffrats, hochladende Vault-Identität.
- **Ende-Erklärung (Sicht des Relays)**: Bereichskennung, Zeitpunkt, Signatur
  eines Geräts des Admins; endgültig; bleibt mit der Bereichskennung
  erhalten, nachdem Postfach und Objekte gelöscht sind.
- **Wiederherstellungspaket (Sicht des Relays)**: Suchkennung, öffentlicher
  Schlüssel zur Authentisierung, Chiffrat, Nummer des Stands, zugehörige
  Vault-Identität, Art und Einrichtung des zweiten Faktors (TOTP-Geheimnis
  oder E-Mail-Adresse), Zähler der Fehlversuche, Sperre bis. Je Vault höchstens
  eines.
- **Wiederherstellungsschlüssel**: nur beim Nutzer, offline; daraus abgeleitet
  der Schlüssel zum Entschlüsseln des Pakets, das Schlüsselpaar zur
  Authentisierung und die Suchkennung. Auf den Geräten bleibt nur der
  öffentliche Schlüssel zum Verschlüsseln des Pakets.
- **Relay-Eintrag (in der Vault)**: Adresse, festgehaltene Identität des
  Relays, Zeitpunkt der Zulassung; je Postfach Lesestand und Kennung der
  Nummerierung (je Gerät); Zustand je Gerät.

## Success Criteria _(mandatory)_

### Measurable Outcomes

- **SC-001**: Zwei Geräte einer Vault, die nie gleichzeitig online sind,
  erreichen über ein Relay denselben Stand: Eine Änderung auf Gerät A ist auf
  Gerät B spätestens 60 Sekunden, nachdem B das Relay erreicht hat, sichtbar,
  in 100 von 100 Versuchen.
- **SC-002**: Eine Untersuchung der gesamten Ablage eines Relays (Datenbank,
  Dateien, Objektspeicher, Protokolle), nachdem zwei Vaults Daten und Dateien
  mit bekannten Markierungswörtern in Inhalten, Tabellen-, Spalten- und
  Dateinamen synchronisiert haben, findet keinen Klartext, keine Tabellen- oder
  Spaltennamen, keine Primärschlüssel, keine Zeitstempel der Änderungen und
  keine Dateinamen: 0 Treffer.
- **SC-003**: Hat das Relay eine Mitgliederliste angenommen, die eine Vault
  entfernt, wird deren nächste Anfrage für den Bereich abgelehnt, in 100 % der Fälle und
  auch auf einer schon offenen Verbindung. Das gilt ebenso für Objekte: Jedes
  weitere Hoch- oder Herunterladen wird abgelehnt, und eine laufende
  Übertragung bricht ab. Ebenso wird die nächste Anfrage eines Geräts in 100 %
  der Fälle abgelehnt, sobald das Relay eine Geräteliste ohne dieses Gerät
  angenommen hat.
- **SC-004**: Mitgliederlisten mit niedrigerer Generation, abgelaufener
  Laufzeit, falscher Signatur oder falschem Admin werden zu 100 % abgelehnt;
  von zwei gültigen Listen derselben Generation gilt in 100 % der Fälle die
  mit dem kleineren Hash, unabhängig von der Reihenfolge des Hochladens; ohne
  gültige Liste liefert das Relay für den Bereich nichts aus.
- **SC-005**: Pakete oder Momentaufnahmen, deren Inhalt das Relay oder ein
  Mitglied verändert oder für ein anderes Mitglied erfunden hat, werden von
  100 % der Empfänger verworfen, ohne dass etwas davon angewendet wird.
- **SC-006**: In Tests mit einem Relay, das Pakete auslässt, ein Postfach
  löscht oder die Nummerierung neu beginnt, zeigt holzi den passenden Zustand
  spätestens beim nächsten Abruf; kein Gerät verliert dabei Daten, die es schon
  hatte, und ein danach neu verknüpftes Gerät erreicht denselben Stand wie die
  anderen.
- **SC-007**: Nach 10.000 Änderungen an denselben 100 Einträgen sinkt der
  belegte Platz des Postfachs durch eine Momentaufnahme um mindestens 90 %; ein
  neues Gerät erreicht aus Momentaufnahme und restlichen Paketen denselben
  Stand wie die anderen Geräte.
- **SC-008**: Ist das Relay nicht erreichbar, bleibt jede Funktion von holzi
  außer dem Relay-Sync nutzbar, ohne blockierenden Dialog; lokale Änderungen
  sind spätestens 60 Sekunden, nachdem das Relay wieder erreichbar ist, auf
  einem anderen Gerät sichtbar.
- **SC-009**: Ein Objekt von 1 GB lässt sich über Speicher-Backend A hoch- und
  auf einem zweiten Gerät herunterladen, und der Hash stimmt; eine
  Untersuchung von Speicher und Protokollen des Relays danach findet nur das
  Chiffrat und keinen Klartext des Objekts.
- **SC-010**: Ein Betreiber setzt ein Relay mit Postfächern, NAT-Durchgang und
  Speicher-Backend A auf und lässt eine Vault zu, ohne ein Konto, eine
  E-Mail-Adresse oder einen fremden Dienst einzurichten; ein Nutzer trägt das
  Relay mit Adresse und Code in unter einer Minute ein.
- **SC-011**: Hat eine Vault ihr Kontingent erschöpft, werden Uploads anderer
  Vaults auf demselben Relay weiter zu 100 % angenommen.
- **SC-012**: Nachdem der Admin einen Bereich beendet hat, nimmt das Relay für
  ihn in 100 % der Tests keine Liste, kein Paket, keine Momentaufnahme und
  keinen Upload mehr an, auch nicht von einem Gerät des Admins und auch nicht
  nach dem Löschen des Postfachs; Mitglieder der letzten gültigen Liste können
  bis zu deren Ablauf lesen und erhalten die Ende-Erklärung, danach ist das
  Postfach gelöscht. Eine Ende-Erklärung, die kein Gerät auf der Geräteliste
  des Admins signiert hat, wird zu 100 % abgelehnt, und kein Bereich wird je an
  eine andere Vault-Identität gebunden.
- **SC-013**: Eine Untersuchung der gesamten Ablage des Relays (Datenbank,
  Dateien, Objektspeicher, Protokolle) nach Einrichten, mehreren
  Aktualisierungen und einem Abruf der Wiederherstellung findet weder den
  Wiederherstellungsschlüssel noch den Schlüssel zum Entschlüsseln des Pakets
  noch den privaten Schlüssel zur Authentisierung noch sonst etwas, womit sich
  das Paket entschlüsseln lässt: 0 Treffer. Mit allem, was das Relay hat,
  lässt sich das Paket in 100 % der Versuche nicht entschlüsseln.
- **SC-014**: Ein Abruf ohne gültigen zweiten Faktor schlägt in 100 % der
  Versuche fehl, ebenso ein Abruf ohne gültigen Besitznachweis; nach der
  festgelegten Zahl von Fehlversuchen wird in 100 % der Fälle gesperrt, und
  vor dem Besitznachweis unterscheidet sich die Antwort des Relays für eine
  unbekannte Suchkennung nicht von der für eine bekannte.
- **SC-015**: Ein Nutzer, der nur Wiederherstellungsschlüssel, zweiten Faktor
  und ein frisches Gerät hat, stellt seine Vault in 100 von 100 Versuchen mit
  dem letzten Stand des Postfachs der Vault wieder her, auch wenn nach dem
  Einrichten neue Generationen des Inhaltsschlüssels entstanden sind; das neue
  Gerät ist danach Hauptgerät, und jedes dabei entfernte Gerät wird vom Relay
  abgelehnt. Die Schritte in holzi dauern ohne die Übertragung der Daten unter
  fünf Minuten.

## Assumptions

- Das Relay ist ein eigener Dienst mit eigenem Betriebsmodell (kopflos, ohne
  Oberfläche, für viele Nutzer) und kann später in ein eigenes Repository
  umziehen. Diese Spec beschreibt sein Verhalten und die Seite von holzi; wo
  der Code liegt, entscheidet der Plan.
- Transport ist iroh: Das Relay spricht dieselbe Protokollfamilie wie der
  direkte Sync (Spec 024) als „blinder Teilnehmer“, der nur speichert und
  weitergibt, und betreibt ein iroh-Relay für den NAT-Durchgang, wie
  haex-sync-server es schon tut. Die Anmeldung folgt dem Challenge-Response
  von NIP-42. Die optionale Signalisierung ist ein Nostr-Relay. Speicher-Backend
  A ist S3-kompatibel und liegt hinter dem Relay; Geräte greifen nie direkt
  darauf zu, sondern das Relay überträgt die Objekte selbst über seinen
  Endpunkt (FR-026, D24).
- Die Rollen des Relays sind getrennt (D32): Der Dienst des Relays
  synchronisiert nur SQLite-Daten über Postfächer. Speicher-Backend A ist ein
  zusätzliches Angebot des Betreibers, das der Dienst nur bewacht und durch
  das er die Objekte streamt; jedes Objekt liegt dort einmal. Mit
  Speicher-Backend B (Spec 029) ist das Relay an Dateien nicht beteiligt.
- Weil das Relay die Objekte selbst überträgt, läuft die gesamte Bandbreite
  für Dateien über den Server des Relay-Betreibers. Das ist hingenommen, weil
  ein Entzug so auch für Objekte sofort wirkt. Der Betreiber muss das bei
  Leitung und Kontingenten einplanen.
- Die Formate von Paket, Mitgliederliste, Geräteliste, Momentaufnahme,
  Ende-Erklärung und Wiederherstellungspaket sowie die Verfahren zum Ableiten
  der Schlüssel aus dem Wiederherstellungsschlüssel legt Spec 024 bzw. der Plan
  fest (Design §15 Punkt 7). Die Bereichskennung
  wird aus der Vault-Identität des Admins und einem Zufallswert abgeleitet
  (FR-021); das ist eine Ergänzung zum Design, das nur „prüft die Signatur des
  Admins“ sagt, und verhindert, dass jemand eine fremde Bereichskennung zuerst
  belegt.
- Kontingente zählen beim Admin eines Bereichs (FR-028), weil der Admin die
  Mitglieder aufnimmt. Mitglieder brauchen für fremde Bereiche keine eigene
  Zulassung (FR-006).
- Standardwerte, die der Plan bestätigt: Höchstlaufzeit einer Liste 90 Tage,
  Erneuerung nach der Hälfte (FR-041), Warnung ab 80 % Kontingent, Frist für
  „Daten zurückgehalten“ 24 Stunden, Toleranz für Uhrzeiten 5 Minuten; für die
  Wiederherstellung TOTP mit 6 Ziffern und 30 Sekunden, ein Link per E-Mail
  15 Minuten gültig, Sperre nach 5 Fehlversuchen für zunächst eine Stunde,
  danach jeweils doppelt so lang (FR-053).
- Die Wiederherstellung setzt voraus, dass das Postfach der Vault auf einem
  Relay liegt; ohne Relay bleibt nur eine Kopie der Datei (Spec 024). Sie
  stellt den Stand des Postfachs her, keinen früheren. Weil der
  Wiederherstellungsschlüssel das Paket öffnet, ohne dass eine Passphrase
  dazukommt, schützt ihn nur die Aufbewahrung durch den Nutzer; die hohe
  Entropie schützt gegen Raten, der zweite Faktor gegen Dritte, die den Code
  finden, aber nicht gegen den Betreiber, der ohne den Code trotzdem nichts
  entschlüsseln kann. Ob ein Relay den Link per E-Mail anbietet, entscheidet
  sein Betreiber; ohne ihn gibt es nur TOTP.
- Der Einladungscode ist ein Geheimnis zur einmaligen Zulassung. Danach reicht
  die Vault-Identität; deshalb speichert holzi den Code nicht, und andere
  Geräte brauchen ihn nicht.
- Die Vault-Identität wechselt in v1 nie (D26). Verknüpfen, Entfernen und die
  Geräteliste legt Spec 024 fest (D27); diese Spec regelt, was das Relay damit
  tut (FR-004, FR-049). Ein entferntes Gerät weist das Relay erst ab, wenn ein
  Hauptgerät die neue Geräteliste hochgeladen hat; bis dahin kann es weiter
  synchronisieren, und dieses Risiko ist hingenommen. Das Beenden eines
  Bereichs löst der Admin aus (Spec 027, 028); diese Spec regelt nur, was das
  Relay damit tut (FR-021).
- Wer Relays anbietet und wie ein Nutzer sie findet, ist nicht Teil dieser
  Spec: Der Nutzer bekommt Adresse und Code vom Betreiber.
- Die Arbeitstitel „Relays“ und die Zustandsnamen legt der Plan endgültig fest.
- Spec 024 ist Voraussetzung (Vault-Identität, Geräteschlüssel, Geräteliste,
  Paketformat, Fortschrittsstände, Nur-direkt-Daten). User Story 7 braucht zusätzlich die
  Objekte aus Spec 025.
- Die Phasenregel der Verfassung gilt: Diese Spec darf vorab geschrieben, aber
  erst umgesetzt werden, wenn Spec 024 im täglichen Gebrauch ist.

## Nicht im Umfang

- Pseudonyme je Bereich oder andere Wege, die Teilnehmer eines Bereichs vor
  dem Relay zu verbergen (D9); ebenso das Verbergen von IP-Adressen und
  Zeitpunkten.
- Verwaltung der Mitglieder, Einladungen samt ihrem Transport (Spec 027),
  neue Generationen der Inhaltsschlüssel und die Oberfläche dafür (Spec 027, 028). Diese
  Spec liefert nur den Mechanismus der Mitgliederliste am Relay und optional
  eine Signalisierung (FR-009).
- Speicher-Backend B, der eigene S3-Speicher des Nutzers (Spec 029).
- Bezahlung für Kontingente (etwa Cashu oder Lightning); in v1 nur
  Einladungscodes.
- Umzug eines Bereichs von einem Relay auf ein anderes.
- Das Übertragen der Admin-Rolle, auf Wunsch oder bei Verlust (D23); ebenso
  das Binden eines Bereichs an eine andere Vault-Identität und jeder Wechsel
  der Vault-Identität (D26).
- Konten; E-Mail-Adressen außer dem freiwilligen zweiten Faktor der
  Wiederherstellung (FR-051); eine Sicherung früherer Stände der Vault auf dem
  Relay.
- Das Hinterlegen des Wiederherstellungsschlüssels, verschlüsselt bei einer
  anderen Person; das kommt später in einer eigenen Spec und nur auf Wunsch
  des Nutzers.
- Löschen von Daten auf einem entfernten Gerät aus der Ferne (D27).
- Eine Oberfläche für Betreiber über die Verwaltung aus FR-007 hinaus.
- Suche, Verzeichnis oder Empfehlung öffentlicher Relays.
- Ein Verlauf früherer Stände auf dem Relay; die Momentaufnahme ersetzt ältere
  Pakete endgültig.
