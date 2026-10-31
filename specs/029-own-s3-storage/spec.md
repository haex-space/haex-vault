# Feature Specification: Eigener S3-Speicher für Spaces

**Feature Branch**: `029-own-s3-storage`
**Created**: 2026-09-28
**Status**: Draft
**Input**: Zeile 029 des Spec-Schnitts im Sync-Entwurf
[`docs/plans/2026-09-28-sync-architecture.md`](../../docs/plans/2026-09-28-sync-architecture.md)
(§12 Speicher-Backend B, §13 Bedrohungsmodell, Entscheidungen D11 und D12):
Der Admin eines Space verbindet seinen eigenen S3-kompatiblen Speicher. Je
Space gibt es einen Bucket und zwei eingeschränkte Zugangsschlüssel (nur Lesen,
Lesen und Schreiben), die der Admin in seiner Vault erzeugt, in seinem
Passwortmanager ([Spec 034](../034-password-manager/spec.md)) ablegt und verschlüsselt an die Mitglieder
gibt, je nach Fähigkeit. Das Relay bekommt nie Zugangsdaten in lesbarer Form und
ist an Dateien nicht beteiligt. Die Geräte greifen direkt auf den Speicher zu. Beim Entzug von Rechten werden die
Zugangsschlüssel erneuert. Weil S3 „nur eigene Dateien löschen“ nicht
durchsetzen kann, MUSS die Versionierung des Buckets eingeschaltet sein. Ein
Anbieter, der diese Anforderungen nicht erfüllt (etwa keine eingeschränkten
Zugangsschlüssel erzeugen kann), lässt sich für einen Space nicht verbinden
(D25).

## Begriffe

- **Space**, **Admin**, **Fähigkeiten Lesen/Schreiben/Löschen**: wie in Spec 027.
  Ein Space ist ein Netzwerkordner für Dateien; Admin ist nur, wer ihn angelegt
  hat.
- **Dateiindex**, **Objekt**, **Konfliktkopie**: wie in Spec 025. Ein Objekt ist
  der verschlüsselte, unveränderliche Inhalt einer Datei, benannt nach dem Hash
  seines Chiffretexts. Wer eine Datei ändert, schreibt ein neues Objekt; alte
  Objekte werden aufgeräumt.
- **Relay**: synchronisiert nur SQLite-Daten (Postfächer); Dateien kommen nie in
  ein Postfach (Spec 026, D32).
- **Speicher-Backend A**: S3-kompatibler Speicher, den der Betreiber eines Relays
  optional zusätzlich bereitstellt; der Relay-Dienst prüft dann den Zugriff und
  reicht jedes verschlüsselte Objekt aus diesem Speicher durch (Spec 026, D24,
  D32).
- **Speicher-Backend B**: der eigene S3-kompatible Speicher eines Nutzers; Thema
  dieser Spec. Das Relay ist an Dateien hier nicht beteiligt (D32).
- **Passwortmanager**: fester Bestandteil von holzi, der Geheimnisse wie
  S3-Zugangsdaten und Zugangsschlüssel in der Vault verwahrt; Erweiterungen mit
  Berechtigung dürfen ihn nutzen. Er wird in der Spec 034 festgelegt;
  diese Spec legt nur fest, welche Geheimnisse dort liegen (D32).
- **Anbieter**: der Dienst, bei dem der eigene Speicher liegt (zum Beispiel ein
  Cloud-Anbieter oder ein selbst betriebener RustFS-Server).
- **Speicherverbindung**: Endpunkt, Region, Anbieter und die
  **Hauptzugangsdaten** des Admins bei diesem Anbieter. Die Hauptzugangsdaten
  dürfen Buckets anlegen und Zugangsschlüssel erzeugen und widerrufen. Sie liegen
  im Passwortmanager der Vault des Admins und damit auf allen seinen Geräten
  (FR-033).
- **Space-Bucket**: der eine Bucket, der zu genau einem Space gehört und nur
  dessen Objekte enthält.
- **Zugangsschlüssel**: ein vom Anbieter ausgestellter Schlüssel, der nur für
  einen Space-Bucket gilt, in zwei Arten: **nur Lesen** und **Lesen und
  Schreiben**. Jede Erneuerung erzeugt eine neue **Generation** des
  Zugangsschlüssels; ältere Generationen werden widerrufen.
- **Eignungsprüfung**: die Prüfung, ob ein Anbieter mit den eingegebenen
  Hauptzugangsdaten alles kann, was diese Spec verlangt. Sie endet mit
  **geeignet** oder **ungeeignet**; eine Zwischenstufe gibt es nicht (D25).
- **Versionierung**: die Fähigkeit eines Buckets, gelöschte oder überschriebene
  Objekte als ältere Versionen aufzubewahren. Die **Aufbewahrungsfrist** legt
  fest, wie lange ältere Versionen bleiben.
- **Postfach**, **Mitgliederliste**: wie in Spec 026. Das Relay gilt als nicht
  vertrauenswürdig.
- **Vault-Identität**, **Geräteschlüssel**, **Geräteliste**, **Hauptgerät**,
  **verknüpftes Gerät**: wie in Spec 024. Grants und Mitgliederlisten nennen
  Vaults (Vault-Identität); Schlüsselumschläge gehen an jedes Gerät auf der
  aktuellen Geräteliste einer Mitglieds-Vault (D28).
- **Inhaltsschlüssel**, **Schlüsselgeneration**: wie in Spec 024 und 027. Ein
  Zugangsschlüssel ist kein Inhaltsschlüssel: Er öffnet den Speicher, nicht die
  Dateien.

## Beziehung zu bestehenden Specs

- Sync-Entwurf
  [`docs/plans/2026-09-28-sync-architecture.md`](../../docs/plans/2026-09-28-sync-architecture.md):
  Diese Spec setzt §12 B um. Sie ersetzt die Speicher-Zugriffsschicht mit
  IAM-Beiwerk des zurückgestellten Entwurfs
  [`2026-09-07-cross-user-sharing-deferred-design.md`](../../docs/plans/2026-09-07-cross-user-sharing-deferred-design.md)
  (§5 dort), wie der Sync-Entwurf es festhält.
- [`024-own-device-sync`](../024-own-device-sync/spec.md) (Vault-Identität,
  Geräte, Geräteliste, Sync zwischen eigenen Geräten): Die Hauptzugangsdaten des
  Admins und die Zugangsschlüssel liegen im Passwortmanager und sind gewöhnliche
  Vault-Daten im Bereich „Vault“ (D30): Sie gelangen mit dem Datensync von Spec
  024 auf alle eigenen Geräte, direkt oder verschlüsselt über das eigene Postfach
  der Vault, und kommen mit der Vault zurück, wenn sie wiederhergestellt wird
  (Spec 026). In einen anderen Bereich gelangen sie nie. Die Umschläge für
  Zugangsschlüssel gehen, wie jeder andere Schlüsselumschlag, an jedes Gerät auf
  der aktuellen Geräteliste der Mitglieds-Vault (D28); jedes Gerät öffnet seinen
  Umschlag mit seinem Geräteschlüssel. Verwalten darf den Speicher jedes Gerät
  auf der Geräteliste der Admin-Vault, Hauptgerät oder verknüpft (D29).
- [`038-storage-connections`](../038-storage-connections/spec.md)
  (Speicherverbindungen): legt die Speicherverbindung fest (Anbieter, Endpunkt,
  Region, Adressierung, Zugangsdaten im Passwortmanager als Eintrag mit Eigentümer
  `storage`, Regel Z14 in 034) und gibt Speicher an Erweiterungen weiter. Diese
  Spec nutzt sie und ergänzt Eignungsprüfung, Space-Buckets und Zugangsschlüssel.
- Spec 034 (Passwortmanager): verwahrt die Hauptzugangsdaten und die
  Zugangsschlüssel dieser Spec (D32). Wie der Passwortmanager aussieht und wie
  Erweiterungen ihn nutzen, legt Spec 034 fest, nicht diese.
- `025-own-device-file-sync` (entfernt, ersetzt durch [`061-sync-rules`](../061-sync-rules/spec.md)) (Dateisync
  zwischen eigenen Geräten): Die eigenen synchronisierten Ordner einer Vault
  können statt direkter Übertragung und Backend A ebenfalls Backend B nutzen
  (User Story 6). Dateiindex und Objekte bleiben unverändert; diese Spec ändert
  nur, wo die Objekte liegen.
- [`026-blind-relay`](../026-blind-relay/spec.md) (Relay und Backend A): Der
  Relay-Dienst synchronisiert nur SQLite-Daten (Postfächer); Dateien kommen nie
  in ein Postfach. Backend A ist Speicher, den der Betreiber eines Relays
  optional zusätzlich stellt; dann prüft der Relay-Dienst den Zugriff und reicht
  die Objekte daraus durch. Bei Backend B ist das Relay an Dateien nicht
  beteiligt (D32). Das Postfach des Space
  (Dateiindex, Mitgliederliste, Schlüsselumschläge) liegt unabhängig davon auf
  dem Relay des Admins, wenn eines eingerichtet ist (Spec 026 und 027); ohne
  Relay synchronisiert der Space das Postfach nur direkt zwischen den Geräten der
  Mitglieder (Spec 027). Backend B ändert nur, wo die Objekte liegen.
