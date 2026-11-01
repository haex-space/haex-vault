<script setup lang="ts">
/** Ablehnen and Erlauben for an agent's call (spec 021, US3, US6); `true` allows. */
defineProps<{ destructive: boolean }>()
const emit = defineEmits<{ answer: [allow: boolean] }>()
const { t } = useI18n()
</script>

<template>
  <div class="flex justify-end gap-2">
    <UiButton
      type="button"
      size="sm"
      variant="outline"
      data-testid="mcp-call-deny"
      @click="emit('answer', false)"
    >
      {{ t('mcpAgents.call.deny') }}
    </UiButton>
    <UiButton
      type="button"
      size="sm"
      :variant="destructive ? 'destructive' : 'default'"
      data-testid="mcp-call-allow"
      @click="emit('answer', true)"
    >
      {{ t('mcpAgents.call.allow') }}
    </UiButton>
  </div>
</template>
