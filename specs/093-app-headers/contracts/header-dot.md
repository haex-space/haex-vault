# Contract: the dot on a menu control

Delta to `specs/090-app-header-bar/contracts/header-description.md`.

## Description

```jsonc
{ "type": "menu", "id": "tag-filter", "label": "Filter by tag", "icon": "lucide:tag",
  "active": true, "items": [ { "type": "item", "id": "all", "label": "All tags", "checked": false } ] }
```

## Checks (Rust, `header_validate.rs`, extension descriptions)

1. `active` on a `menu` must be a boolean (or null); another value refuses the call, as `checked`, `disabled` and the
   other flags do (`flag()` in `header_validate.rs`).
2. `active` on a type other than `menu` is not read and is dropped when the control is deserialised (serde ignores the
   unknown field), consistent with the other per-type fields.

## What the frontend draws

| Where the menu sits                         | What shows                                                                       |
| ------------------------------------------- | -------------------------------------------------------------------------------- |
| In the row, icon form                       | a dot at the top right of the icon                                               |
| In the row, with `text`                     | the dot on the icon, the text unchanged                                          |
| In the overflow panel (`form: menu`)        | the dot on the row's icon; and the overflow button carries a dot too (R4)        |
| `active` false or absent                    | no dot                                                                           |

The dot has an accessible text: the control's `aria-label` gets the suffix from `wm.header.active` ("active"), so the
state is not colour alone.

## Tests

- `header_tests.rs`: accepts `active` on a menu, refuses a non-boolean, ignores it on a button.
- `check-wm-header.ts`: the overflow button shows the dot iff an overflowed control is `active`.
- `window-header` E2E: set a filter in a pilot menu, narrow the window, see the dot on the overflow button.
