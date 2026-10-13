# Contract: Bridge-Methoden `extension_remote_storage_*` für WebDAV und SFTP

Ändert [038 contracts/bridge.md](../../038-storage-connections/contracts/bridge.md). Methoden, Rechte,
Fehlercodes, Dialoge und Grenzen bleiben; hier steht nur, was sich ändert. Grundsatz (FR-017): Eine
Erweiterung bekommt auf jedem Protokoll dieselben Ergebnisse und Fehlerarten.

## Liste der Speicher

`extension_remote_storage_list_backends` → je gedecktem Speicher:

| `type`   | Felder                                             |
| -------- | -------------------------------------------------- |
| `s3`     | `{id, type, name, providerName, bucket}` (wie 038) |
| `webdav` | `{id, type, name, providerName, folder}`           |
| `sftp`   | `{id, type, name, providerName, folder}`           |

`folder` ist der Ordnerpfad des Speichers (leer = Wurzel). Nie Adresse, Host, Port, Hostschlüssel oder
Zugangsdaten (FR-017a).

## Hinzufügen

`extension_remote_storage_add_backend` mit `request`:

| `type`   | `config` ohne `sameProviderAs`    | `config` mit `sameProviderAs` |
| -------- | --------------------------------- | ----------------------------- |
| `s3`     | wie 038                           | `{bucket}` (wie 038)          |
| `webdav` | `{url, folder}`                   | `{folder}`                    |
| `sftp`   | `{host, port?, folder}` (Port 22) | `{folder}`                    |

- `sameProviderAs` muss auf einen Speicher desselben Protokolls zeigen (sonst 3001).
- `url` folgt den Regeln von research R2/R6 (`https`, `http` nur lokal); `host` ist ein Name oder eine
  IP-Adresse ohne Schema, `port` 1–65535.
- **Berechtigung `add`** (FR-017b): Ziel ist bei WebDAV `host:port` der Adresse (Port aus dem Schema, wenn
  keiner angegeben ist), bei SFTP `host:port`. Rückfrage, Ablehnung und lokale Adressen wie 038.
- **Abgelehnt vor jedem Dialog** (3001 `credentials are entered in holzi`): `config` oder `request`
  enthält eines von `accessKeyId`, `secretAccessKey`, `sessionToken`, `username`, `password`,
  `privateKey`, `passphrase`, `hostKey`, `fingerprint`.
- **Dialog**: wie 038. Für eine neue SFTP-Verbindung zeigt das Fenster über der ganzen App nach den
  Zugangsdaten den Fingerabdruck des Hostschlüssels, den der Server beim Test zeigt; erst nach der
  Bestätigung des Nutzers testet holzi weiter. Ablehnen = Abbruch (1002).
- **Ergebnis**: Eintrag wie in der Liste; die Erweiterung erhält Lesen und Schreiben für den neuen
  Speicher (038 FR-013).

## Ändern

`extension_remote_storage_update_backend`: `config` kennt bei `webdav` und `sftp` nur `folder` (statt
`bucket`); ein neuer Ordner wird vor dem Speichern getestet. Fehlt er, bietet der Dialog wie die
Einstellungen an, ihn anzulegen (FR-006), nur nach Bestätigung. Umgesetzt (PR E): Der Dialog über dem Tab hat dafür ein Kästchen
„Ordner beim Anbieter anlegen, falls er fehlt“; holzis Fenster über der App bietet nach dem Ergebnis
`folderMissing` „Ordner anlegen und speichern“. Dasselbe gilt beim Hinzufügen.

## Objekte

`upload`, `download`, `list`, `delete`: unverändert in Form und Ergebnis.

- `list` mit `prefix` liefert dieselben Schlüssel wie bei S3, auch für ein Präfix, das mitten in einem
  Namen endet; Ordner und Zwischendateien erscheinen nicht (research R9).
- Zusätzliche Schlüsselprüfung bei WebDAV und SFTP (3001): Teil länger als 255 Byte, Teil `.`, `\`,
  `:*?"<>|`, Teil beginnt mit `.holzi-part-`, und ein Name, der sich im Elternordner nur in der
  Schreibweise von einem vorhandenen unterscheidet.
- `delete` entfernt danach leere Elternordner bis zur Wurzel des Bereichs der Erweiterung.
- Wie bei S3 (bei PR E ergänzt): Ein Präfix in einem Ordner, den es nicht gibt, listet leer; ein
  Schlüssel, unter dem ein Ordner liegt, ist beim Herunterladen „nicht gefunden“ (1001), auch wenn der
  Server die Adresse an den Ordner weiterleitet.

## Fehler

| Ursache                                  | Antwort an die Erweiterung  |
| ---------------------------------------- | --------------------------- |
| Kontingent beim Anbieter erschöpft (507) | 7000 (Grenze überschritten) |
| Hostschlüssel unbekannt oder geändert    | 2002 `{kind: "network"}`    |
| Server antwortet nicht als WebDAV        | 2002 `{kind: "network"}`    |
| sonst                                    | wie 038 (1001, 2002, 7000)  |

Keine Antwort und keine Fehlermeldung enthält Adresse, Host, Port, Hostschlüssel, Fingerabdruck,
Benutzername oder Antworttext des Servers.
