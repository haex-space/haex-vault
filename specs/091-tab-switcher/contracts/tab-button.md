# Contract: the tab button

Implements FR-001, FR-002, FR-004, FR-005. Component: `src/components/wm/TabButton.vue`; pure face in
`src/lib/wm/tabSwitcher.ts`.

## Where

Inside the window-button group of the header (spec 090's fixed end part), before the minimize button. Owned by holzi:
not part of a `WmHeaderDescription`, not collapsible into the header's overflow menu. The header's layout function
measures the group as one fixed width, so no change to `headerLayout.ts` is needed.

## Faces

See [data-model.md](../data-model.md) §TabButtonFace. One tab: a "+" icon. Two or more: the number in a rounded
badge, `99+` from 100.

## Behaviour

| State                                  | Press                                        | Result                                          |
| -------------------------------------- | -------------------------------------------- | ----------------------------------------------- |
| 1 tab, closed                          | `open(windowId, 'apps')` from `button`       | app tiles over the content                      |
| ≥2 tabs, closed                        | `open(windowId, 'tabs')`                     | overview                                        |
| overview open                          | `close`                                      | closed                                          |
| app tiles open, from `button`          | `close`                                      | closed                                          |
| app tiles open, from `overview`        | `back`                                       | overview                                        |

The press is a UI toggle and is marked `action-exempt:` (research R2); opening a tab goes through `wm.tab.new`.

## Accessibility

- A real `<button>`; `aria-label` from the face's accessible name; `aria-expanded` and `aria-controls` (the overlay's
  id) while it applies; a visible focus ring; the tooltip equals the accessible name.
- The attention mark is a dot **and** part of the accessible name (never colour alone).
- Touch target: at least 44 × 44 px on a compact window (spec 015, T050), the normal 32 px control size otherwise.

## Attention

`attention = window.tabs.some(t => t.id !== window.activeTabId && wm.runtimeFor(t.id).attention)`. It clears when the
tab is activated (existing `clearAttention` on activate) and no other tab has it.
