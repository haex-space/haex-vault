<script setup lang="ts">
/**
 * The deletions of a rule waiting for the user (spec 061 US3, FR-021): their number, and the list
 * to delete them, keep them where they are, or copy them back, one by one or all at once. A file
 * that changed since the run found it is left to the next run and drops from the list.
 */
import type { SyncRuleFileError } from '@bindings/SyncRuleFileError'
import type { SyncRulePending } from '@bindings/SyncRulePending'
import type { SyncRulePendingChoice } from '@bindings/SyncRulePendingChoice'
import type { SyncRuleSide } from '@bindings/SyncRuleSide'
import type { SyncRuleView } from '@bindings/SyncRuleView'
import { asSyncRuleError } from '~/composables/useSyncRules'

const props = defineProps<{
  view: SyncRuleView
  /** The names of both places, as the overview shows them. */
  labels: Record<SyncRuleSide, string>
}>()

const emit = defineEmits<{ changed: [] }>()

const { t } = useI18n()
const { pendingListAsync, pendingResolveAsync } = useSyncRules()

const expanded = ref(false)
const items = ref<SyncRulePending[]>([])
const busy = ref(false)
const failures = ref<SyncRuleFileError[]>([])
const error = ref<string | null>(null)

const count = computed(() => props.view.status.pendingDeletes)

async function loadAsync() {
  try {
    items.value = await pendingListAsync(props.view.rule.id)
    error.value = null
  } catch (cause) {
    showError(cause)
  }
}

function showError(cause: unknown) {
  const code = asSyncRuleError(cause)?.code
  error.value = code
    ? t(`files.syncRules.error.${code}`)
    : t('files.syncRules.failed')
}

async function toggleAsync() {
  expanded.value = !expanded.value
  if (expanded.value) await loadAsync()
}

// A new run may have found more; an open list follows the count.
watch(count, () => {
  if (expanded.value) void loadAsync()
})

async function resolveAsync(paths: string[], choice: SyncRulePendingChoice) {
  if (busy.value || paths.length === 0) return
  busy.value = true
  try {
    failures.value = await pendingResolveAsync(
      props.view.rule.id,
      paths,
      choice,
    )
    error.value = null
  } catch (cause) {
    showError(cause)
  } finally {
    busy.value = false
    await loadAsync()
    emit('changed')
  }
}

function resolveAllAsync(choice: SyncRulePendingChoice) {
  return resolveAsync(
    items.value.map((item) => item.path),
    choice,
  )
}

const choices: { choice: SyncRulePendingChoice; icon: string }[] = [
  { choice: 'delete', icon: 'lucide:trash-2' },
  { choice: 'keep', icon: 'lucide:archive' },
  { choice: 'restore', icon: 'lucide:undo-2' },
]
</script>

<template>
  <div class="flex flex-col gap-2">
    <UiButton
      type="button"
      variant="outline"
      size="sm"
      class="self-start"
      :aria-expanded="expanded"
      :data-testid="`files-sync-rule-pending-${view.rule.name}`"
      @click="toggleAsync"
    >
      <Icon name="lucide:circle-help" class="size-4" />
      {{ t('files.syncRules.pending.count', count) }}
    </UiButton>
    <div
      v-if="expanded"
      class="flex flex-col gap-2 rounded-md border p-2"
      :data-testid="`files-sync-rule-pending-list-${view.rule.name}`"
    >
      <p v-if="error" class="text-xs text-destructive" role="alert">
        {{ error }}
      </p>
      <div v-if="items.length > 1" class="flex flex-wrap gap-1">
        <UiButton
          v-for="entry in choices"
          :key="entry.choice"
          type="button"
          variant="ghost"
          size="sm"
          :disabled="busy"
          :data-testid="`files-sync-rule-pending-all-${entry.choice}`"
          @click="resolveAllAsync(entry.choice)"
        >
          <Icon :name="entry.icon" class="size-4" />
          {{ t(`files.syncRules.pending.all.${entry.choice}`) }}
        </UiButton>
      </div>
      <ul class="flex flex-col gap-1">
        <li
          v-for="item in items"
          :key="item.path"
          class="flex items-center gap-2"
          :data-testid="`files-sync-rule-pending-item-${item.path}`"
        >
          <div class="flex min-w-0 flex-1 flex-col">
            <span class="truncate text-sm">{{ item.path }}</span>
            <span class="truncate text-xs text-muted-foreground">{{
              t('files.syncRules.pending.at', { place: labels[item.side] })
            }}</span>
          </div>
          <UiButton
            v-for="entry in choices"
            :key="entry.choice"
            type="button"
            variant="ghost"
            size="icon-sm"
            :disabled="busy"
            :aria-label="t(`files.syncRules.pending.one.${entry.choice}`)"
            :tooltip="t(`files.syncRules.pending.one.${entry.choice}`)"
            :data-testid="`files-sync-rule-pending-${entry.choice}-${item.path}`"
            @click="resolveAsync([item.path], entry.choice)"
          >
            <Icon :name="entry.icon" class="size-4" />
          </UiButton>
        </li>
      </ul>
      <ul v-if="failures.length" class="flex flex-col gap-1">
        <li
          v-for="failure in failures"
          :key="failure.path"
          class="text-xs text-destructive"
        >
          {{ failure.path }}: {{ failure.reason }}
        </li>
      </ul>
    </div>
  </div>
</template>
