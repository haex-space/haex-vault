/**
 * Fixes Nuxt UI v3 bug where UButton/ULink pass both `noPrefetch` and `prefetch`
 * as boolean props to NuxtLink, triggering a console.warn on every render.
 *
 * Root cause: Vue sets unspecified Boolean props to `false` instead of `undefined`.
 * NuxtLink's checkPropConflicts warns when both are !== undefined.
 *
 * Fix: Patch NuxtLink to not warn on this specific known false-positive.
 */
export default defineNuxtPlugin(() => {
  if (!import.meta.dev) return

  const originalWarn = console.warn
  console.warn = (...args: unknown[]) => {
    if (typeof args[0] === 'string' && args[0].includes('`noPrefetch` and `prefetch` cannot be used together')) {
      return
    }
    originalWarn.apply(console, args)
  }
})
