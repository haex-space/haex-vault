<script setup lang="ts">
/**
 * The run log of a rule (spec 061 US4, FR-043, FR-046, SC-010): the kept runs, the latest first,
 * with cause, result and counters; a run opens to the list of its files with action and side. A
 * mirrored deletion names where the file went: the path in the rule trash of the place, or the
 * system's trash.
 */
import type { SyncRuleRun } from '@bindings/SyncRuleRun'
import type { SyncRuleRunItem } from '@bindings/SyncRuleRunItem'
import type { SyncRuleView } from '@bindings/SyncRuleView'
import { asSyncRuleError } from '~/composables/useSyncRules'

const props = defineProps<{
  view: SyncRuleView | null
  /** The names of both places. */
  labels: { a: string; b: string }
}>()

const open = defineModel<boolean>('open', { required: true })

/** Files loaded per step of a long run. */
const PAGE = 200

const { t, d } = useI18n()
const { runsAsync, runItemsAsync } = useSyncRules()

const runs = ref<SyncRuleRun[]>([])
const loading = ref(false)
const error = ref<string | null>(null)
const expanded = ref<string | null>(null)
const items = ref<SyncRuleRunItem[]>([])
const more = ref(false)

function showError(cause: unknown) {
  const code = asSyncRuleError(cause)?.code
  error.value = code
    ? t(`files.syncRules.error.${code}`)
    : t('files.syncRules.failed')
}

watch(open, async (isOpen) => {
  const view = props.view
  if (!isOpen || !view) return
  runs.value = []
  expanded.value = null
  error.value = null
  loading.value = true
  try {
    runs.value = await runsAsync(view.rule.id)
  } catch (cause) {
    showError(cause)
  } finally {
    loading.value = false
  }
})

async function loadItemsAsync(runId: string, offset: number) {
  try {
    const page = await runItemsAsync(runId, offset, PAGE)
    if (expanded.value !== runId) return
    items.value = offset === 0 ? page : [...items.value, ...page]
    more.value = page.length === PAGE
  } catch (cause) {
    showError(cause)
  }
}

async function toggleAsync(run: SyncRuleRun) {
  if (expanded.value === run.id) {
    expanded.value = null
    return
  }
  expanded.value = run.id
  items.value = []
  more.value = false
  await loadItemsAsync(run.id, 0)
}

function started(run: SyncRuleRun): string {
  return d(new Date(run.startedMs), { dateStyle: 'short', timeStyle: 'medium' })
}

function seconds(run: SyncRuleRun): string {
  return (run.durationMs / 1000).toFixed(1)
}

/** Where the file went or what happened beside the action. */
function detailOf(item: SyncRuleRunItem): string | null {
  if (item.trashKind === 'system') return t('files.syncRules.log.systemTrash')
  if (item.trashKind === 'rule' && item.trashPath) return item.trashPath
  if (item.action === 'skip' && item.detail) {
    return t(`files.syncRules.log.skip.${item.detail}`)
  }
  return item.detail
}
</script>

<template>
  <UiDrawerModal
    v-model:open="open"
    :title="t('files.syncRules.log.title', { name: view?.rule.name ?? '' })"
  >
    <template #content>
      <div class="flex flex-col gap-2" data-testid="files-sync-rule-log">
        <p v-if="loading" class="text-sm text-muted-foreground">
          {{ t('files.syncRules.log.loading') }}
        </p>
        <p v-if="error" class="text-sm text-destructive" role="alert">
          {{ error }}
        </p>
        <p
          v-if="!loading && !error && runs.length === 0"
          class="text-sm text-muted-foreground"
        >
          {{ t('files.syncRules.log.empty') }}
        </p>
        <ul class="flex flex-col gap-2">
          <li
            v-for="(run, index) in runs"
            :key="run.id"
            class="flex flex-col gap-1 rounded-md border p-2"
            :data-testid="`files-sync-rule-log-run-${index}`"
          >
            <button
              type="button"
              class="flex flex-col items-start gap-0.5 text-left"
              :aria-expanded="expanded === run.id"
              :data-testid="`files-sync-rule-log-open-${index}`"
              @click="toggleAsync(run)"
            >
              <span class="text-sm font-medium">
                {{ started(run) }} ·
                {{ t(`files.syncRules.log.outcome.${run.outcome}`) }}
              </span>
              <span class="text-xs text-muted-foreground">
                {{ t(`files.syncRules.log.cause.${run.cause}`) }} ·
                {{
                  t('files.syncRules.log.duration', { seconds: seconds(run) })
                }}
                ·
                {{
                  t('files.syncRules.log.counts', {
                    copied: run.copied,
                    moved: run.moved,
                    deleted: run.deleted,
                    conflicts: run.conflicts,
                    errors: run.errors,
                    skipped: run.skipped,
                  })
                }}
              </span>
              <span v-if="run.error" class="text-xs text-warning">{{
                run.error
              }}</span>
            </button>
            <template v-if="expanded === run.id">
              <p
                v-if="items.length === 0"
                class="text-xs text-muted-foreground"
              >
                {{ t('files.syncRules.log.noFiles') }}
              </p>
              <ul class="flex flex-col gap-0.5">
                <li
                  v-for="item in items"
                  :key="item.seq"
                  class="flex min-w-0 gap-2 text-xs"
                  :data-testid="`files-sync-rule-log-item-${item.path}`"
                >
                  <span class="shrink-0 font-medium">{{
                    t(`files.syncRules.log.action.${item.action}`)
                  }}</span>
                  <span class="min-w-0 flex-1 truncate">{{ item.path }}</span>
                  <span
                    v-if="detailOf(item)"
                    class="truncate text-muted-foreground"
                    >{{ detailOf(item) }}</span
                  >
                  <span class="shrink-0 text-muted-foreground">{{
                    labels[item.side]
                  }}</span>
                </li>
              </ul>
              <UiButton
                v-if="more"
                type="button"
                variant="ghost"
                size="sm"
                class="self-start"
                @click="loadItemsAsync(run.id, items.length)"
              >
                {{ t('files.syncRules.log.more') }}
              </UiButton>
            </template>
          </li>
        </ul>
      </div>
    </template>
  </UiDrawerModal>
</template>
