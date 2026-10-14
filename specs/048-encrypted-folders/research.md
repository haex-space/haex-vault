# Research: Verschlüsselte Ordner in Speichern (048)

Stand 2026-10-10. Zitate aus holzi beziehen sich auf den Stand von `main` bei `43b481d4`; Zitate aus
haex-vault auf Repository `https://github.com/haex-space/haex-vault`, Revision
`8dce379d94e18fcd42c3b73686a06f984ca3f574`, Pfade unter `src-tauri/src/file_sync/`.

## R1 — Woher der Schlüssel der Vault kommt

- **Decision**: Der Schlüssel, der Ordnerschlüssel verpackt (KEK), wird mit HKDF-SHA256 aus dem
  **Inhaltsschlüssel der Vault** (`sync::content_keys`, Bereich „vault“) abgeleitet, Info
  `holzi/encrypted-folder/kek/v1`. Der Kopf eines Ordners nennt die `key_id` der Generation, mit der er
  verpackt ist. Neue Ordner nutzen `current_key`, Leser suchen die Generation über `key_id` in allen
  Generationen des Geräts (`vault_content_keys_no_sync`, wie `envelopes::held_keys`, ohne Grenze).
  `listening_keys` taugt dafür nicht: es liefert für die Presence nur bis zu `limit` Generationen je
  Richtung und keine `key_id`. holzi löscht heute keine Generation aus `vault_content_keys_no_sync`;
  ein späteres Aufräumen alter Generationen MUSS Ordner berücksichtigen, die noch mit ihnen verpackt
  sind (sonst werden sie unlesbar).
- **Rationale**:
  - Das Identitätsgeheimnis der Vault (`vault_identity_secret_no_sync`, `src-tauri/src/sync/keys.rs:3-5`,
    `:169`) liegt nach D27 nur auf Hauptgeräten
    ([`docs/plans/2026-09-28-sync-architecture.md`](../../docs/plans/2026-09-28-sync-architecture.md)
    Zeile 77). Ein Nebengerät könnte damit keinen Ordner lesen (Widerspruch zu FR-007).
  - Den Inhaltsschlüssel hat jedes Gerät: Hauptgeräte verpacken ihn je Gerät in
    `vault_key_envelopes`, jedes Gerät entpackt seinen Umschlag nach
    `vault_content_keys_no_sync` (`src-tauri/src/sync/content_keys.rs:1-16`, `:315`, `:389`). Beim
    Verbinden bekommt ein neues Gerät alle Generationen (`src-tauri/src/sync/link/host.rs:199-235`).
  - Nach D30 sind Inhaltsschlüssel gewöhnliche, wiederherstellbare Vault-Daten; ein Backup der Vault
    reicht also, um verschlüsselte Ordner zu lesen.
  - Eigene HKDF-Info trennt die Verwendung von den vorhandenen (`holzi/vault-identity/v1`,
    `holzi/device-name/v1`, `holzi/presence/v1`, `holzi/link/*`).
- **Folge für die Spec**: Die Annahme „gemeinsames Geheimnis der Vault“ ist damit erfüllt, aber mit
  Generationen: Entfernt der Nutzer ein Gerät, entsteht eine neue Generation (D22). Bestehende Ordner
  bleiben über ihre alte Generation lesbar; ein entferntes Gerät, das einen Ordnerschlüssel schon
  kannte, behält ihn (Entzug ist nicht im Umfang). Die Spec wird um diesen Randfall ergänzt.
- **Alternatives considered**:
  - Identitätsgeheimnis wie haex-vault (`vault_key_derivation.rs:52-57`): nur auf Hauptgeräten.
  - Eigener, synchronisierter Dateischlüssel in einer neuen Tabelle: zweiter Schlüsseltausch neben
    `vault_key_envelopes`, verstößt gegen „eine Umsetzung je Regel“.
  - Köpfe bei jeder neuen Generation neu verpacken: bringt ohne Entzug nichts, weil entfernte Geräte
    den Ordnerschlüssel schon kennen; gehört zum späteren Entzug.

## R2 — Verfahren und Krypto-Bibliotheken

- **Decision**: XChaCha20-Poly1305 aus `chacha20poly1305 0.10` (schon Abhängigkeit,
  `src-tauri/Cargo.toml:123`), HKDF-SHA256 aus `hkdf 0.13` und `sha2 0.11`, Zufall über
  `keys::random_bytes` (`src-tauri/src/sync/keys.rs:27`), `zeroize` für Schlüssel. Keine neue
  Krypto-Abhängigkeit; Prüfsumme des Klartexts als SHA-256 statt BLAKE3.
