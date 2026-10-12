# Quickstart: Verschlüsselte Ordner prüfen (048)

Prüfschritte für Lieferungen B bis H. Automatisch, soweit möglich; was nur von Hand geht, ist
markiert. Befehle laufen im Worktree in der Nix-Shell (`nix develop`).

## Voraussetzungen

- Ein Speicher aus 038 gegen RustFS (E2E-Rig, `scripts/e2e/lib/rustfs.ts`) oder einen echten
  Anbieter.
- Für §4: zwei Geräte derselben Vault (Rig von 033).

## §1 Format (PR B)

```sh
cargo test --lib files::encrypted::format -j 4
```

Erwartet: alle Testvektoren aus `src-tauri/tests/fixtures/encrypted/v1-vectors.json` grün,
einschließlich der Negativfälle aus [contracts/format.md](./contracts/format.md). In der CI laufen
dieselben Vektoren in der Plattform-Probe unter Windows, macOS und Android (SC-010).

## §2 Anlegen, Hineinlegen, Öffnen (US1, PR C)

```sh
pnpm test:e2e files-encrypted
```

Erwartet:

1. „Neuer verschlüsselter Ordner“ → Hinweis beim ersten Mal → Ordner „Unterlagen“ mit Schloss.
2. Datei, PDF, Bild und ein Unterordner hineinkopiert; Liste zeigt Namen, Größen, Vorschaubild.
3. Text, PDF und Bild öffnen im Viewer.
4. Im Bucket direkt (RustFS-Client der Szene): kein Objektname enthält „Unterlagen“, einen Dateinamen,
   einen Ordnernamen oder eine Endung; unter dem Ordner liegen nur `h`, `m/…`, `c/…` (SC-001).
5. Unter `<AppCache>/files-thumbnails` ist kein neues Vorschaubild entstanden (FR-026).

## §3 Manipulation (US2, PR C)

Teil der Szene `files-encrypted` und von `files::encrypted::folder_tests`: ein Byte ändern, ein
Inhaltsobjekt an einer Blockgrenze kürzen, zwei Inhaltsobjekte vertauschen, eine Begleitdatei unter
einen anderen Namen kopieren. Erwartet: betroffene Einträge `damaged`, alle anderen lesbar, kein
veränderter Klartext im Viewer (SC-002).

## §4 Zweites Gerät (US3, PR F)

```sh
pnpm test:e2e files-encrypted-two-devices
```

Erwartet: Gerät 2 öffnet den Ordner ohne Eingabe (SC-007); beide schreiben gleichzeitig `notiz.txt`
→ beide Fassungen sichtbar, eine als „notiz (Konflikt …).txt“. Eine zweite Vault auf demselben Bucket
sieht „Verschlüsselter Ordner einer anderen Vault“.

## §5 Video (US4, PR D)

Automatisch in `files-encrypted`: das MP4-Fixture aus 044 spielt, ein Sprung in die Mitte lädt nur
die betroffenen Blöcke (Range-Anfragen der Szene).

**Von Hand** (SC-004): ein 4-GB-Video hochladen, bei guter Verbindung Start < 5 s, Sprung < 5 s,
Speicherzuwachs von holzi < 100 MB (Systemmonitor). Ergebnis im PR notieren.

## §6 Aufräumen im Ordner (US5, PR E)

Teil von `files-encrypted`: Unterordner mit 1 000 Dateien umbenennen → im Bucket kein `c/…` neu
(SC-006), Dauer < 30 s gegen RustFS; Verschieben, Kopieren, Löschen; Suche nach „strom“ findet die
Datei; Kopieren in einen gewöhnlichen Ordner des Speichers fragt vorher, aufs Gerät nicht.

## §7 Agents (US6, PR G)

`files::agent` Tests und Szene `files-encrypted-agent` mit dem geskripteten Modell des Rigs:

- ohne Freigabe: Liste zeigt „verschlüsselter Ordner 1“ ohne Namen; Suche ohne Treffer daraus.
- Zugriff → Frage nennt Modell und „auf diesem Gerät“/„in der Cloud“; „Lesen“ ohne Haken → nach dem
  Turn wieder Frage.
- gespeichert „nur lokal“, Wechsel auf Cloud-Modell → neue Frage.

## §8 Erweiterungen (US6, PR H)

Szene `extension-encrypted-folder` mit der Probe-Erweiterung:

- Manifest mit `encryptedFolder` → Installation abgelehnt.
- ohne `remoteStorage` → `choose` abgelehnt ohne Frage.
- mit `remoteStorage` → `choose` zeigt Auswahl und Frage; ohne Haken: nach Neuladen der Erweiterung
  wieder Frage; mit Haken: keine Frage mehr; Widerruf in den Einstellungen wirkt sofort.

## §9 Spuren auf dem Gerät (PR C, PR E)

Nach dem Sperren der Vault: `files-opened-encrypted` leer, kein Name aus dem Ordner in den Logs
(`grep` der Szene über das Log der Instanz) (SC-009).

## §10 Auf dem Gerät (US7, PR I bis K)

```sh
cargo test --lib files::encrypted -j 4
cargo test --lib remote_storage::local -j 4
pnpm test:e2e --grep files-encrypted-device
```

Erwartet:

1. In einem Testverzeichnis des Geräts entsteht nach „Neuer verschlüsselter Ordner“ ein Verzeichnis
   `<zufall>.hxef` mit nur `h`; der Dateibrowser zeigt den Namen mit Schloss.
2. Hineinkopieren, Öffnen, Video, Umbenennen, Verschieben, Suche funktionieren; mit den Mitteln des
   Systems gelesen enthält das Verzeichnis nur `h`, `m/…`, `c/…` ohne Klartext (SC-012).
3. Löschen fragt und löscht endgültig; im Papierkorb landet nichts.
4. Eine fremde Datei (`notiz (conflicted copy).txt`) im Verzeichnis erscheint nicht als Eintrag, der
   Hinweis nennt 1, nach dem Aufräumen ist sie noch da.
5. Die Probe-Erweiterung mit Dateizugriff auf das Testverzeichnis bekommt beim Lesen, Schreiben,
   Löschen und Beobachten unter `<zufall>.hxef` eine Ablehnung und sieht es in Listen nicht.
