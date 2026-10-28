<script setup lang="ts">
/**
 * The confirmation before an agent shares a contact (spec 060 FR-029 to FR-032, research R8): the
 * share dialog opens with the agent's proposal, the person may change the choice, and only what
 * they leave chosen goes to the agent, as a text. "Ablehnen" or closing it gives the agent nothing.
 * Every call asks, there is nothing to remember, and without an answer in 120 s Rust refuses.
 * Requests that arrive together wait in line.
 */
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import type { ContactShareConfirmRequest } from '@bindings/ContactShareConfirmRequest'
import type { ShareSelection } from '~/lib/contacts/share'

const queue = ref<ContactShareConfirmRequest[]>([])
const current = computed(() => queue.value[0])
const CONFIRM_TIMEOUT_MS = 120_000
const timeouts = new Map<string, ReturnType<typeof setTimeout>>()
let unlisten: UnlistenFn | undefined

// Closing the dialog any other way than with its buttons (Escape, the backdrop) is a "no".
const open = computed({
  get: () => current.value !== undefined,
  set: (value: boolean) => {
    if (!value) void answerAsync(null)
  },
})

function remove(requestId: string) {
  clearTimeout(timeouts.get(requestId))
  timeouts.delete(requestId)
  queue.value = queue.value.filter((request) => request.requestId !== requestId)
}

onMounted(async () => {
  try {
    unlisten = await listen<ContactShareConfirmRequest>(
      'contact-share-confirm-request',
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
    console.error('[contacts] listening for share confirmations failed', error)
  }
})

onBeforeUnmount(() => {
  unlisten?.()
  for (const timeout of timeouts.values()) clearTimeout(timeout)
  timeouts.clear()
})

async function answerAsync(selection: ShareSelection | null) {
  const request = current.value
  if (!request) return
  remove(request.requestId)
  await invoke('contact_share_confirm_answer', {
    requestId: request.requestId,
    selection,
  }).catch((error: unknown) => {
    console.error('[contacts] answering the share confirmation failed', error)
  })
}
</script>

<template>
  <ContactsShareDialog
    v-if="current"
    :key="current.requestId"
    v-model:open="open"
    mode="confirm"
    :contact-id="current.contactId"
    :contact-name="current.contactName"
    :offered="current.options"
    :proposed="current.proposed"
    @confirm="answerAsync"
    @decline="answerAsync(null)"
  />
</template>
