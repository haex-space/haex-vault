<template>
  <UTabs
    v-model="activeTab"
    :items="tabItems"
    class="w-full"
  />

  <div class="mt-4 space-y-4 overflow-y-auto max-h-[40vh]">
    <!-- Face tab -->
    <template v-if="activeTab === 'face'">
      <!-- Eyes -->
      <div class="space-y-2">
        <span class="text-sm font-medium">{{ t('eyes') }}</span>
        <div class="grid grid-cols-5 gap-2">
          <button
            v-for="option in eyesOptions"
            :key="option"
            type="button"
            class="rounded-lg border-2 p-1 transition-colors hover:border-primary"
            :class="options.eyes === option ? 'border-primary bg-primary/10' : 'border-default'"
            @click="updateOption('eyes', option)"
          >
            <div class="w-full aspect-square [&>svg]:w-full [&>svg]:h-full" v-html="previewSvg('eyes', option)" />
          </button>
        </div>
      </div>

      <!-- Eyebrows -->
      <div class="space-y-2">
        <span class="text-sm font-medium">{{ t('eyebrows') }}</span>
        <div class="grid grid-cols-5 gap-2">
          <button
            v-for="option in eyebrowsOptions"
            :key="option"
            type="button"
            class="rounded-lg border-2 p-1 transition-colors hover:border-primary"
            :class="options.eyebrows === option ? 'border-primary bg-primary/10' : 'border-default'"
            @click="updateOption('eyebrows', option)"
          >
            <div class="w-full aspect-square [&>svg]:w-full [&>svg]:h-full" v-html="previewSvg('eyebrows', option)" />
          </button>
        </div>
      </div>

      <!-- Mouth -->
      <div class="space-y-2">
        <span class="text-sm font-medium">{{ t('mouth') }}</span>
        <div class="grid grid-cols-5 gap-2">
          <button
            v-for="option in mouthOptions"
            :key="option"
            type="button"
            class="rounded-lg border-2 p-1 transition-colors hover:border-primary"
            :class="options.mouth === option ? 'border-primary bg-primary/10' : 'border-default'"
            @click="updateOption('mouth', option)"
          >
            <div class="w-full aspect-square [&>svg]:w-full [&>svg]:h-full" v-html="previewSvg('mouth', option)" />
          </button>
        </div>
      </div>

      <!-- Beard -->
      <div class="space-y-2">
        <div class="flex items-center justify-between">
          <span class="text-sm font-medium">{{ t('beard') }}</span>
          <button
            type="button"
            class="text-xs text-muted hover:text-primary transition-colors"
            @click="toggleBeard"
          >
            {{ options.beardProbability ? t('hide') : t('show') }}
          </button>
        </div>
        <div v-if="options.beardProbability" class="grid grid-cols-5 gap-2">
          <button
            v-for="option in beardOptions"
            :key="option"
            type="button"
            class="rounded-lg border-2 p-1 transition-colors hover:border-primary"
            :class="options.beard === option ? 'border-primary bg-primary/10' : 'border-default'"
            @click="updateOption('beard', option)"
          >
            <div class="w-full aspect-square [&>svg]:w-full [&>svg]:h-full" v-html="previewSvg('beard', option)" />
          </button>
        </div>
      </div>
    </template>

    <!-- Hair tab -->
    <template v-if="activeTab === 'hair'">
      <!-- Front hair -->
      <div class="space-y-2">
        <span class="text-sm font-medium">{{ t('hair') }}</span>
        <div class="grid grid-cols-4 gap-2">
          <button
            v-for="option in hairOptions"
            :key="option"
            type="button"
            class="rounded-lg border-2 p-1 transition-colors hover:border-primary"
            :class="options.hair === option ? 'border-primary bg-primary/10' : 'border-default'"
            @click="updateOption('hair', option)"
          >
            <div class="w-full aspect-square [&>svg]:w-full [&>svg]:h-full" v-html="previewSvg('hair', option)" />
          </button>
        </div>
      </div>

      <!-- Rear hair -->
      <div class="space-y-2">
        <div class="flex items-center justify-between">
          <span class="text-sm font-medium">{{ t('rearHair') }}</span>
          <button
            type="button"
            class="text-xs text-muted hover:text-primary transition-colors"
            @click="toggleRearHair"
          >
            {{ options.rearHairProbability ? t('hide') : t('show') }}
          </button>
        </div>
        <div v-if="options.rearHairProbability" class="grid grid-cols-4 gap-2">
          <button
            v-for="option in rearHairOptions"
            :key="option"
            type="button"
            class="rounded-lg border-2 p-1 transition-colors hover:border-primary"
            :class="options.rearHair === option ? 'border-primary bg-primary/10' : 'border-default'"
            @click="updateOption('rearHair', option)"
          >
            <div class="w-full aspect-square [&>svg]:w-full [&>svg]:h-full" v-html="previewSvg('rearHair', option)" />
          </button>
        </div>
      </div>
    </template>

    <!-- Clothes tab -->
    <template v-if="activeTab === 'clothes'">
      <div class="space-y-2">
        <span class="text-sm font-medium">{{ t('clothes') }}</span>
        <div class="grid grid-cols-5 gap-2">
          <button
            v-for="option in clothesOptions"
            :key="option"
            type="button"
            class="rounded-lg border-2 p-1 transition-colors hover:border-primary"
            :class="options.clothes === option ? 'border-primary bg-primary/10' : 'border-default'"
            @click="updateOption('clothes', option)"
          >
            <div class="w-full aspect-square [&>svg]:w-full [&>svg]:h-full" v-html="previewSvg('clothes', option)" />
          </button>
        </div>
      </div>
    </template>

    <!-- Colors tab -->
    <template v-if="activeTab === 'colors'">
      <!-- Skin color -->
      <div class="space-y-2">
        <span class="text-sm font-medium">{{ t('skinColor') }}</span>
        <div class="flex flex-wrap gap-2">
          <button
            v-for="color in skinColors"
            :key="color"
            type="button"
            class="size-8 rounded-full border-2 transition-all hover:scale-110"
            :class="options.skinColor === color ? 'border-primary ring-2 ring-primary/30' : 'border-default'"
            :style="{ backgroundColor: `#${color}` }"
            @click="updateOption('skinColor', color)"
          />
        </div>
      </div>

      <!-- Hair color -->
      <div class="space-y-2">
        <span class="text-sm font-medium">{{ t('hairColor') }}</span>
        <div class="flex flex-wrap gap-2">
          <button
            v-for="color in hairColors"
            :key="color"
            type="button"
            class="size-8 rounded-full border-2 transition-all hover:scale-110"
            :class="options.hairColor === color ? 'border-primary ring-2 ring-primary/30' : 'border-default'"
            :style="{ backgroundColor: `#${color}` }"
            @click="updateOption('hairColor', color)"
          />
        </div>
      </div>

      <!-- Clothes color -->
      <div class="space-y-2">
        <span class="text-sm font-medium">{{ t('clothesColor') }}</span>
        <div class="flex flex-wrap gap-2">
          <button
            v-for="color in clothesColors"
            :key="color"
            type="button"
            class="size-8 rounded-full border-2 transition-all hover:scale-110"
            :class="options.clothesColor === color ? 'border-primary ring-2 ring-primary/30' : 'border-default'"
            :style="{ backgroundColor: `#${color}` }"
            @click="updateOption('clothesColor', color)"
          />
        </div>
      </div>
    </template>
  </div>
