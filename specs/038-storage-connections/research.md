# Research: Speicherverbindungen (S3) und ihre Weitergabe an Erweiterungen

**Feature**: [spec.md](./spec.md) | **Plan**: [plan.md](./plan.md) | **Date**: 2026-10-05

Jede Entscheidung mit Begründung und verworfenen Alternativen. Pfade sind relativ zum Repository.

## R1 — S3-Client

**Entscheidung**: `rusty-s3` 0.10 (Standard-Features `rustcrypto` und `full`, ohne `aws-lc-rs`) für das
Signieren (AWS Signature V4 als vorsignierte Adressen) und das Lesen der XML-Antworten; die Übertragung
macht das vorhandene `reqwest` 0.13 mit rustls und ring.

**Begründung**: holzi hat keinen S3-Client. `rusty-s3` ist Sans-IO: Es erzeugt nur signierte Anfragen
und liest Antworten, wählt also weder TLS noch Laufzeit. So bleibt es bei einer HTTP-Schicht in holzi
(reqwest, rustls mit ring, wie in `extensions/web.rs` und den Adaptern), und Grenzen für Laufzeit und
Größe lassen sich genauso streamend durchsetzen wie in `web.rs`. Die Kryptografie kommt aus `hmac` und
`sha2` (RustCrypto, schon im Lock), die Zeit aus `jiff`, XML aus `instant-xml`. Es läuft auf Android
und iOS, weil nichts davon an eine Plattform gebunden ist.

**Verworfen**:

- `aws-sdk-s3`: eigene HTTP- und TLS-Schicht, standardmäßig aws-lc-rs (holzi lehnt aws-lc-rs ab),
  groß, eigene Laufzeitannahmen.
- `rust-s3` (haex-vault, 0.37): bringt eine eigene HTTP-Schicht mit; zwei HTTP-Stacks im Build.
- `object_store`: viel Umfang (mehrere Clouds, Multipart, Retry-Logik), der hier nicht gebraucht wird.
- Eigenes Signieren nach SigV4 über `hmac`/`sha2`: wenige hundert Zeilen, aber ein Signaturverfahren mit
  vielen Randfällen (Kodierung, kanonische Kopfzeilen); ein gepflegtes kleines Crate ist das bessere
  Risiko (Laziness Ladder: vorhandene Abhängigkeit → keine; dann kleinste neue).

## R2 — Zugangsdaten im Passwortmanager, unerreichbar für Erweiterungen

**Entscheidung**: Ein Eintrag im Passwortmanager bekommt einen **Eigentümer** (`owner`, Text, leer =
der Nutzer). Speicherverbindungen legen ihre Zugangsdaten als Eintrag mit `owner = 'storage'` an
(Titel „S3: <Name des Anbieters>“, Benutzername = Zugangsschlüssel, Passwort = Geheimnis, eigenes Feld
`sessionToken` wenn vorhanden). Neue Regel **Z14** in 034 (`contracts/access.md`): Ein Eintrag mit
Eigentümer ist für jeden Aufrufer außer `User` und `Internal { feature }` mit `feature == owner` nicht
vorhanden (`NotFound`, nicht in `list_headers`, nicht in den Agent-Kopfdaten, nicht als Quelle eines
Verweises), unabhängig von Freigaben. Die Speicherverbindungen lesen ihn als
`Caller::Internal { feature: "storage" }` mit einer im Code festen Freigabe.

**Begründung**: Die Recherche zeigt, dass 034 heute nichts vor einer Freigabe für `*` verbirgt außer dem
Papierkorb (`access::visible`, `items::item_state`). Spec 038 FR-012 und der Grundsatz des Betreibers
verlangen, dass keine Erweiterung je Zugangsdaten erreicht. Der Eigentümer sitzt am Eintrag selbst und
läuft durch denselben Pfad wie der Papierkorb (`ItemState`, `visible`, `headers_in_scope`,
`agent_headers`, Verweise über `references_db::visible`), also eine Regel an einer Stelle. Der Nutzer
sieht die Einträge weiter im Passwortmanager (Z1); ändert er dort die Zugangsdaten, gilt das für die
Verbindung. `Caller::Internal` gibt es schon für genau solche holzi-Funktionen (029 „s3-storage“), es
wird hier zum ersten Mal benutzt. Auf anderen Geräten hält Z14, weil der Sync nur zwischen Geräten mit
gleichem Schema läuft (Migrationszahl und Trigger-Version, Spec 024 FR-029, `sync/handshake.rs`): Ein
Gerät ohne die Regel bekommt die Einträge nicht.

