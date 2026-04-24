<template>
  <UiDrawerModal
    v-model:open="open"
    :title="t('title')"
    :ui="{ content: 'sm:max-w-lg' }"
  >
    <template #body>
      <div class="space-y-4">
        <!-- Preview + Randomize -->
        <div class="flex flex-col items-center gap-3">
          <div class="size-24 rounded-full overflow-hidden bg-muted [&>svg]:w-full [&>svg]:h-full">
            <div v-html="previewSvg" />
          </div>
          <UiButton
            variant="ghost"
            color="neutral"
            icon="i-lucide-dice-5"
            size="sm"
            @click="randomize"
          >
            {{ t('randomize') }}
          </UiButton>
        </div>

        <!-- Style-specific options -->
        <UiAvatarCustomizerToonHead
          v-if="avatarStyle === 'toon-head'"
          v-model:options="toonHeadOptions"
        />
        <UiAvatarCustomizerBottts
          v-else
          v-model:options="botttsOptions"
        />
      </div>
    </template>

    <template #footer>
      <div class="flex items-center justify-between gap-2 w-full">
        <UiButton
          variant="ghost"
          color="neutral"
          icon="i-lucide-image-up"
          @click="$emit('uploadImage')"
        >
          {{ t('uploadImage') }}
        </UiButton>
        <div class="flex gap-2">
          <UiButton
            variant="ghost"
            color="neutral"
            @click="open = false"
          >
            {{ t('cancel') }}
          </UiButton>
          <UiButton
            color="primary"
            icon="i-lucide-check"
            @click="onConfirm"
          >
            {{ t('confirm') }}
          </UiButton>
        </div>
      </div>
    </template>
  </UiDrawerModal>
</template>

<script setup lang="ts">
import { createAvatar } from '@dicebear/core'
import * as toonHead from '@dicebear/toon-head'
import * as bottts from '@dicebear/bottts'
import {
  type ToonHeadOptions,
  type BotttsOptions,
  type AvatarOptions,
  defaultToonHeadOptions,
  defaultBotttsOptions,
  randomToonHeadOptions,
  randomBotttsOptions,
} from '~/utils/identityAvatar'

// (AvatarOptions is available globally via Nuxt auto-imports from
// ~/utils/identityAvatar — no re-export needed and removing it prevents
// the auto-import duplicate-symbol warning.)

const props = defineProps<{
  avatarStyle: 'toon-head' | 'bottts'
  seed?: string
  initialOptions?: Record<string, unknown> | null
}>()

const emit = defineEmits<{
  confirm: [options: AvatarOptions]
  uploadImage: []
}>()

const open = defineModel<boolean>('open', { default: false })

const { t } = useI18n()

const toonHeadOptions = ref<ToonHeadOptions>(defaultToonHeadOptions())
const botttsOptions = ref<BotttsOptions>(defaultBotttsOptions())

// Initialize options when modal opens. We merge `initialOptions` over the
// type's defaults so a partially-populated record (e.g. an old row that
// only stored `{ style, seed }`) still has every field set — DiceBear
// otherwise samples missing fields from the seed and produces a different
// SVG than what we render in the list view.
watch(open, (isOpen) => {
  if (!isOpen) return

  if (props.initialOptions?.style === 'toon-head') {
    toonHeadOptions.value = { ...defaultToonHeadOptions(), ...props.initialOptions } as ToonHeadOptions
  } else if (props.initialOptions?.style === 'bottts') {
    botttsOptions.value = { ...defaultBotttsOptions(), ...props.initialOptions } as BotttsOptions
  } else if (props.avatarStyle === 'toon-head') {
    toonHeadOptions.value = defaultToonHeadOptions()
  } else {
    botttsOptions.value = defaultBotttsOptions()
  }
})

const currentOptions = computed<AvatarOptions>(() =>
  props.avatarStyle === 'toon-head' ? toonHeadOptions.value : botttsOptions.value,
)

// Live preview SVG
const previewSvg = computed(() => {
  const opts = currentOptions.value

  // Build DiceBear-compatible options (values wrapped in arrays)
  const diceBearOptions: Record<string, unknown> = { seed: props.seed }
  for (const [key, value] of Object.entries(opts)) {
    if (key === 'style') continue
    diceBearOptions[key] = typeof value === 'string' && !key.endsWith('Probability')
      ? [value]
      : value
  }

  if (opts.style === 'toon-head') {
    return createAvatar(toonHead, diceBearOptions).toString()
  }
  return createAvatar(bottts, diceBearOptions).toString()
})

function onConfirm() {
  emit('confirm', { ...currentOptions.value })
  open.value = false
}

// --- Randomize ---
function randomize() {
  if (props.avatarStyle === 'toon-head') {
    toonHeadOptions.value = randomToonHeadOptions()
  } else {
    botttsOptions.value = randomBotttsOptions()
  }
}
</script>

<i18n lang="yaml">
de:
  title: Avatar gestalten
  randomize: Zufällig
  uploadImage: Eigenes Bild
  cancel: Abbrechen
  confirm: Übernehmen
en:
  title: Customize Avatar
  randomize: Randomize
  uploadImage: Upload Image
  cancel: Cancel
  confirm: Apply
</i18n>
