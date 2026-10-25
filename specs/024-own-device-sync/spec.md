# Feature Specification: Vault-Identität, Geräteliste und Datensync zwischen eigenen Geräten

**Feature Branch**: `024-own-device-sync`
**Created**: 2026-09-28
**Status**: Draft
**Input**: Entwurf [`2026-09-28-sync-architecture.md`](../../docs/plans/2026-09-28-sync-architecture.md),
Zeile 024 der Aufteilung in §14, aus der Brainstorming-Sitzung des Betreibers
vom 2026-09-27/28 und den Entscheidungen D26 bis D31 vom 2026-09-28. Eine Vault
läuft gleichzeitig auf mehreren Geräten; jedes Gerät hat einen eigenen
Geräteschlüssel, die Vault-Identität ist die feste öffentliche Adresse der
Vault (D1). Den privaten Schlüssel der Vault-Identität haben nur Hauptgeräte;
welche Geräte zur Vault gehören, sagt die Geräteliste, die ein Hauptgerät mit
der Vault-Identität signiert (D27). Einen Wechsel der Vault-Identität gibt es in
v1 nicht (D26). Kein MLS (D13). Die Geräte einer Vault finden sich über
verschlüsselte Präsenzmeldungen über Nostr, verbinden sich direkt über iroh und
gleichen die ganze Vault außer gerätelokalen Daten ab, direkt über die
geprüfte, verschlüsselte Verbindung und in ganzen Transaktionen. Jede Änderung
behält ihr Ursprungsgerät, der Fortschritt wird je Ursprungsgerät verfolgt.
Heute erzeugt holzi nur einen Platzhalter anstelle eines echten Schlüsselpaars
(`src-tauri/src/identity/bootstrap.rs`); eine kopierte Vault-Datei funktioniert
schon. Der bevorzugte Weg zu einem weiteren Gerät ist das Verknüpfen per Code
oder QR (P1); die Kopie der Vault-Datei bleibt ein Nebenweg (P2), ebenso P2 ist
das Entfernen eines Geräts. Die Unteransicht „Geräte“ der Einstellungs-Kategorie
„Föderation“ zeigt die Rollen der Geräte, die öffentliche Vault-Identität zum
Lesen, „zuletzt online“ und den Live-Stand. Sync über den Sync-Server und die
Wiederherstellung kommen mit Spec 026.

## Begriffe

- **Vault-Identität**: das Schlüsselpaar der Vault (secp256k1, im Nostr-Format).
  Sein öffentlicher Schlüssel ist die feste Adresse der Vault für andere Nutzer
  (Rechte, Mitgliederlisten; Specs 027, 028; keine Einladeadresse, siehe
  Spec 027 FR-008) und prüft die Geräteliste. Der private Schlüssel liegt nur auf Hauptgeräten, in der
  verschlüsselten Vault (D27). In v1 wechselt die Vault-Identität nie (D26).
- **Geräteschlüssel**: ein eigenes Schlüsselpaar je Gerät (secp256k1, im
  Nostr-Format), die Identität des Geräts. Mit ihm weist sich das Gerät beim
  Verbinden aus und signiert, was es über Dritte veröffentlicht
  (Präsenzmeldungen, Aufnahmeanfragen, ab Spec 026 Pakete, ab Spec 027
  Änderungen in gemeinsamen Bereichen); an ihn gehen Umschläge. Es verlässt
  das Gerät nie und wird nicht synchronisiert.
- **Gerät der Vault**: eine Instanz von holzi, die die Vault geöffnet hat, mit
  eigenem Geräteschlüssel. Es gehört zur Vault, solange sein Geräteschlüssel auf
  der aktuellen Geräteliste steht. Gleichbedeutend mit einem Eintrag der
  Unteransicht „Geräte“ (Spec 023).
- **Hauptgerät**: ein Gerät der Vault mit dem privaten Schlüssel der
  Vault-Identität. Nur Hauptgeräte dürfen Geräte hinzufügen und entfernen, also
  Gerätelisten signieren. Eine Vault darf mehrere Hauptgeräte haben; die erste
  Instanz einer Vault ist eines.
- **Verknüpftes Gerät**: ein Gerät der Vault ohne den privaten Schlüssel der
  Vault-Identität. Es liest und schreibt alle Daten der Vault und darf Spaces
  und Datenfreigaben verwalten (D29), aber keine Geräte hinzufügen oder
  entfernen.
- **Geräteliste**: die mit der Vault-Identität von einem Hauptgerät signierte
  Liste aller aktuellen Geräte der Vault (öffentlicher Geräteschlüssel, Rolle
  „Hauptgerät“ oder „verknüpftes Gerät“, Name, Netzwerkkennung), dazu die
  entfernten Geräte mit ihrer Grenze, mit einer Generation. Für sie gelten
  dieselben Regeln wie für Mitgliederlisten (FR-005, FR-043). Sync-Server,
  eigene Geräte und die Geräte der Mitglieder nehmen ein Gerät nur an, wenn es auf der
  aktuellen Geräteliste seiner Vault steht.
- **Verknüpfen**: der bevorzugte Weg, die Vault auf einem weiteren Gerät zu
  nutzen: Die neue Instanz tritt per Code oder QR eines Hauptgeräts bei, das sie
  in die Geräteliste einträgt (FR-023 bis FR-025).
- **Verknüpfungscode**: der einmal verwendbare, kurzlebige Code, den ein
  Hauptgerät dafür als QR-Code und als Text zeigt.
- **Aufnahmeanfrage**: die Bitte einer kopierten Vault-Datei ohne den privaten
  Schlüssel der Vault-Identität, in die Geräteliste aufgenommen zu werden
  (FR-044, FR-045).
- **Entfernen**: ein Hauptgerät nimmt ein Gerät aus der Geräteliste und erzeugt
  eine neue Generation des Inhaltsschlüssels des Bereichs „Vault“ für die
  verbleibenden Geräte (FR-026 bis FR-028).
- **Präsenzmeldung**: eine verschlüsselte, nur für die Geräte der eigenen Vault
  lesbare Nachricht eines Geräts mit seiner aktuellen Erreichbarkeit.
- **Bereich**: wozu eine Menge von Änderungen gehört: die Vault selbst (Bereich
  „Vault“), ein Space (Bereich eines Space, Spec 027) oder eine Datenfreigabe
  (Bereich einer Datenfreigabe, Spec 028). In dieser Spec gibt es nur den
  Bereich „Vault“; auch das Postfach der eigenen Vault beim Sync-Server gehört
  zu ihm. (Nicht zu verwechseln mit den Unteransichten einer
  Einstellungs-Kategorie in Spec 023.)
- **Ursprungsgerät**: das Gerät, das eine Änderung geschrieben hat. Es steht im
  Zeitstempel der hybriden logischen Uhr jeder Änderung (dessen Gerätekennung)
  und bleibt beim Weiterleiten erhalten.
- **Änderungspaket**: eine verschlüsselte Gruppe vollständiger Transaktionen
  eines Bereichs für Wege über Dritte: das Postfach beim Sync-Server (Spec 026)
  und die gemeinsamen Bereiche (Specs 027, 028). Zwischen eigenen Geräten gibt
  es keine Änderungspakete; dort reisen Transaktionsgruppen direkt über die
  geprüfte, verschlüsselte Verbindung (FR-012). Ein Änderungspaket ist atomar
  (FR-013).
- **Momentaufnahme**: ein vollständiger Stand eines Bereichs mit dem
  Ursprungsgerät jeder Änderung, etwa beim Verknüpfen oder im Postfach beim
  Sync-Server (Spec 026). Anders als ein Änderungspaket wird sie je
  vollständiger Transaktionsgruppe geprüft (FR-013).
- **Transaktionsgruppe**: die Änderungen mit gemeinsamem Zeitstempel der
  hybriden logischen Uhr, also eine Transaktion. Sie wird nie geteilt und nie
  zum Teil angewendet (FR-013).
- **Nur-direkt-Daten**: das einzige Vault-Geheimnis, das nie mit dem
  gewöhnlichen Sync reist: der private Schlüssel der Vault-Identität (FR-038,
  D30).
- **Inhaltsschlüssel** und **Schlüsselgeneration**: der Schlüssel, mit dem
  ein Bereich alles verschlüsselt, was über Dritte reist oder dort liegt
  (Präsenzmeldungen, Namen in der Geräteliste, ab Spec 026 Änderungspakete), und
  seine Generation. Für
  den Bereich „Vault“ entsteht eine neue Generation mit jedem Entfernen eines
  Geräts (FR-015).
- **Umschlag**: ein Inhaltsschlüssel, verschlüsselt (NIP-44) an den
  Geräteschlüssel genau eines Geräts.
- **Fortschrittsstand** (Versionsvektor): für jedes Ursprungsgerät der höchste
  Zeitstempel, bis zu dem ein Gerät alle Änderungen dieses Ursprungsgeräts hat
  („lückenloser Fortschritt“, FR-019). Zwei Geräte vergleichen ihre
  Fortschrittsstände und tauschen nur, was dem anderen fehlt.
- **Laufnummer**: die lückenlos aufsteigende Nummer je Ursprungsgerät und
  gemeinsamem Bereich (Specs 027, 028), die eine Grenze beim Entzug
  fälschungssicher macht (FR-019, FR-042). Im Bereich „Vault“ gibt es keine
  Laufnummern.
- **Mitgliederliste**: die von einem Gerät der Admin-Vault signierte Liste der
  Mitglieds-Vaults eines gemeinsamen Bereichs mit ihren Rechten und einer
  Generation (Specs 026–028). Diese Spec legt nur die Regeln fest, die alle
  gemeinsamen Bereiche teilen: die Grenze beim Entzug (FR-042), die Wahl
  zwischen Listen gleicher Generation (FR-043), die Umschläge an jedes Gerät
  (FR-039) und wer verwalten darf (FR-040).
- **Grenze**: je Gerät einer Vault, der ein Recht entzogen wird, die höchste
  Laufnummer, die das ausstellende Gerät beim Ausstellen der Liste von ihm
  angewendet hatte („Grenze beim Entzug“, FR-042). Die Geräteliste trägt für
  ein entferntes Gerät eine Grenze im Bereich „Vault“: den höchsten Zeitstempel
  dieses Geräts, den das ausstellende Hauptgerät angewendet hatte (FR-028).
- **Gerätelokale Daten**: Tabellen und Spalten, die ausdrücklich vom Sync
  ausgenommen sind (Endung `_no_sync`), etwa gespeicherte Sitzungen (Spec 022).
- **Sync-Server**: der nicht vertrauenswürdige, blinde Server aus Spec 026 für
  die Vaults vieler Nutzer, mit einem **Postfach** je Bereich. Im Entwurf und in
  Spec 026 heißt er bisher „das Relay“. Diese Spec baut ihn noch nicht, legt
  aber fest, welche Geräte er annehmen darf (FR-005, FR-009) und was nie als
  gewöhnliche Vault-Information in ein Postfach darf (FR-038).
- **Nostr-Relay**: ein öffentlicher Nachrichtenvermittler, über den sich die
  Geräte einer Vault finden (Präsenzmeldungen) und über den das Verknüpfen
  beginnt. Er speichert keine Vault-Daten.
- **iroh-Relay**: ein Verbindungshelfer, der verschlüsselte Bytes zwischen zwei
  Geräten weiterleitet, wenn keine direkte Verbindung zustande kommt. Er sieht
  keinen Inhalt und speichert nichts.
- „Relay“ allein steht in dieser Spec für nichts davon; in älteren
  Clarifications meint es den Sync-Server.
- **Wiederherstellungspaket**: das optionale, verschlüsselt beim Sync-Server
  hinterlegte Paket, mit dem ein Nutzer, der alle Geräte verloren hat, die
  Vault zurückholt (Spec 026, D31).

## Beziehung zu bestehenden Specs

- Entwurf [`2026-09-28-sync-architecture.md`](../../docs/plans/2026-09-28-sync-architecture.md):
  Diese Spec setzt §4 (Identitäten), §5 (gemeinsame Bausteine) und §6 (Ebene 1,
  Datensync zwischen eigenen Geräten) um, dazu die Grundlagen, die §14 schon
  für 024 verlangt: das Format der Bereiche und Schlüssel-Kennungen, die
  Stelle für echte Signaturprüfung, die Positivliste, die Vault-Geheimnisse auf
  der eigenen Vault hält, und das unveränderliche Feld „Ersteller“. Sie ist die
  Spec für Hauptgeräte, verknüpfte Geräte und die Geräteliste (D27).
- [`2026-09-04-v1-scope-design.md`](../../docs/plans/2026-09-04-v1-scope-design.md)
  §4: „Vault = SQLite = Identität“ und eigene Schlüssel je Gerät bleiben die
  Grundlage. Die Rolle Hauptgerät entspricht dem, was dort `pairing-authority`
  hieß; ein verknüpftes Gerät ist der Fall „weder noch“ dort (etwa eine
  Installation auf einem fremden Rechner). Statt gegenseitig signierter
  `peer_instances`-Einträge und Widerrufs-Epochen gilt die eine, mit der
  Vault-Identität signierte Geräteliste. `confirmation-authority` übernimmt
  diese Spec nicht. Die Aussage aus §5 dort, iroh trage keinen Zustand, gilt
  nicht mehr; der Datensync läuft über iroh.
- [`001-frontend-onboarding`](../001-frontend-onboarding/spec.md): „Öffnen“
  einer kopierten Vault-Datei bleibt, wie es ist, und wird zum Nebenweg, ein
  Gerät hinzuzufügen (User Story 7). Das Verknüpfen (User Story 5) ersetzt den
  nicht gebauten Ablauf „Verbinden“ aus Spec 001 (User Story 2, FR-015 bis
  FR-017 dort): Code oder QR bleiben, die gegenseitig signierten
  `peer_instances`-Einträge entfallen.
- [`013-vault-lifecycle-isolation`](../013-vault-lifecycle-isolation/spec.md)
  und ADR-0003: Ein App-Prozess ist eine Vault-Session. Der Sync läuft nur
  innerhalb dieser Session; Sperren oder Schließen beendet alle Verbindungen
  sofort (FR-031).
- [`014-portable-mode`](../014-portable-mode/spec.md): Ein Gerät im portablen
  Modus ist ein Gerät der Vault wie jedes andere. Alles, was der Sync speichert,
  unterliegt den Speicherregeln von Spec 014. Für einen fremden Rechner ist ein
  verknüpftes Gerät ohne den privaten Schlüssel der Vault-Identität der
  passende Weg.
- [`022-session-restore`](../022-session-restore/spec.md): Gespeicherte
  Sitzungen gehen nie an andere Geräte (FR-010 dort). Diese Spec nimmt
  gerätelokale Daten deshalb vom Sync aus (FR-016).
