# Data Model: The headers of Contacts, Passwords, Wallet and the Marketplace

Nothing is persisted. One field is added to spec 090's description (`specs/090-app-header-bar/data-model.md`).

## WmHeaderControl — `menu`

| Field    | Type    | Rule                                                                                                                         |
| -------- | ------- | ---------------------------------------------------------------------------------------------------------------------------- |
| `active` | boolean | optional, default `false`; the menu's filter, sort or view is not at its default: the renderer draws a dot (spec 093 FR-008) |

`active` on any other type is left out by the checks (as an unknown field of that type). It is state of the app, never
persisted by holzi. In the overflow panel a menu is a group (heading plus entries), so the dot is drawn next to the
heading there; it is positioned over the icon in the row and adds no width.

An entry id is unique in the whole description, not only inside its menu (`headerChecks` warns about a repeated id and
Rust refuses it for extensions). The builders therefore name entries so they cannot collide with a control id or with
another menu's entry: the Marketplace's two "all" entries are `all-categories` and `all-sources`, and a category slug or
a marketplace id is used as `cat:<slug>` and `src:<id>`; the Contacts menu control is `add` (its entries `new`
and `import`), not `new`.

## Per-app descriptions (derived, not stored)

| App         | Place                                  | start                    | center | end (in this order)                                                   |
| ----------- | -------------------------------------- | ------------------------ | ------ | --------------------------------------------------------------------- |
| Passwords   | every place but an entry               | `sidebar` (toggle, keep) | `q`    | `tag-filter` (menu), `menu` (menu), `new` (menu)                      |
| Passwords   | entry open                             | `sidebar`                | —      | `menu`                                                                |
| Contacts    | list, group                            | `sidebar`                | `q`    | `add` (menu)                                                          |
| Contacts    | contact/import open                    | `sidebar`                | —      | —                                                                     |
| Wallet      | list                                   | `sidebar`                | `q`    | `filters` (menu), `capture`, `import`, `new` (buttons)                |
| Wallet      | pass, trash or other detail place open | `sidebar`                | —      | —                                                                     |
| Marketplace | catalogue                              | —                        | `q`    | `category` (menu), `source` (menu, if >1 enabled), `refresh` (button) |

Collapse order values (lower leaves first), after the pilots: `q` 10 (shrink), `tag-filter`/`filters`/`category`/`source`
20, `menu`/`refresh` 30, `new` 40 (leaves last); `sidebar` is `keep`. Final values are fixed in each app's check script.
