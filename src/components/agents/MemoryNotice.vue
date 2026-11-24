<script setup lang="ts">
/**
 * The notice after an agent wrote to the shared memory (spec 097 FR-023, contracts "Events"): every
 * `agent-memory-written` of this device becomes a toast with the model, the page and the text, since
 * nothing else in the window shows what an agent stored.
 */
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { toast } from 'vue-sonner'
import {
  EVENT_MEMORY_WRITTEN,
  memoryNoticeKey,
  type MemoryWritten,
} from '~/lib/agentMemory/pages'

const { t } = useI18n()
const openApp = useAction('wm.app.open')
let unlisten: UnlistenFn | undefined
let disposed = false

function show(notice: MemoryWritten) {
  toast.info(
    t(memoryNoticeKey(notice.kind), {
      name: notice.authorName || t('settings.agentMemory.unknownAuthor'),
      title: notice.title,
    }),
    {
      description: notice.text,
      action: {
        label: t('settings.agentMemory.notice.open'),
        onClick: () => openMemory(notice),
      },
    },
  )
}

function openMemory(notice: MemoryWritten) {
  const at =
    notice.kind === 'removed' ? '/ai/memory' : `/ai/memory/${notice.pageId}`
  void openApp({ appId: 'system.settings', at })
}

onMounted(async () => {
  try {
    const stop = await listen<MemoryWritten>(EVENT_MEMORY_WRITTEN, (event) => {
      if (!disposed) show(event.payload)
    })
    if (disposed) stop()
    else unlisten = stop
  } catch (error) {
    console.error('[agent-memory] listening for notices failed', error)
  }
})
onBeforeUnmount(() => {
  disposed = true
  unlisten?.()
})
</script>

<template>
  <span hidden />
</template>
