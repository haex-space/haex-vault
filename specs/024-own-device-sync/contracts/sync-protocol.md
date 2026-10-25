# Contract: Sync-Protokoll zwischen eigenen Geräten (`holzi-sync/1`, `holzi-link/1`)

Transport: iroh 1.2, QUIC. Jede Anfrage ist ein eigener Bi-Stream. Rahmen: `u32 BE Länge ‖
postcard(Nachricht)`, die Länge wird vor dem Anlegen des Puffers geprüft: vor `Accept` höchstens
64 KiB, danach höchstens 4 MiB. Diese Grenzen sind eine Wahl dieses Plans für den Speicher je
Verbindung (research R6), keine Vorgabe von iroh oder Nostr. Nach dem letzten Rahmen einer Richtung
`finish()`. Höchstens 8 gleichzeitige Bi-Streams und ein `Pull` je Verbindung. Unbekannte
Nachrichten oder zu große Rahmen schließen die Verbindung mit einem Fehlercode.

## ALPN `holzi-sync/1`

### 1. Handshake (erster Bi-Stream, vom Annehmenden geöffnet)

```text
A = annehmendes Gerät, D = wählendes Gerät

A → D  Challenge   { v: 2, nonce_a: [u8;32], endpoint_a: [u8;32], list: ListRef,
                     schema: SchemaVersion }
D → A  DeviceListPush*   (wenn die geltende Liste von D vor der von A rangiert)
D → A  Response    { v: 1, device_d: [u8;32], vault: [u8;32], nonce_d: [u8;32],
                     schema: SchemaVersion, list: ListRef, sig_d: [u8;64] }
A → D  DeviceListPush*   (wenn die geltende Liste von A vor der von D rangiert)
A → D  Accept      { device_a: [u8;32], schema: SchemaVersion, list: ListRef, sig_a: [u8;64] }
       | Reject    { code: RejectCode }
```

- `sig_x = schnorr(device_x, SHA-256("holzi-device-auth/v1" ‖ lp(nonce_a) ‖ lp(nonce_d) ‖
lp(endpoint_x) ‖ lp(endpoint_y) ‖ lp(vault)))`, `lp` = `u32 BE Länge ‖ Bytes`.
- `ListRef = { generation, list_hash }`. Eine Liste rangiert vor einer anderen bei höherer Generation
  oder bei gleicher Generation mit kleinerem Hash. Die Seite mit der vorrangigen Liste schickt sie
  samt ihren Basislisten (älteste zuerst) als `DeviceListPush` auf dem Handshake-Stream, vor ihrer
  nächsten Handshake-Nachricht; deshalb nennt schon `Challenge` die Liste von A. Der Empfänger
  prüft Hash, Vault-Signatur, Generation, Basisliste und Inhalt nach FR-005 und übernimmt die
  Listen atomar. `issued_by` wird dabei als Information gespeichert, ist aber keine Berechtigungs-
  oder Übernahmebedingung. Erst danach wird die geltende Liste neu gewählt. Höchstens 64 Listen je
  Handshake.
- Vor dem Listenabgleich prüft jede Seite weiterhin `endpoint` des Gegenübers gleich
  `Connection::remote_id()`, die Authentifizierungssignatur, `vault` gleich der eigenen und
  `schema` verträglich. Nach dem vollständigen Listenabgleich prüft sie mit der aktualisierten
  geltenden Liste erneut, dass der Geräteschlüssel mit genau diesem `endpoint` eingetragen und
  nicht entfernt ist. Erst dann wird `Accept` gesendet; andernfalls gilt `NotOnList` oder `Removed`.
- `RejectCode`: `ForeignVault`, `NotOnList`, `Removed`, `BadSignature`, `Incompatible`,
  `Duplicate`, `Closing`. Vor `Accept` fließt nur die Geräteliste (FR-009).

`SchemaVersion = { protocol: 2, holzi_migration: u32, crdt_trigger: u32 }`.

**Spec 059 (Versionsstände der Geräte)**: `Challenge.schema` ist der Stand des annehmenden Geräts und das
letzte Feld, `v` und `SchemaVersion.protocol` sind `PROTOCOL_VERSION` (2). Beide Seiten halten den Stand,
den sie sehen, für das Gerät fest, das die geltende Liste für den Endpunkt der Verbindung nennt, bevor sie
entscheiden, also auch bei `Reject`. Das wählende Gerät liest die `Challenge` als ganzen Rahmen und prüft
`v` (das Byte nach dem Variantenkennzeichen), bevor es dekodiert; bei einem anderen `v` endet es mit
`Incompatible`, ohne zu antworten. Vor `Accept` fließen damit Geräteliste und die drei Zahlen des
Versionsstands (FR-009). Die ALPN bleibt `holzi-sync/1`. Einzelheiten:
[specs/059-device-versions/contracts/sync-protocol.md](../../059-device-versions/contracts/sync-protocol.md).

### 2. Nachrichten nach dem Handshake

