# Research: Externe Laufwerke im Dateibrowser

Stand: `main` vom 2026-10-27 (`d51d5751`). Es gibt keine offenen `NEEDS CLARIFICATION`. Was nur
Sekundärquellen stützen, ist als **UNVERIFIED** markiert und hat eine Spike-Aufgabe in den Tasks.
Primärquellen stehen bei jeder Entscheidung.

## R1 – Linux: udisks2 über D-Bus

- **Decision**: udisks2 über eigene, kleine `zbus`-5-Proxys (`ObjectManager`, `Block`,
  `Filesystem`, `Encrypted`, `Drive`) mit der **blockierenden** API von zbus (`zbus::blocking`). Sie
  läuft auf dem Thread des Laufwerk-Monitors (R4) bzw. in `spawn_blocking` für Befehle. Neue direkte
  Abhängigkeit `zbus = "5"` nur für `cfg(all(target_os = "linux", not(target_os = "android")))`,
  ohne zusätzliche Features.
  - Liste: `GetManagedObjects` auf `/org/freedesktop/UDisks2`.
  - Einhängen: `Filesystem.Mount({})` liefert den Pfad.
  - Entsperren: `Encrypted.Unlock(passphrase, {})` liefert das Klartext-Gerät, danach dessen `Mount`.
  - Aushängen: `Filesystem.Unmount({})`. Bei verschlüsselten danach `Encrypted.Lock({})`. Bei
    Wechseldatenträgern zuletzt `Drive.PowerOff({})`, wenn `CanPowerOff`, sonst `Drive.Eject({})`,
    wenn `Ejectable`.
  - Fehler: `org.freedesktop.UDisks2.Error.NotAuthorizedDismissed` und `.Cancelled` → `cancelled`;
    `.DeviceBusy` → `busy`; `.Failed` beim Entsperren → `unlockFailed` mit der Meldung des Systems.
    Für ein falsches Passwort gibt es keinen eigenen Fehlernamen.
- **Rationale**:
  - zbus 5.19 liegt schon im Lockfile (über `notify-rust`, `tauri-plugin-notification`,
    `tauri-plugin-opener`), mit async-io statt tokio. Die blockierende API braucht kein neues Feature
    und ändert nichts an diesen Crates.
  - Der Crate `udisks2` 0.4 hängt an zbus 6 (zweites zbus) und an `gettext-rs`.
  - Das Passwort geht als D-Bus-Argument an udisksd, nicht in eine Prozess-Kommandozeile (FR-017).
