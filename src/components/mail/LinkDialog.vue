<script setup lang="ts">
/**
 * The choice for a mail link that came in from outside (spec 063, US2): one row per extension
 * that writes mail, and "Link kopieren". The system's mail program is never offered here (FR-018),
 * and the dialog opens nothing itself (FR-020): the person taps a row, or closes it and nothing
 * happens. holzi reads the link (`opener_mail_check`), so what the dialog shows is what the
 * extension would get; a link that is refused shows only that it cannot be read.
 */
import { invoke } from '@tauri-apps/api/core'
import { toast } from 'vue-sonner'
import type { MailRequestView } from '@bindings/MailRequestView'
import { useMailInboxStore } from '~/stores/mailInbox'

const props = defineProps<{
  /** The saved session is in place: a link that waited may be shown now. */
  ready: boolean
}>()

const { t } = useI18n()
const inbox = useMailInboxStore()
const { rows, openWithAsync } = useMailOpeners()

const open = ref(false)
const link = ref('')
/** What holzi read from the link, or `null` while it reads and when it refused. */
const request = ref<MailRequestView | null>(null)
const unreadable = ref(false)
/** Counts the links shown, so a late answer for a replaced link is dropped. */
let shown = 0

async function showAsync(next: string) {
  const mine = ++shown
  link.value = next
  request.value = null
  unreadable.value = false
  open.value = true
  try {
    const read = await invoke<MailRequestView>('opener_mail_check', {
      link: next,
    })
    if (mine === shown) request.value = read
  } catch {
    if (mine === shown) unreadable.value = true
  }
}

// A new link replaces the content of an open dialog (research R6).
watch(
  [() => props.ready, () => inbox.link],
  ([ready, waiting]) => {
    if (!ready || waiting === null) return
    const next = inbox.take()
    if (next !== null) void showAsync(next)
  },
  { immediate: true },
)

async function openWithExtensionAsync(extensionId: string) {
  if (await openWithAsync(extensionId, link.value)) open.value = false
}

async function copyAsync() {
  try {
    await navigator.clipboard.writeText(link.value)
    toast.success(t('mail.link.copied'))
  } catch {
    toast.error(t('mail.link.copyFailed'))
  }
}
</script>

<template>
  <UiDrawerModal v-model:open="open" :title="t('mail.link.title')">
    <template #content>
      <div class="flex flex-col gap-4" data-testid="mail-link-dialog">
        <p
          v-if="unreadable"
          class="text-sm text-destructive"
          role="alert"
          data-testid="mail-link-error"
        >
          {{ t('mail.link.unreadable') }}
        </p>
        <template v-else-if="request">
          <dl class="grid grid-cols-[auto_1fr] gap-x-3 gap-y-1 text-sm">
            <template v-if="request.to.length">
              <dt class="text-muted-foreground">{{ t('mail.link.to') }}</dt>
              <dd class="break-all">{{ request.to.join(', ') }}</dd>
            </template>
            <template v-if="request.subject">
              <dt class="text-muted-foreground">
                {{ t('mail.link.subject') }}
              </dt>
              <dd class="break-words">{{ request.subject }}</dd>
            </template>
          </dl>
          <p v-if="rows.length" class="text-sm text-muted-foreground">
            {{ t('mail.link.openWith') }}
          </p>
          <p
            v-else
            class="text-sm text-muted-foreground"
            data-testid="mail-link-none"
          >
            {{ t('mail.link.none') }}
          </p>
          <ul v-if="rows.length" class="flex flex-col gap-2">
            <li v-for="row in rows" :key="row.extensionId">
              <UiButton
                class="w-full justify-start"
                variant="outline"
                :data-testid="`mail-link-open-${row.extensionId}`"
                @click="openWithExtensionAsync(row.extensionId)"
              >
                {{ row.title }}
              </UiButton>
            </li>
          </ul>
        </template>
        <div class="flex justify-end gap-2">
          <UiButton
            type="button"
            variant="outline"
            data-testid="mail-link-copy"
            @click="copyAsync"
          >
            {{ t('mail.link.copy') }}
          </UiButton>
          <UiButton type="button" variant="ghost" @click="open = false">
            {{ t('contacts.cancel') }}
          </UiButton>
        </div>
      </div>
    </template>
  </UiDrawerModal>
</template>
