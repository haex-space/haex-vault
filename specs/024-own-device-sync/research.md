# Research: Vault-Identität, Geräteliste und Datensync zwischen eigenen Geräten

**Spec**: [spec.md](./spec.md) | **Plan**: [plan.md](./plan.md) | **Datum**: 2026-09-28

Jede Entscheidung nennt, was gewählt wurde, warum und was verworfen wurde. Fakten zu fremden
Bibliotheken stammen aus deren veröffentlichten Quellen (crates.io, docs.rs) oder aus haex-vault 2.x (Legacy)
(Repository `https://github.com/haex-space/haex-vault-legacy`, Revision
`8dce379d94e18fcd42c3b73686a06f984ca3f574`) und sind mit „geprüft“ markiert; alles andere ist
eine Entscheidung dieses Plans.

## R1 Abhängigkeiten

**Entscheidung**:

| Crate              | Version                        | Features                                          | Wozu                                                                        |
| ------------------ | ------------------------------ | ------------------------------------------------- | --------------------------------------------------------------------------- |
| `iroh`             | 1.2                            | `default-features = false`, `tls-ring`            | QUIC-Endpunkt, direkte Verbindungen, iroh-Relay für NAT                     |
| `nostr`            | 0.45                           | `nip44`, `nip59`                                  | Schlüssel, Schnorr-Signaturen, NIP-44-Umschläge, Präsenz-Umschläge          |
| `nostr-sdk`        | 0.45                           | Standard; `local-relay` nur in `dev-dependencies` | Client für Präsenz und Verknüpfungs-Treffpunkt; In-Prozess-Relay für Tests  |
| `secp256k1`        | 0.30 (dieselbe wie in `nostr`) | –                                                 | Prüfen von Schnorr-Signaturen über eigene Nutzdaten                         |
| `hkdf`             | 0.13                           | –                                                 | Ableitungen (Vault-Identität aus Platzhalter, Präsenz-Postfach, Treffpunkt) |
| `chacha20poly1305` | 0.10                           | `std`                                             | XChaCha20-Poly1305 für die Namen in der Geräteliste                         |
| `postcard`         | 1                              | `use-std`                                         | kompakte, deterministische Kodierung der Sync-Nachrichten                   |
| `qrcode`           | 0.14                           | `svg`                                             | Verknüpfungscode als SVG für die Oberfläche                                 |

`sha2`, `zeroize`, `base64`, `uuid`, `tokio`, `tokio-util`, `serde` sind schon da.

**Begründung**: iroh ist Betreibervorgabe; geprüft: iroh 1.2.0 ist die aktuelle stabile Version
(MSRV 1.91), haex-vault 2.x nutzt dieselbe Linie mit `tls-ring`. rust-nostr 0.45 hat die NIP-Features
nur noch im Crate `nostr`, nicht mehr in `nostr-sdk` (geprüft). `nostr::PublicKey` hat keine
Prüfmethode für fremde Nutzdaten; dafür dient `secp256k1` 0.30 direkt (geprüft, dieselbe Version
vermeidet doppelte Typen). postcard statt JSON für die Rahmen, weil Handshake, Geräteliste und
Momentaufnahme Binärdaten tragen und JSON sie als Base64 um ein Drittel aufbläht; die Änderungen
selbst bleiben `ColumnChange` von haex-crdt mit ihren JSON-Werten. Die Version steckt im ALPN (R6).

**Verworfen**: `irpc`/`irpc-iroh` (zusätzliche Abstraktion ohne Bedarf; der Protokollumfang ist
klein), `aws-lc-rs` als TLS-Backend (braucht cmake/nasm), ein eingebettetes Nostr-Relay (024
braucht nur Clients), `spake2` für das Verknüpfen (R11: der Code hat 128 Bit Zufall).

## R2 Schlüssel und wo sie liegen

**Entscheidung**:

- **Vault-Identität**: öffentlicher Schlüssel in der synchronisierten Tabelle `vault_identity`
  (nur noch `pubkey`); der private Schlüssel in der neuen Tabelle `vault_identity_secret_no_sync`,
  die nur auf Hauptgeräten eine Zeile hat. Migration: die heutige Spalte `privkey` in
  `vault_identity` entfällt (Tabelle wird neu aufgebaut; die Platzhalter-Zeilen hatten nie einen
  HLC-Zeitstempel und sind nie gesynct worden).
- **Geräteschlüssel** (secp256k1) und **iroh-Schlüssel** (ed25519): in
  `device_keys_no_sync(installation_uuid PK, device_secret, device_pubkey, endpoint_secret,
endpoint_id, created_at)`. Beim Öffnen sucht holzi die Zeile zur eigenen Installations-UUID; gibt
  es keine, erzeugt es neue Schlüssel (FR-003, FR-006). Zeilen anderer Installationen bleiben
  unberührt: Eine Kopie ist ein vollständiges Backup (FR-006), und der Schlüssel des Quellgeräts
  wird nie verwendet, weil holzi nur die Zeile der eigenen Installation liest.
- **Inhaltsschlüssel des Bereichs „Vault“**: nur als Umschläge in der synchronisierten Tabelle
  `vault_key_envelopes`; jedes Gerät entpackt seinen Umschlag selbst und hält den entpackten
  Schlüssel in `vault_content_keys_no_sync`. Eine neue Generation erreicht so nie ein entferntes
  Gerät.
- Alle geheimen Bytes in `Zeroizing`, `Debug` geschwärzt; `nostr::SecretKey` wird nur kurz für eine
  Operation gebaut (geprüft: sein `Drop` löscht nur nach Möglichkeit, `ConversationKey` ist
  `Copy` und wird nicht gehalten).

**Begründung**: `_no_sync` hält Geheimnisse vom Sync fern, liegt aber in der Datei. Eine Kopie
eines Hauptgeräts nimmt den privaten Schlüssel der Vault-Identität mit; das verlangt FR-044
ausdrücklich. Für den Geräteschlüssel schreibt FR-006 vor, dass die Kopie ihn nicht verwendet und
nicht löscht; der Schlüssel je Installations-UUID erfüllt das und hält alles in der verschlüsselten
Datei (auch im portablen Modus, Spec 014). Wer Datei und Passphrase hat, hat ohnehin alle Daten.

**Verworfen**: Geräteschlüssel in einer Datei neben der Vault (braucht eine eigene Verschlüsselung
und bricht den portablen Modus), im Schlüsselbund des Betriebssystems (plattformabhängig, auf Linux
nicht überall vorhanden), fremde Zeilen beim Öffnen einer Kopie löschen (beschneidet das Backup,
Betreiber-Entscheidung), Inhaltsschlüssel entpackt in einer synchronisierten Tabelle (würde eine
neue Generation mit dem Sync an Geräte tragen, die sie nicht bekommen sollen).

## R3 Vault-Identität aus dem Platzhalter (FR-004)

