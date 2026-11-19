import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'

/** What the core reports (`platform/insets.rs`), in CSS pixels. */
interface Insets {
  top: number
  right: number
  bottom: number
  left: number
  keyboard: number
}

const SIDES = ['top', 'right', 'bottom', 'left', 'keyboard'] as const

/**
 * Spec 043 (FR-011, FR-012): the space the system bars, the display cutout and the on-screen
 * keyboard take on a phone, as `--haex-inset-*` on the root element; `.haex-safe-area` pads the
 * page roots by it. When the keyboard opens, the focused field is scrolled back into view. On a
 * desktop the core reports nothing and the variables stay unset.
 */
export default defineNuxtPlugin(() => {
  let keyboard = 0

  function apply(insets: Insets | null) {
    if (!insets) return
    const style = document.documentElement.style
    for (const side of SIDES) {
      style.setProperty(`--haex-inset-${side}`, `${insets[side]}px`)
    }
    const opened = insets.keyboard > keyboard
    keyboard = insets.keyboard
    if (opened) {
      // After the layout took the new padding.
      requestAnimationFrame(() => {
        const focused = document.activeElement
        if (focused instanceof HTMLElement && focused !== document.body)
          focused.scrollIntoView({ block: 'nearest' })
      })
    }
  }

  void invoke<Insets | null>('device_insets')
    .then(apply)
    .catch(() => {})
  void listen<Insets>('device-insets', (event) => apply(event.payload)).catch(
    () => {},
  )
})
