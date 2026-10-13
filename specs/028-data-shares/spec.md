# Feature Specification: Datenfreigaben: Daten von Erweiterungen mit einzelnen Nutzern teilen

**Feature Branch**: `028-data-shares`
**Created**: 2026-09-28
**Status**: Draft
**Input**: Umsetzung von Zeile 028 des Sync-Entwurfs
([`2026-09-28-sync-architecture.md`](../../docs/plans/2026-09-28-sync-architecture.md),
§9 mit §5.4, §10.3, §11 und §13). Erweiterungen erklären in ihrem Manifest mit
einem einheitlichen Schema, was sich teilen lässt: einzelne Einträge (ein
Kalendereintrag) oder ganze Sammlungen (ein Kalender). holzi prüft diese
Erklärung bei Installation und Update, bietet einen eigenen Teilen-Dialog an,
vergibt Rechte je Person, berechnet selbst, welche Einträge zu einer Freigabe
gehören, und schützt beim Senden und beim Empfangen, dass nur diese Einträge
ausgetauscht werden. Die Erweiterung selbst kommt nie mit Schlüsseln,
Synchronisierung oder Rechten in Berührung.

## Begriffe

- **Vault-Identität**: das Schlüsselpaar, das eine Vault nach außen vertritt.
  Sein öffentlicher Schlüssel ist die feste Adresse der Vault; Rechte und
  Mitgliederlisten nennen immer eine Vault-Identität. Der private Schlüssel
  liegt nur auf Hauptgeräten (Spec 024).
- **Geräteschlüssel**: das eigene Schlüsselpaar eines Geräts; er verlässt das
  Gerät nie, unterschreibt alles, was auf diesem Gerät entsteht, und empfängt
  die verschlüsselten Inhaltsschlüssel (Spec 024, D28).
- **Geräteliste**: die mit der Vault-Identität von einem Hauptgerät
  unterschriebene Liste aller aktuellen Geräte einer Vault (öffentlicher
  Geräteschlüssel, Rolle Hauptgerät oder verknüpftes Gerät, Name,
  Netzwerkkennung) mit einer Generation (Spec 024). Ein Gerät gilt für eine
  Vault nur, wenn es auf deren aktueller Geräteliste steht. **Hauptgeräte**
  dürfen Geräte hinzufügen und entfernen, **verknüpfte Geräte** nicht;
  Datenfreigaben verwalten dürfen beide (D29).
- **Bereich**: eine Einheit, die gemeinsam synchronisiert und verschlüsselt
  wird: der Bereich Vault, der Bereich eines Space oder der Bereich einer
  Datenfreigabe.
- **Änderungspaket**: eine verschlüsselte Menge zusammengehöriger Änderungen
  eines Bereichs, so wie sie zwischen Geräten und über das Relay reist. Ein
  Paket trennt nie Änderungen, die zusammen geschrieben wurden.
- **Inhaltsschlüssel**: der Schlüssel, mit dem die Änderungspakete eines
  Bereichs verschlüsselt sind. **Schlüsselgeneration**: eine Fassung davon;
  jede Änderung der Mitglieder (Aufnahme nach bestätigter Annahme, Ändern
  von Fähigkeiten, Entfernen, Austreten) erzeugt eine neue, und ihre
  Mitgliederliste steht damit fest.
- **Relay**: der nicht vertrauenswürdige Server aus Spec 026. Es speichert und
  verteilt nur Verschlüsseltes. **Postfach**: der Speicherplatz eines Bereichs
  auf dem Relay. **Mitgliederliste**: die von einem Gerät des Eigentümers unterschriebene Liste
  der Vault-Identitäten und ihrer Fähigkeiten je Bereich, die auf das Relay
  hochgeladen wird. Ihre **Generation** ist die Schlüsselgeneration, zu der sie
  gehört. **Grenze**: der Teil einer Mitgliederliste, die eine Vault entfernt
  oder ihre Fähigkeiten senkt; er nennt für jedes Gerät dieser Vault die
  höchste lückenlose Nummer je Ursprungsgerät, die das erstellende Gerät da
  angewendet hatte (Spec 024).
- **Erweiterung**: eine haextension im Sinne von ADR-0004. **Eigene Tabellen**
  einer Erweiterung sind die Tabellen mit ihrem Präfix aus öffentlichem
  Schlüssel und Name der Erweiterung (dieselbe Konvention wie haex-vault,
  Repository `https://github.com/haex-space/haex-vault`, Revision
  `8dce379d94e18fcd42c3b73686a06f984ca3f574`, Pfad
  `src-tauri/src/extension/utils.rs`, `get_extension_table_prefix`).
- **Datenfreigabe**: das Teilen von SQLite-Daten einer Erweiterung mit
  einzelnen Nutzern. Sie hat genau einen **Eigentümer (Admin der
  Datenfreigabe)**, die Vault, die sie angelegt hat, und null oder mehr
  **Empfänger**. Diese Spec sagt im Folgenden kurz „Eigentümer“. Die
  Eigentümerschaft ist nicht übertragbar (D23, D26). Jedes Gerät auf der
  aktuellen Geräteliste des Eigentümers darf für ihn handeln (D29).
- **Freigabetyp**: eine Art teilbarer Daten, die eine Erweiterung erklärt,
  entweder ein einzelner Eintrag (ein Kalendereintrag) oder eine Sammlung (ein
  ganzer Kalender, eine Einkaufsliste).
- **Wurzel**: der Eintrag, den der Eigentümer teilt (der Kalender, der
  Kalendereintrag). **Zugehörige Einträge**: alle Einträge, die über die im
  Freigabetyp erklärten Fremdschlüssel von der Wurzel aus erreichbar sind (die
  Termine eines Kalenders, deren Teilnehmer und Erinnerungen).
- **Ersteller**: die Vault-Identität, die einen Eintrag angelegt hat. holzi
  setzt sie beim Anlegen, und sie ändert sich danach nie.
- **Fähigkeiten**: drei aufeinander aufbauende Stufen **Lesen**, **Schreiben**
  und **Löschen**, je Empfänger genau eine: Schreiben umfasst Lesen, Löschen
  umfasst Schreiben (wie bei Spaces, Spec 027). **Admin** ist allein der
  Eigentümer; Admin lässt sich nicht vergeben.
- **Space**: ein Netzwerkordner nur für Dateien (Spec 027). Spaces tragen nie
  SQLite-Daten.

## Beziehung zu bestehenden Specs

- **Sync-Entwurf** ([`2026-09-28-sync-architecture.md`](../../docs/plans/2026-09-28-sync-architecture.md)):
  Quelle dieser Spec. §9 beschreibt Erklärung, Prüfung, abgeleitete
  Zugehörigkeit, Schutz beim Senden und Empfangen und überlappende Freigaben,
  §11 die Fähigkeiten, §13 das Bedrohungsmodell. Die Entwurfsregeln zu Löschen
  und Entzug sind dort nur vorgeschlagen; diese Spec übernimmt sie als offene
  Fragen (FR-028, FR-034).
- **Früherer Entwurf zum Teilen zwischen Nutzern**
  ([`2026-09-07-cross-user-sharing-deferred-design.md`](../../docs/plans/2026-09-07-cross-user-sharing-deferred-design.md)):
  Die dort vorgesehene Liste, welche Zeile zu welchem Bereich gehört, entfällt.
  Die Zugehörigkeit eines Eintrags zu einer Datenfreigabe wird aus den
  erklärten Fremdschlüsseln berechnet (FR-012). Die Tabellenliste als Schutz vor
  Datenabfluss bleibt als Schutz beim Senden (FR-013).
- **Spec 024** (Vault-Identität, Geräteschlüssel, Geräteliste und Sync
  zwischen eigenen Geräten): liefert Vault-Identität, Geräteliste mit
  Hauptgeräten und verknüpften Geräten, unterschriebene Änderungen mit echtem
  Autor, das Führen des Erstellers durch holzi und die feste Liste, welche
  Daten die Vault überhaupt verlassen dürfen, einschließlich der
  Nur-direkt-Daten (nur noch der private Schlüssel der Vault-Identität, D30).
  Empfangene Datenfreigaben und die empfangenen Inhaltsschlüssel erreichen die
  übrigen Geräte des Empfängers über diesen Sync (FR-022, D28). Spec 024 legt
  außerdem fest, dass ein Änderungspaket nur als Ganzes angewendet wird. Ein
  Rotieren der Vault-Identität gibt es in v1 nicht (D26); ein verlorenes Gerät
  entfernt ein Hauptgerät aus der Geräteliste (Spec 024, D27).
- **Spec 026** (Relay): liefert Postfach, Mitgliederliste und die Prüfung
  der Geräteliste am Relay. Jede Datenfreigabe ist dort ein eigener Bereich. Das Relay prüft nur grob Lesen
  gegen Schreiben; alle feineren Regeln dieser Spec prüft jeder Empfänger
  selbst.
- **Spec 027** (Spaces): Spaces teilen Dateien, Datenfreigaben teilen Daten.
  Beide verwenden dieselben Fähigkeitsstufen, dieselbe Regel für eigene
  Einträge, dieselben Regeln für die Mitgliederliste und dasselbe Muster einer
  neuen Schlüsselgeneration bei jeder Änderung der Mitglieder. Diese Spec
  übernimmt aus Spec 027 den Einladungslink, den Weg für die Antworten darauf und die direkte Verbindung
  zwischen Mitgliedern. Dateien, die ein Eintrag einer Erweiterung
  referenziert, teilt diese Spec nicht.