- [`023-settings-app`](../023-settings-app/spec.md): Die Kategorie „Föderation“
  zeigt die Geräte der Vault (FR-022 dort). Ihre Clarifications verschieben
  „zuletzt online“ und die Live-Aktualisierung auf die Sync-Spec; das ist diese
  Spec (User Story 4). Die Geräteliste wird zur Unteransicht „Geräte“ der
  Kategorie, mit den Rollen der Geräte und der öffentlichen Vault-Identität zum
  Lesen; die Specs 025–028 fügen die Unteransichten „Ordner“, „Relays“,
  „Spaces“ und „Datenfreigaben“ hinzu. Die neuen Knöpfe „Gerät verknüpfen“,
  „Gerät entfernen“ und „Aufnehmen“ sind Handlungen im Sinne von FR-021 dort.
- ADR-0001 (gerätebezogene Daten): Gerätebezogene Zeilen mit
  `vault_device_uuid` werden synchronisiert; nur gerätelokale Daten nicht.
- Die geplanten Specs zur **Steuerung durch Agenten** (017–021): Agenten dürfen
  die Geräteliste lesen, aber weder verknüpfen, aufnehmen noch entfernen und
  nie an Schlüssel kommen (FR-036).
- Folgende Specs bauen hierauf auf: **025** (Dateisync zwischen eigenen
  Geräten), **026** (Sync-Server mit Postfächern, auch für den Bereich „Vault“, damit
  Geräte sich angleichen, die nie gleichzeitig online sind, und das
  Wiederherstellungspaket, D31), **027** (Spaces), **028** (Datenfreigaben),
  **029** (eigener S3-Speicher). Sie verweisen auf diese Spec für die
  Geräteliste (FR-005) und das Entfernen (FR-026 bis FR-028), die
  Nur-direkt-Daten (FR-038), die Atomarität von Änderungspaketen und die
  Prüfung von Momentaufnahmen je Transaktionsgruppe (FR-013), den lückenlosen
  Fortschritt (FR-019), die Grenze beim Entzug (FR-042), die Mitgliederlisten
  gleicher Generation (FR-043), die Umschläge an jedes Gerät (FR-039), die
  Verwaltung durch jedes Gerät der Admin-Vault (FR-040) und die nicht
  übertragbare Admin-Rolle (FR-041).

## Clarifications

### Session 2026-09-27

- Q: Wen identifiziert ein Nostr-Schlüssel, die Vault oder das Gerät? → A: Das
  Gerät. Dieselbe Vault läuft gleichzeitig auf mehreren Geräten, jedes mit
  eigener Nostr-Identität; die Vault-Identität ist auf allen dieselbe (D1).
- Q: Werden Schlüssel wie in haex-vault 2.x (Legacy) über MLS verteilt? → A: Nein. Die feste
  Reihenfolge der MLS-Epochen passt nicht zu einem CRDT ohne Reihenfolge (D13).
  Schlüsselgenerationen sind gewöhnliche CRDT-Zeilen, die nebeneinander bestehen
  dürfen.

### Session 2026-09-28

- Q: Wo liegt der private Schlüssel der Vault-Identität? → A: Auf jedem Gerät
  der Vault, in der verschlüsselten Datenbank (D8). Damit kann jedes Gerät
  weitere Geräte aufnehmen. (überholt durch D27)
- Q: Wie wird ein verlorenes oder gestohlenes Gerät ausgesperrt? → A: Durch
  einen Wechsel der Vault-Identität für die verbleibenden Geräte (D8). Die
  Datenbank des verlorenen Geräts schützt weiter die Passphrase. (überholt
  durch D26 und D27)
- Q: Darf ein Server je Klartext der Vault sehen? → A: Nein. Das Relay ist nicht
  vertrauenswürdig (D11) und sieht keinen Inhalt (D9). Diese Spec baut das
  Relay noch nicht, legt aber das Format fest: Änderungen reisen immer
  verschlüsselt, auch direkt zwischen eigenen Geräten, und Vault-Geheimnisse
  verlassen nie die eigenen Geräte. (Präzisiert beim Plan: direkt über die
  verschlüsselte, geprüfte Verbindung; eigene Änderungspakete nur auf Wegen
  über Dritte.)
- Q: Was gehört in diese Spec, was in die folgenden? → A: Die Aufteilung aus §14
  des Entwurfs: 024 bringt Identitäten, direkte Verbindungen, Präsenz und den
  Datensync zwischen eigenen Geräten samt der Grundlagen für Bereiche,
  Autorenschaft und Positivliste. Dateien kommen mit 025, das Relay mit 026.
- Q: Wie kommt ein weiteres Gerät zur Vault? → A: Eine kopierte Vault-Datei
  funktioniert schon heute und bleibt der erste Weg (P1). Die Kopplung eines
  frisch installierten Geräts per QR oder Code ist P2, das Aussperren eines
  Geräts P3. (überholt durch D27: Verknüpfen ist P1, Kopie und Entfernen P2)
- Q: Welche Vault-Geheimnisse dürfen nie über ein Relay reisen, auch nicht
  über das Postfach der eigenen Vault? → A: Eine feste Liste, die
  Nur-direkt-Daten: der private Schlüssel der Vault-Identität, die
  Hauptzugangsdaten des Admins für S3 (Spec 029) und entpackte Inhalts- und
  Zugangsschlüssel. Alles Übrige der Vault darf verschlüsselt durch das
  eigene Postfach, das zum Bereich „Vault“ gehört (FR-018, FR-038). (überholt
  durch D30)
- Q: Wer regelt den Wechsel der Vault-Identität gegenüber Spaces und
  Datenfreigaben? → A: Diese Spec allein, über eine mit der alten Identität
  signierte Nachricht je gemeinsamem Bereich. (überholt durch D23 und D26)
- Q: Wird ein Änderungspaket ganz oder je Änderung geprüft? → A: Ein
  Änderungspaket ganz: Ist eine Änderung ungültig, fällt das ganze Paket.
  Eine Momentaufnahme je Änderung (FR-013). (Nach dem Review präzisiert: je
  vollständiger Transaktionsgruppe, nie teilweise. Beim Plan präzisiert:
  zwischen eigenen Geräten gibt es keine Pakete; dort wird je
  Transaktionsgruppe geprüft.)
- Q: Leitet ein Gerät Änderungen immer mit ihrem ursprünglichen Autor weiter?
  → A: Ja, mit einer Ausnahme: Die Vault des Eigentümers gibt eine Änderung
  zwischen überlappenden Datenfreigaben als neue, eigene Änderung aus
  (FR-021, Spec 028).
- Q: Bekommen Kopien einer Vault, die schon vor dieser Spec auf mehrere Geräte
  kopiert wurden, dieselbe Vault-Identität? → A: Ja. Die Identität wird aus dem
  gemeinsamen Platzhalter abgeleitet; die Kopien bleiben eine Vault (FR-004).
- Q: Welche Server nutzt holzi für Präsenz, NAT-Durchgang und Einladungen,
  solange es kein eigenes Relay gibt? → A: Voreingestellte öffentliche Nostr-
  und iroh-Relays, änderbar in den Einstellungen (FR-008).
- Q: Kann die Admin-Rolle übertragen werden? → A: Nein, in v1 gar nicht, und
  nichts hängt von der Zustimmung der Mitglieder ab (D23, FR-041). Der Teil
  dieser Antwort, der gemeinsame Bereiche bei einem Wechsel der
  Vault-Identität beendet, ist überholt durch D26.
- Q: Gibt es in v1 ein Rotieren der Vault-Identität? → A: Nein (D26). Ein
  verlorenes Gerät ist kein Problem, solange eine Kopie oder das Relay existiert
  und die Passphrase hält; ausgesperrt wird ein Gerät über die Geräteliste
  (D27).
- Q: Wer darf Geräte hinzufügen und entfernen? → A: Nur Hauptgeräte, die den
  privaten Schlüssel der Vault-Identität haben; es kann mehrere geben. Beim
  Verknüpfen fragt holzi, ob der Schlüssel mit übertragen wird (Standard:
  nein). Verknüpfen ist der bevorzugte Weg, Kopieren der Datei bleibt möglich
  (D27).
- Q: An wen werden Schlüssel von Spaces und Datenfreigaben verschlüsselt? → A:
  An jedes Gerät der Mitglieds-Vaults laut deren aktueller Geräteliste (D28).
- Q: Welche Geräte dürfen Spaces und Datenfreigaben verwalten? → A: Jedes Gerät
  der Admin-Vault, das auf ihrer aktuellen Geräteliste steht, signiert mit
  seinem Geräteschlüssel; nur das Verwalten der Geräte bleibt Hauptgeräten
  vorbehalten (D29, FR-040).
- Q: Was darf nur auf direkten Verbindungen reisen? → A: Nur noch der private
  Schlüssel der Vault-Identität, an ein neues Hauptgerät auf ausdrücklichen
  Wunsch und sonst nur verschlüsselt im Wiederherstellungspaket.
  Geräteschlüssel verlassen ihr Gerät gar nicht. Alles andere, auch
  Zugangsdaten für S3 und entpackte Inhaltsschlüssel, ist gewöhnliche
  Vault-Information (D30, FR-038).
- Q: Wie kommt ein Nutzer, der alle Kopien verloren hat, wieder an seine Vault?
  → A: Über ein optionales, verschlüsselt auf dem Relay hinterlegtes
  Wiederherstellungspaket. Abrufen erfordert den Besitznachweis des
  Wiederherstellungsschlüssels, ohne ihn zu übertragen, und einen zweiten
  Faktor (TOTP oder E-Mail-Link) (D31). Das regelt Spec 026.
- Q: Was passiert, wenn sich zwei Hauptgeräte gegenseitig entfernen? → A: Die
  gültige Liste derselben Generation mit dem kleinsten Hash ist maßgeblich. Ihre
  Entfernung zählt, die Entfernung der verlierenden Liste nicht; so bleibt das
  Hauptgerät der gewinnenden Liste erhalten. Eine zusammengeführte Liste der
  nächsten Generation führt diese Entfernung weiter.
- Q: Reisen Änderungen zwischen eigenen Geräten in gespeicherten, signierten
  Paketen mit Laufnummern? → A: Nein, das ist unnötig schwer. haex-crdt trägt
  in jedem Zeitstempel das Ursprungsgerät. Geräte vergleichen je Ursprungsgerät
  den höchsten Zeitstempel und schicken sich aus dem aktuellen Stand, was fehlt,
  in ganzen Transaktionen über die geprüfte, verschlüsselte Verbindung (FR-012,
  FR-019). Die Atomarität je Zeitstempel bleibt, wie haex-crdt sie heute
  sichert. Änderungspakete gibt es nur auf Wegen über Dritte (Spec 026),
  Laufnummern nur in gemeinsamen Bereichen (Specs 027, 028).
- Q: Werden die Schlüssel des Quellgeräts aus einer kopierten Vault-Datei
  gelöscht? → A: Nein. Eine Kopie ist ein vollständiges Backup und bleibt
  unverändert; jede Installation nutzt nur ihren eigenen Geräteschlüssel
  (FR-006).
- Q: Was meint „Relay“? → A: Nie allein. Es heißt Nostr-Relay (Präsenz,
  Verknüpfen), iroh-Relay (Verbindungshelfer) oder Sync-Server (Spec 026, bisher
  „das Relay“); ein weiteres eigenes Gerät ist ein Gerät der Vault.
- Q: Darf eine Geräteliste ohne Hauptgerät entstehen, wenn sich zwei
  Hauptgeräte gegenseitig entfernen? → A: Nein. Jede gültige Liste muss
  mindestens ein Hauptgerät nennen. Beim gleichzeitigen gegenseitigen Entfernen
  gilt die Liste mit dem kleinsten Hash (FR-043) samt ihrer Entfernung; die
  verlierende Liste ist für Geräte und Entfernungen nicht maßgeblich. Genau ein
  Hauptgerät bleibt, das andere wird zum Solitär (FR-005).
- Q: Lässt sich fälschungssicher prüfen, welches Hauptgerät eine Geräteliste
  ausgestellt hat? → A: Nein. Alle Hauptgeräte haben denselben privaten
  Schlüssel der Vault-Identität; auch ein entferntes Hauptgerät kann damit
  signieren. Der Aussteller steht in der Liste nur als Angabe. Das ist die
  hingenommene Grenze aus FR-028 und der Warnung in FR-024.

## User Scenarios & Testing _(mandatory)_

### User Story 1 - Zwei eigene Geräte halten die Vault gleich (Priority: P1)

Eine Nutzerin hat ihre Vault auf dem Laptop und hat ihren Desktop-Rechner damit
verknüpft (User Story 5). Sobald beide Geräte laufen, finden sie sich und
verbinden sich. Ein Chat, den sie am Laptop beginnt, steht wenige Sekunden
später auch am Desktop; ändert sie am Desktop eine Einstellung der Vault, gilt
sie gleich darauf auch am Laptop. Was sie tut, während eines der Geräte aus
ist, holt das andere nach, sobald beide wieder laufen. Dass der Laptop ein
Hauptgerät und der Desktop ein verknüpftes Gerät ist, merkt sie an den Daten
nicht.

**Why this priority**: Das ist der Kern der Spec und das erste Ziel des
Entwurfs. Ohne Sync ist jede Instanz einer Vault eine eigene Abzweigung.

**Independent Test**: Zwei Geräte auf derselben Geräteliste (verknüpft oder,
bis User Story 5 steht, als Kopie der Datei eines Hauptgeräts, User Story 7),
auf beiden öffnen, auf jedem Gerät etwas ändern: Jede Änderung erscheint auf dem
anderen. Ein Gerät beenden, auf dem anderen weiterarbeiten, das erste wieder
öffnen: Es holt alles nach.

**Acceptance Scenarios**:

1. **Given** zwei Geräte stehen auf der Geräteliste der Vault und haben sie
   offen, im selben Netz oder im Internet erreichbar, **When** sie laufen,
   **Then** verbinden sie sich ohne Zutun der Nutzerin.
2. **Given** die Geräte sind verbunden, **When** die Nutzerin auf einem Gerät
   Daten der Vault anlegt, ändert oder löscht, **Then** zeigt das andere Gerät
   dieselbe Änderung, ohne dass sie neu laden muss.
3. **Given** ein Gerät war aus, während auf dem anderen Änderungen entstanden,
   **When** beide wieder verbunden sind, **Then** hat das zurückgekehrte Gerät
   alle diese Änderungen, und beide haben denselben Stand.
