<template>
  <div class="border-b border-default last:border-b-0">
    <UCollapsible v-model:open="isOpen" :unmount-on-hide="false">
      <!-- Section Header (entire default slot is the trigger via Nuxt UI) -->
      <div class="flex flex-col gap-2 p-3 @sm:p-4 cursor-pointer">
        <!-- Title row -->
        <div class="flex items-center gap-2 w-full">
          <UIcon
            name="i-lucide-chevron-right"
            class="w-4 h-4 shrink-0 text-muted transition-transform duration-200"
            :class="{ 'rotate-90': isOpen }"
          />
          <h3 class="text-lg font-semibold flex-1 truncate">{{ title }}</h3>
          <UPopover v-if="description" @click.stop>
            <UiButton
              icon="i-lucide-info"
              variant="ghost"
              color="neutral"
              size="sm"
            />
            <template #content>
              <div class="p-3 max-w-xs text-sm text-muted">
                {{ description }}
              </div>
            </template>
          </UPopover>
        </div>

        <!-- Actions row -->
        <div
          v-if="$slots.actions"
          class="flex flex-wrap items-center gap-2 pl-6"
          @click.stop
        >
          <slot name="actions" />
        </div>
      </div>

      <!-- Section Content -->
      <template #content>
        <div class="px-3 @sm:px-4 pb-3 @sm:pb-4">
          <slot />
        </div>
      </template>
    </UCollapsible>
  </div>
</template>

<script setup lang="ts">
const props = withDefaults(
  defineProps<{
    title: string
    description?: string
    defaultOpen?: boolean
  }>(),
  {
    description: undefined,
    defaultOpen: false,
  },
)

const isOpen = ref(props.defaultOpen)
</script>
