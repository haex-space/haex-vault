# Feature Specification: Dateisync zwischen eigenen Geräten

**Feature Branch**: `025-own-device-file-sync`
**Created**: 2026-09-28
**Status**: Draft
**Input**: Zeile 025 des Spec-Schnitts im Sync-Entwurf
([`2026-09-28-sync-architecture.md`](../../docs/plans/2026-09-28-sync-architecture.md), §7 und §14):
Der Nutzer wählt Ordner, die holzi zwischen seinen eigenen Geräten synchron hält. Neue, geänderte,
umbenannte und gelöschte Dateien kommen auf den anderen Geräten der Vault an, auch sehr große
Dateien und nach unterbrochenen Übertragungen. Gleichzeitige Änderungen derselben Datei verlieren
nichts, sondern erzeugen eine Konfliktkopie. Dateiinhalte verlassen ein Gerät nur verschlüsselt,
auch auf der direkten Verbindung zwischen eigenen Geräten. Betreibervorgabe: Übertragen wird über
iroh, Dateiinhalte als geprüfte, fortsetzbare iroh-Blobs.

## Begriffe

- **Vault-Identität**, **Geräteschlüssel**, **Geräteliste**, **Hauptgerät**, **verknüpftes
  Gerät**: wie in Spec 024. Der öffentliche Schlüssel der Vault-Identität ist die feste Adresse der
  Vault; ihr privater Schlüssel liegt nur auf Hauptgeräten. Jedes Gerät hat seinen eigenen
  Geräteschlüssel, der es nie verlässt. Die Geräteliste, signiert mit der Vault-Identität von einem
  Hauptgerät, führt alle aktuellen Geräte der Vault; ein Gerät gehört zur Vault, solange es auf ihrer
  aktuellen Geräteliste steht. Für den Dateisync sind Hauptgeräte und verknüpfte Geräte gleich:
  Beide lesen und schreiben alle Daten der Vault (D27).
- **Bereich**: der Geltungsbereich synchronisierter Daten: der Bereich „Vault“, der Bereich eines
  Space oder der Bereich einer Datenfreigabe (Spec 024). Alles in dieser Spec gehört zum Bereich
  „Vault“; das eigene Postfach der Vault auf einem Relay (Spec 026) gehört ebenfalls dazu und ist
  kein anderer Bereich.
- **Änderungspaket**: ein verschlüsseltes, signiertes Bündel von Änderungen, wie es Spec 024
  zwischen den eigenen Geräten austauscht.
- **Inhaltsschlüssel**, **Schlüsselgeneration**: wie in Spec 024.
- **Synchronisierter Ordner**: ein Ordner, den der Nutzer auf seinen Geräten gleich halten lässt. Er
  hat einen Namen und gilt für die ganze Vault; welche Geräte ihn tatsächlich führen, entscheidet
  die Bindung.
- **Bindung**: die Zuordnung eines synchronisierten Ordners zu einem Ort auf einem bestimmten Gerät.
  Sie gehört genau diesem Gerät. Ein Gerät ohne Bindung kennt den Ordner, hält aber keine seiner
  Dateien.
- **Dateiindex**: die synchronisierte Liste der Dateien eines synchronisierten Ordners. Ein Eintrag
  je Datei mit Pfad im Ordner, Größe, Änderungszeit, Inhalts-Prüfsumme, eigenem Schlüssel der Datei,
  `created_by`, `modified_by` und der Angabe, ob die Datei gelöscht ist.
- **Objekt**: der verschlüsselte, unveränderliche Inhalt einer Dateiversion, benannt nach der
  Prüfsumme seines verschlüsselten Inhalts. Eine neue Version einer Datei ist immer ein neues
  Objekt.
- **Konfliktkopie**: das Ergebnis gleichzeitiger Änderungen derselben Datei auf verschiedenen
  Geräten: Eine der Versionen behält den Namen, die andere wird als eigene Datei daneben abgelegt.
- **Löschvermerk**: der Eintrag im Dateiindex, der festhält, dass eine Datei gelöscht wurde. Er
  verhindert, dass ein Gerät, das die Löschung verpasst hat, die Datei zurückbringt.
- **Online**: ein Gerät, auf dem holzi mit entsperrter Vault läuft und das ein anderes eigenes Gerät
  direkt erreichen kann (Spec 024).

## Beziehung zu bestehenden Specs

- Sync-Entwurf
  ([`2026-09-28-sync-architecture.md`](../../docs/plans/2026-09-28-sync-architecture.md)): Diese
  Spec setzt §7 um und nutzt dafür die Bausteine aus §5 und §8.1 (Dateiindex, Objekte, Übertragung)
  sowie §8.3 (Konflikte). Sie legt fest, was der Nutzer erlebt; Formate und Protokolle bleiben Sache
  des Plans.
- **Spec 024** (Identität, Geräteschlüssel, direkte Verbindung, Datensync zwischen eigenen Geräten):
  Voraussetzung. Der Dateiindex ist gewöhnliche Vault-Information und reist mit dem Datensync von
  Spec 024 in Änderungspaketen. Nur Geräte auf der aktuellen Geräteliste der Vault tauschen Objekte
  aus. Ein Gerät, das ein Hauptgerät von der Geräteliste entfernt (Spec 024), bekommt keine Objekte
  mehr und liefert keine mehr; weil beim Entfernen eine neue Generation des Inhaltsschlüssels des
  Bereichs „Vault“ entsteht, kann es neue Einträge des Dateiindex und damit die Schlüssel neuer
  Dateiversionen nicht mehr entschlüsseln. Was schon auf ihm liegt, bleibt dort.
- **Spec 027** (Spaces): Ein synchronisierter Ordner nutzt dieselben Abläufe wie ein Space mit der
  eigenen Vault als einzigem Mitglied (Entwurf §7), aber ohne eigenen Bereich: Sein Dateiindex
  gehört zum Bereich „Vault“ (FR-048). Spec 027 erweitert dieselbe Grundlage um weitere Mitglieder,
  Einladungen und Rechte. Es gibt keine zweite Umsetzung für Dateisync.