4. **Given** beide Geräte ändern offline dasselbe Feld desselben Eintrags,
   **When** sie sich verbinden, **Then** haben beide danach denselben Wert (die
   jüngere Änderung gilt), und alle übrigen Felder beider Änderungen bleiben
   erhalten.
5. **Given** gerätelokale Daten wie die gespeicherte Sitzung eines Geräts
   (Spec 022), **When** die Geräte synchronisieren, **Then** kommen diese Daten
   nie auf dem anderen Gerät an.
6. **Given** ein Hauptgerät und ein verknüpftes Gerät, **When** sie
   synchronisieren, **Then** kommen Änderungen in beide Richtungen gleich an,
   aber der private Schlüssel der Vault-Identität gelangt nie auf das
   verknüpfte Gerät (FR-002).
7. **Given** der Laptop war bisher das einzige Gerät der Vault und
   veröffentlicht deshalb keine Präsenz, **When** ein zweites Gerät auf die
   Geräteliste kommt, **Then** veröffentlichen von da an beide Präsenz, finden
   sich und verbinden sich (FR-007).

---

### User Story 2 - Nur eigene Geräte kommen an die Daten (Priority: P1)

Die Nutzerin hat eine zweite, private Vault auf demselben Laptop und ihr
Kollege hat seine eigene Vault. Keines dieser fremden Geräte bekommt Daten ihrer
Vault, auch wenn es sich als eines ihrer Geräte ausgibt. Nur Geräte, die auf der
aktuellen Geräteliste ihrer Vault stehen, bekommen etwas. Wer den Netzverkehr
mitliest, sieht keinen Inhalt. Der private Schlüssel der Vault-Identität
verlässt ein Hauptgerät nur, wenn sie das beim Verknüpfen ausdrücklich wählt.

**Why this priority**: Die Sicherheit ist die Voraussetzung dafür, den Sync
überhaupt einzuschalten. Sie muss vom ersten Tag an stimmen, nicht erst mit dem
Sync-Server.

**Independent Test**: Ein Gerät mit einer anderen Vault, ein Gerät mit einem
Geräteschlüssel, der nicht auf der Geräteliste steht, und ein Gerät mit einer
gefälschten Geräteliste versuchen, sich mit einem Gerät der Vault zu verbinden:
Alle werden abgewiesen, bevor irgendein Inhalt fließt. Den Verkehr zwischen zwei
eigenen Geräten mitschneiden: Er enthält keinen Klartext der Vault.

**Acceptance Scenarios**:

1. **Given** ein Gerät mit einer anderen Vault, **When** es sich mit einem Gerät
   dieser Vault verbinden will, **Then** wird die Verbindung abgewiesen, und es
   erhält keine Daten und keine Änderungen.
2. **Given** ein Gerät, dessen Geräteschlüssel nicht auf der aktuellen
   Geräteliste der Vault steht oder das den Besitz seines Geräteschlüssels nicht
   beweist, **When** es sich verbinden will, **Then** wird es abgewiesen.
3. **Given** eine Geräteliste, die nicht mit dieser Vault-Identität signiert
   ist, **When** ein Gerät der Vault sie erhält, **Then** verwirft es sie und
   behält die letzte gültige Geräteliste.
4. **Given** zwei eigene Geräte synchronisieren, **When** jemand den Verkehr
   mitliest, **Then** sieht er weder Inhalte noch Tabellen-, Spalten- oder
   Schlüsselnamen der Vault.
5. **Given** Daten einer Verbindung wurden unterwegs verändert, **When** sie
   ankommen, **Then** endet die Verbindung, und nichts aus der unvollständigen
   Lieferung wird angewendet.
6. **Given** die Präsenzmeldung eines Geräts, **When** jemand sie liest, der
   kein Gerät dieser Vault ist, **Then** erfährt er daraus nicht, wo das Gerät
   erreichbar ist.

---

### User Story 3 - Keine Änderung geht verloren oder doppelt, der Autor stimmt (Priority: P1)

Die Nutzerin hat drei Geräte: Laptop, Desktop und ein Arbeitsgerät. Laptop und
Arbeitsgerät sind nie gleichzeitig an. Der Desktop läuft meistens. Was sie am
Laptop ändert, kommt über den Desktop auf das Arbeitsgerät und umgekehrt. Jede
Änderung kommt genau einmal an, und bei jeder Änderung steht weiter, auf
welchem Gerät sie entstand, nicht, über welches Gerät sie kam.

**Why this priority**: Über indirekte Wege gehen mit einem einfachen „zuletzt
gesendet“ Änderungen verloren, und das Feld für das Gerät im Änderungsdatensatz
von haex-crdt nennt das weiterleitende Gerät (Entwurf §3.2). Spaces und
Datenfreigaben (027, 028) brauchen den richtigen Autor für ihre Rechte.

**Independent Test**: Drei Geräte; A und C sind nie gleichzeitig mit B
verbunden. Auf A und C abwechselnd Änderungen erzeugen, jeweils nur mit B
synchronisieren: Am Ende haben alle drei denselben Stand, keine Änderung fehlt
oder ist doppelt, und jede nennt ihr Ursprungsgerät.

**Acceptance Scenarios**:

1. **Given** Gerät A hat Änderungen nur mit B ausgetauscht, **When** B sich
   danach mit C verbindet, **Then** erhält C alle Änderungen von A.
2. **Given** C hat Änderungen von A schon über B erhalten, **When** A und C sich
   später direkt verbinden, **Then** tauschen sie diese Änderungen nicht noch
   einmal aus, und keine wird doppelt angewendet.
3. **Given** eine Änderung von A kam über B bei C an, **When** C ihren Autor
   anzeigt oder prüft, **Then** ist der Autor Gerät A der Vault, nicht B.
4. **Given** eine Verbindung bricht mitten im Austausch ab, **When** die Geräte
   sich wieder verbinden, **Then** fehlt keine Änderung, keine ist doppelt, und
   keine zusammengehörige Gruppe von Änderungen wurde nur zum Teil angewendet.
5. **Given** C hat von A über B nur einen Teil der Änderungen erhalten, weil die
   Verbindung abbrach, **When** C sich danach mit A oder einem anderen Gerät
   verbindet, **Then** fordert C die Änderungen von A ab seinem
   Fortschrittsstand an, und danach fehlt keine ältere Änderung von A.

---

### User Story 4 - Föderation, Geräte: Rollen, Identität und wer online ist (Priority: P2)

In den Einstellungen, Kategorie „Föderation“, Unteransicht „Geräte“, sieht die
Nutzerin ihre Geräte. Bei jedem steht, ob es ein Hauptgerät oder ein
verknüpftes Gerät ist, ob es gerade verbunden ist oder wann es zuletzt online
war. Oben steht die öffentliche Vault-Identität, die die Vault in Mitgliederlisten
und Rechten von Spaces und Datenfreigaben benennt; sie kann sie kopieren, aber
nicht ändern. Schaltet sie ein Gerät ein, springt es in der Liste auf „online“;
benennt sie ein Gerät auf diesem um, steht der neue Name gleich darauf auch auf
den anderen. Ein neu hinzugekommenes Gerät erscheint, sobald es auf der
Geräteliste steht.

**Why this priority**: Spec 023 hat „zuletzt online“ und die Live-Liste
ausdrücklich auf diese Spec verschoben. Die Nutzerin sieht hier, ob der Sync
arbeitet und welche Geräte Geräte verwalten dürfen.

**Independent Test**: Ein Hauptgerät A und ein verknüpftes Gerät B; auf A die
Unteransicht „Geräte“ offen lassen. B einschalten, umbenennen, beenden: A zeigt
B nacheinander als online, mit neuem Namen und dann mit „zuletzt online“ und
passender Zeit, ohne neu zu laden, und durchgehend mit der Rolle „verknüpftes
Gerät“. Auf beiden Geräten steht dieselbe öffentliche Vault-Identität, nicht
änderbar.

**Acceptance Scenarios**:

1. **Given** die Unteransicht „Geräte“, **When** ein anderes Gerät gerade
   verbunden ist, **Then** steht es als „online“ da.
2. **Given** ein Gerät ist nicht verbunden, **When** die Liste es zeigt,
   **Then** steht dort, wann es zuletzt online war, oder „noch nie online
   gesehen“, wenn dieses Gerät davon nichts weiß.
3. **Given** die Unteransicht ist offen, **When** ein Gerät online geht, offline
   geht, umbenannt, hinzugefügt oder entfernt wird, **Then** aktualisiert sich
   die Liste ohne Zutun.
4. **Given** ein Gerät, mit dem kein Sync möglich ist (andere, inkompatible
   Version von holzi), **When** die Liste es zeigt, **Then** steht dort, dass
   eines der Geräte aktualisiert werden muss.
5. **Given** die Liste, **When** sie ein Gerät zeigt, **Then** steht dabei seine
   Rolle „Hauptgerät“ oder „verknüpftes Gerät“, und dieses Gerät ist als
   solches markiert.
6. **Given** die Unteransicht auf einem beliebigen Gerät der Vault, **When** die
   Nutzerin sie öffnet, **Then** sieht sie den öffentlichen Schlüssel der
   Vault-Identität, kann ihn kopieren, aber nicht ändern; den privaten Schlüssel
   zeigt holzi nirgends.
7. **Given** ein verknüpftes Gerät, **When** die Nutzerin dort die Unteransicht
   öffnet, **Then** fehlen „Gerät verknüpfen“ und „Gerät entfernen“, und holzi
   sagt, dass das nur auf einem Hauptgerät geht.

---

### User Story 5 - Gerät verknüpfen (Priority: P1)

Die Nutzerin installiert holzi auf einem neuen Rechner, auf dem noch keine
Vault liegt. Auf dem Laptop, einem Hauptgerät, wählt sie in „Föderation“,
Unteransicht „Geräte“, den Knopf „Gerät verknüpfen“; der Laptop zeigt einen
QR-Code und denselben Code als Text. Auf dem neuen Rechner wählt sie auf der
Startseite „Mit einer Vault verknüpfen“, scannt den Code oder gibt ihn ein,
vergibt einen Gerätenamen und eine Passphrase für diese Instanz. Der Laptop
zeigt den Namen des neuen Geräts und fragt, ob es auch ein Hauptgerät werden
soll; vorausgewählt ist „nein“, darunter steht: „Wird ein Hauptgerät
kompromittiert und ist die Passphrase bekannt, ist die Vault verloren
(Totalausfall).“ Sie lässt „nein“ stehen und bestätigt. Der Laptop trägt den
neuen Rechner als verknüpftes Gerät in die Geräteliste ein, gibt ihm die
Schlüssel der Vault und überträgt sie; danach synchronisiert der neue Rechner
wie jedes andere Gerät.

**Why this priority**: Verknüpfen ist der bevorzugte Weg zu einem weiteren
Gerät (D27). Nur so bekommt das Gerät ausdrücklich eine Rolle, und nur so
entsteht ein Gerät ohne den privaten Schlüssel der Vault-Identität, dessen
Verlust die Vault nicht gefährdet.

**Independent Test**: Auf einem Hauptgerät einen Code erzeugen, auf einer
frischen Installation eingeben, auf dem Hauptgerät ohne Hauptgerät-Rolle
bestätigen: Die frische Installation hat danach die Vault mit allen Daten, steht
als verknüpftes Gerät auf der Geräteliste beider Geräte, synchronisiert weiter
und besitzt den privaten Schlüssel der Vault-Identität nicht. Denselben Ablauf
mit Hauptgerät-Rolle wiederholen: Das neue Gerät kann danach selbst Geräte
verknüpfen.

**Acceptance Scenarios**:

1. **Given** ein Hauptgerät, **When** die Nutzerin „Gerät verknüpfen“ wählt,
   **Then** zeigt es einen einmal verwendbaren, kurzlebigen Code als QR-Code und
   als Text.
2. **Given** eine frische Installation und ein gültiger Code, **When** die
   Nutzerin ihn dort eingibt, **Then** zeigt das Hauptgerät den Namen des neuen
   Geräts und wartet auf ihre Bestätigung, bevor es irgendetwas überträgt.
3. **Given** das Hauptgerät wartet auf die Bestätigung, **When** es sie
   erfragt, **Then** fragt es auch, ob das neue Gerät ein Hauptgerät werden
   soll, mit „nein“ vorausgewählt und der Warnung vor dem Totalausfall.
4. **Given** die Nutzerin bestätigt ohne Hauptgerät-Rolle, **When** die
   Übertragung abgeschlossen ist, **Then** steht das neue Gerät als
   verknüpftes Gerät auf der Geräteliste, hat alle Daten der Vault außer den
   gerätelokalen des Hauptgeräts und alle Generationen des Inhaltsschlüssels des
   Bereichs „Vault“ an seinen Geräteschlüssel verpackt, aber nicht den privaten
   Schlüssel der Vault-Identität.
5. **Given** die Nutzerin wählt die Hauptgerät-Rolle, **When** die Übertragung
   abgeschlossen ist, **Then** hat das neue Gerät zusätzlich den privaten
   Schlüssel der Vault-Identität, erhalten nur über diese direkte Verbindung,
   steht als Hauptgerät auf der Geräteliste und kann selbst Geräte verknüpfen
   und entfernen.
6. **Given** ein abgelaufener, schon benutzter oder falsch eingegebener Code,
   **When** er eingegeben wird, **Then** schlägt das Verknüpfen mit einer klaren
   Meldung fehl, und es wird nichts übertragen.
7. **Given** die Nutzerin lehnt auf dem Hauptgerät ab oder bricht ab, **When**
   das passiert, **Then** erhält die neue Installation nichts, die Geräteliste
   bleibt unverändert, und der Code ist verbraucht.

---

### User Story 6 - Gerät entfernen (Priority: P2)

Das Arbeitsgerät der Nutzerin, ein verknüpftes Gerät, wurde gestohlen. Auf dem
Laptop, einem Hauptgerät, wählt sie in „Föderation“, Unteransicht „Geräte“,
beim gestohlenen Gerät „Gerät entfernen“. holzi erklärt, was passiert: Das
Gerät bekommt keine neuen Daten mehr und kann neue Änderungen nicht mehr
entschlüsseln; was schon darauf ist, bleibt dort und schützt nur die
Passphrase; ein Löschen aus der Ferne gibt es nicht. Sie bestätigt. Der Laptop
veröffentlicht eine neue Geräteliste ohne das Arbeitsgerät und eine neue
Generation des Inhaltsschlüssels für die verbleibenden Geräte. Der Desktop
übernimmt beides bei der nächsten Verbindung, ohne dass sie dort etwas
bestätigen muss. Ihre Vault-Identität bleibt, und ihre Spaces und
Datenfreigaben laufen weiter; der Sync-Server und die Geräte ihres Kollegen nehmen
vom gestohlenen Gerät nichts mehr an, sobald sie die neue Geräteliste kennen.

