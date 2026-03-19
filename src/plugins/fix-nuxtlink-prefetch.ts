/**
 * Suppresses the false-positive NuxtLink warning:
 * "`noPrefetch` and `prefetch` cannot be used together"
 *
 * Root cause: Vue sets unspecified Boolean props to `false` instead of `undefined`.
 * NuxtLink's checkPropConflicts warns when both are !== undefined, but `false`
 * is not the same as "explicitly set by the user" — it's Vue's default.
 *
 * This is a known Nuxt UI v3 issue where UButton/ULink forward both props.
 * The warning is harmless (prefetch is correctly ignored when noPrefetch is set).
 *
 * Fix: Filter the specific warning message from console.warn.
 */
export default defineNuxtPlugin(() => {
  if (!import.meta.dev && !import.meta.server) return

  const originalWarn = console.warn
  console.warn = (...args: unknown[]) => {
    if (typeof args[0] === 'string' && args[0].includes('cannot be used together')) return
    originalWarn.apply(console, args)
  }
})