- **Spec 026** (Relay, Speicher A) und **Spec 029** (eigenes S3, Speicher B): Diese Spec überträgt
  Objekte nur direkt zwischen eigenen Geräten. Dass Objekte auf einem Relay oder in einem
  S3-Speicher liegen, damit zwei Geräte sich nicht gleichzeitig online treffen müssen, kommt mit
  diesen Specs: Dass die Objekte eigener synchronisierter Ordner Speicher A nutzen dürfen, legt Spec
  026 in einer eigenen Anforderung für den Client fest; Speicher B für eigene Ordner legt Spec 029
  (User Story 6) fest. Weil ein Objekt schon hier verschlüsselt und nach seinem Inhalt benannt ist,
  können sie es unverändert ablegen. Über das eigene Postfach der Vault auf dem Relay (Spec 026)
  darf auch der Dateiindex samt den Schlüsseln der Dateien reisen: Er ist gewöhnliche Vault-Information.
  Ausschließlich direkt reist nur der private Schlüssel der Vault-Identität (Spec 024, D30).
- [`013-vault-lifecycle-isolation`](../013-vault-lifecycle-isolation/spec.md): Synchronisiert wird
  nur während einer Vault-Session. Ist die Vault gesperrt oder geschlossen, ruht der Dateisync auf
  diesem Gerät.
- [`014-portable-mode`](../014-portable-mode/spec.md): Dateien in einem gebundenen Ordner liegen
  außerhalb der Vault und damit außerhalb des Schutzes des portablen Modus, wie von Agenten
  angelegte Dateien dort.
- [`023-settings-app`](../023-settings-app/spec.md): Die synchronisierten Ordner werden in der
  Kategorie „Föderation“ in der Unteransicht „Ordner“ verwaltet, neben „Geräte“ (Spec 024),
  „Relays“ (Spec 026), „Spaces“ (Spec 027) und „Datenfreigaben“ (Spec 028). Was ausgewählt ist,
  gilt sofort, ohne Knopf zum Übernehmen (FR-021 dort).
- [`020-tab-navigation`](../020-tab-navigation/spec.md): Die Aktionen dieser Spec (FR-007) stehen
  im Katalog von Spec 020.

## Clarifications

### Session 2026-09-27

- Q: Wofür steht ein Geräteschlüssel, und darf dieselbe Vault auf mehreren Geräten zugleich laufen? → A: Ein Nostr-Schlüssel steht für ein Gerät; dieselbe Vault läuft gleichzeitig auf mehreren Geräten, jedes mit eigenem Schlüssel, die Vault-Identität ist auf allen gleich (D1).
- Q: Was ist ein Space? → A: Ein Netzwerkordner nur für Dateien; `Lesen` heißt alle Dateien lesen, `Schreiben` heißt Dateien hinzufügen und ändern (D2). Der Dateisync zwischen eigenen Geräten ist derselbe Mechanismus mit der eigenen Vault als einzigem Mitglied.
- Q: Wird MLS für Schlüssel verwendet? → A: Nein (D13). Schlüssel sind gewöhnliche synchronisierte Einträge; für den Bereich „Vault“ ist der Inhaltsschlüssel fest und wechselt nur mit der Vault-Identität. (Überholt durch D26 und D27: Die Vault-Identität wechselt nie; eine neue Generation des Inhaltsschlüssels des Bereichs „Vault“ entsteht, wenn ein Hauptgerät ein Gerät von der Geräteliste entfernt.)

### Session 2026-09-28

- Q: Wer darf zwischen eigenen Geräten was? → A: Der private Schlüssel der Vault-Identität liegt auf jedem Gerät der Vault (D8). Alle eigenen Geräte sind gleich vertrauenswürdig; zwischen ihnen gibt es keine Rechteprüfung für Dateien. (Überholt durch D27, was den privaten Schlüssel betrifft: Er liegt nur auf Hauptgeräten. Für Dateien gilt weiter: Alle Geräte auf der Geräteliste sind gleichgestellt, ohne Rechteprüfung.)
- Q: Gibt es in v1 ein Rotieren der Vault-Identität? → A: Nein (D26). Ein verlorenes Gerät ist kein Problem, solange eine Kopie oder das Relay existiert und die Passphrase hält; ausgesperrt wird ein Gerät über die Geräteliste (D27).
- Q: Wer darf Geräte hinzufügen und entfernen? → A: Nur Hauptgeräte, die den privaten Schlüssel der Vault-Identität haben; es kann mehrere geben. Beim Verknüpfen fragt holzi, ob der Schlüssel mit übertragen wird (Standard: nein). Verknüpfen ist der bevorzugte Weg, Kopieren der Datei bleibt möglich (D27).
- Q: Welche Daten reisen nur über die direkte Verbindung zwischen eigenen Geräten? → A: Nur der private Schlüssel der Vault-Identität (D30). Geräteschlüssel verlassen ihr Gerät nie. Dateiindex und die Schlüssel der Dateien sind gewöhnliche Vault-Information und dürfen auch über das eigene Postfach der Vault reisen.
- Q: Was passiert, wenn dieselbe Datei gleichzeitig auf zwei Geräten geändert wird? → A: Es entsteht eine Konfliktkopie (D10), kein stilles Überschreiben.
- Q: Wo liegen Dateiinhalte, wenn Geräte nicht gleichzeitig online sind? → A: In v1 gibt es beide Speicher: den des öffentlichen Relays (A) und das eigene S3 des Nutzers (B) (D12). Beides kommt mit Spec 026 und 029; diese Spec überträgt nur direkt.
- Q: Werden Dateien auf der direkten Verbindung zwischen eigenen Geräten zusätzlich verschlüsselt? → A: Ja. Das Relay ist nicht vertrauenswürdig (D11), und ein einziges Format erlaubt, dass jedes Gerät und später jeder Speicher ein Objekt aufbewahrt und weitergibt, ohne den Inhalt zu sehen (Entwurf §5.1, §8.1).
- Q: Braucht der Dateisync zwischen eigenen Geräten eine eigene Umsetzung? → A: Nein. Er ist ein Space mit genau einem Mitglied, der Vault; der Dateiindex liegt im Bereich „Vault“ (Entwurf §7).
- Q: Wo liegen die synchronisierten Dateien auf dem Gerät? → A: In einem Ordner des Dateisystems, den der Nutzer je Gerät wählt, wie bei Syncthing oder Dropbox (FR-002). Das gilt auch für Spaces (Spec 027).

## User Scenarios & Testing _(mandatory)_

### User Story 1 - Einen Ordner auf zwei Geräten gleich halten (Priority: P1)

