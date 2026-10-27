# External drives in the file browser

**Status**: Design agreed with the operator on 2026-10-27; specified as
[spec 064](../../specs/064-external-drives/spec.md). Extends the drives of
[spec 044](../../specs/044-file-browser/spec.md) FR-002 and the sidebar of the [file browser design](2026-10-07-file-browser-design.md).

## Context

The sidebar already lists drives (`src-tauri/src/files/local/drives.rs`): on Linux the mounts under
`/`, `/media`, `/run/media` and `/mnt` from `/proc/self/mountinfo`, on macOS `/Volumes`, on Windows
the drive letters, on Android `/storage/XXXX-XXXX`. Three gaps:

1. Only drives the system already mounted appear. A plugged-in USB stick that nobody mounted is
   invisible.
2. `FilesApp.vue` loads `files_sources` once. A stick plugged in later appears only after reopening
   the window.
3. The name is the mount folder, every drive gets the same icon, and there is no eject.

Goal: behave like Nautilus or Dolphin.

## Decisions

| #   | Question                     | Decision                                                                                                                                                                                                                      |
| --- | ---------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1   | Scope                        | Show unmounted drives, mount on click, update live, show label and kind, eject.                                                                                                                                               |
| 2   | Platforms                    | Linux, macOS and Windows fully. Android only updates live (apps may not eject there).                                                                                                                                         |
| 3   | Other own devices and agents | See mounted drives as paths, as today. Mount, unlock and eject only by the user in the window: no peer request, no agent action.                                                                                              |
| 4   | Encrypted drives             | Unlocked in haex (LUKS, encrypted APFS, BitLocker To Go). The password can come from the password manager and be saved there if the user wants.                                                                               |
| 5   | Link to the password manager | No link table. A normal password entry whose URL is `haex:drive:<volume-uuid>` (UUID lowercase), tag `haex-drive`. It syncs with the password manager, so the same stick unlocks on every own device.                         |
| 6   | Change detection             | One `DriveBackend` per platform, polled about every 2 s by a `DriveMonitor` while a files window is open; a pure diff emits `files://drives`. Native events (udisks signals) can replace polling later behind the same trait. |

Rejected for change detection: native events on every platform (three event loops, much `unsafe`
FFI on macOS and Windows) and polling `files_sources` from the frontend (also re-asks peers and
contacts every time).

## Architecture

**Rust, `files/local/drives/`**

- **`DriveBackend`** trait: `list()`, `mount(id)`, `unlock(id, secret)`, `eject(id)`.
  - Linux: udisks2 over D-Bus with `zbus` (already in the lock file through other crates). No root;
    polkit asks through the desktop's agent where needed. `Filesystem.Mount`, `Encrypted.Unlock`,
    `Filesystem.Unmount` then `Drive.PowerOff`.
  - macOS: the `diskutil` CLI with `-plist` output (`list`, `info`, `mount`, `eject`,
    `apfs unlockVolume` with the passphrase on stdin, never as an argument).
  - Windows: the `windows` crate for drive letters and volume information; eject through the
    volume IOCTLs. BitLocker To Go: see open questions.
  - Fallback without udisks2 (minimal distro, sandbox): today's mountinfo listing, no mount or eject.
- **`DriveMonitor`**: starts with the first files window and stops with the last. Calls `list()`
  about every 2 s, diffs against the previous list (pure function) and emits `files://drives` on a
  change.
- **Peer roots** (`files/peer/serve/ops.rs`) take only drives with `state = mounted`; the wire format
  does not change.

**`Drive`** (ts-rs binding changes with it):

| Field        | Meaning                                                                       |
| ------------ | ----------------------------------------------------------------------------- |
| `id`         | Stable backend id: udisks object path, BSD name (`disk4s1`), volume GUID path |
| `name`       | Volume label, else "Datenträger (32 GB)"                                      |
| `path`       | Mount path, `null` while not mounted                                          |
| `kind`       | `system` \| `internal` \| `removable`; picks the icon (hard drive or USB)     |
| `state`      | `mounted` \| `unmounted` \| `locked`                                          |
| `canEject`   | Show the eject button                                                         |
| `volumeUuid` | For the password manager link; only on encrypted drives                       |
| `total`      | As today                                                                      |
| `free`       | As today                                                                      |

