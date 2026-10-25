# Data Model: Vault-Identität, Geräteliste und Datensync zwischen eigenen Geräten

**Spec**: [spec.md](./spec.md) | **Research**: [research.md](./research.md)

Tabellen mit Endung `_no_sync` bleiben auf dem Gerät (haex-crdt nimmt sie vom Sync aus); alle
anderen sind CRDT-Tabellen und reisen mit dem Sync. Migrationsnummern schließen an die letzte
vorhandene an (heute `0020_wm_session_no_sync` und folgende; beim Umsetzen prüfen). haex-crdt
bringt keine neue Tabelle und keine neue Trigger-Version; die Erweiterung dort betrifft nur den
Schreibweg (contracts/haex-crdt-upstream.md). Alle Schreibungen laufen über `storage/vault_db.rs`
(research R19); Grenzen für das Wachstum jeder Tabelle stehen in research R20.

## Tabellen

### `vault_identity` (geändert, synchronisiert)

| Spalte   | Typ        | Regeln                                                  |
| -------- | ---------- | ------------------------------------------------------- |
| `id`     | INTEGER PK | immer `1`                                               |
| `pubkey` | BLOB(32)   | x-only secp256k1, unveränderlich nach dem Anlegen (D26) |

Die Spalte `privkey` entfällt. Die Migration legt den Platzhalter als Ausgangswert in
`vault_identity_secret_no_sync` (SQL kann nicht ableiten); sie ändert die synchronisierte
Identitätszeile nicht, die neu angelegte Tabelle bleibt leer. Erst nachdem die HLC-Strukturen und das eigene Gerät initialisiert
sind, führt `sync::genesis::ensure_sync_state` eine eigene, wiederholbare Transaktion aus: Sie
ersetzt den Ausgangswert durch die nach R3 abgeleitete Identität, schreibt `pubkey` in die Zeile
und erzeugt damit deren ersten HLC-Zeitstempel. Eine neue Vault bekommt dabei einen frischen
Ausgangswert. Liegt `pubkey` noch nicht vor, ist die Zeile in `vault_identity_secret_no_sync` also
ein Ausgangswert, sonst der private Schlüssel. Bricht dieser
Bootstrap ab, wird dieselbe Transaktion beim nächsten Öffnen anhand der unveränderlichen abgeleiteten
Identität erneut ausgeführt; es entsteht kein zweiter Identitätsdatensatz.

### `vault_identity_secret_no_sync` (neu, gerätelokal)

| Spalte    | Typ        | Regeln                                                                 |
| --------- | ---------- | ---------------------------------------------------------------------- |
| `id`      | INTEGER PK | immer `1`                                                              |
| `privkey` | BLOB(32)   | nur auf Hauptgeräten; nie in Protokollen, Oberfläche, Agenten (FR-002) |

Eine Zeile gibt es genau dann, wenn dieses Gerät ein Hauptgerät ist. Sie entsteht beim Anlegen
der Vault, beim Verknüpfen mit Hauptgerät-Rolle (FR-024) und kommt mit der Kopie eines Hauptgeräts
mit (FR-044).

### `device_keys_no_sync` (neu, gerätelokal)

| Spalte              | Typ      | Regeln                                     |
| ------------------- | -------- | ------------------------------------------ |
| `installation_uuid` | TEXT PK  | aus `<AppLocalData>/installation-id`       |
| `device_secret`     | BLOB(32) | secp256k1, verlässt das Gerät nie (FR-003) |
| `device_pubkey`     | BLOB(32) | x-only                                     |
| `endpoint_secret`   | BLOB(32) | ed25519 für iroh                           |
| `endpoint_id`       | BLOB(32) | öffentlicher iroh-Schlüssel                |
| `created_at`        | INTEGER  | ms seit Epoch                              |

Nach dem Öffnen (`sync::genesis`): gibt es keine Zeile zur eigenen Installation, entsteht eine neue
mit neuen Schlüsseln.
Zeilen anderer Installationen bleiben unverändert und werden nie gelesen (Kopie, FR-006, R12).

### `device_lists` (neu, synchronisiert, nur Einfügen)

| Spalte       | Typ         | Regeln                                                            |
| ------------ | ----------- | ----------------------------------------------------------------- |
| `list_hash`  | BLOB(32) PK | `SHA-256(payload)`                                                |
| `generation` | INTEGER     | ≥ 1                                                               |
| `payload`    | BLOB        | kanonische Geräteliste (postcard), siehe unten                    |
| `signature`  | BLOB(64)    | Schnorr der Vault-Identität über `holzi-device-list/v1 ‖ payload` |