- [`027-spaces`](../027-spaces/spec.md) (Spaces): Spec 027 legt Mitglieder,
  Fähigkeiten, Einladungen, Entzug und das Erneuern des Inhaltsschlüssels fest.
  Diese Spec hängt sich an diese Ereignisse: Jede Einladung, jede
  Rechteänderung und jedes Entfernen verteilt oder erneuert zusätzlich die
  Zugangsschlüssel (FR-015 bis FR-021). Für Backend B erfüllt das Erneuern der
  Zugangsschlüssel die Forderung von Spec 027, dass das Speicher-Backend
  entfernte Mitglieder abweist (FR-023 dort), mit der Frist aus SC-002; dafür
  widerruft ein Gerät des Admins den alten Zugangsschlüssel beim Anbieter, bevor
  es die neue Mitgliederliste veröffentlicht (FR-018). Die
  Löschregeln (Schreiben löscht nur eigene Dateien, Löschen löscht alle) und
  deren Prüfung beim Empfänger bleiben in Spec 027. Spec 027 verlangt beim
  Anlegen eine Wahl zwischen Backend A und „nur direkte Übertragung“ (FR-003
  dort); die Wahl ist Pflicht, einen Standard gibt es nicht. Diese Spec fügt
  Backend B als dritte Wahl hinzu und hebt den Ausschluss „Speicher-Backend eines
  bestehenden Space wechseln“ aus Spec 027 auf (User Story 8). Einen Space
  aufzulösen bleibt auch hier ausgeschlossen. Die direkte Verbindung zwischen
  Mitgliedern aus Spec 027 nutzt diese Spec nicht: Kein Gerät vermittelt
  Speicherzugriffe für ein anderes (FR-023).
- [`028-data-shares`](../028-data-shares/spec.md) (Datenfreigaben): nicht
  betroffen. Datenfreigaben haben keine Objekte und nutzen keinen Speicher.
- [`023-settings-app`](../023-settings-app/spec.md): Die Einstellungen dieser
  Spec liegen in der Kategorie „Föderation“. Den Speicher eines Space legt der
  Admin in der Detailansicht des Space in der Unteransicht „Spaces“ fest (Spec
  027), den Speicher der eigenen Ordner in der Unteransicht „Ordner“ (Spec 025).
  Die Einstellung gilt nach der Auswahl ohne Knopf zum Übernehmen (FR-021 dort).
  Wo die gespeicherten Speicherverbindungen verwaltet werden, klärt der Plan.
- Referenz (gelesen, nicht portiert): der Dateisync von haex-vault 2.x (Legacy), Repository
  `https://github.com/haex-space/haex-vault-legacy`, Revision
  `8dce379d94e18fcd42c3b73686a06f984ca3f574`, Pfad `src-tauri/src/file_sync/`.
  Dort hält der Server volle Zugangsdaten eines Buckets je Nutzer; diese Spec
  verlangt das Gegenteil.

## Clarifications

### Session 2026-09-28

- Q: Darf das Relay die Zugangsdaten eines eigenen S3-Speichers halten? → A:
  Nein (D11). Das Relay ist nicht vertrauenswürdig. Es bekommt weder die
  Hauptzugangsdaten noch einen Zugangsschlüssel in lesbarer Form. (Präzisiert
  durch D30: Verschlüsselt reisen beide als gewöhnliche Vault-Daten auch über
  das eigene Postfach der Vault.)
- Q: Kommen beide Speicher-Backends in v1? → A: Ja (D12): Speicher des Relays (A)
  und eigener S3-Speicher (B).
- Q: (Betreiber) „Relay braucht dann vollen S3-Zugriff?“ → A: Nein. Bei eigenem
  S3 gibt der Admin den Mitgliedern auf den Space-Bucket beschränkte
  Zugangsschlüssel, verschlüsselt an ihre Vaults. Die Geräte greifen damit direkt
  auf den Speicher zu, das Relay ist für Dateien nicht beteiligt. Nur bei
  Backend A ist das Relay selbst der Speicheranbieter; dort liegen ohnehin nur
  Chiffretexte.
- Q: Wie viele Buckets und Schlüssel je Space? → A: Ein Bucket je Space und zwei
  Zugangsschlüssel, nur Lesen und Lesen und Schreiben (Entwurf §12 B).
- Q: Wie schützt der Space sich vor einem Mitglied mit Schreibrecht, das fremde
  Objekte direkt im Speicher löscht? → A: Gar nicht vorab, denn S3 kennt kein
  „nur eigene Objekte löschen“. Deshalb MUSS die Einrichtung die Versionierung
  einschalten; gelöschte Objekte werden aus älteren Versionen zurückgeholt
  (Entwurf §12 B).
- Q: Gehört der Wechsel eines Space zwischen Backend A und B in diese Spec? → A:
  Ja, als P3 (User Story 8). Begründung: Objekte sind in beiden Backends
  identisch, also ist ein Wechsel ein Kopieren und Umschalten, ohne neue
  Verschlüsselung. Ohne Wechsel bliebe die Wahl beim Anlegen endgültig, denn
  Spec 027 kennt weder einen Wechsel noch das Auflösen eines Space. Weil der
  Wechsel nicht nötig ist, um Backend B überhaupt zu nutzen, steht er hinten.
- Q: Wer darf den Speicher eines Space verbinden oder wechseln? → A: Nur der
  Admin (D6). Mitglieder wählen keinen Speicher; sie nutzen, was der Admin
  festgelegt hat.
- Q: Welche Anbieter muss v1 unterstützen? → A: RustFS und AWS S3, geprüft und getestet. MinIO nicht, weil es nicht mehr als Open Source weiterentwickelt wird; R2, B2 und Hetzner folgen nach Prüfung (FR-008).
- Q: Was passiert mit Anbietern, die unsere Anforderungen nicht erfüllen? → A: Sie lassen sich nicht verbinden; einen Ersatzweg über kurzlebige Links gibt es nicht (D25).
- Q: Gibt es in v1 ein Rotieren der Vault-Identität? → A: Nein (D26). Ein verlorenes Gerät ist kein Problem, solange eine Kopie oder das Relay existiert und die Passphrase hält; ausgesperrt wird ein Gerät über die Geräteliste (D27).
- Q: An wen werden Schlüssel von Spaces und Datenfreigaben verschlüsselt? → A: An jedes Gerät der Mitglieds-Vaults laut deren aktueller Geräteliste (D28). Das gilt auch für die Umschläge der Zugangsschlüssel.
- Q: Wo liegen die Hauptzugangsdaten des Admins und die Zugangsschlüssel, und reisen sie nur direkt? → A: Im Passwortmanager der Vault (Spec 034, D32). Sie sind gewöhnliche Vault-Daten: Sie synchronisieren auf alle eigenen Geräte, auch über das eigene Postfach der Vault, und lassen sich mit der Vault wiederherstellen. Nur direkt reist allein der private Schlüssel der Vault-Identität (D30).
- Q: Was macht das Relay bei Dateien? → A: Der Relay-Dienst synchronisiert nur SQLite-Daten (Postfächer), nie Dateien. Stellt der Betreiber zusätzlich S3-Speicher bereit (Backend A), prüft der Relay-Dienst den Zugriff und reicht die Objekte daraus durch; bei eigenem S3 (Backend B) ist das Relay an Dateien nicht beteiligt (D32).

## User Scenarios & Testing _(mandatory)_

### User Story 1 - Eigenen Speicher für einen Space verbinden (Priority: P1)

Eine Nutzerin legt einen Space für ihre Familie an und möchte die Fotos nicht
auf dem Speicher des Relays ablegen, sondern bei ihrem eigenen Anbieter. Sie gibt
Endpunkt, Region und ihre Hauptzugangsdaten ein. holzi prüft den Anbieter, legt
einen Bucket für den Space an, schaltet die Versionierung ein und erzeugt die
beiden Zugangsschlüssel. Ab dann liegen die Objekte des Space in ihrem Bucket.

**Why this priority**: Das ist der Kern der Spec. Ohne Verbindung gibt es kein
Backend B.

**Independent Test**: Gegen einen S3-kompatiblen Testanbieter mit
Versionierung und eingeschränkten Schlüsseln einen Space anlegen und den
Speicher verbinden: Es gibt genau einen neuen Bucket mit eingeschalteter
Versionierung und zwei Zugangsschlüssel, die nur für diesen Bucket gelten. Eine
Datei im Space landet als Objekt in diesem Bucket, nicht auf dem Relay.

**Acceptance Scenarios**:

1. **Given** die Nutzerin ist Admin eines Space, **When** sie in der
   Detailansicht des Space „Eigenen Speicher verbinden“ wählt und Endpunkt, Region und Hauptzugangsdaten eingibt, **Then**
   führt holzi die Eignungsprüfung durch, bevor irgendetwas angelegt oder
   gespeichert wird.
2. **Given** die Eignungsprüfung ist bestanden, **When** die Nutzerin bestätigt,
   **Then** legt holzi einen Bucket an, dessen Name den Namen des Space nicht
   verrät, schaltet die Versionierung und die Aufbewahrungsfrist ein, erzeugt
   beide Zugangsschlüssel und zeigt den Space als „Speicher: eigener S3“.