**Why this priority**: Selten, aber ohne diesen Weg bleibt ein gestohlenes Gerät
für immer Teil der Vault. Weil es in v1 keinen Wechsel der Vault-Identität gibt
(D26), ist die Geräteliste der Weg, ein Gerät auszusperren (D27).

**Independent Test**: Drei Geräte, eines davon ein Hauptgerät; auf ihm ein
anderes entfernen: Das entfernte Gerät kann sich mit keinem der beiden mehr
verbinden, erhält keine neue Änderung und kann nichts entschlüsseln, was mit
der neuen Generation verschlüsselt ist (etwa Präsenzmeldungen); die beiden
verbleibenden synchronisieren weiter, die
Vault-Identität ist dieselbe. Ist die Vault Admin eines Space und Mitglied eines
anderen (Spec 027), bleibt sie beides.

**Acceptance Scenarios**:

1. **Given** die Unteransicht „Geräte“ auf einem Hauptgerät, **When** die
   Nutzerin ein anderes Gerät entfernen will, **Then** erklärt holzi vor der
   Bestätigung die Folgen und dass das Gerät nur durch ein neues Verknüpfen
   zurückkommt.
2. **Given** das Entfernen ist bestätigt, **When** es abgeschlossen ist,
   **Then** gibt es eine Geräteliste mit höherer Generation, die das Gerät nicht
   mehr nennt und seine Grenze trägt, und eine neue Generation des
   Inhaltsschlüssels des Bereichs „Vault“, die nur an die verbleibenden Geräte
   verpackt ist; das Gerät verschwindet aus der Liste.
3. **Given** ein verbleibendes Gerät kennt die neue Geräteliste noch nicht,
   **When** es sie erhält, **Then** übernimmt es sie ohne Zutun der Nutzerin.
4. **Given** das Entfernen ist bekannt, **When** das entfernte Gerät sich
   verbinden will, **Then** wird es von jedem Gerät abgewiesen, das die neue
   Geräteliste kennt; Änderungen des entfernten Geräts mit einem Zeitstempel
   jenseits seiner Grenze lehnt jedes solche Gerät ab, auch wenn ein anderes
   Gerät sie weiterleitet.
5. **Given** Änderungen, die das entfernte Gerät vor dem Entfernen geschrieben
   hat und die innerhalb seiner Grenze liegen, **When** das Entfernen
   abgeschlossen ist, **Then** bleiben sie erhalten, auch wenn sie erst später
   über ein anderes Gerät ankommen.
6. **Given** nach dem Entfernen verschlüsseln die verbleibenden Geräte mit der
   neuen Generation (Präsenzmeldungen, Namen in der Geräteliste, ab Spec 026
   das Postfach beim Sync-Server), **When** das entfernte Gerät solche Daten in
   die Hände bekommt, **Then** kann es sie nicht entschlüsseln.
7. **Given** die Vault ist Mitglied oder Admin eines Space oder einer
   Datenfreigabe (Specs 027, 028), **When** ein Gerät entfernt wird, **Then**
   bleiben Vault-Identität, Admin-Rollen und Mitgliedschaften unverändert; der
   Sync-Server und die Geräte der Mitglieder weisen das entfernte Gerät ab, sobald
   sie die neue Geräteliste kennen, und niemand sonst muss etwas tun.
8. **Given** die Nutzerin will ein Hauptgerät entfernen, **When** holzi die
   Folgen erklärt, **Then** sagt es zusätzlich, dass das nur gegen ein
   ehrliches Gerät hilft: Hat jemand das Hauptgerät und kennt die Passphrase,
   ist die Vault verloren (Totalausfall).
9. **Given** ein verknüpftes Gerät oder dieses Gerät selbst, **When** die
   Nutzerin in der Liste nach „Gerät entfernen“ sucht, **Then** bietet holzi es
   auf einem verknüpften Gerät gar nicht und für dieses Gerät selbst nirgends
   an.

---

### User Story 7 - Kopie der Vault-Datei als Nebenweg (Priority: P2)

Die Nutzerin kopiert die Vault-Datei ihres Laptops, eines Hauptgeräts, auf
einen Zweitrechner und öffnet sie dort mit ihrer Passphrase. Weil die Kopie den
privaten Schlüssel der Vault-Identität enthält, erzeugt sie ihren eigenen
Geräteschlüssel, trägt sich selbst als Hauptgerät in die Geräteliste ein, sagt
der Nutzerin, dass dieser Rechner nun ein Hauptgerät ist, und synchronisiert.
Später kopiert sie die Datei ihres verknüpften Desktops auf ein weiteres Gerät.
Diese Kopie hat den privaten Schlüssel nicht; sie zeigt „wartet auf Aufnahme
durch ein Hauptgerät“ und synchronisiert nichts. Auf dem Laptop erscheint die
Aufnahmeanfrage mit dem Namen des Geräts; die Nutzerin wählt „Aufnehmen“, und
die Kopie wird ein verknüpftes Gerät.

**Why this priority**: Das Kopieren funktioniert heute schon und bleibt
möglich, ist aber nur ein Nebenweg (D27): Die Rolle folgt aus der kopierten
Datei statt aus einer Wahl, und die Kopie eines Hauptgeräts ist immer ein
weiteres Hauptgerät.

**Independent Test**: Die Datei eines Hauptgeräts kopieren und öffnen: Die Kopie
steht ohne weiteres Zutun als Hauptgerät auf der Geräteliste und synchronisiert.
Die Datei eines verknüpften Geräts kopieren und öffnen: Die Kopie sendet und
erhält keine Änderung, bis ein Hauptgerät sie aufnimmt; danach kommen auch die
Änderungen an, die sie in der Zwischenzeit gemacht hat.

**Acceptance Scenarios**:

1. **Given** eine Vault-Datei wurde auf ein anderes Gerät kopiert, **When** sie
   dort zum ersten Mal geöffnet wird, **Then** erzeugt die Kopie ihren eigenen
   Geräteschlüssel und verwendet den des Quellgeräts nicht; die Datei bleibt
   sonst unverändert, auch der Schlüssel des Quellgeräts darin.
2. **Given** die Kopie stammt von einem Hauptgerät, **When** sie zum ersten Mal
   geöffnet wird, **Then** trägt sie sich als Hauptgerät in eine neue
   Geräteliste ein, sagt der Nutzerin, dass dieses Gerät ein Hauptgerät ist, und
   synchronisiert wie jedes andere Gerät.
3. **Given** die Kopie stammt von einem verknüpften Gerät, **When** sie zum
   ersten Mal geöffnet wird, **Then** sendet sie eine Aufnahmeanfrage, zeigt
   „wartet auf Aufnahme durch ein Hauptgerät“ und sendet und erhält bis zur
   Aufnahme keine Änderung.
4. **Given** eine Aufnahmeanfrage, **When** die Nutzerin auf einem Hauptgerät
   „Aufnehmen“ wählt, **Then** steht die Kopie als verknüpftes Gerät auf der
   Geräteliste, erhält alle Generationen des Inhaltsschlüssels an ihren
   Geräteschlüssel verpackt und synchronisiert, auch die Änderungen, die sie vor
   der Aufnahme gemacht hat; wählt die Nutzerin „Ablehnen“, bleibt die Kopie
   draußen.
5. **Given** die Aufnahmeanfrage kam bei einem verknüpften Gerät an, **When**
   dieses sich später mit einem Hauptgerät synchronisiert, **Then** erscheint
   die Anfrage auch dort.
6. **Given** der Laptop war bisher das einzige Gerät der Vault und
   veröffentlicht deshalb keine Präsenz, **When** die kopierte Vault-Datei auf
   dem Zweitrechner zum ersten Mal geöffnet wird, **Then** findet der Laptop die
   Kopie über deren Präsenzmeldung, die beiden verbinden sich, und von da an
   veröffentlichen beide Präsenz (FR-007).

---

### Edge Cases

- Eine Vault hat nur ein einziges Gerät: Es veröffentlicht im Bereich „Vault“
  keine Präsenz und baut keine direkten Verbindungen auf, lauscht aber auf
  Präsenzmeldungen an seine Vault (FR-007). Uploads in das Postfach beim
  Sync-Server (Spec 026) und der Verkehr mit Mitgliedern von Spaces und
  Datenfreigaben (Specs 027, 028) bleiben erlaubt.
- Die Vault-Datei eines solchen Geräts wird kopiert: Die Kopie kennt das
  Quellgerät aus der Geräteliste in der Datei und veröffentlicht deshalb
  Präsenz; das Quellgerät erfährt so von ihr, übernimmt ihre Geräteliste oder
  zeigt ihre Aufnahmeanfrage, und ab dann veröffentlichen beide (FR-007).
  Voneinander unabhängige Vaults mit je einem Gerät bleiben still.
- Eine Vault-Datei wird kopiert, samt der gerätelokalen Daten, die in der Datei
  liegen (ADR-0001): Das Zielgerät verwendet keinen Geräteschlüssel des
  Quellgeräts, sondern erzeugt einen eigenen (FR-006). Es löscht nichts aus der
  Datei; die Kopie bleibt ein vollständiges Backup.
- Zwei Geräte treten mit demselben Geräteschlüssel oder derselben Gerätekennung
  auf, etwa nach dem Klonen einer ganzen virtuellen Maschine: holzi erkennt
  das, synchronisiert mit keinem der beiden weiter und sagt der Nutzerin, dass
  ein Gerät doppelt vorhanden ist (FR-030).
- Zwei eigene Geräte sind nie gleichzeitig online und es gibt kein drittes: Sie
  gleichen sich in dieser Spec nicht an; das leistet erst das Postfach beim
  Sync-Server (Spec 026). Die Geräteliste zeigt für beide „zuletzt online“.
- Ein Gerät läuft mit einer älteren holzi-Version, deren Vault-Schema die
  Änderungen des anderen nicht kennt: Die Geräte synchronisieren nicht
  miteinander, bis beide passend aktualisiert sind; keine Änderung geht
  verloren, beide zeigen den Grund (FR-029).
- Die Uhr eines Geräts geht stark falsch: Die Reihenfolge der Änderungen
  richtet sich nach der hybriden logischen Uhr von haex-crdt, nicht allein nach
  der Wanduhr; „zuletzt online“ kann auf diesem Gerät ungenau sein.
- Die Nutzerin sperrt oder schließt die Vault mitten in einem Austausch: Alle
  Verbindungen enden sofort (Spec 013); eine halb empfangene Lieferung wird
  nicht angewendet und beim nächsten Mal ab dem Fortschrittsstand neu geholt.
- Eine Transaktion von A wurde auf B schon zum Teil von einer jüngeren Änderung
  überschrieben, bevor C sie von B holt: C erhält von dieser Transaktion nur
  die Spalten, die auf B noch gelten, und die jüngere Änderung dazu; die alten
  Werte gibt es auf B nicht mehr. Kommen beide in
  derselben Lieferung, sieht C nie einen Zwischenstand; liegen sie in zwei
  Lieferungen, kann C kurz einen Stand zeigen, den es so auf keinem Gerät gab,
  bis die zweite ankommt. Der Endstand ist auf allen Geräten derselbe; das ist
  die Arbeitsweise von haex-crdt und hingenommen.
- Ein Gerät im portablen Modus (Spec 014) auf einem fremden Rechner: Es ist ein
  gewöhnliches Gerät der Vault; der passende Weg ist ein verknüpftes Gerät ohne
  Hauptgerät-Rolle. Wer es danach nicht mehr in der Vault haben will, entfernt
  es (User Story 6).
- Zwei Hauptgeräte ändern gleichzeitig die Geräteliste, etwa eines nimmt ein
  Gerät auf, das andere entfernt eines: Unter den Listen derselben Generation
  gilt überall dieselbe (FR-043); ein Hauptgerät, das beide sieht,
  veröffentlicht eine zusammengeführte Liste der nächsten Generation, und ein
  in einer der beiden Listen entferntes Gerät bleibt entfernt (FR-005).
- Zwei Hauptgeräte entfernen sich gleichzeitig gegenseitig: Das ist kein
  Wettlauf, sondern eindeutig. Unter den beiden Listen derselben Generation
  gilt die mit dem kleinsten Hash samt ihrer Geräte- und Entfernungsmenge
  (FR-043). Die Entfernung der anderen Liste zählt nicht. Auf allen Geräten,
  die beide Listen kennen, bleibt so unabhängig von der Reihenfolge des
  Eintreffens genau eines der beiden Hauptgerät; das andere ist entfernt und
  wird zum Solitär. Eine Geräteliste ohne Hauptgerät entsteht dadurch nie.
- Die Nutzerin entfernt ein Gerät, während keines ihrer anderen Geräte online
  ist: Die neue Geräteliste erreicht die anderen Geräte (und ab Spec 026 den
  Sync-Server), sobald ein Weg besteht. Bis dahin kann das entfernte Gerät mit
  Geräten synchronisieren, die die Liste noch nicht kennen; das ist ein
  hingenommenes Risiko. Ein böswilliges entferntes Gerät kann einem solchen
  Gerät in dieser Zeit auch Änderungen mit zurückdatiertem Zeitstempel
  unterhalb seiner Grenze geben; diese gelten danach überall. Sie verlieren
  gegen jede jüngere Änderung derselben Zelle. Auch das ist hingenommen: Im
  Bereich „Vault“ vertrauen sich die eigenen Geräte; eine Grenze, die
  Zurückdatieren ausschließt, brauchen erst die gemeinsamen Bereiche mit
  fremden Vaults (FR-042).
- Ein Dieb hat ein verknüpftes Gerät und kennt dessen Passphrase: Bis zum
  Entfernen kann er Daten der Vault lesen und schreiben und Spaces und
  Datenfreigaben verwalten (D29). Geräte hinzufügen oder entfernen kann er
  nicht. Nach dem Entfernen nimmt niemand mehr etwas von ihm an.
- Ein Dieb hat ein Hauptgerät und kennt dessen Passphrase: Er kann eigene Geräte
  aufnehmen und die der Nutzerin entfernen. Das ist der Totalausfall, vor dem
  holzi beim Verknüpfen warnt (FR-024); Hauptgerät zu sein ist deshalb nicht
  der Standard.
- Alle Hauptgeräte sind verloren, verknüpfte Geräte gibt es noch: Die
  verknüpften Geräte synchronisieren weiter, aber niemand kann Geräte
  hinzufügen oder entfernen, bis eine Kopie der Datei eines Hauptgeräts oder
  das Wiederherstellungspaket (Spec 026) wieder ein Hauptgerät schafft.
