<template>
  <UTabs
    v-model="activeTab"
    :items="tabItems"
    class="w-full"
  />

  <div class="mt-4 space-y-4 overflow-y-auto max-h-[40vh]">
    <!-- Face tab -->
    <template v-if="activeTab === 'face'">
      <!-- Face shape -->
      <div class="space-y-2">
        <span class="text-sm font-medium">{{ t('face') }}</span>
        <div class="grid grid-cols-3 sm:grid-cols-6 gap-2">
          <button
            v-for="option in faceOptions"
            :key="option"
            type="button"
            class="rounded-lg border-2 p-1 transition-colors hover:border-primary"
            :class="options.face === option ? 'border-primary bg-primary/10' : 'border-default'"
            @click="updateOption('face', option)"
          >
            <div class="w-full aspect-square [&>svg]:w-full [&>svg]:h-full" v-html="previewSvg('face', option)" />
          </button>
        </div>
      </div>

      <!-- Eyes -->
      <div class="space-y-2">
        <span class="text-sm font-medium">{{ t('eyes') }}</span>
        <div class="grid grid-cols-5 sm:grid-cols-7 gap-2">
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

      <!-- Mouth -->
      <div class="space-y-2">
        <div class="flex items-center justify-between">
          <span class="text-sm font-medium">{{ t('mouth') }}</span>
          <button
            type="button"
            class="text-xs text-muted hover:text-primary transition-colors"
            @click="toggleMouth"
          >
            {{ options.mouthProbability ? t('hide') : t('show') }}
          </button>
        </div>
        <div v-if="options.mouthProbability" class="grid grid-cols-5 sm:grid-cols-9 gap-2">
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
    </template>

    <!-- Top tab -->
    <template v-if="activeTab === 'top'">
      <div class="space-y-2">
        <div class="flex items-center justify-between">
          <span class="text-sm font-medium">{{ t('top') }}</span>
          <button
            type="button"
            class="text-xs text-muted hover:text-primary transition-colors"
            @click="toggleTop"
          >
            {{ options.topProbability ? t('hide') : t('show') }}
          </button>
        </div>
        <div v-if="options.topProbability" class="grid grid-cols-3 sm:grid-cols-5 gap-2">
          <button
            v-for="option in topOptions"
            :key="option"
            type="button"
            class="rounded-lg border-2 p-1 transition-colors hover:border-primary"
            :class="options.top === option ? 'border-primary bg-primary/10' : 'border-default'"
            @click="updateOption('top', option)"
          >
            <div class="w-full aspect-square [&>svg]:w-full [&>svg]:h-full" v-html="previewSvg('top', option)" />
          </button>
        </div>
      </div>
    </template>

    <!-- Sides tab -->
    <template v-if="activeTab === 'sides'">
      <div class="space-y-2">
        <div class="flex items-center justify-between">
          <span class="text-sm font-medium">{{ t('sides') }}</span>
          <button
            type="button"
            class="text-xs text-muted hover:text-primary transition-colors"
            @click="toggleSides"
          >
            {{ options.sidesProbability ? t('hide') : t('show') }}
          </button>
        </div>
        <div v-if="options.sidesProbability" class="grid grid-cols-4 sm:grid-cols-7 gap-2">
          <button
            v-for="option in sidesOptions"
            :key="option"
            type="button"
            class="rounded-lg border-2 p-1 transition-colors hover:border-primary"
            :class="options.sides === option ? 'border-primary bg-primary/10' : 'border-default'"
            @click="updateOption('sides', option)"
          >
            <div class="w-full aspect-square [&>svg]:w-full [&>svg]:h-full" v-html="previewSvg('sides', option)" />
          </button>
        </div>
      </div>
    </template>

    <!-- Style tab -->
    <template v-if="activeTab === 'style'">
      <!-- Base color -->
      <div class="space-y-2">
        <span class="text-sm font-medium">{{ t('baseColor') }}</span>
        <div class="flex flex-wrap gap-2">
          <button
            v-for="color in baseColors"
            :key="color"
            type="button"
            class="size-8 rounded-full border-2 transition-all hover:scale-110"
            :class="options.baseColor === color ? 'border-primary ring-2 ring-primary/30' : 'border-default'"
            :style="{ backgroundColor: `#${color}` }"
            @click="updateOption('baseColor', color)"
          />
        </div>
      </div>

      <!-- Texture -->
      <div class="space-y-2">
        <div class="flex items-center justify-between">
          <span class="text-sm font-medium">{{ t('texture') }}</span>
          <button
            type="button"
            class="text-xs text-muted hover:text-primary transition-colors"
            @click="toggleTexture"
          >
            {{ options.textureProbability ? t('hide') : t('show') }}
          </button>
        </div>
        <div v-if="options.textureProbability" class="grid grid-cols-4 gap-2">
          <button
            v-for="option in textureOptions"
            :key="option"
            type="button"
            class="rounded-lg border-2 p-1 transition-colors hover:border-primary"
            :class="options.texture === option ? 'border-primary bg-primary/10' : 'border-default'"
            @click="updateOption('texture', option)"
          >
            <div class="w-full aspect-square [&>svg]:w-full [&>svg]:h-full" v-html="previewSvg('texture', option)" />
          </button>
        </div>
      </div>
    </template>
  </div>
