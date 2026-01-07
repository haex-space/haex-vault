<template>
  <div class="w-full h-full relative">
    <!-- Error overlay for dev extensions when server is not reachable -->
    <div
      v-if="extension?.devServerUrl && hasError"
      class="absolute inset-0 bg-white dark:bg-gray-900 flex items-center justify-center p-8"
    >
      <div class="max-w-md space-y-4 text-center">
        <UIcon
          name="i-heroicons-exclamation-circle"
          class="w-16 h-16 mx-auto text-yellow-500"
        />
        <h3 class="text-lg font-semibold">Dev Server Not Reachable</h3>
        <p class="text-sm opacity-70">
          The dev server at {{ extension.devServerUrl }} is not reachable.
        </p>
        <div
          class="bg-gray-100 dark:bg-gray-800 p-4 rounded text-left text-xs font-mono"
        >
          <p class="opacity-70 mb-2">To start the dev server:</p>
          <code class="block">cd /path/to/extension</code>
          <code class="block">npm run dev</code>
        </div>
        <UButton
          label="Retry"
          @click="retryLoad"
        />
      </div>
    </div>

    <!-- Loading Spinner -->
    <div
      v-if="isLoading"
      class="absolute inset-0 bg-white dark:bg-gray-900 flex items-center justify-center"
    >
      <div class="flex flex-col items-center gap-4">
        <div
          class="animate-spin rounded-full h-12 w-12 border-b-2 border-blue-500"
        />
        <p class="text-sm text-gray-600 dark:text-gray-400">
          Loading extension...
        </p>
      </div>
    </div>

    <iframe
      ref="iframeRef"
      :class="[
        'w-full h-full border-0 transition-all duration-1000 ease-out',
        isLoading ? 'opacity-0 scale-0' : 'opacity-100 scale-100',
      ]"
      :src="extensionUrl"
      :sandbox="sandboxAttributes"
      allow="autoplay; speaker-selection; encrypted-media;"
      @load="handleIframeLoad"
      @error="hasError = true"
    />
  </div>
</template>

<script setup lang="ts">
import {
  EXTENSION_PROTOCOL_PREFIX,
  EXTENSION_PROTOCOL_NAME,
} from '~/config/constants'

const props = defineProps<{
  extensionId: string
  windowId: string 
}>()

const extensionsStore = useExtensionsStore()
const { platform } = useDeviceStore()

const iframeRef = useTemplateRef('iframeRef')
const hasError = ref(false)
const isLoading = ref(true)

// Convert windowId to ref for reactive tracking
const windowIdRef = toRef(props, 'windowId')

const extension = computed(() => {
  return extensionsStore.availableExtensions.find(
    (ext) => ext.id === props.extensionId,
  )
})

const handleIframeLoad = () => {
  console.log('[ExtensionFrame] Iframe loaded successfully for:', extension.value?.name)

  // Try to inject a test script to see if JavaScript execution works
  try {
    if (iframeRef.value?.contentWindow) {
      console.log('[ExtensionFrame] Iframe has contentWindow access')
      // This will fail with sandboxed iframes without allow-same-origin
      console.log('[ExtensionFrame] Iframe origin:', iframeRef.value.contentWindow.location.href)
    } else {
      console.warn('[ExtensionFrame] Iframe contentWindow is null/undefined')
    }
  } catch (e) {
    console.warn('[ExtensionFrame] Cannot access iframe content (expected with sandbox):', e)
  }

  // Delay the fade-in slightly to allow window animation to mostly complete
  setTimeout(() => {
    isLoading.value = false
  }, 200)
}

const sandboxDefault = ['allow-scripts'] as const

const sandboxAttributes = computed(() => {
  return extension.value?.devServerUrl
    ? [...sandboxDefault, 'allow-same-origin'].join(' ')
    : sandboxDefault.join(' ')
})

// Generate extension URL
const extensionUrl = computed(() => {
  if (!extension.value) {
    return ''
  }

  const { publicKey, name, version, devServerUrl } = extension.value
  const assetPath = 'index.html'

  if (!publicKey || !name || !version) {
    return ''
  }

  // If dev server URL is provided, load directly from dev server
  if (devServerUrl) {
    const cleanUrl = devServerUrl.replace(/\/$/, '')
    const cleanPath = assetPath.replace(/^\//, '')
    return cleanPath ? `${cleanUrl}/${cleanPath}` : cleanUrl
  }

  const extensionInfo = {
    name,
    publicKey,
    version,
  }
  const encodedInfo = btoa(JSON.stringify(extensionInfo))

  if (platform === 'android' || platform === 'windows') {
    // Android: Tauri uses http://{scheme}.localhost format
    return `http://${EXTENSION_PROTOCOL_NAME}.localhost/${encodedInfo}/${assetPath}`
  } else {
    // Desktop: Use custom protocol with base64 as host
    return `${EXTENSION_PROTOCOL_PREFIX}${encodedInfo}/${assetPath}`
  }
})

const retryLoad = () => {
  hasError.value = false
  if (iframeRef.value) {
    //iframeRef.value.src = iframeRef.value.src // Force reload
  }
}

// Initialize extension message handler to set up context
useExtensionMessageHandler(iframeRef, extension, windowIdRef)

onMounted(() => {
  console.log(
    '[ExtensionFrame] Component MOUNTED',
    extension.value?.name,
    'windowId:',
    props.windowId,
  )
})

// Explicit cleanup before unmount
onBeforeUnmount(() => {
  console.log('[ExtensionFrame] Component UNMOUNTING', {
    extensionId: props.extensionId,
    windowId: props.windowId,
    hasIframe: !!iframeRef.value,
  })
  if (iframeRef.value) {
    unregisterExtensionIFrame(iframeRef.value)
  }
})
</script>
