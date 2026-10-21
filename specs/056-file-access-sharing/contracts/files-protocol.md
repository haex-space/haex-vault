# Vertrag: iroh-Protokoll `holzi-files/1`

ALPN `holzi-files/1` am Endpoint der Vault-Sitzung (research R5). Eine Verbindung je Gegenstelle;
der erste Bi-Stream ist der Handshake, jede weitere Anfrage ein eigener Bi-Stream (research R6).

## Rahmen

- Kopf: `u32` BE Länge, dann postcard. Höchstens 64 KiB; größer → Stream schließen mit Code
  `FRAME_TOO_LARGE`.
- Rohstrom nach dem Kopf nur, wo unten angegeben; Länge steht im Kopf.
- Jede Nachricht trägt `v: 1`. Eine unbekannte Version → `Reject`.

## Handshake (erster Stream)

```text
Anfragender  → Hello     { v, vault: [u8;32], device_pubkey: [u8;32],
                           list_payload: Vec<u8>, list_sig: [u8;64], nonce_r: [u8;32] }
Bedienender  → Challenge { v, nonce_s: [u8;32], trash: bool }
Anfragender  → Proof     { v, sig: [u8;64] }   // Domain::FilesAuth über
                                               // lp(nonce_r)‖lp(nonce_s)‖lp(own endpoint)‖lp(other endpoint)‖lp(vault)
Bedienender  → Accept    { v, mode: OwnDevice | Contact } | Reject { v }
```

Prüfung beim bedienenden Gerät (research R4), jede Abweichung → `Reject` ohne Grund:

1. `check_pushed(list_payload, list_sig, vault)` gelingt; Liste nennt `vault`.
2. Fremde Vault: `generation ≥` Eintrag in `haex_file_peer_lists_no_sync`; höhere wird gespeichert.
   Eigene Vault: statt der vorgelegten gilt die eigene gültige Liste.
3. `device_pubkey` steht auf der Liste, ist nicht entfernt, `endpoint_id == connection.remote_id()`.
4. `sig` gilt für `device_pubkey`.
5. Einordnung: eigene Vault und Schalter an → `OwnDevice`; Karte zur Vault gespeichert, nicht
   blockiert, mindestens ein Zugriffsrecht auf eine Freigabe dieses Geräts → `Contact`; sonst
   ablehnen.

## Anfragen

Alle Pfade sind Fensterpfade (`/`, `/a/b`). `share` ist bei `Contact` Pflicht und nennt die Wurzel,
bei `OwnDevice` fehlt es. Vor jeder Anfrage prüft das bedienende Gerät erneut Modus, Zugriffsrecht,
Stufe und Pfad (FR-005, FR-006, FR-007).

| Anfrage        | Kopf                                                        | Antwort                                                                              | Stufe     |
| -------------- | ----------------------------------------------------------- | ------------------------------------------------------------------------------------ | --------- |
| `Shares`       | —                                                           | `Vec<{ id, name, item_kind, level }>`; OwnDevice: `roots` (Laufwerke, bekannte Orte) | Lesen     |
| `ListDir`      | `{ share?, path, max }`                                     | `{ entries: Vec<WireEntry> }`                                                        | Lesen     |
| `Stat`         | `{ share?, path }`                                          | `WireEntry`                                                                          | Lesen     |
| `Read`         | `{ share?, path, start, len }`                              | `{ size, content_type }` + Rohstrom `len` Bytes                                      | Lesen     |
| `BeginUpload`  | `{ share?, path, expected: Absent \| Version{size, etag} }` | `{ upload_id }`                                                                      | Schreiben |
| `UploadPart`   | `{ upload_id, number, len }` + Rohstrom                     | `{ mark }`                                                                           | Schreiben |
| `FinishUpload` | `{ upload_id, marks }`                                      | `()` oder `Conflict`                                                                 | Schreiben |
| `AbortUpload`  | `{ upload_id }`                                             | `()`                                                                                 | Schreiben |
| `CreateFolder` | `{ share?, parent, name }`                                  | `WireEntry`                                                                          | Schreiben |
| `Rename`       | `{ share?, path, new_name }`                                | `WireEntry`                                                                          | Schreiben |
| `Move`         | `{ share?, from, to }`                                      | `()`                                                                                 | Schreiben |
| `Delete`       | `{ share?, path }`                                          | `{ trashed: bool }`                                                                  | Schreiben |

`WireEntry { name, path, kind: File|Folder, size, modified_ms, etag }`.

Regeln:

- `Rename`, `Move`, `Delete` auf die Wurzel einer Freigabe → `Refused` (FR-037). Bei einer Freigabe
  einer Datei sind nur `Read`, `Stat` und Hochladen auf `/<name>` mit `expected = Version` erlaubt.
- `Move` nur innerhalb derselben Wurzel; dazwischen kopiert der Anfragende und löscht (Spec Edge
  Cases).
- `Read` streamt ohne Zeitlimit; ein Entzug, Sperren oder Abbruch beendet den Stream mit
  `STREAM_CANCELLED`.
- `Delete` auf dem Gerät geht in den Papierkorb, wo `trash = true` (research R8).
- Upload-Sitzungen gehören der Verbindung; schließt sie, räumt das bedienende Gerät ihre
  Zwischendateien auf.

## Fehler

`WireError = NotFound | Refused | Conflict | TooLarge | SourceUnavailable | NameTaken | InvalidName |
Busy | Internal`. Keine Pfade außerhalb der Wurzel, keine Zugangsdaten, keine Fehlertexte des
bedienenden Geräts (FR-042). Beim Anfragenden bildet `PeerStore` sie auf `StorageError` ab:
`NotFound → NotFound`, `Refused → AccessDenied`, `Conflict → Conflict` (neu), `TooLarge → TooLarge`,
`SourceUnavailable → Network`, `Busy → Network`, Verbindungsfehler → `Network`.

## Grenzen

- Höchstens 16 gleichzeitige Streams je Verbindung, 64 je bedienendem Gerät; darüber `Busy`.
- `ListDir.max ≤ 100 000` (`MAX_ENTRIES`).
- Dateisystemarbeit läuft auf dem Blocking-Pool der Sitzung, nie auf dem Executor.