- **Rationale**: Dieselben Bausteine nutzt holzi schon für Gerätenamen
  (`src-tauri/src/sync/content_keys_names.rs:18-59`, XChaCha20-Poly1305 unter einem HKDF-Unterschlüssel
  mit AAD). Keine Abhängigkeit, die `aws-lc-rs` bringt. 24-Byte-Nonces erlauben zufällige Nonces ohne
  Zähler über Geräte hinweg.
- **Alternatives considered**: AES-256-GCM (12-Byte-Nonce, zufällig nur begrenzt sicher; auf Android
  ohne AES-Befehle langsamer); BLAKE3 (neue Abhängigkeit für eine Prüfsumme, die SHA-256 genauso gut
  liefert); `age` oder `rage` (kein Lesen von Teilbereichen, eigenes Kopfformat ohne
  Schlüsselhierarchie).

## R3 — Was holzi von haex-vault übernimmt und was nicht

Prüfung von `file_sync/crypto` (5 420 Zeilen, 86 Tests) gegen den Anbieter als Angreifer:

| Befund in haex-vault                                                                                                                     | Folge für 048                                                                           |
| ---------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------- |
| Kein AAD in Blöcken und Schlüsselumschlägen (`chunk.rs:108-110`, `:127-129`; `dek_wrap.rs:57`)                                           | Jeder Block bindet Kopf, Objekt-Id, Index und „letzter Block“ als AAD (FR-022)          |
| Kürzen an einer Blockgrenze unerkannt; keine Prüfung gegen Größe oder Hash (`content.rs:59-66`, `:152-164`; `provider.rs:486-489`)       | Merker „letzter Block“ im AAD, Größe aus der Begleitdatei geprüft (FR-023)              |
| Begleitdatei lässt sich auf fremden Inhalt umbiegen, nicht an ihren Objektnamen gebunden (`provider.rs:372-385`)                         | Begleitdatei bindet Ordner-Id und eigenen Objektnamen im AAD; Inhalt dessen Id (FR-024) |
| Dateischlüssel wird beim Überschreiben wiederverwendet (`provider.rs:251-285`), alte Fassung entschlüsselt sich unter neuer Begleitdatei | Neuer Dateischlüssel je Fassung (FR-006)                                                |
| Ein Schlüssel für Begleitdatei und Umschlag (`provider.rs:318`, `:509`)                                                                  | Unterschlüssel je Zweck per HKDF aus dem Ordnerschlüssel                                |
| Name der Begleitdatei = Name des Inhalts + `.m` (`object_key.rs:102-107`): Anbieter sieht die Paare                                      | Getrennte Zufallsnamen; die Zuordnung steht nur verschlüsselt in der Begleitdatei       |
| Voller Pfad in der Begleitdatei (`sidecar.rs:57`): Umbenennen eines Ordners schreibt jede Begleitdatei darunter neu                      | Einträge mit Eltern-Id und Namen (Modell Cryptomator): Umbenennen = eine Datei (SC-006) |
| Begleitdateien werden nacheinander geladen, bei jedem Abgleich alle (`provider.rs:201-236`)                                              | Parallel, mit Zwischenspeicher nach ETag (R6)                                           |
| Kein Lesen von Teilbereichen; ganze Datei in eine Temp-Datei (`provider.rs:436-466`)                                                     | Blockgenaue Teilbereiche (R5)                                                           |
| „Testvektor“ vergleicht mit sich selbst (`crypto/tests.rs:2483-2510`)                                                                    | Echte Testvektoren mit festen Schlüsseln und Nonces (FR-042, SC-010)                    |

- **Decision**: holzi übernimmt das Modell (Umschlag mit Kopf, Blöcke, zufällige Objektnamen,
  verschlüsselte Begleitdateien, Schlüssel je Datei), nicht den Code und nicht das Format. Das Format
  heißt `HXEF` v1 und ist in [contracts/format.md](./contracts/format.md) beschrieben; es ist nicht mit
  `HXFE` von haex-vault kompatibel (das wäre nur mit allen Lücken zu haben).