- **Spec 023** (Einstellungen): Die Verwaltung der Datenfreigaben liegt in der
  Einstellungskategorie „Föderation“ in der Unteransicht „Datenfreigaben“
  (FR-015).
- **ADR-0004** und die geplante Spec 017 (Erweiterungs-Host): Das signierte
  Manifest, die Bestätigung bei der Installation und die eine Prüfstelle für
  alle Anfragen einer Erweiterung kommen von dort. Diese Spec ergänzt das
  Manifest um Freigabetypen und die Brücke um zwei Anfragen: den Teilen-Dialog
  öffnen und den Freigabestatus eigener Einträge lesen. Die Tabellen einer
  Erweiterung mit Präfix, die synchronisiert werden, gibt es in holzi noch
  nicht (Entwurf §15 Punkt 10); diese Spec setzt sie voraus.
- **Spec 021** (Rechte für Agenten): Diese Spec gibt Agenten keinen Weg, eine
  Datenfreigabe anzulegen oder Rechte zu ändern (siehe Assumptions).

## Clarifications

### Session 2026-09-27

- Q: Werden SQLite-Daten über Spaces geteilt? → A: Nein (D3). Spaces sind
  Netzwerkordner nur für Dateien. Lesen und Schreiben von Daten wird einzeln je
  Nutzer vergeben, über Datenfreigaben.
- Q: Was lässt sich als Datenfreigabe teilen, und wer legt das fest? → A:
  Einzelne Einträge und ganze Sammlungen (D4). Erweiterungen erklären das mit
  einem einheitlichen Schema, das holzi für jede Erweiterung gleich prüfen kann.
- Q: Wer verwaltet eine Datenfreigabe? → A: Der Eigentümer, also wer sie
  angelegt hat, ist ihr einziger Admin (D6). Nur er lädt ein, vergibt, ändert
  und entzieht Fähigkeiten und entfernt Empfänger.
- Q: Darf ein Empfänger weiterteilen? → A: Nein (D7). Nur der Eigentümer lädt
  ein. Dass ein Empfänger Inhalte kopiert und selbst neu teilt, verhindert holzi
  nicht; das ist nicht im Umfang.
- Q: Werden die Schlüssel einer Datenfreigabe wie in haex-vault mit MLS
  verwaltet? → A: Nein (D13). MLS braucht eine feste Reihenfolge, die zu einer
  reihenfolgefreien Synchronisierung nicht passt. Schlüsselgenerationen sind
  gewöhnliche synchronisierte Einträge; zwei gleichzeitig entstandene
  Generationen sind beide gültig.

### Session 2026-09-28

- Q: Über welche Tabellen darf eine Erweiterung Freigabetypen erklären? → A: Nur
  über ihre eigenen Tabellen mit Präfix aus öffentlichem Schlüssel und Name
  (D5). Fremdschlüssel dürfen nur zwischen diesen Tabellen verlaufen, und nur
  synchronisierte Tabellen sind erlaubt. Sonst wird die ganze Erweiterung
  abgelehnt, nicht nur die Erklärung.
- Q: Wie ist Löschen geregelt? → A: „Schreiben“ erlaubt das Löschen eigener Einträge (Ersteller = eigene Vault); fremde Einträge löschen erfordert die Stufe „Löschen“ (FR-028). Gilt ebenso für Spec 027.
- Q: Wie wirkt ein Entzug bei gleichzeitigem Schreiben? → A: Nur nach vorn, mit Prüfung gegen die neueste bekannte Mitgliederliste beim Empfang und sofortiger Sperre am Relay; das verbleibende kleine Zeitfenster wird akzeptiert (FR-034). Gilt ebenso für Spec 027.
- Q: Kann die Admin-Rolle übertragen werden? → A: Nein, in v1 gar nicht, und
  nichts hängt von der Zustimmung der Mitglieder ab (D23). (Das Schließen und
  Verlassen aller Bereiche bei einem Wechsel der Vault-Identität ist überholt
  durch D26.)
- Q: Gibt es in v1 ein Rotieren der Vault-Identität? → A: Nein (D26). Ein
  verlorenes Gerät ist kein Problem, solange eine Kopie oder das Relay existiert
  und die Passphrase hält; ausgesperrt wird ein Gerät über die Geräteliste
  (D27).
- Q: An wen werden Schlüssel von Spaces und Datenfreigaben verschlüsselt? → A:
  An jedes Gerät der Mitglieds-Vaults laut deren aktueller Geräteliste (D28).
- Q: Welche Geräte des Eigentümers dürfen eine Datenfreigabe verwalten? → A:
  Jedes Gerät auf der aktuellen Geräteliste der Eigentümer-Vault, auch ein
  verknüpftes Gerät; es unterschreibt mit seinem Geräteschlüssel. Nur das
  Verwalten der Geräte selbst bleibt Hauptgeräten vorbehalten (D29).
- Q: Sind entpackte Inhaltsschlüssel einer empfangenen Datenfreigabe
  Nur-direkt-Daten? → A: Nein. Nur der private Schlüssel der Vault-Identität
  reist ausschließlich direkt; entpackte Inhaltsschlüssel sind gewöhnliche
  Vault-Daten, die auch über das Postfach der Vault synchronisiert werden und
  wiederherstellbar sind (D30).

### Session 2026-10-01

- Q: Wie wird jemand zu einer Datenfreigabe eingeladen? → A: Wie bei Spaces
  (Spec 027, Klärung vom 2026-10-01) nur über einen Einladungslink, den der
  Eigentümer erzeugt und selbst weitergibt, nie über die Vault-Identität allein.
  Das Recht entsteht erst, wenn der Eigentümer die Annahme bestätigt (FR-008,
  FR-010, FR-019).

## User Scenarios & Testing _(mandatory)_

### User Story 1 - Erweiterung erklärt, was sich teilen lässt (Priority: P1)

Die Autorin einer Kalender-Erweiterung möchte, dass Nutzer einzelne Termine und
ganze Kalender teilen können. Sie trägt in das Manifest zwei Freigabetypen ein,
„Kalender“ und „Kalendereintrag“, jeweils mit Wurzeltabelle, Beschriftungsfeld
und zugehörigen Tabellen. Code für Schlüssel, Synchronisierung oder Rechte
schreibt sie nicht. holzi prüft die Erklärung bei der Installation und lehnt
Erweiterungen ab, die über fremde Tabellen teilen wollen.

**Why this priority**: Ohne geprüfte Erklärung gibt es nichts zu teilen. Die
Prüfung ist zugleich der erste Schutz davor, dass eine Erweiterung Daten des
Kerns oder anderer Erweiterungen hinausträgt.

**Independent Test**: Drei Testerweiterungen installieren: eine mit gültiger
Erklärung, eine mit einer Kerntabelle als zugehöriger Tabelle, eine mit einer
nicht synchronisierten Tabelle. Die erste wird installiert und ihre
Freigabetypen stehen im Bestätigungsdialog; die beiden anderen werden mit einer
Meldung abgelehnt, die Tabelle und Grund nennt, und nichts von ihnen bleibt
installiert.

**Acceptance Scenarios**:

1. **Given** eine Erweiterung, deren Freigabetypen nur eigene, synchronisierte
   Tabellen und Fremdschlüssel zwischen ihnen nennen, **When** der Nutzer sie
   installiert, **Then** zeigt der Bestätigungsdialog die Freigabetypen mit
   Namen, und nach der Bestätigung ist die Erweiterung installiert.
2. **Given** eine Erweiterung, deren Freigabetyp eine Tabelle außerhalb ihres
   Präfixes nennt (eine Kerntabelle oder eine Tabelle einer anderen
   Erweiterung), **When** der Nutzer sie installieren will, **Then** lehnt holzi
   die ganze Erweiterung ab und nennt die Tabelle.
3. **Given** eine Erweiterung, deren Fremdschlüssel aus einer eigenen Tabelle in
   eine fremde zeigt, **When** der Nutzer sie installieren will, **Then** lehnt
   holzi sie ab und nennt die Verbindung.
4. **Given** eine Erweiterung, die eine nicht synchronisierte Tabelle in einen
   Freigabetyp aufnimmt, **When** der Nutzer sie installieren will, **Then**
   lehnt holzi sie ab.
5. **Given** eine installierte Erweiterung mit bestehenden Datenfreigaben,
   **When** ein Update eine unzulässige Erklärung mitbringt, **Then** wird das
   Update abgelehnt, und die installierte Version bleibt mit ihren Daten und
   Datenfreigaben unverändert.

---

### User Story 2 - Einen Termin oder einen ganzen Kalender teilen (Priority: P1)

Anna möchte ihrem Kollegen Ben einen einzelnen Termin zeigen und ihrer Familie
den ganzen Kalender „Familie“. In der Kalender-Erweiterung wählt sie beim Termin
„Teilen“. Es öffnet sich der Teilen-Dialog von holzi, nicht einer der
Erweiterung. Er zeigt den Titel des Termins, Anna erzeugt für Ben
einen Einladungslink, schickt ihn ihm selbst und lässt es bei „Lesen“. Den Kalender „Familie“ teilt sie
mit ihrem Mann Carl mit „Schreiben“. Trägt sie danach einen neuen
Termin in „Familie“ ein, sieht Carl ihn, ohne dass Anna ihn eigens teilt.