Zeilen werden nie geändert oder gelöscht. Geltende Liste: höchste Generation, bei Gleichstand
kleinster `list_hash` (FR-043). Ein Gerät ist entfernt, wenn irgendeine gültige Zeile es als
entfernt führt (FR-005).

**Payload `DeviceList`**:

| Feld             | Inhalt                                                                                                                                                                                                                                                                                                  |
| ---------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `vault`          | öffentlicher Schlüssel der Vault-Identität                                                                                                                                                                                                                                                              |
| `generation`     | Generation                                                                                                                                                                                                                                                                                              |
| `devices[]`      | `device_pubkey`, `endpoint_id`, `role` (`main` \| `linked`), `vault_device_uuid`, `name_sealed` (Name, verschlüsselt mit einem aus dem Inhaltsschlüssel abgeleiteten Schlüssel, FR-005; Format `key_id ‖ nonce(24) ‖ Chiffretext`, zusätzliche Daten `Schlüsselgeneration ‖ device_pubkey`), `added_at` |
| `removed[]`      | `device_pubkey`, `vault_device_uuid`, `limit_hlc` (Grenze beim Entfernen: Fortschrittsstand des Ausstellers für dieses Gerät, FR-028), `removed_at`                                                                                                                                                     |
| `issued_by`      | Geräteschlüssel des ausstellenden Hauptgeräts                                                                                                                                                                                                                                                           |
| `issued_at`      | ms seit Epoch                                                                                                                                                                                                                                                                                           |
| `base_list_hash` | Hash der kausal bekannten gültigen Vorgängerliste; bei der ersten lokalen Liste NULL                                                                                                                                                                                                                    |

Prüfregeln: Signatur der Vault-Identität über den ganzen Datensatz; `base_list_hash` verweist, außer
bei der ersten Liste, auf eine gültige Liste mit niedrigerer Generation; die Liste führt alle
Entfernungen ihrer Basisliste weiter; kein Gerät steht zugleich in `devices` und `removed`; kein
Geräteschlüssel und keine `vault_device_uuid` doppelt; mindestens ein Gerät mit Rolle `main`.
`issued_by` ist nur eine Angabe und wird weder als Berechtigung noch als Bedingung für die
Übernahme geprüft (research R8). Bei zwei gültigen Listen derselben Generation ist die mit dem
kleinsten Hash maßgeblich; nur ihre Geräte- und Entfernungsmengen bestimmen den geltenden Stand.
Eine zusammengeführte Liste der nächsten Generation muss die geltenden Entfernungen dieser Liste
weiterführen und mindestens ein Hauptgerät enthalten (FR-005, FR-043).

### `vault_key_generations` (neu, synchronisiert, nur Einfügen)

| Spalte             | Typ         | Regeln                                                                                                      |
| ------------------ | ----------- | ----------------------------------------------------------------------------------------------------------- |
| `key_id`           | BLOB(16) PK | `SHA-256("holzi-key-id/v1" ‖ schlüssel)[0..16]`                                                             |
| `scope`            | TEXT        | in 024 immer `vault`                                                                                        |
| `generation`       | INTEGER     | steigt mit jedem Entfernen (FR-015)                                                                         |
| `created_by`       | BLOB(32)    | Geräteschlüssel des Ausstellers                                                                             |
| `created_at`       | INTEGER     | ms                                                                                                          |
| `device_list_hash` | BLOB(32)    | Hash der gültigen Geräteliste, die diese Generation ausstellt; bei einer Entfernung genau deren Transaktion |
| `authorization`    | BLOB(64)    | Schnorr des ausstellenden Hauptgeräts über den kanonischen Datensatz samt `device_list_hash`                |

### `vault_key_envelopes` (neu, synchronisiert, nur Einfügen)

| Spalte             | Typ                     | Regeln                                                                                          |
| ------------------ | ----------------------- | ----------------------------------------------------------------------------------------------- |
| `key_id`           | BLOB(16)                | → `vault_key_generations`                                                                       |
| `recipient`        | BLOB(32)                | Geräteschlüssel des Empfängers                                                                  |
| `sender`           | BLOB(32)                | Geräteschlüssel des Ausstellers                                                                 |
| `envelope`         | TEXT                    | NIP-44 v2, Inhalt `{scope, generation, key_id, key}`                                            |
| `device_list_hash` | BLOB(32)                | Geräteliste, deren autorisierte Generation diesen Umschlag erlaubt                              |
| `authorization`    | BLOB(64)                | Schnorr eines aktuellen Hauptgeräts über Generation, Empfänger, Umschlag und `device_list_hash` |
| PK                 | (`key_id`, `recipient`) |                                                                                                 |