**Entscheidung**: Trägt eine Vault nur den Platzhalter, berechnet holzi beim ersten Öffnen
`sk = HKDF-SHA256(ikm = platzhalter_privkey, salt = "holzi", info = "holzi/vault-identity/v1" ||
zähler)` und erhöht `zähler`, bis `sk` ein gültiger secp256k1-Skalar ist. Jede Kopie rechnet
dasselbe und kommt zur selben Vault-Identität. Die Migration schreibt die synchronisierte
`vault_identity`-Zeile erst nach der HLC-Initialisierung in einer idempotenten
Bootstrap-Transaktion; dadurch erhält die Identität ihren ersten echten HLC. Jede Kopie ist danach
ein Hauptgerät und trägt sich in eine eigene Geräteliste der Generation 1 ein; treffen sich zwei
Kopien, gilt FR-043 und ein Hauptgerät veröffentlicht die zusammengeführte Generation 2.

**Begründung**: deterministisch, ohne Abstimmung zwischen den Kopien; der Platzhalter ist echter
Zufall aus `bootstrap.rs` und damit als Eingabe geeignet.

**Verworfen**: jede Kopie eine eigene Identität (trennt Kopien, die der Betreiber als eine Vault
behalten will, Clarification FR-004).

## R4 Abgleich je Ursprungsgerät über den HLC (FR-012, FR-013, FR-019, FR-020)

**Entscheidung**: Der Bereich „Vault“ gleicht den aktuellen Stand ab, wie haex-crdt es vorsieht.
Es gibt keine gespeicherten Pakete und keine Laufnummern.

- **Ursprungsgerät**: Jede Zelle trägt in ihrem Spalten-HLC die Knoten-ID des schreibenden Geräts;
  die Knoten-ID ist die `vault_device_uuid` der Installation (geprüft: `HolziBootstrap` liefert
  sie, `Database::open` initialisiert damit den HLC; `hlc_node_id_suffix`, `hlc_is_from_node` in
  `src/crdt/hlc.rs`). `apply` übernimmt den HLC der fremden Zelle unverändert und schaltet die
  Trigger dabei ab (geprüft, `engine.rs`), der Ursprung bleibt also über jede Weiterleitung
  erhalten. Das Feld `ColumnChange.device_id` füllt der Scanner mit dem scannenden Gerät; holzi
  verwendet es nicht und liest den Ursprung aus `hlc_timestamp`.
- **Fortschrittsstand**: `sync_progress_no_sync(origin, max_hlc)`, je Ursprungsgerät der höchste HLC,
  bis zu dem dieses Gerät alle Änderungen des Ursprungs angewendet oder nach R5 abgelehnt hat. Für
  sich selbst ist es der eigene jüngste HLC.
- **Liefern**: Die Gegenseite schickt ihren Fortschrittsstand. Der Sender scannt jede CRDT-Tabelle
  und das Lösch-Log (`haex_deleted_rows`) einmal ab dem kleinsten Cursor der Gegenseite
  (`scan_table_for_local_changes` mit `after_hlc`; geprüft: Zeilen werden nach dem Zeilen-HLC
  vorgefiltert, dann jede Spalte nach ihrem eigenen HLC), behält je Spalte nur, was jenseits des
  Cursors ihres Ursprungs liegt, sortiert alles nach HLC aufsteigend über alle Ursprünge hinweg und
  packt es in Seiten, die nie eine Transaktionsgruppe teilen (R6). Das schließt
  `vault_key_generations` und `vault_key_envelopes` ein; sie haben keinen zweiten Lieferweg. Ein
  Ursprung, den die Gegenseite nicht nennt, hat den Cursor „nichts“.
- **Anwenden**: Der Empfänger puffert unvollständige Gruppen über mehrere Seiten. Er prüft und
  wendet nur vollständige Gruppen über `Database::apply_remote_changes` in einer Transaktion an;
  erst nach deren Commit erhöht er je Ursprung den Fortschritt auf den höchsten HLC der Gruppe,
  angewendet oder abgelehnt. Für eine unvollständige Gruppe schreibt er weder Fortschritt noch
  Checkpoint. Bei einem Abbruch verwirft er den Puffer und fordert ab dem letzten dauerhaft
  geschriebenen Stand erneut an; `apply` überspringt dabei, was bereits vorhanden ist (gleicher
  HLC, LWW).
- **Lückenlos**: Weil jede Gruppe je Ursprung aufsteigend ist und der Fortschritt nur mit
  vollständig angewendeten oder abgelehnten Gruppen wächst, hat jedes Gerät von jedem Ursprung
  immer einen Anfang ohne Lücke. Ein Gerät, das über B nur einen Teil von A bekam, fordert beim
  nächsten Mal ab genau diesem Stand an, bei wem auch immer.
- **Überschriebene Spalten**: haex-crdt speichert je Spalte nur den aktuellen Wert mit seinem HLC.
  Hat P eine Spalte aus einer Transaktion von A schon überschrieben, gibt es A's alten Wert auf dem
  Sender nicht mehr; geliefert wird von dieser Transaktion, was noch gilt, und P's Spalte im Ursprung
  P. Das ist die heutige Arbeitsweise von haex-crdt. Weil die Seiten über alle Ursprünge nach HLC
  sortiert sind, folgt P's Spalte kurz danach, meist in derselben Seite; der Endstand stimmt immer
  (Edge Case in der Spec). Die vollständige alte Gruppe ließe sich nur mit einem Protokoll
  gesendeter Änderungen liefern, das wir verworfen haben.
- **Anstoßen**: Nach jedem lokalen Commit (R19) und nach jedem angewendeten Empfang sendet ein
  Gerät seinen neuen Fortschrittsstand an jedes verbundene Gerät; wer daraus erkennt, dass ihm
  etwas fehlt, fordert es an. Daten werden nie ungefragt geschoben, so entsteht auch beim Anstoßen
  keine Lücke.

**Begründung**: haex-crdt bringt alles mit: HLC je Transaktion, Ursprung im HLC, Filter nach
Cursor, Seiten ohne geteilte Transaktion, idempotentes `apply`. Ein eigenes Paketprotokoll würde
jede lokale Transaktion ein zweites Mal speichern, verschlüsseln und signieren, ohne dass eigene
Geräte das brauchen (Betreiber-Entscheidung beim Plan).

**Verworfen**: gespeicherte, signierte Änderungspakete mit Laufnummern je Ursprungsgerät (doppelte
Datenhaltung, vier Änderungen an haex-crdt, Nutzen erst bei Fremden; Laufnummern kommen dort mit
027/028), ein einziger Zeitpunkt „zuletzt synchronisiert“ je Gegenseite (verliert Änderungen über
Zwischengeräte), Daten ungefragt nach jedem Commit schieben (Lücken beim Empfänger).

