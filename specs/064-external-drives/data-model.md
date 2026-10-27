# Data Model: Externe Laufwerke im Dateibrowser

Keine neue Tabelle. Laufwerke werden vom System erfragt und nicht gespeichert. Gespeichert werden nur
Laufwerks-Einträge (normale Passworteinträge, Spec 034) und zwei Schlüssel im Tab-Ort (Spec 022).

## Drive (Rust `files::local::drives::Drive`, ts-rs `Drive`)

Ersetzt das bisherige `Drive { name, path, total, free }`. Ohne Rückwärtskompatibilität: Das Binding
ändert sich, alle Nutzer ziehen mit.

| Feld        | Typ              | Regel                                                                                                                |
| ----------- | ---------------- | -------------------------------------------------------------------------------------------------------------------- |
| `id`        | `string`         | Kennung für Befehle in dieser Sitzung (udisks-Objektpfad, BSD-Name, `E:\`, `/storage/…`). Nicht dauerhaft speichern. |
| `key`       | `string \| null` | Stabile Kennung (R3): UUID des verschlüsselten Containers, sonst des Dateisystems; kleingeschrieben; sonst `null`.   |
| `label`     | `string \| null` | Bezeichnung des Datenträgers; ohne sie zeigt die Oberfläche „Datenträger (<Größe>)“ (i18n). Ersetzt `name`.          |
| `path`      | `string \| null` | Einhängepfad; `null` genau dann, wenn `state` nicht `mounted` ist.                                                   |
| `kind`      | `DriveKind`      | `system` \| `internal` \| `removable` (R2, R5, R6).                                                                  |
| `state`     | `DriveState`     | `mounted` \| `unmounted` \| `locked`.                                                                                |
| `release`   | `DriveRelease`   | `none` \| `unmount` \| `eject`: welcher Knopf erscheint (FR-008).                                                    |
| `encrypted` | `boolean`        | Ob ein Container darunter liegt (auch entsperrt), für Passwort-Eintrag und Sperren beim Auswerfen (FR-011).          |
| `size`      | `number \| null` | Größe des Blocks in Byte, für den Ersatznamen.                                                                       |
| `total`     | `number \| null` | Wie bisher, nur bei `mounted`.                                                                                       |
| `free`      | `number \| null` | Wie bisher, nur bei `mounted`.                                                                                       |

**Invarianten** (Rust-Tests):

- `state == mounted` ⇔ `path != null`.
- `kind == system` ⇒ `release == none`.
- `state != mounted` ⇒ `release == none`, `total == free == null`.
- `state == locked` ⇒ `encrypted`.
- Windows: `release == unmount` kommt nicht vor; `state == unmounted` kommt nicht vor.
- Android: `release == none` und `state == mounted` immer.

**Reihenfolge** der Liste: `system`, `internal`, `removable`; innerhalb nach `label` (fehlend zuletzt)
(Groß-/Kleinschreibung egal), dann nach `id`.

## Zustandsübergänge

```text
                 Einstecken
                     │
         ┌───────────┴────────────┐
         ▼                        ▼
    unmounted ──mount──▶ mounted ◀──mount── (nach unlock) ◀──unlock── locked
         ▲                  │                                            ▲
         └──unmount─────────┤                                            │
                            └──eject (Wechseldatenträger)──▶ entfernt    │
                            └──unmount/eject bei encrypted ──▶ lock ─────┘
    Abziehen in jedem Zustand ──▶ entfernt (Monitor meldet es)
```

## DrivesChanged (Kanal-Nachricht)

`{ drives: Drive[] }`: immer die ganze, sortierte Liste nach einer Änderung (R8). Kein Delta; die
Liste ist klein, und so kann die Oberfläche nichts verpassen.

## DriveDiff (rein, nur Rust)

`diff(prev: &[Drive], next: &[Drive]) -> bool` reicht: Der Monitor meldet, wenn sich irgendein Feld
außer `free` geändert hat. `free` ändert sich bei jeder Schreiboperation; er wird mitgeschickt, löst
aber allein keine Meldung aus.

## Laufwerks-Eintrag (Passwortmanager)

Ein normaler Eintrag (kein Eigentümer), Spec 034:

| Feld       | Wert                                                              |
| ---------- | ----------------------------------------------------------------- |
| `title`    | `Laufwerk: <label oder Ersatzname>` (nur beim Anlegen durch haex) |
| `url`      | `haex:drive:<key>`; erkannt wird nur genau diese Form             |
| `password` | das Passwort des Containers                                       |
| `tags`     | enthält `haex-drive` (haex setzt es, prüft es aber nicht)         |

Treffer = alle nicht gelöschten Einträge mit `url == "haex:drive:" + key` (Vergleich exakt; `key`
ist schon klein). Ein Nutzer kann die URL selbst eintragen.

## Tab-Ort (Spec 020/022, `lib/files/registry.ts`)

Neue Schlüssel in `FilesPlace` und `KEYS`:

| Schlüssel | Bedeutung                                                       |
| --------- | --------------------------------------------------------------- |
| `drive`   | `key` des Laufwerks, auf dem `p` liegt; fehlt bei `key == null` |
| `droot`   | `path` dieses Laufwerks, als der Ort entstand                   |

Regel: Beide gibt es nur zusammen und nur bei Quelle `device`. `p` beginnt mit `droot`.
Umrechnen auf einen neuen Einhängepfad: `neu + p.slice(droot.length)`.