3. **Given** die Hauptzugangsdaten sind falsch oder der Endpunkt ist nicht
   erreichbar, **When** die Prüfung läuft, **Then** nennt holzi den Grund
   verständlich, speichert keine Zugangsdaten und legt nichts an.
4. **Given** die Nutzerin hat schon eine Speicherverbindung gespeichert, **When**
   sie einen weiteren Space anlegt, **Then** kann sie diese Verbindung wählen,
   ohne die Zugangsdaten erneut einzugeben, und der neue Space bekommt einen
   eigenen Bucket.
5. **Given** ein Mitglied ohne Admin-Rolle öffnet den Space, **When** es die
   Speicherangaben ansieht, **Then** sieht es, dass der Space eigenen S3-Speicher
   nutzt, aber keine Möglichkeit, ihn zu ändern, und keine Hauptzugangsdaten.

---

### User Story 2 - Mitglieder greifen direkt auf den Speicher zu (Priority: P1)

Ein Familienmitglied mit Fähigkeit Lesen öffnet den Space und lädt Fotos
herunter; seine Schwester mit Schreibrecht legt neue Fotos hinein. Beide greifen
direkt auf den Bucket zu, mit dem Zugangsschlüssel, den der Admin ihnen passend
zu ihrer Fähigkeit gegeben hat. Das Relay sieht davon nichts, und der Anbieter
sieht nur unlesbare Objekte.

**Why this priority**: Ein verbundener Speicher, den die Mitglieder nicht
erreichen, bringt nichts. Und hier muss das Versprechen „Relay und Anbieter sehen
keine Inhalte“ halten.

**Independent Test**: Ein Mitglied mit Lesen und eines mit Schreiben einladen.
Das Lesen-Mitglied lädt Dateien herunter, sein Versuch hochzuladen oder zu
löschen scheitert am Speicher. Das Schreiben-Mitglied lädt eine Datei hoch. Die
Protokolle und der Speicher des Relays enthalten keinen Zugangsschlüssel und keine
Übertragung dieser Objekte. Eine Durchsicht des Buckets findet keinen Dateinamen
und keinen Klartext.

**Acceptance Scenarios**:

1. **Given** der Admin lädt ein Mitglied mit Lesen ein, **When** die Einladung
   angenommen ist, **Then** erhält jedes Gerät auf der aktuellen Geräteliste der
   Mitglieds-Vault einen Umschlag mit dem Zugangsschlüssel „nur Lesen“ und
   keinen anderen (D28).
2. **Given** der Admin lädt ein Mitglied mit Schreiben oder Löschen ein, **When**
   die Einladung angenommen ist, **Then** erhält jedes Gerät der Mitglieds-Vault
   einen Umschlag mit dem Zugangsschlüssel „Lesen und Schreiben“.
3. **Given** ein Mitglied hat einen Zugangsschlüssel, **When** eines seiner
   Geräte eine Datei des Space lädt oder hochlädt, **Then** geht die Übertragung
   direkt zwischen Gerät und Anbieter, nicht über das Relay.
4. **Given** ein Mitglied mit „nur Lesen“, **When** es versucht, ein Objekt
   hochzuladen oder zu löschen, **Then** lehnt der Anbieter ab.
5. **Given** ein Mitglied verknüpft ein weiteres Gerät, nachdem es den
   Zugangsschlüssel erhalten hat, **When** das neue Gerät auf den Space
   zugreift, **Then** nutzt es den Zugangsschlüssel aus dem Passwortmanager der
   eigenen Vault, den der Sync zwischen eigenen Geräten (Spec 024) mitgebracht
   hat, ohne dass der Admin etwas tun muss; spätere Generationen bekommt es als
   eigenen Umschlag (D28).
6. **Given** der Bucket enthält Objekte des Space, **When** jemand mit vollem
   Zugriff den Bucket durchsieht, **Then** findet er nur Objekte mit Namen aus
   Hashes, ohne Dateinamen, Pfade, Ordnerstruktur oder lesbaren Inhalt.

---

### User Story 3 - Entzug erneuert die Zugangsschlüssel (Priority: P1)

Der Admin entfernt ein Mitglied aus dem Space. holzi erneuert sofort den
Zugangsschlüssel, den das Mitglied hatte, und widerruft den alten beim Anbieter;
erst danach veröffentlicht es die neue Mitgliederliste und gibt den neuen
Schlüssel allen verbleibenden Mitgliedern mit derselben Fähigkeit. Der alte
Schlüssel in der Vault des entfernten Mitglieds öffnet den Bucket nicht mehr.

**Why this priority**: Ein entferntes Mitglied darf nicht weiter Objekte laden
oder löschen können. Ohne Erneuerung wäre Entzug bei Backend B wirkungslos.

**Independent Test**: Zwei Mitglieder mit Lesen einladen, eines entfernen: Der
alte Zugangsschlüssel „nur Lesen“ wird vom Anbieter abgelehnt, das verbleibende
Mitglied liest mit dem neuen weiter. Ein Mitglied von Schreiben auf Lesen
zurückstufen: Es kann nicht mehr hochladen, aber weiter lesen.

**Acceptance Scenarios**:

1. **Given** der Admin entfernt ein Mitglied, **When** er das Entfernen
   bestätigt, **Then** erzeugt holzi in einem Vorgang eine neue Generation des
   Zugangsschlüssels, den das Mitglied hatte, widerruft die alte Generation beim
   Anbieter, veröffentlicht danach die neue Mitgliederliste und gibt die neue
   Generation allen verbleibenden Mitgliedern mit dieser Art (FR-018).
2. **Given** dieselbe Lage, **When** das entfernte Mitglied danach mit dem alten
   Schlüssel auf den Bucket zugreift, **Then** lehnt der Anbieter ab (SC-002).
3. **Given** der Admin stuft ein Mitglied von Schreiben oder Löschen auf Lesen
   zurück, **When** die Änderung wirksam ist, **Then** wird der Zugangsschlüssel
   „Lesen und Schreiben“ in derselben Reihenfolge wie beim Entfernen erneuert
   (erst beim Anbieter widerrufen, dann die Mitgliederliste veröffentlichen) und
   nur an die verbleibenden Schreibenden verteilt, und das zurückgestufte
   Mitglied erhält den aktuellen Zugangsschlüssel „nur Lesen“.
4. **Given** der Admin stuft ein Mitglied von Lesen auf Schreiben hoch, **When**
   die Änderung wirksam ist, **Then** erhält das Mitglied den aktuellen
   Zugangsschlüssel „Lesen und Schreiben“, ohne dass ein Zugangsschlüssel
   erneuert wird.
5. **Given** ein Mitglied lädt gerade ein Objekt, **When** es entfernt wird,
   **Then** darf die laufende Übertragung scheitern; was es schon geladen hat,
   behält es (Spec 027).
6. **Given** die Erneuerung ist abgeschlossen, **When** die verbleibenden
   Mitglieder auf den Bucket zugreifen, **Then** nutzen sie ohne eigenes Zutun die
   neue Generation.
7. **Given** der Anbieter ist beim Entfernen nicht erreichbar oder lehnt den
   Widerruf ab, **When** das Gerät des Admins den Vorgang abschließt, **Then**
   veröffentlicht es die neue Mitgliederliste trotzdem (das Relay weist das
   entfernte Mitglied damit schon ab), zeigt dem Admin eine bleibende Warnung,
   dass der alte Zugangsschlüssel noch gilt, und versucht den Widerruf erneut, bis
   der Anbieter ihn bestätigt (FR-022).

---

### User Story 4 - Gelöschte Objekte zurückholen (Priority: P2)

Ein Mitglied mit Schreibrecht löscht mit seinem Zugangsschlüssel direkt im
Bucket Objekte, die andere hochgeladen haben, am Dateiindex vorbei. Die
Einträge im Dateiindex bestehen weiter. holzi bemerkt auf einem Gerät des Admins,
dass Objekte fehlen, und holt sie aus den älteren Versionen des Buckets zurück.

**Why this priority**: Das ist die Absicherung gegen die Grenze von S3. Sie ist
nötig, damit „Schreiben löscht nur eigene Dateien“ bei Backend B mehr als ein
Versprechen ist, aber erst relevant, wenn ein Mitglied sich falsch verhält.

**Independent Test**: Mit dem Zugangsschlüssel „Lesen und Schreiben“ ein Objekt
direkt im Bucket löschen, das ein anderes Mitglied hochgeladen hat: Ein Gerät des
Admins meldet das fehlende Objekt und stellt es wieder her; die Datei ist danach
für alle Mitglieder wieder lesbar.

**Acceptance Scenarios**:

1. **Given** ein gültiger Eintrag im Dateiindex verweist auf ein Objekt, das im
   Bucket fehlt, **When** ein Gerät des Admins den Bucket prüft, **Then** stellt
   es das Objekt aus der jüngsten älteren Version wieder her und zeigt dem Admin
   einen Hinweis, welche Dateien betroffen waren.
2. **Given** ein Objekt wurde mit anderem Inhalt überschrieben, **When** ein
   Gerät es lädt, **Then** erkennt es an dem nicht passenden Hash, dass es nicht
   stimmt, verwendet es nicht, und ein Gerät des Admins stellt die passende ältere
   Version wieder her.
3. **Given** ein Mitglied hat eine Datei rechtmäßig gelöscht (Spec 027), **When**
   das zugehörige Objekt aufgeräumt wird, **Then** stellt holzi es nicht wieder
   her.
