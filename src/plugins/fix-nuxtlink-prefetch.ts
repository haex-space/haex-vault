/**
 * Fixes Nuxt UI v3 bug where UButton/ULink pass both `noPrefetch` and `prefetch`
 * as boolean props to NuxtLink, triggering a console.warn on every render.
 *
 * Root cause: Vue sets unspecified Boolean props to `false` instead of `undefined`.
 * NuxtLink's checkPropConflicts warns when both are !== undefined.
 *
 * Fix: Patch NuxtLink's setup to convert `noPrefetch: false` to `undefined`
 * before the conflict check runs. This eliminates the false positive.
 */
export default defineNuxtPlugin((nuxtApp) => {
  nuxtApp.vueApp.mixin({
    beforeCreate() {
      // Only patch NuxtLink components (they have both noPrefetch and prefetch props)
      const props = this.$options.props as Record<string, unknown> | undefined
      if (!props || !('noPrefetch' in props) || !('prefetch' in props)) return

      const originalSetup = this.$options.setup
      if (!originalSetup) return

      this.$options.setup = (props: Record<string, unknown>, ctx: unknown) => {
        // Convert false noPrefetch to undefined to prevent the warning
        if (props.noPrefetch === false) {
          props.noPrefetch = undefined
        }
        return (originalSetup as Function)(props, ctx)
      }
    },
  })
})