**Why this priority**: Das ist der Kern der Funktion: Daten an einzelne Personen
mit einzelnen Rechten weitergeben, ohne dass die Erweiterung Schlüssel oder
Rechte selbst verwalten muss.

**Independent Test**: In einer Vault mit Testkalender einen Termin mit einer
zweiten Vault teilen und einen Kalender mit einer dritten. Die zweite Vault
erhält genau diesen Termin mit Teilnehmern und Erinnerungen, die dritte den
Kalender mit allen Terminen. Ein danach angelegter Termin im geteilten Kalender
erscheint bei der dritten Vault, bei der zweiten nicht. Kein anderer Eintrag der
Vault erreicht eine der beiden.

**Acceptance Scenarios**:

1. **Given** ein Termin in einer Erweiterung mit dem Freigabetyp
   „Kalendereintrag“, **When** die Erweiterung das Teilen anfragt, **Then**
   öffnet holzi seinen Teilen-Dialog mit Titel des Termins, Name des
   Freigabetyps, Name der Erweiterung und den bisherigen Empfängern.
2. **Given** der Teilen-Dialog, **When** Anna einen Namen für den Empfänger
   angibt, eine Fähigkeitsstufe wählt und bestätigt, **Then** besteht die
   Datenfreigabe, und holzi zeigt einen Einladungslink ohne Schlüssel (Text und
   QR-Code), den Anna selbst weitergibt; die Einladung steht als „offen“ in der
   Übersicht. Mitgliederliste und Schlüsselgeneration ändern sich erst, wenn
   Anna die Annahme bestätigt hat (FR-010).
3. **Given** der Kalender „Familie“ ist mit Carl geteilt, **When** Anna darin
   einen Termin anlegt, **Then** gehört der Termin samt Teilnehmern und
   Erinnerungen zur Datenfreigabe und erreicht Carl.
4. **Given** Anna verschiebt einen Termin aus „Familie“ in einen nicht geteilten
   Kalender, **When** die Änderung Carl erreicht, **Then** verschwindet der
   Termin aus Carls Kopie, und bei Anna bleibt er erhalten.
5. **Given** eine Datenfreigabe, **When** holzi Änderungspakete für sie
   verschickt, **Then** enthalten sie nur Wurzel und zugehörige Einträge aus
   den erklärten Tabellen und nie Geheimnisse der Vault.
6. **Given** die Erweiterung fragt nach dem Stand eines eigenen Eintrags,
   **When** holzi antwortet, **Then** erfährt sie, ob er geteilt ist, und sonst
   nichts: keine Schlüssel, keine Mitgliederliste, keine Rechte anderer.

---

### User Story 3 - Eine Freigabe empfangen und in der eigenen Erweiterung sehen (Priority: P1)

Carl erhält Annas Einladungslink zum Kalender „Familie“ und öffnet ihn. Er
sieht, von wem er kommt, welche Erweiterung sie braucht und welche Fähigkeiten er bekommt, und
nimmt an. Der Kalender erscheint in seiner eigenen Kalender-Erweiterung als
„geteilt von Anna“, auf seinem Laptop und seinem Telefon. Hätte er die
Erweiterung nicht installiert, böte ihm die Einladung die Installation an.

**Why this priority**: Ohne Empfangen ist Teilen wertlos. Dass die Daten in der
gewohnten Erweiterung auftauchen und nicht in einer eigenen Ansicht, macht sie
benutzbar.

**Independent Test**: Eine Einladung auf einer Empfänger-Vault mit zwei Geräten
annehmen: Die Daten erscheinen in der Erweiterung auf beiden Geräten mit dem
Hinweis auf die Herkunft. Eine zweite Einladung verwerfen: Die Vault enthält
danach nichts aus dieser Freigabe. Eine Einladung auf einer Vault ohne die
Erweiterung öffnen: Sie nennt die Erweiterung und bietet die Installation an.

**Acceptance Scenarios**:

1. **Given** eine Einladung, **When** Carl sie öffnet, **Then** zeigt holzi den
   Namen und die Vault-Identität des Eigentümers, Erweiterung, Freigabetyp,
   Beschriftung der Wurzel und die angebotenen Fähigkeiten, und bis zur Annahme
   wird nichts aus der Freigabe gespeichert.
2. **Given** Carl nimmt an, **When** die Daten eintreffen, **Then** liegen sie in
   seiner Kopie derselben Erweiterung, die sie als „geteilt von Anna“
   kennzeichnen kann.
3. **Given** Carl hat mehrere eigene Geräte, **When** eines von ihnen die
   Freigabe angenommen hat, **Then** erscheinen die Daten auch auf den anderen,
   ohne dass Anna etwas tun muss.
4. **Given** Carl hat die Erweiterung nicht installiert, **When** er die
   Einladung öffnet, **Then** nennt sie die benötigte Erweiterung und bietet die
   Installation mit der üblichen Bestätigung an; die Einladung bleibt offen, bis
   er annimmt oder verwirft.
5. **Given** Carl verwirft die Einladung, **When** das wirksam ist, **Then**
   enthält seine Vault nichts aus der Freigabe, und es wurde nichts an Anna
   geschickt; der Link bleibt bei ihr „offen“, bis er abläuft oder sie ihn
   zurückzieht.
6. **Given** ein Änderungspaket enthält einen Eintrag, der nicht von der Wurzel
   aus erreichbar ist, eine fremde Tabelle betrifft oder von einer Vault ohne
   passende Fähigkeit stammt, **When** es bei Carl eintrifft, **Then** wird es
   nicht angewendet.
7. **Given** Carl hat angenommen und kein Gerät von Anna ist online, **When**
   er die Einladung ansieht, **Then** steht sie auf „angenommen, wartet auf
   Admin“, Carl hat noch keinen Schlüssel, und das Relay gibt ihm keinen
   Zugriff auf das Postfach der Freigabe, bis ein Gerät von Anna die Annahme
   verarbeitet hat (FR-010, FR-019).

---

### User Story 4 - Mit Schreibrecht mitarbeiten (Priority: P2)

Anna teilt die Einkaufsliste mit Carl (Schreiben) und mit ihrer
Tochter Dana (nur Lesen). Carl trägt „Milch“ ein und hakt „Brot“ ab. Anna und
Dana sehen beides, auch wenn Anna gerade offline ist. Dana kann nichts
eintragen; ihre Erweiterung erhält eine Fehlermeldung, wenn sie es versucht.

**Why this priority**: Gemeinsames Bearbeiten ist der häufigste Grund zu teilen,
setzt aber das Lesen aus US2 und US3 voraus.

**Independent Test**: Eine Liste mit zwei Empfängern teilen, einer mit
Schreiben, einer ohne. Der mit Schreiben legt einen Eintrag an und ändert einen
fremden; beide Änderungen erreichen Eigentümer und anderen Empfänger. Der ohne
Schreiben versucht dasselbe; holzi lehnt es in seiner Vault ab, und nichts davon
verlässt sie.

**Acceptance Scenarios**:

1. **Given** Carl hat Schreiben, **When** er einen Eintrag zur geteilten Liste
   hinzufügt, **Then** gehört der Eintrag zur Freigabe, sein Ersteller ist
   Carls Vault-Identität, und er erreicht Anna und Dana.
2. **Given** Carl hat Schreiben, **When** er einen Eintrag ändert, den Anna
   angelegt hat, **Then** wird die Änderung bei allen angewendet, und der
   Ersteller bleibt Anna.
3. **Given** Anna ist offline, **When** Carl etwas ändert, **Then** erreicht die
   Änderung Dana trotzdem, und Anna erhält sie, sobald eines ihrer Geräte online
   ist.
4. **Given** Dana hat nur Lesen, **When** ihre Erweiterung einen Eintrag der
   geteilten Liste ändern oder einen neuen darunter anlegen will, **Then** lehnt
   holzi das in Danas Vault ab, und die Erweiterung erhält einen Fehler.
5. **Given** Carl hat Schreiben, aber nicht Löschen, **When** er einen eigenen
   Eintrag löscht, **Then** wird er bei allen gelöscht; **When** er einen von
   Anna angelegten Eintrag löschen will, **Then** lehnt holzi das ab.
6. **Given** jemand verändert den Ersteller eines Eintrags, **When** die
   Änderung eintrifft, **Then** lehnen alle Beteiligten sie ab.

---

### User Story 5 - Rechte ändern und entziehen (Priority: P2)

Anna und Ben arbeiten nicht mehr zusammen. Anna öffnet in den Einstellungen
unter „Föderation“ die Unteransicht „Datenfreigaben“ und entfernt Ben aus der
Freigabe des Termins. Carl setzt sie in der Einkaufsliste von Schreiben auf
Lesen zurück. Ben erhält danach nichts mehr; was er schon hatte, bleibt als
beendete Freigabe bei ihm.

**Why this priority**: Wer teilt, muss das Teilen zurücknehmen können. Ohne
Entzug ist jede Freigabe eine Entscheidung für immer.

**Independent Test**: Einen Empfänger entfernen, danach beim Eigentümer Einträge
ändern. Die Änderungen erreichen die übrigen Empfänger, der entfernte kann sie
weder abrufen noch entschlüsseln. Bei ihm steht die Freigabe als beendet. Einem
anderen Empfänger Schreiben entziehen: Seine eigene Vault lehnt danach jede
Änderung ab, und das Relay nimmt keine Pakete mehr von ihm an.

**Acceptance Scenarios**:

1. **Given** die Unteransicht „Datenfreigaben“, **When** Anna sie öffnet,
   **Then** sieht sie alle eigenen Freigaben mit Empfängern und Fähigkeiten und
   alle empfangenen mit Eigentümer.
2. **Given** Anna entfernt Ben, **When** die Änderung wirksam ist, **Then** gilt
   eine neue Mitgliederliste auf dem Relay, und eine neue Schlüsselgeneration
   geht nur an die verbliebenen Empfänger.
3. **Given** Ben ist entfernt, **When** Anna danach etwas ändert, **Then** kann
   Ben diese Änderung weder abrufen noch lesen.
4. **Given** Ben ist entfernt, **When** er seine Kalender-Erweiterung öffnet,
   **Then** sind die schon empfangenen Daten noch da, als „Freigabe beendet“
   gekennzeichnet, ohne weitere Aktualisierung, und er kann sie entfernen.
5. **Given** Anna setzt Carl auf Lesen zurück, **When** Carl danach etwas
   ändern will, **Then** lehnt seine eigene Vault das ab (FR-029); für Änderungen, die
   gleichzeitig mit der Rückstufung entstanden sind, gilt FR-034.
6. **Given** ein Empfänger will die Rechte eines anderen ändern oder jemanden
   einladen, **When** er es versucht, **Then** bietet holzi das nicht an, weil
   nur der Eigentümer Rechte vergibt.

---

### User Story 6 - Austreten und Erweiterungen entfernen (Priority: P3)

Dana braucht die Einkaufsliste nicht mehr und tritt aus der Freigabe aus. Die
Liste verschwindet aus ihrer Vault auf allen ihren Geräten, bei Anna bleibt sie
unberührt. Später deinstalliert Anna die Kalender-Erweiterung; holzi zeigt
vorher, welche Datenfreigaben dadurch enden.

**Why this priority**: Seltener als Teilen und Entziehen, aber ohne sauberes
Ende blieben Empfänger an Freigaben hängen, die sie nicht mehr wollen.

**Independent Test**: Als Empfänger austreten: Die Daten sind auf allen eigenen
Geräten weg, beim Eigentümer ist kein Eintrag gelöscht, und der Empfänger steht
nicht mehr in der Mitgliederliste, sobald ein Gerät des Eigentümers online war.
Beim Eigentümer die Erweiterung deinstallieren: Der Dialog listet die
Freigaben, danach sind sie für alle Empfänger beendet.

**Acceptance Scenarios**:

1. **Given** Dana tritt aus, **When** der Austritt wirksam ist, **Then** sind
   die empfangenen Einträge auf allen ihren Geräten entfernt, und keine dieser
   Entfernungen erreicht Anna oder Carl als Löschung.
2. **Given** Dana ist ausgetreten, **When** ein Gerät von Anna online ist,
   **Then** entfernt Annas Vault Dana aus der Freigabe mit neuer
   Schlüsselgeneration.
3. **Given** Anna deinstalliert eine Erweiterung mit eigenen Datenfreigaben,
   **When** sie die Deinstallation bestätigt, **Then** hat der Dialog vorher die
   betroffenen Freigaben genannt, und sie enden für alle Empfänger wie ein
   Entzug.
4. **Given** Carl deinstalliert die Erweiterung, zu der er Freigaben empfangen
   hat, **When** er bestätigt, **Then** hat der Dialog sie vorher genannt, und
   Carl tritt aus ihnen aus.

---

### User Story 7 - Ein Eintrag in zwei Freigaben (Priority: P3)

Anna teilt den Kalender „Familie“ mit Carl und den Termin „Elternabend“ daraus
zusätzlich einzeln mit der Nachbarin Eva, mit Schreiben. Eva ändert die Uhrzeit.
Carl sieht die neue Uhrzeit, sobald eines von Annas Geräten online war.

**Why this priority**: Die Überlappung folgt aus D4 und muss richtig
funktionieren, betrifft aber nur einen Teil der Fälle; die Verzögerung ist für
v1 hingenommen.

**Independent Test**: Einen Eintrag über seine Sammlung mit einer Vault und
einzeln mit einer zweiten teilen. Die zweite ändert ihn, während der Eigentümer
offline ist: Die erste sieht die Änderung noch nicht. Der Eigentümer kommt
online: Die erste sieht sie als Änderung des Eigentümers, mit der zweiten als
angezeigter ursprünglicher Autorin.

**Acceptance Scenarios**:

1. **Given** der Termin gehört zu beiden Freigaben, **When** Eva ihn über ihre
   Freigabe ändert, **Then** wird die Änderung gegen Evas Freigabe geprüft.
2. **Given** Anna ist offline, **When** Evas Änderung eintrifft, **Then**
   erreicht sie Carl noch nicht.
3. **Given** ein Gerät von Anna ist online und hat Evas Änderung, **When** es
   sie als neue, von diesem Gerät unterschriebene Änderung in die Freigabe
   „Familie“ weitergibt, **Then** nimmt Carl sie an, obwohl Eva dort kein
   Mitglied ist, und Eva wird als ursprüngliche Autorin angezeigt.
4. **Given** ein Empfänger ist in beiden Freigaben, **When** er aus einer
   austritt, **Then** bleibt der Termin bei ihm, solange er über die andere
   dazugehört.

---

### Edge Cases

- Ein Empfänger mit Schreiben legt unter einem geteilten Termin eine Erinnerung
  an. Sie gehört damit zur Freigabe und erreicht alle Beteiligten. Private
  Einträge unter einer geteilten Wurzel gibt es nicht; die Erweiterung sollte
  das anzeigen können (FR-018).
- Ein Empfänger mit Schreiben hängt einen Eintrag an eine Wurzel außerhalb der
  Freigabe um (verschiebt ihn in einen eigenen Kalender). Die Änderung wäre
  danach nicht mehr von der Wurzel aus erreichbar; holzi lehnt sie schon in
  seiner Vault ab (FR-029). Kopieren bleibt möglich.
- Der Eigentümer löscht die Wurzel, oder ein Empfänger mit dem Recht dazu löscht
  sie. Die Löschung erreicht alle Beteiligten, und die Datenfreigabe endet für
  alle.
- Eine Tabelle wird durch ein Update erst teilbar und enthält schon Einträge.
  Ihr Ersteller ist die eigene Vault-Identität, denn empfangen kann die Vault
  vorher nichts aus ihr haben.
- Ein Update der Erweiterung ändert, welche Tabellen zu einem Freigabetyp
  gehören. Die Zugehörigkeit wird mit der neuen Erklärung berechnet; Einträge,
  die nicht mehr dazugehören, verschwinden bei den Empfängern wie verschobene
  Einträge. Ein Update, das einen Freigabetyp mit bestehenden Freigaben
  entfernt, nennt diese in der Bestätigung, und sie enden danach (FR-006).
- Der Empfänger hat eine ältere Version der Erweiterung, der eine Tabelle oder
  Spalte fehlt, die im Paket vorkommt. Das Paket wird zurückgehalten, nicht
  verworfen, und die Freigabe zeigt den Hinweis, die Erweiterung zu
  aktualisieren (FR-025).
- Eine gleichnamige Erweiterung eines anderen Herausgebers (anderer
  öffentlicher Schlüssel) ist installiert. Sie gilt nicht als dieselbe
  Erweiterung und empfängt nichts.
- Zwei Geräte des Eigentümers ändern gleichzeitig die Rechte desselben
  Empfängers. Es gilt für alle dieselbe zuletzt geschriebene Änderung, und ein
  Gerät des Eigentümers unterschreibt die zusammengeführte Mitgliederliste mit
  einer höheren Generation neu (FR-016); da nur der Eigentümer Rechte schreibt,
  gibt es keinen Streit zwischen Admins.
- Die Vault des Eigentümers geht vollständig verloren (kein Gerät, keine Kopie
  und kein Wiederherstellungspaket mehr, Spec 026). Die Datenfreigabe ist
  eingefroren: Empfänger behalten ihre Daten und können mit ihren Rechten
  weiter untereinander arbeiten, aber niemand kann Rechte ändern oder
  Überlappungen weitergeben. Die Eigentümerschaft lässt sich nicht übertragen
  (D23).
- Ein Gerät des Eigentümers geht verloren oder wird gestohlen. Solange die
  Vault eine weitere Kopie oder ein Postfach am Relay hat und die Passphrase
  hält, ist das kein Problem für die Datenfreigabe (D26). Wer das Gerät
  entsperrt, kann bis zu seinem Entfernen für den Eigentümer handeln, weil
  jedes Gerät der Eigentümer-Vault die Datenfreigabe verwalten darf (D29).
  Entfernt ein Hauptgerät es aus der Geräteliste (Spec 024), weisen Relay und
  Empfänger seine Unterschriften und Verbindungen ab, und ein verbliebenes
  Gerät des Eigentümers unterschreibt die aktuelle Mitgliederliste neu
  (FR-016). Dieses Risiko wird für v1 hingenommen.
- Ein Gerät eines Empfängers wird entfernt. Sobald die neue Geräteliste der
  Empfänger-Vault bekannt ist, lehnen Relay und alle Beteiligten seine
  Änderungen ab (FR-023), und spätere Schlüsselgenerationen werden nicht mehr
  an es verschlüsselt (FR-043). Was es schon hat, bleibt bei ihm; ein Löschen
  aus der Ferne gibt es nicht (Spec 024).
