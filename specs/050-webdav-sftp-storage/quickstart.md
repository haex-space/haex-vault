# Quickstart: WebDAV- und SFTP-Speicher prüfen

Prüfanleitung für Spec 050. Verträge: [contracts/remote-store.md](./contracts/remote-store.md),
[contracts/tauri-commands.md](./contracts/tauri-commands.md), [contracts/bridge.md](./contracts/bridge.md);
Datenmodell: [data-model.md](./data-model.md).

## Voraussetzungen

- Ein Worktree mit `nix develop`; Rust über `scripts/with-nix-host-bridge.sh` (siehe CLAUDE.md). Nur die
  betroffenen Testziele mit `-j 4` bauen.
- Die automatischen Tests (§1–§4) brauchen kein Netz: WebDAV gegen `wiremock` und einen Server im Prozess,
  SFTP gegen einen russh-Server im Prozess über einem temporären Ordner (research R14).
- Für §5 Docker (SFTPGo, siehe unten); für §6 eigene Konten bei Hetzner und einer Nextcloud.

## 1. Protokoll je Verbindung, Migration, Typen

```sh
scripts/with-nix-host-bridge.sh cargo test -j 4 --manifest-path src-tauri/Cargo.toml --lib -- \
  remote_storage::protocol remote_storage::store identity::migrations
```

Erwartet: `ProtocolStore` reicht je `protocol` an den richtigen Store weiter; vorhandene Verbindungen sind
nach der Migration `s3`; Prüfregeln je Protokoll (data-model.md); Trigger-Version erhöht;
`Credentials::Debug` zeigt keine Geheimnisse.

## 2. WebDAV-Client

```sh
scripts/with-nix-host-bridge.sh cargo test -j 4 --manifest-path src-tauri/Cargo.toml --lib -- remote_storage::webdav
```

Erwartet: `PROPFIND` nur mit `Depth: 1`, `href` dekodiert und geprüft (ein Eintrag außerhalb wird
verworfen), Ordner-Adressen mit `/`, 3xx → Fehler ohne Folgen, 401/403/404/507 richtig abgebildet,
409 → Elternordner unterhalb des Speicherordners angelegt, fehlender Speicherordner → `NotFound`,
Zwischendatei + `MOVE` bei `put` und Upload-Sitzung, Abbruch löscht die Zwischendatei, `Range` mit 206
und mit 200.

## 3. SFTP-Client und Hostschlüssel

```sh
scripts/with-nix-host-bridge.sh cargo test -j 4 --manifest-path src-tauri/Cargo.toml --lib -- remote_storage::sftp
```

Erwartet: Anmeldung mit Passwort und mit Schlüssel (Ed25519, RSA, mit Passphrase); unbekannter
Hostschlüssel → `HostKeyUnknown` mit Fingerabdruck, **ohne** dass der Server eine Anmeldung sah (der
Testserver zählt Anmeldeversuche: 0, SC-004); geänderter Schlüssel → `HostKeyChanged`, ebenfalls 0
Anmeldeversuche; Umbenennen mit und ohne `posix-rename@openssh.com`; Teil wiederholen schreibt an
dieselbe Stelle; SSH-Sitzungen werden wiederverwendet und nach 60 s geschlossen (mit `tokio::time::pause`).

## 4. Gleichstand der Erweiterungsfunktionen und Dateibrowser

```sh
scripts/with-nix-host-bridge.sh cargo test -j 4 --manifest-path src-tauri/Cargo.toml --lib -- \
  extensions::remote_storage files::storage_source files::transfer
```

Erwartet: Der Katalog aus Spec 038 (Bereich, Schlüssel, Grenzen, Zugangsdaten) liefert gegen S3, WebDAV
und SFTP dieselben Ergebnisse (SC-002), auch `list` mit einem Präfix mitten in einem Namen; Vorschläge mit
`username`, `password`, `privateKey`, `passphrase`, `hostKey`, `fingerprint` → 3001 ohne Dialog;
Ordner umbenennen und löschen mit einem Aufruf bei WebDAV und SFTP; Transfer innerhalb eines
SFTP-Speichers streamt über holzi; Abbrüche hinterlassen keine Datei unter dem Zielnamen (SC-003).

## 5. E2E gegen SFTPGo

Ein Container liefert SFTP und WebDAV (research R14), gestartet von den Szenarien wie RustFS
(`scripts/e2e/lib/sftpgo.ts`). Hostschlüssel und Benutzerdatei erzeugt der Lauf in einem temporären
Ordner (nichts davon liegt in Git); von Hand etwa so:

```sh
docker run --rm -p 127.0.0.1:2022:2022 -p 127.0.0.1:8090:8090 \
  -e SFTPGO_WEBDAVD__BINDINGS__0__PORT=8090 \
  -e SFTPGO_LOADDATA_FROM=/var/lib/sftpgo/users.json \
  -v "$SFTPGO_DIR:/var/lib/sftpgo" drakkan/sftpgo:v2.7.6
```

Szenarien (Namen beim Bau festgelegt): WebDAV-Speicher anlegen, testen, Ordner fehlt → anlegen
(US1); im Dateibrowser blättern, Video springen, Upload abbrechen, Ordner umbenennen (US2); Erweiterung
lädt hoch/listet/löscht (US3); SFTP mit Schlüssel, Fingerabdruck bestätigen, danach anderen
Hostschlüssel mounten → Warnung, annehmen (US4, US5); zwei Geräte (SC-007). Auf Android über
`adb reverse` wie RustFS (`scripts/e2e/PLATFORMS.md`).

## 6. Von Hand (SC-008)

1. **Hetzner Storage Box, WebDAV**: Voreinstellung wählen, Benutzer `uXXXXX` ergänzen, Passwort, Ordner
   `holzi-test` (fehlt → anlegen lassen). Erwartet: Test grün in < 10 s, Speicher im Dateibrowser, ein
   Video von 500 MB spielt und springt (SC-006).
2. **Hetzner Storage Box, SFTP (Port 23)**: Schlüssel in holzi erzeugen, öffentlichen Schlüssel in der
   Hetzner-Konsole hinterlegen, testen. Erwartet: Fingerabdruck stimmt mit
   docs.hetzner.com/storage/storage-box/general überein; nach Bestätigung grün.
3. **Nextcloud**: Voreinstellung, App-Passwort. Erwartet: grün; eine im Nextcloud-Client geänderte Datei
   erscheint in holzi mit neuer Größe.
4. Auf einem zweiten Gerät nach dem Sync: alle drei Speicher ohne Eingabe nutzbar (SC-007), auch auf
   Android.
5. Zugangsdaten in den Einstellungen ansehen: nur Verweis auf den Eintrag im Passwortmanager; dort ist
   der private Schlüssel verdeckt, bis man ihn aufdeckt.