- **Alternatives considered**: Code übernehmen und nachbessern (die Lücken stecken im Format selbst,
  Nachbessern bricht es ohnehin); Cryptomator-Format v8 (Dateinamen als einzelne Objekte je Ordner,
  Ordner-IDs als Dateien, Schlüsseldatei mit scrypt-Passwort: passt nicht zur Schlüsselhierarchie ohne
  Passwort und kennt keine Konfliktfassungen).

## R4 — Blockgröße

- **Decision**: 64 KiB Klartext je Block (65 552 Byte verschlüsselt). Gelesen wird in Fenstern von bis
  zu 8 MiB je Anfrage beim Anbieter.
- **Rationale**: Der Medienserver liest in 256 KiB (`files/media/server.rs:25`), der PDF-Viewer und
  der Textviewer in kleinen Bereichen (`storage_source.rs:417`). Mit 1 MiB je Block müsste holzi bei
  jeder kleinen Anfrage bis zu 1 MiB laden und entschlüsseln. Mehraufwand 16 Byte je 64 KiB = 0,024 %
  (SC-005: ≤ 1 %). Die Teile eines mehrteiligen Uploads (8 MiB, `transfer/remote.rs:32`) schneiden den
  verschlüsselten Strom ohne Rücksicht auf Blockgrenzen (128 Blöcke sind 8 MiB + 2 048 Byte); das ist
  ohne Belang, weil Teile nur Bytebereiche sind und ein neuer Versuch dieselben Bytes schickt.
- **Alternatives considered**: 1 MiB wie haex-vault (grobe Teilbereiche, mehr Last bei Sprüngen);
  32 KiB wie Cryptomator (doppelt so viele Tags, kein spürbarer Vorteil bei S3-Latenzen).

## R5 — Teilbereiche lesen und Medien

- **Decision**: Eine `EncryptedFileSource` implementiert `StreamingSource` (`files/streaming.rs:13-25`)
  wie `StorageFileSource` (`:65-112`): sie rechnet einen Klartextbereich in die betroffenen Blöcke um,
  lädt sie mit einem `get_range`, prüft und entschlüsselt sie und schneidet zu. Die letzten zwei Fenster
  bleiben im Arbeitsspeicher (höchstens 16 MiB je offener Datei, SC-004).
- **Rationale**: Der Medienserver bleibt unverändert; Tokens, Range und Freigabe-URLs gelten wie in 044.
- **Alternatives considered**: Ganze Datei in eine Temp-Datei wie haex-vault (Klartext auf der Platte,
  Widerspruch zu FR-015 und FR-026).

## R6 — Ordneransicht aufbauen (Begleitdateien schnell lesen)

- **Decision**:
  - Einmal `list` über `<präfix>m/` (seitenweise, 1 000 je Seite), dann die Begleitdateien parallel mit
    höchstens 32 gleichzeitigen `get` laden.
  - Zwischenspeicher in der Tabelle `encrypted_folder_entries_no_sync` (SQLCipher, nicht
    synchronisiert): je Begleitdatei Objektname, ETag und der entschlüsselte Eintrag. Beim nächsten
    Öffnen lädt holzi nur Begleitdateien, deren ETag neu ist oder fehlt; verschwundene fallen raus.
  - Ein verschlüsselter Ordner wird beim ersten Öffnen ganz geladen (alle Ebenen), weil seine Objekte
    flach liegen; Navigation darin braucht danach keine Anfrage mehr.
- **Rationale**: Bei 50 ms je Anfrage und 32 parallel dauern 1 000 Begleitdateien etwa 1,6 s, 10 000
  etwa 16 s (SC-003: < 5 s bzw. < 30 s); das zweite Öffnen braucht nur das `list` (< 1 s).
- **Alternatives considered**: Ein Index-Objekt je Ordner (schneller, aber zwei Geräte, die gleichzeitig
  schreiben, überschreiben sich gegenseitig ohne bedingte Schreibvorgänge, die nicht jeder Anbieter
  kann); Zuordnung in der synchronisierten Vault-Datenbank (vom Betreiber verworfen).

## R7 — Einträge, Fassungen und Konflikte