- Ein Empfänger verknüpft ein neues Gerät. Es erhält die empfangenen Einträge
  und die bisherigen Inhaltsschlüssel über den Sync der eigenen Vault (FR-022);
  spätere Generationen verschlüsselt ein Gerät des Eigentümers direkt an es,
  sobald es die neue Geräteliste kennt (FR-043). Der Eigentümer tut dafür
  nichts, erfährt aber, wie viele Geräte die Empfänger-Vault hat (D28).
- Ein entfernter Empfänger schreibt weiter mit der älteren Schlüsselgeneration,
  die er noch hat. Das Relay lehnt das ab (Spec 026). Was direkte oder
  anderweitig eintreffende Pakete angeht, siehe FR-034.
- Eine Einladung trifft ein, während die Erweiterung gerade deinstalliert wird,
  oder für einen Freigabetyp, den die installierte Version nicht kennt. Sie
  bleibt offen wie bei fehlender Erweiterung (FR-020).
- Der Eigentümer teilt einen Eintrag mit sich selbst. holzi bietet das nicht an;
  die eigenen Geräte teilen die ganze Vault ohnehin (Spec 024).

## Requirements _(mandatory)_

### Functional Requirements

**Deklaration und Prüfung**

- **FR-001**: Eine Erweiterung MUSS in ihrem signierten Manifest Freigabetypen
  erklären können, für alle Erweiterungen im selben Schema. Jeder Freigabetyp
  nennt: einen Namen, eindeutig innerhalb der Erweiterung; die Wurzeltabelle mit
  ihrem Schlüssel; das Feld, das im Teilen-Dialog als Beschriftung dient; die
  zugehörigen Tabellen, jede mit der Fremdschlüsselspalte, die auf die
  übergeordnete Tabelle zeigt; optional je zugehöriger Tabelle den Freigabetyp
  derselben Erweiterung, dessen zugehörige Einträge mitkommen (die Termine eines
  Kalenders bringen ihre Teilnehmer und Erinnerungen mit). Tabellennamen sind
  logisch; holzi ordnet sie den Tabellen mit dem Präfix der Erweiterung zu. Das
  konkrete Format im Manifest legt der Plan fest.
- **FR-002**: holzi MUSS die Erklärung bei jeder Installation und jedem Update
  prüfen, bevor die Erweiterung verwendet werden kann, gegen das Schema, das die
  Erweiterung danach hat. holzi MUSS die ganze Erweiterung ablehnen, wenn:
  (a) eine Wurzel- oder zugehörige Tabelle außerhalb des eigenen Präfixes
  liegt; (b) ein Fremdschlüssel auf eine Tabelle außerhalb des eigenen Präfixes
  zeigt; (c) eine genannte Tabelle nicht synchronisiert wird oder als nur
  gerätelokal gekennzeichnet ist, oder eine Schlüssel-, Fremdschlüssel- oder
  Beschriftungsspalte nicht synchronisiert wird; (d) eine genannte Tabelle oder
  Spalte nicht existiert oder ein Fremdschlüssel nicht auf die übergeordnete
  Tabelle zeigt; (e) ein Name doppelt vorkommt, ein Verweis auf einen
  unbekannten Freigabetyp zeigt oder die Verweise zwischen Freigabetypen einen
  Kreis bilden.
- **FR-003**: Eine Ablehnung MUSS Erweiterung, betroffene Tabelle oder
  Verbindung und Grund nennen. Von einer abgelehnten Installation DARF NICHTS
  installiert bleiben. Bei einem abgelehnten Update MUSS die installierte
  Version mit ihren Daten und Datenfreigaben unverändert bleiben.
- **FR-004**: Der Bestätigungsdialog bei Installation und Update (ADR-0004) MUSS
  die erklärten Freigabetypen mit Namen zeigen. Ein Update, das Freigabetypen
  hinzufügt, braucht eine neue Bestätigung wie jede neue Berechtigung.
- **FR-005**: Für jede Tabelle, die in einem Freigabetyp vorkommt, MUSS holzi
  beim Anlegen eines Eintrags den Ersteller auf die Vault-Identität der
  schreibenden Vault setzen. Die Erweiterung DARF den Ersteller weder setzen
  noch ändern. Einträge, die schon bestehen, wenn eine Tabelle teilbar wird,
  erhalten die eigene Vault-Identität.
- **FR-006**: Entfernt ein Update einen Freigabetyp, zu dem Datenfreigaben
  bestehen, MUSS die Bestätigung des Updates diese nennen. Nach der Bestätigung
  enden sie wie in FR-036 (eigene) beziehungsweise FR-026 (empfangene).

**Teilen**

- **FR-007**: Den Teilen-Dialog MUSS holzi selbst anzeigen. Eine Erweiterung
  DARF ihn nur anfragen, für einen Eintrag der Wurzeltabelle eines ihrer
  Freigabetypen. Eine Datenfreigabe entsteht oder ändert sich nur durch eine
  Bestätigung des Nutzers in diesem Dialog oder in der Unteransicht
  „Datenfreigaben“ (FR-015).
- **FR-008**: Der Teilen-Dialog MUSS zeigen: die Beschriftung der Wurzel, den
  Freigabetyp, die Erweiterung, was mitgeteilt wird (bei einer Sammlung: „mit
  allen Einträgen“) und die bisherigen Empfänger mit ihren Fähigkeiten. Er MUSS
  erlauben, einen Empfänger durch einen Einladungslink einzuladen (Spec 027
  FR-008) oder aus dem Adressbuch (Spec 051 FR-033) und für jeden Empfänger genau eine Fähigkeitsstufe zu wählen: Lesen, Schreiben (umfasst
  Lesen) oder Löschen (umfasst Schreiben). Freie Kombinationen gibt es nicht.
  Admin DARF NICHT wählbar sein.
- **FR-009**: Rechte MÜSSEN an Vault-Identitäten gehen, nie an Geräte. Der
  Empfänger kann Geräte hinzufügen, ohne dass der Eigentümer etwas tut. Weil
  Inhaltsschlüssel je Gerät verschlüsselt werden (FR-043), erfährt der
  Eigentümer, wie viele Geräte ein Empfänger hat; das wird hingenommen (D28).
- **FR-010**: Bestätigt der Eigentümer im Teilen-Dialog einen neuen Empfänger,
  MUSS holzi: die Datenfreigabe anlegen, falls es sie noch nicht gibt; die
  Einladung mit Namen, Fähigkeitsstufe, Einmalgeheimnis, Ablaufzeit und Stand
  „offen“ speichern; und den Einladungslink ohne Schlüssel zeigen, den der
  Eigentümer selbst weitergibt (Spec 027 FR-008). Kein Recht und kein
  Eintrag in der Mitgliederliste entstehen vorher; ein Eingeladener erhält
  keinen verschlüsselten Inhaltsschlüssel, und das Relay gibt ihm keinen
  Zugriff. Erst wenn ein Gerät des Eigentümers die unterschriebene Annahme
  (FR-019) mit gültigem, offenem Geheimnis erhalten hat und der Eigentümer sie
  nach Prüfung der Vault-Identität bestätigt hat (Spec 027 FR-009), MUSS es den
  Empfänger aufnehmen: das Recht mit der Vault-Identität des Empfängers
  speichern, eine neue Schlüsselgeneration anlegen, deren Inhaltsschlüssel
  verschlüsselt an jedes Gerät der Vault des Eigentümers und jedes Empfängers
  der neuen Mitgliederliste geht (FR-043), dem neuen Empfänger zusätzlich die
  älteren Generationen geben (FR-011) und die neue Mitgliederliste mit dieser
  Generation auf das Relay laden (Spec 026). Wie bei Spaces (Spec 027) erzeugt
  jede Änderung der Mitglieder (Aufnahme nach bestätigter Annahme, Ändern von
  Fähigkeiten, Entfernen, Austreten) eine neue Schlüsselgeneration mit fester
  Mitgliederliste; Einladen, Verwerfen und Ablehnen der Annahme ändern die
  Mitglieder nicht.
- **FR-011**: Ein neu hinzugefügter Empfänger MUSS nach der Annahme den ganzen
  aktuellen Stand der Datenfreigabe erhalten, nicht nur die Änderungen nach
  seiner Einladung. Dafür MUSS holzi ihm zusätzlich die Inhaltsschlüssel aller
  älteren Schlüsselgenerationen verschlüsselt an jedes seiner Geräte geben
  (FR-043),
  sodass er auch Änderungen von vor seiner Einladung lesen kann.
- **FR-012**: Zu einer Datenfreigabe MÜSSEN genau die Wurzel und alle
  zugehörigen Einträge gehören, berechnet aus den erklärten Fremdschlüsseln.
  Neue Einträge unter der Wurzel gehören sofort dazu. Ein Eintrag, der durch
  eine Änderung des Eigentümers nicht mehr erreichbar ist, MUSS bei den
  Empfängern aus deren Kopie verschwinden, ohne beim Eigentümer gelöscht zu
  werden; ein Eintrag, der erreichbar wird, gehört ab dann dazu. Dieses
  Verlassen des Umfangs darf allein der Eigentümer auslösen: Die Empfänger
  entfernen ihre Kopie, ohne eine Löschung zurückzuschicken, und die Zeile des
  Eigentümers bleibt; gehört der Eintrag beim Empfänger noch zu einer anderen
  empfangenen Freigabe, bleibt er (FR-042).