- Die Kopie der Datei eines entfernten Geräts wird geöffnet: Stammt sie von
  einem verknüpften Gerät, sendet sie eine Aufnahmeanfrage, die die Nutzerin
  ablehnen kann; stammt sie von einem Hauptgerät, kann sie sich selbst wieder
  eintragen, weil sie den privaten Schlüssel der Vault-Identität hat (FR-028,
  hingenommen).
- Eine Vault aus einer Zeit vor dieser Spec hat nur einen Platzhalter als
  Vault-Identität und wurde schon auf mehrere Geräte kopiert (FR-004).
- Während des Verknüpfens verliert eine Seite die Verbindung: Das Verknüpfen
  bricht ab, die neue Installation behält nichts Halbes, die Geräteliste bleibt
  unverändert, und die Nutzerin beginnt mit einem neuen Code.

## Requirements _(mandatory)_

### Functional Requirements

**Identitäten und Geräteliste**

- **FR-001**: Beim Anlegen einer Vault MUSS holzi eine echte Vault-Identität
  (secp256k1-Schlüsselpaar aus einer kryptographisch sicheren Zufallsquelle)
  erzeugen und in der verschlüsselten Vault speichern. Diese erste Instanz ist
  ein Hauptgerät und MUSS die erste Geräteliste mit sich selbst als einzigem
  Gerät signieren. Der Platzhalter von heute DARF für neue Vaults nicht mehr
  entstehen.
- **FR-002**: Der private Schlüssel der Vault-Identität DARF NIE in Protokollen,
  Diagnosen, Fehlermeldungen, der Oberfläche, für Erweiterungen oder für Agenten
  erscheinen. Er ist das einzige Nur-direkt-Datum (FR-038): Er DARF NIE mit dem
  gewöhnlichen Sync reisen, auch nicht auf direkten Verbindungen, und DARF NIE
  auf ein verknüpftes Gerät gelangen. Ein Hauptgerät DARF ihn nur an ein neues
  Hauptgerät beim Verknüpfen geben, wenn die Nutzerin das ausdrücklich wählt
  (FR-024), oder verschlüsselt in das Wiederherstellungspaket legen (Spec 026).
  Eine Kopie der Vault-Datei eines Hauptgeräts enthält ihn; das ist Sache der
  Nutzerin (FR-044).
- **FR-003**: Jedes Gerät MUSS für jede Vault, die es öffnet, einen eigenen
  Geräteschlüssel haben, der beim ersten Öffnen der Vault auf diesem Gerät
  entsteht. Mit ihm weist sich das Gerät aus und signiert, was es über Dritte
  veröffentlicht (Begriffe), und an ihn gehen die Umschläge für dieses Gerät.
  Ein Geräteschlüssel DARF NIE synchronisiert,
  über das Netz übertragen oder von einem anderen Gerät verwendet werden.
- **FR-004**: Eine Vault, die vor dieser Spec angelegt wurde und nur den
  Platzhalter trägt, MUSS beim ersten Öffnen mit dieser Version eine echte
  Vault-Identität bekommen. Kopien derselben Vault, die schon vor dieser Spec
  auf mehrere Geräte kopiert wurden, MÜSSEN dabei dieselbe Vault-Identität
  bekommen, abgeleitet aus dem gemeinsamen Platzhalter; sie bleiben eine Vault
  und synchronisieren miteinander. Jede dieser Kopien hat damit den privaten
  Schlüssel, ist ein Hauptgerät und trägt sich selbst in die Geräteliste ein;
  gleichzeitig entstandene Listen führen die Kopien nach FR-043 zusammen. Wie
  abgeleitet wird, klärt der Plan.
- **FR-005** („Geräteliste“): Jede Vault MUSS eine Geräteliste führen, die ein
  Hauptgerät mit dem privaten Schlüssel der Vault-Identität signiert. Sie nennt
  jedes aktuelle Gerät mit öffentlichem Geräteschlüssel, Rolle („Hauptgerät“
  oder „verknüpftes Gerät“), Namen beim Aufnehmen und Netzwerkkennung; den
  Namen MUSS sie verschlüsselt tragen, sodass nur Geräte der eigenen Vault ihn
  lesen können, während Sync-Server und Mitglieder nur Schlüssel, Rolle und
  Netzwerkkennung sehen. Dazu nennt sie
  jedes entfernte Gerät mit seiner Grenze (FR-028) und trägt eine Generation.
  Nur Hauptgeräte DÜRFEN Gerätelisten ausstellen. Eine gültige Liste höherer
  Generation ersetzt eine niedrigere; für Listen gleicher Generation gilt
  FR-043. Ein Empfänger MUSS jede Liste verwerfen, deren Signatur nicht zur
  Vault-Identität passt, und die letzte gültige behalten; kann er keine gültige
  Liste prüfen, DARF er mit keinem Gerät dieser Vault synchronisieren (fail
  closed). Ein Gerät, das in der maßgeblichen gültigen Liste als entfernt steht,
  MUSS für diesen Empfänger entfernt bleiben, auch wenn eine andere Liste
  derselben Generation es noch nennt. Bei Listen derselben Generation ist nur die
  Liste mit dem kleinsten Hash für Geräte und Entfernungen maßgeblich;
  Entfernungen der verlierenden Liste zählen nicht. So bleibt beim gegenseitigen
  Entfernen zweier Hauptgeräte genau eines Hauptgerät, und jede gültige
  Geräteliste nennt mindestens ein Hauptgerät. Der Aussteller einer Liste ist
  nur eine Angabe und nicht fälschungssicher (Clarifications). Die Geräteliste ist gewöhnliche
  Vault-Information und reist mit dem Sync; der Sync-Server (Spec 026) und die Geräte
  der Mitglieder von Spaces und Datenfreigaben (Specs 027, 028) prüfen jedes
  Gerät gegen die aktuelle Geräteliste seiner Vault.
- **FR-006**: Öffnet eine Installation eine Vault-Datei, die von einem anderen
  Gerät kopiert wurde, MUSS sie einen neuen Geräteschlüssel für sich erzeugen
  und DARF den Geräteschlüssel des Quellgeräts NICHT verwenden, auch wenn er in
  der Datei liegt. Sie DARF ihn und die übrigen Daten des Quellgeräts NICHT
  löschen: Eine Kopie bleibt ein vollständiges Backup und eine vollwertige
  Instanz. Sie DARF erst synchronisieren, wenn ihr Geräteschlüssel auf der
  aktuellen Geräteliste steht (FR-044).

**Finden und Verbinden**

- **FR-007**: Jedes Gerät, dessen Geräteliste mindestens ein weiteres Gerät
  nennt, MUSS, solange die Vault offen ist, eine Präsenzmeldung
  veröffentlichen, die nur Geräte derselben Vault entschlüsseln können und die
  angibt, wie das Gerät gerade erreichbar ist. Eine Präsenzmeldung MUSS von
  ihrem Gerät signiert sein und nach kurzer Zeit ablaufen, wenn sie nicht
  erneuert wird. Ein Gerät, dessen Geräteliste nur es selbst nennt, DARF im
  Bereich „Vault“ keine Präsenzmeldung veröffentlichen und keine direkte
  Verbindung aufbauen, außer während eines Verknüpfens (FR-023) oder als
  Antwort auf eine gültige Präsenzmeldung eines anderen Geräts derselben
  Vault. Es MUSS trotzdem, solange die Vault offen ist, auf Präsenzmeldungen an
  seine Vault lauschen. So findet ein Quellgerät eine frische Kopie seiner
  Vault-Datei: Die Kopie kennt das Quellgerät aus der Geräteliste der kopierten
  Datei und veröffentlicht deshalb Präsenz, als Gerät der neuen Geräteliste,
  wenn sie sich als Hauptgerät selbst eingetragen hat, sonst mit ihrer
  Aufnahmeanfrage (FR-044); das Quellgerät übernimmt die neue Geräteliste nach
  FR-005 oder legt die Anfrage nach FR-045 ab, und sobald die Kopie auf der
  Liste steht, veröffentlichen beide. Voneinander unabhängige Vaults mit je
  einem Gerät bleiben still. Die Einschränkung gilt nur für Präsenz und
  direkten Sync im Bereich „Vault“: Uploads in das Postfach beim Sync-Server
  (Spec 026) und der Verkehr mit Mitgliedern von Spaces und Datenfreigaben
  (Specs 027, 028) bleiben auch mit einem einzigen Gerät erlaubt.
- **FR-008**: Geräte derselben Vault MÜSSEN sich über die Präsenzmeldungen
  finden und selbständig eine direkte Verbindung aufbauen, im selben Netz wie
  über das Internet, auch hinter üblichen Heimroutern. Die Nutzerin MUSS dafür
  keine Adresse eingeben. holzi MUSS dafür voreingestellte öffentliche
  Nostr-Relays und iroh-Relays nutzen, die der Nutzer in den Einstellungen
  ändern oder durch eigene Server ersetzen kann. Dieselben Server tragen auch
  die Antworten auf Einladungen zu Spaces und Datenfreigaben (Spec 027, FR-041).
- **FR-009**: Beim Verbindungsaufbau MÜSSEN beide Seiten ihren Geräteschlüssel
  nennen, seinen Besitz beweisen und die Generation ihrer Geräteliste nennen;
  kennt eine Seite eine höhere Generation, MUSS sie diese Liste vorlegen, und
  die andere prüft und übernimmt sie nach FR-005. Ein Gerät DARF eine
  Verbindung im Bereich „Vault“ nur annehmen, wenn der Geräteschlüssel der
  Gegenseite auf der aktuellen Geräteliste der eigenen Vault steht und nicht als
  entfernt gilt. Vor dieser Prüfung DARF außer der Geräteliste kein Inhalt und
  keine Änderung fließen. Ausgenommen sind nur das Verknüpfen (FR-024) und
  die Aufnahmeanfrage einer Kopie (FR-044); dabei fließt vor der Aufnahme
  nichts aus der Vault.
- **FR-010**: Geräte MÜSSEN die Verbindung selbständig wieder aufbauen, wenn sie
  abbricht oder sich die Erreichbarkeit eines Geräts ändert (anderes Netz,
  Aufwachen aus dem Ruhezustand).

**Übertragung, Änderungspakete und Bereiche**

- **FR-011**: Jede Änderung MUSS genau einem Bereich angehören. In dieser Spec
  ist das immer der Bereich „Vault“; das Format MUSS weitere Bereiche (Spaces,
  Datenfreigaben) ohne Änderung aufnehmen können.
- **FR-012**: Zwischen eigenen Geräten MÜSSEN Änderungen über eine nach FR-009
  geprüfte, verschlüsselte direkte Verbindung reisen, in vollständigen
  Transaktionsgruppen aus dem aktuellen Stand des sendenden Geräts; ein
  eigenes Änderungspaket oder ein gespeichertes Protokoll gesendeter Änderungen
  ist dafür nicht nötig. Wo Änderungen über Dritte reisen oder dort liegen
  (Postfach beim Sync-Server, Spec 026; gemeinsame Bereiche, Specs 027, 028),
  MÜSSEN sie in Änderungspaketen reisen: Ein Änderungspaket MUSS seinen Bereich
  und die Kennung seines Inhaltsschlüssels offen tragen und alles andere
  (Tabellen, Spalten, Schlüssel der Zeilen, Zeitstempel, Werte, Autoren)
  verschlüsselt, und ein Gerät DARF es unverändert weitergeben können, ohne es
  zu öffnen.
- **FR-013**: Eine zusammengehörige Gruppe von Änderungen (eine Transaktion,
  also die Änderungen mit gemeinsamem Zeitstempel der hybriden logischen Uhr)
  DARF NIE auf mehrere Lieferungen oder Pakete aufgeteilt und NIE zum Teil
  angewendet werden, wie haex-crdt es heute sichert. Auf einer direkten
  Verbindung zwischen eigenen Geräten und bei einer Momentaufnahme MUSS der
  Empfänger je vollständiger Transaktionsgruppe prüfen: Ist eine Änderung
  ungültig, verwirft er ihre ganze Transaktionsgruppe; die übrigen gültigen
  Gruppen übernimmt er. Das verfeinert die Clarification „Eine Momentaufnahme
  je Änderung“: geprüft wird jede Änderung, verworfen oder übernommen wird je
  Gruppe. Ein Änderungspaket (Specs 026–028) ist dagegen atomar: Ist auch nur
  eine Änderung darin ungültig (etwa Signatur, Autor, Recht, Bereich oder Feld
  „Ersteller“ nach FR-022), MUSS der Empfänger das ganze Paket verwerfen;
  angewendet wird es ganz oder gar nicht. Specs 026 und 027 wenden diese Regeln
  an.
- **FR-014**: Ein Empfänger MUSS ein Änderungspaket verwerfen, das sich nicht
  entschlüsseln lässt, verändert wurde oder zu einem anderen Bereich gehört als
  angegeben, und DARF davon nichts anwenden. Auf einer direkten Verbindung
  schützt die Verbindung selbst vor Veränderung; schlägt ihre Prüfung fehl,
  endet sie, und aus der unvollständigen Lieferung DARF nichts angewendet
  werden.
- **FR-015**: Der Bereich „Vault“ MUSS einen Inhaltsschlüssel haben, der nur in
  der Vault liegt. Jede seiner Generationen MUSS an den Geräteschlüssel jedes
  Geräts der aktuellen Geräteliste verpackt sein (je ein Umschlag); ein
  Hauptgerät verpackt beim Verknüpfen (FR-024) und beim Aufnehmen (FR-045) alle
  bisherigen Generationen an das neue Gerät. Eine neue Generation entsteht mit
  jedem Entfernen eines Geräts (FR-026) und ist nur an die verbleibenden Geräte
  verpackt. Ein Gerät, das eine Geräteliste mit einem entfernten Gerät kennt,
  MUSS alles, was es danach mit dem Inhaltsschlüssel verschlüsselt
  (Präsenzmeldungen, Namen in der Geräteliste, ab Spec 026 Änderungspakete), mit
  einer Generation verschlüsseln, die nicht an dieses Gerät verpackt ist. Ein
  Empfänger MUSS alles mit dem Schlüssel entschlüsseln, den dessen Kennung
  nennt, ohne dass eine „aktuelle“ Generation vereinbart sein muss.

**Was synchronisiert wird**

- **FR-016**: Der Sync MUSS alle Daten der Vault umfassen außer den
  gerätelokalen Daten und dem privaten Schlüssel der Vault-Identität (FR-002).
  Gerätelokale Daten DÜRFEN NIE ein Gerät über den Sync verlassen.