4. **Given** die Aufbewahrungsfrist eines gelöschten Objekts ist abgelaufen,
   **When** der Admin die betroffene Datei wiederherstellen will, **Then** meldet
   holzi, dass keine ältere Version mehr vorhanden ist, und nennt die Datei als
   verloren, sofern kein Gerät sie noch hat.
5. **Given** der Admin öffnet die Speicherangaben des Space, **When** er die
   Aufbewahrungsfrist ändert, **Then** gilt die neue Frist für den Bucket, und
   holzi zeigt, dass Löschungen nur innerhalb dieser Frist rückgängig zu machen
   sind.

---

### User Story 5 - Ungeeigneter Anbieter wird abgelehnt (Priority: P2)

Ein Nutzer betreibt einen S3-kompatiblen Speicher, der Versionierung kann, aber
keine auf einen Bucket beschränkten Schlüssel ausstellt. Er will ihn für einen
Space verbinden. holzi prüft den Anbieter, nennt ihm das fehlende Kriterium und
lehnt die Verbindung ab, ohne etwas anzulegen oder zu speichern. Für den Space
wählt er einen anderen Anbieter oder den Speicher des Relays; für seine eigenen
Ordner kann er den Anbieter weiter nutzen (User Story 6).

**Why this priority**: Ein Anbieter, der Entzug oder Wiederherstellung nicht
durchsetzen kann, würde die Versprechen aus User Story 2 bis 4 stillschweigend
brechen. Die klare Ablehnung schützt davor, ist aber erst relevant, wenn jemand
einen solchen Anbieter wählt (D25).

**Independent Test**: Gegen Testanbieter prüfen, denen je genau ein Kriterium aus
FR-004 fehlt: Jede Prüfung endet mit „ungeeignet“ und nennt das fehlende
Kriterium; beim Anbieter bleiben weder Bucket noch Zugangsschlüssel zurück, und
der Space nutzt weiter seinen bisherigen Speicher.

**Acceptance Scenarios**:

1. **Given** die Eignungsprüfung stellt fest, dass der Anbieter keine
   eingeschränkten Zugangsschlüssel erzeugen kann (FR-004 e), **When** sie das
   Ergebnis zeigt, **Then** lautet es „ungeeignet“, holzi nennt genau dieses
   Kriterium verständlich und bietet keinen anderen Zugriffsweg an.
2. **Given** der Schlüssel „Lesen und Schreiben“ könnte ältere Versionen löschen
   oder die Versionierung ändern (FR-004 f), **When** die Prüfung das feststellt,
   **Then** ist der Anbieter ungeeignet, und holzi erklärt, dass gelöschte
   Objekte sonst nicht sicher zurückzuholen wären.
3. **Given** der Anbieter weist einen widerrufenen Testschlüssel nicht innerhalb
   von 5 Minuten ab (FR-004 g), **When** die Prüfung endet, **Then** ist er
   ungeeignet, und holzi nennt die Frist, die er überschritten hat.
4. **Given** mehrere Kriterien fehlen, **When** holzi das Ergebnis zeigt,
   **Then** nennt es jedes fehlende Kriterium einzeln (FR-005).
5. **Given** die Prüfung endet mit „ungeeignet“, **When** der Admin das Ergebnis
   schließt, **Then** hat holzi keine Zugangsdaten gespeichert und beim Anbieter
   nichts angelegt oder entfernt alles, was die Prüfung angelegt hat (FR-007), und
   der Space behält seinen bisherigen Speicher.

---

### User Story 6 - Eigene synchronisierte Ordner auf eigenem Speicher (Priority: P2)

Ein Nutzer synchronisiert Ordner zwischen seinen eigenen Geräten (Spec 025) und
möchte, dass die Dateien auch dann ankommen, wenn seine Geräte nicht
gleichzeitig online sind, ohne den Speicher des Relays zu nutzen. Er verbindet
dafür seinen eigenen S3-Speicher. Weil nur seine eigenen Geräte zugreifen,
braucht es keine eingeschränkten Zugangsschlüssel.

**Why this priority**: Das nutzt dieselbe Einrichtung ohne Mitglieder und ohne
Schlüsselverteilung. Es ist wertvoll, aber nicht nötig, um Spaces mit eigenem
Speicher zu betreiben.

**Independent Test**: Auf Gerät 1 den eigenen Speicher für die eigenen Ordner
verbinden, warten, bis Gerät 2 die Speicherverbindung mit dem Sync der Vault
erhalten hat (direkt oder über das eigene Postfach), Gerät 2 beenden. Dann auf Gerät 1 eine Datei ablegen,
Gerät 1 beenden, danach Gerät 2 starten: Die Datei kommt aus dem Bucket an,
obwohl beide seit dem Ablegen nie gleichzeitig online waren.

**Acceptance Scenarios**:

1. **Given** der Nutzer hat eigene synchronisierte Ordner, **When** er in den
   Einstellungen unter „Föderation“ → „Ordner“ den eigenen Speicher dafür
   verbindet, **Then** legt holzi einen Bucket für die eigenen Ordner an und
   nutzt ihn von allen eigenen Geräten aus.
2. **Given** dieselbe Einrichtung, **When** ein weiteres eigenes Gerät auf der
   Geräteliste der Vault synchronisiert, **Then** erhält es die
   Speicherverbindung aus dem Passwortmanager mit dem Datensync der Vault
   (Spec 024), direkt oder über das eigene Postfach, und greift ohne weitere
   Eingabe zu (FR-033).
3. **Given** der Anbieter kann keine Versionierung, **When** der Nutzer ihn für
   die eigenen Ordner verbindet, **Then** warnt holzi, dass gelöschte Objekte
   nicht wiederherstellbar sind, erlaubt die Einrichtung aber.
4. **Given** der Anbieter kann keine eingeschränkten Zugangsschlüssel, **When**
   der Nutzer ihn für die eigenen Ordner verbindet, **Then** ist das kein
   Hindernis, und die Eignungsprüfung meldet ihn deswegen nicht als ungeeignet
   (FR-032).

---

### User Story 7 - Bucket aufgeben und Zugangsdaten aufräumen (Priority: P2)

Die Admin zieht einen Space von ihrem eigenen Speicher weg (User Story 8) oder
nutzt ihn nicht mehr für ihre eigenen Ordner. holzi widerruft alle
Zugangsschlüssel dieses Buckets beim Anbieter und fragt, ob der Bucket samt aller
älteren Versionen gelöscht werden soll. Braucht keiner ihrer Spaces und nicht ihre
eigenen Ordner die Speicherverbindung mehr, kann sie sie entfernen; danach liegen
die Hauptzugangsdaten auf keinem ihrer Geräte mehr.

**Why this priority**: Zugangsschlüssel, die weiter gelten, obwohl niemand den
Bucket mehr nutzt, sind ein offenes Tor, und Hauptzugangsdaten, die ohne Zweck in
der Vault liegen, ein unnötiges Risiko. Das Aufräumen gehört zur Sicherheit,
auch wenn es selten vorkommt.

**Independent Test**: Die eigenen Ordner vom eigenen Speicher auf direkte
Übertragung zurückstellen: holzi fragt nach dem Bucket; mit „Bucket löschen“ ist
er weg, ohne bleibt er bestehen. Danach die Speicherverbindung entfernen: Keine
Vault-Datei der eigenen Geräte enthält noch die Hauptzugangsdaten. Für einen
Space dasselbe nach einem Umzug nach User Story 8: Beide Zugangsschlüssel werden
vom Anbieter abgelehnt.

**Acceptance Scenarios**:

1. **Given** ein Space oder die eigenen Ordner hören auf, einen Bucket zu nutzen,
   **When** das wirksam ist, **Then** widerruft holzi alle Generationen aller
   Zugangsschlüssel dieses Buckets beim Anbieter.
2. **Given** dieselbe Lage, **When** holzi nach dem Bucket fragt, **Then** kann
   die Admin wählen, ihn samt aller älteren Versionen zu löschen oder ihn zu
   behalten; Standard ist Behalten.
3. **Given** die Admin will eine Speicherverbindung entfernen, **When** noch ein
   Space oder die eigenen Ordner sie nutzen, **Then** verhindert holzi das und
   nennt, wer sie noch nutzt.
4. **Given** eine Speicherverbindung wird entfernt, **When** das Entfernen
   wirksam ist, **Then** enthält keine Vault-Datei eines eigenen Geräts mehr die
   Hauptzugangsdaten; ein Löschvermerk des Syncs trägt höchstens die Kennung des
   Eintrags.
5. **Given** der Widerruf beim Anbieter scheitert (etwa ohne Netz), **When** die
   Admin den Bucket aufgibt, **Then** merkt holzi sich den ausstehenden Widerruf,
   versucht ihn erneut, sobald der Anbieter erreichbar ist, und zeigt ihn als
   offen an, bis er gelungen ist.

---

### User Story 8 - Speicher eines Space wechseln (Priority: P3)

Ein Admin hat einen Space auf dem Speicher des Relays angelegt und möchte ihn in
seinen eigenen S3-Speicher umziehen, oder umgekehrt. holzi kopiert alle Objekte,
auf die der Dateiindex verweist, in den neuen Speicher, schaltet den Space erst
danach um und räumt den alten Speicher auf.