**Zeilen aus mehreren Gruppen (gefunden vom Lasttest T048, 2026-10-01)**: Die Zellen einer Zeile können aus
verschiedenen Transaktionsgruppen stammen. Wurde eine Zeile angelegt und danach geändert, trägt die
Gruppe der Anlage nur noch die Zellen, die die Änderung nicht überschrieben hat. Der Pull liefert die
Gruppen nach HLC; trennen die Seiten beide, legte die erste Seite die Zeile ohne eine Spalte an, die
einen Wert braucht (`NOT NULL`, kein Standardwert), und `apply_remote_changes` scheiterte, bei jedem
Versuch an derselben Stelle. Bei 4 MiB je Seite trifft das einen Abgleich über mehr als eine Seite oder
einen dort abgebrochenen. Der Empfänger hält deshalb eine Gruppe zurück, deren Zeilen noch nicht
angelegt werden können, bis die fehlenden Zellen in einer späteren Gruppe desselben Pulls ankommen, und
der Fortschritt je Ursprung bleibt unter der ältesten zurückgehaltenen Gruppe. Bricht der Pull ab, geht
nichts verloren, was der nächste nicht noch einmal sendet; auf der letzten Seite wird angewendet, was
übrig ist. Die Alternative „unvollständige Zeile verwerfen“ (`ConstraintDecision::SkipRow` in
haex-crdt) verlöre die Zellen dauerhaft, da der Fortschritt weiterliefe.

## R5 Prüfen beim Empfang (FR-013, FR-014, FR-021, FR-028)

**Entscheidung**: holzi prüft vor `apply`, gruppiert nach Transaktions-HLC
(`group_by_hlc_key`, geprüft). Eine Gruppe hat genau einen Ursprung. Abgelehnt wird eine ganze
Gruppe, wenn ihr Ursprung in einer bekannten gültigen Geräteliste als entfernt steht und ihr HLC
jenseits seiner Grenze liegt. Berührt sie eine Tabelle, die dieses Gerät nicht als CRDT-Tabelle
kennt, bricht es den Pull mit einem Schemafehler ab und erhöht den Fortschritt nicht (das verhindert
schon der Vergleich der Schema-Version im Handshake, R14). Die übrigen Gruppen gehen an
`Database::apply_remote_changes` (heute mit `SignatureApplyPolicy` und `NoopSignatureProvider`,
geprüft `database/mod.rs`). Ein Ursprung, der auf keiner Geräteliste steht, wird angenommen, wenn
ein geprüftes eigenes Gerät ihn liefert: Das betrifft Daten aus der Zeit vor dieser Spec und Kopien
in der Wartezeit (FR-044).

Momentaufnahmen beim Verknüpfen prüft holzi auf dieselbe Weise.

**Begründung**: Im Bereich „Vault“ bürgt das liefernde Gerät (FR-021); die Prüfung richtet sich nur
gegen entfernte Geräte. Die Ablehnung folgt eindeutig aus Geräteliste und HLC: Dieselbe Gruppe wird
bei jedem Empfang wieder abgelehnt, deshalb darf der Fortschritt über sie hinausgehen, ohne dass
holzi sich abgelehnte Gruppen merkt. Weil die Prüfung vor `apply` stattfindet und der Fortschritt
nach dem Commit geschrieben wird, braucht holzi keine eigene `ApplyPolicy` und keinen neuen
Einstieg für `apply` in haex-crdt.

**Verworfen**: eine eigene `ApplyPolicy` über einen neuen Einstieg `apply_remote_changes_with_policy`
(nur nötig, um den Fortschritt im selben Commit zu schreiben; das ist wegen des idempotenten
`apply` unnötig), eine ganze Seite verwerfen, wenn eine Gruppe ungültig ist (gültige Gruppen würden
verloren gehen), abgelehnte Gruppen als gespeicherte Bereiche verbuchen (die Regel ist eindeutig;
gespeicherte Bereiche wüchsen ohne Grenze).

## R6 Transport: iroh-Endpunkt und Protokoll

**Entscheidung**:

- Ein Endpunkt je Vault-Session, erst nach dem Entsperren gebaut, mit dem gespeicherten
  iroh-Schlüssel: `Endpoint::builder(presets::Minimal)` (keine n0-DNS/pkarr-Adresssuche),
  `.secret_key(..)`, `.alpns([b"holzi-sync/1", b"holzi-link/1"])`,
  `.relay_mode(RelayMode::custom(urls))` aus den Einstellungen (Standard: die öffentlichen
  iroh-Relays von n0), eigene Adresssuche über `MemoryLookup`, gefüllt aus Präsenzmeldungen (R7). `bind()`
  mit 15 s Zeitgrenze (geprüft: haex-vault 2.x macht das, weil `bind` an der DNS-Auflösung der iroh-Relays hängen kann).
- Annehmen über `Router` mit je einem `ProtocolHandler` pro ALPN.
- **Gegenseitiger Handshake** auf dem ersten Bi-Stream, den der Annehmende öffnet: Herausforderung
  mit 32 Byte Zufall in beide Richtungen; jede Seite signiert mit ihrem Geräteschlüssel (Schnorr)
  `SHA-256("holzi-device-auth/v1" || len‖nonce_a || len‖nonce_b || len‖eigene endpoint_id ||
len‖fremde endpoint_id || len‖vault_pubkey)`. Geprüft wird: `endpoint_id` gleich
  `Connection::remote_id()`, Signatur passt zum Geräteschlüssel, dieser steht mit genau dieser
  `endpoint_id` auf der aktuellen Geräteliste und gilt nicht als entfernt. Muster aus haex-vault 2.x
  `quic_did_auth` (geprüft), dort nur einseitig; hier gegenseitig, weil die Bindung
  `endpoint_id ↔ Geräteschlüssel` sonst nur an der Präsenzmeldung hinge.
- **Nachrichten**: ein Bi-Stream je Anfrage, Rahmen `u32 BE Länge ‖ postcard`, die Länge wird vor
  dem Anlegen des Puffers geprüft. Höchstens 8 gleichzeitige Streams und höchstens ein `Pull` je
  Verbindung. Einzelheiten: [contracts/sync-protocol.md](./contracts/sync-protocol.md).
- **Größen** (Schutz vor Überlastung): vor `Accept` höchstens 64 KiB je Rahmen, nach dem Handshake
  höchstens 4 MiB. Diese Werte sind eine Wahl dieses Plans für den Speicher je Verbindung, keine
  Vorgabe von iroh oder Nostr; Sync-Daten gehen nie über Nostr. Eine Seite füllt einen Rahmen bis
  4 MiB. Ist eine einzelne Transaktionsgruppe größer, reist sie in mehreren aufeinanderfolgenden
  Rahmen derselben Seite (`Page { part, last }`), und der Empfänger wendet sie erst an, wenn sie
  vollständig ist. Eine Gruppe darf höchstens so groß sein wie die eingestellte Transaktionsgrenze
  (R19, Standard 100 MiB); größere lehnt schon das Schreiben ab. So hält keine große Transaktion den
  Abgleich an, und der Empfänger puffert je Verbindung höchstens `2 * max_transaction_bytes` plus
  1 MiB Slack an zurückgehaltenen Gruppen; dazu kommt höchstens eine noch nicht vollständige Gruppe
  derselben Grenze.
