# Research: Passwörter aus haex-vault 2.x (Legacy) übernehmen

Quelle für alle Angaben zu haex-vault 2.x: `haex-space/haex-vault-legacy` @
`8dce379d94e18fcd42c3b73686a06f984ca3f574`. Keine spätere Migration (0001–0020) berührt
`haex_passwords_*`; maßgeblich ist `src-tauri/database/migrations/0000_jazzy_chat.sql`.

## R1 Öffnen der Vault-Datei

**Decision**: holzi kopiert die gewählte Datei und, falls vorhanden, `<datei>-wal` in ein
eigenes temporäres Verzeichnis (`tempfile::TempDir`, wird beim Verlassen des Gültigkeitsbereichs
gelöscht) und öffnet dort die Kopie mit einer eigenen `haex_crdt::rusqlite::Connection`
(kein `haex_crdt::Database::open`). Danach `PRAGMA key` mit dem Vault-Passwort und als Erstes
eine Probeabfrage auf `sqlite_master`.

**Rationale**:

- haex-vault 2.x öffnet mit `conn.pragma_update(None, "key", password)` und setzt sonst keine
  `cipher_*`-Pragmas (`src-tauri/src/database/core/init.rs:35`, `create.rs:81`). Das ist SQLCipher 4
  mit Standardwerten (haex-vault 2.x bündelt SQLCipher 4.14.0 über libsqlite3-sys 0.38.2). holzi
  bündelt dieselbe Version: haex-crdt @ `928d06a` zieht rusqlite 0.40.2 mit
  `bundled-sqlcipher-vendored-openssl` (`src-tauri/Cargo.lock:8361`, `:5289`). Gleiche Bibliothek,
  gleiche Standardwerte, kein Kompatibilitätsmodus nötig.
- haex-vault 2.x läuft im WAL-Modus (`init.rs:79-90`). Ein Leser einer WAL-Datenbank legt `-shm` an
  und kann beim Schließen checkpointen, also die Quelldatei ändern und `-wal` löschen.
  `?immutable=1` verhindert das, ignoriert aber die WAL und verliert damit deren Änderungen.
  Nur eine Kopie erfüllt beides aus FR-003: Quelle unverändert und WAL-Stand eingelesen. Die
  Kopien sind weiter mit dem Vault-Passwort verschlüsselt (auch die WAL-Seiten), es liegt also
  kein Klartext im temporären Verzeichnis.
- `Database::open` würde holzis Migrationen, Trigger und die Dateisperre auf die fremde Datei
  anwenden; ein roher Connection ist der kleinste korrekte Weg. `src-tauri/clippy.toml` verbietet
  nur `Database::with_connection`.
- Kopieren statt `std::fs::read` in den Speicher: Die Vault kann durch Base64-Anhänge groß sein;
  `std::fs::copy` hält sie nicht im Speicher.

**Alternatives considered**:

- Direkt mit `SQLITE_OPEN_READ_ONLY` öffnen: legt `-shm` neben der Quelle an oder scheitert auf
  schreibgeschütztem Ort (USB-Stick). Verstößt gegen FR-003.
- `sqlite3_deserialize` aus dem Speicher: mit SQLCipher-Verschlüsselung nicht verlässlich, und
  die WAL lässt sich so nicht einspielen.
- Einmaliges Skript mit `sqlcipher`-CLI und KDBX-Export: vom Operator verworfen (Spec, Input).

## R2 Fehlerbilder beim Öffnen (FR-005)

**Decision**: Gründe für `PasswordsImportFailed { reason }`, alle vor dem ersten Schreiben:

| Fall                                                                                                              | Erkennung                           | `reason`                         |
| ----------------------------------------------------------------------------------------------------------------- | ----------------------------------- | -------------------------------- |
| Datei nicht lesbar oder nicht kopierbar                                                                           | I/O-Fehler                          | `unreadable` (vorhanden)         |
| Leere Datei, kleiner als eine Seite (4096 Byte), oder Klartext-SQLite (`SQLite format 3\0` in den ersten 16 Byte) | Kopfbytes prüfen, vor dem Schlüssel | `unsupported_format` (vorhanden) |
| Falsches Passwort oder fremde Datei                                                                               | Probeabfrage meldet `SQLITE_NOTADB` | `legacy_haex_vault_locked` (neu) |
| Keine Tabelle `haex_passwords_item_details`                                                                       | `sqlite_master`                     | `no_passwords` (neu)             |
| Pflichttabelle oder Pflichtspalte fehlt                                                                           | `PRAGMA table_info` gegen R5        | `unsupported_format`             |