- **FR-017**: Anlegen, Ändern und Löschen MÜSSEN ankommen; eine Löschung MUSS
  auf allen Geräten gelten, auch auf solchen, die den Eintrag zwischendurch noch
  geändert hatten. Gleichzeitige Änderungen desselben Felds MÜSSEN auf allen
  Geräten zum selben Ergebnis führen.
- **FR-018**: Welche Daten die eigene Vault in einen anderen Bereich als „Vault“
  verlassen dürfen (den Bereich eines Space oder einer Datenfreigabe), MUSS
  eine ausdrückliche Positivliste festlegen, keine Ausschlussliste. Das
  Postfach der eigenen Vault beim Sync-Server (Spec 026) ist kein anderer
  Bereich, sondern gehört zum Bereich „Vault“; was dort nicht hin darf, regelt
  FR-038. Der Inhaltsschlüssel des Bereichs „Vault“, der private Schlüssel der
  Vault-Identität, die Geräteschlüssel und alle anderen Vault-Geheimnisse, auch
  entpackte Schlüssel und Zugangsdaten, DÜRFEN auf dieser Liste nie stehen;
  eine neue Tabelle ist nicht darauf, solange sie nicht ausdrücklich
  eingetragen wird.
- **FR-038** („Nur-direkt-Daten“): Einziges Nur-direkt-Datum ist der private
  Schlüssel der Vault-Identität (D30). Er DARF ein Hauptgerät nur auf einer
  direkten, nach FR-009 geprüften Verbindung zu einem neuen Hauptgerät
  verlassen, wenn die Nutzerin das beim Verknüpfen ausdrücklich wählt
  (FR-024), und sonst nur verschlüsselt im Wiederherstellungspaket (Spec 026,
  D31); nie mit dem gewöhnlichen Sync, nie als Vault-Information in einem
  Postfach beim Sync-Server und nie in einem anderen Bereich als „Vault“.
  Geräteschlüssel verlassen ihr Gerät überhaupt nicht (FR-003). Alle übrigen
  Daten der Vault außer den gerätelokalen sind gewöhnliche Vault-Information,
  auch die Geräteliste, die Umschläge, entpackte Inhaltsschlüssel von Spaces und
  Datenfreigaben, entpackte Zugangsschlüssel und Zugangsdaten für S3 (Spec 029)
  sowie der Dateiindex samt den Schlüsseln je Datei der eigenen
  synchronisierten Ordner (Spec 025): Sie reisen mit dem Sync, auch durch das
  Postfach der eigenen Vault, verschlüsselt mit dem Inhaltsschlüssel des
  Bereichs „Vault“, und lassen sich so auf jedem Gerät der Vault
  wiederherstellen. Specs 025, 026, 028 und 029 verweisen auf diese Regel; ein
  neues Vault-Geheimnis, das nie den Sync-Server erreichen darf, MUSS hier
  eingetragen werden.

**Fortschritt und Autorenschaft**

- **FR-019** („lückenloser Fortschritt“): Jedes Gerät MUSS seinen Fortschritt
  im Bereich „Vault“ je Ursprungsgerät führen, nicht als einen einzigen
  Zeitpunkt für alle: Der Fortschrittsstand ist je Ursprungsgerät der höchste
  Zeitstempel der hybriden logischen Uhr, bis zu dem das Gerät alle Änderungen
  dieses Ursprungsgeräts hat, soweit sie nicht schon von jüngeren Änderungen
  überschrieben sind. Der Zeitstempel entscheidet weiter über Konflikte
  (FR-017). Zwei Geräte MÜSSEN beim Verbinden ihre Fortschrittsstände
  vergleichen und nur übertragen, was dem anderen fehlt, in beide Richtungen.
  Der Sender MUSS die Änderungen je Ursprungsgerät in aufsteigender Folge der
  Zeitstempel liefern, und der Empfänger DARF seinen Fortschrittsstand nur im
  selben Schritt erhöhen, in dem er die gelieferten Änderungen anwendet. So
  entsteht keine Lücke, auch nicht über Zwischengeräte oder nach einem Abbruch.
  In gemeinsamen Bereichen (Specs 027, 028) MUSS zusätzlich jede Änderung eine
  Laufnummer tragen, lückenlos aufsteigend je Ursprungsgerät und Bereich und
  von ihm signiert (FR-021); dort ist der Fortschrittsstand je Ursprungsgerät
  die höchste Laufnummer, bis zu der alle Änderungen vorliegen. Eine Änderung
  jenseits einer Lücke zählt dann nicht zum Fortschrittsstand, die Lücke wird
  ausdrücklich angefordert, und eine zweite, andere Änderung unter einem schon
  belegten Paar aus Ursprungsgerät und Laufnummer wird als Fälschung
  verworfen. Diese Laufnummern tragen die Grenze beim Entzug (FR-042). Wie sie
  vergeben werden, legen Specs 027 und 028 fest; diese Spec nutzt sie nicht.
- **FR-020**: Über jeden Weg zwischen den Geräten, auch über Zwischengeräte,
  MUSS jede Änderung jedes Gerät erreichen, sobald ein Verbindungsweg besteht.
  Kommt eine Änderung mehrfach an, DARF das keine zweite Wirkung haben. Eine
  abgebrochene Übertragung MUSS sich ab dem Fortschrittsstand fortsetzen
  lassen, ohne Änderungen zu verlieren.
- **FR-021**: Jede Änderung MUSS ihr Ursprungsgerät und ihre Vault als Autor
  tragen. Im Bereich „Vault“ ist das Ursprungsgerät die Gerätekennung im
  Zeitstempel der Änderung. Ein weiterleitendes Gerät DARF den Autor NICHT
  verändern; er MUSS auch in gespeicherten und weitergegebenen Ständen
  erhalten bleiben. Im Bereich „Vault“ bürgt das liefernde eigene Gerät für die
  Echtheit: direkt durch die Prüfung der Verbindung (FR-009), über den
  Sync-Server durch seine Signatur (Spec 026). Eine Signatur des
  Ursprungsgeräts je Änderung ist dort nicht nötig. In gemeinsamen Bereichen
  (Specs 027, 028) MUSS jede Änderung mit dem Geräteschlüssel des
  Ursprungsgeräts signiert sein, und ein weiterleitendes Gerät DARF die
  Signatur NICHT verändern. Einzige Ausnahme ist die Neuausgabe durch die
  Vault des Eigentümers (Admin der
  Datenfreigabe) zwischen überlappenden Datenfreigaben (Spec 028): Sie gibt
  eine weitergeleitete Änderung als neue Änderung im Bereich der zweiten
  Datenfreigabe aus, signiert von einem Gerät der Eigentümer-Vault, die damit
  ihr Autor ist; der ursprüngliche Autor bleibt nur als Anzeige erhalten, nicht
  als signierender Autor. In gemeinsamen Bereichen zählt eine Signatur nur,
  wenn das Ursprungsgerät auf der aktuellen Geräteliste seiner Vault steht oder
  die Änderung innerhalb seiner Grenze liegt; in jedem Bereich zählt eine
  Änderung eines entfernten Ursprungsgeräts nur innerhalb seiner Grenze
  (FR-028).

**Grundlagen für spätere Bereiche**

- **FR-022**: holzi MUSS für Tabellen, die es verlangen, ein unveränderliches
  Feld „Ersteller“ unterstützen: Es wird beim Anlegen einer Zeile mit der Vault
  des Autors gefüllt und DARF danach von niemandem geändert werden; ein
  Empfänger MUSS jede Änderung dieses Felds ablehnen. In dieser Spec nutzt es
  noch keine Tabelle; Specs 025, 027 und 028 bauen darauf auf.
- **FR-039** („Umschläge an jedes Gerät“): Rechte und Mitgliederlisten nennen
  Vaults, aber jede Schlüsselgeneration eines Space oder einer Datenfreigabe
  wird an jedes Gerät der aktuellen Geräteliste jeder Mitglieds-Vault verpackt
  (D28; Specs 027, 028). Die eigene Vault MUSS jeden Schlüssel, den eines ihrer
  Geräte so erhält und entpackt, als gewöhnliche Vault-Information ablegen
  (FR-038), sodass ein später verknüpftes oder aufgenommenes Gerät die
  bisherigen Schlüssel über den Sync der eigenen Vault erhält, ohne dass der
  Admin online sein muss; spätere Generationen verpackt der Admin direkt an es,
  sobald er die neue Geräteliste kennt. Der Admin erfährt dadurch, wie viele
  Geräte eine Mitglieds-Vault hat; das ist hingenommen. Diese Spec bringt die
  Ablage in der eigenen Vault; das Verpacken regeln Specs 027 und 028.
- **FR-040** („Jedes Gerät der Admin-Vault verwaltet“): Handlungen des Admins in
  Spaces und Datenfreigaben (anlegen, einladen, Rechte ändern, Mitglieder
  entfernen, Mitgliederlisten ausstellen, Momentaufnahmen des Bereichs) DARF
  jedes Gerät der Admin-Vault ausführen, Hauptgerät wie verknüpftes Gerät
  (D29). Es signiert mit seinem Geräteschlüssel; die Signatur gilt, solange das
  Gerät auf der aktuellen Geräteliste der Admin-Vault steht. Allgemein ist ein
  Gerät in einem gemeinsamen Bereich berechtigt, wenn sein Geräteschlüssel auf
  der aktuellen Geräteliste einer Vault steht, die auf der Mitgliederliste des
  Bereichs steht, und nur im Rahmen der Rechte dieser Vault. Nur das Verwalten
  der Geräte bleibt Hauptgeräten vorbehalten (FR-005). Specs 026–028 wenden das
  an.
- **FR-041** („Admin-Rolle nicht übertragbar“): In v1 DARF die Admin-Rolle eines
  Bereichs nie auf eine andere Vault übergehen, weder auf Wunsch noch bei
  Verlust; keine Regel dieser Spec DARF von der Zustimmung der Mitglieder
  abhängen (D23). Weil die Vault-Identität in v1 nie wechselt (D26) und Geräte
  über die Geräteliste kommen und gehen, bleibt die Admin-Rolle bei der Vault,
  solange diese ein Gerät auf ihrer Geräteliste hat. Specs 027 und 028 wenden
  das an.
- **FR-042** („Grenze beim Entzug“): Eine neue Mitgliederliste, die eine Vault
  entfernt oder ihre Rechte senkt, MUSS eine Grenze tragen: für jedes Gerät der
  betroffenen Vault die höchste Laufnummer (FR-019), die das ausstellende Gerät
  der Admin-Vault in diesem Moment von ihm angewendet hatte. Jedes Gerät, das
  diese Liste kennt, MUSS jede Änderung der betroffenen Vault mit einer
  Laufnummer jenseits der Grenze ihres Geräts ablehnen, die das entzogene Recht
  braucht, egal welchen Zeitstempel sie trägt. Rückdatieren ist nicht möglich,
  weil die Laufnummern bis zur Grenze schon vergeben sind (FR-019). Ein Gerät
  der betroffenen Vault, das in der Grenze fehlt, gilt mit Grenze null.
  Änderungen, die ein Gerät angewendet hatte, bevor es die Liste kannte,
  bleiben (das hingenommene Fenster aus D19); die nächste berechtigte Änderung
  derselben Zelle behebt die Abweichung. Keine Regel DARF sich auf den
  Zeitstempel einer Änderung im Verhältnis zur Liste stützen. Das verfeinert
  D19: Die Prüfung beim Empfang richtet sich nach der Grenze, nicht nach der
  Zeit. Specs 027 und 028 wenden diese Regel an. Im Bereich „Vault“ gibt es
  keine Rechte und keine Laufnummern; dort trägt nur die Geräteliste für ein
  entferntes Gerät eine Grenze als Zeitstempel (FR-028).
- **FR-043** („Mitgliederlisten gleicher Generation“): Mitgliederlisten
  veröffentlichen nur Geräte der Admin-Vault (FR-040), aber zwei solche Geräte
  können verschiedene gültige Listen mit derselben Generation veröffentlichen.
  Unter gültigen Listen derselben Generation MUSS die Liste mit dem
  lexikographisch kleinsten Hash gelten, beim Sync-Server (Spec 026) wie bei
  jedem Empfänger. Der Sync-Server MUSS eine gespeicherte Liste derselben Generation durch
  eine mit kleinerem Hash ersetzen, statt sie abzuweisen, und DARF sie durch
  eine mit größerem Hash NICHT ersetzen. Ein Gerät der Admin-Vault, das zwei
  verschiedene Listen derselben Generation sieht, MUSS eine Liste mit der
  nächsthöheren Generation veröffentlichen, die die Änderungen beider Listen
  zusammenführt. Dieselbe Regel gilt für Gerätelisten (FR-005): Ein
  Hauptgerät, das zwei verschiedene Gerätelisten derselben Generation sieht,
  MUSS eine zusammengeführte Liste der nächsthöheren Generation
  veröffentlichen, die den geltenden Stand der Liste mit dem kleinsten Hash und
  neue zulässige Geräte aus der anderen Liste zusammenführt und die geltenden
  Entfernungen weiterführt. Specs 026, 027 und 028 wenden diese Regel an.

**Gerät verknüpfen (P1)**

- **FR-023**: Ein Hauptgerät MUSS auf Wunsch der Nutzerin einen
  Verknüpfungscode erzeugen, als QR-Code und als Text. Der Code MUSS einmal
  verwendbar sein, nach spätestens 10 Minuten ablaufen und genug Zufall
  enthalten, dass er sich nicht erraten lässt. Er enthält, wie die neue
  Installation das Hauptgerät erreicht. Ein verknüpftes Gerät DARF keinen
  Verknüpfungscode erzeugen.
- **FR-024**: Eine frische Installation MUSS auf ihrer Startseite das
  Verknüpfen anbieten: Code scannen oder eingeben, Gerätename und Passphrase
  für diese Instanz vergeben; die Instanz erzeugt dabei ihren Geräteschlüssel
  (FR-003). Beide Seiten MÜSSEN beweisen, dass sie den Code kennen, bevor
  irgendetwas übertragen wird. Das Hauptgerät MUSS danach den Namen des neuen
  Geräts zeigen und fragen, ob es auch ein Hauptgerät werden soll: vorausgewählt
  „nein“, mit der Warnung „Wird ein Hauptgerät kompromittiert und ist die
  Passphrase bekannt, ist die Vault verloren (Totalausfall).“ Erst nach der
  Bestätigung der Nutzerin MUSS das Hauptgerät den Stand der Vault übertragen,
  alle Generationen des Inhaltsschlüssels des Bereichs „Vault“ an den
  Geräteschlüssel des neuen Geräts verpacken und eine neue Geräteliste mit dem
  neuen Gerät in der gewählten Rolle signieren. Den privaten Schlüssel der
  Vault-Identität DARF es nur übertragen, wenn die Nutzerin die Rolle
  Hauptgerät gewählt hat, und nur auf dieser direkten Verbindung (FR-038).
