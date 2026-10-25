# Research: Versionsstände der Geräte in der Föderationsübersicht

Spec: [spec.md](spec.md). Ausgangslage im Code (Stand `origin/main` 5ae5009a): `handshake::local_schema()`
liefert `SchemaVersion { protocol, holzi_migration, crdt_trigger }`; `accept_with` vergleicht es mit dem
`schema` aus `Response` (`handshake.rs`, Prüfung `v != PROTOCOL_VERSION || schema != local.schema`), `dial_with`
mit dem `schema` aus `Accept`. `Accept` kommt nur bei Gleichheit, bei Abweichung kommt `Reject { Incompatible }`
ohne Werte. `endpoint_devices::note_failure` setzt dann `problem = incompatible_version`.

## R1 Wo der Stand des Angerufenen herkommt

**Entscheidung**: `Message::Challenge` bekommt als letztes Feld `schema: SchemaVersion`. Das annehmende Gerät
nennt seinen Stand damit in seiner ersten Nachricht, das wählende in `Response` (unverändert). Beide Seiten
kennen den Stand des anderen, bevor sie entscheiden, und halten ihn fest, auch wenn sie danach ablehnen.

**Begründung**: Die kleinste Änderung, die FR-005 erfüllt. `Reject` mit Werten zu erweitern hieße, dass die
Werte nur im Fehlerfall fließen und der Erfolgsfall (Anzeige „alles gleich“) einen zweiten Weg braucht
(`Accept.schema`). Mit `Challenge` gibt es einen Weg für beide Fälle. Die Werte sind keine Geheimnisse; die
Spec 024 FR-009 („vor `Accept` fließt nur die Geräteliste“) wird dafür um die drei Zahlen ergänzt, weil sie
nichts über Inhalte der Vault sagen und ein Gerät, das den Handshake erreicht, ohnehin die Vault-Kennung
(`vault`) und die Gerätelisten-Referenz sieht.

**Verworfen**: `Reject { code, schema }` (zwei Wege, Erfolgsfall braucht trotzdem `Accept.schema`);
Versionsstand als eigener zusätzlicher Rahmen vor `Challenge` (eine Nachricht mehr, keine Information mehr);
den Stand über die Präsenzmeldung (Nostr, Art 21059) mitschicken (Meldungen sind nicht authentifiziert gegenüber
dem Handshake und erreichen nicht jedes Gerät).

## R2 Protokollversion, ALPN und ältere Stände

**Entscheidung**: `PROTOCOL_VERSION` wird 2. Die ALPN bleibt `holzi-sync/1`. `v` bleibt das erste Feld und damit
das Byte direkt nach dem Variantenkennzeichen jeder Handshake-Nachricht (u8, postcard schreibt es als ein Byte; `Challenge` ist Variante 0). Das wählende Gerät liest die
`Challenge` als Rahmen, prüft dieses Byte gegen `PROTOCOL_VERSION` und dekodiert erst dann; bei Abweichung
endet es mit `Refused(Incompatible)` und nennt das Gerät über den Endpunkt (R3), ohne Werte. Das annehmende Gerät
liest `Response` wie bisher vollständig: Das Layout von `Response` ändert sich nicht, ein Stand von vorher
liefert `v = 1` und sein `schema`, wird festgehalten und mit `Incompatible` abgelehnt.

**Begründung**: Die ALPN trennt Geräte auf Transportebene; ein Gerät mit anderer ALPN erreicht gar keinen
Handshake, hinterlässt also weder Werte noch Meldung. Genau das soll die Anzeige vermeiden. Postcard
kennt keine optionalen Endfelder, ein älteres wählendes Gerät dekodiert die neue `Challenge` aber, weil
postcard überzählige Bytes am Ende nicht beanstandet (`postcard::from_bytes` liefert den Rest zurück), liest
`v = 2` und lehnt sich selbst ab. Das neue Gerät muss das Versionsbyte prüfen, bevor es dekodiert, weil eine
alte `Challenge` sonst als Dekodierfehler endet statt als Versionsmeldung. Spec 024 R14 („Protokollversion
steckt im ALPN“) wird dahin geändert: Die ALPN ändert sich nur bei einem Bruch, der den Handshake selbst
unlesbar macht; die Protokollversion `v` direkt nach dem Variantenkennzeichen ist die Versionsprüfung.

