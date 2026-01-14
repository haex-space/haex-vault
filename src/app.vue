<template>
  <UApp :locale="locales[locale]">
    <div data-vaul-drawer-wrapper>
      <NuxtPage />
    </div>

    <!-- Global Permission Prompt Dialog -->
    <HaexExtensionDialogPermissionPrompt
      :open="permissionPrompt.isOpen.value"
      :prompt-data="permissionPrompt.promptData.value"
      :pending-count="permissionPrompt.pendingCount.value"
      @update:open="(v) => !v && permissionPrompt.cancelPrompt()"
      @decision="permissionPrompt.handleDecision"
    />

    <!-- External Client Authorization Dialog -->
    <HaexExtensionDialogExternalAuth
      v-model:open="externalAuthOpen"
      :pending-auth="externalAuth.pendingAuth.value"
      @decision="externalAuth.handleDecision"
    />
  </UApp>
</template>

<script setup lang="ts">
import * as locales from '@nuxt/ui/locale'
import { setDebugEnabled, setModuleDebug } from '~/stores/logging'

const { locale } = useI18n()

// Enable debug logging for troubleshooting E2E tests
// TODO: Remove after fixing nightly build issues
setDebugEnabled(true)
setModuleDebug('WINDOW_MGR', true)
setModuleDebug('BROADCAST', true)
setModuleDebug('EXT_BRIDGE', true)

// Handle Android back button
useAndroidBackButton()

// Initialize deep-link handler (desktop only)
const deepLink = useDeepLink()
onMounted(() => {
  deepLink.init()
})

// Global permission prompt handler
const permissionPrompt = usePermissionPrompt()
onMounted(() => {
  permissionPrompt.init()
})

// External client authorization handler
const externalAuth = useExternalAuth()
const externalAuthOpen = computed({
  get: () => externalAuth.isOpen.value,
  set: (v) => {
    if (!v) externalAuth.cancelPrompt()
  },
})
onMounted(() => {
  externalAuth.init()
})
</script>

<style>
.fade-enter-active {
  transition: all 1s ease-out;
}

.fade-leave-active {
  transition: all 1s ease-out reverse;
}

.fade-enter-from,
.fade-leave-to {
  opacity: 0;
}
</style>