- **FR-025**: Die Passphrase DARF beim Verknüpfen NIE übertragen werden. Die
  neue Geräteliste DARF das Hauptgerät erst veröffentlichen, wenn die
  Übertragung abgeschlossen ist. Bricht das Verknüpfen vorher ab, MUSS die neue
  Installation alles verwerfen, was sie bis dahin erhalten hat, und die
  Geräteliste bleibt unverändert.

**Gerät entfernen (P2)**

- **FR-026**: Ein Hauptgerät MUSS jedes andere Gerät der Vault entfernen können.
  Das Entfernen MUSS eine neue Geräteliste mit höherer Generation ausstellen,
  die das Gerät nicht mehr als aktuelles Gerät nennt und es mit seiner Grenze
  als entfernt führt (FR-028), und eine neue Generation des Inhaltsschlüssels
  des Bereichs „Vault“ erzeugen, die nur an die verbleibenden Geräte verpackt
  ist (FR-015). Vor der Bestätigung MUSS holzi die Folgen erklären: keine neuen
  Daten mehr für das Gerät, was die Vault danach verschlüsselt, kann es nicht
  entschlüsseln,
  vorhandene Daten bleiben dort und schützt nur die Passphrase, kein Löschen
  aus der Ferne, Spaces und Datenfreigaben laufen unverändert weiter, zurück
  nur durch neues Verknüpfen; bei einem Hauptgerät zusätzlich, dass das
  Entfernen nur gegen ein ehrliches Gerät hilft. Vault-Identität, Admin-Rollen
  und Mitgliedschaften bleiben unverändert (D26). Ein Gerät DARF sich NICHT
  selbst entfernen; entfernt wird es nur von einem anderen Hauptgerät. Hat das
  entfernte Gerät die geltende Mitgliederliste eines Space oder einer
  Datenfreigabe unterschrieben, deren Admin die Vault ist, MUSS das entfernende
  Hauptgerät diese Listen im selben Vorgang mit höherer Generation neu
  unterschreiben und hochladen (Specs 027, 028). Geheimnisse wie Einträge im
  Passwortmanager oder S3-Zugangsdaten DARF holzi beim Entfernen NICHT
  automatisch ändern: Ob sie als kompromittiert gelten und ersetzt werden,
  entscheidet die Nutzerin.
- **FR-027**: Jedes verbleibende Gerät MUSS eine gültige neue Geräteliste und
  die neue Schlüsselgeneration ohne Zutun der Nutzerin übernehmen (FR-005);
  eine Bestätigung auf jedem Gerät ist nicht nötig, weil nur ein Hauptgerät
  eine gültige Geräteliste signieren kann. Die Geräteliste MUSS auf denselben
  Wegen reisen wie alle Vault-Information, direkt und über das Postfach der
  eigenen Vault (Spec 026), und der Sync-Server und die Geräte der Mitglieder von
  Spaces und Datenfreigaben erhalten sie ebenso (Specs 026–028).
- **FR-028** („Grenze beim Entfernen“): Die Geräteliste, die ein Gerät entfernt,
  MUSS für dieses Gerät im Bereich „Vault“ den höchsten Zeitstempel nennen,
  bis zu dem das ausstellende Hauptgerät in diesem Moment Änderungen von ihm
  hatte (sein Fortschrittsstand für dieses Gerät, FR-019); die Grenze in
  gemeinsamen Bereichen ergänzen Specs 027 und 028 als Laufnummer (FR-042).
  Jedes Gerät, das diese Liste kennt, MUSS Verbindungen des entfernten Geräts
  abweisen und jede seiner Änderungen jenseits der Grenze ablehnen, auch wenn
  ein anderes Gerät sie weiterleitet; Änderungen innerhalb der Grenze bleiben
  gültig, auch wenn sie erst später über ein anderes Gerät ankommen. Was ein
  Gerät vorher angewendet hatte, bleibt wie in FR-042. Dass ein böswilliges
  entferntes Gerät im Bereich „Vault“ zurückdatierte Änderungen unterhalb
  seiner Grenze über ein Gerät einschleusen kann, das die Liste noch nicht
  kennt, ist hingenommen (Edge Cases).
  Daten, die schon auf dem entfernten Gerät liegen, bleiben dort; ein Löschen
  aus der Ferne gibt es nicht. Ein entferntes Hauptgerät hat weiter den
  privaten Schlüssel der Vault-Identität und kann damit neue Gerätelisten
  signieren; das Entfernen eines Hauptgeräts wirkt deshalb nur gegen ein
  ehrliches Gerät (hingenommen, siehe die Warnung in FR-024).

**Kopie der Vault-Datei (P2)**

- **FR-044** („Kopie der Vault-Datei“): Enthält eine Kopie beim ersten Öffnen
  den privaten Schlüssel der Vault-Identität (Kopie eines Hauptgeräts), MUSS sie
  sich als Hauptgerät in eine neue Geräteliste eintragen, und holzi MUSS der
  Nutzerin sagen, dass dieses Gerät nun ein Hauptgerät ist. Enthält sie ihn
  nicht (Kopie eines verknüpften Geräts), MUSS sie eine mit ihrem neuen
  Geräteschlüssel signierte Aufnahmeanfrage mit Namen, öffentlichem
  Geräteschlüssel und Netzwerkkennung an die Geräte der Vault senden und
  „wartet auf Aufnahme durch ein Hauptgerät“ zeigen. Bis zur Aufnahme DARF sie
  keine Änderung senden oder erhalten; was sie in dieser Zeit ändert, bleibt
  auf ihr und synchronisiert nach der Aufnahme wie jede andere Änderung.
- **FR-045** („Aufnahmeanfrage“): Jedes Gerät der Vault, das eine gültige
  Aufnahmeanfrage erhält, MUSS sie als offene Anfrage in der Vault ablegen,
  sodass sie über den Sync jedes Hauptgerät erreicht. Ein Hauptgerät MUSS
  offene Anfragen in der Unteransicht „Geräte“ mit dem Namen des Geräts und
  den Knöpfen „Aufnehmen“ und „Ablehnen“ zeigen. „Aufnehmen“ MUSS die Kopie als
  verknüpftes Gerät in eine neue Geräteliste eintragen und alle Generationen des
  Inhaltsschlüssels des Bereichs „Vault“ an ihren Geräteschlüssel verpacken;
  „Ablehnen“ verwirft die Anfrage. holzi DARF eine Kopie NIE ohne diese
  Handlung der Nutzerin aufnehmen.

**Verträglichkeit und Betrieb**

- **FR-029**: Geräte mit unverträglichen Versionen des Sync oder des
  Vault-Schemas DÜRFEN nicht miteinander synchronisieren. Beide MÜSSEN den
  Grund zeigen (FR-034), und keine Änderung DARF dadurch verloren gehen; nach
  dem Aktualisieren synchronisieren sie ohne weiteres Zutun. (Ergänzt durch Spec 059: die Liste zeigt die
  Werte, die verglichen werden, und nennt das zu aktualisierende Gerät.)
- **FR-030**: Treten zwei Geräte mit demselben Geräteschlüssel oder derselben
  Gerätekennung auf, MUSS holzi den Sync mit beiden anhalten und der Nutzerin
  melden, dass ein Gerät doppelt vorhanden ist.
- **FR-031**: Sperren oder Schließen der Vault MUSS alle Sync-Verbindungen und
  laufenden Übertragungen sofort beenden (innerhalb der Fristen von Spec 013);
  danach DARF das Gerät für diese Vault keine Präsenz mehr erneuern und nichts
  mehr senden oder annehmen.
- **FR-032**: Der Sync MUSS im Hintergrund laufen, ohne die Bedienung zu
  blockieren; empfangene Änderungen MÜSSEN in offenen Fenstern und Tabs ohne
  Neuladen erscheinen.

**Föderation in den Einstellungen**

- **FR-033**: Die Geräteliste in der Unteransicht „Geräte“ der Kategorie
  „Föderation“ (Spec 023 FR-022) MUSS je Gerät den Namen und die Rolle
  („Hauptgerät“ oder „verknüpftes Gerät“) zeigen, dieses Gerät als solches
  markieren und je anderem Gerät zeigen, ob es gerade verbunden ist
  („online“), sonst wann es zuletzt online war, oder „noch nie online
  gesehen“. „Zuletzt online“ ist der jüngste Zeitpunkt, den dieses Gerät von
  dem anderen kennt, aus eigener Verbindung, aus Präsenzmeldungen oder über ein
  drittes Gerät.
- **FR-034**: Die Liste MUSS sich live aktualisieren, wenn ein Gerät online oder
  offline geht, umbenannt, hinzugefügt oder entfernt wird; ein entferntes Gerät
  verschwindet aus ihr. Ein Gerät, mit dem nach FR-029 oder FR-030 kein Sync
  möglich ist, MUSS den Grund zeigen. Steht dieses Gerät selbst nicht auf der
  Geräteliste, MUSS die Unteransicht das sagen: „wartet auf Aufnahme durch ein
  Hauptgerät“ (FR-044) oder, wenn es von seiner Entfernung erfährt, dass es aus
  der Vault entfernt wurde und nicht mehr synchronisiert.
- **FR-035**: Auf einem Hauptgerät MUSS die Unteransicht „Geräte“ den Knopf
  „Gerät verknüpfen“ (FR-023), an jedem anderen Gerät „Gerät entfernen“
  (FR-026) und die offenen Aufnahmeanfragen (FR-045) anbieten. Auf einem
  verknüpften Gerät DÜRFEN diese Knöpfe NICHT erscheinen; stattdessen sagt
  holzi, dass Geräte nur auf einem Hauptgerät verknüpft, aufgenommen und
  entfernt werden. Das Entfernen dieses Geräts selbst DARF NICHT angeboten
  werden.
- **FR-046** („öffentliche Vault-Identität“): Die Unteransicht „Geräte“ MUSS auf
  jedem Gerät der Vault den öffentlichen Schlüssel der Vault-Identität im
  Nostr-Format zeigen, als den Namen der Vault in Mitgliederlisten und Rechten
  von Spaces und Datenfreigaben (Specs 027, 028). Er ist keine Adresse zum
  Einladen: Eingeladen wird nur über einen Einladungslink des Admins (Spec 027
  FR-008). Er MUSS sich kopieren, aber
  nicht ändern lassen (D26). Den privaten Schlüssel DARF die Oberfläche nie
  zeigen (FR-002).
- **FR-036**: Agenten mit Leserecht auf die Einstellungen DÜRFEN die Geräteliste
  samt Rollen und Online-Stand und den öffentlichen Schlüssel der
  Vault-Identität abrufen. Verknüpfen, die Wahl der Rolle, Aufnehmen, Ablehnen
  und Entfernen DÜRFEN nur von der Nutzerin selbst ausgelöst werden, nie von
  Agenten oder Erweiterungen; private Schlüssel, das Signieren von Gerätelisten
  und Verknüpfungscodes DÜRFEN Agenten und Erweiterungen nie erreichen.
- **FR-037**: Alle neuen Texte (Stände, Rollen, Meldungen, Erklärungen,
  Warnungen) MÜSSEN auf Deutsch und Englisch vorliegen (Spec 023 FR-020).

### Key Entities

- **Vault-Identität**: Schlüsselpaar der Vault; öffentlicher Teil (Adresse der
  Vault, auf jedem Gerät sichtbar), privater Teil (nur auf Hauptgeräten). Wechselt
  in v1 nie.
- **Geräteschlüssel**: Schlüsselpaar eines Geräts für eine Vault; bleibt auf
  diesem Gerät; weist das Gerät aus und signiert, was es über Dritte
  veröffentlicht; Empfänger von Umschlägen.
- **Geräteliste**: Vault, Generation, aktuelle Geräte (öffentlicher
  Geräteschlüssel, Rolle, Name beim Aufnehmen, Netzwerkkennung), entfernte
  Geräte mit Grenze; mit der Vault-Identität von einem Hauptgerät signiert.
- **Gerät der Vault** (erweitert aus Spec 023): Name, Rolle, dieses Gerät
  ja/nein, online jetzt, zuletzt online, Grund, falls kein Sync möglich ist.
- **Aufnahmeanfrage**: Name, öffentlicher Geräteschlüssel, Netzwerkkennung,
  Zeitpunkt; mit dem Geräteschlüssel der Kopie signiert; offen, aufgenommen oder
  abgelehnt.
- **Präsenzmeldung**: vom Gerät signiert, nur für die eigene Vault lesbar;
  Erreichbarkeit, Ablaufzeit.
- **Bereich**: Kennung und Art (in dieser Spec nur „Vault“).
- **Änderungspaket** (erst ab Spec 026, auf Wegen über Dritte): Bereich,
  Kennung des Inhaltsschlüssels, verschlüsselter Inhalt aus einer oder mehreren
  vollständigen Transaktionen mit Autor je Änderung.
- **Inhaltsschlüssel**: Kennung, Schlüsselgeneration, Bereich; je Gerät der
  Geräteliste ein Umschlag.
- **Umschlag**: Inhaltsschlüssel, verschlüsselt an einen Geräteschlüssel;
  gebunden an Bereich, Generation und Empfänger.
- **Änderung**: Bereich, Zeitstempel der hybriden logischen Uhr mit dem
  Ursprungsgerät, Vault des Autors, Inhalt; in gemeinsamen Bereichen zusätzlich
  Laufnummer und Signatur des Ursprungsgeräts.
- **Fortschrittsstand**: im Bereich „Vault“ je Ursprungsgerät der höchste
  Zeitstempel, bis zu dem alle Änderungen vorliegen; in gemeinsamen Bereichen
  je Ursprungsgerät die höchste lückenlose Laufnummer.
- **Mitgliederliste** (Grundlage für Specs 026–028): Bereich, Generation,
  Mitglieds-Vaults mit Rechten, bei einem Entzug die Grenze je Gerät der
  betroffenen Vault; von einem Gerät der Admin-Vault signiert.
- **Verknüpfungscode**: einmalig, kurzlebig; Erreichbarkeit des Hauptgeräts und
  ein Geheimnis für den gegenseitigen Nachweis.
- **Nur-direkt-Daten**: allein der private Schlüssel der Vault-Identität
  (FR-038).

## Success Criteria _(mandatory)_

### Measurable Outcomes

- **SC-001**: Sind zwei Geräte verbunden, erscheint eine Änderung in 95 % der
  Fälle innerhalb von 5 Sekunden auf dem anderen Gerät, im selben Netz wie über
  das Internet.
