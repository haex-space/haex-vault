# Contract: Android intents and native commands

Everything Android-only goes through the `holzi-android` plugin (`src-tauri/plugins/holzi-android`, ADR 0010). The core calls
it through `HolziAndroid` methods; `other.rs` holds a no-op for every other platform.

## Screen commands (FR-008)

| Plugin command    | Args              | Effect                                                                                         |
| ----------------- | ----------------- | ---------------------------------------------------------------------------------------------- |
| `setScreenBright` | `{ on: boolean }` | `on`: window brightness to full; `off`: back to the system value (`BRIGHTNESS_OVERRIDE_NONE`). |
| `setKeepScreenOn` | `{ on: boolean }` | Sets or clears `FLAG_KEEP_SCREEN_ON` on the window.                                            |

Core commands `screen_set_bright(on)` and `screen_set_keep_on(on)` call these; on desktop the second uses the Screen Wake Lock
API in the webview where present and the first does nothing. `useScreenBright.ts` turns both on when "Show large" opens and off
when it closes, the tab closes, the window hides or the app goes to the background.

## Share target (FR-037)

`AndroidManifest.xml` of the app (`src-tauri/gen/android/app/src/main/`):

```xml
<intent-filter>
  <action android:name="android.intent.action.SEND" />
  <action android:name="android.intent.action.SEND_MULTIPLE" />
  <category android:name="android.intent.category.DEFAULT" />
  <data android:mimeType="application/vnd.apple.pkpass" />
  <data android:mimeType="application/pdf" />
  <data android:mimeType="image/*" />
</intent-filter>
```

`MainActivity.kt` (`launchMode singleTask`): on `onCreate` and `onNewIntent`, for each `EXTRA_STREAM` URI it copies the stream at
once into the app's cache directory (the URI permission is transient), refuses a stream over 25 MiB, and hands the paths to
the plugin. On a cold start the plugin is not loaded in `onCreate`, so Kotlin buffers the paths and the plugin pulls them in
`load`. They go into the in-process store `sharedFiles` (a list of paths, kept until a vault is open; `deep_link/inbox.rs`
and `stores/shareInbox.ts` do not fit: one holds `.url` files, the other one text), and the frontend (`useSharedFile.ts`)
opens the pass import dialog then. A pkpass opens the pkpass dialog; a
PDF or an image opens the capture dialog (decode, pick, confirm). A cancelled or finished dialog removes the copy (FR-035).

## Pinned shortcut (FR-038)

Plugin command `pinPassShortcut { id: string, label: string }` calls `ShortcutManager.requestPinShortcut` with a shortcut whose
intent opens the app with `haex-vault://pass/<id>`. The label is what the user typed (default neutral text from i18n, never the
title). The id is an opaque UUID; no code and no title are in the shortcut. `deep_link` `known_link` accepts
`haex-vault://pass/<uuid>` (16 KiB limit unchanged); the inbox holds it until unlock; the frontend then opens `/entry/<id>`. An
unknown id shows the usual "not found" page. Where pinning is not supported the command returns `unsupported` and the button is
hidden.

## Camera (FR-034)

`@tauri-apps/plugin-barcode-scanner` is called with the formats QR, Code 128, EAN-13, PDF417 and Aztec (capability
`mobile.json`, `barcode-scanner:default`, unchanged). It returns text and format, which the capture dialog shows for
confirmation. Slice E verifies the formats on the emulator and a phone.

## Checks

- Emulator: `adb shell am start -a android.intent.action.SEND -t application/vnd.apple.pkpass --eu android.intent.extra.STREAM ...`
  with the vault locked (waits, then the dialog) and unlocked (dialog at once); a second share to a running app arrives in
  `onNewIntent`.
- Emulator: pin a shortcut (the launcher of the emulator image must support pinning), open it locked and unlocked.
- Manual on a phone: brightness restored on leave, screen stays on while large, camera formats.