- **Ende der Session** (FR-031): der Sync-Dienst hängt am Abbruch-Token des Vault-Gates;
  `router.shutdown()` mit 2 s Zeitgrenze, danach alle Endpunkt-Klone freigeben (geprüft: UDP-Sockets
  schließen erst, wenn der letzte Klon weg ist).

**Verworfen**: `presets::N0` (veröffentlicht Adressen bei n0 über pkarr, widerspricht der
Präsenz über Nostr), ein einseitiger Handshake, ein großer Rahmen je Sync (bis 200 MB in
haex-vault 2.x; Seiten halten den Speicher klein), eine feste Obergrenze je Transaktionsgruppe im
Protokoll (eine größere lokale Transaktion hielte den Abgleich ihres Ursprungs für immer an;
`paginate_changes` liefert eine zu große Gruppe nach seiner „≥1-Regel“ ohnehin als eigene Seite,
geprüft).

## R7 Präsenz über Nostr (FR-007, FR-008, FR-010)

**Entscheidung**:

- **Postfach-Schlüssel**: `mb_sk = HKDF(inhaltsschlüssel der aktuellen Generation, info =
"holzi/presence/v1" || tagesnummer)`, daraus `mb_pk`. Nur Geräte mit dem Inhaltsschlüssel können
  ihn bilden; nach einem Entfernen wechselt er mit der Generation (FR-026).
- **Ereignis**: ein flüchtiges Ereignis der Art 21059 im Aufbau von NIP-59 (Siegel der Art 13 mit
  dem Geräteschlüssel signiert, außen ein zufälliger Einmalschlüssel, `p`-Tag `mb_pk`, Inhalt mit
  NIP-44 verschlüsselt), von Hand gebaut statt über `GiftWrapBuilder`. Inhalt: Geräteschlüssel,
  `endpoint_id`, URL des iroh-Relays und direkte Adressen, Generation der Geräteliste, Zeitstempel, Nonce.
  Flüchtige Arten speichert ein Nostr-Relay nicht (geprüft, NIP-01).
- **Zeitstempel**: `created_at` (das Zeitfeld jedes Nostr-Ereignisses, NIP-01) ist bei Gift-Wrap und
  Siegel die aktuelle Zeit, nicht wie bei gespeicherten NIP-59-Nachrichten zufällig in die
  Vergangenheit gelegt. Abos setzen kein `since` (den Zeitfilter eines Abos, NIP-01); flüchtige
  Ereignisse kommen ohnehin nur live. Frisch ist eine Meldung, wenn ihr innerer `ts` höchstens
  150 s alt ist.
- **Größen**: Eine Präsenzmeldung ist unter 1 KiB; holzi sendet nie ein Ereignis über 16 KiB und
  verwirft größere empfangene Ereignisse, bevor es sie entschlüsselt. Höchstens 1 Meldung je Gerät und
  Minute außer bei Adressänderungen. Ein Nostr-Relay bekommt von holzi damit nie mehr als wenige
  kleine Ereignisse je Gerät und Minute; seine eigenen Grenzen meldet es per NIP-11 (`limitation`).
- **Takt**: beim Öffnen der Vault, bei jeder Adressänderung (`watch_addr`) und alle 60 s. Wer
  gerade online ist, abonniert `kind 21059, #p = mb_pk` (auch für den Vortag, um den Tageswechsel
  abzudecken) und verbindet sich mit jedem neuen Gerät; so reicht es, dass eine Seite die andere
  hört. „Online“ gilt bis 150 s nach der letzten Meldung oder solange eine Verbindung besteht
  (SC-008: Wechsel binnen 60 s sichtbar).
- **Ein-Gerät-Vault** (FR-007): abonniert, veröffentlicht aber nicht; antwortet nur, indem es sich
  mit einem sich meldenden Gerät verbindet.
- **Server** (FR-008): voreingestellte öffentliche Nostr-Relays, die der Nutzer ändern kann; vor
  der Auslieferung prüft ein Task per NIP-11, dass sie flüchtige Ereignisse annehmen. Kandidaten
  (zu prüfen): `wss://relay.damus.io`, `wss://nos.lol`, `wss://relay.primal.net`. iroh-Relays:
  die n0-Standardrelays. Die eingebauten Server stehen immer in der Liste und lassen sich abschalten,
  nicht löschen; hinzugefügte lassen sich abschalten oder löschen.

**Begründung**: Präsenz nützt nur, wenn beide Geräte online sind, denn der Sync ist direkt; ein
gespeichertes Ereignis (1059) würde Nostr-Relays mit Meldungen füllen und wegen der Zeitverschiebung von
NIP-59 (bis zu 2 Tage, geprüft) eine lange Ablaufzeit brauchen. Die zufällige Verschiebung von
`created_at` verbirgt nur bei gespeicherten Ereignissen etwas; ein Nostr-Relay sieht die echte
Ankunftszeit, und flüchtige Ereignisse speichert es nicht. Mit Verschiebung würde ein Abo mit
`since` sie wegfiltern, und Nostr-Relays mit einer Untergrenze für `created_at` (NIP-11
`created_at_lower_limit`) wiesen sie ab. Der tägliche Postfach-Schlüssel
verhindert, dass ein Nostr-Relay dieselbe Vault über Tage verknüpft (D9 erlaubt Metadaten, verlangt es
aber nicht).

**Verworfen**: Art 1059 mit NIP-40-Ablauf (gespeichert, Ablauf vor 2 Tagen unzuverlässig),
Präsenz an jedes Gerät einzeln (mehr Ereignisse, Geräteanzahl sichtbar), mDNS im lokalen Netz
(nicht verlangt; die iroh-Relays decken auch das lokale Netz ab).

## R8 Geräteliste (FR-005, FR-043)

**Entscheidung**: Die Liste ist ein kanonisch kodierter Datensatz (postcard, feste Feldfolge) mit
Vault-Identität, Generation, `base_list_hash`, Einträgen (Geräteschlüssel, `endpoint_id`, Rolle,
`vault_device_uuid`, verschlüsselter Name), entfernten Einträgen (Geräteschlüssel,
`vault_device_uuid`, Grenze als HLC) und `issued_by`. Die Vault-Identität signiert den ganzen
Datensatz über `SHA-256("holzi-device-list/v1" || bytes)`; die Liste liegt als ein Block in einer
Spalte, die Signatur deckt also jedes Feld. Der Name ist mit einem aus dem Inhaltsschlüssel
abgeleiteten Schlüssel verschlüsselt (FR-005: Sync-Server und Mitglieder sehen ihn nicht).
Gespeichert in der synchronisierten Tabelle `device_lists(list_hash PK, generation, payload,
signature)`; die geltende Liste ist die höchste Generation, bei Gleichstand der kleinste Hash. Ein
Gerät gilt als entfernt, wenn es die maßgebliche Liste als entfernt führt. Jede Liste führt alle
Entfernungen ihrer Basisliste weiter.