**Verworfen**:

- Ein reserviertes Tag: 034 schließt reservierte Tags ausdrücklich aus (`contracts/access.md`), und ein
  Tag lässt sich vom Nutzer entfernen; dann wären die Zugangsdaten offen.
- Eine eigene Tabelle für Zugangsdaten außerhalb von 034: widerspricht Spec 038 FR-005 und 029
  (Zugangsdaten im Passwortmanager).
- Eine eigene Tabelle `item_owners` neben `haex_passwords_item_details`: zwei Zeilen, die zusammen
  angelegt und gelöscht werden müssen; ein Sync-Zwischenstand ohne Eigentümer-Zeile würde den Eintrag
  kurz offenlegen. Die Spalte im Eintrag hat diesen Zwischenstand nicht.

## R3 — Datenmodell und Sync

**Entscheidung**: Zwei Migrationen, damit Z14 (PR B) für sich prüfbar bleibt: `0027_passwords_owner`
(Spalte `owner` in `haex_passwords_item_details`) und `0028_storage_connections` mit zwei
synchronisierten Tabellen (`haex_storage_connections`, `haex_storages`) und einer geräteeigenen
(`storage_tests_no_sync` für das letzte Testergebnis je Speicher); `HOLZI_TRIGGER_VERSION` wird jeweils
erhöht. Kennungen als UUID, keine UNIQUE-Constraints in synchronisierten
Tabellen (wie `migrations_extensions.rs`). SQL in einer eigenen Datei
`src-tauri/src/identity/migrations_storage.rs`, registriert in `migrations.rs`.

**Begründung**: Spec FR-006 (vault-weit, 029 FR-033). Das letzte Testergebnis ist eine Beobachtung eines
Geräts (ein Gerät ohne Netz soll nicht allen anderen „Fehler“ melden), daher `_no_sync`.

**Verworfen**: Verbindung und Speicher in einer Tabelle (SDK-„Backend“ = beides): 029 legt Buckets auf
einer Verbindung an, die Verbindung muss ohne Bucket bestehen können (Spec Annahme, Begriffe).

## R4 — Bereich einer Erweiterung im Bucket

**Entscheidung** (Review 2026-10-06): Präfix `holzi-ext/<vault_id>/<extension_id>/`, mit
`extension_id = UUIDv5(NS_EXT, "<publicKey>:<name>")` (`extensions/ids.rs::extension_id`) und
`vault_id = UUIDv5(NS_VAULT, hex(vault_identity.pubkey))` (Vault-Identität aus Spec 024, nicht der
Schlüssel selbst; `NS_VAULT` neu in `extensions/ids.rs`). Eine Entwicklerversion bekommt ein eigenes Präfix
`holzi-ext-dev/<vault_id>/<dev_extension_id>/` aus ihrer geräteeigenen `dev_extension_id`, nie das der
installierten Fassung. Schlüssel der Erweiterung sind relativ zum Präfix.

**Begründung**: Die Kennung der Erweiterung hängt nur an Herausgeberschlüssel und Name, die der Vault an
ihrer Identität, die alle Geräte einer Vault teilen und die bei der Genesis entsteht. Beide sind also auf
allen Geräten und über Neuinstallationen gleich (FR-010), und das Präfix ist mit höchstens 88 Bytes kurz genug
für die 1024-Byte-Grenze von S3. Mit der Vault im Präfix teilen sich zwei Vaults auf demselben Bucket keinen
Bereich. Das Manifest einer Entwicklerversion ist nicht signiert; es könnte Herausgeberschlüssel und Namen
einer installierten Erweiterung nennen und so deren Objekte erreichen. Deshalb trennt holzi sie wie ihre
Tabellen (Spec 017, `extensions/dev.rs::conflict`). Eine Entwicklerversion sieht die Objekte der
installierten Fassung nicht, und ihre eigenen nur auf dem Gerät, auf dem sie geladen ist.

**Verworfen**:

- `TablePrefix` (`<publicKey>__<name>__`): bis über 100 Zeichen und mit dem Namen im Schlüssel; der
  öffentliche Schlüssel im Bucket verrät beim Anbieter mehr als nötig. Aus demselben Grund steht die
  Vault nur als abgeleitete Kennung im Präfix.
