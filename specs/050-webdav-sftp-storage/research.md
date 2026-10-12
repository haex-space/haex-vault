# Research: WebDAV- und SFTP-Speicher neben S3

Stand 2026-10-11. Grundlage: Code-Recherche im Abzweig von `main` (1a684eb9) und zwei Recherchen zu
WebDAV-Anbietern und SSH-Bibliotheken (Quellen je Abschnitt). Die Entscheidungen von Spec 038
(research.md dort, R1–R11) gelten weiter, soweit hier nichts anderes steht.

## R1 — Protokoll je Verbindung statt eines S3-Stores je Prozess

**Befund**: Es gibt genau einen Anbieter je Prozess, und der ist S3: `s3_store()` in
`src-tauri/src/remote_storage/commands.rs:23` ist ein `OnceLock<S3Store>`. Ihn nutzen `service()`
(Einstellungen), der Dateibrowser (`files/browser_commands.rs:89`) und die Erweiterungen
(`StorageState::store()`, `extensions/remote_storage_dialog.rs:114`). Alle Aufrufer halten ein
`Arc<dyn RemoteStore>` und geben bei jedem Aufruf ein `Access` (Ort und Zugangsdaten) mit.

**Decision**: Ein `ProtocolStore` ersetzt `s3_store()`. Er hält je eine Instanz von `S3Store`,
`WebDavStore` und `SftpStore` und implementiert `RemoteStore`, indem er nach `access.location.protocol`
weiterreicht. `s3_store()` wird zu `stores()`; die drei Aufrufstellen ändern nur den Namen. `Location`
bekommt `protocol: Protocol` und `host_key: Option<String>`; die Felder von S3 (Region, Adressierung)
bleiben und sind bei WebDAV und SFTP leer bzw. ohne Bedeutung, `bucket` trägt bei ihnen den Ordnerpfad.

**Rationale**: Kein Aufrufer muss das Protokoll kennen (FR-018), die Fälschungen des Traits in den Tests
bleiben gültig, und die Einspeisung in Tests (`StorageState` setzt einen anderen Store) bleibt eine
Stelle. Ein flaches `Location` mit Protokollfeld vermeidet, alle Vergleiche
`from.location == to.location` (Kopieren beim Anbieter, `files/transfer/remote.rs:300`) umzubauen.

**Alternatives considered**: ein `enum Location` mit einer Variante je Protokoll (sauberer, aber berührt
jede Stelle, die Felder liest, ohne Gewinn für die Aufrufer); je Verbindung ein eigenes
`Arc<dyn RemoteStore>` aus einer Fabrik (verschiebt die Wahl in jeden Aufrufer und bricht die
Einspeisung der Tests).

## R2 — WebDAV-Client

**Decision**: holzi schreibt den WebDAV-Client selbst auf dem vorhandenen `reqwest` (rustls, ring) und
`quick-xml` 0.42 (schon im Build über `keepass` und `plist`; als direkte Abhängigkeit mit derselben
Version). Methoden: `PROPFIND` (immer `Depth: 1`), `GET` mit `Range`, `HEAD`, `PUT` (gestreamt), `DELETE`,
`MKCOL`, `MOVE`, `COPY`. Den HTTP-Client baut eine gemeinsame Funktion, die heute in
`remote_storage/s3.rs:59-80` steckt (keine Weiterleitungen, kein Proxy, `resolve_to_addrs` auf die
geprüften Adressen); sie zieht nach `remote_storage/http.rs` um.

Regeln:

- **Anmeldung**: Basic über `https` (oder `http` im lokalen Netz, R6). Alle vier Anbieter der
  Voreinstellungen antworten mit `WWW-Authenticate: Basic` (geprüft 2026-10-11); Digest braucht keiner
  und gehört nicht dazu.
- **Ordner-Adressen immer mit `/` am Ende**, damit kein Server auf die Adresse mit `/` weiterleitet. Eine
  Antwort 3xx ist ein Fehler (`Network`, im Test „Adresse antwortet nicht als WebDAV“ mit dem Hinweis auf
  die Weiterleitung, ohne die Adresse zu nennen).
- **Auflisten nur mit `Depth: 1`** und Ebene für Ebene; Apache (Hetzner, kDrive) verweigert `infinity`,
  SabreDAV liefert still nur eine Ebene (RFC 4918 §9.1). Abgefragt werden `resourcetype`,
  `getcontentlength`, `getlastmodified` (RFC 1123) und `getetag`.