Eine Nutzerin hat ihre Vault auf Arbeitsrechner und Laptop. Auf dem Arbeitsrechner wählt sie in den
Einstellungen „Ordner synchronisieren“ und ihren Ordner „Projekte“. Auf dem Laptop erscheint
„Projekte“ als verfügbarer Ordner; sie wählt dort einen Ort dafür. Die Dateien kommen an, und was
sie danach auf einem der beiden Geräte speichert, steht kurz darauf auch auf dem anderen.

**Why this priority**: Das ist der Kern der Spec. Ohne ihn gibt es keinen Dateisync.

**Independent Test**: Zwei Geräte derselben Vault, beide online. Auf Gerät A einen Ordner mit
einigen Dateien und Unterordnern synchronisieren, auf Gerät B binden: Inhalt und Struktur stimmen
überein. Auf B eine Datei ändern und eine anlegen: Beides kommt auf A an.

**Acceptance Scenarios**:

1. **Given** Gerät A, **When** die Nutzerin einen Ordner zum Synchronisieren wählt, **Then**
   erscheint er auf jedem anderen Gerät der Vault als verfügbarer, noch nicht gebundener Ordner mit
   seinem Namen.
2. **Given** ein verfügbarer Ordner auf Gerät B, **When** die Nutzerin einen leeren Ort dafür wählt,
   **Then** erscheinen dort alle Dateien und Unterordner mit ihrem Inhalt und ihrer Änderungszeit.
3. **Given** ein auf beiden Geräten gebundener Ordner, **When** eine Datei auf einem Gerät angelegt
   oder geändert wird, **Then** hat das andere Gerät, wenn es online ist, danach dieselbe Datei mit
   demselben Inhalt (SC-001).
4. **Given** der gewählte Ort auf Gerät B enthält schon Dateien, **When** die Nutzerin ihn bindet,
   **Then** werden gleiche Dateien nicht übertragen, nur dort vorhandene Dateien kommen zum Ordner
   hinzu, und Dateien mit gleichem Pfad, aber anderem Inhalt werden nach User Story 2 behandelt;
   keine Datei geht verloren.
5. **Given** Gerät B war offline, **When** es wieder online ist, **Then** holt es alle inzwischen
   eingetretenen Änderungen nach und meldet Änderungen, die es selbst in der Zwischenzeit hatte, an
   A.
6. **Given** holzi war auf einem Gerät geschlossen und im Ordner wurde in der Zeit etwas geändert,
   **When** die Vault dort wieder geöffnet wird, **Then** erkennt holzi diese Änderungen und
   synchronisiert sie.

---

### User Story 2 - Gleichzeitige Änderungen gehen nicht verloren (Priority: P1)

Der Nutzer ändert dieselbe Datei auf dem Laptop im Zug und auf dem Arbeitsrechner, bevor beide sich
wieder sehen. Danach liegen beide Fassungen auf beiden Geräten: eine unter dem ursprünglichen Namen,
die andere als Konfliktkopie daneben. holzi zeigt an, dass ein Konflikt entstanden ist.

**Why this priority**: Datenverlust ist der schlimmste Fehler eines Dateisyncs.
Betreiberentscheidung D10.

**Independent Test**: Zwei Geräte trennen, dieselbe Datei auf beiden unterschiedlich ändern, wieder
verbinden: Auf beiden Geräten gibt es danach die Datei und genau eine Konfliktkopie, zusammen mit
beiden Fassungen, und auf beiden Geräten trägt dieselbe Fassung den ursprünglichen Namen.

**Acceptance Scenarios**:

1. **Given** eine Datei wurde auf zwei Geräten geändert, ohne dass eines die Änderung des anderen
   kannte, **When** die Geräte synchronisieren, **Then** entsteht eine Konfliktkopie, und beide
   Fassungen sind auf allen gebundenen Geräten vorhanden.
2. **Given** eine Konfliktkopie, **When** der Nutzer sich den Ordner ansieht, **Then** erkennt er
   sie am Namen: ursprünglicher Name, Kennzeichen für Konflikt, Name des Geräts und Zeitpunkt, mit
   derselben Dateiendung.
3. **Given** auf zwei Geräten entsteht unabhängig eine Datei mit demselben Pfad und
   unterschiedlichem Inhalt, **When** sie synchronisieren, **Then** entsteht ebenfalls eine
   Konfliktkopie; bei gleichem Inhalt entsteht keine.
4. **Given** eine Datei wurde auf einem Gerät geändert und auf einem anderen gleichzeitig gelöscht,
   **When** sie synchronisieren, **Then** bleibt die geänderte Fassung erhalten, auf allen
   gebundenen Geräten.
5. **Given** ein Konflikt ist entstanden, **When** der Nutzer die Statusansicht öffnet, **Then**
   sieht er ihn dort mit beiden Dateinamen, bis er eine der beiden Dateien umbenannt, verschoben
   oder gelöscht hat.

---

### User Story 3 - Umbenennen, Verschieben und Löschen (Priority: P1)

Die Nutzerin räumt ihren Ordner auf: Sie benennt Dateien um, verschiebt einen Unterordner mit großen
Videos und löscht alte Entwürfe. Auf dem anderen Gerät geschieht dasselbe, ohne dass die Videos noch
einmal übertragen werden. Ein Tablet, das eine Woche aus war, bringt die gelöschten Entwürfe nicht
zurück.

**Why this priority**: Ohne verlässliches Löschen und Umbenennen füllt sich jeder Ordner mit
Wiedergängern und Doppelten.

**Independent Test**: Eine große Datei umbenennen und in einen Unterordner verschieben: Auf dem
anderen Gerät steht sie danach am neuen Ort, und es wurde kaum Inhalt übertragen. Eine Datei
löschen, während ein drittes Gerät offline ist; das dritte Gerät wieder einschalten: Die Datei
verschwindet auch dort und taucht auf keinem Gerät wieder auf.

**Acceptance Scenarios**:

1. **Given** eine Datei wird im Ordner umbenannt oder verschoben, **When** das andere Gerät
   synchronisiert, **Then** steht sie dort am neuen Pfad, ohne dass ihr Inhalt erneut übertragen
   wird (SC-005).
2. **Given** eine Datei wird auf einem Gerät gelöscht, **When** die anderen gebundenen Geräte
   synchronisieren, **Then** wird sie dort ebenfalls entfernt, in den Papierkorb des Systems, wo es
   einen gibt.
3. **Given** ein Gerät war während der Löschung offline und hat die Datei noch unverändert, **When**
   es wieder online ist, **Then** entfernt es die Datei ebenfalls, statt sie den anderen
   zurückzuschicken.
