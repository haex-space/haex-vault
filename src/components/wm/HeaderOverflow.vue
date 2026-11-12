<script setup lang="ts">
/**
 * The overflow menu of a window header (spec 090-app-header-bar, FR-014, research R10): the `⋯`
 * button and a popover holding the controls that did not fit, each in its menu form. It closes
 * after a button, toggle, choice or menu entry; a search stays open, so typing is not cut off.
 * Closing gives focus back to the `⋯` button.
 */
import type { HeaderSearchField } from '~/composables/useHeaderSearches'
import type { WmHeaderControl } from '~/types/bindings/WmHeaderControl'
import type { WmHeaderEvent } from '~/types/bindings/WmHeaderEvent'

defineProps<{
  controls: readonly WmHeaderControl[]
  icons?: Record<string, string>
  searchFor: (control: WmHeaderControl) => HeaderSearchField | undefined
}>()

const emit = defineEmits<{ event: [event: WmHeaderEvent] }>()

const { t } = useI18n()
const open = ref(false)

function onEvent(event: WmHeaderEvent) {
  emit('event', event)
  if (event.kind !== 'input' && event.kind !== 'submit') open.value = false
}
</script>

<template>
  <ShadcnPopover v-model:open="open">
    <ShadcnPopoverTrigger as-child>
      <UiButton
        variant="ghost"
        size="icon"
        class="shrink-0"
        :aria-label="t('wm.header.overflow')"
        :tooltip="t('wm.header.overflow')"
        data-testid="wm-header-overflow"
        @pointerdown.stop
      >
        <Icon name="lucide:ellipsis" class="size-4" :aria-hidden="true" />
      </UiButton>
    </ShadcnPopoverTrigger>
    <ShadcnPopoverContent
      align="end"
      class="flex w-72 flex-col gap-1 p-1"
      data-testid="wm-header-overflow-menu"
    >
      <WmHeaderControl
        v-for="control in controls"
        :key="control.id"
        form="menu"
        :control="control"
        :icons="icons"
        :search="searchFor(control)"
        @event="onEvent"
      />
    </ShadcnPopoverContent>
  </ShadcnPopover>
</template>
