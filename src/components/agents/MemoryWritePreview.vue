<script setup lang="ts">
/**
 * What an agent wants to write to the shared memory, inside the question before the write (spec 097
 * FR-021, contracts "The question"): for an addition the page and the added text, for a rewrite the
 * stored and the new text, for a removal the text that goes. The approval carries the stored
 * snapshot, which is checked again before writing.
 */
import { memoryWritePreview } from '~/lib/agentMemory/pages'

const props = defineProps<{
  toolName: string
  toolInput: Record<string, unknown>
  stored: { description: string; body: string } | null
  modelName: string
}>()

const { t } = useI18n()

const preview = computed(() =>
  memoryWritePreview(props.toolName, props.toolInput, props.stored),
)
</script>

<template>
  <div class="space-y-2 text-sm" data-testid="memory-write-preview">
    <p class="text-muted-foreground">{{ modelName }}</p>
    <p class="font-medium">
      {{ preview.title }}
      <span
        v-if="preview.missing"
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
