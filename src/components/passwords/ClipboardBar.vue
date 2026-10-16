<script setup lang="ts">
/**
 * The bar under the breadcrumbs while the Ablage holds something and nothing is selected (spec 036,
 * US3, FR-013): "n in der Ablage" with Einfügen into the open folder and "Ablage leeren". Where no
 * paste goes (trash, search, a tag view) Einfügen is off and says why.
 */
defineProps<{
  /** The place takes a paste. */
  canPaste: boolean
}>()

const emit = defineEmits<{
  paste: []
}>()

const { t } = useI18n()
const clipboard = usePasswordsClipboardStore()
const { shortcut } = useShortcutText()
</script>

<template>
  <div
    class="flex flex-wrap items-center gap-2 rounded-xl border border-dashed px-3 py-1.5"
    role="status"
    data-testid="passwords-ablage"
  >
    <Icon
      :name="
        clipboard.mode === 'cut' ? 'lucide:scissors' : 'lucide:clipboard-copy'
      "
      class="size-4 shrink-0 text-muted-foreground"
    />
    <span
      class="mr-auto text-sm font-medium"
      data-testid="passwords-ablage-count"
    >
      {{
        t(
          'passwords.clipboard.count',
          { count: clipboard.count },
          clipboard.count,
        )
      }}
    </span>
    <UiButton
      size="sm"
      variant="outline"
      :disabled="!canPaste"
      :tooltip="
        canPaste
          ? shortcut({ mod: true, key: 'V' })
          : t('passwords.clipboard.notHere')
      "
      data-testid="passwords-ablage-paste"
      @click="emit('paste')"
    >
      <Icon name="lucide:clipboard-paste" class="size-4" />
      {{ t('passwords.clipboard.paste') }}
    </UiButton>
    <UiButton
      size="sm"
      variant="ghost"
      data-testid="passwords-ablage-clear"
      @click="clipboard.clear()"
    >
      {{ t('passwords.clipboard.clear') }}
    </UiButton>
  </div>
</template>
