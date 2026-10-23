# Vertrag: iroh-Protokoll `haex-vault-files/1`

ALPN `haex-vault-files/1` am Endpoint der Vault-Sitzung (research R5). Eine Verbindung je Gegenstelle;
der erste Bi-Stream ist der Handshake, jede weitere Anfrage ein eigener Bi-Stream (research R6).

## Rahmen

- Kopf: `u32` BE Länge, dann postcard. Höchstens 64 KiB; größer → Stream schließen mit Code
  `FRAME_TOO_LARGE`.
- Rohstrom nach dem Kopf nur, wo unten angegeben; Länge steht im Kopf.
- Jede Nachricht trägt `v: 1`. Eine unbekannte Version → `Reject`.

## Handshake (erster Stream)

`sync::foreign_auth` (Spec 057) mit `Purpose { alpn: b"haex-vault-files/1", domain: Domain::FilesAuth }`
(Domäne `haex-vault-files-auth/v1`): `Hello` und `Proof` in beide Richtungen, Prüfung der Geräteliste
und des Endpoints auf beiden Seiten, höchstens 5 s (research R4). Danach auf demselben Stream:

```text
Bedienender  → Welcome { v, mode: OwnDevice | AccessOff | Contact, trash: bool }
```

Einordnung beim bedienenden Gerät; jede andere → Verbindung schließen ohne Grund:

1. `Trust::OwnVault` und Schalter an → `OwnDevice`; Schalter aus → `AccessOff` (der Anfragende zeigt
   „Dateizugriff auf diesem Gerät ausgeschaltet“; keine Anfrage wird beantwortet).
2. `Trust::Contact` und mindestens ein Zugriffsrecht auf eine Freigabe, die dieses Gerät bedient →
   `Contact`.
3. `Trust::Unverified`, `Contact` ohne Zugriffsrecht → schließen.

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
| `Watch`        | `{ share?, path }` (eigener Stream, bleibt offen)           | je Stapel `FolderChanged { paths }`, bis der Stream schließt                         | Lesen     |

`WireEntry { name, path, kind: File|Folder, size, modified_ms, etag }`.

Regeln:

- `Rename`, `Move`, `Delete` auf die Wurzel einer Freigabe → `Refused` (FR-037). Bei einer Freigabe
  einer Datei sind nur `Read`, `Stat` und Hochladen auf `/<name>` mit `expected = Version` erlaubt.
- `Move` nur innerhalb derselben Wurzel; dazwischen kopiert der Anfragende und löscht (Spec Edge
  Cases).
- `Read` streamt ohne Zeitlimit; ein Entzug, Sperren oder Abbruch beendet den Stream mit
  `STREAM_CANCELLED`.
- `Delete` auf dem Gerät geht in den Papierkorb, wo `trash = true` (research R8).
- `Watch` nur für Ordner auf dem Gerät des Bedienenden; in Speichern antwortet es mit `Unsupported`. Ein Entzug
  oder das Sperren schließt den Stream (research R16).
- Aufrufer: Jede Anfrage trägt `caller: User | Agent`. Eigene Daten von holzi: bei `OwnDevice` für
  `User` nur lesend, für `Agent` gesperrt; bei `Contact` immer gesperrt (FR-007).
- Upload-Sitzungen gehören der Verbindung; schließt sie, räumt das bedienende Gerät ihre
  Zwischendateien auf.

## Fehler

`WireError = NotFound | Refused | Conflict | Unsupported | TooLarge | SourceUnavailable | NameTaken |
InvalidName | Busy | Internal`. Keine Pfade außerhalb der Wurzel, keine Zugangsdaten, keine Fehlertexte des
bedienenden Geräts (FR-042). Beim Anfragenden bildet `PeerStore` sie auf `StorageError` ab:
`NotFound → NotFound`, `Refused → AccessDenied`, `Conflict → Conflict` (neu), `Unsupported → Unsupported`,
`TooLarge → TooLarge`, `SourceUnavailable → Network`, `Busy → Network`, Verbindungsfehler → `Network`.

## Grenzen

- Höchstens 16 gleichzeitige Streams je Verbindung, 64 je bedienendem Gerät; darüber `Busy`.
- `ListDir.max ≤ 100 000` (`MAX_ENTRIES`).
- Dateisystemarbeit läuft auf dem Blocking-Pool der Sitzung, nie auf dem Executor.
