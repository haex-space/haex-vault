// Where a window's own overlays are drawn (spec 090-app-header-bar): `Window.vue` provides the
// element that covers the content below its header, and the header's overflow menu is teleported
// into it, so the menu belongs to the window and not to the screen.
import type { InjectionKey, Ref } from 'vue'

export const WM_OVERLAY: InjectionKey<Ref<HTMLElement | null>> =
  Symbol('wmWindowOverlay')