- **`href`** ist ein absoluter Pfad oder eine volle Adresse und prozentkodiert (§8.3): holzi dekodiert,
  prüft, dass er unter der angefragten Adresse liegt (sonst wird der Eintrag verworfen und geloggt), und
  lässt den ersten Eintrag (den Ordner selbst) weg.
- **Fehlende Elternordner**: `PUT` und `MKCOL` antworten mit 409 (§9.7.1, §9.3.1). holzi legt dann die
  Ordner Stufe für Stufe mit `MKCOL` an (405 = schon da) und wiederholt den Aufruf einmal; das gilt nur
  unterhalb des Ordners des Speichers. Fehlt der Ordner des Speichers selbst (`PROPFIND` auf ihn: 404),
  ist das `NotFound` (im Test `folderMissing`); ihn legt holzi nur nach Bestätigung an (FR-006). SFTP
  verfährt ebenso mit `create_dir`.
- **`MOVE`/`COPY`** mit `Destination` als volle Adresse auf demselben Host und `Overwrite: T`; 201 und 204
  sind Erfolg.
- **Statuscodes**: 401 → `AccessDenied`, 403 → `MissingRight`, 404 → `NotFound`, 507 → `QuotaExceeded`
  (R10), 405 auf `PROPFIND` der Basisadresse → im Test „Adresse antwortet nicht als WebDAV“, 5xx und
  Netzfehler → `Network`.
- **Teilbereiche**: `GET` mit `Range`; ein Server, der mit 200 und dem ganzen Inhalt antwortet, wird wie
  bei S3 behandelt (Warnung einmal, `s3.rs:376-379`), die ersten Bytes bis zum Start werden verworfen.
- **Änderungszeit**: Nur die Nextcloud-Familie übernimmt `X-OC-Mtime`; holzi setzt keine Zeit und zeigt
  die Zeit des Servers (Zeitpunkt des Hochladens). Spec 048 FR-016 nutzt genau diese Zeit.
- **Bedingte Schreibvorgänge** (`If-Match`): nicht genutzt. `golang.org/x/net/webdav` (rclone, SFTPGo)
  ignoriert `If-Match` bei `PUT`; Spec 048 erkennt Konflikte ohnehin ohne (048 research R7).

**Rationale**: Sechs Methoden und ein Parser für `multistatus` sind wenig Code; volle Kontrolle über
Weiterleitungen, `/`-Regeln, `href`-Prüfung und Fehlerabbildung ist hier Sicherheitsanforderung
(FR-011), und ein zweiter XML-Stack entfällt.

**Alternatives considered**: `reqwest_dav` 0.3.3 (erlaubt einen eigenen `reqwest::Client`, zieht aber
`serde-xml-rs`/`xml-rs`, `digest_auth`, `chrono`; wenig Kontrolle über `href` und Fehler; das Beispiel
nutzt `Depth: infinity`); `rustydav`, `hyperdav` (nicht gepflegt); OpenDAL (gute Vorlage, aber schwere
Abhängigkeit mit eigener HTTP-Schicht, siehe Gespräch 2026-10-11).

Quellen: RFC 4918; https://docs.hetzner.com/storage/storage-box/access/access-webdav/;
https://docs.nextcloud.com/server/latest/developer_manual/client_apis/WebDAV/basic.html;
https://app.koofr.net/help/webdav; https://www.infomaniak.com/en/support/faq/2409/connecting-to-kdrive-via-webdav;
https://github.com/golang/net/blob/master/webdav/webdav.go

## R3 — SFTP-Client

**Decision**: `russh` 0.63.3 (2026-09-09, Apache-2.0, MSRV 1.89) mit
`default-features = false, features = ["ring", "rsa", "flate2"]` und `russh-sftp` 3.0.0 (2026-09-08,
Apache-2.0). Schlüssel über die Re-Exporte von russh (`russh::keys`, darin `ssh-key` =0.7.0-rc.11). Die
neueren Versionen (russh 0.64.x, russh-sftp 3.0.1) sind jünger als zwei Wochen und werden später
nachgezogen.

- **Verbindung**: holzi löst den Host auf und prüft die Adressen (R6), öffnet einen `tokio::net::TcpStream`
  genau zu einer geprüften Adresse und übergibt ihn `russh::client::connect_stream`. Verbindungsaufbau,
  Schlüsseltausch und Anmeldung laufen in `tokio::time::timeout` mit der Frist des Aufrufs (russh hat
  keine eigene Frist dafür); `Config` setzt `inactivity_timeout` und `keepalive_interval`.