**Verworfen**: ALPN `holzi-sync/2` (zwei alte Stände sind dann füreinander unsichtbar, siehe oben);
`PROTOCOL_VERSION` bei 1 lassen (der Wert „Sync-Protokoll“ in der Anzeige wäre gleich, obwohl das Format
bricht; ein älteres Gerät würde die Nachricht lesen und sie stillschweigend falsch deuten, wenn sich ein
Feld verschiebt).

**Prüfung** (Aufgabe T-Spike): Test, dass `postcard::from_bytes::<AlteChallenge>` auf den Bytes der neuen
`Challenge` gelingt und `v = 2` liefert; sonst gilt der Rückfall „nur das Versionsbyte prüfen“ auch für die
Alt-Seite nicht, und die Spec-Randfälle werden angepasst.

## R3 Welches Gerät die Werte bekommt

**Entscheidung**: Beide Seiten bestimmen das Gerät über den Endpunkt der Verbindung
(`problems::device_at_endpoint` gegen die geltende, von der Vault signierte Geräteliste), nicht über das, was die
Gegenseite behauptet. Ein Endpunkt, den die Liste nicht nennt, bekommt keine Werte (auch kein
`device_presence_no_sync`-Zeilchen für ein fremdes Gerät).

**Begründung**: Das wählende Gerät kennt den Geräteschlüssel des Angerufenen vor `Accept` noch nicht, den
Endpunkt aber vom Transport. `note_failure` löst bei `RefusedByPeer` schon heute so auf. Ein Weg für beide
Seiten ist eine Umsetzung der Regel (Spec-Leitsatz „eine Umsetzung je Regel“).

## R4 Wo die Werte liegen

**Entscheidung**: Drei Spalten in `device_presence_no_sync`: `peer_protocol`, `peer_migration`, `peer_trigger`
(`INTEGER`, alle `NULL` = unbekannt), angelegt in der Migration `0040_device_peer_schema`. Ein Schreiben ersetzt
alle drei in einem Statement; `problems::set` und `seen::touch` lassen sie unberührt. `removal.rs` löscht die
Zeile eines entfernten Geräts wie bisher. Die Migration berührt keine CRDT-Tabelle, `HOLZI_TRIGGER_VERSION`
bleibt 27; die Zahl der Migrationen steigt auf 40.

**Begründung**: Die Tabelle ist gerätelokal (`_no_sync`), hat genau einen Schreiber-Pfad je Zugriff
(`problems`, `seen`, `presence::record_seen`) und eine Zeile je Gerät. Eine eigene Tabelle brächte einen
Join ohne Gewinn. Drei `INTEGER` statt eines BLOBs, damit Fehlerfälle in SQL prüfbar bleiben.

**Verworfen**: eigene Tabelle `device_schema_no_sync`; ein JSON-Wert in einer Textspalte.

## R5 Wann die Werte geschrieben werden und wie die Liste es erfährt

**Entscheidung**: Der Handshake meldet jede gesehene `SchemaVersion` über einen Rückruf an den Knoten
(wie `on_applied`), und zwar sobald er sie kennt: das annehmende Gerät nach der Prüfung von Vault-Kennung und
Signatur (nicht für unbekannte Vaults), das wählende nach der Prüfung von `v` und `endpoint_a`. Der Knoten
löst das Gerät auf (R3), schreibt die Werte und ruft bei Änderung `notify_devices_changed`, das bestehende
Ereignis `sync-devices-changed` (FR-008). Geschrieben wird bei jedem Handshake, ein Schreiben ohne Änderung
löst kein Ereignis aus (wie `problems::set`).

**Begründung**: FR-006. Das Ereignis gibt es schon; die Liste lädt neu (`list_vault_devices`).

## R6 Anzeige

