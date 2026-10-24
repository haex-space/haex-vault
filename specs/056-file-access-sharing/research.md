# Research: Dateizugriff zwischen eigenen Geräten und Dateifreigaben an Kontakte

Alle Pfade relativ zu `src-tauri/src/`, Stand des Abzweigs von `main` am 2026-10-21 (`d91e9172`).
Abhängigkeiten: iroh 1.3.0, nostr 0.45.5 (nip44, nip59), nostr-sdk 0.45.4.

## R1 Ebene des Protokolls: Fensterpfade statt Objekt-Schlüssel

**Decision**: Das Protokoll `haex-vault-files/1` spricht in **Fensterpfaden** (`/a/b`) und Dateibrowser-
Begriffen (auflisten, Angaben, Bereich lesen, hochladen, Ordner anlegen, umbenennen, verschieben,
löschen). Das bedienende Gerät führt jede Anfrage über **dieselbe Schicht wie der Dateibrowser** aus:
`StorageFiles` mit dem Scope für verschlüsselte Ordner (`files/storage_source.rs:173`,
`files/encrypted/route.rs:67`). Das Gerät selbst ist dabei, wie schon für verschlüsselte Ordner auf dem
Gerät, ein `StorageFiles` über `LocalStore` (`files/encrypted/device.rs:32`, `DeviceRoot`).

**Rationale**: So entschlüsselt das bedienende Gerät verschlüsselte Ordner und spricht selbst mit
S3/WebDAV/SFTP (FR-012, FR-015, FR-043): Weder Chiffrat noch Zugangsdaten noch Schlüssel verlassen
es. Eine einzige Ausführungsschicht deckt Gerät, Speicher und verschlüsselte Ordner ab; die Sperre der
eigenen Daten von holzi (`OwnPlaces`, `files/local/places.rs:25`) und `has_raw_part`
(`files/encrypted/device.rs:128`) gelten an derselben Stelle.

**Eigene Daten von holzi** (Betreiber 2026-10-22): Für `Mode::OwnDevice` gelten dieselben Regeln wie
im lokalen Dateibrowser — sichtbar, nur lesend (`access::check` mit dem Aufrufer „Nutzer“ ergibt
`ReadOnly`, `files/access.rs:106`), für Agents gesperrt (`access::agent`, `:153`); der Aufrufer des
anfragenden Geräts wird dafür mitgeschickt (`User` oder `Agent`). Für `Mode::Contact` sind sie immer
gesperrt (`OwnPlaces::touches`), auch innerhalb einer Freigabe.

**Alternatives considered**: Ein Spiegel des Traits `ObjectStore` auf dem Draht (rohe Schlüssel).
Verworfen: Ein Kontakt bekäme bei einem verschlüsselten Ordner nur Chiffrat, und das bedienende Gerät
müsste Speicher roh herausreichen; die Prüfung der Wurzel wäre auf Schlüssel- statt Pfadebene.

## R2 Anfragende Seite: ein `ObjectStore` namens `PeerStore`

**Decision**: Beim anfragenden Gerät ist jedes andere eigene Gerät und jede empfangene Dateifreigabe
ein **Speicher** mit Namensraum-Kennung: `peer:<vault_device_uuid>` und `share:<received_id>`.
`browser_commands::storage_files` (`files/browser_commands/mod.rs:34`) löst diese Kennungen ohne
`haex_storages` auf und baut `StorageFiles::new(Arc<PeerStore>, access)` mit
`Protocol::Peer` (wie `Protocol::Local`: `#[serde(skip)] #[ts(skip)]`, von `Protocol::parse`
abgelehnt, `remote_storage/mod.rs:82`, `:208`). `Protocol::Peer.is_tree()` ist wahr. `PeerStore`
übersetzt jede Trait-Methode in eine Anfrage nach R1.

**Rationale**: Alles, was einen Speicher kann, kann damit auch ein anderes Gerät oder eine
Dateifreigabe, ohne die etwa 40 Stellen anzufassen, die über `SourceRef`, `Place`, `Side` und
`Target` verzweigen (Bestandsaufnahme der Recherche): Viewer mit Bereichs-Lesen (`StorageFileSource`,
`files/streaming.rs:65`), Transfers mit Zwischendatei und Abbruch (`files/transfer/remote*.rs`), Suche
(`files/browser_commands/search.rs:38`), Vorschaubilder, Agent-Berechtigung je Speicher
(`AgentFileKind::Storage`, `files/permissions.rs:277` prüft das Ziel nur auf „nicht leer“) und
FR-040/FR-041 ohne neue Berechtigungsart.