- **Hostschlüssel**: `client::Handler::check_server_key` läuft beim ersten Schlüsseltausch, vor jeder
  Anmeldung. holzi vergleicht den Schlüssel mit dem bestätigten (R5); passt er nicht oder fehlt er,
  merkt sich der Handler den gezeigten Schlüssel und antwortet `Ok(false)`, russh bricht mit
  `Error::UnknownKey` ab, bevor Zugangsdaten gesendet werden (FR-012, SC-004).
- **Anmeldung**: `authenticate_password` oder `authenticate_publickey` mit
  `PrivateKeyWithHashAlg` (bei RSA `best_supported_rsa_hash()`).
- **SFTP**: Kanal öffnen, `request_subsystem("sftp")`, `SftpSession::new(channel.into_stream())`.
  Genutzt werden `read_dir`, `metadata`, `open`/`create` (Datei mit `AsyncRead`/`AsyncWrite`/`AsyncSeek`),
  `create_dir`, `remove_file`, `remove_dir`, `rename`, `sync_all` (`fsync@openssh.com`, wenn der Server
  es kann). Lesen und Schreiben sind in russh-sftp schon gestaffelt (16 gleichzeitige Anfragen).
- **Überschreibendes Umbenennen**: `SSH_FXP_RENAME` überschreibt bei OpenSSH nicht, und russh-sftp kennt
  `posix-rename@openssh.com` nicht. holzi öffnet dafür einen zweiten SFTP-Kanal als `RawSftpSession`,
  liest bei `init()` die Erweiterungen des Servers und nutzt `extended("posix-rename@openssh.com", …)`,
  wenn sie gemeldet ist; sonst Ziel löschen und umbenennen (wie rclone). Das kurze Fenster ohne Ziel deckt
  der Randfall „soweit der Server das zulässt“ der Spec.
- **Verbindungen wiederverwenden**: Ein SSH-Aufbau kostet mehrere Rundreisen, und Hetzner erlaubt 10
  gleichzeitige Verbindungen je Konto. holzi hält je Verbindung (Kennung, Hostschlüssel, Benutzer) bis zu
  zwei SSH-Sitzungen offen und schließt sie nach 60 s ohne Aufruf; ein Wechsel von Zugangsdaten oder
  Hostschlüssel verwirft sie.
- **Kopieren beim Server**: `copy-data` kennt russh-sftp nicht und viele Server auch nicht. SFTP meldet
  „kein Kopieren beim Anbieter“ (R8), der Transfer streamt dann über holzi.

**Rationale**: russh ist die einzige gepflegte SSH-Bibliothek in reinem Rust; mit dem Backend ring
kommt keine neue native Bibliothek dazu (ring baut schon für alle Ziele über rustls), und die
Abhängigkeiten von russh passen zu den vorhandenen (`ed25519-dalek` 3, `sha2` 0.11, `rsa` 0.10.0-rc.18,
`p256` 0.14).

**Alternatives considered**: `ssh2`/`async-ssh2-lite`/`libssh-rs` (C-Bibliotheken mit OpenSSL),
`openssh`/`openssh-sftp-client` (rufen das Programm `ssh` auf, das es auf Android nicht gibt), `thrussh`
(libsodium, alte Krypto-Versionen), `ssh-rs` (blockierend, nicht gepflegt).

**Hinweis außerhalb dieser Spec**: `Cargo.lock` enthält heute schon `aws-lc-rs` 1.18.1 und `aws-lc-sys`
über `quinn-proto`, `rustls` und `rustls-webpki`. Ob es tatsächlich gebaut wird, zeigt
`cargo tree -i aws-lc-rs -e features`; das klärt ein eigener Auftrag. Diese Spec fügt keinen Weg zu
`aws-lc-rs` hinzu (Prüfung als Aufgabe).

Quellen: https://docs.rs/russh/0.63.3/russh/client/; https://github.com/warp-tech/russh;
https://github.com/RustCrypto/SSH; crates.io-API (Versionen und Abhängigkeiten, abgerufen 2026-10-11).

## R4 — SSH-Schlüssel

**Decision**:

- **Erzeugen** (FR-008): `PrivateKey::random(&mut rng, Algorithm::Ed25519)`, Kommentar `holzi`, ohne
  Passphrase (geschützt durch den Passwortmanager der Vault). holzi zeigt den öffentlichen Schlüssel als
  OpenSSH-Zeile (`ssh-ed25519 AAAA… holzi`) zum Kopieren. Hetzner verlangt auf Port 22 das Format
  RFC 4716, auf Port 23 die OpenSSH-Zeile; die Voreinstellung nutzt Port 23 (R11). Der private
  Schlüssel geht nie in die Oberfläche: holzi hält ihn bis zum Speichern als vorläufigen Schlüssel im
  Prozess (Kennung → `Zeroizing`, verworfen nach 30 Minuten, beim Sperren der Vault und nach dem
  Speichern) und gibt nur Kennung und öffentlichen Schlüssel zurück (Analyse U1).
- **Einfügen oder Datei einlesen**: `russh::keys::decode_secret_key(text, passphrase)` liest OpenSSH (auch
  mit Passphrase), PKCS#1, PKCS#8, `EC PRIVATE KEY` und PuTTY-PPK. holzi prüft den Schlüssel beim
  Speichern und legt den Text so ab, wie der Nutzer ihn gab, mit der Passphrase in einem eigenen Feld.
- **Ablage**: im Eintrag des Passwortmanagers (R9 in data-model.md: eigene Felder `privateKey` und
  `passphrase`); eigene Felder sind dort Geheimnisse (Listen zeigen nur, dass es sie gibt;
  `passwords/model.rs:32`).

**Alternatives considered**: eine eigene Abhängigkeit auf `ssh-key` 0.6.7 (stabil, aber unverträglich mit
der Version, die russh fest verlangt); erzeugte Schlüssel mit Passphrase (eine zweite Geheimnisebene
ohne Gewinn, weil die Passphrase im selben Eintrag läge).

## R5 — Hostschlüssel: beim ersten Verbinden bestätigen

**Decision**:

- Die Verbindung speichert den bestätigten Hostschlüssel als OpenSSH-Zeile (`host_key`) und den
  Zeitpunkt der Bestätigung; beides synchronisiert (FR-009).
- Beim Test einer neuen SFTP-Verbindung ohne Schlüssel endet der Test mit `hostKeyUnknown` und dem
  gezeigten Schlüssel samt Fingerabdruck (`SHA256:<base64>`, wie OpenSSH). Die Einstellungen zeigen den
  Fingerabdruck mit dem Hinweis, ihn mit der Angabe des Anbieters zu vergleichen (bei der Voreinstellung
  Hetzner mit dem Verweis auf die veröffentlichten Fingerabdrücke). Bestätigt der Nutzer, schickt die
  Oberfläche den Schlüssel mit dem nächsten Speichern mit; holzi verbindet erneut und akzeptiert nur
  genau diesen Schlüssel.
- Den zuletzt gezeigten Schlüssel je Verbindung merkt sich der Prozess (nicht gespeichert), damit
  „annehmen“ nur genau den Schlüssel übernimmt, den der Nutzer gesehen hat (Analyse U2).
- Zeigt der Server später einen anderen Schlüssel, scheitert jeder Aufruf mit `HostKeyChanged` (R10);
  die Einstellungen zeigen am Speicher die Warnung mit altem und neuem Fingerabdruck und die Aktion
  „Neuen Schlüssel annehmen“ (`storage_host_key_accept`, contracts/tauri-commands.md). Erweiterungen und
  Agents bekommen `Network` und können keinen Schlüssel annehmen.
- Ein vom Nutzer an einem Gerät angenommener Schlüssel kommt über den Sync auf die anderen Geräte und
  gilt dann dort.

**Rationale**: Trust-on-first-use wie OpenSSH, aber mit ausdrücklicher Bestätigung statt Nachfrage im
Terminal; die Bestätigung gehört der Vault, nicht dem Gerät, weil Verbindungen vault-weit sind.

**Alternatives considered**: Hostschlüssel je Gerät (`_no_sync`, jedes Gerät fragt neu: lästig und ohne
Sicherheitsgewinn, weil dieselbe Person bestätigt); Hostschlüssel als Vorschlag einer Erweiterung
(verboten, FR-017b: sonst könnte eine Erweiterung einen Server unterschieben).

## R6 — Adressen prüfen auch für SFTP

**Befund**: `remote_storage/address.rs` ist auf URLs mit `http`/`https` gebaut: keine Benutzerangaben,
keine Abfrage, kein Fragment; `scope_of` legt `local` oder `public` fest; `pin` löst vor jedem Aufruf auf
und prüft jede Adresse gegen den gespeicherten Bereich; Link-Local, Multicast und Broadcast nie.

