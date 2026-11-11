# Contract: the header description and its checks

Shape: [data-model.md](../data-model.md). Numbers: [research.md](../research.md) R3. This contract is what Rust
(`bridge/header.rs`) enforces for an extension and what TypeScript (`WmHeader*` bindings) types for a system app.

## Example

```json
{
  "start": [
    { "type": "toggle", "id": "sidebar", "label": "Show sidebar", "icon": "icons/sidebar.svg", "monochrome": true, "pressed": true,
      "collapse": { "form": "keep" } }
  ],
  "center": [
    { "type": "search", "id": "q", "label": "Search notes", "placeholder": "Search…", "value": "",
      "collapse": { "order": 10, "form": "shrink" } }
  ],
  "end": [
    { "type": "choice", "id": "view", "label": "View", "selected": "list",
      "options": [{ "id": "list", "label": "List", "icon": "icons/list.svg" }, { "id": "grid", "label": "Grid", "icon": "icons/grid.svg" }],
      "collapse": { "order": 20 } },
    { "type": "menu", "id": "more", "label": "More", "icon": "icons/more.svg",
      "items": [{ "type": "item", "id": "export", "label": "Export…", "icon": "icons/export.svg" }, { "type": "separator" },
                { "type": "item", "id": "settings", "label": "Settings" }],
      "collapse": { "order": 30, "form": "keep" } }
  ]
}
```

## Checks, in this order

For a bridge call `extension_header_set` with `params` = a description (an object):

| #   | Check                                                                                      | On failure                                                       |
| --- | ------------------------------------------------------------------------------------------ | ---------------------------------------------------------------- |
| 1   | `params` serialises to at most 32 KiB (inline `data:` icon strings not counted)            | whole description refused                                        |
| 2   | `params` is an object; `start`, `center`, `end` are each absent or an array                | whole refused                                                    |
| 3   | at most 32 controls over the three arrays                                                  | whole refused                                                    |
| 4   | each element is an object with a string `type`                                             | whole refused                                                    |
| 5   | `type` is not in the catalog                                                               | **that control is left out**; the rest goes on                    |
| 6   | the control's fields match its type (data-model.md), lengths and counts within the limits  | whole refused, message names the control's id and the field       |
| 7   | `id` matches `[A-Za-z0-9._:-]{1,64}` and is unique over controls, entries, options and segments | whole refused                                                |
| 8   | `label` present where required (FR-009), a button/toggle/menu has an `icon` or `text`      | whole refused                                                    |
| 9   | each `icon` is a bundle path (no `://`, no `..`, one of the five picture types by extension) or an inline `data:` URL of such a type; a `lucide:` name, a URL or another type is refused | whole refused, message names the control's id |
| 9a  | each distinct icon file is read with `read_verified_file` and is at most 256 KiB; a missing or unreadable file is an error. There is no limit on the number of icons or on their total size: the files are shipped in the signed bundle, nothing is loaded from outside, and the number of icons is bounded by the number of controls and entries (R3) | whole refused, message names the path |
| 9b  | a field `icons` in `params` is ignored (Rust builds it)                                    | dropped silently                                                  |
| 10  | `collapse.order` an integer 0–1000, `collapse.form` one of the four                        | whole refused                                                    |
| 11  | `choice.selected` is one of its option ids                                                 | whole refused                                                    |

A refused description leaves the previous one in place (nothing is emitted). The call answers with the existing
validation error `3001` and a message of the form `header: control "<id>": <what is wrong>`; the message never
quotes more than the id and the field name (FR-062 of spec 017 style). The extension's developer console shows it
(spec 017, US12) because the error goes through the same path as every refused call.

An empty description (`{}`) is valid and clears the controls (the header shows the title and the window buttons).

## What the frontend does with a valid description

- Stores it in the calling tab's `TabRuntime.header` (the frame's own tab only: the event carries the frame id, the
  frame composable maps it to its tab, FR-019).
- Renders every text as text (`{{ }}`, never `v-html`; FR-023).
- Renders a system app's `lucide:` icon as an Iconify icon and an extension's picture from `icons[icon]` as an `<img>`
  (`mask-image` when `monochrome`); a path missing from `icons` renders nothing and cannot happen for Rust-checked input (R9).
- Applies at most one description per animation frame (R7).

## System apps

`useWmHeader(() => description, onEvent)` takes the same type. A system app is trusted code, so no run-time
check is made; the TypeScript type and the check scripts keep it correct, and a violation in development logs to the
console (`console.warn`) rather than throwing.

## Not part of the contract

Layout, the collapse algorithm ([collapse.md](./collapse.md)), the bridge method's place in the allowlist
([extension-header.md](./extension-header.md)).
