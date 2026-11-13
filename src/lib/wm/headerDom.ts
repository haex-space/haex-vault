// Where a control of the window's header is in the page, for an app that has to hand the focus to it
// (spec 093-app-headers, FR-004 and the "Keyboard" edge case). The header belongs to the window, so it
// sits outside the app's own root: look from the app's element up to its window, then down.

/** The element of the header's control with this test id, seen from any element of the same window. */
export function headerControl<T extends HTMLElement = HTMLElement>(
  from: Element | null | undefined,
  testId: string,
): T | null {
  return (
    from
      ?.closest('[data-wm-window-id]')
      ?.querySelector<T>(
        `[data-testid="wm-header"] [data-testid="${testId}"]`,
      ) ?? null
  )
}