**Decision**: Der Endpunkt einer SFTP-Verbindung ist eine Adresse der Form `sftp://host[:port]` (ohne
Pfad, Benutzer, Abfrage, Fragment; Port 22, wenn er fehlt). `address.rs` lässt das Schema `sftp` zu,
bestimmt den Bereich wie bisher und liefert mit `pin` die geprüften Adressen; `sftp` gilt nie als
unverschlüsselt. Für WebDAV gelten die bestehenden Regeln unverändert; der Pfad ist Teil der Adresse und
wird mit `/` am Ende gespeichert. Die Aktion `add` einer Erweiterung hat bei beiden den Host und Port als
Ziel (Spec 038 FR-009b, `permissions/target.rs`).

**Rationale**: Ein Endpunkt-Feld und eine Prüfung für alle drei Protokolle; dieselbe Abwehr gegen
DNS-Rebinding und den Metadatendienst gilt für SFTP ohne neuen Code.

## R7 — Ohne halbe Dateien: Zwischendateien und Upload-Sitzungen

**Befund**: Ein `PUT` bei S3 ist atomar; bei WebDAV- und SFTP-Servern liegt eine Datei während des
Hochladens schon unter ihrem Namen. Der Transfer (`files/transfer/remote.rs`) lädt unter 8 MiB mit `put`,
darüber in Teilen (`create_multipart`, `upload_part`, `complete_multipart`, `abort_multipart`) und
wiederholt einen Teil nach Netzfehlern.

**Decision**:

- Die vier Multipart-Methoden des Traits heißen künftig `begin_upload`, `upload_part`, `finish_upload`,
  `abort_upload`, mit derselben Form (Teile der Reihe nach ab 1; `upload_part` liefert eine Marke, bei S3
  das ETag des Teils). S3 bleibt Multipart.
- **WebDAV**: `begin_upload` startet einen gestreamten `PUT` auf eine Zwischendatei
  `.holzi-part-<UUID>` im Zielordner; jeder Teil geht in den Strom (gebundener Kanal, Gegendruck);
  `finish_upload` schließt den Strom, wartet auf die Antwort und verschiebt die Zwischendatei mit `MOVE`
  und `Overwrite: T` auf den Zielnamen; `abort_upload` bricht den Strom ab und löscht die Zwischendatei.
  Ein gescheiterter Teil lässt sich nicht wiederholen: Die Sitzung ist dann verbraucht, jeder weitere
  Aufruf liefert `Network`, und der Transfer endet wie nach erschöpften Wiederholungen (Spec:
  „beginnt nach einem Abbruch von vorn“).
- **SFTP**: `begin_upload` legt die Zwischendatei an; ein Teil wird an seine Stelle geschrieben (die
  Sitzung merkt sich die Stelle je Teilnummer), sodass Wiederholen eines Teils geht; `finish_upload`
  ruft `fsync` (wenn der Server es kann), schließt und benennt um (R3); `abort_upload` löscht.
- **`put`** kleiner Dateien geht bei WebDAV und SFTP denselben Weg (Zwischendatei, dann umbenennen).
- Die Zustände der Sitzungen hält der jeweilige Store in einer Tabelle nach Kennung; eine Sitzung, die
  30 Minuten keinen Teil bekam, wird abgebrochen.
- **Zwischendateien** heißen `.holzi-part-<UUID>`. Auflisten blendet sie aus (Dateibrowser, Agents,
  Erweiterungen, FR-022), und eine, deren Änderungszeit älter als 24 Stunden ist, wird beim Auflisten
  gelöscht (ohne Fehler, wenn das scheitert).

**Rationale**: Der Transfer bleibt fast unverändert, die Wiederholung je Teil bleibt bei S3 und SFTP
erhalten, und ohne Zwischendatei wäre FR-016 bei WebDAV und SFTP nicht erfüllbar.

**Alternatives considered**: eine neue Methode `upload_stream` mit einem Leser (sauber, aber die
Wiederholung je Teil müsste in jeden Store wandern); das Hochladen in Stücken von Nextcloud (nicht im
Umfang der Spec).

## R8 — Ordner als echte Ordner: Baum und flacher Namensraum

**Befund**: Der Dateibrowser (`files/storage_source.rs`) bildet Ordner bei S3 als Präfix mit einem leeren
Markierungsobjekt `prefix/` ab; Umbenennen kopiert jedes Objekt und löscht es; Löschen löscht jedes
Objekt, die Markierung zuletzt. Der Transfer zählt Ordnerbäume mit einem rekursiven `list` auf
(`transfer/remote_plan.rs:174-182`); `list_dir` liefert Objekte und Präfixe wie S3 mit `delimiter=/`.