</template>

<script setup lang="ts">
import { createAvatar } from '@dicebear/core'
import * as toonHead from '@dicebear/toon-head'
import type { ToonHeadOptions } from '~/utils/identityAvatar'

const options = defineModel<ToonHeadOptions>('options', { required: true })

const { t } = useI18n()

const activeTab = ref('face')
const tabItems = computed(() => [
  { label: t('tabs.face'), value: 'face' },
  { label: t('tabs.hair'), value: 'hair' },
  { label: t('tabs.clothes'), value: 'clothes' },
  { label: t('tabs.colors'), value: 'colors' },
])

// Available options
const eyesOptions = ['happy', 'wide', 'bow', 'humble', 'wink'] as const
const eyebrowsOptions = ['raised', 'angry', 'happy', 'sad', 'neutral'] as const
const mouthOptions = ['laugh', 'angry', 'agape', 'smile', 'sad'] as const
const beardOptions = ['moustacheTwirl', 'fullBeard', 'chin', 'chinMoustache', 'longBeard'] as const
const hairOptions = ['sideComed', 'undercut', 'spiky', 'bun'] as const
const rearHairOptions = ['longStraight', 'longWavy', 'shoulderHigh', 'neckHigh'] as const
const clothesOptions = ['turtleNeck', 'openJacket', 'dress', 'shirt', 'tShirt'] as const