- **Decision**:
  - Jeder Eintrag hat eine zufällige `entry_id`, eine `parent_id` (Wurzel = Ordner-Id) und einen Namen.
    Ordner sind eigene Einträge; Umbenennen und Verschieben schreiben nur die Begleitdatei des Eintrags
    neu.
  - Jede Änderung schreibt eine neue Begleitdatei mit `revision` und `base` (Objektname der Begleitdatei,
    die sie ersetzt) und löscht die alte danach. Hängen zwei Begleitdateien derselben `entry_id` an
    derselben `base`, ist das ein Konflikt: beide bleiben, die ältere heißt „Name (Konflikt
    JJJJ-MM-TT hh-mm)“. Zwei Einträge mit verschiedener `entry_id` und gleichem Namen im selben Ordner
    bekommen dieselbe Konfliktbenennung.
  - Überschreiben: neues Inhaltsobjekt mit neuem Dateischlüssel, dann neue Begleitdatei, dann altes
    Inhaltsobjekt und alte Begleitdatei löschen (FR-017). Ein Leser, der die alte Begleitdatei hat,
    liest das alte Inhaltsobjekt ganz oder bekommt „nicht gefunden“ und lädt neu.
  - Verwaiste Inhaltsobjekte (kein Verweis aus einer Begleitdatei) und überholte Begleitdateien löscht
    holzi beim Öffnen eines Ordners, wenn ihre Änderungszeit beim Anbieter älter als 24 Stunden ist.
- **Rationale**: Anbieter wie S3 kennen keine Transaktionen über mehrere Objekte; die Reihenfolge
  „neu schreiben, dann alt löschen“ verliert nie Daten, und `base` erkennt gleichzeitiges Schreiben ohne
  bedingte Schreibvorgänge.
- **Alternatives considered**: Pfad als Schlüssel wie haex-vault (teures Umbenennen); bedingtes
  Schreiben mit `If-Match` (nicht von allen S3-kompatiblen Anbietern unterstützt).

## R8 — Verschlüsselte Ordner in gewöhnlichen Ordnern finden

- **Decision**: Ein verschlüsselter Ordner ist ein Präfix `<zufall>.hxef/` (26 Zeichen Base32 + Endung)
  im Elternordner. `StorageFiles::list` erkennt ihn an der Endung, lädt seine Köpfe parallel (je ein
  kleines `get`), entschlüsselt die Namen und zeigt ihn als Ordner mit Schloss. Köpfe kommen in den
  Zwischenspeicher `encrypted_folder_heads_no_sync` (Objektname, ETag, Name, Zustand).
- **Rationale**: Ohne Endung müsste holzi für jeden Unterordner eines Speichers nachsehen, ob er einen
  Kopf hat. Die Endung verrät nur, dass es einen verschlüsselten Ordner gibt; das erlaubt die Spec
  (Annahmen).
- **Alternatives considered**: `HEAD` auf jeden Unterordner (eine Anfrage je Ordner bei jedem Auflisten);
  Liste der verschlüsselten Ordner in der Vault (widerspricht „alles im Bucket“).

## R9 — Wo die Schicht in `files/` sitzt

- **Decision**: Neues Modul `src-tauri/src/files/encrypted/` mit:
  - `format/`: reiner Code ohne Ein- und Ausgabe (Kopf, Blöcke, Begleitdatei, Umschläge, Schlüssel,
    Testvektoren).
  - `folder.rs`: ein geöffneter Ordner, Ansicht aus den Begleitdateien, Zwischenspeicher.
  - `ops.rs`: Anlegen, Umbenennen, Verschieben, Löschen, Kopieren im Ordner.
  - `source.rs`: `EncryptedFileSource`.
  - `transfer.rs`: Ver- und Entschlüsseln für Hoch- und Herunterladen.

  `StorageFiles` (`files/storage_source.rs:206`) bekommt eine Weiche: Pfade, die durch einen
  verschlüsselten Ordner gehen, gibt es an `encrypted` weiter. `side_of`/`Side`
  (`transfer/remote_plan.rs:24`) bekommt `Side::Encrypted`, damit Transfers die Lese- und
  Schreibschritte (`upload`, `fetch`, `fill`) mit Ver- und Entschlüsseln umhüllen. Kopieren beim
  Anbieter (`transfer/remote.rs:300-311`) gilt innerhalb eines verschlüsselten Ordners (FR-018) und
  zwischen zwei verschlüsselten Ordnern derselben Vault (FR-020): das Inhaltsobjekt bekommt eine neue
  `oid` und behält `cid` und Dateischlüssel, der Dateischlüssel wird für den neuen Eintrag neu
  verpackt ([contracts/format.md](./contracts/format.md), „Kopieren beim Anbieter“).

