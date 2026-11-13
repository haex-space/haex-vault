// What a window shares with the overlays of its content (spec 090-app-header-bar). `Window.vue`
// provides the element that covers the content below its header, and the header's overflow panel is
// teleported into it, so the panel belongs to the window and not to the screen. It also provides
// whether that panel is open: a sidebar that covers the content (`useSidebarFrame`) and the panel
// never stay open together, whichever opens last closes the other.
import type { InjectionKey, Ref } from 'vue'

export const WM_OVERLAY: InjectionKey<Ref<HTMLElement | null>> =
  Symbol('wmWindowOverlay')

export const WM_OVERFLOW_OPEN: InjectionKey<Ref<boolean>> = Symbol(
  'wmWindowOverflowOpen',
)
