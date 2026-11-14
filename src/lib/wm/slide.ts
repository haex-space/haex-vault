// How a panel slides in from a window's edge, one place for the two kinds of panel (spec 090-app-header-bar): the
// sidebar of an app frame (`useSidebarFrame`) and the header's overflow panel (`HeaderOverflow.vue`).
// Both move with the same time and curve and stand still for a reduced-motion setting, so they feel
// alike. The classes are whole strings so Tailwind finds them.

/** Time, curve and the reduced-motion rule of a slide. */
export const SLIDE_TIMING =
  'duration-200 ease-out motion-reduce:transition-none'

/**
 * The classes of an app sidebar: below `@2xl` it lies over the content and slides in from the left,
 * from `@2xl` it is a column beside the content that is hidden by its width. `open` is the narrow
 * state (`menuOpen`), `wideHidden` the wide one. One width for every app.
 */
export function sidebarPanelClass(
  open: boolean,
  wideHidden: boolean,
): string[] {
  return [
    `absolute inset-0 z-20 bg-background transition-[translate,width,visibility] ${SLIDE_TIMING}`,
    '@2xl:static @2xl:z-auto @2xl:shrink-0 @2xl:overflow-hidden @2xl:translate-x-0',
    open ? 'visible translate-x-0' : 'invisible -translate-x-full',
    wideHidden ? '@2xl:invisible @2xl:w-0' : '@2xl:visible @2xl:w-64',
  ]
}

/** `<Transition>` classes of a panel that slides in from the right edge (the overflow panel). */
export const SLIDE_FROM_RIGHT = {
  enterActiveClass: `transition-transform ${SLIDE_TIMING}`,
  leaveActiveClass:
    'transition-transform duration-200 ease-in motion-reduce:transition-none',
  enterFromClass: 'translate-x-full',
  leaveToClass: 'translate-x-full',
} as const

/** `<Transition>` classes of the backdrop behind such a panel: it fades, it does not move. */
export const FADE = {
  enterActiveClass: `transition-opacity ${SLIDE_TIMING}`,
  leaveActiveClass:
    'transition-opacity duration-200 ease-in motion-reduce:transition-none',
  enterFromClass: 'opacity-0',
  leaveToClass: 'opacity-0',
} as const