- **Rationale**: Es gibt keinen Quellen-Trait; die Befehle verzweigen über `storage_of`
  (`browser_commands.rs:95-100`). Eine Weiche in `StorageFiles` hält Fenster, Suche
  (`search.rs:245-300`) und Agents unverändert, weil sie entschlüsselte Einträge bekommen.
- **Vorarbeit**: `browser_commands.rs` hat 817 Zeilen und liegt über der Grenze von 500 Zeilen
  (`.spaex/constitution.md:12`). Bevor 048 dort etwas ergänzt, wird es geteilt (eigener Refactor-PR).

## R10 — Vorschaubilder, Logs und Kopien für System-Apps

- **Decision**:
  - **Vorschaubilder**: `render()` (`files/thumbnails.rs:126`) bekommt eine Variante aus Bytes. Für
    verschlüsselte Ordner liegt das Ergebnis in einem LRU im Arbeitsspeicher von `FilesState` (64 MB),
    nie unter `files-thumbnails`.
  - **Logs**: Ein Typ `Redacted` für Pfade aus verschlüsselten Ordnern, der in `Display` und `Debug`
    nur „<verschlüsselt>“ zeigt. Die vorhandenen Logs mit Pfaden (`local/ops.rs:104`,
    `browser_commands.rs:490`, `:530-533`, `thumbnails.rs:107`, `transfer/local.rs:300-304`, `:463-466`)
    bekommen ihn, wo ein Pfad aus einem verschlüsselten Ordner kommen kann.
  - **Kopien für System-Apps**: unter `<AppCache>/files-opened-encrypted/<uuid>/`. Gelöscht beim Abbruch
    des Gate-Tokens (`vault_gate/mod.rs:254`, Sperren oder Schließen) und beim Start (Muster
    `prune_scratch`, `extensions/fs/dialogs.rs:255-275`, hier ohne 24-Stunden-Frist).
- **Rationale**: FR-026 bis FR-028; heute bleiben Kopien in `files-opened` liegen, bis der Cache geleert
  wird (`browser_commands.rs:548`).

## R11 — Android: „Mit System-App öffnen“

- **Decision**: 048 baut keine eigene Android-Öffnung. Sobald 044 „Mit System-App öffnen“ auf Android
  liefert (heute `Unsupported`, `browser_commands.rs:537-543`), nutzt es einen `FileProvider` im Plugin
  `holzi-android` mit `FLAG_GRANT_READ_URI_PERMISSION` für genau eine `content://`-URI; 048 verlangt nur,
  dass die Kopie im privaten Cache bleibt (FR-028a). Bis dahin zeigt der Dateibrowser die Aktion auf
  Android nicht an.
- **Alternatives considered**: Kopie nach Downloads (für jede App lesbar, widerspricht FR-028a);
  entschlüsselnder `ContentProvider` mit Pipe (kein Klartext auf der Platte, aber viele Apps brauchen
  springbare Dateien).

## R12 — Freigaben für Erweiterungen

- **Decision**:
  - Neue Berechtigungsart `encryptedFolder` mit Ziel `<storage_id>/<folder_id>` und den Aktionen `read`
    und `readWrite` in `extensions/permissions` (`model.rs:13-50`, `target.rs`). Gespeicherte Freigaben
    liegen in der vorhandenen Tabelle der Erweiterungs-Berechtigungen.
  - `manifest_map.rs` (`category_kind`, `:44-56`) erkennt die Kategorie und meldet sie als verboten; neu
    ist eine `BundleRejection::ForbiddenPermission` in `install_preview` und `install`
    (`registry/install.rs:274-281`, `:394`) und im Laden von Entwicklerversionen (`dev.rs:324`).
  - Der Dialog ist der vorhandene `PermissionRequestDialog.vue` mit Haken „merken“
    (`PermissionRequestDialog.vue:12`, `:73`); für diese Art ist er ab Werk **nicht** gesetzt, und der
    Dialog zeigt Speicher und entschlüsselten Ordnernamen.
  - Ohne Haken hält `PermissionState::hold` (`prompts.rs:95-100`) die Antwort; neu wird sie für diese
    Art beim Entladen oder Neuladen des Frames der Erweiterung vergessen (`forget`), nicht erst am Ende
    des Prozesses.
