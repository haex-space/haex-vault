# Contract: the tab button

Implements FR-001, FR-002, FR-004, FR-005. Component: `src/components/wm/TabButton.vue`; pure face in
`src/lib/wm/tabSwitcher.ts`.

## Where

Inside the window-button group of the header (spec 090's fixed end part), before the minimize button. Owned by holzi:
not part of a `WmHeaderDescription`, not collapsible into the header's overflow menu. The header's layout function
measures the group as one fixed width, so no change to `headerLayout.ts` is needed, provided the measurement takes the
whole slot and is repeated when the button's width changes (T019). This spec amends spec 090's "the window buttons are the
one part of the header that belongs to holzi": the tab button is a second one (T035).

## Faces

See [data-model.md](../data-model.md) §TabButtonFace. One tab: a "+" icon. Two or more: the number in a rounded
badge, `99+` from 100.

## Behaviour

| State                           | Press                                  | Result                     |
| ------------------------------- | -------------------------------------- | -------------------------- |
| 1 tab, closed                   | `open(windowId, 'apps')` from `button` | app tiles over the content |
| ≥2 tabs, closed                 | `open(windowId, 'tabs')`               | overview                   |
| overview open                   | `close`                                | closed                     |
| app tiles open, from `button`   | `close`                                | closed                     |
| app tiles open, from `overview` | `back`                                 | overview                   |

The press is a UI toggle and is marked `action-exempt:` (research R2); opening a tab goes through `wm.tab.new`.

Like the window buttons, the button stops `pointerdown` (no window drag) and `dblclick` (a quick double press must not
maximize the window).

## Accessibility

- A real `<button>`; `aria-label` from the face's accessible name; `aria-expanded` and `aria-controls` (the overlay's
  id) while it applies; a visible focus ring; the tooltip equals the accessible name.
- The attention mark is a dot **and** part of the accessible name (never colour alone).
- Size: the header's control size of 32 px on every window (spec 090 R8: the header has one fixed height). On a compact
  window the hit area grows to 44 × 44 px through padding (spec 015, T050) without making the header taller. The 44 px
  full size is the rule for the tiles' close buttons (switcher-overlay.md).

## Attention

`attention = window.tabs.some(t => t.id !== window.activeTabId && wm.runtimeFor(t.id).attention)`. It clears when the
app calls `clearAttention` (the chat when its prompt is answered, an extension with `extension_tab_attention {active:
false}`). Showing a tab does not clear it; this spec does not change that. The mark is gone when no inactive tab has
attention.