4. **Given** eine Datei wird aus dem Ordner hinausverschoben, **When** die anderen Geräte
   synchronisieren, **Then** gilt das dort als Löschung; hineinverschobene Dateien gelten als neu.
5. **Given** ein Unterordner wird gelöscht, **When** die anderen Geräte synchronisieren, **Then**
   verschwindet er mit seinem ganzen Inhalt, außer Dateien, die dort in der Zwischenzeit geändert
   wurden (User Story 2 AS4).

---

### User Story 4 - Große Dateien und unterbrochene Übertragungen (Priority: P2)

Ein Nutzer legt eine 20-GB-Videodatei in den Ordner. Der Laptop verlässt nach der Hälfte das WLAN.
Später setzt er die Übertragung fort, ohne von vorn zu beginnen, und zwar vom Arbeitsrechner oder
vom Heimserver, je nachdem, wer die Datei schon vollständig hat und erreichbar ist.

**Why this priority**: Große Dateien sind der Alltag eines Dateisyncs, und Verbindungen brechen ab.
Ohne Fortsetzen wären große Dateien praktisch unerreichbar.

**Independent Test**: Drei Geräte A, B, C. Eine große Datei auf A anlegen, B vollständig empfangen
lassen. C während der Übertragung von A trennen, A ausschalten, C wieder verbinden: C setzt bei B
fort, ohne die bereits empfangenen Teile noch einmal zu laden, und die Datei ist danach unverändert.

**Acceptance Scenarios**:

1. **Given** eine Übertragung bricht ab, **When** die Verbindung wiederkommt, **Then** setzt sie
   dort fort, wo sie aufgehört hat.
2. **Given** das ursprüngliche Gerät ist nicht erreichbar, aber ein anderes eigenes Gerät hat
   dieselbe Fassung, **When** ein Gerät sie braucht, **Then** holt es sie von dort und setzt eine
   abgebrochene Übertragung dort fort.
3. **Given** eine Datei wird gerade empfangen, **When** jemand in den Ordner schaut, **Then** steht
   dort unter ihrem Namen entweder die alte Fassung oder die vollständige neue, nie eine halbe.
4. **Given** empfangene Daten passen nicht zur erwarteten Prüfsumme, **When** holzi das erkennt,
   **Then** verwirft es diese Teile, holt sie neu und schreibt nichts Falsches in den Ordner.
5. **Given** kein Gerät mit der benötigten Fassung ist online, **When** die Nutzerin die
   Statusansicht öffnet, **Then** sieht sie, dass die Datei auf ein anderes Gerät wartet, und auf
   welches.

---

### User Story 5 - Status sehen, pausieren und Speicherplatz (Priority: P2)

Die Nutzerin will wissen, ob alles angekommen ist. Die Statusansicht zeigt für jeden Ordner, ob er
aktuell ist, was gerade übertragen wird und wie weit, welche Geräte den Ordner führen und welche
Probleme es gibt. Unterwegs im teuren Hotel-WLAN pausiert sie den Sync auf dem Laptop. Als die
Platte des Laptops fast voll ist, hält holzi an, statt sie ganz zu füllen, und sagt ihr das.

**Why this priority**: Ein Sync, dem man nicht ansieht, ob er fertig ist, ist nicht
vertrauenswürdig. Pausieren und Plattenschutz verhindern Schaden in Grenzsituationen.

**Independent Test**: Während einer großen Übertragung die Statusansicht öffnen: Sie zeigt Datei,
Fortschritt und Gegengerät. Pausieren: Die Übertragung steht, lokale Änderungen werden nicht
erfasst und nicht verschickt, eingehende Änderungen landen im Dateiindex, aber nicht auf der
Platte. Fortsetzen: Das Gerät setzt sie auf der Platte um, und alles läuft weiter. Einen Ordner binden, dessen Inhalt nicht auf die Platte
passt: holzi lädt bis zur Reserve, hält an und meldet es.

**Acceptance Scenarios**:

1. **Given** ein gebundener Ordner, **When** die Nutzerin die Statusansicht öffnet, **Then** sieht
   sie seinen Zustand (aktuell, wird synchronisiert, pausiert, wartet auf ein Gerät, Problem),
   laufende Übertragungen mit Fortschritt und die Geräte, die ihn gebunden haben.
2. **Given** der Sync läuft, **When** die Nutzerin ihn für einen Ordner oder für dieses Gerät
   pausiert, **Then** erfasst dieses Gerät für diesen Umfang keine lokalen Änderungen mehr,
   schreibt keine Dateien auf die Platte und überträgt keine Objekte, bis sie fortsetzt; der
   Datensync von Spec 024 läuft weiter, eingehende Änderungen des Dateiindex werden mit ihrem
   Änderungspaket atomar übernommen und erst nach dem Fortsetzen auf der Platte umgesetzt
   (FR-044), und die anderen Geräte synchronisieren untereinander weiter.
3. **Given** der Sync war pausiert, **When** die Nutzerin fortsetzt, **Then** holt das Gerät alle
   zwischenzeitlichen Änderungen in beide Richtungen nach.
4. **Given** der freie Platz würde durch eine eingehende Datei unter die Reserve fallen, **When**
   holzi das erkennt, **Then** beginnt es diese Übertragung nicht, zeigt den Grund an und macht mit
   Dateien weiter, die noch passen.
5. **Given** die Pausierung, **When** die Vault geschlossen und wieder geöffnet wird, **Then** ist
   der Sync auf diesem Gerät weiter pausiert.

---

### User Story 6 - Dateien vom Sync ausnehmen (Priority: P2)

Ein Entwickler synchronisiert seinen Dokumente-Ordner. Temporäre Dateien seines Editors und
Sperrdateien der Office-Programme sollen nicht auf die anderen Geräte wandern, ebenso wenig ein
Unterordner mit Build-Ausgaben. Die üblichen temporären Dateien bleiben von selbst draußen; den
Unterordner nimmt er mit einem Muster aus.

**Why this priority**: Ohne Ausschlüsse erzeugen temporäre Dateien ständigen Verkehr und falsche
Konflikte.

**Independent Test**: Im Ordner eine Office-Sperrdatei, eine Datei mit Endung `.tmp` und einen
ausgenommenen Unterordner anlegen: Keine davon erscheint auf dem anderen Gerät. Das Muster
entfernen: Der Unterordner kommt an.

**Acceptance Scenarios**:

