/**
 * Vue directive that propagates data-testid to the first interactive element (button, input, etc.)
 * This solves the problem that Nuxt UI components don't forward data-* attributes to DOM elements.
 *
 * Usage:
 *   <UButton v-testid="'my-button'" ... />
 *
 * This will find the first button/input/a element inside the component and set data-testid on it.
 */
import type { Directive, DirectiveBinding } from 'vue'

const interactiveSelectors = 'button, input, textarea, select, a, [role="button"]'

const vTestid: Directive<HTMLElement, string> = {
  mounted(el: HTMLElement, binding: DirectiveBinding<string>) {
    if (!binding.value) return

    // If the element itself is interactive, set the attribute directly
    if (el.matches(interactiveSelectors)) {
      el.setAttribute('data-testid', binding.value)
      return
    }

    // Otherwise, find the first interactive element inside
    const interactive = el.querySelector(interactiveSelectors)
    if (interactive) {
      interactive.setAttribute('data-testid', binding.value)
    } else {
      // Fallback: set on the element itself
      el.setAttribute('data-testid', binding.value)
    }
  },
  updated(el: HTMLElement, binding: DirectiveBinding<string>) {
    if (!binding.value) return

    // Handle dynamic value changes
    if (el.matches(interactiveSelectors)) {
      el.setAttribute('data-testid', binding.value)
      return
    }

    const interactive = el.querySelector(interactiveSelectors)
    if (interactive) {
      interactive.setAttribute('data-testid', binding.value)
    } else {
      el.setAttribute('data-testid', binding.value)
    }
  },
}

export default defineNuxtPlugin((nuxtApp) => {
  nuxtApp.vueApp.directive('testid', vTestid)
})
