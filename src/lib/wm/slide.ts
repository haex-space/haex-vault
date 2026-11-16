// How a panel slides in from a window's edge, one place for the panels of a window (spec 090-app-header-bar): the
// sidebar of an app frame (`useSidebarFrame`) on the left, the header's overflow sidebar
// (`HeaderOverflow.vue`, the same `Sidebar.vue`) on the right and the search field that drops in below the header
// (`HeaderSearchPopover.vue`). All move with the same time and curve and stand still for a
// reduced-motion setting, so they feel alike. The classes are whole strings so Tailwind finds them.

/** Time, curve and the reduced-motion rule of a slide. */
export const SLIDE_TIMING =
  'duration-200 ease-out motion-reduce:transition-none'

/**
 * The classes of a window sidebar, the one definition of the app sidebar on the left and of the
 * header's overflow sidebar on the right: below `@2xl` it lies over the content and slides in from
 * its side, from `@2xl` it is a column beside the content that is hidden by its width. `open` is the
 * narrow state, `wideHidden` the wide one. One width for every sidebar.
 */
export function sidebarPanelClass(
  open: boolean,
  wideHidden: boolean,
  side: 'left' | 'right' = 'left',
): string[] {
  const away = side === 'left' ? '-translate-x-full' : 'translate-x-full'
  return [
    `absolute inset-0 z-20 bg-background transition-[translate,width,visibility] ${SLIDE_TIMING}`,
    '@2xl:static @2xl:z-auto @2xl:shrink-0 @2xl:overflow-hidden @2xl:translate-x-0',
    side === 'right' ? 'border-l border-border' : '',
    open ? 'visible translate-x-0' : `invisible ${away}`,
    wideHidden ? '@2xl:invisible @2xl:w-0' : '@2xl:visible @2xl:w-64',
  ]
}

/** `<Transition>` classes of the search field that drops in below the header (`HeaderSearchPopover`). */
export const DROP_FROM_TOP = {
  enterActiveClass: `transition-[opacity,translate] ${SLIDE_TIMING}`,
  leaveActiveClass:
    'transition-[opacity,translate] duration-150 ease-in motion-reduce:transition-none',
  enterFromClass: 'opacity-0 -translate-y-2',
  leaveToClass: 'opacity-0 -translate-y-2',
} as const
