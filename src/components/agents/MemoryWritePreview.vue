<script setup lang="ts">
/**
 * What an agent wants to write to the shared memory, inside the question before the write (spec 097
 * FR-021, contracts "The question"): for an addition the page and the added text, for a rewrite the
 * stored and the new text, for a removal the text that goes. The stored page is read here by its
 * title; a title without a page says so.
 */
import { invoke } from '@tauri-apps/api/core'
import { memoryWritePreview } from '~/lib/agentMemory/pages'
import type { AgentMemoryPage } from '~/types/bindings/AgentMemoryPage'

const props = defineProps<{
  toolName: string
  toolInput: Record<string, unknown>
}>()

const { t } = useI18n()
const stored = ref<AgentMemoryPage | null>(null)
const loaded = ref(false)

const preview = computed(() =>
  memoryWritePreview(props.toolName, props.toolInput, stored.value),
)

async function loadAsync() {
  loaded.value = false
  const title = props.toolInput.title
  try {
    stored.value =
      typeof title === 'string'
        ? await invoke<AgentMemoryPage | null>('agent_memory_page_by_title', {
            args: { title },
          })
        : null
  } catch (error) {
    console.error('[agent-memory] the stored page could not be read', error)
    stored.value = null
  } finally {
    loaded.value = true
  }
}

watch(() => props.toolInput, loadAsync, { immediate: true })
</script>

<template>
  <div class="space-y-2 text-sm" data-testid="memory-write-preview">
    <p class="font-medium">
      {{ preview.title }}
      <span
        v-if="loaded && preview.missing"
        class="text-muted-foreground"
        data-testid="memory-write-preview-missing"
      >
        ·
        {{
          preview.kind === 'add'
            ? t('settings.agentMemory.preview.newPage')
            : t('settings.agentMemory.preview.notFound')
        }}
      </span>
    </p>
    <p v-if="preview.newTitle">
      {{ t('settings.agentMemory.preview.newTitle') }}: {{ preview.newTitle }}
    </p>
    <div v-if="preview.stored">
      <p class="text-muted-foreground">
        {{
          preview.kind === 'remove'
            ? t('settings.agentMemory.preview.removed')
            : t('settings.agentMemory.preview.before')
        }}
      </p>
      <p class="italic">{{ preview.stored.description }}</p>
      <pre
        class="max-h-40 overflow-auto whitespace-pre-wrap rounded bg-muted/30 p-2 text-xs"
        >{{ preview.stored.text }}</pre>
    </div>
    <div v-if="preview.text !== undefined">
      <p class="text-muted-foreground">
        {{
          preview.kind === 'add'
            ? t('settings.agentMemory.preview.added')
            : t('settings.agentMemory.preview.after')
        }}
      </p>
      <p v-if="preview.description" class="italic">
        {{ preview.description }}
      </p>
      <pre
        class="max-h-40 overflow-auto whitespace-pre-wrap rounded bg-muted/30 p-2 text-xs"
        >{{ preview.text }}</pre>
    </div>
  </div>
</template>