- **Erreichen des Ordners**: Eine Erweiterung kennt nur ihren Bereich und kann keinen Ordner finden.
  Neu im vault-sdk: `extension_encrypted_folder_choose(storageId)` öffnet in holzi eine Auswahl der
  verschlüsselten Ordner dieses Speichers (nur mit `remoteStorage` für ihn) und stellt dabei die Frage
  nach FR-036; zurück kommt eine Ordner-Kennung. Mit ihr arbeiten `…_list`, `…_download`, `…_upload`,
  `…_delete` relativ zum Ordner ([contracts/bridge.md](./contracts/bridge.md)).
- **Rationale**: Die Frage kommt vom Nutzer aus holzis Fenster; die Erweiterung lernt ohne Antwort
  nichts (FR-039).
- **Alternatives considered**: Pfade im ganzen Speicher für Erweiterungen (bricht 038 FR-010 weit über
  verschlüsselte Ordner hinaus).

## R13 — Freigaben für Agents

- **Stand**: Von 044 US6 gibt es nur die reine Prüfung `files/access.rs` (`check`, `StorageGrant`,
  `:77-161`). Tabelle `agent_file_permissions` (T074), Rückfrage-Brücke (T076), Einstellungen (T077)
  und Ausführer (T082) sind offen (`specs/044-file-browser/tasks.md:190-198`); den MCP-Server von
  Spec 021 gibt es noch nicht.
- **Decision**:
  - `files/access.rs` bekommt das Ziel `EncryptedFolder { storage, folder }` mit `Read`, `ReadWrite`,
    `Denied` und einer Reichweite `LocalOnly`/`AlsoCloud`. Für Agents liegen die Freigaben in
    `agent_file_permissions` mit `kind = 'encryptedFolder'` und neuer Spalte `reach`.
  - Nicht gemerkte Antworten hält ein Speicher im Prozess je Agent und Aufgabe: beim eingebauten Agent
    die `assistant_message_id` des Turns (`chat/turn/mod.rs:81-100`), bei externen Agents die
    MCP-Sitzung (021).
  - **Lokal oder Cloud** aus `ProviderKind` (`storage/providers.rs:44-58`): `Local` = lokal; `ApiKey`
    und `CliDelegate` = Cloud, auch bei einer Basis-URL auf `localhost` (holzi weiß nicht, was dahinter
    läuft); externe Agents = Cloud.
- **Folge für die Reihenfolge**: Die Agent-Lieferung von 048 kommt nach 044 PR G.

## R14 — Prüfen ohne Netz

- **Decision**:
  - Reine Format-Tests mit Testvektoren in `files/encrypted/format/*_tests.rs` und einer Datei
    `src-tauri/tests/fixtures/encrypted/v1-vectors.json`, die auch ein unabhängiges Prüfskript liest.
  - Ordner- und Transfer-Tests mit `FakeStore` (wie 044), inklusive Manipulationen (SC-002) und
    gleichzeitigem Schreiben.
  - E2E gegen RustFS: `files-encrypted.test.ts` (ein Gerät) und `files-encrypted-two-devices.test.ts`
    (Rig von 033).
  - Testvektoren laufen in der Plattform-Probe der CI auch unter Windows, macOS und Android (SC-010).
- **Rationale**: Keine Netzdienste in `cargo test` (Constitution); die CI-Probe deckt Android ab.

## R15 — Eine Speicher-Schnittstelle, auch für lokale Verzeichnisse (User Story 7, 2026-10-12)

- **Decision**: Verschlüsselte Ordner auf dem Gerät laufen über dieselbe Schicht `files/encrypted` wie
  im Speicher. Darunter kommt ein `LocalStore`, eine Umsetzung der gemeinsamen Speicher-Schnittstelle
  (`ObjectStore`, bis 2026-10-14 `RemoteStore`; `src-tauri/src/remote_storage/object_store.rs`) für ein lokales
  Verzeichnis. Ausgewählt wird er wie S3, WebDAV und SFTP über `ProtocolStore`
  (`remote_storage/protocol.rs:30-43`) mit einem neuen `Protocol::Local`. Es wird nie als Speicher
  gespeichert oder in den Einstellungen angeboten; `Location.bucket` trägt den Wurzelpfad.
  - Schlüssel sind Pfade unter der Wurzel; `..` und Auswege über Links lehnt `LocalStore` ab.
  - `list` geht rekursiv, `list_dir` eine Ebene; `head` liefert Größe, Änderungszeit als HTTP-Datum
    und ein ETag aus Größe, Änderungszeit und Inode (Windows: Datei-Index).
  - `get_range` liest ab einer Stelle mit `take`; `put` schreibt eine Teil-Datei und benennt sie um
    (FR-047); Upload-Sitzungen sind eine Teil-Datei, an die Teile angehängt werden, `finish_upload`
    synchronisiert und benennt um, `abort_upload` löscht sie.
  - `copies_on_provider` = wahr (`fs::copy`), `rename_tree` = `fs::rename`, `remove_tree` =
    `remove_dir_all`.