- Präfix ohne Vault (bis Review 2026-10-06): Zwei Vaults desselben Nutzers oder zweier Nutzer auf einem
  Bucket hätten denselben Bereich je Erweiterung.
- Entwicklerversion mit dem Präfix der installierten Fassung (bis Review 2026-10-06): siehe oben.

## R5 — Schlüsselprüfung

**Entscheidung**: Ein Schlüssel einer Erweiterung ist gültig, wenn er nicht leer ist, nicht mit `/`
beginnt, keinen leeren Teil (`//`), keinen Teil `.` oder `..`, kein Steuerzeichen (U+0000–U+001F,
U+007F) und keinen Rückstrich enthält, gültiges UTF-8 ist und zusammen mit dem Präfix höchstens 1024
Bytes lang ist. Ein Präfix fürs Auflisten folgt denselben Regeln, darf aber leer sein und mit `/`
enden. Antworten des Anbieters, deren Schlüssel nicht mit dem Präfix der Erweiterung beginnen, werden
verworfen (zweite Absicherung).

**Begründung**: FR-011, SC-003. S3 kennt keine Pfade, aber Anbieter und Werkzeuge deuten `/` und `..`
unterschiedlich; ein strenger Satz Regeln vor dem Aufruf hält die Bereiche sicher getrennt.

## R6 — Dialog von holzi für Erweiterungen

**Entscheidung**: Wie `extension_dialog_confirm` (`bridge/methods.rs`, `host.open_dialog`): Der
Bridge-Aufruf meldet `extension-storage-request {requestId, frame, kind, proposal}` und wartet
(`recv_timeout`, 300 s wie `DIALOG_WAIT`). Zwei Stufen (Review 2026-10-06):

1. Ein Vue-Dialog über dem Tab (`StorageDialog.vue`, im Rahmen wie `FrameDialog.vue`) zeigt Erweiterung,
   Vorschlag und die Wahl einer vorhandenen Verbindung und fragt nur nach Bestätigung. Er enthält **nie**
   Felder für Zugangsdaten.
2. Braucht die Bestätigung neue Zugangsdaten (neue Verbindung oder „neue Zugangsdaten“), öffnet holzi
   danach ein eigenes Fenster für die ganze App (`StorageCredentialsModal.vue`, an der Wurzel der App
   eingehängt, nicht in `ExtensionFrame.vue`). Es liegt über Tableiste und Werkzeugleiste von holzi, also
   über Flächen, die der Rahmen einer Erweiterung nie erreicht, und enthält als einzige Stelle neben
   Einstellungen → Speicher (FR-001) Felder für Zugangsdaten.

Beide antworten über den Command `storage_dialog_resolve(requestId, answer)`. Die Antwort mit
Zugangsdaten geht nur vom Fenster von holzi an Rust, nie an die Erweiterung; Rust legt an, testet und
gibt der Erweiterung nur Kennung oder Fehler. Schließen des Rahmens oder Ablauf zählt als Abbruch
(`drop_dialogs_of`), auch wenn das Fenster für die Zugangsdaten schon offen ist; es schließt dann.

Die Antwort mit Zugangsdaten wartet auf deren Test (FR-013b, Betreiber 2026-10-07):
`storage_dialog_resolve` liefert `StorageTrial` zurück. Scheitert der Test, bekommt das Fenster
`failed { outcome, leftoverKey }`, zeigt den Text der Einstellungen einschließlich eines möglichen
Bereinigungshinweises und bleibt offen; der Aufruf der Erweiterung
wartet im selben Dialog (`Dialog::failed`, `Dialog::answer`) auf korrigierte Zugangsdaten oder den
Abbruch, mit neuer Frist von 300 s je Antwort. Endet die Anfrage, bekommt das Fenster `ended`.

**Begründung**: Ein vorhandenes, getestetes Muster für „Erweiterung wartet auf den Nutzer“; FR-013,
FR-013a. Eine native Dialogbox (wie der Speichern-Dialog) kann keine Formularfelder. Was im Rahmen eines
Tabs liegt, kann eine Erweiterung pixelgleich nachzeichnen; ein Formular für Zugangsdaten dort könnte sie
also fälschen und die Eingabe selbst lesen. Eine Bestätigung ohne Felder zu fälschen bringt ihr nichts.
Lernt der Nutzer „Zugangsdaten nur im Fenster über der ganzen App“, erkennt er ein Formular im Tab als
falsch.