1. **Given** ein neuer synchronisierter Ordner, **When** darin übliche temporäre Dateien entstehen,
   **Then** werden sie nicht synchronisiert.
2. **Given** der Nutzer fügt ein Ausschlussmuster hinzu, **When** es gespeichert ist, **Then** gilt
   es auf allen Geräten, die den Ordner führen; passende Dateien werden fortan nicht mehr
   synchronisiert, auf keinem Gerät gelöscht und bleiben lokal liegen.
3. **Given** ein Muster wird entfernt, **When** die Änderung gilt, **Then** werden die bisher
   ausgenommenen Dateien wie neue Dateien synchronisiert.

---

### User Story 7 - Ordner lösen oder nicht mehr synchronisieren (Priority: P3)

Ein Nutzer braucht einen großen Ordner auf seinem kleinen Laptop nicht mehr. Er löst ihn dort; die
Dateien bleiben auf dem Laptop liegen, werden aber nicht mehr abgeglichen, und die anderen Geräte
synchronisieren weiter. Später beendet er den Sync des Ordners ganz; auf keinem Gerät wird dabei
etwas gelöscht.

**Why this priority**: Wichtig, aber seltener als die Kernabläufe.

**Independent Test**: Ordner auf Gerät B lösen, auf A eine Datei ändern: B bekommt die Änderung
nicht, die Dateien auf B sind unverändert. Ordner ganz beenden: Auf allen Geräten bleiben die
Dateien, und der Ordner steht nicht mehr in der Liste.

**Acceptance Scenarios**:

1. **Given** ein gebundener Ordner, **When** der Nutzer ihn auf diesem Gerät löst, **Then** bleiben
   die Dateien dort unverändert liegen, und keine Datei wird auf anderen Geräten gelöscht.
2. **Given** ein Ordner, **When** der Nutzer den Sync für alle Geräte beendet, **Then** lösen alle
   Geräte ihn, sobald sie davon erfahren, und keine Datei wird gelöscht.
3. **Given** ein gelöster Ordner, **When** der Nutzer ihn auf diesem Gerät erneut bindet, **Then**
   gilt User Story 1 AS4.

---

### Edge Cases

- Der gebundene Ort ist weg (Laufwerk abgezogen, Ordner umbenannt oder gelöscht). holzi deutet das
  nie als Löschung aller Dateien: Der Ordner geht auf diesem Gerät in den Zustand „Problem“ und wird
  nicht synchronisiert, bis der Ort wieder da ist oder der Nutzer ihn neu bindet.
- Eine Datei ändert sich lokal, während holzi eine neue Fassung von einem anderen Gerät empfängt.
  Die lokale Änderung gewinnt nicht stillschweigend und geht nicht verloren: Es entsteht eine
  Konfliktkopie.
- Ein Dateiname ist auf einem anderen Gerät nicht erlaubt (etwa ein Doppelpunkt unter Windows) oder
  kollidiert dort, weil das Dateisystem Groß- und Kleinschreibung nicht unterscheidet, oder die
  Datei ist zu groß für das dortige Dateisystem. Die Datei wird dort nicht angelegt, der Fall
  erscheint in der Statusansicht, und auf den anderen Geräten bleibt sie unberührt.
- Eine Datei ist lokal nicht lesbar (fehlende Rechte, von einem Programm gesperrt). Sie wird
  übersprungen, gemeldet und später erneut versucht.
- Die Uhren zweier Geräte gehen unterschiedlich. Ob Änderungen gleichzeitig waren, hängt nicht an
  der Uhrzeit, sondern daran, welche Fassung jedes Gerät zuletzt kannte.
- Ein neues Gerät kommt zur Vault, verknüpft oder als Kopie der Vault-Datei (Spec 024). Es übernimmt
  keine Bindungen eines anderen Geräts; alle Ordner erscheinen dort als verfügbar, nicht gebunden.
  Eine Kopie tauscht erst Objekte aus, wenn ein Hauptgerät sie auf die Geräteliste gesetzt hat
  (FR-015).
- Ein Hauptgerät entfernt ein Gerät von der Geräteliste. Die übrigen Geräte liefern ihm keine
  Objekte mehr und nehmen keine mehr von ihm an; seine Dateien in gebundenen Ordnern bleiben auf ihm
  liegen, holzi löscht dort nichts aus der Ferne.
- Nur ein Gerät hat einen Ordner gebunden. Es hält den Dateiindex aktuell; ein zweites Gerät, das
  später bindet, bekommt den Stand von dort.
- Der Dateiindex kommt über Spec 024 früher an als die Inhalte. Die Dateien erscheinen erst, wenn
  ihr Inhalt da ist; bis dahin zeigt die Statusansicht, worauf gewartet wird.
- Viele Dateien verschwinden auf einmal, weil der Nutzer den Ordnerinhalt versehentlich gelöscht
  hat. Das ist eine gewöhnliche Löschung und wird weitergegeben; auf den anderen Geräten landen die
  Dateien im Papierkorb.

## Requirements _(mandatory)_

### Functional Requirements

**Synchronisierte Ordner und Bindung**

- **FR-001**: holzi MUSS dem Nutzer erlauben, einen Ordner zum synchronisierten Ordner zu machen.
  Der synchronisierte Ordner MUSS für die ganze Vault gelten und auf jedem Gerät der Vault mit
  seinem Namen als verfügbar erscheinen.
- **FR-002**: Eine Bindung MUSS einem synchronisierten Ordner auf genau einem Gerät einen Ort
  zuordnen. Sie gehört diesem Gerät und DARF NICHT an andere Geräte weitergegeben werden; jedes
  Gerät bindet selbst. Wo die Dateien auf dem Gerät liegen:
  in einem Ordner des Dateisystems, den der Nutzer auf diesem Gerät wählt (wie bei Syncthing oder Dropbox).
- **FR-003**: Ein Gerät ohne Bindung DARF keine Dateien des Ordners anlegen oder speichern; es kennt
  nur Name und Zustand.
- **FR-004**: Beim Binden an einen Ort, der schon Dateien enthält, MUSS holzi den vorhandenen Inhalt
  mit dem Ordner zusammenführen: Gleiche Dateien werden nicht übertragen, nur lokal vorhandene
  kommen hinzu, abweichende Dateien mit gleichem Pfad werden nach FR-032 behandelt. Beim Binden DARF
  keine lokale Datei überschrieben oder gelöscht werden.