- **Name**: Die Schnittstelle hieß bis zum Merge von Spec 050 `RemoteStore` und heißt seit PR L (2026-10-14)
  `ObjectStore` (rein mechanischer PR; Betreiber 2026-10-12).
- **Rationale**: Der Betreiber will alle Speicher-Anbindungen über eine Schnittstelle (S3, WebDAV,
  SFTP, lokal). Die Schicht `files/encrypted` braucht dann keine zweite Umsetzung (FR-041, FR-044).
- **Alternatives considered**: `LocalStore` direkt an `StorageFiles` übergeben, ohne `Protocol`
  (vermeidet Änderungen an Bindings und Datenbank-Text, aber eine Anbindung neben der Weiche); den
  ganzen Geräte-Browser über die Schnittstelle führen (verliert Laufwerke, Links, Papierkorb,
  Beobachten; verworfen, Betreiber 2026-10-12).

## R16 — Kennung des Geräts als Speicher

- **Decision**: Für den Zwischenspeicher und die Freigaben braucht ein verschlüsselter Ordner auf dem
  Gerät eine Speicher-Kennung. Sie ist `device:<wurzel>` je Dateisystem-Wurzel: unter Linux, macOS und
  Android `device:/`, unter Windows je Laufwerk `device:C:`. Schlüssel sind die Pfade ohne die Wurzel.
  So bleibt die Kennung stabil, wenn ein Ordner auf dem Gerät verschoben wird, und Freigaben für
  Agents und Erweiterungen (`<storage_id>/<folder_id>`) gelten weiter.
- Zeilen im Zwischenspeicher (`encrypted_folder_*_no_sync`) zu Ordnern, deren Kopf beim Laden fehlt,
  löscht holzi; für Gerätekennungen läuft `cache::remove_storage` nie.
- **Alternatives considered**: eine Kennung je Elternordner (verliert Zwischenspeicher und Freigaben
  beim Verschieben); eine je Ordner-Kennung (kollidiert mit Kopien desselben Ordners).

## R17 — Weiche im Dateibrowser des Geräts

- **Decision**: Eine Funktion in `files/browser_commands/mod.rs` (`place_of`) liefert für einen
  Gerätepfad in einem verschlüsselten Ordner eine Speicher-Sicht (`StorageFiles` mit `LocalStore` und
  `Scope` für `device:<wurzel>`) samt Pfad darin. Das Fenster sieht den Ordner wie im Speicher unter
  seinem entschlüsselten Namen (`/home/anna/Unterlagen/Belege`); ein solcher Pfad existiert auf der
  Platte nicht. Darum ist ein Pfad, der existiert, gewöhnlich, und nur ein fehlender wird über die
  Köpfe seiner Ebenen gesucht (`files/encrypted/device.rs`, `route_device`). Unter Unix und Android ist
  der Pfad der Speicher-Sicht gleich dem Gerätepfad; unter Windows fällt das Laufwerk weg. Ein roher
  Pfad durch `<zufall>.hxef` ist kein Pfad des Fensters (`notFound`). Eine Ebene des Geräts, die sich
  nicht auflisten lässt (Androids `/storage`), gilt als Ebene ohne verschlüsselte Ordner.
  - Umsetzung 2026-10-13 statt der ersten Fassung (rohe Pfadteile `<zufall>.hxef` erkennen): mit
    entschlüsselten Namen braucht keine Antwort (Einträge, Suchtreffer, Transfers) eine Umrechnung,
    und Pfadleiste wie Brotkrumen zeigen Namen statt Zufallszeichen.
- Jede Stelle, die heute nach `storage_of()` verzweigt
  (`navigate.rs:108`, `:128`; `view.rs:50`, `:110`, `:168`, `:313`; `change.rs:35`, `:56`, `:89-103`,
  `:149-206`, `:218-245`; `search.rs:41`; `encrypted.rs:48`, `:101`), nimmt diese Sicht, wenn der Pfad
  in einem verschlüsselten Ordner liegt.