- **FR-013**: Beim Senden für eine Datenfreigabe DÜRFEN nur Einträge aus FR-012,
  nur aus den erklärten Tabellen und nur synchronisierte Spalten die Vault
  verlassen. Die Vault-Identität, Geräteschlüssel, Inhaltsschlüssel anderer
  Bereiche und alle Tabellen des Kerns DÜRFEN eine Datenfreigabe NIE
  erreichen. Diese Grenze MUSS als feste Liste des Erlaubten umgesetzt sein,
  nicht als Liste des Verbotenen.
- **FR-014**: Nur der Eigentümer DARF Empfänger hinzufügen, Fähigkeiten ändern
  und Empfänger entfernen (D6). Für ihn handeln darf jedes Gerät auf der
  aktuellen Geräteliste seiner Vault, auch ein verknüpftes Gerät; es
  unterschreibt mit seinem Geräteschlüssel (D29). Für Einträge, die zu einer empfangenen
  Datenfreigabe gehören, DARF holzi keinen Teilen-Dialog öffnen; eine solche
  Anfrage der Erweiterung wird mit einem Hinweis abgelehnt (D7). Das gilt auch
  für Einträge, die der Empfänger selbst in der Freigabe angelegt hat.
- **FR-015**: holzi MUSS in der Einstellungskategorie „Föderation“ (Spec 023)
  eine Unteransicht „Datenfreigaben“ anbieten, die alle Datenfreigaben zeigt:
  die eigenen mit Empfängern, Fähigkeiten und dem Stand jeder Einladung
  (offen, angenommen, abgelehnt, abgelaufen, zurückgezogen), die empfangenen mit Eigentümer und eigenen
  Fähigkeiten. Aus ihr MÜSSEN sich Rechte ändern, Empfänger entfernen und eigene
  Freigaben beenden lassen. Der Teilen-Dialog (FR-007, FR-008) bleibt ein
  allgemeiner Dialog des Kerns, den die Erweiterung öffnet; er gehört nicht zu
  dieser Unteransicht.
- **FR-016**: Der Eigentümer MUSS Rechte von jedem seiner Geräte aus ändern
  können. Gleichzeitige Änderungen seiner Geräte MÜSSEN sich auf allen
  Beteiligten gleich auflösen, zugunsten der zuletzt geschriebenen. Für die
  Mitgliederliste MÜSSEN dieselben Regeln gelten wie bei Spaces (Spec 027):
  (a) die Vault des Eigentümers ist am Relay für den Bereich der Datenfreigabe
  berechtigt, mit allen ihren Geräten, ohne selbst als Empfänger in der Liste
  zu stehen; (b) haben Geräte des Eigentümers unabhängig voneinander neue
  Mitgliederlisten erzeugt, MUSS ein Gerät des Eigentümers die
  zusammengeführte Liste mit einer höheren Generation als jede der
  zusammengeführten neu unterschreiben und hochladen, und alle gleichzeitig
  entstandenen Schlüsselgenerationen bleiben gültig; (c) zum Verschlüsseln
  DARF ein Gerät nur eine Schlüsselgeneration verwenden, deren
  Inhaltsschlüssel an keine inzwischen entfernte Vault verschlüsselt ist; gibt
  es keine, MUSS ein Gerät des Eigentümers eine neue anlegen; (d) nur Geräte
  auf der aktuellen Geräteliste des Eigentümers veröffentlichen
  Mitgliederlisten (D29); entfernt ein Hauptgerät des Eigentümers ein Gerät, das die
  aktuelle Mitgliederliste unterschrieben hat, MUSS dieses Hauptgerät sie im
  selben Vorgang neu unterschreiben und hochladen (Spec 024 FR-026); zwei Geräte des Eigentümers können
  verschiedene Listen mit derselben Generation veröffentlichen. Unter gültigen
  Listen derselben Generation MUSS beim Relay und bei jedem Empfänger die mit
  dem lexikografisch kleinsten Hash der Liste gelten; das Relay ersetzt eine
  gespeicherte Liste derselben Generation nur durch eine mit kleinerem Hash
  (Spec 026). Ein Gerät des Eigentümers, das zwei Listen derselben Generation
  sieht, MUSS nach (b) die Generation + 1 veröffentlichen, die beide
  Änderungen zusammenführt.
- **FR-017**: Die Erweiterung DARF nie Inhaltsschlüssel, Mitgliederlisten,
  Rechte anderer Empfänger oder Zugriff auf die Synchronisierung erhalten.
- **FR-018**: Die Erweiterung MUSS für Einträge ihrer Wurzeltabellen und deren
  zugehörige Einträge abfragen können, ob sie geteilt sind, ob sie aus einer
  empfangenen Freigabe stammen, von wem (Name und Vault-Identität des
  Eigentümers), mit welchen eigenen Fähigkeiten und ob die Freigabe beendet
  ist, damit sie das anzeigen kann. Mehr gibt die Abfrage nicht preis.

**Empfangen**

- **FR-019**: Das Öffnen eines Einladungslinks MUSS zuerst dessen Signatur nach
  Spec 027 FR-008 prüfen und erst danach eine Vorschau zeigen: Name und
  Vault-Identität des Eigentümers, Erweiterung, Freigabetyp, Beschriftung der
  Wurzel und angebotene Fähigkeiten; dabei geht nichts an den Eigentümer.
  Einen gefälschten oder veränderten Link MUSS holzi lokal abweisen und darf
  keine Annahme senden.
  Der Empfänger MUSS annehmen oder verwerfen können. Die Annahme ist eine
  Antwort an den Eigentümer mit dem Einmalgeheimnis, unterschrieben vom
  Geräteschlüssel des annehmenden Geräts, gültig, wenn es auf der aktuellen
  Geräteliste der Empfänger-Vault steht; sie bringt diese Geräteliste mit. Bis
  der Eigentümer sie nach FR-010 bestätigt hat, MUSS die Einladung beim
  Empfänger „angenommen, wartet auf Admin“ zeigen; er hat bis dahin keinen
  Schlüssel und keinen Zugriff am Relay. Vor der Aufnahme DARF nichts aus der
  Freigabe in seiner Vault gespeichert werden außer der Einladung selbst. Ein
  Verwerfen hinterlässt beim Eingeladenen keinen Schlüssel, weil er nie einen
  erhalten hat. Ein Gerät des Eigentümers MUSS Nachrichten ohne gültiges,
  offenes Geheimnis still verwerfen (Spec 027 FR-041).
- **FR-020**: Ist die benötigte Erweiterung nicht installiert, oder kennt die
  installierte Version den Freigabetyp nicht, MUSS die Einladung das nennen und
  die Installation beziehungsweise das Update über die übliche Bestätigung
  anbieten. Die Einladung MUSS offen bleiben, bis der Empfänger annimmt oder
  verwirft; mit Ablauf oder Zurückziehen des Links endet sie ebenfalls. Als
  dieselbe Erweiterung gilt nur eine mit gleichem öffentlichem Schlüssel und
  Namen.
- **FR-021**: Nach der Annahme MÜSSEN die Daten in den Tabellen derselben
  Erweiterung in der Vault des Empfängers landen. holzi MUSS festhalten, welche
  Einträge zu welcher empfangenen Datenfreigabe gehören.
- **FR-022**: Empfangene Daten und die empfangenen Inhaltsschlüssel MÜSSEN auf
  alle Geräte des Empfängers gelangen, über den Sync zwischen eigenen Geräten
  (Spec 024). Das Gerät, das einen an seinen Geräteschlüssel verschlüsselten
  Inhaltsschlüssel entpackt, MUSS ihn in den Daten der eigenen Vault ablegen
  (D28). Entpackte Inhaltsschlüssel sind gewöhnliche Vault-Daten, keine
  Nur-direkt-Daten: Sie reisen im Bereich „Vault“ auch über das Postfach der
  Vault und sind wiederherstellbar (D30). Einen Bereich außer dem Bereich
  „Vault“ DÜRFEN sie nie erreichen (FR-013).
- **FR-023**: Jede Vault, die Änderungen einer Datenfreigabe empfängt
  (Eigentümer wie Empfänger), MUSS eine Änderung nur zulassen, wenn: (a) ihre
  Unterschrift gültig ist und das unterschreibende Gerät auf der aktuellen
  Geräteliste der Vault des Autors steht; (b) ihre Tabelle zu den erklärten
  Tabellen des Freigabetyps derselben Erweiterung gehört; (c) bei Anlegen und
  Ändern der Eintrag nach der Änderung von der Wurzel aus erreichbar ist, ein
  neuer Eintrag also einen Fremdschlüssel in die Freigabe trägt; Löschungen
  prüft (c) nicht, sie werden allein nach den Fähigkeitsregeln aus FR-028
  geprüft, und eine Änderung des Eigentümers, die einen Eintrag aus dem Umfang
  nimmt, gilt als Verlassen des Umfangs nach FR-012; (d) der Autor die nötige Fähigkeit hat
  (FR-027, FR-028, FR-034) oder der Eigentümer ist; (e) der Ersteller eines
  bestehenden Eintrags unverändert bleibt und der eines neuen Eintrags der
  Autor ist. Ausgenommen von (e) sind Änderungen, die der Eigentümer nach
  FR-041 neu ausstellt: Dort bleibt der Ersteller der ursprüngliche, und
  unterschreibender Autor ist der Eigentümer.