`created_by` und `sender` sind nur Datenfelder. Eine Zeile wird erst angenommen, wenn die referenzierte
Geräteliste gültig ist, `device_list_hash` die konkrete autorisierende Listen-Transaktion bezeichnet,
die Generation zu dieser Liste gehört und die jeweilige `authorization` mit dem Schlüssel eines in
der referenzierten Liste als Hauptgerät eingetragenen Geräts verifiziert. So kann kein verknüpftes
Gerät eine Generation oder einen Umschlag ausstellen. Bei einer Entfernung muss die
Generation genau an die Liste gebunden sein, die diese Entfernung und ihre `limit_hlc` enthält.

### `vault_content_keys_no_sync` (neu, gerätelokal)

| Spalte       | Typ         | Regeln                                                |
| ------------ | ----------- | ----------------------------------------------------- |
| `key_id`     | BLOB(16) PK |                                                       |
| `generation` | INTEGER     |                                                       |
| `key`        | BLOB(32)    | entpackt aus dem eigenen Umschlag; nie in Protokollen |

### `sync_progress_no_sync` (neu, gerätelokal)

| Spalte    | Typ     | Regeln                                                                                      |
| --------- | ------- | ------------------------------------------------------------------------------------------- |
| `origin`  | TEXT PK | `vault_device_uuid` des Ursprungsgeräts = Knoten-ID im HLC                                  |
| `max_hlc` | TEXT    | höchster HLC, bis zu dem alle Änderungen dieses Ursprungs angewendet oder nach R5 abgelehnt |

Geschrieben nur nach dem Commit einer vollständigen Transaktionsgruppe und nur nach oben. Für eine
unvollständige Gruppe werden weder Fortschritt noch Checkpoint geschrieben; bei einem Abbruch wird
der Puffer verworfen. Abgelehnte Gruppen eines entfernten Geräts werden nicht gespeichert: Die
Regel aus research R5 lehnt sie bei jedem Empfang wieder ab.
Für das eigene Gerät wird der Fortschritt nicht gespeichert, sondern aus dem jüngsten eigenen
Spalten-HLC in einer synchronisierten Tabelle gebildet; der gespeicherte HLC von haex-crdt taugt
dafür nicht, weil auch Schreibungen auf gerätelokale Tabellen ihn erhöhen. Eine Zeile für das
eigene Gerät entsteht nur, wenn ein anderes Gerät eigene Änderungen liefert, die diese Datei nicht
hat (etwa nach dem Wiederherstellen einer älteren Kopie). Fehlt ein Ursprung, gilt „nichts“.

### `pending_links_no_sync` (neu, gerätelokal)

| Spalte               | Typ         | Regeln                                                                                            |
| -------------------- | ----------- | ------------------------------------------------------------------------------------------------- |
| `link_id`            | BLOB(32) PK | zufällige Kennung des Verknüpfungsvorgangs                                                        |
| `peer_device_pubkey` | BLOB(32)    | Gerät der Gegenseite                                                                              |
| `new_list_hash`      | BLOB(32)    | noch nicht veröffentlichte neue Geräteliste                                                       |
| `new_list_payload`   | BLOB        | vollständige kanonische Geräteliste                                                               |
| `new_list_signature` | BLOB(64)    | Signatur der neuen Liste                                                                          |
| `role`               | TEXT        | `main` \| `linked`                                                                                |
| `resume_secret`      | BLOB(32)    | reserviertes lokales Sitzungsgeheimnis für eine nicht umgesetzte `LinkResume`, nie synchronisiert |
| `state`              | TEXT        | `transferring` \| `awaiting_publication` \| `completed`                                           |
| `created_at`         | INTEGER     | ms; nach 24 h ohne Abschluss wird der Datensatz gelöscht                                          |

H legt den Datensatz vor `LinkTransfer` an und löscht ihn erst nach dem idempotenten Commit der
neuen Geräteliste. N legt ihn vor `LinkDone` an und behält ihn, bis die neue Liste eingetroffen ist.
Ein Neustart kann damit die Veröffentlichung ohne den verbrauchten Einmalcode wiederholen. Eine
Netz-Wiederaufnahme mit `LinkResume` ist nicht umgesetzt; ein gespeicherter Transfer findet den
Host stattdessen über die gewöhnliche Gerätelisten-Synchronisation. Bricht die Nutzerin das
Verknüpfen ab oder ist der Datensatz älter als 24 h, wird er gelöscht (R20).

### `admission_requests` (neu, synchronisiert)