**Alternatives considered**: Neue Varianten `SourceRef::Peer`/`SourceRef::Share`. Verworfen: Sie
bräuchten jede Verzweigung neu und brächten keinen Unterschied im Verhalten.

**Folgen**: `Access` braucht `Credentials`; wie `local::access_of` (`remote_storage/local.rs:31`)
trägt `PeerStore` leere Zugangsdaten und kodiert das Ziel in `Location.endpoint`. Stellen, die eine
Speicher-Kennung in `haex_storages` erwarten (Einstellungen der Speicher, `remove_storage` der
Agent-Berechtigungen, Erweiterungs-Bereiche), MÜSSEN die Präfixe `peer:` und `share:` erkennen und
überspringen; Erweiterungen bekommen sie nie (Spec „Nicht im Umfang“).

## R3 Pfade für ein ganzes Gerät

**Decision**: Beim Geräte-Dateizugriff ist der Fensterpfad die Wurzel des Dateisystems in `/`-Form;
auf einem Windows-Gerät ist der erste Teil das Laufwerk (`/C:/Users/…`). Die Anfrage `roots` liefert
Laufwerke und bekannte Orte des bedienenden Geräts (dieselben Daten wie `files_sources` für das
Gerät, `files/browser_commands/navigate.rs:63`); die Seitenleiste zeigt sie unter dem Gerät. Bei einer
Dateifreigabe ist `/` die Wurzel der Freigabe; eine freigegebene Datei erscheint als einzige Datei
unter `/`.

**Rationale**: Ein Fensterpfad ist relativ zu einer Wurzel; das bedienende Gerät baut daraus
`DeviceRoot`-Speicher je Laufwerk bzw. einen Speicher mit der Wurzel der Freigabe und prüft
Einschluss mit `local_fs::resolve`/`resolve_below` (`remote_storage/local_fs.rs:17`, `:63`), die
`..`, symbolische Links nach außen und unter Windows `:` schon ablehnen (FR-006).

## R4 Handshake von `haex-vault-files/1`

**Decision** (angepasst 2026-10-23): Der erste Stream jeder Verbindung läuft durch
`sync::foreign_auth` (Spec 057, `sync/foreign_auth.rs`) mit
`Purpose { alpn: FILES_ALPN, domain: Domain::FilesAuth }`: beide Seiten senden `Hello` (Vault,
Geräteschlüssel, unterschriebene Geräteliste, Nonce) und `Proof` (Signatur über beide Nonces, beide
Endpoints, Vault und ALPN). Jede Seite prüft die andere: Liste von der Vault unterschrieben, Gerät mit
genau diesem Endpoint gelistet und nicht entfernt; für die eigene Vault entscheidet die eigene gültige
Liste. Ergebnis `Trust::OwnVault`, `Trust::Contact` (Karte gespeichert, nicht blockiert, Liste nicht
älter als die der Karte oder eine in dieser Sitzung gesehene, FR-003) oder `Trust::Unverified`.

Danach schickt das bedienende Gerät auf demselben Stream `Welcome { v, mode, trash }` oder schließt
die Verbindung ohne Grund:

- `OwnVault` und Schalter an → `Mode::OwnDevice`; Schalter aus → `Mode::AccessOff`.
- `Contact` und mindestens ein Zugriffsrecht auf eine Freigabe, die dieses Gerät bedient →
  `Mode::Contact { vault }`.
- sonst (auch `Unverified`) → schließen.

`SeenGenerations` lebt je Sitzung im Zustand des Servers bzw. Clients wie bei 057.

**Rationale**: Eine Prüfung fremder Geräte an genau einer Stelle (Constitution). Der Sync-Handshake
lehnt fremde Vaults bewusst ab (`RejectCode::ForeignVault`) und bleibt unberührt. Die eigene Tabelle
`haex_file_peer_lists_no_sync` entfällt (Betreiber 2026-10-23); die Grenze steht in den Edge Cases der
Spec.

**Alternatives considered**: Eigener Handshake mit dauerhafter Tabelle der höchsten Generation
(ursprünglicher Plan). Verworfen: zweite Umsetzung derselben Regel neben 057.

## R5 Server im Endpoint

**Decision** (angepasst 2026-10-23): `SyncNode::bind_with` und `Protocols` (Spec 057,
`sync/endpoint_protocols.rs`) registrieren `FILES_ALPN = b"haex-vault-files/1"` (`sync/wire.rs`,
Namensschema von Spec 054 wie `haex-vault-ext-peer/1`) neben `holzi-sync/1`. Der Handler
`FilesProtocol` bekommt `NodeTasks` (Tracker und Cancel-Token der Sitzung) und startet jede Verbindung
auf dem Tracker, wie `PeerProtocol` (`extensions/peers/admission.rs`), damit Sperren laufende Anfragen
beendet (FR-044). Ausgehend: `SyncNode::dial_alpn(addr, FILES_ALPN)`. `sync::service::run` baut die
Handler beider Specs in einem `Protocols`.

