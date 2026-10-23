<script setup lang="ts">
/**
 * The content of an extension tab (spec 017, T046): a sandboxed frame with scripts, pointer
 * lock and fullscreen only — no `allow-same-origin`, popups, top navigation or forms (research R12) — served by the `holzi-ext`
 * scheme with the start token of its frame session. A development version (US12) comes from its
 * development server and shows its console output below the frame.
 */
import { ref } from 'vue'
import { useExtensionFrame } from '~/composables/useExtensionFrame'

const props = defineProps<{ extensionId: string }>()
const { t } = useI18n()

const iframe = ref<HTMLIFrameElement | null>(null)
const {
  state,
  error,
  src,
  dialog,
  dev,
  consoleLines,
  answerDialog,
  storageRequest,
  storageCredentialsPending,
  answerStorage,
  onLoad,
  reloadAsync,
} = useExtensionFrame(iframe, props.extensionId)
</script>

<template>
  <div class="flex h-full min-h-0 flex-col">
    <div class="relative min-h-0 flex-1">
      <iframe
        v-if="src"
        ref="iframe"
        :src="src"
        sandbox="allow-scripts allow-pointer-lock"
        allow="fullscreen"
        allowfullscreen
        data-testid="extension-frame"
        :data-extension-id="extensionId"
        referrerpolicy="no-referrer"
        class="h-full w-full border-0"
        :class="{ invisible: state !== 'ready' }"
        :inert="
          dialog !== null ||
          storageRequest !== null ||
          storageCredentialsPending ||
          undefined
        "
        :title="t('extensions.frame.title')"
        @load="onLoad"
      />
      <div
        v-if="state === 'loading'"
        class="absolute inset-0 flex items-center justify-center text-sm text-muted-foreground"
      >
        {{ t('extensions.frame.loading') }}
      </div>
      <ExtensionsFrameError
        v-if="state === 'error'"
        class="absolute inset-0"
        :message="error ?? t('extensions.frame.failed')"
        @reload="reloadAsync"
      />
      <ExtensionsFrameDialog
        v-if="dialog"
        class="absolute inset-0"
        :dialog="dialog"
        @answer="answerDialog"
      />
      <ExtensionsStorageDialog
        v-if="storageRequest"
        class="absolute inset-0"
        :request="storageRequest"
        @answer="answerStorage"
      />
    </div>
    <ExtensionsDevConsole v-if="dev" :lines="consoleLines" />
  </div>
</template>