**Why this priority**: Nützlich, weil sonst die erste Wahl endgültig wäre. Aber
nicht nötig, um Backend B zu nutzen (siehe Clarifications).

**Independent Test**: Einen Space mit Dateien auf Backend A auf Backend B
umziehen (von „nur direkte Übertragung“ ebenso): Danach sind alle Dateien für
alle Mitglieder lesbar, neue Objekte landen im Bucket, und der Speicher des
Relays enthält keine Objekte des Space mehr. Dasselbe in die andere Richtung.

**Acceptance Scenarios**:

1. **Given** ein Space auf Backend A, **When** der Admin den Wechsel auf eigenen
   Speicher startet, **Then** laufen Eignungsprüfung und Einrichtung wie in User
   Story 1, danach kopiert ein Gerät des Admins alle Objekte, auf die gültige
   Einträge verweisen.
2. **Given** das Kopieren läuft, **When** Mitglieder den Space nutzen, **Then**
   lesen und schreiben sie weiter im bisherigen Speicher; neue Objekte kopiert
   holzi ebenfalls mit.
3. **Given** alle Objekte sind kopiert und geprüft, **When** holzi umschaltet,
   **Then** nutzen alle Mitglieder ab da den neuen Speicher, und der alte wird
   aufgeräumt (Backend A: Objekte gelöscht; Backend B: Zugangsschlüssel
   widerrufen, Bucket nach Wahl des Admins).
4. **Given** das Kopieren bricht ab, **When** der Admin es erneut startet,
   **Then** setzt es fort, ohne schon kopierte Objekte erneut zu übertragen, und
   der Space bleibt bis zum Umschalten im alten Speicher.

---

### Edge Cases

- **Zugangsschlüssel gelangt nach außen**: Der Admin kann jeden
  Zugangsschlüssel jederzeit von Hand erneuern (FR-020). Wer einen Schlüssel
  „nur Lesen“ hat, bekommt nur Chiffretexte, die er ohne Inhaltsschlüssel nicht
  öffnen kann. Wer „Lesen und Schreiben“ hat, kann Objekte löschen (zurückholbar
  nach User Story 4) oder unbrauchbare Objekte hochladen (am Hash erkannt, beim
  Aufräumen entfernt), aber keine Datei fälschen.
- **Mitglied wird während eines Downloads entfernt**: Die laufende Übertragung
  darf mit dem Widerruf scheitern.
- **Anbieter ohne Versionierung**: Für einen Space mit Mitgliedern lehnt holzi
  die Einrichtung ab und nennt den Grund (FR-006). Für die eigenen Ordner warnt
  holzi nur (User Story 6).
- **Anbieter erfüllt nur einen Teil der Kriterien**: Es gibt keine Zwischenstufe
  (D25). Fehlt ein Kriterium aus FR-004, ist er für Spaces ungeeignet; der Admin
  wählt einen anderen Anbieter, Backend A oder „nur direkte Übertragung“ (Spec
  027). Für die eigenen Ordner gilt die mildere Prüfung nach FR-032.
- **Versionierung wird später beim Anbieter ausgeschaltet** (etwa von Hand in
  dessen Oberfläche): Ein Gerät des Admins bemerkt das bei der nächsten Prüfung,
  schaltet sie wieder ein, wenn es kann, und warnt den Admin sonst.
- **Falsche Zugangsdaten**: Die Eignungsprüfung scheitert mit verständlichem
  Grund (falscher Schlüssel, fehlende Rechte, Endpunkt nicht erreichbar, Region
  passt nicht). Nichts wird gespeichert oder angelegt.
- **Hauptzugangsdaten werden beim Anbieter widerrufen oder laufen ab**: Die
  Zugangsschlüssel der Mitglieder gelten weiter, solange der Anbieter sie nicht
  mitwiderruft. Erneuerung, Wiederherstellung und Aufräumen scheitern jedoch;
  holzi zeigt dem Admin, dass neue Hauptzugangsdaten nötig sind.
- **Bucket wird außerhalb von holzi gelöscht**: Die Geräte erkennen, dass der
  Bucket fehlt, und zeigen den Speicher des Space als nicht verfügbar. Der Admin
  kann einen neuen Bucket einrichten; Geräte mit Schreibrecht laden die Objekte
  hoch, die sie noch haben. Dateien, deren Objekt kein Gerät mehr hat, meldet
  holzi als verloren.
- **Admin offline**: Mit Zugangsschlüsseln arbeiten die Mitglieder ohne den
  Admin weiter. Rechteänderungen und Erneuerungen gibt es ohnehin nur von einem
  Gerät des Admins aus, ebenso Wiederherstellung und Aufräumen (FR-025).
- **Zwei Geräte des Admins erneuern gleichzeitig**: Es entstehen zwei neue
  Generationen desselben Zugangsschlüssels. Mitglieder nutzen die höchste
  Generation; das Gerät des Admins, das die niedrigere sieht, widerruft sie
  (FR-019). Kein Zugangsschlüssel bleibt gültig, ohne dass er in der Vault des
  Admins verzeichnet ist.
- **Ein Mitglied lädt Datenmüll hoch** und treibt die Kosten: holzi kann das
  nicht verhindern. Nicht referenzierte Objekte räumt ein Gerät des Admins auf
  (FR-040); der Admin kann
  das Mitglied zurückstufen oder entfernen. Wenn der Anbieter den Verbrauch
  meldet, zeigt holzi ihn an (FR-036).
- **Anbieter-Kontingent erschöpft**: Hochladen scheitert mit einer Meldung, die
  auf den Speicher des Space verweist; die Datei bleibt lokal und wird später
  erneut versucht.
- **Gerät des Admins wird gestohlen**: Das Gerät hat die Hauptzugangsdaten und
  die Zugangsschlüssel aus dem Passwortmanager. Nachdem ein Hauptgerät es von
  der Geräteliste entfernt hat (Spec 024), bekommt es nichts Neues mehr. holzi
  ändert dabei keine Hauptzugangsdaten und keine Zugangsschlüssel automatisch
  (Spec 024 FR-026): Ob sie als kompromittiert gelten und beim Anbieter ersetzt
  werden, entscheidet der Admin; die Erneuerung von Hand bietet FR-020. War das gestohlene Gerät ein Hauptgerät und ist die Passphrase
  bekannt, ist die Vault verloren (Spec 024); das kann diese Spec nicht abfangen.
- **Ein Mitglied entfernt ein Gerät von seiner Geräteliste** (Spec 024, etwa ein
  verlorenes Gerät): Neue Umschläge gehen nur noch an die verbleibenden Geräte
  (D28). Den Zugangsschlüssel erneuert holzi dafür nicht automatisch; ob das
  Mitglied oder der Admin ihn als kompromittiert betrachtet, entscheiden sie
  selbst (FR-020).
- **Neues Mitglied, während eine Erneuerung aussteht**: Es erhält die höchste
  Generation, die das Gerät des Admins kennt.
- **Admin-Vault geht ganz verloren**: Mit einem Wiederherstellungspaket
  (Spec 026) holt der Admin die Vault samt Passwortmanager zurück, also auch
  Hauptzugangsdaten und Zugangsschlüssel (D30), und verwaltet den Speicher
  weiter. Ohne Wiederherstellung ist der Space eingefroren (Entwurf §15,
  offene Frage 3). Die Zugangsschlüssel gelten weiter, bis der Nutzer sie beim
  Anbieter selbst widerruft; holzi kann sie ohne die Admin-Vault nicht mehr
  verwalten.

## Requirements _(mandatory)_

### Functional Requirements

**Speicher verbinden und prüfen**

- **FR-001**: Der Admin eines Space MUSS für den Space eigenen S3-kompatiblen
  Speicher verbinden können, beim Anlegen des Space oder später (dann mit
  Wechsel nach FR-037). Mitglieder ohne Admin-Rolle DÜRFEN den Speicher NICHT
  festlegen oder ändern.
- **FR-002**: Zum Verbinden MUSS holzi Endpunkt, Region und die
  Hauptzugangsdaten abfragen, dazu, wenn der Anbieter es braucht, die Angabe, wie
  Buckets adressiert werden. Für die Anbieter aus FR-008 SOLL holzi Endpunkt und
  Adressierung vorbelegen.
- **FR-003**: Eine gespeicherte Speicherverbindung MUSS für weitere Spaces und
  die eigenen Ordner wiederverwendbar sein, ohne die Zugangsdaten erneut
  einzugeben.