**Rationale**: Eine SQLCipher-Datei besteht ohne Schlüssel aus Salt und Zufallsdaten. Falsches
Passwort und fremde verschlüsselte Datei sehen gleich aus (Spec, Clarifications). holzi selbst
erkennt den falschen Schlüssel ebenfalls an `SQLITE_NOTADB` (`instances/open.rs:196-200`).
`wrong_credentials` passt nicht, weil sein Text die Schlüsseldatei von KeePass nennt.

## R3 Wo der Leser sitzt

**Decision**: Neues Modul `src-tauri/src/passwords/import/legacy_haex_vault/` mit `open.rs` (Kopie,
Schlüssel, Prüfung des Aufbaus, R1–R2, R5) und `read.rs` (Zeilen → `ImportModel`). `ImportSource`
bekommt die Variante `LegacyHaexVault` (serde: `"legacyhaexvault"`). `service/import.rs::read_model`
verzweigt für diese Quelle vor `std::fs::read` zu `legacy_haex_vault::read(path, &credentials)`; die
anderen Quellen bleiben bei `parse(bytes)`.

**Rationale**: Die anderen Parser sind rein und bekommen Bytes; dieser Leser braucht eine Datei
auf der Platte. Die Verzweigung sitzt an der Stelle, die schon Datei-I/O macht (im
`spawn_blocking` von `read_model`), so bleibt `parse()` rein und alles danach (Vorschau,
Doppelte, `apply::run`, Bericht, Abbruch) ist geteilt.

**Alternatives considered**: `parse()` um einen Pfad erweitern: verwässert die reine Schnittstelle
für drei Quellen wegen einer.

## R4 Was das gemeinsame Importmodell zusätzlich tragen muss

Das Modell (`import/mod.rs:112-209`) und `apply.rs` verlieren heute, was die bisherigen Quellen
nicht hatten. Die Tabellen von holzi haben alle Spalten (`identity/migrations_passwords.rs`).

| Lücke                          | Heute                                                      | Änderung                                                                                                                                                                                                                       |
| ------------------------------ | ---------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Farbe des Eintrags             | `color = NULL` (`apply.rs:442`)                            | `ImportItem.color: Option<String>`                                                                                                                                                                                             |
| Autofill-Aliase                | `autofill_aliases = NULL` (`apply.rs:449`)                 | `ImportItem.autofill_aliases: Option<serde_json::Value>`                                                                                                                                                                       |
| Farbe, Reihenfolge des Ordners | nicht geschrieben (`apply.rs:335-384`)                     | `ImportGroup.color`, `ImportGroup.sort_order: Option<i64>`                                                                                                                                                                     |
| Tag-Farben                     | Tags nur als Namen                                         | `ImportModel.tag_colors: Vec<(String, String)>`; gesetzt über vorhandenes `tags::set_color` (`tags.rs:128`), nur wenn das Tag noch keine Farbe hat                                                                             |
| Passkeys ohne Eintrag          | nicht möglich (`apply.rs:474` setzt immer `item_id`)       | `ImportModel.passkeys: Vec<PasskeyInput>` mit `item_id = None`                                                                                                                                                                 |
| Volle Passkey-Angaben          | `RawPasskey` ohne Symbol, Farbe, Spitzname, letzte Nutzung | nicht nötig: `ImportItem.passkeys` ist schon `Vec<PasskeyInput>` mit allen 18 Spalten; der neue Leser füllt ihn direkt, `passkeys::insert` schreibt den gelieferten öffentlichen Schlüssel unverändert (`passkeys.rs:123-164`) |
| Generator-Voreinstellungen     | kein Teil des Imports                                      | `ImportModel.presets: Vec<PresetInput>`; geschrieben über `presets::save` (`presets.rs:66`)                                                                                                                                    |

Alle neuen Felder sind `Option` bzw. leer per `Default`; die drei bisherigen Parser bleiben
unverändert.

