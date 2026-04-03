<template>
  <UModal
    v-model:open="open"
    :title="title"
    :description="description"
    :fullscreen="isSmallScreen"
    v-bind="$attrs"
    :ui="mergedUi"
  >
    <!-- Trigger: maps #trigger to UModal's default slot -->
    <slot name="trigger" />

    <!-- Header: pass-through to UModal's native header -->
    <template v-if="$slots.header" #header>
      <slot name="header" />
    </template>

    <!-- Description: pass-through -->
    <template v-if="$slots.description" #description>
      <slot name="description" />
    </template>

    <!-- Body -->
    <template v-if="$slots.body" #body>
      <slot name="body" />
    </template>

    <!-- Footer: pass-through -->
    <template v-if="$slots.footer" #footer>
      <slot name="footer" />
    </template>
  </UModal>
</template>

<script setup lang="ts">
const props = defineProps<{
  title?: string
  description?: string
  ui?: Record<string, string>
}>()

const open = defineModel<boolean>('open', { default: false })
const { isSmallScreen } = storeToRefs(useUiStore())

const mergedUi = computed(() => ({
  ...props.ui,
  header: `pt-[env(safe-area-inset-top)] ${props.ui?.header ?? ''}`.trim(),
  body: `flex flex-col justify-center ${props.ui?.body ?? ''}`.trim(),
  footer: `pb-[env(safe-area-inset-bottom)] ${props.ui?.footer ?? ''}`.trim(),
}))
</script>