- **FR-004**: Vor dem Speichern der Zugangsdaten als aktive Verbindung MUSS
  holzi die Eignungsprüfung durchführen. Die Prüfung DARF dafür eindeutig
  markierte, temporäre Testressourcen (etwa einen Test-Bucket und Testschlüssel)
  anlegen oder, wenn der Anbieter es unterstützt, Dry-Run-Prüfungen verwenden.
  Jede angelegte Testressource MUSS vor dem Ergebnis entfernt werden; schlägt
  die Bereinigung fehl, MUSS holzi eine verschlüsselte, dauerhaft gespeicherte
  Bereinigungsaufgabe mit Wiederholungsstatus anlegen, darf die Verbindung nicht
  als geeignet freigeben und darf die Zugangsdaten nicht verteilen. Die Prüfung
  MUSS mindestens prüfen, ob (a) die Zugangsdaten gültig sind und der Endpunkt
  erreichbar ist, (b) Buckets angelegt werden können, (c) die Versionierung
  eingeschaltet werden kann, (d) eine Aufbewahrungsfrist für ältere Versionen
  gesetzt werden kann, (e) auf einen Bucket beschränkte Zugangsschlüssel „nur
  Lesen“ und „Lesen und Schreiben“ erzeugt und widerrufen werden können und
  beide Schlüssel ihre erlaubten und verbotenen Operationen tatsächlich
  durchsetzen, einschließlich des Zugriffs auf keinen anderen Bucket, (f) der
  Schlüssel „Lesen und Schreiben“ weder ältere Versionen löschen noch
  Versionierung oder Aufbewahrungsfrist ändern kann und (g) der Anbieter einen
  widerrufenen Zugangsschlüssel innerhalb von 5 Minuten abweist, gemessen, indem
  die Prüfung einen Testschlüssel erzeugt, widerruft und bis zur Ablehnung
  weiter benutzt.
- **FR-005**: Das Ergebnis der Eignungsprüfung MUSS eines von zwei Ergebnissen
  sein und dem Admin mit Grund gezeigt werden: **geeignet** (alles aus FR-004
  erfüllt) oder **ungeeignet** (mindestens eine Eigenschaft aus FR-004 fehlt).
  Eine Zwischenstufe gibt es nicht (D25). Bei „ungeeignet“ MUSS jede nicht
  erfüllte Eigenschaft einzeln und verständlich benannt werden.
- **FR-006**: Für einen Space DARF holzi nur einen Anbieter mit dem Ergebnis
  „geeignet“ einrichten. Einen ungeeigneten Anbieter, etwa ohne Versionierung,
  DARF holzi NICHT verbinden, und es DARF keinen anderen Zugriffsweg anbieten
  (D25). Für die eigenen Ordner (FR-031) gilt die Prüfung nach FR-032; fehlende
  Versionierung meldet holzi dort nur als Warnung.
- **FR-007**: Scheitert die Prüfung oder die Einrichtung, DÜRFEN weder
  Zugangsdaten in der Vault noch halb angelegte Buckets oder Zugangsschlüssel
  beim Anbieter zurückbleiben; was holzi schon angelegt hat, MUSS es wieder
  entfernen.
- **FR-008**: holzi MUSS in v1 mindestens diese Anbieter nachweislich
  unterstützen: RustFS und AWS S3. Beide sind nach den Kriterien aus FR-004 geprüft und getestet.
  Erfüllt einer von ihnen ein Kriterium nicht, ist er ungeeignet und lässt sich für Spaces nicht
  verbinden (FR-006); der Betreiber entscheidet dann neu über den Umfang von v1. Cloudflare R2,
  Backblaze B2, Hetzner Object Storage und weitere kommen hinzu, sobald sie geprüft sind. Andere
  S3-kompatible Anbieter DÜRFEN verbunden werden, wenn sie die Eignungsprüfung bestehen.

**Bucket und Versionierung**

- **FR-009**: Jeder Space mit eigenem Speicher MUSS genau einen eigenen Bucket
  haben, der nur Objekte dieses Space enthält. Zwei Spaces DÜRFEN sich KEINEN
  Bucket teilen, auch nicht bei derselben Speicherverbindung.
- **FR-010**: Der Name des Buckets DARF weder Namen noch Kennung des Space noch
  Hinweise auf seine Mitglieder enthalten.
- **FR-011**: Bei der Einrichtung MUSS holzi die Versionierung des Buckets
  einschalten und eine Aufbewahrungsfrist für ältere Versionen setzen. Die
  Standardfrist ist 30 Tage; der Admin DARF sie ändern.
- **FR-012**: Der Bucket DARF NICHT öffentlich lesbar oder beschreibbar sein;
  holzi MUSS öffentlichen Zugriff bei der Einrichtung ausschließen, wenn der
  Anbieter das erlaubt.
- **FR-013**: Ein Gerät des Admins MUSS, wenn es online ist, mindestens einmal
  am Tag und bei jedem Öffnen des Space prüfen, ob die Versionierung noch
  eingeschaltet ist, sie wieder einschalten, wenn sie aus ist, und den Admin
  warnen, wenn das nicht gelingt.

**Zugangsschlüssel und Verteilung**

- **FR-014**: holzi MUSS bei der Einrichtung je Space
  zwei Zugangsschlüssel erzeugen, die nur für dessen Bucket gelten: „nur Lesen“
  (Objekte lesen und auflisten) und „Lesen und Schreiben“ (zusätzlich Objekte
  hochladen und löschen, nicht aber ältere Versionen löschen und keine
  Bucket-Einstellungen ändern).
- **FR-015**: holzi MUSS jedem Mitglied genau den Zugangsschlüssel geben, den
  seine Fähigkeit braucht: „nur Lesen“ für Lesen, „Lesen und Schreiben“ für
  Schreiben oder Löschen. Ein Mitglied mit Lesen DARF den Schlüssel „Lesen und
  Schreiben“ NICHT entschlüsseln können.
- **FR-016**: Zugangsschlüssel MÜSSEN zusammen mit den Angaben, die ein Gerät
  zum Zugriff braucht (Endpunkt, Region, Bucket), in Umschlägen verteilt werden,
  je ein Umschlag an jedes Gerät auf der aktuellen Geräteliste der Mitglieds-Vault
  (D28), auf demselben Weg wie die übrigen verschlüsselten Daten des Space
  (Spec 027). Das Relay DARF sie nur verschlüsselt weiterreichen.
- **FR-017**: Jedes Gerät des Mitglieds MUSS seinen Umschlag selbst mit seinem
  Geräteschlüssel öffnen. Die Mitglieds-Vault MUSS den empfangenen
  Zugangsschlüssel in ihrem Passwortmanager (Spec 034) ablegen, als
  gewöhnliche Vault-Daten im Bereich „Vault“ (D30, D32): Er gelangt mit dem Sync
  zwischen eigenen Geräten (Spec 024) auf alle Geräte der Mitglieds-Vault, auch
  über deren eigenes Postfach, sodass ein später verknüpftes Gerät ihn ohne
  Zutun des Admins hat, und lässt sich mit der Vault wiederherstellen. Spätere
  Generationen gehen als eigener Umschlag auch an dieses Gerät (FR-016).
  Zugangsschlüssel DÜRFEN NICHT in andere Spaces, in Datenfreigaben oder an
  andere Vaults gelangen, außer als Umschlag nach FR-016.

**Erneuern bei Entzug**

- **FR-018**: Entfernt der Admin ein Mitglied oder nimmt ihm eine Fähigkeit,
  MUSS holzi den betroffenen Zugangsschlüssel in einem Vorgang und in dieser
  Reihenfolge erneuern: neue Generation beim Anbieter erzeugen, die vorherige
  Generation beim Anbieter widerrufen, erst danach die neue Mitgliederliste
  veröffentlichen (Spec 027) und die neue Generation an alle verbleibenden
  Mitglieder mit dieser Art verteilen. Welcher Schlüssel betroffen ist, folgt aus
  FR-015. Das Erneuern des Inhaltsschlüssels (Spec 027) geschieht unabhängig
  davon. Bis der Anbieter den Widerruf bestätigt, MUSS holzi die bisherige
  Mitgliederliste unverändert lassen und darf die neue Generation nicht
  verteilen.
- **FR-019**: Beim Anbieter DARF je Art höchstens die höchste Generation
  gültig bleiben, sobald die Verteilung abgeschlossen ist. Sieht ein Gerät des
  Admins eine niedrigere, noch gültige Generation, MUSS es sie widerrufen.
- **FR-020**: Der Admin MUSS jeden Zugangsschlüssel eines Space jederzeit von
  Hand erneuern können, mit derselben Wirkung wie FR-018.
- **FR-021**: Kommt ein Mitglied hinzu oder erhält es eine höhere Fähigkeit,
  MUSS es die aktuelle Generation des passenden Schlüssels erhalten, ohne dass ein
  Zugangsschlüssel erneuert wird. Die neue Schlüsselgeneration des
  Inhaltsschlüssels nach Spec 027 bleibt davon unberührt.
- **FR-022**: Scheitert ein Widerruf beim Anbieter, MUSS holzi ihn als offen
  festhalten, erneut versuchen, bis der Anbieter ihn bestätigt, und dem Admin bis
  dahin eine bleibende Warnung zeigen, dass der alte Zugangsschlüssel noch gilt.
  Scheitert der Widerruf beim Entfernen oder Zurückstufen (FR-018), MUSS holzi
  die bisherige Mitgliederliste beibehalten und darf die neue Mitgliederliste
  oder neue Zugangsschlüssel nicht veröffentlichen beziehungsweise verteilen;
  das Relay darf nicht als Ersatz für den Anbieter-Widerruf gelten. Der offene
  Vorgang MUSS den bereits bestätigten Widerruf von einer späteren
  Veröffentlichung und Verteilung trennen und diese Schritte nach einem
  Neustart idempotent fortsetzen, ohne den erfolgreichen Widerruf zu wiederholen
  oder ungültig zu machen.

**Zugriffswege**