**Ordner wiederverwenden (FR-017)**: `write_groups` vergibt heute für jeden Ordner eine neue
UUID, ein wiederholter Import verdoppelt den Baum für alle Quellen. Neu: Vor dem Anlegen sucht
`write_groups` einen Ordner mit gleichem Namen (exakt) unter demselben, schon aufgelösten
Elternordner und nimmt dessen Kennung; nur neu angelegte Ordner kommen in den Ledger, damit ein
Rückbau keine vorhandenen Ordner löscht. Die Ursache wird damit im geteilten Schreiber behoben,
nicht nur für die neue Quelle.

**Ledger**: bekommt `passkeys` (Passkeys ohne Eintrag; die mit Eintrag verschwinden per
`ON DELETE CASCADE`) und `presets`.

**Voreinstellungen und „Standard“**: `presets::save` nimmt einer anderen Voreinstellung das
Standard-Kennzeichen. Eine importierte Voreinstellung wird nur Standard, wenn holzi noch keine
Standard-Voreinstellung hat; vorhandene Namen werden übersprungen (FR-013). Zeitstempel der
Quelle gehen dabei verloren (`save` nimmt immer `now`); das ist für Einstellungen hinnehmbar und
steht nicht im Bericht.

## R5 Aufbau der Quelle und Abbildung

Die Tabellen von haex-vault 2.x entsprechen Spalte für Spalte denen von holzi (034 hat sie
übernommen), bis auf:

- `haex_passwords_binaries.data` ist Base64-Text (Standardalphabet mit Padding), holzi speichert
  BLOB. `hash` ist SHA-256 der dekodierten Bytes als Hex in Kleinbuchstaben
  (`src/utils/passwords/binaries.ts:18-39`); holzi rechnet den Hash beim Schreiben ohnehin neu.
- Zur Laufzeit hängt die CRDT-Schicht `haex_hlc`, `haex_column_hlcs`, `haex_column_sigs` an
  (`src-tauri/src/crdt/trigger.rs:18-27`). Sie sind bekannt und werden ignoriert (FR-015). Der
  Leser fragt Spalten immer mit Namen ab, nie `SELECT *`.

Pflichtspalten (fehlt eine → `unsupported_format`): die Spalten aus der Migration 0000 außer
Zeitstempeln. Spalten, die weder in 0000 noch unter den drei CRDT-Spalten stehen, und weitere
Tabellen mit Präfix `haex_passwords_` landen als `UnknownSourceData` im Bericht. Tabellen der
alten Erweiterung (`%__haex-pass__haex_passwords_%`) nennt die Vorschau als Warnung.

Einzelheiten der Abbildung: [contracts/haex-vault-mapping.md](contracts/haex-vault-mapping.md).

## R6 Papierkorb

`trash` ist in haex-vault 2.x eine echte Zeile in `haex_passwords_groups` (`stores/passwords/groups.ts:16,
82-97`). Gelöschte Ordner bekommen `parent_id = 'trash'`, ganze Teilbäume liegen also darunter.
**Decision**: Die Zeile `trash` wird `ImportGroup { is_recycle_bin: true }`; ein Eintrag ist
`trashed`, wenn seine Ordnerkette `trash` erreicht. `previous_parent_ref`/`trashed_from_ref`
bleiben leer, haex-vault 2.x merkt sich die Herkunft nicht; Wiederherstellen führt nach oben (Spec,
Edge Cases).

## R7 Verlauf

Drei Formen von `snapshot_data` existieren (`src/utils/passwords/snapshots.ts:9-22`, KeePass-Import
`import/keepass.vue:410-465`, externe Anfragen `useCoreExternalRequestHandlers/passwords.ts:322-344`).
**Decision**: In `SnapshotData` von holzi (`passwords/snapshots.rs:50-71`, camelCase, `#[serde(default)]`,
liest laut Moduldoku haex-vault-2.x-Stände) deserialisieren; unbekannte Felder wie `tags` werden
ignoriert. `attachments` kommen maßgeblich aus `haex_passwords_snapshot_binaries`, weil der
KeePass-Import von haex-vault 2.x sie nur dort ablegt; `write_state` füllt `data.attachments` ohnehin
aus den Dateien. Zeitpunkt: `modified_at`, sonst `created_at`. Ist die JSON nicht lesbar →
Stand fehlt, `HistoryUnreadable` im Bericht.

## R8 Symbole