const skinColors = ['f1c3a5', 'c68e7a', 'b98e6a', 'a36b4f', '5c3829']
const hairColors = ['2c1b18', 'a55728', 'b58143', 'd6b370', '724133', 'e8e1e1']
const clothesColors = ['545454', 'b11f1f', '0b3286', '147f3c', 'eab308', '731ac3', 'ec4899', 'f97316', '151613', 'e8e9e6']

function updateOption(key: keyof ToonHeadOptions, value: string | number) {
  options.value = { ...options.value, [key]: value }
}

function toggleBeard() {
  options.value = {
    ...options.value,
    beardProbability: options.value.beardProbability ? 0 : 100,
  }
}

function toggleRearHair() {
  options.value = {
    ...options.value,
    rearHairProbability: options.value.rearHairProbability ? 0 : 100,
  }
}

// Mini-preview: render full avatar with one option varied
const previewCache = new Map<string, string>()

function previewSvg(key: string, value: string): string {
  const cacheKey = `${key}:${value}:${options.value.skinColor}:${options.value.hairColor}:${options.value.clothesColor}`
  const cached = previewCache.get(cacheKey)
  if (cached) return cached

  const o = options.value
  const pick = (k: string) => k === key ? value : (o as unknown as Record<string, unknown>)[k]

  const previewOptions: Record<string, unknown> = {
    // Force-show the feature for preview
    beardProbability: key === 'beard' ? 100 : o.beardProbability,
    rearHairProbability: key === 'rearHair' ? 100 : o.rearHairProbability,
    hairProbability: 100,
    // Wrap values into arrays for DiceBear
    eyes: [pick('eyes')],
    eyebrows: [pick('eyebrows')],
    mouth: [pick('mouth')],
    beard: [pick('beard')],
    hair: [pick('hair')],
    rearHair: [pick('rearHair')],
    clothes: [pick('clothes')],
    skinColor: [pick('skinColor')],
    hairColor: [pick('hairColor')],
    clothesColor: [pick('clothesColor')],
  }

  const svg = createAvatar(toonHead, previewOptions).toString()
  previewCache.set(cacheKey, svg)
  return svg
}

// Clear cache when colors change (affects all previews)
watch(() => [options.value.skinColor, options.value.hairColor, options.value.clothesColor], () => {
  previewCache.clear()
})
</script>

<i18n lang="yaml">
de:
  tabs:
    face: Gesicht
    hair: Haare
    clothes: Kleidung
    colors: Farben
  eyes: Augen
  eyebrows: Augenbrauen
  mouth: Mund
  beard: Bart
  hair: Frisur
  rearHair: Langes Haar
  clothes: Kleidung
  skinColor: Hautfarbe
  hairColor: Haarfarbe
  clothesColor: Kleidungsfarbe
  show: Anzeigen
  hide: Ausblenden
en:
  tabs:
    face: Face
    hair: Hair
    clothes: Clothes
    colors: Colors
  eyes: Eyes
  eyebrows: Eyebrows
  mouth: Mouth
  beard: Beard
  hair: Hairstyle
  rearHair: Long Hair
  clothes: Clothes
  skinColor: Skin Color
  hairColor: Hair Color
  clothesColor: Clothes Color
  show: Show
  hide: Hide
</i18n>