**Rationale**: iroh setzt die ALPNs aus allen registrierten Handlern; nach `spawn` kommt keiner dazu.
Der Baustein existiert seit 057.

## R6 Rahmen auf dem Draht

**Decision**: Ein QUIC-Bi-Stream je Anfrage. Kopf: Länge (u32 BE) + postcard, höchstens 64 KiB;
danach bei `read` die Bytes als Rohstrom bis `len`, bei Hochladen die Teile als Rohstrom mit Länge je
Teil. Antworten: Kopf + optional Rohstrom. Listen höchstens `MAX_ENTRIES` (100 000) Einträge, darüber
`TooLarge` wie bei Speichern. Je Verbindung höchstens 16 gleichzeitige Streams, je Gerät höchstens 64
(Schutz des bedienenden Geräts, SC-006).

**Rationale**: postcard ist im Sync-Code schon der Draht (`sync/wire.rs`); Streams sind in QUIC billig,
jede Anfrage bricht für sich ab. Der Rohstrom beim Lesen erlaubt Video ohne Puffern (SC-002).

## R7 Schreiben ohne halbe Dateien und mit Versionsstand

**Decision**: Hochladen über die Upload-Sitzung des Traits (`begin_upload`/`upload_part`/
`finish_upload`/`abort_upload`). `PeerStore::begin_upload` holt zuerst `head` des Ziels und schickt
den gesehenen **Versionsstand** (`Absent` oder `{ size, etag }`) mit; das bedienende Gerät prüft ihn
bei `finish` erneut und antwortet bei Abweichung `Conflict` (FR-036). Auf dem Gerät schreibt
`LocalStore` mit Zwischendatei und Umbenennen (`remote_storage/local_fs.rs:158-179`), in Speichern
und verschlüsselten Ordnern gelten deren Upload-Wege. `StorageError` und `FilesErrorCode` bekommen
`Conflict`; die Transfers bilden `Conflict` auf ihre Konfliktfrage ab (`Hooks.ask`, Ersetzen / Beide
behalten, FR-021): „Ersetzen“ wiederholt ohne Versionsstand, „Beide behalten“ lädt unter freiem Namen.

**Rationale**: Bearbeiten in holzi ist nicht im Umfang (044); Fremdschreiben trifft also Transfers und
Agents. Die Prüfung am Ende des Hochladens deckt das Fenster ab, in dem die Datei übertragen wird.
`ponytail:` Zwischen Konfliktfrage und `begin_upload` bleibt ein kurzes Fenster ohne Prüfung; die
Aufrüstung wäre ein Versionsstand im Trait selbst.

**Alternatives considered**: Versionsstand als neuer Parameter aller `ObjectStore`-Methoden.
Verworfen für diese Spec: berührt S3/WebDAV/SFTP ohne Nutzen für sie.

## R8 Löschen in den Papierkorb

**Decision**: Das bedienende Gerät löscht auf dem Gerät über `transfer::entries::remove(path,
Removal::Trash)` (`files/transfer/entries.rs:20`, wird `pub(crate)`), nicht über
`LocalStore::delete` (endgültig, `remote_storage/local.rs:151`). Hat das Gerät keinen Papierkorb
(Android), meldet `Challenge.trash = false`; das anfragende Gerät fragt dann vor dem Löschen
(FR-038), wie 044 FR-023 es lokal tut. Löschen in Speichern und verschlüsselten Ordnern folgt deren
Regeln.

## R9 Geräte eines Kontakts erreichen

**Decision** (geändert 2026-10-23, Betreiber): Das anfragende Gerät wählt jedes Gerät der
gespeicherten Visitenkarte des Kontakts direkt an, mit `endpoint_addr_of(&d.endpoint_id, d.iroh_relay,
&[])` (`sync/presence.rs`) und `dial_alpn(addr, FILES_ALPN)`, alle parallel; danach `foreign_auth`
wie bei eigenen Geräten (R4). Das ist der Weg von Spec 057 (`extensions/peers/invite.rs`: keine
Adresse von anderswo als der Karte). Antwortet nach 10 s kein Gerät, ist der Kontakt „nicht
erreichbar“ mit dem Hinweis, eine aktuelle Karte zu teilen. Ein Gerät ohne Zugriffsrecht für die
anfragende Vault schließt im Handshake ohne Grund (FR-009, schon gebaut).

