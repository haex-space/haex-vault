# Feature Specification: Versionsstände der Geräte in der Föderationsübersicht

**Feature Branch**: `059-device-versions`

**Created**: 2026-10-25

**Status**: Draft

**Input**: User description: „ich habe auf zwei geräten holzi im main checkout und laufen, aber die
vaults syncen nicht, weil sie angeblich nicht die gleiche version hätten“ und „ich möchte diese werte
gerne in der föderationsübersicht sehen, damit das besser erkennbar wird, welche vault hinterher hinkt“
(Betreiber, 2026-10-24/25). Entscheidungen des Betreibers: Jedes Gerät merkt sich die Werte, die es im
Handshake sieht, pro Gerät und nur lokal. Das Verbindungsformat wird dafür angepasst, weil es noch keine
Nutzer gibt und das jetzt sauber geht (2026-10-25): Beide Seiten nennen ihren Versionsstand, bevor eine
von ihnen entscheidet. Die Werte stehen immer bei jedem Gerät, auch bei „Dieses Gerät“; Abweichendes ist
markiert.

## Clarifications

### Session 2026-10-25

- Q: Soll die Agenten-Aktion `settings.devices.list` die drei Versionswerte eines Geräts auch liefern? →
  A: Ja. Die Aktion ist schreibgeschützt (`settings.read`) und liefert die Werte eines Geräts, oder gar
  keine, wenn sie unbekannt sind. Wer die Aktion nutzen darf, bestimmen weiter die Freigaben aus Spec 020/021;
  Erweiterungen bekommen die Werte nicht (FR-011).
- Q: Soll die rote Meldung unter einem Gerät sagen, welches Gerät aktualisiert werden muss, wenn die Werte das
  eindeutig zeigen? → A: Ja (FR-012). Zeigen alle abweichenden Werte in dieselbe Richtung, nennt die Meldung das
  Gerät; bei gegenläufigen oder unbekannten Werten bleibt der bisherige Text.
- Q: Soll die Liste bei den Werten eines Geräts dazuschreiben, wann sie zuletzt gesehen wurden? → A: Nein. Die
  Angabe „zuletzt online“ (Spec 024 FR-033) genügt; es wird kein eigener Zeitpunkt gespeichert.

## Begriffe

- **Versionsstand**: die drei Werte, die zwei Geräte beim Verbinden vergleichen und die alle
  übereinstimmen müssen, damit sie synchronisieren (Spec 024 FR-029):
  1. **Sync-Protokoll**: die Version der Sprache, in der die Geräte miteinander reden.
  2. **Vault-Schema**: wie viele Datenbank-Migrationen von holzi dieses Gerät kennt, also wie die
     Tabellen der Vault aufgebaut sind.
  3. **Änderungsverfolgung**: die Version der Regeln, nach denen holzi Änderungen für den Sync
     festhält.
- **Hinterher hinkendes Gerät**: das Gerät, dessen Wert kleiner ist als der des anderen. Es muss
  aktualisiert werden.

## Beziehung zu bestehenden Specs

- **Spec 024** (Eigener Geräte-Sync): FR-029 sagt, dass Geräte mit unverträglichen Versionen nicht
  synchronisieren und beide den Grund zeigen; FR-033 bis FR-035 beschreiben die Geräteliste. Diese Spec
  ergänzt die Liste um die Werte hinter dem Grund und ändert dafür den Beginn des Handshakes (FR-005).
  Die Prüfung selbst (alle drei Werte gleich) ändert sich nicht. Die Meldung „Eines der Geräte muss
  aktualisiert werden, sonst synchronisieren sie nicht.“ bleibt der Text, wenn die Werte keine eindeutige
  Richtung zeigen; sonst nennt sie das Gerät (FR-012).
- **Spec 023** (Einstellungen): Die Anzeige sitzt in der Kategorie „Föderation“, Unteransicht
  „Geräte“, im Block „Geräte der Vault“.
- **Spec 017** (Erweiterungs-Host): Die Versionsstände der Erweiterungen (T125, „Daten kommen teils
  erst nach seinem Update an“) bleiben, wie sie sind; hier geht es um den Stand von holzi selbst.

## User Scenarios & Testing _(mandatory)_

### User Story 1 - Sehen, welches Gerät hinterherhinkt (Priority: P1)

Mara hat holzi auf zwei Rechnern. Beide laufen, die Vaults synchronisieren nicht, und beim zweiten
Rechner steht rot „Eines der Geräte muss aktualisiert werden, sonst synchronisieren sie nicht.“ Mara
weiß nicht, welcher der beiden zu alt ist. Sie öffnet Einstellungen → Föderation → Geräte. Bei jedem
Gerät stehen jetzt Sync-Protokoll, Vault-Schema und Änderungsverfolgung. Beim zweiten Rechner ist das
Vault-Schema rot und kleiner als bei „Dieses Gerät“: Der zweite Rechner ist veraltet. Mara aktualisiert
ihn, und nach kurzer Zeit stehen die Werte gleich und nichts ist mehr rot.

