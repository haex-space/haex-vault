<script setup lang="ts">
/**
 * A confirmation an extension asked for (spec 017, T118, `extension_dialog_confirm`): drawn over
 * the extension's own tab only, never over holzi. Escape and the cancel button answer "no".
 *
 * It holds the keyboard while it is open (`useFrameDialogFocus`).
 */
import { ref } from 'vue'
import type { FrameDialog } from '~/composables/useExtensionFrame'
import { useFrameDialogFocus } from '~/composables/useFrameDialogFocus'

defineProps<{ dialog: FrameDialog }>()
const emit = defineEmits<{ answer: [confirmed: boolean] }>()
const { t } = useI18n()

const root = ref<HTMLElement | null>(null)
const reclaimFocus = useFrameDialogFocus(root, 'extension-dialog-confirm')
</script>

<template>
  <div
    ref="root"
    class="flex items-center justify-center bg-background/70 p-4"
    data-testid="extension-dialog"
    @keydown.esc="emit('answer', false)"
    @focusout="reclaimFocus"
  >
    <div
      role="alertdialog"
      aria-modal="true"
      class="w-full max-w-md space-y-4 rounded-lg border border-border bg-card p-5 shadow-lg"
    >
      <p class="text-xs text-muted-foreground">
        {{ t('extensions.dialog.fromExtension') }}
      </p>
      <h2 v-if="dialog.title" class="text-base font-semibold">
        {{ dialog.title }}
      </h2>
      <p class="whitespace-pre-line text-sm">{{ dialog.message }}</p>
      <div class="flex justify-end gap-2">
        <UiButton
          variant="outline"
          data-testid="extension-dialog-cancel"
          @click="emit('answer', false)"
        >
          {{ dialog.cancelLabel ?? t('extensions.dialog.cancel') }}
        </UiButton>
        <UiButton
          :variant="dialog.destructive ? 'destructive' : 'default'"
          data-testid="extension-dialog-confirm"
          @click="emit('answer', true)"
        >
          {{ dialog.confirmLabel ?? t('extensions.dialog.confirm') }}
        </UiButton>
      </div>
    </div>
  </div>
</template>