- **FR-005**: holzi MUSS eine Bindung ablehnen, deren Ort mit dem Ort einer anderen Bindung auf
  diesem Gerät zusammenfällt oder in ihm liegt oder ihn enthält.
- **FR-006**: Der Nutzer MUSS einen Ordner auf diesem Gerät lösen und den Sync eines Ordners für
  alle Geräte beenden können. Beides DARF auf keinem Gerät Dateien löschen.
- **FR-007**: Anlegen, Binden, Lösen, Beenden, Pausieren, Fortsetzen und das Ändern von Ausschlüssen
  MÜSSEN Aktionen im Katalog von Spec 020 sein. Die Einstellungsansicht MUSS jede Auswahl sofort
  übernehmen, ohne Knopf zum Übernehmen.

**Erkennen lokaler Änderungen**

- **FR-008**: Während einer Vault-Session MUSS holzi neue, geänderte, umbenannte, verschobene und
  gelöschte Dateien und Unterordner in gebundenen Ordnern erkennen, auch leere Unterordner.
- **FR-009**: Beim Öffnen der Vault MUSS holzi Änderungen erkennen, die geschahen, während holzi auf
  diesem Gerät nicht lief.
- **FR-010**: holzi MUSS eine Datei erst übernehmen, wenn sie eine Weile unverändert ist, und DARF
  keine halb geschriebene Fassung als neue Version verbreiten.
- **FR-011**: Ob sich ein Inhalt geändert hat, MUSS holzi am Inhalt entscheiden, nicht allein an
  Änderungszeit oder Größe. Eine Datei, deren Inhalt gleich geblieben ist, DARF keine neue Version
  erzeugen.
- **FR-012**: Ist der gebundene Ort nicht mehr vorhanden oder nicht lesbar, MUSS holzi den Ordner
  auf diesem Gerät anhalten und DARF daraus KEINE Löschungen ableiten.

**Übertragung der Inhalte**

- **FR-013**: Der Dateiindex MUSS als Vault-Information im Bereich „Vault“ mit dem Datensync von
  Spec 024 zu allen Geräten der Vault gelangen, auf direktem Weg oder über das eigene Postfach der
  Vault auf einem Relay (Spec 026).
- **FR-014**: Dateiinhalte MÜSSEN direkt zwischen eigenen, online befindlichen Geräten übertragen
  werden. Ein Gerät MUSS eine Fassung von jedem eigenen Gerät holen können, das sie vollständig hat,
  nicht nur vom Gerät, auf dem sie entstand.
- **FR-015**: Nur Geräte, deren Geräteschlüssel auf der aktuellen Geräteliste der Vault steht
  (Spec 024), DÜRFEN Objekte anfragen oder liefern. Einem Gerät, das nicht oder nicht mehr darauf
  steht, DARF kein Gerät Objekte liefern oder Objekte von ihm annehmen.
- **FR-016**: Eine abgebrochene Übertragung MUSS fortgesetzt werden können, auch von einem anderen
  Gerät, ohne bereits empfangene und geprüfte Teile erneut zu übertragen.
- **FR-017**: Die Größe einer Datei DARF nur durch Speicherplatz und Dateisystem der beteiligten
  Geräte begrenzt sein. Der Arbeitsspeicher, den eine Übertragung braucht, DARF NICHT mit der
  Dateigröße wachsen.
- **FR-018**: Eine empfangene Datei MUSS erst dann unter ihrem Namen im Ordner erscheinen, wenn sie
  vollständig und geprüft ist. Bis dahin bleibt dort die bisherige Fassung oder nichts.
- **FR-019**: Die Änderungszeit einer empfangenen Datei MUSS der im Dateiindex entsprechen.
- **FR-020**: Ein Gerät DARF eine Fassung nur liefern, solange es sie unverändert hat. Hat sich die
  lokale Datei seitdem geändert, liefert es diese Fassung nicht.
- **FR-021**: Ein Objekt, auf das kein Eintrag des Dateiindex mehr zeigt, MUSS von den Geräten
  entfernt werden, die es nur zum Weitergeben aufbewahren.

**Verschlüsselung und Echtheit**

- **FR-022**: Jede Dateiversion MUSS als Objekt verschlüsselt werden, mit einem eigenen Schlüssel je
  Datei, der nur im Dateiindex steht. Das gilt auch auf der direkten Verbindung zwischen eigenen
  Geräten. Die Schlüssel der Dateien sind gewöhnliche Vault-Information (Spec 024, D30): Mit dem
  Dateiindex DÜRFEN sie, verschlüsselt mit dem Inhaltsschlüssel des
  Bereichs „Vault“, auch über das eigene Postfach der Vault reisen, DÜRFEN aber in keinen anderen
  Bereich gelangen.
- **FR-023**: Ein Objekt MUSS nach der Prüfsumme seines verschlüsselten Inhalts benannt sein und
  DARF sich nie ändern; eine neue Fassung ist ein neues Objekt.
- **FR-024**: Der Empfänger MUSS jedes Objekt und jeden empfangenen Teil gegen diese Prüfsumme
  prüfen und nicht passende Daten verwerfen, bevor etwas in den Ordner geschrieben wird.
- **FR-025**: Dateinamen, Pfade, Größen und Inhalte DÜRFEN ein Gerät nur verschlüsselt verlassen:
  der Dateiindex in Änderungspaketen von Spec 024, die Inhalte als Objekte.
- **FR-026**: Das Objekt-Format MUSS dasselbe sein, das Spaces (Spec 027) und die Speicher von Spec
  026 und 029 verwenden, so dass ein Objekt ohne Umwandlung dort abgelegt werden kann.

**Umbenennen, Verschieben, Löschen**

- **FR-027**: Umbenennen und Verschieben innerhalb eines Ordners MÜSSEN als Pfadänderung
  weitergegeben werden, ohne den Inhalt erneut zu übertragen.
- **FR-028**: Eine gelöschte Datei MUSS im Dateiindex einen Löschvermerk bekommen. Die anderen
  gebundenen Geräte MÜSSEN sie entfernen, in den Papierkorb des Systems, wo es einen gibt.
- **FR-029**: Ein Gerät, das die Datei in der gelöschten Fassung noch hat, MUSS sie bei Eintreffen
  des Löschvermerks entfernen und DARF sie NICHT als neue Datei zurückbringen, auch nicht nach
  langer Offline-Zeit.
- **FR-030**: Wurde eine Datei auf einem Gerät geändert, während sie auf einem anderen gelöscht
  wurde, MUSS die geänderte Fassung erhalten bleiben und auf alle gebundenen Geräte gelangen.