</template>

<script setup lang="ts">
import { createAvatar } from '@dicebear/core'
import * as bottts from '@dicebear/bottts'

export interface BotttsOptions {
  style: 'bottts'
  face: string
  eyes: string
  mouth: string
  mouthProbability: number
  top: string
  topProbability: number
  sides: string
  sidesProbability: number
  texture: string
  textureProbability: number
  baseColor: string
}

const options = defineModel<BotttsOptions>('options', { required: true })

const { t } = useI18n()

const activeTab = ref('face')
const tabItems = computed(() => [
  { label: t('tabs.face'), value: 'face' },
  { label: t('tabs.top'), value: 'top' },
  { label: t('tabs.sides'), value: 'sides' },
  { label: t('tabs.style'), value: 'style' },
])

// Available options
const faceOptions = ['round01', 'round02', 'square01', 'square02', 'square03', 'square04'] as const
const eyesOptions = ['bulging', 'dizzy', 'eva', 'frame1', 'frame2', 'glow', 'happy', 'hearts', 'robocop', 'round', 'roundFrame01', 'roundFrame02', 'sensor', 'shade01'] as const
const mouthOptions = ['bite', 'diagram', 'grill01', 'grill02', 'grill03', 'smile01', 'smile02', 'square01', 'square02'] as const
const topOptions = ['antenna', 'antennaCrooked', 'bulb01', 'glowingBulb01', 'glowingBulb02', 'horns', 'lights', 'pyramid', 'radar'] as const
const sidesOptions = ['antenna01', 'antenna02', 'cables01', 'cables02', 'round', 'square', 'squareAssymetric'] as const
const textureOptions = ['camo01', 'camo02', 'circuits', 'dirty01', 'dirty02', 'dots', 'grunge01', 'grunge02'] as const

const baseColors = [
  'ffb300', '1e88e5', '546e7a', '6d4c41', '00acc1',
  'f4511e', '5e35b1', '43a047', '757575', '3949ab',
  '039be5', '7cb342', 'c0ca33', 'fb8c00', 'd81b60',
  '8e24aa', 'e53935', '00897b', 'fdd835',
]

function updateOption(key: keyof BotttsOptions, value: string | number) {
  options.value = { ...options.value, [key]: value }
}

function toggleMouth() {
  options.value = { ...options.value, mouthProbability: options.value.mouthProbability ? 0 : 100 }
}

function toggleTop() {
  options.value = { ...options.value, topProbability: options.value.topProbability ? 0 : 100 }
}

function toggleSides() {
  options.value = { ...options.value, sidesProbability: options.value.sidesProbability ? 0 : 100 }
}

function toggleTexture() {
  options.value = { ...options.value, textureProbability: options.value.textureProbability ? 0 : 100 }
}

const previewCache = new Map<string, string>()

function previewSvg(key: string, value: string): string {
  const cacheKey = `${key}:${value}:${options.value.baseColor}`
  const cached = previewCache.get(cacheKey)
  if (cached) return cached

  const o = options.value
  const pick = (k: string) => k === key ? value : (o as Record<string, unknown>)[k]

  const previewOptions: Record<string, unknown> = {
    // Force-show the feature for preview
    mouthProbability: key === 'mouth' ? 100 : o.mouthProbability,
    topProbability: key === 'top' ? 100 : o.topProbability,
    sidesProbability: key === 'sides' ? 100 : o.sidesProbability,
    textureProbability: key === 'texture' ? 100 : o.textureProbability,
    // Wrap values into arrays for DiceBear
    face: [pick('face')],
    eyes: [pick('eyes')],
    mouth: [pick('mouth')],
    top: [pick('top')],
    sides: [pick('sides')],
    texture: [pick('texture')],
    baseColor: [pick('baseColor')],
  }

  const svg = createAvatar(bottts, previewOptions).toString()
  previewCache.set(cacheKey, svg)
  return svg
}

watch(() => options.value.baseColor, () => {
  previewCache.clear()
})
</script>

<i18n lang="yaml">
de:
  tabs:
    face: Gesicht
    top: Kopf
    sides: Seiten
    style: Style
  face: Gesichtsform
  eyes: Augen
  mouth: Mund
  top: Kopf-Accessoire
  sides: Seiten-Accessoire
  texture: Textur
  baseColor: Grundfarbe
  show: Anzeigen
  hide: Ausblenden
en:
  tabs:
    face: Face
    top: Top
    sides: Sides
    style: Style
  face: Face Shape
  eyes: Eyes
  mouth: Mouth
  top: Top Accessory
  sides: Side Accessory
  texture: Texture
  baseColor: Base Color
  show: Show
  hide: Hide
</i18n>
