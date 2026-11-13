# Contract: the dot on a menu control

Delta to `specs/090-app-header-bar/contracts/header-description.md`.

## Description

```jsonc
{
  "type": "menu",
  "id": "tag-filter",
  "label": "Filter by tag",
  "icon": "lucide:tag",
  "active": true,
  "items": [
    { "type": "item", "id": "all", "label": "All tags", "checked": false },
  ],
}
```

## Checks (Rust, `header_validate.rs`, extension descriptions)

1. `active` on a `menu` must be a boolean (or null); another value refuses the call, as `checked`, `disabled` and the
   other flags do (`flag()` in `header_validate.rs`).
2. `active` on a type other than `menu` is not read and is dropped when the control is deserialised (serde ignores the
   unknown field), consistent with the other per-type fields.

## What the frontend draws

| Where the menu sits              | What shows                                                                                                           |
| -------------------------------- | -------------------------------------------------------------------------------------------------------------------- |
| In the row, icon form            | a dot at the top right of the icon                                                                                   |
| In the row, with `text`          | the dot on the icon, the text unchanged                                                                              |
| In the overflow panel (`inMenu`) | the dot after the group's heading (the group has no icon of its own); and the overflow button carries a dot too (R4) |
| `active` false or absent         | no dot                                                                                                               |

The dot is positioned over the icon (absolute) and adds no width, so the ghost row's measured widths, and with them the
collapse, are the same with and without it.

In the overflow panel the entries of a menu have no `data-testid` today (only the group has one). Step 0 gives each
entry row there the same id as in the row, `<test id of the menu>-<entry id>`, so a scenario finds an entry whether the
control is in the row or in the panel (FR-004; the E2E helper `clickHeaderControl` opens the panel first).

The dot has an accessible text: the control's `aria-label` gets the suffix from `wm.header.active` ("active"), so the
state is not colour alone.

## Tests

- `header_tests.rs`: accepts `active` on a menu, refuses a non-boolean, ignores it on a button.
- `check-wm-header.ts`: the overflow button shows the dot iff an overflowed control is `active`.
- `window-header` E2E: a menu with `active` (the dev extension fixture of `extension-header`), narrow the window, see the
  dot on the overflow button and next to the group's heading; an entry is clickable by its `<menu>-<entry>` id in the
  panel.