**Decision**:

- WebDAV und SFTP bilden die S3-Form nach: `list` liefert für jeden Ordner einen Eintrag `ordner/` der
  Größe 0 (wie eine Markierung), `list_dir` liefert Unterordner als Präfixe (auch leere) und Dateien als
  Objekte; `put` auf einen Schlüssel mit `/` am Ende legt den Ordner an (`MKCOL`, `create_dir`),
  `delete` auf ihn entfernt einen leeren Ordner. So funktionieren Anlegen, Transfer und Kopieren im
  Dateibrowser ohne Änderung.
- Zwei neue Methoden für einen Vorgang am ganzen Ordner: `rename_tree(from, to)` (WebDAV `MOVE`, SFTP
  Umbenennen) und `remove_tree(prefix)` (WebDAV `DELETE` auf den Ordner, rekursiv nach §9.6.1; SFTP
  rekursiv von unten). `storage_source` nutzt sie, wenn `Location::is_tree()` gilt, statt der Schleifen
  über Objekte; S3 bleibt bei den Schleifen und gibt für beide Methoden `Unsupported` zurück (wird nie
  aufgerufen, geprüft im Test).
- Eine Methode `copies_on_provider(&Access) -> bool`: S3 und WebDAV ja (`COPY`), SFTP nein; der Transfer
  kopiert beim Anbieter nur, wenn sie ja sagt, sonst streamt er über holzi (der Weg zwischen Speichern
  gibt es schon).
- `ObjectInfo` bekommt `etag: Option<String>`: S3 das ETag, WebDAV `getetag`, SFTP eine Marke aus Größe
  und Änderungszeit. Spec 048 braucht sie für ihre Zwischenspeicher (048 data-model.md); wer zuerst
  landet, führt das Feld ein, die andere Spec übernimmt es.

**Rationale**: Die Markierungs-Form ist die kleinste Änderung an Dateibrowser, Transfer, Suche und Agents;
nur Umbenennen und Löschen eines Ordners würden ohne eigenen Weg bei WebDAV und SFTP zu vielen Aufrufen
statt einem, und Kopieren beim Anbieter gibt es bei SFTP nicht.

**Alternatives considered**: eine eigene Quelle „Baum“ im Dateibrowser neben „Speicher“ (doppelter Code
für Navigation, Viewer, Transfers, Suche, Agents); ein `enum` für die Form im Trait mit Methoden je Form
(mehr Fläche ohne Nutzen für die Aufrufer).

## R9 — Erweiterungen: dieselben Ergebnisse auf jedem Protokoll

**Befund**: Erweiterungen schreiben unter `holzi-ext/<vault>/<ext>/`, Schlüssel bis 1024 Byte
(`extensions/remote_storage_keys.rs`); ihr `list` ist ein Präfixvergleich (`remote_storage.rs:296-330`):
`a/b` trifft auch `a/bc`.

**Decision**:

- **Auflisten mit Präfix**: Der Store liest ab dem Ordner bis zum letzten `/` des Präfixes rekursiv
  (Ebene für Ebene) und behält Dateien, deren Schlüssel mit dem Präfix beginnt; Ordnereinträge (R8) und
  Zwischendateien fallen für Erweiterungen weg, damit das Ergebnis dem von S3 gleicht.
- **Schlüssel prüfen je Protokoll**: zusätzlich zu Spec 038 FR-011 bei WebDAV und SFTP kein Teil länger
  als 255 Byte, keine Teile `.`, kein `\`, keine Zeichen, die verbreitete Dateisysteme verbieten
  (`:*?"<>|` und Steuerzeichen), kein Teil, der mit `.holzi-part-` beginnt; Gesamtlänge wie bei S3
  (1024 Byte mit Präfix).
- **Groß-/Kleinschreibung**: Vor dem Schreiben einer neuen Datei liest der Store den Elternordner (eine
  Ebene) und lehnt ab, wenn dort ein Name liegt, der sich nur in der Schreibweise unterscheidet
  (`InvalidKey`, Randfall der Spec); so trifft ein Schreiben auf Koofr (unterscheidet nicht) nie eine
  andere Datei. Die Prüfung liegt in `tree.rs` und gilt auch für den Dateibrowser: Dort ist ein solcher
  Name „schon vorhanden“ und löst die Konfliktfrage aus (Analyse C1).