**Folgen**: Hat ein Gerät des Eigentümers sein iroh-Relay seit der Karte gewechselt, bleibt es
unerreichbar, bis der Kontakt eine aktuelle Karte teilt. Wer eine Endpoint-ID kennt, erfährt, dass
das Gerät online ist, auch ohne Zugriffsrecht; die frühere Eigenschaft „antwortet nur Berechtigten“
entfällt.

**Rationale**: Derselbe Weg wie Spec 057, kein zweiter Mechanismus für dieselbe Frage, kein eigener
Nostr-Client, keine Drosselung und kein Replay-Schutz. Das Wählen geschieht weiter nur nach einer
Handlung des Nutzers (FR-028, FR-029).

**Alternatives considered**: _Anklopfen über Nostr_ (ursprüngliche Entscheidung): Gift-Wraps der
Präsenz mit den Arten 24103/24104 an die Geräteschlüssel der Karte; das Gerät antwortet nur
Berechtigten mit seiner aktuellen Adresse. Findet Geräte auch nach einem Relay-Wechsel und verrät
Unberechtigten nichts, braucht aber einen eigenen Nostr-Client, Frische- und Replay-Prüfung und
Drosselung, und 056 und 057 hätten zwei Wege. Verworfen. _Erst Karte, dann Anklopfen_: beste
Erreichbarkeit, größter Umfang, und der Vorteil des Schweigens entfällt trotzdem. Verworfen.

## R10 Dateifreigaben als Vault-Daten und der Zustand „wartet auf Gerät“

**Decision**: Drei synchronisierte Tabellen und eine lokale (data-model.md). Jede Änderung an einer
Dateifreigabe oder ihren Zugriffsrechten setzt `haex_file_shares.changed_at`; das bedienende Gerät
setzt `served_at = changed_at`, sobald es die Änderung über `AppState::vault_changes()`
(`vault_events.rs:42`) gesehen hat. „Änderung wartet auf ‹Gerät›“ heißt `changed_at > served_at`
(FR-026). Das bedienende Gerät prüft Zugriffsrechte bei jeder Anfrage gegen die Datenbank (FR-005),
nicht gegen einen Zwischenspeicher; ein Entzug bricht laufende Streams über ein Cancel-Token je
(Freigabe, Vault) ab (FR-024).

**Rationale**: Ohne Quittung kann ein anderes Gerät nicht wissen, ob der Entzug angekommen ist. Keine
UNIQUE-Bedingungen (CRDT-Regel, `identity/migrations_contacts.rs:6-10`); Eindeutigkeit über
abgeleitete IDs (UUIDv5).

## R11 Kontakte

**Decision**: Zugriffsrechte gelten nur, wenn zur Vault-Identität eine gespeicherte Visitenkarte
existiert und die Vault nicht blockiert ist (`contacts::card::received::stored`, `is_blocked`,
`card/received.rs:102`, `:165`) — geprüft bei jeder Anfrage (FR-023 wirkt also sofort). Zusätzlich
löscht ein Abonnent auf `haex_contacts_cards` verwaiste Zugriffsrechte, und der Löschdialog des
Kontakts nennt betroffene Dateifreigaben. Zugriff auf Kontaktdaten nur über eine neue Methode von
`ContactsService` mit `Caller::Internal { feature: "fileShares" }` (`contacts/service/mod.rs:1-8`).

## R12 Schalter „Geräte-Dateizugriff“

**Decision**: Eigene einzeilige Tabelle `haex_file_device_access_no_sync`, fehlend = an (FR-014). Nicht
`preferences` mit `PrefScope::Device`: Das ist CRDT-synchronisiert, und ein anderes eigenes Gerät
dürfte den Schalter des bedienenden Geräts nicht umlegen können (`storage/preferences.rs:24`). UI nach
`ScreenCaptureSetting.vue` in `FederationView.vue`, Aktion im Katalog mit `scope: 'guardrails'`
(nicht für Agents).

## R13 Seitenleiste und Quellen

**Decision**: `Sources` (`files/browser_commands/navigate.rs:37`) bekommt `devices: Vec<FilesPeerDevice>`
(Gerät, Name, Zustand) und `contacts: Vec<FilesContactShares>` (Kontakt, bekannte empfangene
Dateifreigaben). Zustände kommen aus der Präsenz (eigene Geräte) bzw. der letzten Abfrage beim
Kontakt. Klappt der Nutzer einen Kontakt auf, ruft das Fenster `files_contact_query` (FR-028); sonst
werden Geräte von Kontakten nicht angewählt (FR-029). Neue Typnamen mit Präfix `FilesPeer`/`FileShare` (ts-rs-Kollisionen).

