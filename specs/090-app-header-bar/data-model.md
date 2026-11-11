# Data Model: The header bar belongs to the app

Nothing here is persisted. A description lives in the tab's runtime and ends with the tab, the frame or the app that
set it (research R5). Type names carry the `WmHeader` prefix (feedback: ts-rs name collisions).

## WmHeaderDescription

What an app hands holzi for its window's header (FR-007). It belongs to one tab and replaces the previous one whole.

| Field    | Type                | Notes                                                                    |
| -------- | ------------------- | ------------------------------------------------------------------------ |
| `start`  | `WmHeaderControl[]` | optional, default `[]`; the left area                                    |
| `center` | `WmHeaderControl[]` | optional, default `[]`; the area that takes the free space               |
| `end`    | `WmHeaderControl[]` | optional, default `[]`; the right area, left of the window buttons       |
| `icons`  | `{ [path]: dataUrl }` | **added by Rust**, never by the app; only the pictures not yet sent for this frame; the frontend merges them into the tab's icon table (R9) |

Invariants: at most 32 controls in all; an identifier appears once in the whole description (also across menu entries,
options and segments); at most 32 KiB serialised without inline `data:` icons; every icon file and every inline `data:` icon (decoded) at most 256 KiB, no limit
on the number of icons or their total size. (Limits: research R3.) Layout direction decides which side `start`
is on; the order inside an area is the order given.

## WmHeaderControl

Common fields of every control:

| Field      | Type                | Rule                                                                                      |
| ---------- | ------------------- | ----------------------------------------------------------------------------------------- |
| `type`     | string              | one of the catalog below; an unknown type is left out (FR-021)                            |
| `id`       | string              | required, unique in the description; comes back in events (FR-010)                        |
| `label`    | string              | accessible name; required for every type except `title` and `separator` (FR-009)          |
| `icon`     | string              | optional. System app: an Iconify name (`lucide:…`). Extension: a path in its own bundle; a development version (no stored bundle) may give a `data:` URL instead (R9) |
| `testId`   | string              | optional, **system apps only**: the test id of the drawn control (default `wm-header-<id>`); Rust refuses it in an extension's description (contract check 6) |
| `monochrome` | boolean           | optional, default `false`; draw the picture in the text colour from its alpha channel (extension pictures only) |
| `tooltip`  | string              | optional; default is `label`                                                              |
| `disabled` | boolean             | optional, default `false`                                                                 |
| `collapse` | `WmHeaderCollapse`  | optional (FR-011)                                                                         |

### Catalog (FR-008)

| `type`       | Extra fields                                                                                   | Events                       |
| ------------ | ---------------------------------------------------------------------------------------------- | ---------------------------- |
| `button`     | `text?` (visible text); at least one of `icon` and `text`                                      | `click`                      |
| `toggle`     | `pressed: boolean`, `text?`; at least one of `icon` and `text`                                 | `toggle` (new state)         |
| `search`     | `value: string`, `placeholder?`                                                                | `input`, `submit`            |
| `menu`       | `items: WmHeaderMenuItem[]`, `text?`; at least one of `icon` and `text`                        | `click` (`value` = item id)  |
| `choice`     | `options: {id, label, icon?}[]` (1–8), `selected: string` (an option id)                        | `select` (option id)         |
| `breadcrumb` | `segments: {id, label}[]` (1–64), `editable?: boolean`, `editValue?: string`                   | `select` (segment id), `submit` (typed path, when `editable`) |
| `title`      | `text: string` (replaces `label`; no accessible name needed)                                   | none                         |
| `separator`  | none                                                                                           | none                         |

`WmHeaderMenuItem` is `{type: "item", id, label, icon?, disabled?}` or `{type: "separator"}`.

### WmHeaderCollapse

| Field   | Type                                          | Default                                                       |
| ------- | --------------------------------------------- | ------------------------------------------------------------- |
| `order` | integer 0–1000; the lowest collapses first    | `1001 + rank`, where `rank` counts the controls over the areas in this sequence: the end area from its last control, then center from its last control, then start from its last control (0, 1, 2, …). So a control that says nothing collapses after every control with an order of its own |
| `form`  | `overflow` \| `icon` \| `shrink` \| `keep`    | `overflow`                                                    |

Form that does not fit the control (`icon` without an icon, `shrink` on a button): counts as `overflow` (spec, Edge Cases).
`shrink` has a type-given smallest width: `search` 120 px, `breadcrumb` 96 px, `title` 64 px (R7).

## WmHeaderEvent

What the app is told when a control is used (FR-012). Same shape for an extension and a system app.

| Field   | Type                                                    |
| ------- | ------------------------------------------------------- |
| `id`    | string — the control's id                               |
| `kind`  | `click` \| `toggle` \| `input` \| `submit` \| `select`  |
| `value` | string \| boolean, absent for `click` of a button       |

To an extension it travels as `{type: "haextension:header:event", data: WmHeaderEvent, timestamp}` on the SDK port.

## TabRuntime (extended)

`src/lib/wm/types.ts`, per tab, never persisted:

| New field       | Type                                         | Set by                                          | Cleared by                                  |
| --------------- | -------------------------------------------- | ----------------------------------------------- | ------------------------------------------- |
| `header`        | `WmHeaderDescription \| null`                | `WmTabApi.setHeader`                            | frame reload/close/failure; app unmount; tab closed |
| `headerHandler` | `(event: WmHeaderEvent) => void \| null`     | `WmTabApi.registerHeaderHandler`                | the returned unregister function            |

## Layout result (internal, `headerLayout.ts`)

`layoutHeader` returns, per control id, `{state: "row"} | {state: "icon"} | {state: "shrunk", width: number} |
{state: "overflow"}`, plus `menuVisible: boolean` (whether the overflow menu button shows). It is derived, never stored (contracts/collapse.md).

## State transitions

```text
tab has no description ── setHeader(d) ──▶ shows d (title in the middle if d.center is empty)
        ▲                                        │
        │ frame reloads / closes / app unmounts  │ setHeader(d2): replaces d whole
        └────────────────────────────────────────┘
tab not active: description kept, not shown; active again: shown again
```

## Search field state (FR-013)

The header component keeps the text the user typed. A new description's `value` is applied only when it differs from
the `value` the previous description handed over (not from what is typed). An echo of the typed text therefore changes
nothing, caret and focus stay.