- **Quellen**: [Block](https://storaged.org/doc/udisks2-api/latest/gdbus-org.freedesktop.UDisks2.Block.html),
  [Filesystem](https://storaged.org/doc/udisks2-api/latest/gdbus-org.freedesktop.UDisks2.Filesystem.html),
  [Encrypted](https://storaged.org/doc/udisks2-api/latest/gdbus-org.freedesktop.UDisks2.Encrypted.html),
  [Drive](https://storaged.org/doc/udisks2-api/latest/gdbus-org.freedesktop.UDisks2.Drive.html),
  [udiskserror.c](https://github.com/storaged-project/udisks/blob/master/udisks/udiskserror.c).
- **Alternatives**: Crate `udisks2` (siehe oben). `udisksctl` als Prozess (Passwort-Eingabe nur
  interaktiv, Ausgabe nicht zum Parsen gedacht). zbus mit Feature `tokio` (ändert das Verhalten der
  anderen zbus-Nutzer im Prozess).
- **Ohne udisks2** (Dienst fehlt, Sandbox): Der Verbindungsaufbau oder `GetManagedObjects` scheitert.
  Dann fällt die Liste auf das heutige mountinfo-Verfahren zurück, ohne Einhängen, Entsperren und
  Auswerfen (FR-024).

## R2 – Welche Laufwerke erscheinen, und welche Art haben sie?

- **Decision** (Linux), angelehnt an `should_include_volume` von gvfs:
  1. `Block.HintIgnore` → nicht zeigen.
  2. Ein Block mit `Encrypted`, dessen `CleartextDevice` `/` ist → als **gesperrt** zeigen.
  3. Ist er entsperrt, zeigt haex stattdessen den Klartext-Block, aber nur, wenn dieser ein
     `Filesystem` hat. Ein Klartext-Block mit LVM bleibt unsichtbar (etwa die eigene Systemplatte).
  4. Sonst nur Blöcke mit `Filesystem`. Swap und unformatierte Partitionen haben keins.
  5. Eingehängte Blöcke zeigt haex nur, wenn der Mount-Pfad die heutige Regel `is_user_drive` erfüllt
     (`/`, `/media`, `/run/media`, `/mnt`), damit `/boot/efi` oder `/recovery` verborgen bleiben.
     `/home` bleibt wie heute weg.
  6. Nicht eingehängte Blöcke zeigt haex, solange 1–4 sie nicht ausschließen.
- **Art**: `system` für das bei `/` eingehängte Laufwerk; `removable` bei `Drive.Removable`; sonst
  `internal`.
- **Wichtig**: `HintSystem` ist **kein** Filter. udisks setzt ihn bei jeder internen Platte
  ([udiskslinuxblock.c](https://github.com/storaged-project/udisks/blob/master/src/udiskslinuxblock.c));
  ein Filter darauf würde alle internen Partitionen verstecken (widerspricht Klärung 2). gvfs filtert
  ebenfalls nicht danach
  ([gvfsudisks2volumemonitor.c](https://gitlab.gnome.org/GNOME/gvfs/-/blob/master/monitor/udisks2/gvfsudisks2volumemonitor.c)).
  `HintSystem` entscheidet nur, welche polkit-Aktion gilt: Bei internen Platten (`-system`) fragt
  polkit nach dem Admin-Passwort, bei Wechseldatenträgern in einer aktiven Sitzung nicht
  ([Policy](https://github.com/storaged-project/udisks/blob/master/data/org.freedesktop.UDisks2.policy.in)).
  Das deckt FR-006 („fragt das System selbst“).
- **Rationale**: So sieht der Nutzer dasselbe wie in Nautilus. Die Regel ist eine reine Funktion über
  einem Abbild der udisks-Eigenschaften und damit ohne D-Bus testbar.

## R3 – Kennung eines Laufwerks über Neustarts und Geräte hinweg

- **Decision**: `Drive.id` ist die Kennung für Befehle in der laufenden Sitzung: udisks-Objektpfad,
  BSD-Name, Laufwerksbuchstabe. `Drive.key` ist die **stabile** Kennung:
  - Ist das Laufwerk verschlüsselt, die UUID des Containers (LUKS-Header, APFS-Volume). Das gilt
    gesperrt **und** entsperrt, damit Tab und Passwort-Eintrag dasselbe Laufwerk finden.
  - Sonst die UUID des Dateisystems (`IdUUID`, `VolumeUUID`).
  - Fehlt beides, `null`. Dann gibt es weder Wiederherstellung (FR-025) noch Passwort-Eintrag.
  - haex schreibt die UUID immer klein.
- **Rationale**: Objektpfade wie `/org/freedesktop/UDisks2/block_devices/sdb1` hängen am Anschluss und
  wechseln beim Umstecken. Die LUKS-UUID entsteht bei `luksFormat` und ändert sich nur über
  `luksUUID --uuid` ([cryptsetup](https://man7.org/linux/man-pages/man8/cryptsetup-luksFormat.8.html)).
  Bekannte Grenze: geklonte Datenträger teilen die UUID; FAT-Volume-IDs haben nur 32 Bit. Beides
  nimmt haex hin; mehrere passende Einträge zeigt der Dialog zur Auswahl (FR-013).

## R4 – Änderungen erkennen

- **Decision**: wie im Design (Entscheidung 6) ein `DriveMonitor`. Er fragt alle 2 s das Backend ab,
  vergleicht mit dem letzten Stand (reine Funktion `diff`) und meldet Änderungen. Er läuft nur,
  solange jemand zuhört (R8). Je Plattform gilt eine **billige Vorprüfung**:
  - Linux: `GetManagedObjects` direkt. udisksd beantwortet das aus dem Speicher, ohne Plattenzugriff
    [Likely].
  - macOS: Fingerabdruck aus den Namen in `/dev/disk*` und den Einträgen in `/Volumes`. Nur wenn er
    sich ändert, ruft haex `diskutil` auf (R5). So entsteht nicht alle 2 s ein Prozess.
  - Windows: `FindFirstVolumeW`/`FindNextVolumeW` mit `GetVolumePathNamesForVolumeNameW`. **Nicht**
    `GetLogicalDrives`: Das hängt ausgehängte Volumes wieder ein
    ([FSCTL_DISMOUNT_VOLUME](https://learn.microsoft.com/en-us/windows/win32/api/winioctl/ni-winioctl-fsctl_dismount_volume)).
  - Android: `read_dir("/storage")` wie heute.
- **Rationale**: Ein Mechanismus für alle Plattformen; SC-001 (3 s) wird mit 2 s eingehalten. Native
  Ereignisse (udisks-Signale, DiskArbitration, `CM_Register_Notification`) kann ein Backend später
  hinter demselben Trait nachrüsten.
- **Alternatives**: Native Ereignisse je Plattform (drei Ereignisschleifen, auf macOS und Windows
  `unsafe`-FFI); vom Betreiber im Design verworfen.

## R5 – macOS: `diskutil`

- **Decision**:
  - Liste: `diskutil list -plist`, `diskutil info -plist <disk>` je Volume, `diskutil apfs list -plist`
    für gesperrte APFS-Volumes (`Locked`, `APFSVolumeUUID`). Geparst mit dem Crate `plist` 1.x (schon
    im Lockfile, neu als direkte Abhängigkeit nur für macOS).
  - Befehle:
    - `diskutil mount <id>`, `diskutil unmount <id>`, `diskutil eject <id>`
    - zum Entsperren `diskutil apfs unlockVolume <id> -stdinpassphrase`, das Passwort über stdin
      ([diskutil(8)](https://keith.github.io/xcode-man-pages/diskutil.8.html), selbst geprüft)
    - `-passphrase` steht in der Prozessliste und ist deshalb verboten (FR-017).
  - Art: `Internal = false` oder `RemovableMediaOrExternalDevice = true` → `removable`; bei `/` →
    `system`; sonst `internal`.
  - **Sichtbarkeit** (Analyse C1, SC-006). macOS legt auf jeder internen Platte Volumes an, die
    niemand öffnen soll:
    - Volumes mit einer APFS-Systemrolle (`Preboot`, `Recovery`, `VM`, `Update`, `Hardware`, `xART`)
      und Partitionen vom Typ `EFI` erscheinen nie.
    - Das System-Volume (Rolle `System`) erscheint nur als `/`.
    - Das Daten-Volume (Rolle `Data`) erscheint nicht eigens; es steckt hinter `/` über das Firmlink.
    - Eingehängte Volumes erscheinen bei `/` oder unter `/Volumes`.
    - Nicht eingehängte Volumes mit einem nutzbaren Dateisystem erscheinen ebenfalls, wenn sie
      gewöhnliche interne Volumes sind; Systemrollen, EFI-Partitionen und unformatierte Volumes
      bleiben ausgeschlossen. Das APFS-Volume mit Rolle `Data` ist eine bewusste Ausnahme und wird
      nicht separat gelistet, weil es hinter `/` über das Firmlink steckt.
    - Die Rollen-Schlüssel (`Roles` in `apfs list -plist`) sind UNVERIFIED und werden mit den Fixtures
      aus dem Spike geprüft.
- **Risiko (UNVERIFIED)**: Apple dokumentiert die Plist-Schlüssel nicht; die Namen stammen aus
  Fremd-Parsern ([ghw](https://github.com/jaypipes/ghw/blob/main/pkg/block/block_darwin.go),
  [fleet](https://github.com/fleetdm/fleet/blob/main/orbit/pkg/table/diskutil/apfs/apfs_darwin.go)).
  Gegenmaßnahme: Fixture-Tests mit echten, auf einem Mac aufgenommenen Ausgaben (Spike-Aufgabe), und
  der Parser behandelt fehlende Schlüssel als „unbekannt“ statt als Fehler.
- **Alternatives**: DiskArbitration über `objc2-disk-arbitration` (dokumentierte Schlüssel und
  Ereignisse, aber FFI und als veraltet markierte freie Funktionen). Wird es, wenn die Plists sich als
  unbrauchbar erweisen.

## R6 – Windows

- **Decision**:
  - Liste: Volumes mit Laufwerksbuchstaben über `FindFirstVolumeW`/`GetVolumePathNamesForVolumeNameW`,
    Name und Dateisystem über `GetVolumeInformationW`.
  - Art: `IOCTL_STORAGE_QUERY_PROPERTY` → `STORAGE_DEVICE_DESCRIPTOR.BusType == BusTypeUsb` oder
    `RemovableMedia` → `removable`. Das Volume wird dafür mit `dwDesiredAccess = 0` geöffnet, was ohne
    Admin geht ([CreateFile](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-createfilew),
    [STORAGE_DEVICE_DESCRIPTOR](https://learn.microsoft.com/en-us/windows/win32/api/winioctl/ns-winioctl-storage_device_descriptor)).
    Das Laufwerk mit dem Windows-Verzeichnis ist `system`.
  - Auswerfen: `IOCTL_STORAGE_GET_DEVICE_NUMBER` → Datenträger-Instanz →
    `CM_Request_Device_EjectW` mit Veto-Puffer, damit Windows keinen eigenen Dialog zeigt und den Grund
    zurückgibt ([doc](https://learn.microsoft.com/en-us/windows/win32/api/cfgmgr32/nf-cfgmgr32-cm_request_device_ejectw)).
    Ob der Datenträger oder sein Elterngerät ausgeworfen wird: Spike (UNVERIFIED).
  - Aushängen interner Laufwerke: nicht angeboten. `FSCTL_LOCK_VOLUME`/`FSCTL_DISMOUNT_VOLUME`
    brauchen einen direkten Volume-Handle und damit Admin (Spec FR-008 angepasst).
  - Volumes ohne Buchstaben: nicht gezeigt. Ihre GUID ist nur auf diesem Rechner gültig, und Windows
    gibt Wechseldatenträgern selbst einen Buchstaben (Spec-Annahme angepasst).
  - **BitLocker To Go**: `Win32_EncryptableVolume.UnlockWithPassphrase` verlangt Admin-Rechte
    ([doc](https://learn.microsoft.com/en-us/windows/win32/secprov/win32-encryptablevolume));
    `manage-bde -unlock` nimmt kein Passwort als Argument
    ([doc](https://learn.microsoft.com/en-us/windows-server/administration/windows-commands/manage-bde-unlock)).
    Also gilt FR-019: haex öffnet den Dialog des Systems über `ShellExecuteExW` mit dem Verb
    `unlock-bde` auf `X:\` (UNVERIFIED, Spike), notfalls `bdeunlock.exe X:`. Ohne Passwortmanager.
  - Ob ein gesperrtes BitLocker-Laufwerk ohne Admin als gesperrt erkennbar ist (Fehlercode von
    `GetVolumeInformationW`): UNVERIFIED, Spike. Ist es das nicht, erscheint es als eingehängt, und
    der Fehler beim Öffnen sagt „gesperrt – im System entsperren“ mit demselben Knopf.
- **Abhängigkeit**: `windows-sys` (schon transitiv im Lockfile) als direkte Abhängigkeit für
  `cfg(windows)` mit den Features `Win32_Foundation`, `Win32_Storage_FileSystem`, `Win32_System_IO`
  (`DeviceIoControl`), `Win32_System_Ioctl`,
  `Win32_Devices_DeviceAndDriverInstallation`, `Win32_UI_Shell`. Version wie die neueste bereits
  vorhandene, um keine weitere Version zu ziehen.

## R7 – Entsperren und Passwortmanager

- **Decision**: Alles mit Passwörtern läuft in Rust, wie bei 052 (`remote_storage/credentials.rs`).
  - **Suchen**: `PasswordsService::list_headers(&Caller::User, &[])`, gefiltert auf
    `url == Some("haex:drive:<key>")`. Einen URL-Index gibt es nicht. Bei der kleinen Zahl an
    Einträgen reicht der Filter.
  - **Lesen**: `read_secret_item(&Caller::User, &[], id)`, nur in Rust; das Passwort geht direkt an
    das Backend.
  - **Merken**:
    - Neuer Eintrag mit `create_item(&Caller::User, &[], ItemInput { title: "Laufwerk: <Name>",
password, url: "haex:drive:<key>", tags: ["haex-drive"] })`. Ein normaler Eintrag ohne
      Eigentümer, wie FR-016 es verlangt.
    - URL an einem gewählten Eintrag setzen: `update_item` mit `Patch::Set` und dem `updated_at` aus
      dem Kopf.
    - Tag setzen: `set_tags`.
  - **Auswahl im Dialog**: neuer Befehl `files_drive_unlock_candidates`. Er liefert nur Titel,
    Benutzername und URL von Einträgen mit Passwort, wie `storage_credentials_candidates`. Die
    Oberfläche übernimmt das Muster von `CredentialsEntryPicker.vue`.
  - Ein getipptes Passwort geht einmal per `invoke` nach Rust und wird danach im Dialog geleert
    (FR-017).
- **Ohne Vault**: `AppState::database()` liefert `NoActiveInstance`. Dann meldet der Befehl, dass der
  Passwortmanager fehlt, und der Dialog graut den Teil aus (FR-018). Laut ADR 0003 beendet das
  Schließen des Vaults den Prozess; einen „gesperrt, aber laufend“-Zustand gibt es nicht.
  - Im Fenster ist der Fall deshalb vermutlich nicht erreichbar (Analyse U1). FR-018 ist eine
    Absicherung.
  - Geprüft wird sie in Rust (`noVault` bei fehlender Datenbank), nicht im E2E.
- **Entscheidung „ohne Dialog“** (FR-013): `files_drive_open` versucht bei genau einem passenden
  Eintrag selbst zu entsperren. Scheitert das, liefert er `needsPassword` mit dem Grund
  `savedFailed`. Das Passwort verlässt Rust dabei nie.

## R8 – Ereignisse an die Oberfläche

- **Decision**: wie `files_watch` über einen `tauri::ipc::Channel`.
  - `files_drives_watch(channel) → id` meldet bei jeder Änderung die ganze Liste (`DrivesChanged`).
  - `files_drives_unwatch(id)` meldet ab.
  - Der Monitor-Thread läuft, solange mindestens ein Kanal angemeldet ist.
  - Die Oberfläche meldet sich einmal an: Ein Singleton-Composable `useFilesDrives` hängt am
    vorhandenen Zähler `openFrames` in `FilesApp.vue` (erstes Fenster an, letztes ab).
  - Seitenleiste und Wiederherstellung lesen daraus.
  - Ein Reload der Seite beendet die Kanäle mit `unwatch_all`, wie die Ordner-Watches
    (`lib.rs` `on_page_load`).
- **Rationale**: Der Dateibrowser kennt keine globalen Events, nur Kanäle je Aufruf. Rust erfährt
  nicht, welche Fenster offen sind; der Zähler im Frontend gibt es schon.

## R9 – Vor dem Aushängen freigeben

- **Decision**: `files_drive_release(id, cancelTransfers)` gibt vor dem Aushängen alles unter dem
  Einhängepfad frei:
  - **Ordner-Watches**: `Watching::Local` bekommt `root`; neu `FilesState::unwatch_under(prefix)`.
  - **Media-Tokens**: `Grant` bekommt `root: Option<PathBuf>`; neu `MediaServer::release_under(prefix)`.
  - **Suchen**: `SearchManager` merkt sich `root`; neu `cancel_under(prefix)`.
  - **Erweiterungs-Watches**: über das vorhandene `end_unless` in `extensions/fs/watch.rs`.
  - **Transfers**: neu `TransferManager::touching(prefix)` über `Job.sources`/`target` und
    `RemoteJob` mit `Side::Device`. Läuft einer, liefert der Befehl ohne `cancelTransfers` die Antwort
    `transfersRunning`. Die Oberfläche fragt dann (FR-010) und ruft erneut mit `true` auf; dann
    bricht `cancel` die Transfers ab.
  - Peer-Watches (`files/peer/serve/watch.rs`) halten keinen Handle dauerhaft offen; ihr Recheck alle
    10 s merkt das Verschwinden.
  - Hält danach noch ein anderes Programm das Laufwerk, kommt `busy` (FR-012).
- **Rationale**: Bisher ist nichts davon nach Pfad adressierbar (Code-Research). Die Präfix-Varianten
  sind je ein `retain` und folgen dem Vorbild `end_unless`.

## R10 – Tab-Ort und Rücksprung

- **Decision**:
  - Zwei neue Query-Schlüssel im Ort des Dateibrowsers:
    - `drive`: der `key` des Laufwerks, auf dem `p` liegt
    - `droot`: dessen Einhängepfad, als der Ort entstand
  - Sie stehen in `KEYS` von `lib/files/registry.ts`.
  - `go()` und `onSidebarPick` in `FilesApp.vue` sowie `filesActionHandlers.ts` setzen sie aus der
    Laufwerksliste. Der passende Eintrag ist das Laufwerk mit dem längsten `path`-Präfix.
  - Beim Laden gilt:
    - Liefert `files_list` `notFound`, prüft der Tab zuerst, ob das an `p` gemeldete Laufwerk mit der
      gespeicherten Identität `drive` übereinstimmt. Wenn nicht, sucht er das gespeicherte Laufwerk
      und remapt den Pfad von `droot` auf dessen aktuellen Mount-Pfad, bevor er erneut lädt.
    - Ist das gespeicherte Laufwerk nicht vorhanden, nicht zugänglich, nicht eingehängt oder
      gesperrt, zeigt der Tab stattdessen `DriveNotice` mit „Einhängen“ oder „Entsperren“.
    - Nach Erfolg öffnet er `neuerPfad + (p ohne droot)` (FR-025).
    - Fehlt das Laufwerk, gilt der Rücksprung: Der Tab steigt mit `parentPath` auf, bis
      `files_stat` einen Ordner findet, und zeigt einen Hinweis. Ganz oben landet er im Startordner.
- **Befund**: Der Rücksprung aus 044 (Randfall „Der offene Ordner verschwindet“) ist im Code nicht
  umgesetzt. Ein verschwundener Ordner zeigt heute nur den Fehler `notFound` in `FolderView.vue`.
  Diese Spec setzt ihn um, für alle Gerätepfade, nicht nur für Laufwerke.
- **Rationale**: Die Sitzung (Spec 022, `lib/wm/session.ts`) speichert beliebige String-Schlüssel,
  aber `parseFilesPlace` verwirft unbekannte. Zwei Schlüssel genügen, um einen Pfad auf ein neues
  Einhängeziel umzurechnen.

## R11 – Agents, andere Geräte und `drives()`

- **Decision**:
  - Neu: `volumes()` liefert alle Laufwerke mit Zustand (für die Seitenleiste und den Monitor).
  - `drives()` behält seine Bedeutung: nur **eingehängte** Laufwerke mit Pfad, gefiltert aus
    `volumes()`.
  - Damit bleiben `files/agent/exec.rs` (`files.sources`), die Peer-Wurzeln in
    `files/peer/serve/ops.rs` (`roots`, `drive_entries`) und `WireRoot` unverändert (FR-021, FR-022).
  - Es gibt keine Agent-Action zum Einhängen, Entsperren oder Auswerfen.
- **Rationale**: Die kleinste Änderung, die verhindert, dass gesperrte Laufwerke in Agent-Ausgaben
  oder an andere Geräte gelangen.

## R12 – E2E auf dem Linux-CI

- **Decision**: ein neues Szenario `files-drives.test.ts`. Vor der Umsetzung kommt ein Spike im
  CI-Job:
  1. `sudo apt-get install -y udisks2 cryptsetup`.
  2. Eine polkit-Regel unter `/etc/polkit-1/rules.d/` erlaubt dem Nutzer `runner` die Aktionen
     `org.freedesktop.udisks2.*`.
  3. Zwei Loop-Images: vfat mit Label, und LUKS2 über `sudo cryptsetup luksFormat`.
  4. Dann `udisksctl loop-setup`.
  - Das Szenario überspringt sich selbst, wenn `udisksctl` fehlt (Muster: `docker info` in den
    Storage-Szenarien).
- **Rationale**: Der Runner ist eine VM mit systemd, D-Bus und `sudo` ohne Passwort
  ([Runner](https://docs.github.com/en/actions/reference/runners/github-hosted-runners)). Er hat aber
  keine aktive logind-Sitzung, deshalb gilt `allow_active` nicht. Ob udisks2 vorinstalliert ist und
  dm-crypt im Azure-Kernel geht: UNVERIFIED. Scheitert der Spike, bleibt US1–US4 auf Linux manuell
  (quickstart) und die Rust-Tests decken die Logik über ein Fake-Backend.

## R13 – Android

- **Decision**: Die Liste bleibt beim heutigen Durchsuchen von `/storage` (`is_volume_id`). Der
  Monitor macht sie live (FR-023). Kein Einhängen und Auswerfen: `MOUNT_UNMOUNT_FILESYSTEMS` ist „Not
  for use by third-party applications“
  ([doc](https://developer.android.com/reference/android/Manifest.permission#MOUNT_UNMOUNT_FILESYSTEMS)).
- **Alternatives**: `StorageVolume.getDirectory()` über JNI ist sauberer, aber für eine Liste, die
  schon funktioniert, unnötig.

## R14 – Zusammenspiel mit Spec 062 (Split-Ansicht)

- **Befund**: PR #600 (062 PR A) ist offen. PR B wird den Inhalt von `FilesApp.vue` nach
  `files/Panel.vue` verschieben.
- **Decision**: Die Änderungen dieser Spec an `FilesApp.vue` bleiben klein: Anmeldung von
  `useFilesDrives` am Zähler `openFrames`, Verdrahtung der Seitenleiste. Laufwerks-Hinweis und
  Rücksprung liegen in eigenen Composables und Komponenten (`useFilesDriveRestore`,
  `DriveNotice.vue`), die mit dem Ordner-Zustand ins Panel wandern. Wer zuerst merged, wird vom
  anderen rebased.
