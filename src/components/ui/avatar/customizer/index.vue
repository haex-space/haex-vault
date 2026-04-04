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
import type { ToonHeadOptions } from './toon-head.vue'
import type { BotttsOptions } from './bottts.vue'

export type AvatarOptions = ToonHeadOptions | BotttsOptions

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

const toonHeadOptions = ref<ToonHeadOptions>(getDefaultToonHeadOptions())
const botttsOptions = ref<BotttsOptions>(getDefaultBotttsOptions())

// Initialize options when modal opens
watch(open, (isOpen) => {
  if (!isOpen) return

  if (props.initialOptions?.style === 'toon-head') {
    toonHeadOptions.value = { ...getDefaultToonHeadOptions(), ...props.initialOptions } as ToonHeadOptions
  } else if (props.initialOptions?.style === 'bottts') {
    botttsOptions.value = { ...getDefaultBotttsOptions(), ...props.initialOptions } as BotttsOptions
  } else if (props.avatarStyle === 'toon-head') {
    toonHeadOptions.value = getDefaultToonHeadOptions()
  } else {
    botttsOptions.value = getDefaultBotttsOptions()
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
// TODO: Implement randomize function
function randomize() {
  if (props.avatarStyle === 'toon-head') {
    toonHeadOptions.value = randomizeToonHead()
  } else {
    botttsOptions.value = randomizeBottts()
  }
}

function pick<T>(arr: readonly T[]): T {
  return arr[Math.floor(Math.random() * arr.length)]!
}

function randomizeToonHead(): ToonHeadOptions {
  return {
    style: 'toon-head',
    eyes: pick(['happy', 'wide', 'bow', 'humble', 'wink'] as const),
    eyebrows: pick(['raised', 'angry', 'happy', 'sad', 'neutral'] as const),
    mouth: pick(['laugh', 'angry', 'agape', 'smile', 'sad'] as const),
    beard: pick(['moustacheTwirl', 'fullBeard', 'chin', 'chinMoustache', 'longBeard'] as const),
    beardProbability: Math.random() < 0.5 ? 100 : 0,
    hair: pick(['sideComed', 'undercut', 'spiky', 'bun'] as const),
    rearHair: pick(['longStraight', 'longWavy', 'shoulderHigh', 'neckHigh'] as const),
    rearHairProbability: Math.random() < 0.5 ? 100 : 0,
    clothes: pick(['turtleNeck', 'openJacket', 'dress', 'shirt', 'tShirt'] as const),
    skinColor: pick(['f1c3a5', 'c68e7a', 'b98e6a', 'a36b4f', '5c3829'] as const),
    hairColor: pick(['2c1b18', 'a55728', 'b58143', 'd6b370', '724133', 'e8e1e1'] as const),
    clothesColor: pick(['545454', 'b11f1f', '0b3286', '147f3c', 'eab308', '731ac3', 'ec4899', 'f97316', '151613', 'e8e9e6'] as const),
  }
}

function randomizeBottts(): BotttsOptions {
  return {
    style: 'bottts',
    face: pick(['round01', 'round02', 'square01', 'square02', 'square03', 'square04'] as const),
    eyes: pick(['bulging', 'dizzy', 'eva', 'frame1', 'frame2', 'glow', 'happy', 'hearts', 'robocop', 'round', 'roundFrame01', 'roundFrame02', 'sensor', 'shade01'] as const),
    mouth: pick(['bite', 'diagram', 'grill01', 'grill02', 'grill03', 'smile01', 'smile02', 'square01', 'square02'] as const),
    mouthProbability: Math.random() < 0.5 ? 100 : 0,
    top: pick(['antenna', 'antennaCrooked', 'bulb01', 'glowingBulb01', 'glowingBulb02', 'horns', 'lights', 'pyramid', 'radar'] as const),
    topProbability: Math.random() < 0.5 ? 100 : 0,
    sides: pick(['antenna01', 'antenna02', 'cables01', 'cables02', 'round', 'square', 'squareAssymetric'] as const),
    sidesProbability: Math.random() < 0.5 ? 100 : 0,
    texture: pick(['camo01', 'camo02', 'circuits', 'dirty01', 'dirty02', 'dots', 'grunge01', 'grunge02'] as const),
    textureProbability: Math.random() < 0.5 ? 100 : 0,
    baseColor: pick(['ffb300', '1e88e5', '546e7a', '6d4c41', '00acc1', 'f4511e', '5e35b1', '43a047', '757575', '3949ab', '039be5', '7cb342', 'c0ca33', 'fb8c00', 'd81b60', '8e24aa', 'e53935', '00897b', 'fdd835'] as const),
  }
}

// --- Defaults ---

function getDefaultToonHeadOptions(): ToonHeadOptions {
  return {
    style: 'toon-head',
    eyes: 'happy',
    eyebrows: 'neutral',
    mouth: 'smile',
    beard: 'fullBeard',
    beardProbability: 0,
    hair: 'sideComed',
    rearHair: 'longStraight',
    rearHairProbability: 0,
    clothes: 'tShirt',
    skinColor: 'f1c3a5',
    hairColor: '2c1b18',
    clothesColor: '0b3286',
  }
}

function getDefaultBotttsOptions(): BotttsOptions {
  return {
    style: 'bottts',
    face: 'round01',
    eyes: 'round',
    mouth: 'smile01',
    mouthProbability: 100,
    top: 'antenna',
    topProbability: 100,
    sides: 'round',
    sidesProbability: 100,
    texture: 'circuits',
    textureProbability: 0,
    baseColor: '1e88e5',
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
