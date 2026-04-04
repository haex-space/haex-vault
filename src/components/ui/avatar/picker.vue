<template>
  <div class="flex flex-col items-center gap-3">
    <!-- Current avatar display -->
    <button
      type="button"
      class="relative group cursor-pointer"
      :title="t('change')"
      @click="showCustomizer = true"
    >
      <UiAvatar
        :src="modelValue"
        :seed="seed"
        :size="size"
        :avatar-style="avatarStyle"
        :avatar-options="avatarOptions"
      />
      <div class="absolute inset-0 rounded-full bg-black/50 opacity-0 group-hover:opacity-100 transition-opacity flex items-center justify-center">
        <UIcon name="i-lucide-pencil" class="size-5 text-white" />
      </div>
    </button>

    <!-- Remove button -->
    <button
      v-if="modelValue || avatarOptions"
      type="button"
      class="text-xs text-muted hover:text-error transition-colors"
      @click="onRemove"
    >
      {{ t('remove') }}
    </button>

    <!-- Hidden file input -->
    <input
      ref="fileInput"
      type="file"
      accept="image/*"
      class="hidden"
      @change="onFileSelected"
    >

    <!-- Customizer modal -->
    <UiAvatarCustomizer
      v-model:open="showCustomizer"
      :avatar-style="avatarStyle ?? 'bottts'"
      :seed="seed"
      :initial-options="avatarOptions"
      @confirm="onCustomizerConfirmAsync"
      @upload-image="openFilePicker"
    />

    <!-- Crop dialog -->
    <UiDrawerModal
      v-model:open="showCropDialog"
      :title="t('crop.title')"
    >
      <template #body>
        <Cropper
          ref="cropperRef"
          :src="cropImageSrc"
          :stencil-props="{ aspectRatio: 1 }"
          :stencil-component="CircleStencil"
          class="max-h-[50vh]"
        />
      </template>
      <template #footer>
        <div class="flex justify-end gap-2 w-full">
          <UiButton
            variant="ghost"
            color="neutral"
            @click="showCropDialog = false"
          >
            {{ t('crop.cancel') }}
          </UiButton>
          <UiButton
            color="primary"
            :loading="isCropping"
            @click="onCropConfirm"
          >
            {{ t('crop.confirm') }}
          </UiButton>
        </div>
      </template>
    </UiDrawerModal>
  </div>
</template>

<script setup lang="ts">
import { createAvatar } from '@dicebear/core'
import * as toonHead from '@dicebear/toon-head'
import * as bottts from '@dicebear/bottts'
import { Cropper, CircleStencil } from 'vue-advanced-cropper'
import 'vue-advanced-cropper/dist/style.css'
import { compressCanvasToBase64, compressSvgToBase64 } from '~/utils/imageCompression'
import type { AvatarOptions } from './customizer/index.vue'

defineProps<{
  modelValue?: string | null
  avatarOptions?: Record<string, unknown> | null
  seed?: string
  size?: 'xs' | 'sm' | 'md' | 'lg' | 'xl'
  avatarStyle?: 'bottts' | 'toon-head'
}>()

const emit = defineEmits<{
  'update:modelValue': [value: string | null]
  'update:avatarOptions': [value: Record<string, unknown> | null]
}>()

const { t } = useI18n()

const fileInput = ref<HTMLInputElement>()
const cropperRef = ref<InstanceType<typeof Cropper>>()
const cropImageSrc = ref('')
const showCustomizer = ref(false)
const showCropDialog = ref(false)
const isCropping = ref(false)

function openFilePicker() {
  showCustomizer.value = false
  fileInput.value?.click()
}

function onRemove() {
  emit('update:avatarOptions', null)
  emit('update:modelValue', null)
}

async function onCustomizerConfirmAsync(options: AvatarOptions) {
  const svgString = renderAvatarSvg(options)
  const base64 = await compressSvgToBase64(svgString)

  emit('update:avatarOptions', { ...options })
  emit('update:modelValue', base64)
}

function renderAvatarSvg(options: AvatarOptions): string {
  // Build DiceBear options with arrays
  const diceBearOptions: Record<string, unknown> = {}
  for (const [key, value] of Object.entries(options)) {
    if (key === 'style') continue
    diceBearOptions[key] = typeof value === 'string' && !key.endsWith('Probability')
      ? [value]
      : value
  }

  if (options.style === 'toon-head') {
    return createAvatar(toonHead, diceBearOptions).toString()
  }
  return createAvatar(bottts, diceBearOptions).toString()
}

function onFileSelected(event: Event) {
  const input = event.target as HTMLInputElement
  const file = input.files?.[0]
  if (!file) return

  const reader = new FileReader()
  reader.onload = (e) => {
    cropImageSrc.value = e.target?.result as string
    showCropDialog.value = true
  }
  reader.readAsDataURL(file)

  // Reset input so same file can be selected again
  input.value = ''
}

async function onCropConfirm() {
  if (!cropperRef.value) return

  isCropping.value = true
  try {
    const { canvas } = cropperRef.value.getResult()
    if (!canvas) return

    // Resize to 128x128 before compression
    const resized = document.createElement('canvas')
    resized.width = 128
    resized.height = 128
    const ctx = resized.getContext('2d')!
    ctx.drawImage(canvas, 0, 0, 128, 128)

    const base64 = await compressCanvasToBase64(resized)
    // Upload overrides customizer options
    emit('update:avatarOptions', null)
    emit('update:modelValue', base64)
    showCropDialog.value = false
  } finally {
    isCropping.value = false
  }
}
</script>

<i18n lang="yaml">
de:
  change: Avatar anpassen
  remove: Zurücksetzen
  crop:
    title: Bild zuschneiden
    cancel: Abbrechen
    confirm: Übernehmen
en:
  change: Customize avatar
  remove: Reset
  crop:
    title: Crop image
    cancel: Cancel
    confirm: Apply
</i18n>