- **FR-031**: Hinaus- oder hineinverschobene Dateien MÜSSEN als Löschung bzw. als neue Datei gelten.

**Konflikte**

- **FR-032**: Wurde eine Datei auf zwei Geräten geändert oder neu angelegt, ohne dass eines die
  Fassung des anderen kannte, und unterscheiden sich die Inhalte, MUSS eine Konfliktkopie entstehen.
  Keine der beiden Fassungen DARF verloren gehen.
- **FR-033**: Welche Fassung den ursprünglichen Namen behält, MUSS nach einer festen Regel bestimmt
  werden, die auf allen Geräten dasselbe Ergebnis liefert.
- **FR-034**: Der Name der Konfliktkopie MUSS dem Muster
  `<Name> (Konflikt <Gerätename> <Datum Uhrzeit>).<Endung>` folgen: ursprünglicher Name, das
  Kennzeichen „Konflikt“, der Name des Geräts, von dem die Fassung stammt, und der Zeitpunkt der
  Änderung, mit derselben Dateiendung. Das genaue Format von Datum und Uhrzeit legt der Plan fest.
- **FR-035**: Ob zwei Änderungen gleichzeitig waren, MUSS sich daraus ergeben, welche Fassung jedes
  Gerät zuletzt kannte, nicht aus den Uhren der Geräte.
- **FR-036**: Eine lokale Änderung während des Empfangs einer neuen Fassung MUSS als Konflikt nach
  FR-032 behandelt werden.
- **FR-037**: Die Statusansicht MUSS offene Konflikte eines Ordners zeigen, bis der Nutzer die
  Konfliktkopie oder die Datei umbenannt, verschoben oder gelöscht hat.

**Ausschlüsse**

- **FR-038**: holzi MUSS übliche temporäre Dateien von vornherein ausnehmen, etwa Sperrdateien von
  Office-Programmen, Sicherungsdateien von Editoren, Dateien mit Endung `.tmp`, Systemdateien wie
  `.DS_Store` und `Thumbs.db` sowie holzis eigene unvollständige Empfangsdateien.
- **FR-039**: Der Nutzer MUSS je Ordner eigene Ausschlussmuster für Dateien und Unterordner
  festlegen können. Sie gelten für alle Geräte, die den Ordner führen.
- **FR-040**: Ausgenommene Dateien DÜRFEN weder übertragen noch auf einem Gerät gelöscht werden.
  Wird ein Muster entfernt, MÜSSEN die betroffenen Dateien wie neue Dateien synchronisiert werden.
- **FR-041**: Symbolische Links und Sondereinträge des Dateisystems DÜRFEN NICHT verfolgt oder
  übertragen werden; die Statusansicht MUSS sie nennen.

**Status, Pausieren, Speicherplatz**

- **FR-042**: holzi MUSS für jeden Ordner einen Zustand zeigen (aktuell, wird synchronisiert,
  pausiert, wartet auf ein Gerät, Problem), dazu laufende Übertragungen mit Datei, Fortschritt und
  Gegengerät, die Geräte, die den Ordner gebunden haben, und die Probleme einzelner Dateien mit
  Grund.
- **FR-043**: Wartet eine Datei auf Inhalt, den kein online befindliches Gerät hat, MUSS die
  Statusansicht das mit den Geräten zeigen, die ihn haben.
- **FR-044**: Der Nutzer MUSS den Sync je Ordner und für dieses Gerät als Ganzes pausieren und
  fortsetzen können. Pausieren hält für die betroffenen Ordner das Erfassen lokaler Änderungen, das
  Schreiben von Dateien auf die Platte und die Übertragung von Objekten an. Der Datensync von Spec
  024 läuft weiter, sodass Einträge des Dateiindex von anderen Geräten weiter ankommen und
  zusammen mit ihrem Änderungspaket atomar in den Dateiindex übernommen werden (Spec 024); auf der
  Platte umgesetzt werden sie erst nach dem Fortsetzen. Die Pausierung gilt nur für dieses Gerät,
  bleibt über das Schließen der Vault erhalten und DARF keine Änderung verlieren: Beim Fortsetzen
  erfasst das Gerät die zwischenzeitlichen lokalen Änderungen, setzt die zwischenzeitlich
  übernommenen Einträge auf der Platte um und behandelt Überschneidungen wie jeden anderen
  Konflikt (FR-030, FR-032).
- **FR-045**: holzi MUSS vor jedem Empfang prüfen, ob der Platz reicht, und DARF den freien Platz
  des Ziellaufwerks NICHT unter eine Reserve bringen. Eine Datei, die nicht passt, wird nicht
  begonnen und mit Grund gemeldet; andere Dateien laufen weiter.
- **FR-046**: Dateien, die auf einem Gerät nicht angelegt werden können (unzulässiger Name,
  Namenskollision, zu groß für das Dateisystem, fehlende Rechte), MÜSSEN dort als Problem erscheinen
  und DÜRFEN auf anderen Geräten keine Änderung auslösen.
- **FR-047**: Der Zusatzspeicher, den holzi für den Dateisync auf einem Gerät außerhalb der
  gebundenen Orte belegt, MUSS in der Statusansicht sichtbar sein.

**Gemeinsame Grundlage mit Spaces**

- **FR-048**: Ein synchronisierter Ordner MUSS dieselben Abläufe wie Spaces (Spec 027) für
  Dateiindex, Objekte, Übertragung und Konflikte nutzen, aber ohne eigenen Bereich: Der Dateiindex
  gehört zum Bereich „Vault“.
- **FR-049**: Jeder Eintrag des Dateiindex MUSS `created_by` und `modified_by` tragen und
  festhalten, auf welchem Gerät die letzte Änderung geschah. `created_by` DARF sich nach dem Anlegen
  nicht mehr ändern.
- **FR-050**: Zwischen eigenen Geräten DARF es keine Rechteprüfung für Dateien geben: Jedes
  gebundene Gerät hat `Lesen`, `Schreiben` und `Löschen`.

### Key Entities

- **Synchronisierter Ordner**: Name, Ausschlussmuster und Zustand „aktiv“ oder „beendet“. Gilt für
  die ganze Vault und reist mit dem Datensync von Spec 024.
- **Bindung**: synchronisierter Ordner, Gerät, Ort auf dem Gerät, Pausierung. Bleibt auf ihrem Gerät
  (FR-002).
