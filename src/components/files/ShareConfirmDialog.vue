<script setup lang="ts">
/**
 * The confirmation before an agent gives files away (spec 056 FR-045, research R17): a new share, a
 * contact added or a level raised names the folder or file, the device that serves it and each
 * contact with its level. Only "Freigeben" lets it through; there is nothing to remember, and
 * without an answer in 120 s Rust refuses. Confirmations that arrive together wait in line.
 */
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import type { FileShareConfirmRequest } from '@bindings/FileShareConfirmRequest'

const { t } = useI18n()
const queue = ref<FileShareConfirmRequest[]>([])
const current = computed(() => queue.value[0])
const CONFIRM_TIMEOUT_MS = 120_000
const timeouts = new Map<string, ReturnType<typeof setTimeout>>()
let unlisten: UnlistenFn | undefined

function remove(requestId: string) {
  clearTimeout(timeouts.get(requestId))
  timeouts.delete(requestId)
  queue.value = queue.value.filter((request) => request.requestId !== requestId)
}

onMounted(async () => {
  try {
    unlisten = await listen<FileShareConfirmRequest>(
      'file-share-confirm-request',
      (event) => {
        const request = event.payload
        queue.value.push(request)
        timeouts.set(
          request.requestId,
          setTimeout(() => remove(request.requestId), CONFIRM_TIMEOUT_MS),
        )
      },
    )
  } catch (error) {
    console.error('[files] listening for share confirmations failed', error)
  }
})

onBeforeUnmount(() => {
  unlisten?.()
  for (const timeout of timeouts.values()) clearTimeout(timeout)
  timeouts.clear()
})

async function answerAsync(confirmed: boolean) {
  const request = current.value
  if (!request) return
  remove(request.requestId)
  await invoke('file_share_confirm_answer', {
    requestId: request.requestId,
    confirmed,
  }).catch((error: unknown) => {
    console.error('[files] answering the share confirmation failed', error)
  })
}
</script>

<template>
  <ShadcnAlertDialog
    :open="current !== undefined"
    @update:open="(open: boolean) => !open && answerAsync(false)"
  >
    <ShadcnAlertDialogContent v-if="current" data-testid="files-share-confirm">
      <ShadcnAlertDialogHeader>
        <ShadcnAlertDialogTitle>
          {{
            current.newShare
              ? t('files.shares.confirm.titleNew')
              : t('files.shares.confirm.titleMore')
          }}
        </ShadcnAlertDialogTitle>
        <ShadcnAlertDialogDescription>
          {{
            t('files.shares.confirm.body', {
              name: current.shareName,
              device:
                current.deviceName ?? t('files.shares.confirm.thisDevice'),
            })
          }}
        </ShadcnAlertDialogDescription>
      </ShadcnAlertDialogHeader>
      <p
        class="truncate text-xs text-muted-foreground"
        data-testid="files-share-confirm-path"
      >
        {{ current.path }}
      </p>
      <ul v-if="current.grants.length" class="flex flex-col gap-1 text-sm">
        <li
          v-for="grant in current.grants"
          :key="grant.contactName"
          class="flex items-center justify-between gap-2"
        >
          <span class="truncate">{{ grant.contactName }}</span>
          <span class="text-muted-foreground">{{
            t(`files.shares.level.${grant.level}`)
          }}</span>
        </li>
      </ul>
      <p v-else class="text-sm text-muted-foreground">
        {{ t('files.shares.confirm.noContacts') }}
      </p>
      <ShadcnAlertDialogFooter class="flex-wrap gap-2">
        <ShadcnAlertDialogCancel data-testid="files-share-confirm-deny">
          {{ t('files.shares.confirm.deny') }}
        </ShadcnAlertDialogCancel>
        <UiButton
          data-testid="files-share-confirm-allow"
          @click="answerAsync(true)"
        >
          {{ t('files.shares.confirm.allow') }}
        </UiButton>
      </ShadcnAlertDialogFooter>
    </ShadcnAlertDialogContent>
  </ShadcnAlertDialog>
</template>
