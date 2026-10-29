<script setup lang="ts">
/**
 * The mail button of an address when an extension writes mail (spec 063, US1): a small menu with
 * the system's mail program and one row per extension that registered for it. Closing the menu
 * without a choice opens nothing (FR-004).
 */
import { openUrl } from '@tauri-apps/plugin-opener'
import { toast } from 'vue-sonner'
import { useExtensionsStore } from '~/stores/extensions'

const props = defineProps<{
  /** The `mailto:` link of the address. */
  url: string
}>()

const { t } = useI18n()
const extensions = useExtensionsStore()
const { rows, openWithAsync } = useMailOpeners()

async function openSystemAsync() {
  try {
    await openUrl(props.url)
  } catch {
    toast.error(t('contacts.openFailed'))
  }
}
</script>

<template>
  <ShadcnDropdownMenu>
    <ShadcnDropdownMenuTrigger as-child>
      <UiButton
        variant="ghost"
        size="icon"
        :aria-label="t('contacts.open.mail')"
        :tooltip="t('contacts.open.mail')"
        data-testid="contacts-open-mail"
      >
        <Icon name="lucide:mail" class="size-4" />
      </UiButton>
    </ShadcnDropdownMenuTrigger>
    <ShadcnDropdownMenuContent align="end">
      <ShadcnDropdownMenuItem
        class="gap-2"
        data-testid="contacts-open-system"
        @select="openSystemAsync"
      >
        <Icon name="lucide:external-link" class="size-4" />
        {{ t('mail.openers.system') }}
      </ShadcnDropdownMenuItem>
      <ShadcnDropdownMenuItem
        v-for="row in rows"
        :key="row.extensionId"
        class="gap-2"
        :data-testid="`contacts-open-with-${row.extensionId}`"
        @select="openWithAsync(row.extensionId, url)"
      >
        <img
          v-if="extensions.icons[row.extensionId]"
          :src="extensions.icons[row.extensionId]"
          alt=""
          class="size-4 rounded-sm"
        />
        <Icon v-else name="lucide:puzzle" class="size-4" />
        {{ row.title }}
      </ShadcnDropdownMenuItem>
    </ShadcnDropdownMenuContent>
  </ShadcnDropdownMenu>
</template>