- **`issued_by`** ist nur eine Angabe. Alle Hauptgeräte haben denselben privaten Schlüssel der
  Vault-Identität, auch ein entferntes; wer die Liste wirklich ausgestellt hat, lässt sich nicht
  fälschungssicher belegen. Eine zweite Signatur mit dem Geräteschlüssel hülfe nicht: Ein entferntes
  Hauptgerät kann über eine ältere Basisliste einen neuen Geräteschlüssel als Hauptgerät eintragen.
  Das ist die hingenommene Grenze aus FR-028.
- **Gleiche Generation** (FR-043): Gilt Liste L1 (kleinster Hash) und verliert L2, bestimmen nur
  L1s Geräte- und Entfernungsmenge den geltenden Stand; `issued_by` ist keine Prüf- oder
  Konfliktregel. Beim gegenseitigen Entfernen zweier Hauptgeräte bleibt so das Hauptgerät aus L1,
  das andere wird zum Solitär. Ein Hauptgerät, das beide Listen sieht, veröffentlicht eine Liste
  der nächsten Generation, die den geltenden Stand von L1 und zulässige neue Geräte aus L2
  zusammenführt, die geltenden Entfernungen weiterführt und mindestens ein Hauptgerät enthält.
- **Mindestens ein Hauptgerät**: Ein Hauptgerät kann sich nicht selbst entfernen (FR-026); mit der
  Regel für gleiche Generation nennt jede geltende Liste mindestens ein Hauptgerät.
- **Kopie eines Hauptgeräts** (FR-044): Sie trägt sich mit neuem Geräteschlüssel in eine neue Liste
  über der Liste aus der Datei ein. Dafür muss ihr Geräteschlüssel in keiner Vorgängerliste stehen.

**Begründung**: Die Liste reist als gewöhnliche Vault-Information (FR-005) und wird zusätzlich im
Handshake verglichen (FR-009), damit ein Gerät eine neuere Liste auch ohne vorherigen Sync kennt.
Gespeichert als unveränderliche Zeilen statt als eine überschriebene Zeile, damit „entfernt bleibt
entfernt“ aus allen je gesehenen Listen folgt. Aufräumen: R20.

**Verworfen**: `issued_by` als Prüfregel (kausal prior nicht entferntes Hauptgerät; nicht
fälschungssicher, sperrt aber die Kopie eines Hauptgeräts aus), eine Zusammenführung, die die
maßgebliche Liste ignoriert oder dadurch alle Hauptgeräte entfernt.

## R9 Inhaltsschlüssel und Umschläge (FR-015)

**Entscheidung**: Ein Inhaltsschlüssel sind 32 Zufallsbytes mit `key_id = SHA-256("holzi-key-id/v1"
|| schlüssel)[0..16]`, einer Generation und dem Bereich. Jede Generation referenziert den Hash der
gültigen Geräteliste, die sie ausstellt, und trägt eine Schnorr-Autorisierung des dort kausal
berechtigten Hauptgeräts, also eines Geräts, das in dieser Liste als Hauptgerät eingetragen ist; so
kann kein verknüpftes Gerät eine Generation ausstellen. Ein Umschlag referenziert dieselbe Liste und trägt zusätzlich eine
Schnorr-Autorisierung über Generation, Empfänger und verschlüsselten Inhalt. NIP-44 v2 läuft vom
Geräteschlüssel des ausstellenden Hauptgeräts an den Geräteschlüssel des Empfängers, Inhalt
`{bereich, generation, key_id, schlüssel}` (Format aus dem Entwurf, §5.2). `created_by` und `sender`
allein sind keine Autorisierung. In 024 verwendet ihn holzi für zweierlei: den
Postfach-Schlüssel der Präsenz (R7) und die Namen in der Geräteliste (XChaCha20-Poly1305 mit 24 Byte
Nonce und einem abgeleiteten Schlüssel `HKDF(inhaltsschlüssel, "holzi/device-name/v1")`, zusätzliche
Daten `generation ‖ device_pubkey`). Beides nutzt die höchste Generation, die an kein entferntes
Gerät verpackt ist (Entwurf §5.2). Änderungspakete mit diesem Schlüssel kommen mit Spec 026.

**Verworfen**: Änderungen auch zwischen eigenen Geräten mit dem Inhaltsschlüssel verschlüsseln (die
iroh-Verbindung ist schon mit TLS 1.3 verschlüsselt und beidseitig geprüft; eine zweite Schicht
schützt dort nichts zusätzlich).

## R10 Kodierung signierter Daten

**Entscheidung**: Signiert wird immer `SHA-256(domänen-tag ‖ kanonische bytes)` mit BIP-340;
kanonische Bytes sind postcard eines festen Structs. Domänen-Tags: `holzi-device-auth/v1`,
`holzi-device-list/v1`, `holzi-key-generation/v1`, `holzi-key-envelope/v1`,
`holzi-admission/v1`, `holzi-link/v1`, `holzi-link-resume/v1`; die Präsenzmeldung ist ein
Nostr-Ereignis und trägt dessen Signatur (R7). Signaturen
je Änderung gibt es in 024 nicht (FR-021); die Zellsignatur mit Laufnummer für gemeinsame Bereiche
legen Specs 027 und 028 fest und bauen dafür auf `column_sig_preimage*` von haex-crdt auf.

**Verworfen**: JSON für signierte Daten (Feldreihenfolge und Zahlformat sind nicht eindeutig).

## R11 Verknüpfen (FR-023 bis FR-025)

**Entscheidung**:

- **Code**: 16 Zufallsbytes, als Base32 in Vierergruppen (26 Zeichen) und als QR-Code (SVG aus
  `qrcode`), einmal verwendbar, 10 Minuten gültig. Auf dem Desktop gibt die Nutzerin den Code ein
  oder fügt ihn ein; einen Kamera-Scanner gibt es erst mit mobilen Geräten (FR-024 verlangt
  „scannen oder eingeben“).
- **Treffpunkt**: Das Hauptgerät abonniert, solange der Code gilt, flüchtige Ereignisse an
  `rv_pk = HKDF(code, "holzi/link/rendezvous/v1")`. Die neue Installation veröffentlicht dort ihre
  `endpoint_id` und Adressen (NIP-44 an `rv_pk`); das Hauptgerät verbindet sich über ALPN
  `holzi-link/1`.
- **Nachweis**: beide Seiten senden `HMAC-SHA256(HKDF(code, "holzi/link/proof/v1"), transkript)`
  über beide `endpoint_id`s und Geräteschlüssel; erst danach zeigt das Hauptgerät den Namen und die
  Rollenfrage (FR-024). 128 Bit Zufall machen einen Offline-Angriff auf das Transkript aussichtslos.
