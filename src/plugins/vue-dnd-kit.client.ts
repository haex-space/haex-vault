import VueDndKitPlugin from '@vue-dnd-kit/core'

export default defineNuxtPlugin((nuxtApp) => {
  nuxtApp.vueApp.use(VueDndKitPlugin, {
    overlayPosition: {
      zIndex: 10000, // Higher than Nuxt UI Drawer (z-50 = 50)
    },
  })
})