Der Handshake-Stream bleibt als Kontroll-Stream offen; auf ihm senden beide Seiten `Progress`
(und später `DeviceListPush`). Jeder `Pull` öffnet einen eigenen Bi-Stream: der Anfragende schreibt
`Pull` und schließt seine Richtung, der Sender antwortet mit `Page`s und schließt dann seine. Zwischen
zwei Geräten lebt höchstens eine Verbindung; wählen sich beide gleichzeitig an, bleibt die, die das
Gerät mit der kleineren `endpoint_id` gewählt hat.

| Nachricht        | Richtung | Inhalt                                                                              | Antwort                                   |
| ---------------- | -------- | ----------------------------------------------------------------------------------- | ----------------------------------------- |
| `DeviceListPush` | beide    | `{ payload, signature }`                                                            | –                                         |
| `Progress`       | beide    | `{ vector: Vec<(origin: Uuid, max_hlc: String)>, last_seen: Vec<(device, ms)> }`    | –                                         |
| `Pull`           | beide    | `{ vector: Vec<(origin, max_hlc)>, replace: bool }` (eigener Stand des Anfragenden) | `Page`s bis `more = false`, oder `Resync` |
| `Page`           | beide    | `{ changes: Vec<Change>, group_continues: bool, more: bool, served }` (≤ 4 MiB)     | –                                         |
| `Resync`         | beide    | `{ reason: TombstonesExpired }`                                                     | Ablauf „Resync“ unten                     |

`Change = { table, row_pks, column, hlc, value, continues }`: `ColumnChange` von haex-crdt ohne
`device_id` und `sig`; `value` ist der JSON-Text des Werts in der Kodierung von haex-crdt (BLOB als
`{"$blob_hex": …}`), weil postcard keinen `serde_json::Value` tragen kann. Ein Wert, der allein nicht
in eine Seite passt, reist in Teilen: Jeder Teil außer dem letzten trägt `continues`, die Teile einer
Zelle folgen aufeinander. `served` ist nur auf der letzten Seite (`more = false`) gefüllt: der
Fortschrittsstand des Senders, gegen den er den `Pull` bedient hat.

Ablauf:

1. Nach dem Handshake senden beide `Progress`. Nach jedem lokalen Commit (research R19) und nach
   jeder angewendeten Seite sendet ein Gerät erneut `Progress` an jedes verbundene Gerät, dicht
   folgende Anstöße zusammengefasst.
2. Wer im `Progress` der Gegenseite für irgendeinen Ursprung einen höheren `max_hlc` sieht als im
   eigenen Stand, schickt `Pull` mit seinem eigenen Stand.
3. Der Sender scannt jede CRDT-Tabelle und das Lösch-Log ab dem kleinsten Cursor im `Pull`,
   einschließlich `vault_key_generations` und `vault_key_envelopes`; Schlüsselgenerationen und
   Umschläge haben damit keinen zweiten Lieferweg. Fehlt ein Ursprung, gilt der Cursor „nichts“. Er
   behält je Spalte, was jenseits des Cursors ihres Ursprungs liegt, mit ihrem ursprünglichen HLC.
   Von einer Transaktion, deren Spalten zum Teil schon von jüngeren Änderungen überschrieben sind,
   liefert er die Spalten, die noch gelten; die alten Werte gibt es nicht mehr (research R4). Er
   sortiert nach HLC aufsteigend über alle Ursprünge und schickt `Page`s, die nie eine
   Transaktionsgruppe teilen. Gerätelokale Tabellen scannt er nie.
4. Passt eine Gruppe nicht in einen Rahmen, reist sie in mehreren `Page`s mit
   `group_continues = true` bis auf die letzte. Der Empfänger puffert sie und wendet sie erst an,
   wenn sie vollständig ist. Er berechnet ihre Größe mit derselben kanonischen
   `serialized_parameter_bytes`-Regel wie `CrdtTransaction`; übersteigt sie
   `DatabaseConfig.max_transaction_bytes`, bricht er den `Pull` ab.
5. Der Empfänger prüft jede vollständige Transaktionsgruppe je Seite (research R5), wendet die
   gültigen Gruppen in einer Transaktion an und erhöht danach je Ursprung seinen Fortschritt auf
   den höchsten HLC der vollständig angewendeten oder abgelehnten Gruppe. Für eine unvollständige
   Gruppe schreibt er weder Fortschritt noch Checkpoint. Bricht die Verbindung vorher ab, verwirft
   er den Puffer; der nächste `Pull` beginnt beim zuletzt dauerhaft geschriebenen Fortschritt.
6. Der Sender liest seinen Fortschrittsstand vor dem Scan und liefert je Ursprung nichts, was darüber
   liegt; Anwenden und Liefern schließen sich auf einem Gerät gegenseitig aus. Nach der letzten
   Seite hebt der Empfänger seinen Fortschritt je Ursprung auf `served`. Ohne diesen Schritt fände
   er nach einem Commit, der nur gerätelokale Tabellen berührt, immer wieder „mehr“ beim Sender.
   Der eigene Stand eines Geräts ist deshalb auch nicht der gespeicherte HLC von haex-crdt (den
   jede Schreibung erhöht), sondern der jüngste eigene Spalten-HLC in einer synchronisierten
   Tabelle.