- **FR-024**: Ein Änderungspaket ist atomar (Spec 024): Enthält es eine
  Änderung, die FR-023 verletzt, DARF KEINE Änderung aus diesem Paket
  angewendet werden. Eine Momentaufnahme wird dagegen je vollständiger
  Transaktionsgruppe geprüft (Spec 024): Eine unzulässige Änderung verwirft
  ihre ganze Gruppe, die übrigen Gruppen werden angewendet; eine Gruppe wird
  nie teilweise angewendet. holzi MUSS
  jede Ablehnung protokollieren und mit den folgenden Paketen weitermachen.
- **FR-025**: Enthält ein Paket Tabellen oder Spalten des Freigabetyps, die die
  installierte Version der Erweiterung noch nicht kennt, MUSS holzi es
  zurückhalten statt es zu verwerfen, und die Freigabe MUSS auf ein Update der
  Erweiterung hinweisen. Nach dem Update wird es angewendet.
- **FR-026**: Der Empfänger MUSS eine Einladung verwerfen und aus einer
  angenommenen Freigabe jederzeit austreten können. Beim Austritt MÜSSEN die
  Einträge, die nur über diese Freigabe in seiner Vault sind, auf allen seinen
  Geräten entfernt werden. Diese Entfernung DARF die Datenfreigabe NICHT als
  Löschung erreichen. Der Austritt MUSS dem Eigentümer mitgeteilt werden, ein Verwerfen
  nicht (Spec 027 FR-012). Beim Verwerfen entfernt seine Vault die offene
  Einladung ohne neue Schlüsselgeneration, weil der Eingeladene nie Mitglied
  war (FR-010); beim
  Austritt entfernt sie das Recht, sobald eines seiner Geräte davon erfährt,
  wie bei einem Entzug (FR-032).

**Bearbeiten**

- **FR-027**: Schreiben MUSS erlauben, neue Einträge innerhalb der Freigabe
  anzulegen und jeden Eintrag der Freigabe zu ändern (einen Artikel zur
  geteilten Einkaufsliste hinzufügen, einen abhaken).
- **FR-028**: Löschen: Schreiben MUSS das Löschen eigener Einträge (Ersteller
  ist die eigene Vault-Identität) erlauben; fremde Einträge zu löschen MUSS die
  Fähigkeit Löschen erfordern. Der Eigentümer darf alles löschen.
- **FR-029**: holzi MUSS in der Vault des Empfängers jede Änderung an Einträgen
  einer empfangenen Freigabe ablehnen, die die eigenen Fähigkeiten nicht
  erlauben, bevor sie gespeichert wird: Ändern und Anlegen ohne Schreiben,
  Löschen nach FR-028, Umhängen eines Eintrags aus der Freigabe heraus. Die
  Erweiterung MUSS dafür einen Fehler erhalten.
- **FR-030**: Änderungen eines Empfängers MÜSSEN den Eigentümer und alle
  anderen Empfänger erreichen, auch wenn der Eigentümer offline ist (über das
  Postfach, Spec 026). Sind Geräte verschiedener Beteiligter gleichzeitig
  online, DÜRFEN sie die Änderungen dieser Datenfreigabe auch direkt
  austauschen (Spec 027, direkte Verbindung zwischen Mitgliedern): Jedes Gerät
  weist sich mit seinem Geräteschlüssel aus, der auf der aktuellen Geräteliste
  einer Vault der Mitgliederliste oder des Eigentümers stehen MUSS.
- **FR-031**: Gleichzeitige Änderungen desselben Feldes MÜSSEN sich bei allen
  Beteiligten gleich auflösen, nach denselben Regeln wie der Sync zwischen
  eigenen Geräten. Konfliktkopien gibt es für Daten nicht.

**Entzug**

- **FR-032**: Der Eigentümer MUSS Fähigkeiten eines Empfängers jederzeit ändern
  und Empfänger entfernen können. Dabei MUSS holzi eine neue Mitgliederliste auf
  das Relay laden, sodass es den Empfänger sofort nach den neuen Rechten
  behandelt, und eine neue Schlüsselgeneration anlegen, die nur an die
  verbliebenen Empfänger geht, an jedes ihrer Geräte (wie bei jeder Änderung
  der Mitglieder, FR-010, FR-043).
- **FR-033**: Nach einem Entfernen DARF der entfernte Empfänger keine Änderung
  mehr lesen können, die mit einer späteren Schlüsselgeneration verschlüsselt
  ist.
- **FR-034**: Ein Entzug (Entfernen oder Zurückstufen) wirkt nur nach vorn und
  wird über die Grenze aus Spec 024 abgegrenzt, nicht über Zeitstempel: Die
  neue Mitgliederliste trägt für jedes Gerät der betroffenen Vault die höchste
  lückenlose Nummer, die das erstellende Gerät des Eigentümers da angewendet
  hatte. Jedes Gerät, das diese Liste kennt, MUSS jede Änderung der
  betroffenen Vault jenseits der Grenze ablehnen, die das entzogene Recht
  braucht, gleich welchen Zeitstempel und welche Schlüsselgeneration sie
  trägt; Änderungen bis zur Grenze prüft es nach den vorherigen Fähigkeiten.
  Zurückdatieren ist unmöglich, weil die Nummern bis zur Grenze schon vergeben
  sind (Spec 024). Hat ein Gerät eine solche Änderung angewendet, bevor es die
  Liste kannte, bleibt sie (das akzeptierte Zeitfenster); die nächste
  berechtigte Änderung desselben Feldes behebt die Abweichung. Das Relay
  sperrt den Betroffenen sofort (Spec 026). Diese Regel präzisiert die Klärung
  vom 2026-09-28 zum Entzug: Die Prüfung gegen die neueste bekannte
  Mitgliederliste ist die Prüfung gegen deren Grenze.
- **FR-035**: Was ein entfernter Empfänger schon empfangen hat, MUSS in seiner
  Vault bleiben, gekennzeichnet als „Freigabe beendet“ und ohne weitere
  Aktualisierung. Er MUSS es in einem Schritt entfernen können; wie beim
  Austritt erreicht diese Entfernung niemanden sonst.
- **FR-036**: Der Eigentümer MUSS eine ganze Datenfreigabe beenden können. Für
  jeden Empfänger gilt dann FR-035. Dasselbe gilt, wenn die Wurzel gelöscht
  wird.
- **FR-037**: Die Deinstallation einer Erweiterung MUSS vor der Bestätigung die
  betroffenen Datenfreigaben nennen. Beim Eigentümer enden seine Freigaben wie
  in FR-036, beim Empfänger tritt er aus den empfangenen aus wie in FR-026.

**Überlappung**

- **FR-038**: Ein Eintrag MUSS zu mehreren Datenfreigaben desselben Eigentümers
  gehören können (ein Termin einzeln und über seinen Kalender). Jede Freigabe
  hat eigene Empfänger, Rechte und Schlüssel.
- **FR-039**: Eine Änderung MUSS gegen die Freigabe geprüft werden, über die sie
  eingetroffen ist.
- **FR-040**: Die Vault des Eigentümers MUSS eine zugelassene Änderung an einem
  Eintrag, der auch zu anderen seiner Freigaben gehört, in diese weitergeben,
  sobald eines ihrer Geräte online ist und die Änderung hat. Bis dahin sehen die
  Empfänger der anderen Freigaben sie nicht; diese Verzögerung ist für v1
  hingenommen.
- **FR-041**: Die Vault des Eigentümers MUSS eine weitergegebene Änderung als
  neue Änderung im Bereich der anderen Datenfreigabe ausstellen, unterschrieben
  von einem Gerät des Eigentümers. Die Empfänger der anderen Freigabe MÜSSEN
  sie wie jede Änderung des Eigentümers prüfen und zulassen. Der ursprüngliche
  Autor MUSS als Anzeige erkennbar bleiben, aber nicht als unterschreibender
  Autor; der Ersteller des Eintrags bleibt unverändert (FR-023 (e)). Das ist
  die ausdrückliche Ausnahme zur Regel aus Spec 024, dass ein Weitergebender
  Autor und Unterschrift nie ändert.
- **FR-042**: Gehört ein Eintrag beim Empfänger zu mehreren empfangenen
  Freigaben, MUSS er nur einmal in dessen Vault liegen. Endet eine dieser
  Freigaben, bleibt er, solange eine andere ihn umfasst.

**Schlüssel je Gerät**

- **FR-043** (Schlüssel je Gerät der Empfänger-Vault): Ein Gerät des
  Eigentümers MUSS den Inhaltsschlüssel jeder Schlüsselgeneration mit NIP-44 an
  jedes Gerät verschlüsseln, das auf der aktuellen Geräteliste der Vault des
  Eigentümers oder einer Vault der zugehörigen Mitgliederliste steht, an
  dessen Geräteschlüssel (D28). Gerätelisten der Empfänger-Vaults erfährt es
  aus der Annahme (FR-019), über das Relay (Spec 026) oder über direkte
  Verbindungen; eine neuere Geräteliste ersetzt eine ältere nach den Regeln von
  Spec 024. Ein später hinzugekommenes Gerät erhält die bestehenden
  Generationen über den Sync der eigenen Vault (FR-022); spätere Generationen
  verschlüsselt ein Gerät des Eigentümers direkt an dieses Gerät. Ein Gerät,
  das nicht mehr auf der Geräteliste seiner Vault steht, DARF keine
  Inhaltsschlüssel späterer Generationen erhalten.

### Key Entities

