<template>
  <div class="flex flex-col items-center gap-3">
    <!-- Current avatar display -->
    <button
      type="button"
      class="relative group cursor-pointer"
      :title="t('change')"
      @click="openFilePicker"
    >
      <UiAvatar
        :src="modelValue"
        :seed="seed"
        :size="size"
      />
      <div class="absolute inset-0 rounded-full bg-black/50 opacity-0 group-hover:opacity-100 transition-opacity flex items-center justify-center">
        <UIcon name="i-lucide-camera" class="size-5 text-white" />
      </div>
    </button>

    <!-- Remove button -->
    <button
      v-if="modelValue"
      type="button"
      class="text-xs text-muted hover:text-error transition-colors"
      @click="$emit('update:modelValue', null)"
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

    <!-- Crop dialog -->
    <UiDrawerModal
      v-model:open="showCropDialog"
      :title="t('crop.title')"
    >
      <template #content>
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
import { Cropper, CircleStencil } from 'vue-advanced-cropper'
import 'vue-advanced-cropper/dist/style.css'
import { compressCanvasToBase64 } from '~/utils/imageCompression'

defineProps<{
  modelValue?: string | null
  seed?: string
  size?: 'xs' | 'sm' | 'md' | 'lg' | 'xl'
}>()

const emit = defineEmits<{
  'update:modelValue': [value: string | null]
}>()

const { t } = useI18n()

const fileInput = ref<HTMLInputElement>()
const cropperRef = ref<InstanceType<typeof Cropper>>()
const cropImageSrc = ref('')
const showCropDialog = ref(false)
const isCropping = ref(false)

function openFilePicker() {
  fileInput.value?.click()
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
    emit('update:modelValue', base64)
    showCropDialog.value = false
  } finally {
    isCropping.value = false
  }
}
</script>

<i18n lang="yaml">
de:
  change: Avatar ändern
  remove: Entfernen
  crop:
    title: Bild zuschneiden
    cancel: Abbrechen
    confirm: Übernehmen
en:
  change: Change avatar
  remove: Remove
  crop:
    title: Crop image
    cancel: Cancel
    confirm: Apply
</i18n>