- **FR-023**: Die Geräte der Mitglieder MÜSSEN ausschließlich mit dem
  Zugangsschlüssel aus dem Passwortmanager ihrer eigenen Vault auf den Bucket
  zugreifen. Kein Gerät DARF für ein anderes
  Zugriffe beim Anbieter vermitteln oder ihm vom Anbieter signierte Einzelzugriffe
  ausstellen, weder über das Relay noch über die direkte Verbindung zwischen
  Mitgliedern (Spec 027) (D25).
- **FR-024**: Geräte der Admin-Vault MÜSSEN für ihre Zugriffe auf den Bucket die
  Hauptzugangsdaten aus dem Passwortmanager der Vault nutzen, die der Datensync
  von Spec 024 auf alle eigenen Geräte bringt. Jedes Gerät auf der aktuellen
  Geräteliste der Admin-Vault, Hauptgerät oder verknüpft, DARF den Speicher
  verwalten (D29). Ein Gerät des Admins, das die Hauptzugangsdaten noch nicht
  hat, DARF NICHT erneuern, wiederherstellen oder aufräumen (FR-018, FR-029,
  FR-040), bis es sie erhalten hat.
- **FR-025**: Lesen und Hochladen MÜSSEN für die Mitglieder auch funktionieren,
  wenn kein Gerät der Admin-Vault online ist. Nur Einrichtung, Rechteänderungen,
  Erneuern, Prüfen der Versionierung (FR-013), Wiederherstellen (FR-029) und
  Aufräumen (FR-040) brauchen ein Gerät des Admins.

**Direkter Zugriff und was der Speicher sieht**

- **FR-026**: Mit Zugangsschlüssel MÜSSEN die Geräte Objekte direkt beim
  Anbieter lesen und hochladen. Das Relay DARF an diesen Übertragungen NICHT
  beteiligt sein. Geräte der Mitglieder löschen dabei keine Objekte; das
  Aufräumen übernimmt ein Gerät des Admins (FR-040).
- **FR-027**: Im Bucket DÜRFEN nur Objekte liegen, benannt nach dem Hash ihres
  Chiffretexts. Dateinamen, Pfade, Ordnerstruktur, Klartextgrößen und
  Klartext-Hashes DÜRFEN weder in Objektnamen noch in Metadaten der Objekte
  stehen.
- **FR-028**: Ein Gerät MUSS jedes geladene Objekt gegen den Hash im Dateiindex
  prüfen und ein nicht passendes Objekt verwerfen.

**Wiederherstellen und Aufräumen**

- **FR-029**: Ein Gerät des Admins MUSS, wenn es online ist, mindestens einmal
  am Tag und auf Anforderung des Admins prüfen, ob jedes Objekt, auf das ein
  gültiger Eintrag im Dateiindex verweist, im Bucket vorhanden ist und zum Hash
  passt. Fehlt es oder passt es nicht, MUSS das Gerät die jüngste passende ältere
  Version wiederherstellen und dem Admin die betroffenen Dateien nennen.
- **FR-030**: Objekte, deren Einträge rechtmäßig gelöscht oder ersetzt wurden
  (Spec 025 und 027), DÜRFEN NICHT wiederhergestellt werden. Ist keine passende
  ältere Version mehr vorhanden, MUSS holzi die Datei als verloren melden, sofern
  kein Gerät das Objekt noch hat.
- **FR-040**: Alte Objekte (ersetzte Versionen einer Datei, gelöschte Dateien)
  und nicht referenzierte Objekte MUSS bei Backend B ein Gerät des Admins mit den
  Hauptzugangsdaten aus dem Bucket entfernen, sobald kein gültiger Eintrag im
  Dateiindex mehr auf sie verweist (Spec 027). Bei den eigenen Ordnern (FR-031)
  räumt ein eigenes Gerät auf. Ältere Versionen dieser Objekte bleiben bis zum
  Ende der Aufbewahrungsfrist erhalten.

**Eigene synchronisierte Ordner**

- **FR-031**: Der Nutzer MUSS für die eigenen synchronisierten Ordner seiner
  Vault (Spec 025) eigenen Speicher verbinden können, mit einem eigenen Bucket für
  diese Ordner.
- **FR-032**: Für die eigenen Ordner MÜSSEN die eigenen Geräte die
  Speicherverbindung selbst nutzen; Zugangsschlüssel entfallen. Die
  Eignungsprüfung MUSS fehlende eingeschränkte Schlüssel und einen zu langsamen
  Widerruf (FR-004 e bis g) dort ignorieren und fehlende Versionierung oder
  Aufbewahrungsfrist (FR-004 c und d) nur als Warnung melden. Ungeeignet ist ein
  Anbieter für die eigenen Ordner nur, wenn (a) oder (b) fehlen.

**Ende eines Space und Hauptzugangsdaten**

- **FR-033**: Die Hauptzugangsdaten und die Zugangsschlüssel, die der Admin
  erzeugt, MÜSSEN im Passwortmanager der Admin-Vault liegen (Spec 034,
  D32). Sie sind gewöhnliche Vault-Daten im Bereich „Vault“ (D30): Sie MÜSSEN mit
  dem Datensync von Spec 024 auf alle Geräte der Admin-Vault gelangen, direkt
  oder verschlüsselt über das eigene Postfach der Vault, und mit der Vault
  wiederherstellbar sein (Spec 026). Die Hauptzugangsdaten DÜRFEN weder an
  Mitglieder noch in Spaces, Datenfreigaben oder deren Postfächer und
  Momentaufnahmen gelangen, auch nicht verschlüsselt, und DÜRFEN in keinem
  Protokoll stehen. Das Relay DARF sie nur verschlüsselt im eigenen Postfach der
  Admin-Vault sehen.
- **FR-034**: Hört ein Space oder hören die eigenen Ordner auf, einen Bucket zu
  nutzen (Wechsel nach FR-037, Rückkehr der eigenen Ordner zur direkten
  Übertragung), MUSS holzi alle Generationen aller Zugangsschlüssel dieses
  Buckets widerrufen und den Admin fragen, ob der Bucket samt älterer Versionen
  gelöscht werden soll; Standard ist Behalten. Dasselbe MUSS gelten, wenn eine
  spätere Spec das Auflösen eines Space einführt (Spec 027 schließt es aus).
- **FR-035**: Der Admin MUSS eine Speicherverbindung entfernen können, sobald
  kein Space und nicht die eigenen Ordner sie mehr nutzen; vorher MUSS holzi das
  verhindern und die Nutzer der Verbindung nennen. Nach dem Entfernen DÜRFEN die
  Hauptzugangsdaten in keiner Vault-Datei eines eigenen Geräts bleiben; ein
  Löschvermerk des Syncs DARF nur die Kennung des Eintrags tragen.

**Verbrauch**

- **FR-036**: holzi KANN dem Admin den belegten Speicher eines Space anzeigen,
  getrennt nach aktuellen und älteren Versionen, wenn der Anbieter diese Angaben
  liefert.

**Wechsel des Speicher-Backends**

- **FR-037**: Der Admin MUSS einen Space von Backend A oder „nur direkte
  Übertragung“ (Spec 027) auf Backend B und von B auf A umziehen können. Ein
  Gerät des Admins MUSS dazu alle Objekte, auf die gültige Einträge verweisen,
  in den neuen Speicher kopieren und gegen ihren Hash prüfen. Vor dem atomaren
  Umschalten MUSS es neue Schreibvorgänge kurz quieszieren oder dual erfassen,
  einen letzten Delta-Abgleich durchführen und erst danach umschalten, sodass
  jede bis zur Sperre akzeptierte Änderung im neuen Speicher vorhanden ist.
- **FR-038**: Bis zum Umschalten MÜSSEN die Mitglieder den bisherigen Speicher
  weiter nutzen können; ein abgebrochener Umzug MUSS sich fortsetzen lassen, ohne
  schon kopierte Objekte erneut zu übertragen. Der Umzug darf erst nach dem
  Schreibbarriere- und Delta-Abgleich als abgeschlossen gelten.
- **FR-039**: Nach dem Umschalten MUSS holzi den alten Speicher aufräumen:
  Objekte auf Backend A löschen, bei Backend B die Zugangsschlüssel widerrufen und
  den Bucket nach Wahl des Admins löschen oder behalten (wie FR-034).

### Key Entities

- **Speicherverbindung**: wie in Spec 038 festgelegt; Anbieter, Endpunkt, Region, Adressierung und
  Hauptzugangsdaten. Gehört einer Vault, liegt in deren Passwortmanager und
  damit auf allen ihren Geräten (FR-033) und kann von mehreren Spaces und den
  eigenen Ordnern genutzt werden.
- **Speicher des Space**: welches Backend ein Space nutzt und bei Backend B
  welcher Bucket und welche Aufbewahrungsfrist. Wird vom Admin geschrieben und mit den Daten des Space
  verteilt, ohne Hauptzugangsdaten.
- **Zugangsschlüssel**: Art (nur Lesen, Lesen und Schreiben), Generation,
  Kennung beim Anbieter (für den Widerruf) und die geheimen Schlüsseldaten.
  Liegt im Passwortmanager der Admin-Vault und jeder Mitglieds-Vault, die ihn
  erhalten hat. Die Vault des Admins verzeichnet alle Generationen und ob sie
  widerrufen sind.
- **Schlüsselumschlag für Zugangsschlüssel**: ein Zugangsschlüssel samt
  Endpunkt, Region und Bucket, verschlüsselt an genau ein Gerät auf der
  aktuellen Geräteliste einer Mitglieds-Vault; je Gerät ein Umschlag (D28).