**Verworfen**:

- Rückfrage über den Weg der Berechtigungen (1004 und Wiederholung): Sie trägt keine Eingaben und keine
  Antwort an die Erweiterung (Kennung des neuen Speichers).
- Felder für Zugangsdaten im Dialog über dem Tab (bis Review 2026-10-06): fälschbar, siehe oben.

## R7 — Grenzen, Auflisten, Fehler

**Entscheidung**:

- Hochladen: Base64-Daten höchstens `max_response_bytes` (wie der Anfragekörper in `web.rs`), sonst 7000.
- Herunterladen: streamend gelesen, abgebrochen bei `max_response_bytes / 4 * 3` (Base64-Aufschlag, wie
  `web.rs::max_body`), sonst 7000.
- Eine Frist (`timeout_ms`) für den ganzen Aufruf, auch beim Auflisten über mehrere Seiten.
- Auflisten folgt den Seiten des Anbieters bis `max_rows` Objekte; gibt es mehr, antwortet holzi 7000 mit
  dem Hinweis, ein engeres Präfix zu nutzen (das SDK hat keine Fortsetzung).
- Fehler beim Anbieter: fehlender Schlüssel oder Bucket 1001; Zugangsdaten abgelehnt (403, 401,
  `InvalidAccessKeyId`, `SignatureDoesNotMatch`, `ExpiredToken`) 2002 mit `details.kind =
"accessDenied"`; Netz, Zeitüberschreitung, 5xx 2002 mit `kind = "network"`; nie Endpunkt, Kopfzeilen
  oder Antworttext des Anbieters in der Meldung.

**Begründung**: FR-014, FR-015 und die Grenzwerte von Spec 017 (`sql::exec::Limits`), kein neuer
Grenzwert.

## R8 — Unverschlüsselte Endpunkte

**Entscheidung** (Review 2026-10-06, Betreiber 2026-10-06): Jede Verbindung merkt sich beim Festlegen
ihres Endpunkts dessen Adressbereich (`endpoint_scope`): `local`, wenn der Host auf Loopback oder private
Adressen auflöst, sonst `public`. `local` ist erlaubt für einen Endpunkt, den der Nutzer in holzi
eingibt, und für den Vorschlag einer Erweiterung mit der Berechtigung `remoteStorage`/`add` für diesen
Host (FR-009b); ohne diese Berechtigung fragt holzi (1004) oder lehnt ab (1002), bevor der Dialog
erscheint. Vor jedem Aufruf, auch vor dem Verbindungstest, löst holzi den Host des Endpunkts selbst auf und
prüft **jede** aufgelöste Adresse:

- Immer abgelehnt: Link-Local (`169.254.0.0/16` samt `169.254.169.254` für Metadaten der Cloud,
  `fe80::/10`), unspezifizierte Adressen (`0.0.0.0`, `::`), Multicast und Broadcast, jeweils auch in
  IPv4-gemappter Form (`::ffff:a.b.c.d`).
- Loopback (`127.0.0.0/8`, `::1`, `localhost`) und private Adressen (RFC 1918, RFC 4193) nur bei
  `endpoint_scope = local`.
- Öffentliche Adressen nur bei `endpoint_scope = public`.

Ist eine aufgelöste Adresse nicht erlaubt, scheitert der Aufruf (2002 `network`, im Test „Endpunkt nicht
erreichbar“). Die Verbindung geht an genau die geprüfte Adresse (`reqwest::ClientBuilder::resolve` für
diesen Aufruf); reqwest löst den Namen nicht ein zweites Mal auf. Ein Name, der beim Festlegen öffentlich
war und später auf eine lokale Adresse zeigt (DNS rebinding), erreicht also nichts. Der Hostname bleibt
für TLS (SNI, Zertifikat) und die Signatur (`Host`).

