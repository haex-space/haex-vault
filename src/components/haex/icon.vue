<template>
  <div class="inline-flex">
    <UTooltip :text="tooltip">
      <!-- Bundled Icon (iconify) - also show when fallback is active -->
      <UIcon
        v-if="isBundledIcon || showFallback"
        :name="name"
        v-bind="$attrs"
      />

      <!-- External Image (Extension icon) -->
      <img
        v-else
        :src="imageUrl"
        v-bind="$attrs"
        @error="handleImageError"
      />
    </UTooltip>
  </div>
</template>

<script setup lang="ts">
import { convertFileSrc } from '@tauri-apps/api/core'

defineOptions({
  inheritAttrs: false,
})

const props = defineProps<{
  name: string
  tooltip?: string
}>()

// Check if it's a bundled icon (iconify icon name, no file extension or URL)
const isBundledIcon = computed(() => {
  // If it starts with http:// or https://, it's a URL (not bundled)
  if (props.name.startsWith('http://') || props.name.startsWith('https://')) {
    return false
  }
  // If it has a file extension, it's an external image (not bundled)
  return !props.name.match(/\.(png|jpg|jpeg|svg|gif|webp|ico)$/i)
})

// Check if it's already a URL (no conversion needed)
const isUrl = computed(() => {
  return props.name.startsWith('http://') || props.name.startsWith('https://')
})

// Convert file path to Tauri URL for images
const imageUrl = ref('')
const showFallback = ref(false)

// Default fallback icon
const FALLBACK_ICON = 'i-heroicons-puzzle-piece-solid'

watchEffect(() => {
  if (!isBundledIcon.value && !showFallback.value) {
    if (isUrl.value) {
      // Already a URL, use directly
      imageUrl.value = props.name
    } else {
      // Convert local file path to Tauri asset URL
      imageUrl.value = convertFileSrc(props.name)
    }
  }
})

const handleImageError = () => {
  console.warn(`Failed to load icon: ${props.name}`)
  showFallback.value = true
}

// Use fallback icon if image failed to load
const name = computed(() => {
  if (showFallback.value) {
    return FALLBACK_ICON
  }
  return props.name
})
</script>