## R14 ADR

**Decision**: ADR 0015 „Datei-Zugriff zwischen eigenen Geräten“ hält das akzeptierte Risiko fest
(Spec Assumptions), dazu, dass `haex-vault-files/1` fremde Vaults zulässt, `holzi-sync/1` nicht.

## R16 Live-Aktualisierung

**Decision**: Neue Anfrage `Watch { share?, path }` auf einem eigenen, lang laufenden Stream: Das
bedienende Gerät beobachtet den Ordner mit `local::watch::watch_folder` (`files/local/watch.rs:60`,
500 ms Entprellung; für verschlüsselte Ordner auf dem Gerät wie `files_watch` in
`files/browser_commands/navigate.rs:205`) und schickt je Stapel `FolderChanged { paths }` als
Fensterpfade; schließt der Stream, endet die Beobachtung. Beim anfragenden Gerät nimmt `files_watch`
auch `peer:`/`share:` an und reicht die Meldungen an denselben `Channel<FolderChanged>` weiter; das
Fenster (`src/composables/useFilesFolder.ts:67`) beobachtet diese Quellen wie das Gerät. Speicher-
Quellen beim Eigentümer (S3, WebDAV, SFTP) lehnen `Watch` mit `Unsupported` ab — lokal gibt es dort
auch keine Live-Aktualisierung. Jeder `Watch` zählt gegen die Stream-Grenzen (R6); ein Entzug beendet
ihn wie jeden Stream (FR-024).

**Rationale**: Der Watcher existiert und ist entprellt; eine Push-Meldung über den offenen Stream
braucht kein Abfragen im Takt und erfüllt SC-009 (≤ 2 s).

**Alternatives considered**: Regelmäßiges Neu-Auflisten. Verworfen: Last auf dem bedienenden Gerät
und Verzögerung bis zum nächsten Takt.

## R17 Agents verwalten Dateifreigaben

**Decision** (Betreiber 2026-10-22): Neue `AgentFileKind::FileShares` (`files/permissions.rs:28`),
Ziel leer, Status `granted`/`denied`, ohne Zeile = nicht erteilt; nur der Nutzer setzt sie
(Einstellungen der Agent-Berechtigungen). Die Aktionen `files.shares.*` werden für Agents aufrufbar
(eigener Scope `files.shares`); der Executor prüft die Berechtigung. Anlegen, Kontakt hinzufügen und
Stufe erhöhen öffnen **immer** einen Bestätigungsdialog im Fenster (Quelle, Gerät, Kontakt, Stufe), ohne
„merken“ und unabhängig von der Freigabestufe; ohne Fenster oder ohne Antwort in 120 s → abgelehnt.
Entziehen, Herabstufen und Löschen laufen ohne Rückfrage. Den Kontakt nennt der Agent über seine
Kontakte-Berechtigung (Spec 051, `AgentFileKind::Contacts`); der Dialog zeigt den aufgelösten Kontakt
mit Namen.

**Rationale**: Eine Freigabe gibt Daten nach außen und lässt sich nicht zurückholen; eine präparierte
Datei könnte einen Agent dazu anweisen. Entziehen verringert nur Zugriff und braucht keine Hürde.

**Alternatives considered**: Freigabestufe allein (im Modus „Auto“ ohne Rückfrage). Verworfen vom
Betreiber.

## R15 Testansatz

**Decision**:

- Unit: Einordnung im Handshake, Listenprüfung mit Generationen, Pfad-Einschluss (Sammlung aus `..`,
  Links, Verknüpfungen, Groß-/Kleinschreibung, Ziele von Verschieben, SC-005), Prüfung je Anfrage,
  Versionsstand, Rahmen (Kopf zu groß, Abbruch mitten im Rohstrom).
- Integration: zwei Vaults im Prozess mit `Member::genesis()` und Loopback-Endpoints
  (`sync/test_support.rs:110`, `sync/endpoint_tests.rs:18`), Kontakt-Geräte über die Adresse ihrer
  Karte; Gleichstand-Katalog des `ObjectStore`-Verhaltens auch gegen
  `PeerStore`.
- Live: Änderung im beobachteten Ordner kommt als `FolderChanged` an; Entzug beendet `Watch`.
- Agents: ohne Berechtigung abgelehnt; mit Berechtigung ohne Bestätigung keine Freigabe (SC-010),
  Entziehen ohne Dialog.
- E2E: Spec 033 um eine zweite Vault erweitert (eigene Geräte und Kontakt).

Keine Netzdienste in `cargo test`, keine Wartezeiten mit `sleep`.