- **Freigabetyp-Erklärung**: Teil des signierten Manifests einer Erweiterung.
  Name, Wurzeltabelle mit Schlüssel, Beschriftungsfeld, zugehörige Tabellen mit
  Fremdschlüssel, optionale Verweise auf andere Freigabetypen derselben
  Erweiterung. Bei Installation und Update geprüft (FR-002).
- **Datenfreigabe**: Kennung, Eigentümer (Vault-Identität), Freigabetyp mit
  Erweiterung, Wurzel, Zeitpunkt der Anlage, Zustand (aktiv oder beendet,
  FR-036). Gehört der Vault des Eigentümers und
  synchronisiert sich zwischen seinen Geräten.
- **Recht**: Datenfreigabe, Vault-Identität des Empfängers, Fähigkeiten (Lesen,
  Schreiben, Löschen). Entsteht erst mit der Aufnahme nach bestätigter
  Annahme; nur vom Eigentümer geschrieben.
- **Schlüsselgeneration**: je Datenfreigabe; Inhaltsschlüssel verschlüsselt an
  jedes Gerät laut Geräteliste jeder berechtigten Vault (FR-043). Eine neue entsteht bei
  jeder Änderung der Mitglieder (Aufnahme nach bestätigter Annahme, Ändern
  von Fähigkeiten, Entfernen, Austreten); ihre Mitgliederliste steht damit
  fest. Neue Empfänger
  erhalten auch die Inhaltsschlüssel aller älteren Generationen (FR-011).
- **Mitgliederliste**: die von einem Gerät des Eigentümers unterschriebene Liste der Empfänger
  mit Fähigkeiten zu einer Generation, auf dem Relay (Spec 026). Die Vault des
  Eigentümers steht nicht darin und ist am Relay dennoch mit allen ihren
  Geräten berechtigt (FR-016).
- **Einladung**: ein vom Eigentümer erzeugter, nach Spec 027 FR-008 signierter
  Einladungslink mit Datenfreigabe, Eigentümer, Erweiterung, Freigabetyp,
  Beschriftung, Fähigkeiten, Einmalgeheimnis und Ablaufzeit, ohne Schlüssel.
  Der Eigentümer speichert sie mit Namen und Stand (offen, angenommen, abgelehnt, abgelaufen,
  zurückgezogen). Die Annahme ist eine vom Empfänger unterschriebene Antwort,
  die der Eigentümer bestätigt (FR-010, FR-019).
- **Zuordnung empfangener Einträge**: beim Empfänger; welcher Eintrag in den
  Tabellen einer Erweiterung zu welcher empfangenen Datenfreigabe gehört. Vom
  Kern geführt, nicht von der Erweiterung.
- **Ersteller**: an jedem Eintrag einer teilbaren Tabelle; unveränderlich
  (FR-005, FR-023).

## Success Criteria _(mandatory)_

### Measurable Outcomes

- **SC-001**: Eine Erweiterung, deren Erklärung eine fremde Tabelle, einen
  Fremdschlüssel aus dem eigenen Präfix hinaus oder eine nicht synchronisierte
  Tabelle nennt, wird bei Installation und Update in 100 % der Testfälle
  abgelehnt, und die vorher installierte Version bleibt in 100 % der Fälle
  unverändert.
- **SC-002**: In Tests mit mehreren Datenfreigaben erreicht kein einziger
  Eintrag außerhalb von Wurzel und zugehörigen Einträgen einen Empfänger,
  geprüft in dessen Vault und im entschlüsselten Inhalt der Änderungspakete.
- **SC-003**: Kein Änderungspaket einer Datenfreigabe enthält je ein Geheimnis
  der Vault (Vault-Identität, Geräteschlüssel, Inhaltsschlüssel anderer
  Bereiche) oder Daten einer Tabelle des Kerns.
- **SC-004**: 100 % der eingeschleusten unzulässigen Änderungen (ohne Fähigkeit,
  mit geändertem Ersteller, in fremder Tabelle, nicht erreichbar, falsche
  Unterschrift) werden von jeder empfangenden Vault abgelehnt. Für Änderungen,
  die erst durch einen Entzug unzulässig werden, gilt SC-010.
- **SC-005**: Ein Nutzer teilt einen Eintrag mit einer Person in weniger als 30
  Sekunden, ausgehend vom geöffneten Eintrag in der Erweiterung.
- **SC-006**: Sind Absender und Empfänger online, erscheint ein neuer Eintrag
  unter einer geteilten Wurzel beim Empfänger innerhalb von 10 Sekunden.
- **SC-007**: Nach einem Entfernen kann der entfernte Empfänger 0 % der
  Änderungen lesen, die mit einer späteren Schlüsselgeneration verschlüsselt
  sind.
- **SC-008**: Eine Beispielerweiterung (Einkaufsliste) wird allein durch ihre
  Erklärung teilbar, ohne eigenen Code für Schlüssel, Synchronisierung oder
  Rechte.
- **SC-009**: Nach Austritt oder Entfernen der eigenen Kopie verliert der
  Eigentümer in 100 % der Fälle keinen Eintrag.
- **SC-010**: Nach einem Entfernen oder Zurückstufen lehnt jedes Gerät, das die
  neue Mitgliederliste kennt, 100 % der Änderungen der betroffenen Vault
  jenseits der Grenze ab, die das entzogene Recht brauchen, auch zurückdatierte.
  Änderungen, die ein Gerät schon vorher angewendet hat, können bleiben, bis
  eine berechtigte Änderung sie überschreibt (dokumentiertes Risiko, Klärung
  vom 2026-09-28 zum Entzug).
- **SC-011**: Ein Eingeladener, dessen Annahme der Eigentümer noch nicht
  bestätigt hat, erhält in 100 % der Testfälle keinen Inhaltsschlüssel und
  keinen Zugriff auf das Postfach der Freigabe am Relay.

## Assumptions

- holzi hat noch keine Tabellen, die einer Erweiterung gehören, ihr Präfix
  tragen und synchronisiert werden (Entwurf §15 Punkt 10). Diese Spec setzt sie
  voraus, ebenso den Erweiterungs-Host mit Manifest, Bestätigung bei der
  Installation und einer Prüfstelle für Anfragen (geplante Spec 017, ADR-0004).
  Nach der Phasenregel der Constitution wird sie erst umgesetzt, wenn diese
  Grundlagen, die Specs 024 und 026 sowie Einladungen und direkte
  Verbindungen zwischen Mitgliedern aus Spec 027 im Einsatz sind.
- Spec 024 liefert Vault-Identität, Geräteliste mit Hauptgeräten und
  verknüpften Geräten, Unterschriften mit echtem Autor, das Führen des
  Erstellers durch den Kern und die Nur-direkt-Daten. Spec 026 liefert Relay,
  Postfach, Mitgliederliste und die Prüfung der Geräteliste. Spec 027 liefert den Einladungslink samt Antworten und die direkte
  Verbindung zwischen Mitgliedern.
- Das konkrete Format der Erklärung im Manifest und die Anfragen der Erweiterung
  an holzi legt der Plan fest. Beides berührt auch die Repositories `vault-sdk`
  und `haextension`; Verweise darauf werden mit vollem Commit-SHA angegeben.
- Eigentümer und Empfänger nutzen holzi und dieselbe Erweiterung (gleicher
  öffentlicher Schlüssel und Name), nicht zwingend dieselbe Version.
- Personen werden über einen Einladungslink eingeladen, den der Eigentümer
  selbst weitergibt, oder aus dem Adressbuch (Spec 051). Den Namen in der
  Einladung gibt der Eigentümer für sich selbst an.
- Ein entfernter Empfänger behält, was er schon entschlüsseln konnte, wie beim
  Entfernen aus einem Space (Entwurf §13; haex-vault ADR 0002, Repository
  `https://github.com/haex-space/haex-vault`, Revision
  `8dce379d94e18fcd42c3b73686a06f984ca3f574`, Pfad
  `docs/adr/0002-shared-space-authenticity-and-confidentiality.md`).
- Anlegen von Datenfreigaben und Ändern von Rechten sind dem Nutzer
  vorbehalten, bis Spec 021 etwas anderes für Agenten festlegt.
- Die Kosten einer neuen Schlüsselgeneration wachsen mit der Zahl der Geräte
  aller beteiligten Vaults, weil je Gerät verschlüsselt wird (D28). Für
  Freigaben mit wenigen Personen ist das unkritisch; gemessen wird im Plan.

## Nicht im Umfang

- Dateien teilen; das machen Spaces (Spec 027). Dateien, auf die ein Eintrag
  einer Erweiterung verweist, gehen mit einer Datenfreigabe nicht mit.
- Daten des Kerns teilen (Chat-Verlauf, Einstellungen, Modelle). Geteilt werden
  nur Daten von Erweiterungen.
- Weiterteilen durch Empfänger und mehrere Admins (D6, D7), ebenso das
  Verhindern, dass ein Empfänger Inhalte kopiert und selbst neu teilt.
- Die Eigentümerschaft (Admin-Rolle) übertragen, weder auf Wunsch noch nach
  Verlust der Vault (D23). Geht die Vault des Eigentümers verloren, ist die
  Datenfreigabe eingefroren.
- Ein Rotieren der Vault-Identität (D26).
- Rechte feiner als je Datenfreigabe (je Eintrag oder Feld) und Gruppen als
  Empfänger.
- Teilen mit Personen ohne holzi, öffentliche Links.
- Verbergen der Teilnehmer vor dem Relay (D9), nachträgliches Neuverschlüsseln
  alter Daten und Schutz nach einer Kompromittierung.
- MLS (D13).