- **Eignungsergebnis**: Ergebnis (geeignet oder ungeeignet) und die Liste der
  erfüllten und nicht erfüllten Eigenschaften aus FR-004.
- **Offener Widerruf**: ein Zugangsschlüssel, dessen Widerruf beim Anbieter
  noch aussteht (FR-022).

## Success Criteria _(mandatory)_

### Measurable Outcomes

- **SC-001**: Speicher, Datenbank und Protokolle des Relays enthalten nach einer
  vollständigen Testreihe (Einrichten, Einladen, Hoch- und Herunterladen, Entzug,
  Löschen) in 0 % der Fälle Hauptzugangsdaten oder einen Zugangsschlüssel in
  lesbarer Form, und Hauptzugangsdaten verschlüsselt in 0 % der Fälle außerhalb
  des eigenen Postfachs der Admin-Vault.
- **SC-002**: Nach dem Entfernen eines Mitglieds lehnt der Anbieter dessen alten
  Zugangsschlüssel in 100 % der Testläufe innerhalb von 5 Minuten ab, für jeden
  Anbieter mit dem Ergebnis „geeignet“ (die Prüfung nach FR-004 g hat diese Frist
  gemessen), sofern ein Gerät des Admins online und der Anbieter erreichbar ist
  (FR-018); sonst gilt die Frist ab dem bestätigten Widerruf (FR-022).
- **SC-003**: Eine Durchsicht des Buckets mit vollem Zugriff findet in 0 % der
  Objekte einen Dateinamen, Pfad oder lesbaren Inhalt, und der Name des Buckets
  enthält weder Namen noch Kennung des Space.
- **SC-004**: Ein Admin mit bereitliegenden Hauptzugangsdaten verbindet einen
  Space auf einem geeigneten Anbieter in weniger als 3 Minuten mit seinem eigenen
  Speicher.
- **SC-005**: Für jeden ungeeigneten Testanbieter nennt die Eignungsprüfung in
  100 % der Fälle jede fehlende Eigenschaft einzeln, und danach bleiben beim
  Anbieter 0 Buckets und 0 Zugangsschlüssel aus dem Versuch zurück.
- **SC-006**: Ein Objekt, das ein Mitglied mit Schreibrecht direkt im Bucket
  gelöscht oder überschrieben hat, ist innerhalb der Aufbewahrungsfrist in 100 %
  der Testläufe wiederherstellbar und nach der nächsten Prüfung durch ein Gerät
  des Admins wieder für alle Mitglieder lesbar.
- **SC-007**: Ein Mitglied mit Lesen scheitert in 100 % der Versuche am Anbieter,
  wenn es hochladen oder löschen will; ein Mitglied mit Schreiben scheitert in
  100 % der Versuche, ältere Versionen zu löschen oder die Versionierung
  auszuschalten.
- **SC-008**: Für jeden Testanbieter, dem genau ein Kriterium aus FR-004 (a bis
  g) fehlt, endet die Eignungsprüfung in 100 % der Fälle mit „ungeeignet“, und in
  0 % der Versuche lässt sich damit ein Space verbinden.
- **SC-009**: Nach einem Umzug zwischen Backend A und B sind 100 % der Dateien
  für alle Mitglieder lesbar, und der alte Speicher enthält nach dem Aufräumen
  keine Objekte des Space mehr (Backend B: sofern der Admin den Bucket löschen
  lässt).
- **SC-010**: Nachdem ein Space oder die eigenen Ordner einen Bucket aufgegeben
  haben, gelten beim Anbieter 0 Zugangsschlüssel dieses Buckets mehr, sobald der
  Anbieter erreichbar war.

## Assumptions

- Spec 027 liefert Mitgliederliste, Fähigkeiten, Einladung, Entzug und das
  Erneuern des Inhaltsschlüssels; diese Spec hängt nur die Zugangsschlüssel daran.
  Die dort noch offenen Löschregeln (Entwurf §11) ändern hier nur, wer
  rechtmäßig löschen darf, nicht, wie der Speicher geschützt wird.
- Spec 024 liefert die Geräteliste und den Datensync zwischen eigenen Geräten,
  Spec 026 das eigene Postfach der Vault und die Wiederherstellung. Die
  Hauptzugangsdaten und die Zugangsschlüssel sind gewöhnliche Vault-Daten im
  Passwortmanager (D30, D32); ein Gerät der Vault, das nie gleichzeitig mit
  einem anderen eigenen Gerät online ist, bekommt sie deshalb über das eigene
  Postfach, sobald die Vault ein Relay hat. Ohne Relay kommen sie erst bei der
  nächsten direkten Verbindung an; dasselbe gilt für die Speicherverbindung der
  eigenen Ordner (User Story 6).
- Der Passwortmanager (Spec 034) steht bereit, bevor diese Spec
  umgesetzt wird. Diese Spec legt nur fest, welche Geheimnisse dort liegen.
- Das Aufräumen alter Objekte (FR-040) übernimmt bei Backend B ein Gerät des
  Admins, weil nur es die Hauptzugangsdaten hat; bei Backend A regelt Spec 026,
  wer löschen darf.
- Eingeschränkte Zugangsschlüssel werden über die Verwaltungsschnittstelle des
  jeweiligen Anbieters erzeugt, die nicht bei allen Anbietern Teil der
  S3-Schnittstelle ist. Deshalb braucht jeder unterstützte Anbieter eine eigene
  Anbindung dafür; der Plan legt fest, wie.
- Ein Zugangsschlüssel gilt je Art für alle Mitglieder dieser Art, nicht je
  Mitglied. Entfernen eines Mitglieds erneuert deshalb den Schlüssel aller
  verbleibenden Mitglieder dieser Art. Schlüssel je Mitglied wären feiner, aber
  der Entwurf sieht zwei Schlüssel je Space vor, und die Kosten der Erneuerung
  bleiben klein.
- Mitglieder mit Löschen erhalten denselben Schlüssel wie Mitglieder mit
  Schreiben, weil S3 Schreiben und Löschen nicht trennt. Die Unterscheidung prüfen
  die Empfänger im Dateiindex (Spec 027); User Story 4 fängt die Grenze ab.
- Die Aufbewahrungsfrist von 30 Tagen ist eine Voreinstellung, keine Garantie
  des Anbieters. Setzt der Anbieter sie nicht um, gilt sie als nicht erfüllt
  (FR-004 d), und der Anbieter ist für Spaces ungeeignet.
- Anbieter ohne auf einen Bucket beschränkte Zugangsschlüssel sind für Spaces
  ausgeschlossen (D25). Wer einen solchen Anbieter betreibt, nutzt für Spaces
  Backend A oder „nur direkte Übertragung“ und für die eigenen Ordner weiter den
  eigenen Speicher.
- Die Wiederherstellung (FR-029) setzt voraus, dass mindestens ein Gerät des
  Admins regelmäßig online ist. Zwischen zwei Prüfungen kann eine gelöschte Datei
  für Mitglieder unerreichbar sein.
- Der Anbieter sieht IP-Adressen, Zeitpunkte, Anzahl und Größe der Objekte und
  wer mit welchem Zugangsschlüssel zugreift. Das ist wie beim Relay (Entwurf
  §10.4) hingenommen.
- Kosten beim Anbieter trägt der Nutzer, dem die Speicherverbindung gehört.
- Verlust der Admin-Vault ohne Wiederherstellung (Spec 026) friert den Space
  ein (Entwurf §15, offene Frage 3); eine Übergabe der Admin-Rolle ist nicht Teil
  dieser Spec.

## Nicht im Umfang

- Zugangsschlüssel je Mitglied statt je Art.
- Neues Verschlüsseln aller Objekte nach einem Entzug; ein entferntes Mitglied
  behält, was es schon entschlüsseln konnte (Entwurf §13, Spec 027).
- Mehrere Speicher für einen Space gleichzeitig (Spiegelung, Redundanz über
  Anbieter hinweg).
- Speicher, der kein S3 spricht (WebDAV, SFTP, Dateifreigaben im Netz).
- Zugriff auf Anbieter, die die Eignungsprüfung nicht bestehen, etwa über
  kurzlebige, vom Anbieter signierte Einzelzugriffe, die ein Gerät des Admins
  ausstellt (D25).
- Verwaltung von Konten, Abrechnung oder Kontingenten beim Anbieter über die
  Anzeige nach FR-036 hinaus.
- Eine Übergabe der Admin-Rolle oder der Speicherverbindung an ein anderes
  Mitglied.
- Einen Space auflösen oder löschen (wie Spec 027); FR-034 gilt, sobald eine
  spätere Spec das einführt.
- Ein Umzug von Backend B auf „nur direkte Übertragung“; dabei gingen Dateien
  verloren, die kein Gerät hat.
- Datenfreigaben (Spec 028); sie nutzen keinen Objektspeicher.
- Der Passwortmanager selbst, seine Oberfläche und sein Zugriff für
  Erweiterungen (Spec 034).
- Eigener Speicher für das Postfach eines Space; Dateiindex, Mitgliederliste und
  Schlüsselumschläge laufen weiter über das Relay des Admins, wenn eines
  eingerichtet ist, sonst nur direkt (Spec 026 und 027).