- **Leere Ordner nach dem Löschen**: Nach dem Löschen einer Datei entfernt holzi die dadurch leeren
  Elternordner bis zur Wurzel des Bereichs der Erweiterung, nie darüber. SFTP `remove_dir` scheitert an
  nicht leeren Ordnern von selbst; bei WebDAV prüft holzi mit `PROPFIND`, dass der Ordner leer ist, und
  löscht dann. Das kleine Fenster zwischen Prüfen und Löschen betrifft nur den Bereich derselben
  Erweiterung (sie schreibt gleichzeitig von einem anderen Gerät) und wird hingenommen.
- **Liste der Speicher** (FR-017a): `type` ist `s3`, `webdav` oder `sftp`; bei WebDAV und SFTP steht statt
  `bucket` ein Feld `folder`.
- **Vorschläge** (FR-017b): `type: "webdav"` mit `config: { url, folder }`, `type: "sftp"` mit
  `config: { host, port?, folder }`, `sameProviderAs` wie bisher mit `config: { folder }`. Abgelehnt wird
  jeder Aufruf mit `username`, `password`, `privateKey`, `passphrase`, `hostKey` oder `fingerprint`
  (zusätzlich zu den S3-Feldern, `remote_storage_manage.rs:32`). Details in contracts/bridge.md.

**Rationale**: SC-002 verlangt dieselben Ergebnisse; die Prüfungen liegen vor jedem Aufruf beim Server,
wie in Spec 038 R5.

## R10 — Fehler und Testergebnisse

**Decision**:

- `StorageError` bekommt `QuotaExceeded` (WebDAV 507; SFTP meldet beim Schreiben nur `SSH_FX_FAILURE`,
  daher fragt holzi danach `statvfs@openssh.com` ab, wenn der Server es anbietet, und meldet
  `QuotaExceeded`, wenn weniger frei ist als der Rest der Datei, sonst `Network` mit Log), `HostKeyChanged`,
  `HostKeyUnknown` und `Unsupported` (R8). Für Erweiterungen wird `QuotaExceeded` zur Fehlerart „Grenze
  überschritten“ und `HostKey*` zu „Netzwerkfehler“ (FR-021); der Dateibrowser nennt beide eigens.
- `TestOutcome` bekommt `folderMissing` (statt `bucketMissing` bei WebDAV und SFTP), `notWebDav`,
  `hostKeyUnknown` (mit gezeigtem Schlüssel und Fingerabdruck in `TestResult`) und `hostKeyChanged`.
- Der Test (`remote_storage/probe.rs`) bleibt: `holzi-test/<UUID>` schreiben, lesen, auflisten, löschen;
  bei WebDAV und SFTP zusätzlich den dabei angelegten Ordner `holzi-test/` entfernen, wenn er leer ist.
  Ein fehlender Ordner wird am ersten Schreiben erkannt (WebDAV 409 nach erfolglosem `PROPFIND` des
  Speicherordners, SFTP `No such file`), nicht durch eigenes Anlegen.
- „Ordner anlegen“ (FR-006) ist ein eigener Command (`storage_folder_create`), den die Oberfläche nach
  Bestätigung ruft; danach testet sie erneut.

## R11 — Voreinstellungen

**Decision**: `ProviderKind` (füllt nur vor) bekommt `hetznerStorageBox`, `nextcloud`, `koofr`, `kdrive`.
Die Muster legt das Frontend fest (`ConnectionForm.vue`, `PRESETS`):

| Voreinstellung      | Protokoll | Adresse                                              | Hinweis                                                                                                     |
| ------------------- | --------- | ---------------------------------------------------- | ----------------------------------------------------------------------------------------------------------- |
| Hetzner Storage Box | WebDAV    | `https://<benutzer>.your-storagebox.de/`             | „WebDAV“ und „Externe Erreichbarkeit“ in der Hetzner-Konsole einschalten; Unterkonten `<benutzer>-subN`     |
| Hetzner Storage Box | SFTP      | `sftp://<benutzer>.your-storagebox.de:23`            | Fingerabdrücke laut docs.hetzner.com/storage/storage-box/general; Port 23 nimmt OpenSSH-Schlüssel           |
| Nextcloud           | WebDAV    | `https://<server>/remote.php/dav/files/<benutzer>/`  | App-Passwort (bei Zwei-Faktor Pflicht)                                                                      |
| Koofr               | WebDAV    | `https://app.koofr.net/dav/Koofr/`                   | Benutzer ist die E-Mail; nur App-Passwort; nach mehreren falschen Passwörtern sperrt Koofr etwa eine Stunde |
| Infomaniak kDrive   | WebDAV    | `https://<kdrive-id>.connect.kdrive.infomaniak.com/` | Nicht in kSuite Free/Standard; App-Passwort bei Zwei-Faktor                                                 |
| Anderer Server      | WebDAV    | `https://`                                           | —                                                                                                           |
| Anderer Server      | SFTP      | `sftp://` (Port 22)                                  | —                                                                                                           |

