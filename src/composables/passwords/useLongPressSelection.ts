import { onLongPress } from '@vueuse/core'

interface Options {
  delay?: number
}

/**
 * Wires long-press (500ms touch/mouse hold) to a selection trigger while
 * suppressing the click that fires on pointerup so the trigger doesn't
 * immediately toggle again.
 *
 * Matches the file-manager convention on iOS/Android: press-and-hold
 * enters selection mode on the pressed row.
 */
export const useLongPressSelection = (
  elementRef: Ref<HTMLElement | null | undefined>,
  onTrigger: () => void,
  options: Options = {},
) => {
  const suppressNextClick = ref(false)

  onLongPress(
    elementRef,
    () => {
      suppressNextClick.value = true
      onTrigger()
      // Lightweight haptic cue where supported — iOS Safari ignores it, but
      // Android and desktop Chrome will vibrate briefly on mobile hardware.
      if (typeof navigator !== 'undefined' && 'vibrate' in navigator) {
        try {
          navigator.vibrate(30)
        } catch {
          // ignore — vibrate can throw on user-gesture-lacking contexts
        }
      }
    },
    {
      delay: options.delay ?? 500,
      distanceThreshold: 10,
      modifiers: { prevent: true },
    },
  )

  const shouldSuppressClick = (): boolean => {
    if (suppressNextClick.value) {
      suppressNextClick.value = false
      return true
    }
    return false
  }

  return { shouldSuppressClick }
}
