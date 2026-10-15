import { readonly, ref } from 'vue'
import { onColorSchemeApplied } from '~/composables/useColorScheme'
import type { Scheme } from '@bindings/Scheme'

/** Only the class `dark` on `<html>` tells which scheme applies (`useColorScheme`). */
function applied(): Scheme {
  return document.documentElement.classList.contains('dark') ? 'dark' : 'light'
}

const scheme = ref<Scheme>('dark')
let listening = false

/**
 * The scheme that applies right now, light or dark, also while the vault's choice is `system`
 * (spec 053, FR-006). It follows `onColorSchemeApplied`, so a change of the system's scheme reaches
 * it as well.
 */
export function useAppliedScheme() {
  if (!listening) {
    listening = true
    scheme.value = applied()
    onColorSchemeApplied(() => {
      scheme.value = applied()
    })
  }
  return readonly(scheme)
}