**Entscheidung**: `VaultDevice` bekommt `schema: DeviceSchema | null` mit `{ protocol, holziMigration,
crdtTrigger }` (`null` = unbekannt) und `ownSchema` wird nicht je Zeile wiederholt: die Zeile mit `isCurrent`
trägt den eigenen Stand (aus `local_schema()` gelesen, nicht gespeichert), die Oberfläche vergleicht jede andere
Zeile damit. Die Logik des Vergleichs liegt rein in `src/lib/sync/deviceStatus.ts` (`schemaDiff`): je Wert
`equal | lower | higher`. `DeviceRow.vue` zeigt eine Zeile `Sync-Protokoll 2 · Vault-Schema 39 · Änderungsverfolgung
27`; abweichende Werte tragen zusätzlich zum Rot ein Zeichen (`↓`/`↑`) und einen Lesetext
(`aria-label`: „niedriger als dieses Gerät“, „höher als dieses Gerät“). Auf einem schmalen Bildschirm bricht die
Zeile um (Werte in `flex-wrap`).

**Begründung**: Spec FR-001 bis FR-003, FR-009. Der eigene Stand liegt in der Antwort, damit die Oberfläche
nicht einen zweiten Befehl braucht.

**Verworfen**: Vergleich im Backend (ein Wert „lagging“ je Gerät; die drei Werte laufen unabhängig,
siehe Randfall der Spec).

## R6a Meldung und Agenten-Aktion (Klärung 2026-10-25)

**Entscheidung**: Die Meldung wird in `deviceStatus.ts` entschieden, rein und testbar: `problemMessage(problem,
diff, name)` liefert einen von drei Schlüsseln: `incompatible_version_other` („{name} muss aktualisiert werden,
sonst synchronisieren sie nicht.“, wenn alle abweichenden Werte des anderen Geräts kleiner sind),
`incompatible_version_this` („Dieses Gerät muss aktualisiert werden, …“, alle größer), sonst der bisherige
`incompatible_version` („Eines der Geräte …“). Ohne Stand des anderen Geräts oder bei gegenläufigen Werten der
bisherige Text. In den Meldungen steht `{name}` als Platzhalter (vue-i18n lehnt `<…>` ab); ein Gerät ohne Namen
bekommt `settings.federation.unnamed`. `settings.devices.list` (`settingsActionHandlers.ts`) liefert `schema`
(`protocol`, `holziMigration`, `crdtTrigger`) je Gerät, bei unbekanntem Stand ohne das Feld, wie bei `lastSeen`; die
Beschreibung in `settingsActions.ts` nennt es. Eine Aktion für Erweiterungen kommt nicht dazu.

**Begründung**: FR-011, FR-012. Dieselbe Funktion bedient Anzeige und Tests; der Agent liest dieselben Zahlen wie
die Nutzerin.

**Kein eigener Zeitpunkt** für die Werte (Klärung 3): die Zeile nennt „zuletzt online“ ohnehin.

## R7 Tests

**Entscheidung**:

- Rust: `handshake_schema_tests.rs`: beide Seiten kennen nach einem angenommenen Handshake und nach einer
  `Incompatible`-Ablehnung die Werte des anderen; wählendes Gerät lehnt sich bei einer alten `Challenge` (ohne
  `schema`, nur das Versionsbyte lesbar) mit `Incompatible` ab; `peer_schema_tests.rs`: Speichern, Ersetzen, `NULL` bei
  nie gesehen, Zeile weg beim Entfernen; `device_view_tests.rs` erweitert um `schema`.
- TypeScript: `scripts/check-settings.ts` (läuft mit `pnpm check:settings`) für `schemaDiff` (gleich, kleiner, größer, unbekannt, gegenläufig) und `problemMessage` (drei Texte); `scripts/check-agent-actions.ts`, falls dort die Felder von `settings.devices.list` geprüft werden.
- E2E: die bestehende Szene `settings-federation` (eine Installation) prüft, dass „Dieses Gerät“ die drei Zahlen
  zeigt und nichts markiert ist. Ein zweites Gerät oder ein unverträglicher Stand lässt sich dort nicht erzeugen
  (beide Instanzen wären derselbe Build); Abweichung und „unbekannt“ decken die Rust-Tests und `check-settings.ts` ab.
- Gesamtlauf nur gezielt (`cargo test -j 4 --lib sync::`, die betroffenen Testbinaries), wie im Projekt üblich.

## R8 Offene Punkte, die nicht in diese Spec gehören

Der Stand von Erweiterungen (T125) und eine Warnung beim Verknüpfen eines Geräts mit abweichendem Stand bleiben
außen vor, wie in der Spec festgehalten.