- **Auflisten** eines gewöhnlichen Ordners (`ops::list`): Einträge mit dem Namen `<zufall>.hxef`
  werden über ihren Kopf als verschlüsselte Ordner mit Namen und Zustand gezeigt (wie
  `StorageFiles::list`).
- **Transfers**: `side_of` liefert für solche Pfade `Side::Storage(sicht)`; `crosses` in
  `files_transfer_start` berücksichtigt das. Gerät ↔ verschlüsselter Ordner auf dem Gerät läuft damit
  über die Transfers aus PR C und E.
- **Löschen**: über `start_storage_delete` (endgültig, Rückfrage im Fenster, FR-046), nicht
  `platform_removal()`.
- **Beobachten**: `files_watch` auf einen Pfad in einem verschlüsselten Ordner beobachtet das
  Verzeichnis `<zufall>.hxef` und meldet jede Änderung als Änderung des offenen Ordners; das Fenster
  lädt neu und holt neue Begleitdateien nach ETag.
- **Suche** (`files/search.rs:155`): Der Lauf über das Gerät steigt nicht in `<zufall>.hxef` ab,
  sondern sucht im entschlüsselten Baum des Ordners (`search_storage`) weiter (FR-049).

## R18 — Rohe Dateien für Erweiterungen und Agents sperren

- **Decision**:
  - **Erweiterungen**: `touches_denied` / `authorize_by` in `extensions/fs/mod.rs:213-277` prüfen
    zusätzlich, ob ein Pfadteil des aufgelösten Pfads `format::is_folder_prefix` erfüllt; solche Pfade
    sind wie holzis eigene Daten gesperrt (Lesen, Schreiben, Löschen, Beobachten, Dialoge). `read_dir`
    (`ops.rs:115-131`) lässt sie in Listen weg.
  - **Agents**: `files/access.rs` `check()` (`:77`) behandelt `Target::Device` in einem verschlüsselten
    Ordner wie gesperrt; `visible_to` (`:150`) lässt rohe `.hxef`-Verzeichnisse weg. Die Dateiaktionen
    (`files/agent/exec.rs`, `exec_write.rs`) gehen für Pfade darin über die Speicher-Sicht mit den
    Freigaben aus PR G.
- **Rationale**: FR-050, FR-051; eine Erweiterung mit Dateizugriff könnte sonst Objekte löschen und den
  Ordner beschädigen.
- **Erweiterungen über `encryptedFolders` (Betreiber 2026-10-13)**: `device:<wurzel>` hängt an der
  Dateizugriff-Erlaubnis für den Ordner, der den verschlüsselten Ordner enthält, nicht an
  `remoteStorage`; dazu die Erlaubnis je Ordner. Die Auswahl nennt nur Ordner aus dem Zwischenspeicher
  der Köpfe, deren Ort die Erweiterung erreicht. Verworfen: nur die Erlaubnis je Ordner (eine
  Erweiterung ohne Dateizugriff käme so an Gerätedaten).
- **Agents**: ein Gerätepfad in einen verschlüsselten Ordner wird zum Speicher `device:<wurzel>`, der
  der Freigabe „Dateien des Geräts“ folgt; ein Aufruf mischt solche Pfade nie mit gewöhnlichen (die
  Speicher-Sicht des Geräts ginge an der Prüfung von holzis eigenen Daten vorbei). Unter Windows
  erreichen Agents diese Ordner noch nicht.

## R19 — Fremde Dateien

- **Decision**: Unter einem verschlüsselten Ordner gilt alles als fremd, was nicht `h`, `m/<26>`,
  `c/<26>` oder eine Teil-Datei von holzi ist. Das Laden (`load.rs`) übergeht fremde Dateien und zählt
  sie; das Aufräumen (`cleanup.rs`) löscht sie nie; das Löschen des ganzen Ordners nennt ihre Zahl in
  der Rückfrage und löscht sie mit. Die Regel kommt in `docs/formats/encrypted-folder-v1.md` und gilt
  auch im Speicher (etwa für Dateien, die jemand über die Weboberfläche des Anbieters hochlädt).
- `EncryptedInfo` bekommt für den Ordner selbst `foreign: number`; das Fenster zeigt den Hinweis.