- **Übertragung**: nach der Bestätigung eine Momentaufnahme des Bereichs „Vault“ (alle Zellen mit
  ihren ursprünglichen HLCs, samt Lösch-Log, ohne gerätelokale Daten), der Fortschrittsstand, alle
  Inhaltsschlüssel-Generationen als Umschläge an das neue Gerät, die neue Geräteliste und bei
  gewählter Hauptgerät-Rolle der private Schlüssel der Vault-Identität, alles auf dieser
  Verbindung. Die neue Installation legt ihre Vault mit ihrer Passphrase an, wendet die
  Momentaufnahme an und behält bei Abbruch nichts (FR-025). Das Hauptgerät veröffentlicht die neue
  Geräteliste erst danach. H persistiert vor der Übertragung den neuen Listensatz und eine
  `link_id`; nach `LinkDone` wird die Veröffentlichung beim Öffnen idempotent abschließbar. N
  bewahrt bis zum Empfang der neuen Liste den angewendeten Transfer als `awaiting_publication`.
  Eine eigene Netz-Nachricht für die Wiederaufnahme ist nicht gebaut: geht `LinkDone` verloren,
  bleibt N verknüpft und die beiden Geräte treffen sich über die neuere Liste von N, siehe
  `contracts/sync-protocol.md`.)
- Die neue Installation muss dieselbe Version des Sync-Protokolls und des Vault-Schemas haben
  (FR-029); sonst bricht das Verknüpfen mit einer Meldung ab.

**Verworfen**: eine kopierte SQLite-Datei übertragen (die neue Passphrase kennt der Absender
nicht), SPAKE2 mit kürzerem Code (zusätzliche Kryptographie für einen Komfortgewinn, den der
QR-Code schon bringt).

## R12 Kopie der Vault-Datei und Aufnahmeanfrage (FR-006, FR-044, FR-045)

**Entscheidung**: Beim Öffnen erkennt holzi eine Kopie daran, dass `device_keys_no_sync` keine
Zeile für die eigene Installations-UUID hat, aber Zeilen anderer. Es erzeugt eigene Schlüssel in
einer neuen Zeile und legt über den vorhandenen `known_devices`-Ablauf eine neue
`vault_device_uuid` an; die neue Installation schreibt damit unter einer eigenen HLC-Knoten-ID. Die
Zeilen des Quellgeräts bleiben unverändert in der Datei (FR-006).

- Mit `vault_identity_secret_no_sync`: neue Geräteliste mit sich selbst als Hauptgerät, Hinweis an
  die Nutzerin.
- Ohne: signierte Aufnahmeanfrage (Name, Geräteschlüssel, `endpoint_id`), per Präsenzmeldung an
  die Vault; jedes Gerät, das sie empfängt, legt sie in der synchronisierten Tabelle
  `admission_requests` ab. Die Kopie synchronisiert nichts, bis sie auf der Geräteliste steht;
  ihre lokalen Änderungen tragen ihre neue Knoten-ID und gehen nach der Aufnahme mit dem
  gewöhnlichen Abgleich mit (R4).

## R13 Entfernen (FR-026 bis FR-028)

**Entscheidung**: Ein Hauptgerät stellt in einer Transaktion aus: Geräteliste Generation+1 ohne
das Gerät, mit dessen Grenze (sein Fortschrittsstand für das Gerät, also der höchste HLC, bis zu
dem es alle Änderungen von ihm hat, R4), und eine neue
Inhaltsschlüssel-Generation mit Umschlägen nur an die verbleibenden Geräte. Laufende Verbindungen
zum entfernten Gerät werden sofort geschlossen, sein Eintrag aus dem `MemoryLookup` genommen. Das
Neu-Unterschreiben von Mitgliederlisten (FR-026) ist in 024 leer, weil es noch keine Spaces gibt;
die Stelle ist als Erweiterungspunkt für 027/028 angelegt. Geheimnisse werden nicht verändert.

## R14 Doppelte Geräte und Versionen (FR-029, FR-030)

**Entscheidung**: Die Protokollversion steckt im ALPN; die Schema-Version (Migrationsstand von
holzi und haex-crdt-Triggerversion) wird im Handshake verglichen. Bei Abweichung: kein Sync, beide
zeigen „ein Gerät muss aktualisiert werden“. Doppelt: zwei gleichzeitige Präsenzmeldungen oder
Verbindungen mit demselben Geräteschlüssel, aber verschiedenen `endpoint_id`s, oder dieselbe
`vault_device_uuid` auf zwei Geräteschlüsseln → Sync mit beiden anhalten, Meldung.

## R15 Oberfläche

**Entscheidung**: `FederationView.vue` wird zur Unteransicht „Geräte“ mit Kopf (öffentliche
Vault-Identität als `npub`, kopierbar), Liste (Name, Rolle, dieses Gerät, online/zuletzt online,
Grund bei Problemen), auf Hauptgeräten „Gerät verknüpfen“, „Gerät entfernen“ und offene
Aufnahmeanfragen, darunter die Gruppe „Verbindungsserver“ (Nostr- und iroh-Relays). Aufbau nach
COSMIC/GNOME wie in Spec 023 (Gruppen und Zeilen aus haex-ui). Die Startseite bekommt „Mit einer
Vault verknüpfen“ (`LinkSheet.vue`). Live-Aktualisierung über ein Tauri-Ereignis
`sync-devices-changed`. Jede Handlung ist eine Aktion im Katalog von Spec 020; Verknüpfen,
Aufnehmen, Ablehnen und Entfernen sind nicht für Agenten freigegeben (FR-036).

## R16 Tests

**Entscheidung**:

- **Rust-Unit-Tests** in eigenen `_tests.rs`-Dateien: NIP-44-Testvektoren, Schnorr, HKDF-Ableitung
  aus dem Platzhalter, Regeln der Geräteliste (Generation, kleinster Hash, entfernt bleibt
  entfernt, gegenseitiges Entfernen mit genau einem verbleibenden Hauptgerät), Liefern je
  Ursprung (Cursor, Sortierung über alle Ursprünge, Seiten ohne geteilte Gruppe, eine Gruppe über
  mehrere Rahmen), Fortschritt erst nach dem Commit, Abbruch zwischen Seiten, Grenze eines
  entfernten Geräts, Prüfung je Gruppe, `Resync` nach abgelaufenen Löschvermerken, die Grenzen aus
  R20, und je Speicher-Modul jede heutige Anweisung durch den Transformer von haex-crdt (R19).
- **Rust-Integrationstests** unter `src-tauri/tests/`: mehrere Geräte im selben Prozess, jedes mit
  eigener Vault-Datei und eigenem iroh-Endpunkt (`presets::Minimal`, `RelayMode::Disabled`,
  `MemoryLookup` mit den Loopback-Adressen, geprüft aus haex-vault 2.x
  `owner_sync_integration_tests/helpers.rs`), Präsenz über `nostr_sdk::local_relay::MockRelay`.
  SC-003 (drei Geräte, 1.000 Änderungen, wechselnde Wege) läuft hier.
- **E2E** (Spec 016): zwei App-Prozesse, Nostr über ein kleines Test-Relay aus
  `nostr-sdk`/`local-relay`, das das E2E-Werkzeug startet, iroh direkt über Loopback; die Server
  setzt das Szenario über die Einstellungen. Szenarien für User Stories 1, 2, 3 und 5 (SC-011).
