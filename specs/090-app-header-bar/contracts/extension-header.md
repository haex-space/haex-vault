# Contract: the extension's side of the header

Implements FR-018–FR-023. Companion to [spec 017 `bridge.md`](../../017-extension-host/contracts/bridge.md); the method
table there gets one row and `permissions.md` one note.

## Bridge method

| Method                 | Params                  | Result | Permission | Errors                                                            |
| ---------------------- | ----------------------- | ------ | ---------- | ----------------------------------------------------------------- |
| `extension_header_set` | a `WmHeaderDescription` | `null` | none       | `3001` invalid (reason in the message), `8002` extension disabled |

No permission: the call can only change the header of the calling frame's own tab (FR-023). It is listed in
`allowlist.rs` next to `extension_tab_attention`; the handler is `header::set(ctx, params)`.

Handler: validate ([header-description.md](./header-description.md)), read the icon files through the shared helper
(`extensions/icon_file.rs`, also used by notifications; a database read, as the notification call has), add the `icons`
table (only icons not yet sent for this frame; see research R9), then
`ctx.emitter.emit("extension-header-set", {"frame": ctx.session.frame, "header": <validated, with icons>})` and return
`null`.

## Tauri event to the frontend

`extension-header-set { frame, header }`. `useExtensionFrame` (through `useExtensionHeader`, to keep that file under
500 lines) accepts it only when `frame` is its own; it then calls `tab.setHeader(header)` and registers the handler
that posts events to the SDK port. On reload, close, failure and unmount it calls `tab.setHeader(null)`.

## Event to the extension

On the frame's SDK port, from the frontend: `{ type: "haextension:header:event", data: WmHeaderEvent, timestamp }`.
Sent only for the frame's own tab's header and only while that description is the one shown. A control that no longer
exists in the current description produces no event (spec: Edge Cases). A disabled control, and a disabled menu entry,
produces no event either.

`input` events of a `search` are merged: the latest text is sent at most every ~100 ms; `submit`, blur and a change
of description flush immediately (FR-012).

## SDK (`haex-space/vault-sdk`, separate PR; holzi does not wait for it)

```ts
client.header.set(description: HeaderDescription): Promise<boolean>   // extension_header_set; false = host has no header
client.header.onEvent(handler: (event: HeaderEvent) => void): () => void
// HAEXTENSION_EVENTS.HEADER_EVENT = 'haextension:header:event'
```

Types `HeaderDescription`, `HeaderControl` and `HeaderEvent` are exported from the package; `icon` is a path in the
extension's bundle (or a `data:` URL in a development version). In haex-vault 2.x `header.set` resolves with `false` (the host's unknown-method reply is absorbed, research R14);
every other error, such as `3001` for a refused description, still rejects.

## What an extension must not be able to do (tests)

| Attempt                                                                                                                                                                                          | Result                                                                 |
| ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ---------------------------------------------------------------------- |
| a text with `<img onerror=…>` as a label                                                                                                                                                         | shown as that text                                                     |
| 33 controls, a 65-entry menu, a 33 KiB description                                                                                                                                               | refused with `3001`, previous header stays                             |
| a button without `label`; duplicate `id`                                                                                                                                                         | refused                                                                |
| an unknown control type among valid ones                                                                                                                                                         | the valid ones show                                                    |
| an `event` or a description naming another frame/tab                                                                                                                                             | impossible: the frame id comes from holzi's session, not from `params` |
| an icon that is a `lucide:` name, a URL, a path outside the bundle, a non-picture file, or over 256 KiB (a file or a decoded inline picture); an inline `data:` icon from an installed extension | refused with `3001`, previous header stays                             |
| a `testId` field, or a control character or bidi-format character in a text                                                                                                                      | refused with `3001`, previous header stays                             |
| an SVG with a script or an external reference                                                                                                                                                    | drawn as an image: the script does not run, nothing loads              |
| an `icons` table sent in `params`                                                                                                                                                                | ignored; Rust builds the table from the bundle                         |
| the window buttons                                                                                                                                                                               | no field can reach, cover or move them                                 |