**Which unmounted drives appear**: all removable media, and internal partitions the system does not
mark as system partitions (udisks `HintSystem`/`HintIgnore`), so EFI, recovery and swap stay hidden.
Each partition of a stick is its own entry, as in Nautilus. Unformatted partitions do not appear.

**Frontend**

- The "Laufwerke" section of `files/Sidebar.vue`, ordered system → internal → removable, updated by
  `files://drives`.
- Click on an unmounted drive: spinner, mount, then open it.
- Click on a locked drive: unlock (below), mount, open.
- Eject button beside mounted removable drives.

## Unlocking

On a click on a locked drive haex looks for entries whose URL is exactly `haex:drive:<uuid>`:

- **One match**: unlock right away, no dialog. On a wrong password the dialog opens with "the saved
  password no longer fits".
- **Several matches**: the dialog lists them to pick from.
- **No match**: the dialog.

**Dialog "Laufwerk entsperren"**: a password field, or "Aus Passwortmanager wählen" (the usual entry
search). A checkbox **"Im Passwortmanager merken"**, off by default:

- Typed password: a new normal entry, title "Laufwerk: <label>", URL `haex:drive:<uuid>`, tag
  `haex-drive`.
- Picked entry without a URL: haex sets the URL.
- Picked entry with another URL: haex asks before replacing it; otherwise it is used once.

Nothing is saved before the unlock succeeded, so wrong passwords never land in the password manager.
Without an open vault only the password field works; the password manager part is greyed out.

**Secret handling**: the password goes only to the system service (udisks, `diskutil` over stdin).
It is never logged and never kept in frontend state. The entries are normal entries with normal
grants, as decided in [spec 052](../../specs/052-storage-credentials-picker/spec.md).

## Errors and edge cases

- **Polkit prompt cancelled or denied**: notice "Einhängen abgebrochen", no error dialog.
- **Wrong password**: the dialog stays open with the error; nothing is saved.
- **LVM inside an unlocked LUKS container**: "enthält LVM, nicht unterstützt".
- **Eject while haex holds files on the drive**: haex first releases what it holds itself (folder
  watcher, media server tokens, running search). A running transfer from or to the drive asks
  "Übertragung abbrechen und auswerfen?". If another program holds the drive: "wird noch von einem
  anderen Programm benutzt".
- **Stick pulled without eject**: the monitor drops it; tabs on it jump to the nearest existing
  folder and transfers abort with the reason (spec 044).

## Testing

- **Rust** (`*_tests.rs`, fake `DriveBackend`): the diff (added, removed, state changed);
  classification from udisks properties as a pure function (system/internal/removable, visible or
  not); name fallback; building and parsing `haex:drive:<uuid>`; entry lookup with 0, 1 and n
  matches; saved only after a successful unlock; eject releases own watchers and tokens; peer roots
  hold only mounted drives.
- **Frontend** (`scripts/check-files-*.ts`): the sidebar applies `files://drives`; click flows
  mount → open and locked → dialog or direct unlock.
- **E2E** (Linux CI): loop devices through `udisksctl loop-setup` (one vfat image, one LUKS image):
  mount, unlock, eject.
- **Manual**: a real USB stick and a LUKS stick on Linux, encrypted APFS on macOS, BitLocker To Go on
  Windows, an OTG stick on Android (live update only).

## Open questions for research

- **BitLocker To Go without admin rights**: whether a password can unlock it programmatically. If
  not, haex opens the system dialog (`bdeunlock.exe`) and Windows gets no password manager link.
- **udisks2 on the CI runner**: whether `udisksctl loop-setup` works on the GitHub Ubuntu runner. If
  not, the E2E scene stays manual.
