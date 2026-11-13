# Data Model: The headers of Contacts, Passwords, Wallet and the Marketplace

Nothing is persisted. One field is added to spec 090's description (`specs/090-app-header-bar/data-model.md`).

## WmHeaderControl — `menu`

| Field    | Type    | Rule                                                                                                                           |
| -------- | ------- | ------------------------------------------------------------------------------------------------------------------------------ |
| `active` | boolean | optional, default `false`; the menu's filter, sort or view is not at its default: the renderer draws a dot (spec 093 FR-008)   |

`active` on any other type is left out by the checks (as an unknown field of that type). It is state of the app, never
persisted by holzi.

## Per-app descriptions (derived, not stored)

| App         | Place               | start                    | center | end (in this order)                                                    |
| ----------- | ------------------- | ------------------------ | ------ | ---------------------------------------------------------------------- |
| Passwords   | list, folder, tag   | `sidebar` (toggle, keep) | `q`    | `tag-filter` (menu), `menu` (menu), `new` (menu)                       |
| Passwords   | entry open          | `sidebar`                | —      | `menu`                                                                 |
| Contacts    | list, group         | `sidebar`                | `q`    | `new` (menu)                                                           |
| Contacts    | contact/import open | `sidebar`                | —      | —                                                                      |
| Wallet      | list                | `sidebar`                | `q`    | `filters` (menu), `new` (button or menu)                               |
| Wallet      | pass open           | `sidebar`                | —      | —                                                                      |
| Marketplace | catalogue           | —                        | `q`    | `category` (menu), `source` (menu, if >1 enabled), `refresh` (button)  |

Collapse order values (lower leaves first), after the pilots: `q` 10 (shrink), `tag-filter`/`filters`/`category`/`source`
20, `menu`/`refresh` 30, `new` 40 (leaves last); `sidebar` is `keep`. Final values are fixed in each app's check script.
