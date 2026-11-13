# Contract: how the header collapses

Implements FR-011, FR-014–FR-017, SC-002. A pure function in `src/lib/wm/headerLayout.ts`; `Header.vue` measures and
applies (research R7).

## Input

```ts
layoutHeader(
  controls: { id: string; area: 'start' | 'center' | 'end'; index: number; order: number;
              form: 'overflow' | 'icon' | 'shrink' | 'keep'; group: string | null; fullWidth: number;
              iconWidth: number | null; minWidth: number | null }[],
  fixed: { windowButtons: number; menuButton: number; gap: number; padding: number },
  available: number, // the header's width
): { states: Map<string, 'row' | 'icon' | { shrunk: number } | 'overflow'>; menuVisible: boolean }
```

- `order`: the control's own, or the default `1001 + rank` by place (data-model.md: the end area from its last control,
  then center, then start, each from its last control) so every control has one; ties between equal own orders are
  broken by that same place rank, so the result never depends on map or array order.
- `form` is already normalised: `icon` without an icon and `shrink` on a type with no smallest width are `overflow`.
- `group` (the control's `collapse.group`, `null` if none): controls of the same area with the same group move into
  the menu together. Wherever a control would become `overflow` (all three stages), the others of its group
  become `overflow` with it, so a pair such as back and forward is never split between the row and the menu.
- `iconWidth` is the measured width of the icon form (`null` unless the form is `icon`); `minWidth` is the type's
  smallest width (`null` unless the form is `shrink`).
- `fixed.windowButtons` is the measured width of the window buttons (one button on a compact window).

## Algorithm

1. Everything is `row` at its full width. If the sum fits, done (`menuVisible = false`).
2. Sort the controls by `order` (ties as above). Controls of form `keep` are set aside for step 5.
3. **First stage**, in order, until the sum fits (after each step re-check):
   - `overflow`: becomes `overflow` (moves into the menu); the menu button's width is added once.
   - `icon`: becomes `icon` (width `iconWidth`).
   - `shrink`: becomes `{shrunk: w}` with `w` as large as the remaining overflow allows, never below `minWidth`
     (so a control only shrinks as far as needed). A `shrink` control that is already at `minWidth` stays there.
4. **Second stage**, if still not fitting: in the same order, every control that is `icon` or `shrunk` becomes `overflow`.
5. **Third stage**, if still not fitting: the `keep` controls, in the order of a control that says nothing, become
   `overflow`. (Safety net; the window buttons and the menu button are never moved.)
6. `menuVisible` is true when at least one control is `overflow`; then `fixed.menuButton` is part of the sum from the
   start of step 3 (the loop re-runs once more with it).

## Restoring (FR-017)

The function is called again with the new width; it always starts at step 1, so widening gives back controls in the
reverse of the order they collapsed, each in its previous form, by construction. There is no state between calls.

## Focus

A control that stays in the row keeps its element (it is keyed by `id`), so focus does not move when others
collapse around it (FR-016). A focused control that moves into the menu returns focus to the menu button.

## Layout direction

Areas are logical (`start`, `end`), never `left`/`right`, so the function is the same in a right-to-left layout; the row
uses logical flex order and `Header.vue` is checked under `dir="rtl"` in the E2E scenario (tasks T018).

## Cases the checks cover (`scripts/check-wm-header.ts`)

| Case                                                      | Expected                                                              |
| --------------------------------------------------------- | --------------------------------------------------------------------- |
| fits                                                      | all `row`, no menu                                                    |
| two `overflow` controls, order 1 and 2                    | 1 collapses first, then 2; restore reverses                           |
| order across areas (start 5, end 1)                       | end control first, although it is not `start`                         |
| `icon` then `overflow` of the same control (second stage) | `icon` while it fits, `overflow` only when the whole row would not    |
| `shrink` search                                           | gets narrower in steps of what is missing, never below 120, then menu |
| `keep` among others                                       | others collapse first; `keep` last                                    |
| everything `keep`                                         | step 5 collapses them in the default place order                      |
| `icon` on a control without an icon                       | behaves as `overflow`                                                 |
| equal orders                                              | default place order, independent of array order                       |
| menu button width                                         | counted once, only while the menu has content                         |
| window narrower than the window buttons                   | nothing in the row but the buttons; menu holds the rest               |
| compact (one window button)                               | a wider budget by one button's width                                  |