- Keine willkürlichen Wartezeiten in Tests (Constitution): Warten auf Ereignisse mit Zeitgrenze.

## R17 Leistung

Ein Abgleich scannt jede CRDT-Tabelle einmal ab dem kleinsten Cursor der Gegenseite und filtert je
Ursprung im Speicher; bei 1–10 Geräten und den heutigen Tabellen sind das wenige Dutzend Abfragen.
Im laufenden Betrieb liegen die Cursor dicht am aktuellen Stand, der Scan liefert nur die jüngsten
Zeilen. SC-001 (5 s) wird von Präsenz und Verbindungsaufbau bestimmt; bei bestehender Verbindung
stößt der Commit sofort den Abgleich an (R4, R19). Sollte der Scan je Tabelle messbar werden, kann
er sich auf die Tabellen beschränken, die haex-crdt als geändert markiert
(`scan_dirty_tables`).

## R18 Graphify-Abfrage

Abfragen: „where are vault identity, known_devices and extension table ownership defined?“ und
„which modules handle vault open/unlock, lifecycle close, onboarding start page and tauri command
registration?“ (Budget 1.000 bzw. 1.200, abgeschnitten). Kandidaten und Ergebnis:
`identity/bootstrap.rs`, `identity/migrations.rs`, `storage/known_devices.rs`, `device/commands.rs`
werden erweitert; `instances/presence.rs` meint Prozesse, nicht Geräte, und passt nicht;
`vault_gate` liefert Abbruch-Token und Task-Tracker für den Sync-Dienst. Ein Sync-Modul gibt es
nicht; `src-tauri/src/sync/` ist neu. Der Graph ist vom 2026-09-21 und älter als 022/023; die
Kandidaten wurden im Code geprüft (zur manuellen Nachprüfung vermerkt).

**Nachprüfung nach dem Bau von `sync/` (2026-10-01, T080)**: Mit einer Suche im Code statt einer neuen
Graphify-Abfrage (der Graph ist weiter veraltet): Außerhalb von `src-tauri/src/sync/` kommen die Namen
„Geräteliste“, „Inhaltsschlüssel“, „Vault-Identität“ nur in `identity/migrations.rs` (Tabellen) und in
der Verdrahtung (`lib.rs`, `state.rs`) vor. `instances/presence.rs` meint weiter Prozesse, nicht
Geräte. `chat/send_admission.rs` meint das Zulassen einer Chat-Nachricht zum Senden und hat mit
`sync/admission` (Aufnahme einer Kopie) nichts zu tun; beide Namen sind durch ihr Modul eindeutig.
Doppelte Begriffe oder Module gibt es nicht. Die Dateigrößen sind geprüft: Im Bereich `sync/`, in den
Sync-Integrationstests und den neuen E2E-Szenarien ist keine Datei über 500 Zeilen, und in keiner
Produktionsdatei stehen Tests.

## R19 Schreiben über den CRDT-Weg von haex-crdt (Betreiber-Entscheidung)

**Befund** (geprüft): holzi hat keinen allgemeinen Befehl wie `sql_execute`/`sql_execute_with_crdt`
in haex-vault 2.x. Chat, Einstellungen, Sitzung, Provider, Modelle, Wartung und Freigaben greifen mit
56 Aufrufen in 15 Produktionsdateien selbst über `haex_crdt::Database::with_connection` zu (dazu
14 Integrationstestdateien), den Notausgang hinter
dem Feature `raw-connection`, jede mit eigenem `spawn_blocking` und eigener Fehlerumwandlung. Die
Arbeit des CRDT-Transformers macht holzi dabei von Hand: Jede INSERT- und UPDATE-Anweisung auf eine
CRDT-Tabelle muss `haex_hlc_no_sync = current_hlc()` selbst setzen (`storage/mod.rs`, „Etappe-0
finding #2“). Die Trigger übernehmen den HLC nur aus diesem Feld (geprüft, `crdt/trigger/mod.rs`:
`WHEN NEW.haex_hlc_no_sync IS NOT NULL`). Fehlt es, ist die Schreibung für den Sync unsichtbar,
ohne dass etwas fehlschlägt. Außerdem öffnet in den meisten Dateien keine Stelle eine Transaktion;
eine Stelle mit mehreren Anweisungen (etwa `rename_thread`: lesen, dann schreiben) läuft in mehreren
Transaktionen mit je eigenem HLC und ist nicht atomar, auch ohne Sync.

Das damalige `execute_with_crdt` auf `DbConnection` setzte den HLC über den Transformer selbst,
auch in `ON CONFLICT … DO UPDATE` (geprüft, `insert_transformer.rs`), und verbot Schreibungen auf
CRDT-Metaspalten, konnte aber nur eine Anweisung je Transaktion, nahm Parameter nur als JSON (keine
BLOBs) und hatte eine feste Größengrenze.

**Entscheidung**:

- **haex-crdt** (umgesetzt in haexmas/haex-crdt#34 bis #36, gepinnt in holzi mit voller SHA;
  [contracts/haex-crdt-upstream.md](./contracts/haex-crdt-upstream.md)):
  `Database::write(|tx: &mut CrdtTransaction| …)` öffnet eine `IMMEDIATE`-Transaktion mit einem
  HLC; darin laufen beliebig viele `tx.execute`, `tx.query_map` und `tx.query_row`, jede Schreibung
  durch den Transformer. Tabellen mit Endung `_no_sync` lässt der Transformer am Namen unberührt,
  derselbe Aufruf schreibt also auch gerätelokale Tabellen. Parameter sind `&[&dyn ToSql]`, BLOBs
  eingeschlossen. Die Größengrenze gilt für die Summe der Transaktion, gemessen mit
  `serialized_parameter_bytes`, und kommt aus `DatabaseConfig.max_transaction_bytes`.
  `Database::read(|conn: &ReadOnlyConnection| …)` liest; `PRAGMA query_only` und ein Authorizer
  weisen dort jede Schreibung ab. Fehler kommen typisiert an (`Error::Database(DatabaseError)`,
  `Error::Consumer` für eigene Fehler aus dem Closure, `Error::sqlite_error()`). Die alte
  `DbConnection`-Schicht aus haex-vault 2.x mit `execute_with_crdt`, `select_with_crdt` und den Hooks
  ist entfernt; `select_with_crdt` änderte unter harten Löschungen an einem `SELECT` nichts.
- **holzi**: Ein Modul `src-tauri/src/storage/vault_db.rs` kapselt das asynchron (`spawn_blocking`,
  einheitliche Umwandlung in `HolziError`): `read(|conn| …)` und `write(|tx| …)`. Nach dem Commit
  stößt `write` den Sync-Dienst an (`tokio::sync::Notify`); der fasst dicht folgende Anstöße
  zusammen und schickt verbundenen Geräten seinen Fortschrittsstand (R4).
- Alle Zugriffe ziehen um; jedes von Hand gesetzte `haex_hlc_no_sync = current_hlc()` entfällt
  (haex-crdt lehnt es ab: `CrdtMetaColumnWriteForbidden`).
