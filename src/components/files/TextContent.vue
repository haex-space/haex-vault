<script setup lang="ts">
/**
 * A text the way the viewers show it (spec 044 FR-015, spec 058 FR-003): a notice when only the
 * start of a long file came, then the text as written, wrapped, selectable. Used by the file viewer
 * and by the attachment view of the password manager, so both look the same.
 */
import type { TextContent } from '@bindings/TextContent'

defineProps<{ text: TextContent }>()

const { t } = useI18n()
</script>

<template>
  <p
    v-if="text.truncated"
    class="border-b bg-muted px-4 py-2 text-xs text-muted-foreground"
  >
    {{ t('files.viewer.truncated') }}
  </p>
  <pre class="p-4 font-mono text-sm break-words whitespace-pre-wrap">{{
    text.text
  }}</pre>
</template>
