# Contract: Tauri commands

| Command             | Arguments           | Result                                           | Notes                                                                                                                           |
| ------------------- | ------------------- | ------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------- |
| `link_text_parse`   | `{ text: string }`  | `LinkText` (see data model)                      | Works without an open vault. Errors: `NotALinkCode`, `UnsupportedVersion` (localized by kind in the UI, like other error kinds) |
| `link_code_create`  | –                   | `LinkCodeInfo` (+ `text`; `qrSvg` can be `null`) | existing; main device only; QR is rendered from `text`                                                                          |
| `link_join_start`   | `LinkJoinStartArgs` | `LinkJoinState`                                  | **unchanged**; the sheet passes the parsed servers as before                                                                    |
| `camera_scan_begin` | –                   | –                                                | opens the camera gate; only the `main` window may call it                                                                       |
| `camera_scan_end`   | –                   | –                                                | closes the gate; idempotent; also done when the window closes                                                                   |

Events: none new.

Capabilities: the two camera commands are added to the `main` window's default capability; `mobile.json` is unchanged
(`barcode-scanner:default`).

Permission handler: `PermissionKind::Camera` → `Allow` while the gate is open, `Deny` otherwise; all other kinds `Default`.