- **Dateiindex-Eintrag**: Pfad, Größe, Änderungszeit, Inhalts-Prüfsumme, Verweis auf das Objekt,
  Schlüssel der Datei, `created_by`, `modified_by`, Gerät der letzten Änderung, Löschvermerk. Liegt
  im Bereich „Vault“.
- **Objekt**: verschlüsselter, unveränderlicher Inhalt einer Dateiversion, benannt nach der
  Prüfsumme des verschlüsselten Inhalts.
- **Konfliktkopie**: eigener Dateiindex-Eintrag mit eigenem Pfad nach FR-034.
- **Übertragung**: Objekt, Gegengerät, Fortschritt, empfangene und geprüfte Teile. Nur auf dem
  empfangenden Gerät.

## Success Criteria _(mandatory)_

### Measurable Outcomes

- **SC-001**: Eine neue oder geänderte Datei bis 10 MB steht auf einem anderen online befindlichen
  eigenen Gerät im selben lokalen Netz in 95 % der Fälle innerhalb von 15 Sekunden nach dem
  Speichern.
- **SC-002**: Eine Datei von 20 GB kommt vollständig und unverändert an, auch wenn die Übertragung
  dreimal an beliebiger Stelle abbricht; insgesamt werden dabei höchstens 5 % mehr als die
  Dateigröße übertragen.
- **SC-003**: In einer Testreihe mit gleichzeitigen Änderungen, gleichzeitigem Anlegen und Ändern
  gegen Löschen geht in 0 Fällen eine Fassung verloren; alle Geräte haben danach dieselben Dateien
  unter denselben Namen.
- **SC-004**: Gelöschte Dateien kehren in 0 Fällen zurück, auch wenn ein Gerät während der Löschung
  bis zu 30 Tage offline war.
- **SC-005**: Umbenennen oder Verschieben einer 1-GB-Datei überträgt weniger als 1 MB.
- **SC-006**: Ein Mitschnitt aller Daten, die ein Gerät für den Dateisync verschickt, enthält in 0
  Fällen einen Dateiinhalt oder Dateinamen im Klartext (geprüft mit einer eindeutigen Markierung in
  Inhalt und Namen).
- **SC-007**: Manipulierte oder beschädigte empfangene Daten landen in 0 Fällen im Ordner.
- **SC-008**: Ein abgezogenes Laufwerk oder ein verschwundener gebundener Ort führt in 0 Fällen zu
  Löschungen auf anderen Geräten.
- **SC-009**: Ein unveränderter Ordner mit 100 000 Dateien ist nach dem Öffnen der Vault auf einem
  üblichen Laptop mit SSD innerhalb von 60 Sekunden als aktuell erkannt, ohne Inhalte zu übertragen.
- **SC-010**: Eine Nutzerin bindet einen verfügbaren Ordner auf einem zweiten Gerät in weniger als
  einer Minute, ausgehend vom geöffneten Arbeitsbereich.
- **SC-011**: Der freie Platz eines Ziellaufwerks fällt durch den Dateisync in 0 Fällen unter die
  Reserve.

## Assumptions

- Die synchronisierten Dateien liegen in einem Ordner des Dateisystems, den der Nutzer auf jedem
  Gerät selbst wählt, wie bei Syncthing oder Dropbox (Empfehlung zur offenen Frage in FR-002).
  Andere Programme sehen und bearbeiten die Dateien dort direkt.
- Spec 024 liefert Vault-Identität, Geräteschlüssel, die Geräteliste, die direkte Verbindung
  zwischen eigenen Geräten und den Datensync der Vault. Diese Spec baut darauf auf und fügt keine
  eigene Anmeldung hinzu.
- Übertragen wird über iroh, Dateiinhalte als geprüfte, fortsetzbare iroh-Blobs (Betreibervorgabe).
  Wie Objekte zerlegt und verschlüsselt werden, legt der Plan fest; haex-vault 2.x (Legacy)
  (`https://github.com/haex-space/haex-vault-legacy` @ `8dce379d94e18fcd42c3b73686a06f984ca3f574`,
  `src-tauri/src/file_sync/crypto/envelope.rs`) dient als Vorbild, nicht als Vorlage.
- Ob ein Gerät Objekte zum Weitergeben verschlüsselt aufbewahrt oder bei Bedarf aus der lokalen
  Datei erzeugt, entscheidet der Plan. FR-020, FR-021 und FR-047 gelten in beiden Fällen.
- Ohne Relay (Spec 026) oder eigenes S3 (Spec 029) müssen zwei Geräte gleichzeitig online sein,
  damit Inhalte übertragen werden. Der Dateiindex selbst kann schon vorher angekommen sein.
- Die Höhe der Platzreserve und die Wartezeit nach FR-010 legt der Plan fest; die Reserve ist so
  bemessen, dass das System weiter arbeiten kann.
- Leere Unterordner werden als Einträge des Ordners mitgeführt.
- Der Papierkorb nach FR-028 ist der des Betriebssystems. Hat ein System keinen, wird die Datei
  gelöscht.
- Welche der Aktionen aus FR-007 Agenten freigegeben werden können, regelt die Agenten-Spec 021.
  Diese Spec gibt Agenten nichts frei.
- Mobile Geräte, die nur im Vordergrund laufen, synchronisieren nur, solange holzi offen ist
  (Entwurf §15 Punkt 9).

## Nicht im Umfang

- Objekte auf einem Relay (Spec 026) oder in einem eigenen S3-Speicher (Spec 029).
- Ordner mit anderen Nutzern teilen; das sind Spaces (Spec 027).
- Einen synchronisierten Ordner in einen Space umwandeln. Ob und wie das geht, entscheidet Spec 027.
- Ein Versionsverlauf älterer Fassungen oder eine Wiederherstellung gelöschter Dateien über den
  Papierkorb des Systems hinaus.
- Selektiver Sync innerhalb eines Ordners (nur einzelne Unterordner auf einem Gerät) und Dateien,
  die nur bei Bedarf geladen werden.
- Rechte, Eigentümer, erweiterte Attribute und Ausführbarkeit von Dateien; nur Inhalt, Name, Ort und
  Änderungszeit werden abgeglichen.
- Grenzen für Bandbreite oder Sync nur über bestimmte Netze; der Nutzer pausiert stattdessen
  (FR-044).
- Inhaltliches Zusammenführen von Konflikten; holzi legt Konfliktkopien an, der Nutzer entscheidet.
