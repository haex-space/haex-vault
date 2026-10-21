# Research: Dateizugriff zwischen eigenen Geräten und Dateifreigaben an Kontakte

Alle Pfade relativ zu `src-tauri/src/`, Stand des Abzweigs von `main` am 2026-10-21 (`d91e9172`).
Abhängigkeiten: iroh 1.3.0, nostr 0.45.5 (nip44, nip59), nostr-sdk 0.45.4.

## R1 Ebene des Protokolls: Fensterpfade statt Objekt-Schlüssel

**Decision**: Das Protokoll `holzi-files/1` spricht in **Fensterpfaden** (`/a/b`) und Dateibrowser-
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

## R4 Handshake von `holzi-files/1`

**Decision**: Eigener Handshake auf dem ersten Stream jeder Verbindung, nach dem Vorbild von
`sync/handshake.rs`:

1. Anfragender → `Hello { v, vault, device_pubkey, list_payload, list_sig, nonce_r }`.
2. Bedienender → `Challenge { nonce_s, trash: bool }`.
3. Anfragender → `Proof { sig }` über `lp(nonce_r)‖lp(nonce_s)‖lp(own endpoint)‖lp(other endpoint)‖lp(vault)`
   mit dem Geräteschlüssel, neue Domäne `Domain::FilesAuth` (`sync/signing.rs`).
4. Bedienender → `Accept { mode }` oder `Reject` ohne Grund nach außen.

Prüfung beim bedienenden Gerät:

- Liste: `device_list::check_pushed(payload, sig, vault)` (`sync/device_list.rs:371`).
- Das Gerät steht auf der Liste, ist nicht entfernt, und `endpoint_id == connection.remote_id()`.
- **Eigene Vault**: Statt der vorgelegten Liste gilt die eigene gültige Liste (`valid_lists` +
  `effective`, `:238`, `:276`).
- **Fremde Vault**: Die Generation MUSS ≥ der höchsten gesehenen sein (FR-003, neue Tabelle
  `haex_file_peer_lists_no_sync`, data-model.md); eine höhere wird gespeichert.
- Die Signatur bindet den Geräteschlüssel an die QUIC-Gegenstelle wie in `handshake.rs:333-341`.

Die Einordnung nach FR-004 ergibt `Mode::OwnDevice` oder `Mode::Contact { vault }`.

**Rationale**: Der Sync-Handshake lehnt fremde Vaults bewusst ab (`RejectCode::ForeignVault`,
`handshake.rs:164`) und tauscht Änderungspakete; ihn zu öffnen hieße, die Grenze von 024 zu
verwischen. Der neue Handshake verwendet dessen Bausteine (Transkript, Domänen, Listenprüfung).

**Alternatives considered**: Vertrauen allein auf die Endpoint-ID aus der Visitenkarte. Verworfen:
Die Karte enthält die Geräteliste nicht und veraltet; ein neues Gerät des Kontakts käme nicht hinein,
ein entferntes nicht hinaus.

## R5 Server im Endpoint

**Decision**: `SyncNode::bind` (`sync/endpoint.rs:167`) nimmt optionale weitere Protokoll-Handler
und registriert `FILES_ALPN = b"holzi-files/1"` (`sync/wire.rs`) mit `.accept(FILES_ALPN, …)` am
selben `Router`. Der Handler `FilesProtocol` hat eigenen Zustand (Datenbank, Schlüssel, Vault,
`FilesState`, Speicherdienst, Cancel-Token) und startet jede Verbindung auf dem Tracker der Sitzung,
wie `SyncProtocol` (`:141-157`), damit Sperren laufende Anfragen beendet (FR-044). Ausgehend:
`SyncNode::dial_files(addr)` nach dem Muster von `dial_link` (`:261`).

**Rationale**: iroh setzt die ALPNs aus allen registrierten Handlern (`iroh-1.3.0/src/protocol.rs:512`);
ein Router kann nach `spawn` keine Protokolle mehr aufnehmen. Ein Endpoint je Sitzung bleibt.

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

## R9 Anklopfen über Nostr

**Decision**: Zwei neue innere Arten in denselben Gift-Wraps wie die Präsenz (`presence::wrap_payload`,
`unwrap_rumor`, `presence.rs:225`, `:283`; äußere Art 21059, flüchtig):

- **24103 Anklopfen**: an jeden Geräteschlüssel der Visitenkarte des Kontakts, auf dessen Relays
  (`ContactCard.nostr_relays`), Inhalt `{ v, vault, list_payload, list_sig, reply_relays, ts, nonce }`.
- **24104 Antwort**: an den Geräteschlüssel des Anklopfenden, auf `reply_relays`, Inhalt wie
  `LinkMeeting` (`sync/link/meeting.rs:27`): `{ v, endpoint, iroh_relay, addrs, ts, nonce }`.

Das Gerät des Eigentümers antwortet nur, wenn Siegel-Unterzeichner und vorgelegte Liste nach R4
gültig sind und es diesem Kontakt mindestens eine aktive Dateifreigabe bedient (FR-009). Frische wie
die Präsenz (150 s zurück, 30 s vor, `presence.rs:119`), Antworten je Vault höchstens alle 5 s.
Ein eigener Nostr-Client hört auf Gift-Wraps an den eigenen Geräteschlüssel; der Präsenz-Client
eignet sich nicht, weil `presence_loop::resubscribe` alle Abos beendet (`presence_loop.rs:208`). Er
läuft in `sync::service::run` mit (`sync/service.rs:164`) und übernimmt dessen Reconnect bei
geänderten Relays.

**Rationale**: Die Präsenz-Bausteine sind schon auf beliebige Empfänger ausgelegt (Empfänger ist ein
`PublicKey`-Parameter). Eigene Geräte brauchen kein Anklopfen; ihre Adressen kommen aus der Präsenz
(`endpoint.rs:347`).

## R10 Dateifreigaben als Vault-Daten und der Zustand „wartet auf Gerät“

**Decision**: Drei synchronisierte Tabellen und zwei lokale (data-model.md). Jede Änderung an einer
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
Dateifreigaben). Zustände kommen aus der Präsenz (eigene Geräte) bzw. dem letzten Anklopfen. Klappt der
Nutzer einen Kontakt auf, ruft das Fenster `files_contact_query` (FR-028); sonst wird nicht
angeklopft (FR-029). Neue Typnamen mit Präfix `FilesPeer`/`FileShare` (ts-rs-Kollisionen).

## R14 ADR

**Decision**: ADR 0015 „Datei-Zugriff zwischen eigenen Geräten“ hält das akzeptierte Risiko fest
(Spec Assumptions), dazu, dass `holzi-files/1` fremde Vaults zulässt, `holzi-sync/1` nicht.

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
  (`sync/test_support.rs:110`, `sync/endpoint_tests.rs:18`), Anklopfen über `MockRelay`
  (`presence_loop_tests.rs:104`); Gleichstand-Katalog des `ObjectStore`-Verhaltens auch gegen
  `PeerStore`.
- Live: Änderung im beobachteten Ordner kommt als `FolderChanged` an; Entzug beendet `Watch`.
- Agents: ohne Berechtigung abgelehnt; mit Berechtigung ohne Bestätigung keine Freigabe (SC-010),
  Entziehen ohne Dialog.
- E2E: Spec 033 um eine zweite Vault erweitert (eigene Geräte und Kontakt).

Keine Netzdienste in `cargo test`, keine Wartezeiten mit `sleep`.