| Spalte              | Typ         | Regeln                                                            |
| ------------------- | ----------- | ----------------------------------------------------------------- |
| `device_pubkey`     | BLOB(32) PK | der Kopie                                                         |
| `vault_device_uuid` | TEXT        | Knoten-ID der Kopie; ihre Änderungen aus der Wartezeit tragen sie |
| `endpoint_id`       | BLOB(32)    |                                                                   |
| `name`              | TEXT        |                                                                   |
| `requested_at`      | INTEGER     | ms                                                                |
| `signature`         | BLOB(64)    | Schnorr der Kopie über `holzi-admission/v1 ‖ …`                   |
| `state`             | TEXT        | `open` \| `admitted` \| `rejected`                                |

Übergänge: „Aufnehmen“ (Hauptgerät, gleichzeitig neue Geräteliste) löscht die Zeile, und `open →
rejected` („Ablehnen“) behält sie als Merkzeichen: Die Kopie wiederholt ihre Anfrage, und eine
gelöschte Zeile ließe sie sofort wieder erscheinen. Eine abgelehnte Zeile wird nicht wieder
geöffnet. Nach jeder Zusammenführung sortiert jedes Gerät offene Anfragen deterministisch nach
`(requested_at, device_pubkey)` und lässt nur die ersten 20 offen; alle weiteren werden idempotent
zu `rejected`. So führt dieselbe zusammengeführte Menge auf allen Geräten zum selben Ergebnis.
Jedes Gerät löscht nach der Zusammenführung Zeilen von Geräten, die auf der geltenden Liste
stehen oder dort entfernt sind, sowie Zeilen (offen oder abgelehnt), die älter als 30 Tage sind
(R20). Der Zustand `admitted` kommt nicht vor: Wer aufgenommen ist, steht auf der Liste.
`vault_device_uuid` kommt aus der signierten Anfrage.

### `device_presence_no_sync` (neu, gerätelokal)

| Spalte           | Typ          | Regeln                                                                 |
| ---------------- | ------------ | ---------------------------------------------------------------------- |
| `device_pubkey`  | BLOB(32) PK  |                                                                        |
| `last_seen`      | INTEGER      | jüngster Zeitpunkt aus Verbindung, Präsenz oder drittem Gerät (FR-033) |
| `endpoint_addr`  | BLOB         | letzte bekannte Erreichbarkeit (postcard `EndpointAddr`)               |
| `problem`        | TEXT NULL    | `incompatible_version` \| `duplicate` \| NULL                          |
| `peer_protocol`  | INTEGER NULL | Spec 059: `SchemaVersion.protocol` des Geräts im letzten Handshake     |
| `peer_migration` | INTEGER NULL | Spec 059: `SchemaVersion.holzi_migration`                              |
| `peer_trigger`   | INTEGER NULL | Spec 059: `SchemaVersion.crdt_trigger`; alle drei `NULL` = unbekannt   |

„Zuletzt online“ über ein drittes Gerät: Geräte tauschen im Handshake ihre `last_seen`-Werte aus
und übernehmen den jeweils jüngeren. Die Zeile eines entfernten Geräts wird gelöscht.

### `known_devices` (vorhanden, synchronisiert)

Bleibt die Quelle für Installation ↔ `vault_device_uuid` und den änderbaren Namen (Spec 023). Neu
ist keine Spalte; die Verbindung zum Geräteschlüssel führt die Geräteliste über
`vault_device_uuid`.

## Werte, die nicht in Tabellen liegen

- **Seite** einer Lieferung: `ColumnChange`-Werte von haex-crdt (Tabelle, PKs, Spalte, HLC, Wert),
  über alle Ursprünge nach HLC aufsteigend, nie eine Transaktionsgruppe geteilt
  (contracts/sync-protocol.md). Nicht gespeichert; entsteht beim Senden aus dem aktuellen Stand.
- **Präsenzmeldung**: flüchtiges Ereignis Art 21059 an `mb_pk` (R7), Inhalt `{device_pubkey,
endpoint_id, iroh_relay_url, direct_addrs, device_list_generation, ts, nonce}`.
- **Verknüpfungscode**: 16 Byte Zufall, `expires_at`, `used`; nur im Speicher des Hauptgeräts.

## Zustände

**Rolle dieses Geräts** (abgeleitet, nicht gespeichert):

```
auf geltender Geräteliste, Zeile in vault_identity_secret_no_sync → Hauptgerät
auf geltender Geräteliste, keine Zeile                            → verknüpftes Gerät
nicht auf der Liste, offene Aufnahmeanfrage                       → wartet auf Aufnahme
in irgendeiner gültigen Liste als entfernt                        → entfernt (kein Sync mehr)
```

**Verbindung zu einem anderen Gerät**: `unbekannt → gefunden (Präsenz) → verbindet → geprüft
(Handshake, FR-009) → synchronisiert ↔ getrennt`; von jedem Zustand nach `abgewiesen` (nicht auf
der Liste, entfernt, falsche Signatur, Version, doppelt).