**Why this priority**: Das ist der Anlass der Spec. Ohne die Werte weiß die Nutzerin nur, dass ein Gerät
aktualisiert werden muss, aber nicht welches.

**Independent Test**: Zwei Geräte derselben Vault, eines mit einem Stand, der sich in einem der drei
Werte unterscheidet. Auf dem aktuellen Gerät zeigt die Geräteliste beide Zeilen mit allen drei Werten,
der abweichende Wert ist markiert und als kleiner erkennbar. Nach dem Angleichen verschwindet die
Markierung ohne Neustart der Ansicht.

**Acceptance Scenarios**:

1. **Given** zwei Geräte mit unterschiedlichem Vault-Schema, **When** die Nutzerin auf dem Gerät mit
   dem höheren Wert die Geräteliste öffnet, **Then** stehen bei beiden Geräten alle drei Werte, und das
   Vault-Schema des anderen Geräts ist markiert und als das kleinere erkennbar.
2. **Given** dasselbe, **When** sie die Liste auf dem Gerät mit dem kleineren Wert öffnet, **Then** ist
   dort der Wert des anderen Geräts als der größere erkennbar; das Gerät selbst ist als hinterherhinkend
   erkennbar.
3. **Given** beide Geräte haben denselben Stand, **When** die Liste geöffnet wird, **Then** stehen die
   Werte da, und nichts ist markiert.
4. **Given** alle abweichenden Werte des anderen Geräts sind kleiner als die dieses Geräts, **When** die
   Liste die Versionsmeldung zeigt, **Then** nennt sie das andere Gerät beim Namen als das zu aktualisierende.
5. **Given** ein Gerät wird aktualisiert und verbindet sich danach, **When** die Liste offen ist,
   **Then** zeigt sie den neuen Stand ohne neu zu öffnen, und Meldung und Markierung verschwinden.

---

### User Story 2 - Den eigenen Stand kennen (Priority: P2)

Die Nutzerin will wissen, welchen Stand dieses Gerät hat, ohne dass es ein Problem gibt: etwa um einem
anderen Gerät zu sagen, welchen es braucht, oder um zu prüfen, dass ein Update angekommen ist. Die Zeile
„Dieses Gerät“ zeigt dieselben drei Werte.

**Why this priority**: Die Anzeige braucht den eigenen Stand als Maßstab für die Markierung, und er
hilft auch ohne Problem. Er kostet nichts, weil er schon bekannt ist.

**Independent Test**: Die Geräteliste zeigt bei „Dieses Gerät“ die drei Werte, auch in einer Vault mit
nur einem Gerät.

**Acceptance Scenarios**:

1. **Given** eine Vault mit nur diesem Gerät, **When** die Geräteliste offen ist, **Then** stehen bei
   „Dieses Gerät“ die drei Werte, und es gibt keine Markierung.
2. **Given** holzi wurde aktualisiert und neu gestartet, **When** die Liste geöffnet wird, **Then**
   zeigt „Dieses Gerät“ den neuen Stand.

---

### User Story 3 - Gerät, dessen Stand noch unbekannt ist (Priority: P3)

Ein Gerät wurde gerade verknüpft oder war noch nie in einer Verbindung zu diesem Gerät. Die Liste
erfindet keine Werte: Bei diesem Gerät stehen keine, und es ist klar, dass sie noch nicht bekannt sind
(nicht, dass sie leer wären).

**Why this priority**: Ohne diese Regel läse die Nutzerin fehlende Werte als „alles gleich“ oder als
Fehler.

**Independent Test**: Ein verknüpftes Gerät, das dieses Gerät noch nie erreicht hat, steht in der Liste
ohne Werte und mit einem Hinweis, dass der Stand noch nicht bekannt ist.

**Acceptance Scenarios**:

1. **Given** ein Gerät, mit dem dieses Gerät nie verbunden war, **When** die Liste es zeigt, **Then**
   stehen keine Werte da, aber ein Hinweis „Stand noch nicht bekannt“.
2. **Given** dasselbe Gerät verbindet sich später, **When** die Liste offen ist, **Then** erscheinen
   seine Werte.

---

### Edge Cases

- Ein Gerät mit einem älteren Sync-Protokoll, das den neuen Beginn des Handshakes nicht kennt: Es
  nennt seinen Stand nach altem Muster in seiner Antwort und lehnt die neue Frage mit der bekannten
  Versionsmeldung ab; ruft dieses Gerät es an, versteht es dessen Beginn nicht. Wo ein Gerät seinen
  Stand nicht nennen konnte, steht „Stand noch nicht bekannt“ plus die bestehende Meldung (FR-007).
  Zwischen zwei Ständen, die beide den neuen Beginn kennen, ist das nie der Fall.
