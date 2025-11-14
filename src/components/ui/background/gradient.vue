<template>
  <div
    class="absolute inset-0 -z-10 overflow-hidden pointer-events-none"
    :class="{ 'opacity-0': !gradientEnabled }"
  >
    <!-- Base fill -->
    <div
      class="absolute inset-0"
      :style="{
        backgroundColor: currentGradient.baseFill,
      }"
    />

    <!-- Gradient orbs -->
    <div
      v-for="(orb, index) in currentGradient.orbs"
      :key="index"
      class="absolute rounded-full blur-3xl"
      :style="{
        backgroundColor: orb.color,
        opacity: orb.opacity,
        width: orb.size,
        height: orb.size,
        left: orb.left,
        top: orb.top,
        transform: `translate(-50%, -50%)`,
      }"
    />
  </div>
</template>

<script setup lang="ts">
import type { GradientVariant } from '~/types/gradient'

const { currentThemeName } = storeToRefs(useUiStore())
const { gradientVariant, gradientEnabled } = storeToRefs(useGradientStore())

// Define gradient orb configuration
interface GradientOrb {
  color: string
  opacity: number
  size: string
  left: string
  top: string
}

interface GradientConfig {
  baseFill: string
  orbs: GradientOrb[]
}

// GitLab-inspired gradients
const gradients: Record<GradientVariant, { light: GradientConfig; dark: GradientConfig }> = {
  gitlab: {
    light: {
      baseFill: '#ffffff',
      orbs: [
        { color: '#FC6D26', opacity: 0.25, size: '60%', left: '20%', top: '30%' },
        { color: '#A989F5', opacity: 0.25, size: '50%', left: '70%', top: '60%' },
        { color: '#FFB9C9', opacity: 0.20, size: '45%', left: '50%', top: '80%' },
      ],
    },
    dark: {
      baseFill: '#232150',
      orbs: [
        { color: '#FC6D26', opacity: 0.20, size: '60%', left: '20%', top: '30%' },
        { color: '#A989F5', opacity: 0.20, size: '50%', left: '70%', top: '60%' },
        { color: '#FFB9C9', opacity: 0.15, size: '45%', left: '50%', top: '80%' },
      ],
    },
  },
  ocean: {
    light: {
      baseFill: '#ffffff',
      orbs: [
        { color: '#0EA5E9', opacity: 0.25, size: '60%', left: '25%', top: '35%' },
        { color: '#06B6D4', opacity: 0.25, size: '50%', left: '65%', top: '65%' },
        { color: '#8B5CF6', opacity: 0.20, size: '45%', left: '45%', top: '15%' },
      ],
    },
    dark: {
      baseFill: '#0c1222',
      orbs: [
        { color: '#0EA5E9', opacity: 0.20, size: '60%', left: '25%', top: '35%' },
        { color: '#06B6D4', opacity: 0.20, size: '50%', left: '65%', top: '65%' },
        { color: '#8B5CF6', opacity: 0.15, size: '45%', left: '45%', top: '15%' },
      ],
    },
  },
  sunset: {
    light: {
      baseFill: '#ffffff',
      orbs: [
        { color: '#F59E0B', opacity: 0.25, size: '60%', left: '30%', top: '40%' },
        { color: '#EF4444', opacity: 0.25, size: '50%', left: '60%', top: '70%' },
        { color: '#EC4899', opacity: 0.20, size: '45%', left: '75%', top: '25%' },
      ],
    },
    dark: {
      baseFill: '#1a1625',
      orbs: [
        { color: '#F59E0B', opacity: 0.20, size: '60%', left: '30%', top: '40%' },
        { color: '#EF4444', opacity: 0.20, size: '50%', left: '60%', top: '70%' },
        { color: '#EC4899', opacity: 0.15, size: '45%', left: '75%', top: '25%' },
      ],
    },
  },
  forest: {
    light: {
      baseFill: '#ffffff',
      orbs: [
        { color: '#10B981', opacity: 0.25, size: '60%', left: '35%', top: '45%' },
        { color: '#059669', opacity: 0.25, size: '50%', left: '55%', top: '75%' },
        { color: '#14B8A6', opacity: 0.20, size: '45%', left: '70%', top: '20%' },
      ],
    },
    dark: {
      baseFill: '#0a1f1a',
      orbs: [
        { color: '#10B981', opacity: 0.20, size: '60%', left: '35%', top: '45%' },
        { color: '#059669', opacity: 0.20, size: '50%', left: '55%', top: '75%' },
        { color: '#14B8A6', opacity: 0.15, size: '45%', left: '70%', top: '20%' },
      ],
    },
  },
}

const currentGradient = computed(() => {
  const isDark = currentThemeName.value.includes('dark')
  const variant = gradientVariant.value
  return gradients[variant][isDark ? 'dark' : 'light']
})
</script>
