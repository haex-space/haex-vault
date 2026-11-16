// What a window shares with the overlays of its content (spec 090-app-header-bar). `Window.vue`
// provides the element that covers the content below its header, and the header's overflow panel is
// teleported into it, so the panel belongs to the window and not to the screen. It also provides
// whether that panel is open: a sidebar that covers the content (`useSidebarFrame`) and the panel
// never stay open together, whichever opens last closes the other. The field of a `search` control
// opens the same way, as a popover below the header (`HeaderSearchPopover.vue`); `WM_SEARCH_OPEN`
// holds the id of the control whose field is open, and the field and the overflow panel never stay
// open together either.
import type { InjectionKey, Ref } from 'vue'

export const WM_OVERLAY: InjectionKey<Ref<HTMLElement | null>> =
  Symbol('wmWindowOverlay')

export const WM_OVERFLOW_OPEN: InjectionKey<Ref<boolean>> = Symbol(
  'wmWindowOverflowOpen',
)

/** The id of the `search` control whose field is open under the header; `null` while none is. */
export const WM_SEARCH_OPEN: InjectionKey<Ref<string | null>> =
  Symbol('wmWindowSearchOpen')

/**
 * The element at the right of a window's content where the header draws its overflow sidebar
 * (`Window.vue` provides it): a column beside the content in a wide window, over the content in a
 * narrow one, like the app's sidebar on the left.
 */
export const WM_SIDEBAR_HOST: InjectionKey<Ref<HTMLElement | null>> = Symbol(
  'wmWindowSidebarHost',
)