- Zwei Geräte unterscheiden sich in mehr als einem Wert, mit gegenläufiger Richtung (eines hat das
  größere Schema, das andere die größere Änderungsverfolgung): Jeder abweichende Wert ist einzeln
  markiert und als größer oder kleiner erkennbar; die Liste sagt nicht „das eine ist neuer“.
- Ein Gerät wurde aktualisiert, hat sich aber noch nicht wieder verbunden: Es zeigt weiter den zuletzt
  gesehenen Stand, der der Wirklichkeit hinterherhinkt, und die Meldung nach FR-012 nennt es weiter als das zu
  aktualisierende. Einen eigenen Zeitpunkt für die Werte gibt es nicht; wie lange der Stand zurückliegt, ergibt
  sich aus „zuletzt online“ (Spec 024 FR-033). Erst die nächste Verbindung ersetzt die Werte.
- Das Gerät, das gerade geprüft wird, ist der Rechner mit dem älteren Stand: Die Markierung vergleicht
  immer mit dem Stand dieses Geräts, nicht mit dem höchsten der Vault.
- Eine Vault im sperrenden oder schließenden Zustand: Nichts davon ändert sich; die Werte gehören zum
  Gerätestatus, den Spec 013 beim Sperren ohnehin beendet.
- Kleiner Bildschirm (Telefon): Drei Werte pro Zeile umbrechen lesbar unter den Namen, ohne dass die
  Zeile seitwärts scrollt.

## Requirements _(mandatory)_

### Functional Requirements

- **FR-001**: Die Geräteliste (Spec 024 FR-033) MUSS bei jedem Gerät den Versionsstand zeigen:
  Sync-Protokoll, Vault-Schema und Änderungsverfolgung, jeweils als Zahl, jeweils mit einer Beschriftung,
  die ohne Vorwissen verständlich ist.
- **FR-002**: Bei „Dieses Gerät“ MUSS die Liste den Stand des laufenden holzi zeigen, auch wenn die
  Vault kein weiteres Gerät hat.
- **FR-003**: Jeder Wert eines anderen Geräts, der vom Wert dieses Geräts abweicht, MUSS markiert sein
  und als größer oder kleiner als der eigene erkennbar sein. Die Markierung darf nicht allein von der
  Farbe abhängen (Spec 036: lesbar auch ohne Farbunterschied).
- **FR-004**: Jedes Gerät MUSS die Werte, die es im Handshake von einem anderen Gerät sieht, für dieses
  Gerät festhalten, und zwar nur auf diesem Gerät. Sie DÜRFEN NICHT synchronisiert und NICHT in eine
  Vault-Datei geschrieben werden, die auf ein anderes Gerät kopiert wird (Spec 024: `_no_sync`).
- **FR-005**: Im Handshake MÜSSEN beide Seiten ihren Versionsstand nennen, bevor eine von ihnen
  annimmt oder ablehnt: das annehmende Gerät schon in seiner ersten Nachricht, das anrufende in seiner
  Antwort. So kennt jedes Gerät bei jedem Verbindungsversuch den Stand des anderen, auch wenn der
  Versuch an der Verträglichkeit scheitert. Das Sync-Protokoll bekommt dafür eine neue Version; die
  Verträglichkeitsprüfung (Spec 024 FR-029: alle drei Werte gleich) und der Sync gleicher Stände ändern
  sich nicht, und die Versionsmeldung als Ergebnis bleibt dieselbe (FR-012 ändert nur ihren Text).
- **FR-006**: Die Werte eines Geräts MÜSSEN bei jeder Verbindung mit ihm neu festgehalten werden, auch
  wenn sie sich nicht geändert haben, und sie MÜSSEN die alten ersetzen. Wird ein Gerät aktualisiert,
  zeigt die Liste nach der nächsten Verbindung den neuen Stand (und die Versionsmeldung entfällt wie
  bisher).
- **FR-007**: Kennt dieses Gerät die Werte eines Geräts nicht, MUSS die Liste das sagen („Stand noch
  nicht bekannt“) und darf keine Werte zeigen.
- **FR-008**: Die Liste MUSS sich live aktualisieren (Spec 024 FR-034), wenn sich Werte eines Geräts
  ändern.
- **FR-009**: Die Anzeige MUSS auf allen Plattformen funktionieren, auf Desktop und Telefon, und die
  Zeile darf auf einem schmalen Bildschirm nicht seitwärts scrollen.
- **FR-010**: Alle neuen Texte MÜSSEN auf Deutsch und Englisch vorliegen. Die Texte sagen „Gerät“ und
  „Vault“, nennen nie ein „Relay“ allein, und der Produktname in Texten ist „HaexVault“.
