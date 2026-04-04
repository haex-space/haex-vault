<template>
  <div class="relative inline-flex shrink-0">
    <!-- Main avatar -->
    <div
      :class="[
        'rounded-full overflow-hidden bg-muted flex items-center justify-center',
        sizeClasses[size],
      ]"
    >
      <img
        v-if="src"
        :src="src"
        :alt="alt"
        class="w-full h-full object-cover"
      >
      <div
        v-else
        class="w-full h-full [&>svg]:w-full [&>svg]:h-full"
        v-html="fallbackSvg"
      />
    </div>

    <!-- Badge (e.g. identity avatar on device) -->
    <div
      v-if="badgeSrc || badgeSeed"
      :class="[
        'absolute -bottom-0.5 -right-0.5 rounded-full overflow-hidden ring-2 ring-default bg-muted flex items-center justify-center',
        badgeSizeClasses[size],
      ]"
    >
      <img
        v-if="badgeSrc"
        :src="badgeSrc"
        :alt="badgeAlt"
        class="w-full h-full object-cover"
      >
      <div
        v-else-if="badgeSeed"
        class="w-full h-full [&>svg]:w-full [&>svg]:h-full"
        v-html="badgeFallbackSvg"
      />
    </div>
  </div>
</template>

<script setup lang="ts">
import { createAvatar, type Style } from '@dicebear/core'
import * as bottts from '@dicebear/bottts'
import * as toonHead from '@dicebear/toon-head'

const avatarStyles: Record<string, Style<object>> = { bottts, 'toon-head': toonHead }

const props = withDefaults(defineProps<{
  src?: string | null
  seed?: string
  avatarOptions?: Record<string, unknown> | null
  alt?: string
  size?: 'xs' | 'sm' | 'md' | 'lg' | 'xl'
  avatarStyle?: keyof typeof avatarStyles
  badgeSrc?: string | null
  badgeSeed?: string
  badgeAlt?: string
}>(), {
  size: 'md',
  alt: 'Avatar',
  avatarStyle: 'bottts',
  badgeAlt: 'Badge',
})

const sizeClasses: Record<string, string> = {
  xs: 'size-6',
  sm: 'size-8',
  md: 'size-10',
  lg: 'size-14',
  xl: 'size-20',
}

const badgeSizeClasses: Record<string, string> = {
  xs: 'size-3',
  sm: 'size-4',
  md: 'size-5',
  lg: 'size-6',
  xl: 'size-8',
}

const fallbackSvg = computed(() => {
  if (props.src) return ''
  if (props.avatarOptions) {
    const style = avatarStyles[props.avatarOptions.style as string] ?? avatarStyles[props.avatarStyle] ?? bottts
    return createAvatar(style, { seed: props.seed, ...props.avatarOptions }).toString()
  }
  if (!props.seed) return ''
  return createAvatar(avatarStyles[props.avatarStyle] ?? bottts, { seed: props.seed }).toString()
})

const badgeFallbackSvg = computed(() => {
  if (!props.badgeSeed) return ''
  return createAvatar(bottts, { seed: props.badgeSeed }).toString()
})
</script>