holzi nutzt Iconify-Namen mit Doppelpunkt (`lucide:key-round`, `src/lib/passwords/icons.ts`) und
zeigt Unbekanntes als Standardsymbol (`resolveIcon`, `:134-147`). haex-vault 2.x speichert `i-lucide-*`,
`mdi:*` aus seinem Symbolwähler (69 Namen, `editor/iconPicker.vue:101-181`), Kurznamen der alten
Erweiterung und `binary:<hash>`.
**Decision**: Abbildungstabelle in `import/legacy_haex_vault/icons.rs`:
`i-lucide-x` → `lucide:x`; `mdi:*` und Kurznamen → passender `lucide:`-Name. Jedes Ziel muss in
`IMPORT_ICONS` stehen; ein Test prüft das wie `icons_tests.rs:18` für KeePass. `binary:<hash>` →
`IconRef::Custom(bytes)` aus der Binärtabelle. Unbekannt oder Bild fehlt → kein Symbol,
`IconNotMapped` im Bericht. Achtung: haex-vault 2.x löscht beim Öffnen Bild-Binärdaten, auf die nur
ein `icon`-Feld zeigt (`binaries.ts:65-82`); fehlende Bilder sind also zu erwarten.

## R9 Zeitstempel und Werte

- Zeitstempel kommen gemischt als `YYYY-MM-DD HH:MM:SS` (SQLite, UTC) und ISO mit `Z`.
  `holzi_time` (`import/mod.rs:237`) über `parse_iso_millis` akzeptiert beides.
- `expires_at` ist ein Datum `YYYY-MM-DD`; unverändert übernehmen, wie der KeePass-Parser.
- `otp_digits`/`otp_period`/`otp_algorithm` können trotz Standardwert NULL sein; `check_otp`
  und `otp_columns` setzen die Standardwerte, ungültige Werte bleiben wie sie sind (034 FR-023).
- Eigene Felder haben keine Reihenfolge-Spalte; haex-vault 2.x schreibt sie bei jedem Speichern neu
  in Anzeigereihenfolge. Lesen mit `ORDER BY rowid`.
- Tags in haex-vault 2.x sind case-sensitiv eindeutig; holzi faltet Namen (`tags.rs:39-55`). Fallen
  zwei zusammen → `TagMerged` im Bericht.

## R10 Vault-Passwort im Speicher

`Credentials.password` ist `Zeroizing<String>`. `rusqlite` baut aus `pragma_update` intern einen
SQL-Text, der nicht genullt wird. Das tut holzi beim Öffnen der eigenen Vault ebenso; der Leser
übernimmt das mit einem `ponytail:`-Kommentar (Grenze: Klartext-Kopie im freigegebenen Speicher
bis zur Wiederverwendung; Ausbau: `sqlite3_key_v2` über `ffi` mit Zeroizing-Puffer).

## R11 Plattformen

Der Leser nutzt nur `std`, `tempfile` und rusqlite aus haex-crdt; nichts ist plattformabhängig.
Eine Android-/iOS-Build-Umgebung existiert noch nicht (`src-tauri/gen/` fehlt). Inhalts-URIs
von Android behandelt der Import heute für keine Quelle; das bleibt außerhalb dieser Spec.

## R12 Tests

- Fixture zur Laufzeit wie `tests/common/kdbx_fixture.rs`: `tests/common/legacy_haex_vault_fixture.rs`
  legt mit `haex_crdt::rusqlite` eine SQLCipher-Datei an, führt das SQL aus
  `tests/fixtures/passwords/legacy_haex_vault_0000_passwords.sql` aus (Auszug aus 0000 mit Quellangabe
  der Revision), hängt die drei CRDT-Spalten an und schreibt Beispieldaten für jedes Feld. Für den
  WAL-Fall bleibt eine Verbindung mit `wal_autocheckpoint=0` offen, während importiert wird.
  Testpasswörter sind Testwerte und keine Geheimnisse (Constitution I).
- Frontend: reine Hilfsfunktionen des Wizards liegen in `src/lib/passwords/importReport.ts` und
  werden mit `pnpm check:passwords` (`scripts/check-passwords-import.ts`, `node --test`) geprüft.
  Neue Logik dort (welche Quelle Passwort bzw. Schlüsseldatei braucht, Texte der neuen
  Berichtsarten und Gründe) bekommt dort ihre Fälle. Ein e2e-Szenario für den Import gibt es
  bisher nicht; diese Spec fügt keins hinzu, die Rust-Tests decken den Ablauf über den
  öffentlichen Dienst ab.