- `Database::with_connection` steht danach in `clippy.toml` unter `disallowed-methods`. Ausnahmen mit
  Begründung nur für Wartung (`PRAGMA`, `VACUUM` in `storage/maintenance.rs`) und die Tests. Der
  Sync-Dienst nutzt `scan_table_for_local_changes` und `apply_remote_changes` von `Database`.
- Der erste Umsetzungsschritt lässt jede heutige Anweisung durch den Transformer laufen (Tests je
  Speicher-Modul), bevor die Stellen umziehen; sqlparser muss holzis SQL verstehen.

**Begründung**: Der Weg, den haex-crdt für CRDT-Schreibungen vorsieht, statt der eigenen Konvention;
eine vergessene HLC-Zuweisung kann es nicht mehr geben; ein Ort für Transaktion, Größengrenze, Fehler
und Anstoß des Sync statt vieler; atomare Schreibvorgänge unabhängig vom Sync. Die Signaturen je Änderung in gemeinsamen Bereichen
(Specs 027, 028) hängen sich später an `Database::write`.

**Verworfen**: das damalige `execute_with_crdt` direkt (eine Anweisung je Transaktion, keine BLOBs),
eine eigene `write`-Hülle über dem rohen Weg mit von Hand gesetztem HLC (die Konvention bliebe, die
Größengrenze greift dort nicht), ein Commit-Beobachter in haex-crdt (behandelt das Symptom, die vielen
Stellen blieben), ein allgemeiner SQL-Befehl für die Oberfläche wie in haex-vault 2.x (holzi schreibt
nur aus dem Backend; ein SQL-Befehl wäre eine neue Angriffsfläche für Erweiterungen und Agenten).

## R20 Begrenztes Wachstum

**Entscheidung**: Keine Tabelle wächst ohne Grenze.

| Tabelle                   | Wächst mit            | Regel                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                               |
| ------------------------- | --------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `haex_deleted_rows`       | jeder Löschung        | holzi räumt heute nie auf (geprüft: kein Aufruf von `cleanup_deleted_rows`). Neu: Löschvermerke älter als 90 Tage werden gelöscht (`RetentionPolicy::TimeBasedDays { days: 90 }`). Nennt ein `Pull` für irgendeinen Ursprung einen Stand vor dieser Frist, liefert der Sender keine Seiten, sondern `Resync`: Das veraltete Gerät schickt zuerst seine eigenen, noch nicht übertragenen Änderungen (die Gegenseite holt sie per `Pull`) und ersetzt danach seine synchronisierten Tabellen durch eine Momentaufnahme. Sonst tauchten gelöschte Einträge wieder auf. |
| `device_lists`            | jeder Geräteänderung  | Jede Liste führt alle Entfernungen weiter (R8). Eine ältere Liste darf erst gelöscht werden, wenn keine behaltene Liste mehr in `base_list_hash` auf sie verweist; Listen gleicher Generation bleiben, bis die zusammengeführte Liste da ist. So bleibt für jede behaltene Liste der gesamte gültige Ahnenpfad erhalten.                                                                                                                                                                                                                                            |
| `sync_progress_no_sync`   | jedem Ursprungsgerät  | eine Zeile je Ursprung, keine abgelehnten Bereiche (R5); Zeilen entfernter Geräte fallen mit ihrer Liste weg, sobald ihr Stand die Grenze erreicht hat.                                                                                                                                                                                                                                                                                                                                                                                                             |
| `admission_requests`      | Kopien                | Nach jeder Zusammenführung bleiben deterministisch nur die 20 kleinsten offenen Anfragen nach `(requested_at, device_pubkey)` offen; alle übrigen werden `rejected`. Nach „Aufnehmen“ oder „Ablehnen“ gelöscht; offene nach 30 Tagen gelöscht.                                                                                                                                                                                                                                                                                                                      |
| `pending_links_no_sync`   | Verknüpfungen         | nach Abschluss gelöscht; ohne Abschluss nach 24 h, oder wenn die Nutzerin das Verknüpfen abbricht.                                                                                                                                                                                                                                                                                                                                                                                                                                                                  |
| `vault_key_*`             | jedem Entfernen       | je Generation 32 Byte je Gerät. Die Tabellen dürfen erst mit einem Release aktiviert werden, für das Spec 026 eine konkrete, begrenzte Aufbewahrung und den Resync veralteter Geräte definiert; bis dahin ist die Aktivierung ein Release-Gate. Damit gilt „Keine Tabelle wächst ohne Grenze“ für den aktivierten Funktionsumfang.                                                                                                                                                                                                                                  |
| `device_presence_no_sync` | jedem Gerät der Liste | Zeile beim Entfernen des Geräts gelöscht.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                           |

Die Aufräumarbeiten laufen nach dem Öffnen im vorhandenen Wartungsablauf (`storage/maintenance.rs`)
und nach jedem Abgleich für die Löschvermerke.

**Stand der Umsetzung (2026-10-01)**: Umgesetzt und getestet sind `haex_deleted_rows` (90 Tage),
`pending_links_no_sync` (24 Stunden, beim Öffnen), `admission_requests` (nach jeder Zusammenführung und
beim Öffnen) und `device_presence_no_sync` (beim Entfernen). Bewusst **nicht** umgesetzt sind zwei Regeln:

- `device_lists`: Eine Liste zu löschen ist ein synchronisiertes Löschen. Der Löschvermerk erreicht die
  anderen Geräte, die eine andere Sicht haben können: Kennt ein Gerät schon eine Liste höherer
  Generation, die auf die gelöschte aufbaut, verliert es deren Ahnenpfad, und die höhere Liste gilt
  dort nicht mehr. Löschbar wären ohnehin nur Zweige, die verloren haben; der Ahnenpfad jeder
  behaltenen Liste bleibt. Der Gewinn ist eine kleine Zeile je gleichzeitiger Änderung, das Risiko eine
  verlorene Geräteliste.
- `sync_progress_no_sync` entfernter Geräte: „Fehlt ein Ursprung, heißt das: nichts“ (`progress.rs`).
  Ohne Zeile holte jeder Abgleich alle Änderungen des entfernten Geräts erneut; setzte man stattdessen
  die Grenze ein, hielte man ein Gerät, das nie Änderungen des entfernten Geräts erhielt, für
  vollständig, und diese Änderungen fehlten für immer. Eine Zeile je entferntes Gerät wächst nur mit
  Entfernungen, also mit Handlungen der Nutzerin.

Beide Regeln bleiben offen, bis sich ein Weg findet, der kein Löschen auf fremder Sicht und keine
falsche Vollständigkeit braucht (etwa ein lokales Löschen mit Tombstone).

**Begründung**: Jede synchronisierte Tabelle, die nur wächst, macht jede Momentaufnahme und jeden
Scan teurer; Löschvermerke sind heute schon ohne Grenze. Die Fristen sind großzügig, weil ein
zu früh gelöschter Löschvermerk gelöschte Daten zurückbringt.