`https` immer; `http` nur bei `endpoint_scope = local` und nie zu Link-Local; der Dialog und die
Einstellungen kennzeichnen ihn als unverschlüsselt und lokal. Ein Vorschlag einer Erweiterung mit `http`
auf einen Host, der nicht lokal auflöst, wird beim Aufruf abgelehnt (3001, vor dem Dialog). Ändert der
Nutzer den Endpunkt in den Einstellungen, legt holzi `endpoint_scope` neu fest. Keine Weiterleitungen folgen
(S3 antwortet mit Fehler statt Umleitung; eine Umleitung wäre ein Fehler 2002).

**Begründung**: FR-017, FR-009b. Selbst betriebenes RustFS im Heimnetz ohne Zertifikat ist ein Kernfall,
auch eingerichtet von einer Erweiterung (Betreiber 2026-10-06). Die Berechtigung `add` nennt den Host in
der Rückfrage, der Dialog zeigt den Endpunkt als lokal; ohne beides könnte eine Erweiterung signierte
Anfragen von holzi an Dienste im lokalen Netz schicken (SSRF). Den Metadatendienst einer Cloud-Maschine
erreicht sie nie. `extensions/web.rs` prüft aufgelöste Adressen nicht, darum die eigene Prüfung hier.

**Verworfen**: Prüfung nur des eingegebenen Namens oder nur beim Speichern (bis Review 2026-10-06):
DNS rebinding umgeht sie. Lokale Endpunkte nur vom Nutzer (`endpoint_origin`, Review 2026-10-06):
der Betreiber will, dass eine Erweiterung mit Berechtigung einen lokalen Speicher einrichten kann.

## R9 — Der Verbindungstest

**Entscheidung**: Schreiben, Lesen, Auflisten und Löschen eines Testobjekts
`holzi-test/<UUID>` im Bucket; Löschen auch, wenn ein Zwischenschritt scheitert. Scheitert das Löschen
selbst (Recht fehlt, Anbieter nicht erreichbar), nennt das Ergebnis den Schlüssel des Objekts, mehr kann
holzi nicht tun (SC-004). Ergebnis
„bestanden“ oder ein Grund aus R7 (Zugangsdaten falsch, Endpunkt nicht erreichbar, Bucket fehlt, Recht
fehlt). In den Einstellungen nur nach Bestätigung gespeichert; das Ergebnis landet in
`storage_tests_no_sync`.

**Begründung**: FR-003, FR-004, SC-004. Nur Lesen würde einen Schlüssel „nur Lesen“ als geeignet melden,
mit dem eine Erweiterung dann nicht hochladen kann.

## R10 — Tests ohne Netz

**Entscheidung**: Ein Trait `ObjectStore` (R21 von Spec 017) mit der S3-Umsetzung und einer Fälschung im
Speicher für die Tests der Bridge (Rechte, Präfix, Schlüssel, Grenzen, Dialog). Die S3-Umsetzung wird
gegen `wiremock` (schon Dev-Abhängigkeit, lokaler Listener) getestet: signierte Anfrage vorhanden,
Antworten lesen, Fehlerabbildung, Grenzen. Ein echter Anbieter (RustFS im Container) nur in der
Quickstart-Anleitung und im e2e-Rig, nicht in `cargo test`.

**Begründung**: Constitution: keine Netzdienste in Tests; `wiremock` ist ein lokaler Listener wie in
`web_tests.rs` und den Adapter-Tests.

## R11 — Anpassung des vault-sdk

**Entscheidung**: Eigener PR im vault-sdk (haex-space/vault-sdk, über den haexmas-Fork):

- `AddBackendRequest`: `{ name, type: "s3", config: { endpoint?, region?, bucket, pathStyle? },
sameProviderAs?: backendId }`, ohne Zugangsdaten. Mit `sameProviderAs` kommen Endpunkt, Region und
  Adressierung aus der vorhandenen Verbindung, `config` trägt dann nur `bucket` (Review 2026-10-06).
- `UpdateBackendRequest`: `{ backendId, name?, config?: { bucket? } }`, ohne Zugangsdaten.
- `StorageBackendInfo`: `{ id, type, name, providerName, bucket }`.
- `S3Config` mit Zugangsdaten als veraltet markiert und entfernt im nächsten Major.

holzi lehnt einen Aufruf mit `accessKeyId`, `secretAccessKey` oder `sessionToken` ab (3001), bevor ein
Dialog erscheint (FR-013a), egal mit welcher SDK-Fassung.

**Begründung**: Clarifications 2026-10-05 (Zugangsdaten nur in holzi; die Liste nur mit Namen).
