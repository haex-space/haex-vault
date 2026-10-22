# Other platforms for the end-to-end tests

The scenarios run on Linux (`tauri-driver` with `WebKitWebDriver`, a virtual screen per device and one
process per device, [`lib/platform/linux.ts`](lib/platform/linux.ts)) and, since spec 043, on Android
([`lib/platform/android.ts`](lib/platform/android.ts), section below). Windows, macOS and iOS are not
implemented yet: the table further down states what each would need so that the interface fits it, to
be confirmed by the follow-up spec of that platform
([`specs/033-multi-device-e2e/research.md`](../../specs/033-multi-device-e2e/research.md), R9). A platform
is added by a file beside `lib/platform/linux.ts`; no scenario changes.

## Android (spec 043)

Run it with an emulator or a phone attached (one device, or name it with `ANDROID_SERIAL`). The
devShell brings the emulator of the CI (`holzi-emulator`: API 35, `google_apis`, x86_64), the
chromedriver of its web view and `iroh-relay`, so in `nix develop`:

```sh
holzi-emulator -no-window -no-snapshot-save &   # adb sees it as emulator-5554
pnpm test:e2e --platform android --apk app-x86_64-debug.apk --grep files-basic
```

`--grep` takes part of a scenario name. When only tests change, the debug APK of a CI run (artifact
`holzi-android-debug`) saves the Android build. Outside the devShell, name chromedriver by
`E2E_CHROMEDRIVER` (Chrome for Testing, the major version of the device's web view) and `iroh-relay`
by `E2E_IROH_RELAY` (`cargo install iroh-relay --version <its version in src-tauri/Cargo.lock>
--features server --locked`). The run also needs the Linux tools of a Linux run: it builds the Linux
app for the other devices of a group, and its preflight stops without them.

- **Driver**: chromedriver attached to the web view of the app that already runs
  (`goog:chromeOptions.androidUseRunningApp`; without it chromedriver would clear the app's data). Its
  major version must be the device web view's (`dumpsys package com.google.android.webview`); get it
  from Chrome for Testing. Only debug APKs open their web view to a debugger.
- **Lifecycle**: `am start` and the web view's DevTools socket for a start, `am force-stop` for stop and
  kill, `pidof` for `alive`, `pm clear` for fresh data. Closing the vault ends the app (FR-006), so the
  run's close behaviour is always `exit`.
- **Display**: a desktop-sized display (`wm size 1280x800`, `wm density 160`, 1280×800 dp like the
  virtual screen), so desktop scenarios see the desktop layout; phone-width checks are scenarios of
  their own (`android-*`). Density 160 keeps the pixel count low: the emulator renders on the CPU, and
  a film in a maximized window at 2560×1600 froze it.
- **Services**: every port of the run's services (Nostr test relay, stand-in provider, RustFS, extension
  dev server) is made reachable on the device as `127.0.0.1:<port>` with `adb reverse`
  ([`lib/platform/reach.ts`](lib/platform/reach.ts)), so all devices use the same URLs.
- **URLs**: `tauri://localhost` is `http://tauri.localhost` and `holzi-ext://localhost` is
  `http://holzi-ext.localhost` on Android; the page maps what scenarios name.
- **Not on Android**: [`platform-exclusions.ts`](platform-exclusions.ts) lists the scenarios that do not
  run there, as `excluded` (with the requirement and a counter case) or `pending` (with the stage of
  spec 043 that makes them run); `pnpm check:e2e-exclusions` checks the list and prints the share.
- **Mixed groups** (stage 2 of spec 043): a device holds one app's data, so a group has one device on
  the phone, the one named `phone`, else its first device; the others run on Linux from the debug
  build a Linux run makes (`group.ts`, `runsOnPhone`). The phone sits behind the emulator's network,
  so the devices of a group meet through a local iroh relay (`iroh-relay --dev`,
  [`lib/iroh-relay.ts`](lib/iroh-relay.ts)) instead of the closed port a Linux run uses; the Linux
  devices bind loopback only (`HOLZI_E2E_SYNC_LOOPBACK=1`, debug builds), since the emulator's network
  loses the direct path under load. A file a
  scenario hands to a device is staged on the phone only for the device that runs there
  (`onDevice`). A vault file copies from the phone to a Linux device; the other way is not needed
  yet.
- **CI**: three shards on an API 35 x86_64 emulator (`android-e2e` in `.github/workflows/ci.yml`,
  [`../ci/android-e2e.sh`](../ci/android-e2e.sh)), split with `--shard i/n`.

## What a platform provides

The whole of it is `DeviceHost` in [`lib/platform/host.ts`](lib/platform/host.ts):

- `newData(folder)`: a place for the data of one device, with `copyVaultFile` (copy the vault file of a
  stopped device into another device's place), `keep` (copy it out for the failure material) and
  `dispose`.
- `start({ data, folder })`: start the application over the data and return a running device: the page
  operations (`click`, `type`, `invoke`, `exec`, `waitForDisplayed`, `navigate`), `stop`, `kill`, `alive`
  and `screenshot`.

Everything else (`goOffline`, `goOnline`, `lock`, `restart`, groups, users, waiting) is built on these in
`lib/device.ts` and `lib/group.ts`. `pnpm check:e2e-lib` fails when a scenario or a scenario-facing helper
names a platform, so the scenarios stay as they are.

## The four platforms

| Platform | Likely driver                                                                                     | Runner                       | Several devices                                                              | Known limits                                                        |
| -------- | ------------------------------------------------------------------------------------------------- | ---------------------------- | ---------------------------------------------------------------------------- | ------------------------------------------------------------------- |
| Windows  | Tauri WebDriver bridge (`tauri-driver`) with Microsoft Edge WebDriver                             | Windows runner               | several processes with separate data folders, as on Linux                    | no virtual screen; the screenshot and process operations differ     |
| macOS    | no official driver for the system web view; third-party driver or a bridge inside the application | macOS runner                 | separate data folders                                                        | the biggest unknown; the bridge would be test-only code             |
| Android  | chromedriver attached to the running web view (implemented, section above)                        | emulator (CI: KVM runner)    | one Android device per group; mixed groups with Linux in stage 2 of spec 043 | start and stop are app lifecycle, not processes; data is per device |
| iOS      | Appium (XCUITest)                                                                                 | macOS runner with simulators | several simulators on one host                                               | simulators are limited in count; data is per simulator              |

On all four, `start`, `stop` and `kill` are app lifecycle calls, `goOffline` and `goOnline` stay built
from start, stop and the servers setting, and `copyVaultFile` is a file transfer into the sandbox of the
other device. The test relay has to be reachable from every device: on one host that is a local address,
on emulators or simulators on separate hosts it needs a network between them.
