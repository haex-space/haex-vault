<template>
  <div class="@container">
    <!-- Header -->
    <div class="p-6 border-b border-default">
      <div class="flex items-center gap-3">
        <UiButton
          v-if="showBack"
          icon="i-heroicons-arrow-left"
          variant="ghost"
                    @click="emit('back')"
        />
        <div>
          <h2 class="text-2xl font-bold">
            <slot name="title">{{ title }}</slot>
          </h2>
          <p
            v-if="description || $slots.description"
            class="text-sm text-muted mt-1"
          >
            <slot name="description">{{ description }}</slot>
          </p>
        </div>
      </div>
    </div>

    <!-- Content -->
    <div class="p-6 space-y-6">
      <slot />
    </div>
  </div>
</template>

<script setup lang="ts">
withDefaults(
  defineProps<{
    title?: string
    description?: string
    showBack?: boolean
  }>(),
  {
    showBack: false,
  },
)

const emit = defineEmits<{
  back: []
}>()
</script>