**Resync** (research R20): Nennt ein `Pull` für irgendeinen Ursprung einen Stand, der älter ist als
die Frist für Löschvermerke, antwortet der Sender mit `Resync`. Das veraltete Gerät lässt die
Gegenseite zuerst seine eigenen Änderungen per `Pull` holen, dann fordert es eine Momentaufnahme an
(`Pull` mit leerem Stand und dem Merkmal `replace`) und ersetzt damit seine synchronisierten Tabellen
in einer Transaktion.

Daten gehen nie ungefragt über die Leitung; so entsteht beim Empfänger keine Lücke (FR-019).
`ColumnChange.device_id` wird nicht gesendet oder beim Empfang ignoriert; der Ursprung ist der
Knoten im `hlc_timestamp`.

## ALPN `holzi-link/1` (Verknüpfen)

```text
H = Hauptgerät, N = neue Installation; beide kennen den Code c

N findet H über den Treffpunkt (contracts/nostr-events.md) und wählt H an.
H → N  Hello      { nonce_h, endpoint_h, device_h }
N → H  Proof      { nonce_n, endpoint_n, device_n, vault_device_uuid, name, schema, mac_n }
H → N  HostProof  { mac_h }
       -- H zeigt Name und Rollenfrage; wartet auf die Nutzerin --
H → N  Decision   { accepted: bool }
H → N  Page(page)  -- wiederholt für jede Snapshot-Seite
H → N  Transfer   { link_id, envelopes, list_payload, list_signature,
                    vault_secret: Option<SecretBytes> }
N → H  Done       { link_id }
       -- H veröffentlicht die neue Geräteliste --
```

- `k = HKDF-SHA256(c, info = "holzi/link/proof/v1")`,
  `mac_x = HMAC-SHA256(k, "holzi-link/v1" ‖ lp(nonce_h) ‖ lp(nonce_n) ‖ lp(endpoint_h) ‖
lp(endpoint_n) ‖ lp(device_h) ‖ lp(device_n) ‖ rolle_x)`, `rolle_x` ∈ {`H`, `N`}.
- Falscher `mac`, abgelaufener oder verbrauchter Code: Verbindung zu, nichts übertragen (FR-024).
  Der Code ist verbraucht, sobald `Hello` gesendet wurde.
- H erzeugt vor `Transfer` eine zufällige `link_id` und legt einen dauerhaften
  `pending_link`-Datensatz mit `link_id`, dem vollständigen neuen Gerätelistensatz und dem Zustand
  `transferring` an. N speichert `link_id` und den Zustand `awaiting_publication`, bevor es
  `Done` sendet. Beide Seiten behalten dafür das aus dem einmal verwendeten Code abgeleitete
  Link-Sitzungsgeheimnis; der Code selbst muss nicht erneut vorgelegt werden.
- Eine Netz-Wiederaufnahme mit `Resume { link_id, state, mac }` ist in dieser Implementierung nicht
  umgesetzt. `Resume` und das zugehörige Sitzungsgeheimnis bleiben für eine spätere Protokollversion
  reserviert. Ein Absturz zwischen `Done` und dem ersten Listen-Commit wird stattdessen beim Öffnen
  durch `finish_pending` idempotent abgeschlossen; der verbrauchte Code muss dafür nicht erneut
  vorgelegt werden.
- `snapshot` sind `Page`s wie in Abschnitt 2 mit dem Stand „nichts“ für jeden Ursprung: alle Zellen
  des Bereichs „Vault“ mit ihren ursprünglichen HLCs, samt Lösch-Log, ohne gerätelokale Tabellen.
  `progress` ist der Fortschrittsstand von H nach der letzten vollständig angewendeten
  Transaktionsgruppe. N prüft je vollständiger Transaktionsgruppe (FR-013) und wendet an;
  unvollständige Gruppen ändern weder Fortschritt noch Checkpoint. `key_generations` und
  `envelopes_for_n` sind dabei nur der initiale
  Link-Transfer; im anschließenden gewöhnlichen Sync laufen diese Zeilen ausschließlich über
  `Pull`/`Page`.
- Bricht die Verbindung ab, bevor N den vollständigen Transfer gespeichert hat, ist der Link nicht
  abgeschlossen. Hat N dagegen alles gespeichert und kommt `Done` nicht bei H an, bleibt N
  verknüpft (die Vault bleibt in `awaiting_publication`), und H behält seinen Datensatz im Zustand
  `transferring`; H veröffentlicht nichts. Die beiden finden sich, sobald beide online sind, über
  die gewöhnliche Begegnung: N trägt die Liste mit sich, die H nicht hat (höhere `list_generation`
  in der Präsenz, Kandidat, einmal anwählen, R7), genau wie eine Kopie, die sich selbst aufgenommen
  hat (FR-007). Sobald das neue Gerät auf Hs wirksamer Liste steht, löscht H den Datensatz
  (`host::drop_listed`); sonst nach 24 Stunden. Liegt der Datensatz schon in
  `awaiting_publication`, veröffentlicht H ihn beim Öffnen wie beschrieben.
- `vault_secret` wird nur bei gewählter Hauptgerät-Rolle gesendet und nur auf dieser Verbindung
  (FR-038).