Die Platzhalter `<…>` markiert das Formular als noch zu ergänzen (US1 Szenario 5). Nach einem Test mit
429 (Koofr) nennt holzi „Zu viele Versuche, später erneut“ statt „Zugangsdaten falsch“.

## R12 — Gleichzeitige Aufrufe je Verbindung

**Decision**: Ein Semaphor je Verbindung begrenzt gleichzeitige Aufrufe: WebDAV 4 Anfragen, SFTP 2
SSH-Sitzungen (in jeder sind Lesen und Schreiben gestaffelt). Hetzner erlaubt 10 Verbindungen je Konto und
sperrt darüber; Koofr arbeitet ohnehin nacheinander. Auflisten von Bäumen liest höchstens 4 Ordner
gleichzeitig.

## R13 — Datenmodell, Sync und Versionen

**Decision**: Eine Migration (nächste freie Nummer, Spec 048 nimmt eventuell vorher eine) ergänzt
`haex_storage_connections` um `protocol` (TEXT NOT NULL DEFAULT 's3'), `host_key` (TEXT) und
`host_key_confirmed_at` (TEXT); `HOLZI_TRIGGER_VERSION` steigt um eins (neue Spalten einer
CRDT-Tabelle). `haex_storages.bucket` bleibt und trägt bei WebDAV und SFTP den Ordnerpfad; die
Prüfregeln hängen am Protokoll der Verbindung (data-model.md).

**Rationale**: Geräte mit verschiedener Schema-Version synchronisieren nicht miteinander (der Handshake
lehnt ab, `sync/handshake_refusal_tests.rs:38`); ein älteres holzi sieht also nie eine Verbindung mit
unbekanntem Protokoll. Eine umbenannte Spalte (`bucket` → `folder`) kostete eine Tabellen-Neuanlage in
einer synchronisierten Tabelle ohne Nutzen.

## R14 — Tests ohne Netzdienste

**Decision**:

- **WebDAV-Einheitstests**: `wiremock` (schon Dev-Abhängigkeit) erkennt eigene Methoden
  (`method("PROPFIND")`, `method("MKCOL")`); damit Parser, Fehlerabbildung, `href`-Prüfung,
  Weiterleitungen, 409-Folge und Zwischendateien.
- **Zustandsbehaftete Tests** (Katalog der Erweiterungsfunktionen, SC-002; Dateibrowser): ein
  WebDAV-Server im Prozess über eine Dev-Abhängigkeit mit Speicher im RAM (Kandidat `dav-server`, beim
  Setup zu prüfen: Version, Lizenz, Verhalten bei `Depth`; Ersatz: ein kleiner Testserver auf `hyper`
  über einem temporären Ordner) und ein SFTP-Server im Prozess aus `russh::server::run_stream` und
  `russh_sftp::server::run` über einem temporären Ordner (`tokio::io::duplex` oder Loopback).
- **Gleichstand**: Der Katalog der Erweiterungsfunktionen aus Spec 038 läuft als eine Testfunktion je
  Fall gegen alle drei Stores (S3 gegen die vorhandene Testunterstützung).
- **E2E**: `drakkan/sftpgo` (v2.7.6, nur als Testbild) liefert SFTP und WebDAV in einem Container, mit
  festen Hostschlüsseln (gemountet), Benutzern aus einer JSON-Datei (`SFTPGO_LOADDATA_FROM`) und
  `SFTPGO_WEBDAVD__BINDINGS__0__PORT`. Neben RustFS in den E2E-Diensten; auf Android über
  `adb reverse` wie RustFS (`scripts/e2e/PLATFORMS.md`).
- Von Hand (SC-008): Hetzner Storage Box (WebDAV und SFTP) und Nextcloud.

## R15 — vault-sdk und haextension

**Decision**: Ein eigener PR im vault-sdk ergänzt die Typen der Speicherliste (`type`, `folder`) und die
Vorschläge (`webdav`, `sftp`) ohne Zugangsdaten. Erweiterungen, die nur Objekte hoch- und herunterladen,
brauchen keine Änderung (US3). Spec 038 und haextension `^4.x` bleiben die Grundlage; holzi lehnt einen
unbekannten `type` wie bisher ab.