- **FR-011**: Die Werte MÜSSEN der Agenten-Aktion `settings.devices.list` zur Verfügung stehen (nur
  Lesen, wie die übrigen Angaben dort: ohne Werte bei einem Gerät, dessen Stand unbekannt ist). Sie DÜRFEN
  über die Erweiterungs-Schnittstelle (Spec 017) und über keine andere Aktion zugänglich werden.
- **FR-012**: Die Versionsmeldung eines Geräts MUSS sagen, welches Gerät aktualisiert werden muss, wenn alle
  abweichenden Werte in dieselbe Richtung zeigen: sind sie beim anderen Gerät alle kleiner, nennt sie dieses
  („<Name> muss aktualisiert werden, sonst synchronisieren sie nicht.“; ein Gerät ohne Namen heißt wie in der
  Liste „Unbenanntes Gerät“); sind sie alle größer, nennt sie „Dieses Gerät“. Bei gegenläufigen Werten oder wenn
  der Stand des anderen Geräts unbekannt ist, bleibt der bisherige Text „Eines der Geräte muss aktualisiert werden,
  sonst synchronisieren sie nicht.“

### Key Entities

- **Versionsstand**: die drei Zahlen Sync-Protokoll, Vault-Schema, Änderungsverfolgung; pro Gerät
  höchstens einer, der zuletzt gesehene.
- **Gerät in der Liste**: bekommt zu den bestehenden Angaben (Name, Rolle, online, zuletzt online,
  Problem) den Versionsstand dazu, oder die Angabe, dass er unbekannt ist.

## Success Criteria _(mandatory)_

### Measurable Outcomes

- **SC-001**: Hat ein Gerät eine Versionsmeldung, kann die Nutzerin in unter 10 Sekunden sagen,
  welches Gerät aktualisiert werden muss, ohne etwas anderes zu öffnen.
- **SC-002**: Nach dem Aktualisieren eines Geräts zeigt die Liste auf dem anderen Gerät den neuen Stand
  innerhalb von 60 Sekunden nach der nächsten Verbindung, ohne dass die Ansicht neu geöffnet wird.
- **SC-003**: Der Sync von Geräten mit gleichem Stand und die Meldung bei unverträglichem Stand
  verhalten sich in allen Tests von Spec 024 wie vorher; kein Test dort ändert sich außer um die
  zusätzlichen Werte und den neuen Beginn des Handshakes.
- **SC-005**: Nach jedem Verbindungsversuch zwischen zwei Geräten mit neuem Beginn, auch einem an der
  Verträglichkeit gescheiterten, kennt jedes die Werte des anderen: in 100 % der Versuche.
- **SC-004**: Auf einem Telefon bleibt jede Zeile der Liste ohne Seitwärtsscrollen lesbar.

## Assumptions

- Der Versionsstand eines Geräts ist für die Nutzerin nicht geheim; er sagt nichts über Inhalte der
  Vault. Er bleibt trotzdem auf dem Gerät, das ihn gesehen hat (FR-004).
- Heute liefert der Handshake dem annehmenden Gerät den Stand des Anrufers immer, bevor es entscheidet;
  das anrufende Gerät erfährt den Stand des Angerufenen aber erst, wenn dieser annimmt, also nur bei
  gleichem Stand. Deshalb ändert FR-005 den Beginn des Handshakes. Es gibt noch keine Nutzer und keine
  Vaults, die auf einem älteren Stand weiterlaufen müssen; Entwicklungsgeräte werden gemeinsam
  aktualisiert.
- Die Anzeige liest, was der Handshake liefert; den Wert „Sync-Protokoll“ bringt dieselbe Nachricht mit.
  Ob die ALPN (`holzi-sync/1`) mit dem Protokoll mitzählt, entscheidet der Plan (Spec 024 research R14);
  dort ist abzuwägen, dass ein Gerät mit anderer ALPN gar keinen Handshake erreicht und damit auch nicht
  zeigen kann, welche Werte abweichen.
- Es gibt keine Beschriftung „aktuell“ oder „veraltet“ pro Gerät, nur pro Wert größer oder kleiner,
  weil die drei Werte unabhängig sind (siehe Randfall mit gegenläufiger Richtung).
- Die Zahlen sind die, die holzi heute vergleicht; sie ändern sich nur durch Updates von holzi.
- Wie die Werte gespeichert und gelesen werden (neue Spalte, Befehl `list_vault_devices`) gehört in
  den Plan; der Vertrag `VaultDevice` in `contracts/tauri-commands.md` von Spec 024 wird dabei erweitert.
- Eine Warnung beim Verknüpfen eines Geräts mit abweichendem Stand ist nicht Teil dieser Spec.