- **SC-002**: Zwei Geräte der Geräteliste, die beide online sind, sind nach dem
  Öffnen der Vault innerhalb von 30 Sekunden verbunden, ohne Zutun der
  Nutzerin. Das gilt auch für ein soeben verknüpftes oder aufgenommenes Gerät
  und für die frische Kopie der Datei eines Hauptgeräts und ihr Quellgerät, das
  bis dahin das einzige Gerät der Vault war.
- **SC-003**: In einer automatischen Prüfung mit drei Geräten und wechselnden,
  indirekten Wegen (mindestens 1.000 Änderungen) fehlt am Ende auf keinem Gerät
  eine Änderung, keine ist doppelt angewendet, alle Geräte haben denselben
  Stand, und 100 % der Änderungen nennen ihr wahres Ursprungsgerät.
- **SC-004**: Nach einer Trennung, in der ein Gerät N Änderungen erzeugt hat,
  überträgt das Wiederverbinden höchstens diese N Änderungen (weniger, wenn
  einige davon schon überschrieben sind) und keine, die das andere Gerät schon
  hat.
- **SC-005**: In 100 % der geprüften Fälle (fremde Vault, Geräteschlüssel nicht
  auf der aktuellen Geräteliste, nicht mit der Vault-Identität signierte
  Geräteliste, fehlender Besitznachweis, entferntes Gerät, Kopie vor der
  Aufnahme) wird die Verbindung abgewiesen, bevor eine einzige Änderung
  fließt.
- **SC-006**: Ein Mitschnitt des Verkehrs zwischen zwei Geräten enthält keinen
  Klartext von Vault-Inhalten, Tabellen- oder Spaltennamen; eine automatische
  Prüfung zeigt, dass gerätelokale Daten und private Geräteschlüssel in keiner
  Übertragung und private Schlüssel in keinem Protokoll vorkommen. Wohin der private
  Schlüssel der Vault-Identität darf, prüft SC-012.
- **SC-007**: Ein Gerät, dessen Geräteliste nur es selbst nennt, veröffentlicht
  während einer ganzen Vault-Session keine Präsenzmeldung und baut keine
  direkte Verbindung im Bereich „Vault“ auf, solange sich kein anderes Gerät
  derselben Vault meldet; sein einziger Präsenzverkehr ist das Lauschen auf
  Präsenzmeldungen an seine Vault. Uploads in das Postfach beim Sync-Server (Spec 026) und
  Verkehr mit Mitgliedern von Spaces und Datenfreigaben (Specs 027, 028) zählen
  nicht dazu.
- **SC-008**: Die Geräteliste zeigt einen Wechsel von online zu offline oder
  umgekehrt innerhalb von 60 Sekunden, eine Umbenennung, ein hinzugefügtes und
  ein entferntes Gerät innerhalb der Zeit aus SC-001, sobald die Änderung dieses
  Gerät erreicht hat.
- **SC-009**: Ein Verknüpfen dauert vom Anzeigen des Codes bis zum Beginn der
  Übertragung weniger als 2 Minuten; eine Nutzerin schafft es beim ersten
  Versuch ohne Anleitung. In 100 % der Fälle, in denen sie die Frage nach der
  Rolle nicht ändert, wird das neue Gerät ein verknüpftes Gerät ohne den
  privaten Schlüssel der Vault-Identität.
- **SC-010**: Nach dem Entfernen nimmt kein Gerät, das die neue Geräteliste
  kennt, eine einzige Verbindung oder Änderung jenseits der Grenze des
  entfernten Geräts mehr an; das entfernte Gerät kann in 100 % der geprüften
  Fälle nichts entschlüsseln, was mit der neuen Generation verschlüsselt ist; die
  verbleibenden Geräte synchronisieren weiter, und Vault-Identität, Admin-Rollen
  und Mitgliedschaften sind unverändert.
- **SC-011**: Die Szenarien der User Stories 1, 2, 3 und 5 laufen als
  automatische Tests mit mehreren App-Prozessen gegen die gebaute App (Spec 016)
  und bestehen.
- **SC-012**: Eine automatische Prüfung zeigt für den privaten Schlüssel der
  Vault-Identität (FR-038), dass er in 100 % der geprüften Fälle nur in der
  Übertragung eines Verknüpfens mit gewählter Hauptgerät-Rolle auf der direkten
  Verbindung und verschlüsselt im Wiederherstellungspaket (Spec 026) vorkommt:
  in keiner Übertragung des gewöhnlichen Sync, auch nicht auf direkten
  Verbindungen, auf keinem verknüpften Gerät, in keinem Paket und keiner
  Momentaufnahme für ein Postfach beim Sync-Server, in keinem Paket eines anderen
  Bereichs als „Vault“ und in keinem Protokoll. Die Prüfung schlägt fehl,
  sobald er auf einem dieser Wege auftaucht.
- **SC-013**: In 100 % der geprüften Fälle verwerfen alle Geräte (und ab Spec
  026 der Sync-Server) eine Geräteliste, die nicht mit der Vault-Identität signiert
  ist; ein verknüpftes Gerät kann keine gültige Geräteliste ausstellen.
  Veröffentlichen zwei Hauptgeräte verschiedene Gerätelisten derselben
  Generation, wählen alle Geräte dieselbe, unabhängig von der Reihenfolge des
  Eintreffens, ein Hauptgerät veröffentlicht danach eine zusammengeführte Liste
  der nächsten Generation, und die geltenden Entfernungen der maßgeblichen
  kleinsten Hash-Liste bleiben erhalten. Entfernen sich zwei Hauptgeräte
  gegenseitig, bleibt auf allen Geräten dasselbe der beiden Hauptgerät.
- **SC-014**: In einer automatischen Prüfung, die Änderungen über mehrere Wege
  zustellt und Übertragungen an zufälligen Stellen abbricht, steht der
  Fortschrittsstand eines Geräts in 100 % der Fälle nie über einer Änderung
  desselben Ursprungsgeräts, die ihm fehlt und noch gilt; am Ende haben alle
  Geräte denselben Stand. Keine Transaktionsgruppe wird zum Teil angewendet.
- **SC-015**: Nachdem eine Mitgliederliste einer Vault ein Recht entzogen hat,
  lehnt jedes Gerät, das diese Liste kennt, in 100 % der geprüften Fälle alle
  Änderungen dieser Vault jenseits der Grenze ab, die das entzogene Recht
  brauchen, auch rückdatierte. Änderungen, die ein Gerät angewendet hatte,
  bevor es die Liste kannte, dürfen bleiben, bis eine berechtigte Änderung
  derselben Zelle sie überschreibt (hingenommenes Risiko, D19). Prüfbar, sobald
  Spaces bestehen (Spec 027).
- **SC-016**: Veröffentlichen zwei Geräte der Admin-Vault verschiedene
  Mitgliederlisten derselben Generation, wählen der Sync-Server und alle Empfänger in
  100 % der geprüften Fälle dieselbe Liste, unabhängig von der Reihenfolge des
  Eintreffens, und ein Gerät der Admin-Vault veröffentlicht danach eine
  zusammengeführte Liste der nächsten Generation. Prüfbar, sobald Spaces
  bestehen (Spec 027).
- **SC-017**: Eine Momentaufnahme mit einer ungültigen Änderung verliert beim
  Empfänger in 100 % der geprüften Fälle genau die Transaktionsgruppe dieser
  Änderung; alle anderen Gruppen kommen an, keine Gruppe zum Teil. Prüfbar,
  sobald es Momentaufnahmen gibt (Spec 026).
- **SC-018**: Die Kopie der Datei eines verknüpften Geräts sendet und erhält in
  100 % der geprüften Fälle keine Änderung, bis ein Hauptgerät sie aufnimmt;
  danach erreichen alle ihre Änderungen, auch die aus der Wartezeit, die anderen
  Geräte. Die Kopie der Datei eines Hauptgeräts steht ohne weiteres Zutun als
  Hauptgerät auf der Geräteliste und ist in der Zeit aus SC-002 verbunden.
- **SC-019**: Ein Gerät, das verknüpft wird, nachdem die Vault Schlüssel eines
  Space erhalten hat, kann in 100 % der geprüften Fälle die bisherigen Inhalte
  dieses Space lesen, sobald es mit einem eigenen Gerät synchronisiert hat, auch
  wenn der Admin des Space nicht online ist (FR-039). Prüfbar, sobald Spaces
  bestehen (Spec 027).

## Assumptions

- Vom Betreiber vorgegeben: Transport über iroh (direkte QUIC-Verbindungen, mit
  einem iroh-Relay nur für den Verbindungsaufbau durch NAT), Schlüssel und
  Präsenz im Nostr-Format (secp256k1), Umschläge nach NIP-44. Die
  Präsenzmeldung ersetzt die übliche Adresssuche von iroh über pkarr/DNS
  (Entwurf §4). Wie der Sync-Kanal heißt (Arbeitsname `holzi-sync/1`) und wie
  Nachrichten, Gerätelisten und Meldungen kodiert sind, klärt der Plan (Entwurf §15
  Punkt 7).
- Die Netzwerkkennung eines Geräts ist sein iroh-Schlüssel; die Geräteliste
  bindet ihn an den Geräteschlüssel (Entwurf §4).
- Zusammenführen, Löschvermerke und die Ausnahme gerätelokaler Daten liefert
  haex-crdt, wie holzi es einbindet: Repository
  `https://github.com/haexmas/haex-crdt`, heute Revision
  `ed230d2c3f58c1b10710b6025ea0ce6c20b8d009`; der Plan nennt eine kleine
  Erweiterung für Schreibvorgänge und die Revision, die danach gilt. Das Ursprungsgerät steht dort in
  jedem Zeitstempel und lässt sich nach Ursprungsgerät abfragen; das Feld für
  das Gerät im Änderungsdatensatz nennt dagegen das sendende Gerät (Entwurf
  §3.2) und wird für den Autor nicht verwendet. Felder werden
  einzeln zusammengeführt, die jüngere Änderung gilt; Konflikte bei Dateien
  (Konfliktkopien, D10) gehören zu Spec 025.
- Innerhalb der eigenen Vault gibt es keine Rechte an Daten: Jedes Gerät auf
  der Geräteliste darf alle Daten lesen und schreiben (Entwurf §6). Einzige
  Unterscheidung ist die Rolle: Nur Hauptgeräte verwalten Geräte. Lesen,
  Schreiben, Löschen und Admin gibt es erst für Spaces und Datenfreigaben
  (027, 028).
- Die Passphrase gehört zur Datei auf einem Gerät. Eine kopierte Vault-Datei
  öffnet sich mit der bisherigen Passphrase; beim Verknüpfen vergibt die
  Nutzerin auf dem neuen Gerät eine Passphrase, die gleich oder anders sein
  darf. Ein Wechsel der Passphrase ist nicht Teil dieser Spec.
- Das Verknüpfen überträgt den aktuellen Stand der Vault samt Löschvermerken,
  nicht jede frühere Fassung; danach läuft der gewöhnliche Sync.
- Ein Hauptgerät in falschen Händen, dessen Passphrase bekannt ist, bedeutet
  den Totalausfall der Vault: Es kann Geräte aufnehmen und entfernen. Das ist
  hingenommen; holzi warnt beim Verknüpfen und macht die Rolle Hauptgerät nicht
  zum Standard (D27).
- Eine neue Geräteliste wirkt erst auf Geräten, die sie kennen. Bis sie ein
  Gerät erreicht, kann das entfernte Gerät mit ihm noch synchronisieren; dieses
  Fenster ist hingenommen, wie das Fenster aus D19 bei Mitgliederlisten.
- Weil Umschläge an jedes Gerät einer Mitglieds-Vault gehen (FR-039), erfährt
  der Admin eines Space oder einer Datenfreigabe, wie viele Geräte eine
  Mitglieds-Vault hat (D28, hingenommen).
- Mobile Geräte sind nur im Vordergrund erreichbar (v1-scope §7); das ist für
  diese Spec kein Fehlerfall, sondern ein Gerät, das gerade offline ist.
  holzi läuft heute nur auf dem Desktop.
- Die gerätelokalen Daten eines Quellgeräts liegen nach ADR-0001 physisch in
  einer kopierten Datei, auch sein Geräteschlüssel. FR-006 verlangt, dass das
  Zielgerät sie nicht als eigene Identität verwendet und nichts davon löscht.
  Wer die Datei und ihre Passphrase hat, hat ohnehin alle Daten der Vault und,
  bei einem Hauptgerät, den privaten Schlüssel der Vault-Identität.

## Nicht im Umfang

- Sync über den Sync-Server und Postfächer, auch für die eigene Vault; kommt mit
  Spec 026. Diese Spec legt nur fest, welche Geräte angenommen werden (FR-005,
  FR-009) und was nie als gewöhnliche Vault-Information in ein Postfach darf
  (FR-038). Bis dahin gleichen sich nur Geräte an, zwischen denen irgendwann
  ein Weg über gleichzeitig laufende Geräte besteht.
- Die Wiederherstellung, wenn alle Geräte verloren sind (Wiederherstellungspaket,
  Wiederherstellungsschlüssel, zweiter Faktor); das regelt Spec 026 (D31).
- Dateisync zwischen eigenen Geräten (Spec 025), Spaces (027),
  Datenfreigaben (028), eigener S3-Speicher (029).
- Weitere Rechte zwischen eigenen Geräten als die zwei Rollen; die Fähigkeit
  `confirmation-authority` aus v1-scope §4.
- Die Rolle eines vorhandenen Geräts ändern (ein verknüpftes Gerät zum
  Hauptgerät machen oder umgekehrt); bis dahin entfernt die Nutzerin das Gerät
  und verknüpft es neu.
- Ein Wechsel der Vault-Identität (D26).
- Ein Schalter, der den Sync auf einem Gerät pausiert, und die Wahl einzelner
  Daten, die nicht synchronisiert werden sollen.
- Andere Geräte als dieses umbenennen (Spec 023); ein Gerät, das die Nutzerin
  nicht mehr braucht, entfernt sie oder löscht die Vault dort selbst.
- Das Entfernen rückgängig machen; ein entferntes Gerät kommt nur durch ein
  neues Verknüpfen zurück.
- Ein Löschen der Daten auf einem entfernten Gerät aus der Ferne.
- Die Admin-Rolle übertragen, weder auf Wunsch noch bei Verlust (FR-041). Hat
  die Admin-Vault kein Gerät mehr, bleibt der Bereich eingefroren.
- Verstecken, welche Geräte zu einer Vault gehören, vor den genutzten Servern
  (Pseudonyme, D9) und Schutz vor Verkehrsanalyse.
- Eine Übersicht des Sync-Fortschritts je Datenart oder ein Protokoll
  einzelner übertragener Änderungen.
