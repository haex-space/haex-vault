<script setup lang="ts">
/**
 * "Sync-Regeln" (spec 061 US4, FR-025, FR-032, FR-040): the rules of this device as a list in the
 * panel of the file browser, laid out like a folder: a header to sort by, one row per rule with its
 * places, state, progress or last run, and the actions; a menu opens the preview, the exclusions,
 * the log and the dialog to change the rule; a click opens the details of a row. A filter narrows
 * many rules down. Deleting a rule leaves the files of both places alone (FR-006).
 */
import type { Sources } from '@bindings/Sources'
import type { SyncRulePlace } from '@bindings/SyncRulePlace'
import type { SyncRuleView } from '@bindings/SyncRuleView'
import { asSyncRuleError } from '~/composables/useSyncRules'
import {
  filterRules,
  progressShare,
  RULE_SORT_KEYS,
  type RuleRow,
  type RuleSort,
  type RuleSortKey,
  sortRules,
} from '~/lib/files/syncRules'
import { formatFileSize } from '~/lib/passwords/format'
import { useSyncRulesStore } from '~/stores/syncRules'

const props = defineProps<{
  sources: Sources | null
}>()

const { t, d } = useI18n()
const store = useSyncRulesStore()
const { setEnabledAsync, deleteAsync, runNowAsync } = useSyncRules()

const creating = ref(false)
const busy = ref<string | null>(null)
const deleting = ref<SyncRuleView | null>(null)
/** The rule the dialog changes; `null` for a new one. */
const editing = ref<SyncRuleView | null>(null)
/** The rule whose preview, exclusions or log is open. */
const shown = ref<SyncRuleView | null>(null)
const previewing = ref(false)
const ignoring = ref(false)
const logging = ref(false)
const expanded = ref(new Set<string>())
const filter = ref('')
const sort = ref<RuleSort>({ key: 'name', ascending: true })

onMounted(() => {
  store.initialize()
  void store.startListening()
  void store.loadAsync()
})

/** The name a place shows: a storage or device by its name, the device by its path. */
function placeLabel(place: SyncRulePlace): string {
  if (place.source.kind === 'device') return place.path
  const id = place.source.storageId
  const named =
    props.sources?.storages.find((storage) => storage.id === id)?.name ??
    props.sources?.devices.find((device) => device.id === id)?.name ??
    t('files.syncRules.unknownSource')
  return place.path === '/' ? named : `${named} ${place.path}`
}

function labelsOf(view: SyncRuleView | null) {
  return view
    ? { a: placeLabel(view.rule.placeA), b: placeLabel(view.rule.placeB) }
    : { a: '', b: '' }
}

function openNew() {
  editing.value = null
  creating.value = true
}

function openEdit(view: SyncRuleView) {
  editing.value = view
  creating.value = true
}

function openShown(view: SyncRuleView, which: 'preview' | 'ignore' | 'log') {
  shown.value = view
  previewing.value = which === 'preview'
  ignoring.value = which === 'ignore'
  logging.value = which === 'log'
}

const ARROWS = { aToB: '→', bToA: '←', both: '↔' } as const

function placesOf(view: SyncRuleView): string {
  return `${placeLabel(view.rule.placeA)} ${ARROWS[view.rule.direction]} ${placeLabel(view.rule.placeB)}`
}

type Row = RuleRow & { view: SyncRuleView }

const rows = computed<Row[]>(() => {
  const all = store.rules.map((view) => ({
    view,
    name: view.rule.name,
    state: view.status.state,
    lastRunAt: view.rule.lastRunAt,
    places: placesOf(view),
  }))
  return sortRules(filterRules(all, filter.value), sort.value)
})

function pickSort(key: RuleSortKey) {
  sort.value = {
    key,
    ascending: sort.value.key === key ? !sort.value.ascending : true,
  }
}

function toggleDetails(id: string) {
  const next = new Set(expanded.value)
  if (!next.delete(id)) next.add(id)
  expanded.value = next
}

function lastRun(view: SyncRuleView): string {
  const at = view.rule.lastRunAt
  return at
    ? t('files.syncRules.lastRun', {
        at: d(new Date(at), { dateStyle: 'short', timeStyle: 'short' }),
      })
    : t('files.syncRules.neverRun')
}

/** The bar and text of a running rule; none while another state shows. */
function running(view: SyncRuleView) {
  if (view.status.state !== 'running') return null
  const progress = store.progress[view.rule.id]
  const share = progress ? progressShare(progress) : null
  // No report yet (the list opened after the run began) is not "comparing": it only says it runs.
  const text =
    progress?.phase === 'copying' && progress.filesTotal > 0
      ? t('files.syncRules.progress.files', {
          done: progress.filesDone,
          total: progress.filesTotal,
          size: `${formatFileSize(progress.bytesDone)} / ${formatFileSize(progress.bytesTotal)}`,
        })
      : progress?.phase === 'scanning'
        ? t('files.syncRules.progress.scanning')
        : t('files.syncRules.progress.running')
  return { share, text }
}

const activity = computed(() =>
  Object.fromEntries(store.rules.map((view) => [view.rule.id, running(view)])),
)

async function act(view: SyncRuleView, work: () => Promise<unknown>) {
  if (busy.value) return
  busy.value = view.rule.id
  try {
    await work()
    await store.loadAsync()
  } catch (cause) {
    const code = asSyncRuleError(cause)?.code
    store.error = code
      ? t(`files.syncRules.error.${code}`)
      : t('files.syncRules.failed')
  } finally {
    busy.value = null
  }
}

async function confirmDeleteAsync() {
  const view = deleting.value
  deleting.value = null
  // action-exempt: sync-rule controls are window-local; no sync-rule action catalog exists yet.
  if (view) await act(view, () => deleteAsync(view.rule.id))
}
</script>

<template>
  <div class="flex min-h-0 flex-col" data-testid="files-sync-rules">
    <div class="flex h-11 shrink-0 items-center gap-2 px-2">
      <UiInput
        v-model="filter"
        class="min-w-0 flex-1"
        type="search"
        :placeholder="t('files.syncRules.filter')"
        :aria-label="t('files.syncRules.filter')"
        data-testid="files-sync-rules-filter"
        @keydown.esc="filter = ''"
      >
        <template #prepend>
          <ShadcnInputGroupAddon align="inline-start">
            <Icon name="lucide:search" class="size-4" />
          </ShadcnInputGroupAddon>
        </template>
      </UiInput>
      <UiButton
        type="button"
        size="sm"
        class="shrink-0"
        data-testid="files-sync-rules-new"
        @click="openNew"
      >
        <Icon name="lucide:plus" class="size-4" />
        {{ t('files.syncRules.create.open') }}
      </UiButton>
    </div>
    <p
      v-if="store.error"
      class="px-3 pb-2 text-sm text-destructive"
      role="alert"
      data-testid="files-sync-rules-error"
    >
      {{ store.error }}
    </p>
    <div
      v-if="!store.loading && store.rules.length === 0"
      class="flex flex-1 items-center justify-center p-6 text-sm text-muted-foreground"
      data-testid="files-sync-rules-empty"
    >
      {{ t('files.syncRules.empty') }}
    </div>
    <div
      v-else-if="!store.loading && rows.length === 0"
      class="flex flex-1 items-center justify-center p-6 text-sm text-muted-foreground"
    >
      {{ t('files.syncRules.noMatch') }}
    </div>
    <template v-else>
      <div
        class="flex shrink-0 items-center gap-3 border-b px-3 py-1 text-xs text-muted-foreground"
      >
        <button
          v-for="key in RULE_SORT_KEYS"
          :key="key"
          type="button"
          class="flex items-center gap-1 hover:text-foreground"
          :class="[
            key === 'name' && 'min-w-0 flex-1',
            key === 'status' && 'w-28 shrink-0',
            key === 'lastRun' && 'hidden w-56 shrink-0 @md:flex',
          ]"
          :data-testid="`files-sync-rules-sort-${key}`"
          @click="pickSort(key)"
        >
          {{ t(`files.syncRules.columns.${key}`) }}
          <Icon
            v-if="sort.key === key"
            :name="sort.ascending ? 'lucide:arrow-up' : 'lucide:arrow-down'"
            class="size-3"
          />
        </button>
        <span class="w-36 shrink-0" aria-hidden="true" />
      </div>
      <ul class="min-h-0 flex-1 overflow-y-auto" role="list">
        <li
          v-for="{ view } in rows"
          :key="view.rule.id"
          class="border-b"
          :data-testid="`files-sync-rule-${view.rule.name}`"
        >
          <div
            class="flex items-center gap-3 px-3 py-2 text-sm hover:bg-accent"
          >
            <button
              type="button"
              class="flex min-w-0 flex-1 flex-col text-left focus-visible:outline-none"
              :aria-expanded="expanded.has(view.rule.id)"
              :data-testid="`files-sync-rule-open-${view.rule.name}`"
              @click="toggleDetails(view.rule.id)"
            >
              <span class="truncate font-medium">{{ view.rule.name }}</span>
              <span class="truncate text-xs text-muted-foreground">{{
                placesOf(view)
              }}</span>
            </button>
            <span class="w-28 shrink-0">
              <ShadcnBadge
                variant="secondary"
                :data-testid="`files-sync-rule-state-${view.rule.name}`"
                >{{
                  t(`files.syncRules.state.${view.status.state}`)
                }}</ShadcnBadge
              >
            </span>
            <div
              class="hidden w-56 shrink-0 flex-col gap-1 text-xs text-muted-foreground @md:flex"
            >
              <template v-if="activity[view.rule.id]">
                <div
                  class="h-1 overflow-hidden rounded bg-muted"
                  role="progressbar"
                  :aria-label="t('files.syncRules.progress.label')"
                  :aria-valuemin="0"
                  :aria-valuemax="100"
                  :aria-valuenow="
                    activity[view.rule.id]?.share == null
                      ? undefined
                      : Math.round((activity[view.rule.id]?.share ?? 0) * 100)
                  "
                  :data-testid="`files-sync-rule-progress-${view.rule.name}`"
                >
                  <div
                    v-if="activity[view.rule.id]?.share != null"
                    class="h-full bg-primary transition-[width]"
                    :style="{
                      width: `${Math.round((activity[view.rule.id]?.share ?? 0) * 100)}%`,
                    }"
                  />
                  <div v-else class="h-full w-1/3 animate-pulse bg-primary" />
                </div>
                <span class="truncate">{{ activity[view.rule.id]?.text }}</span>
              </template>
              <span v-else class="truncate">{{ lastRun(view) }}</span>
            </div>
            <div class="flex w-36 shrink-0 justify-end">
              <UiButton
                type="button"
                variant="ghost"
                size="icon-sm"
                :disabled="busy !== null || !view.rule.enabled"
                :aria-label="t('files.syncRules.runNow')"
                :tooltip="t('files.syncRules.runNow')"
                :data-testid="`files-sync-rule-run-${view.rule.name}`"
                @click="act(view, () => runNowAsync(view.rule.id))"
              >
                <Icon name="lucide:refresh-cw" class="size-4" />
              </UiButton>
              <UiButton
                type="button"
                variant="ghost"
                size="icon-sm"
                :disabled="busy !== null"
                :aria-label="
                  view.rule.enabled
                    ? t('files.syncRules.pause')
                    : t('files.syncRules.resume')
                "
                :tooltip="
                  view.rule.enabled
                    ? t('files.syncRules.pause')
                    : t('files.syncRules.resume')
                "
                :data-testid="`files-sync-rule-toggle-${view.rule.name}`"
                @click="
                  act(view, () =>
                    setEnabledAsync(view.rule.id, !view.rule.enabled),
                  )
                "
              >
                <Icon
                  :name="view.rule.enabled ? 'lucide:pause' : 'lucide:play'"
                  class="size-4"
                />
              </UiButton>
              <ShadcnDropdownMenu>
                <ShadcnDropdownMenuTrigger as-child>
                  <UiButton
                    type="button"
                    variant="ghost"
                    size="icon-sm"
                    :aria-label="t('files.syncRules.more')"
                    :tooltip="t('files.syncRules.more')"
                    :data-testid="`files-sync-rule-more-${view.rule.name}`"
                  >
                    <Icon name="lucide:ellipsis-vertical" class="size-4" />
                  </UiButton>
                </ShadcnDropdownMenuTrigger>
                <ShadcnDropdownMenuContent align="end">
                  <ShadcnDropdownMenuItem
                    class="gap-2"
                    :data-testid="`files-sync-rule-preview-${view.rule.name}`"
                    @select="openShown(view, 'preview')"
                  >
                    <Icon name="lucide:list-checks" class="size-4" />
                    {{ t('files.syncRules.plan.open') }}
                  </ShadcnDropdownMenuItem>
                  <ShadcnDropdownMenuItem
                    class="gap-2"
                    :data-testid="`files-sync-rule-ignore-${view.rule.name}`"
                    @select="openShown(view, 'ignore')"
                  >
                    <Icon name="lucide:filter-x" class="size-4" />
                    {{ t('files.syncRules.ignore.open') }}
                  </ShadcnDropdownMenuItem>
                  <ShadcnDropdownMenuItem
                    class="gap-2"
                    :data-testid="`files-sync-rule-log-${view.rule.name}`"
                    @select="openShown(view, 'log')"
                  >
                    <Icon name="lucide:history" class="size-4" />
                    {{ t('files.syncRules.log.open') }}
                  </ShadcnDropdownMenuItem>
                  <ShadcnDropdownMenuItem
                    class="gap-2"
                    :data-testid="`files-sync-rule-edit-${view.rule.name}`"
                    @select="openEdit(view)"
                  >
                    <Icon name="lucide:pencil" class="size-4" />
                    {{ t('files.syncRules.edit.open') }}
                  </ShadcnDropdownMenuItem>
                </ShadcnDropdownMenuContent>
              </ShadcnDropdownMenu>
              <UiButton
                type="button"
                variant="ghost"
                size="icon-sm"
                :disabled="busy !== null"
                :aria-label="t('files.syncRules.delete')"
                :tooltip="t('files.syncRules.delete')"
                :data-testid="`files-sync-rule-delete-${view.rule.name}`"
                @click="deleting = view"
              >
                <Icon name="lucide:trash-2" class="size-4" />
              </UiButton>
            </div>
          </div>
          <div
            v-if="expanded.has(view.rule.id)"
            class="flex flex-col gap-1 bg-muted/30 px-3 py-2 text-xs text-muted-foreground"
            :data-testid="`files-sync-rule-details-${view.rule.name}`"
          >
            <span>{{ lastRun(view) }}</span>
            <span>
              {{ t('files.syncRules.direction.label') }}:
              {{ t(`files.syncRules.direction.${view.rule.direction}`) }}
              · {{ t('files.syncRules.deleteMode.label') }}:
              {{ t(`files.syncRules.deleteMode.${view.rule.deleteMode}`) }}
            </span>
            <FilesSyncRulePending
              v-if="view.status.pendingDeletes > 0"
              :view="view"
              :labels="labelsOf(view)"
              @changed="store.loadAsync()"
            />
            <div
              v-if="view.status.conflictCopies.length"
              class="flex flex-col gap-1"
              :data-testid="`files-sync-rule-conflicts-${view.rule.name}`"
            >
              <span class="text-xs text-muted-foreground">{{
                t(
                  'files.syncRules.conflicts',
                  view.status.conflictCopies.length,
                )
              }}</span>
              <ul class="flex flex-col gap-1">
                <li
                  v-for="copy in view.status.conflictCopies"
                  :key="copy"
                  class="truncate text-xs"
                >
                  {{ copy }}
                </li>
              </ul>
            </div>
            <span
              v-if="
                view.status.reason &&
                (view.status.state === 'waiting' ||
                  view.status.state === 'error')
              "
              class="text-warning"
              >{{ view.status.reason }}</span
            >
            <span v-if="view.status.deviations > 0">{{
              t('files.syncRules.deviations', {
                count: view.status.deviations,
              })
            }}</span>
            <ul v-if="view.status.errors.length" class="flex flex-col gap-1">
              <li
                v-for="failure in view.status.errors"
                :key="failure.path"
                class="text-destructive"
              >
                {{ failure.path }}: {{ failure.reason }}
              </li>
            </ul>
          </div>
        </li>
      </ul>
    </template>
    <FilesSyncRuleDialog
      v-model:open="creating"
      :sources="sources"
      :rule="editing?.rule ?? null"
      :labels="labelsOf(editing)"
      @saved="store.loadAsync()"
    />
    <FilesSyncRulePlan
      v-model:open="previewing"
      :target="shown ? { id: shown.rule.id } : null"
      :labels="labelsOf(shown)"
    />
    <FilesSyncRuleIgnoreEditor v-model:open="ignoring" :view="shown" />
    <FilesSyncRuleLog
      v-model:open="logging"
      :view="shown"
      :labels="labelsOf(shown)"
    />
    <ShadcnAlertDialog
      :open="deleting !== null"
      @update:open="(value: boolean) => (deleting = value ? deleting : null)"
    >
      <ShadcnAlertDialogContent>
        <ShadcnAlertDialogHeader>
          <ShadcnAlertDialogTitle>{{
            t('files.syncRules.deleteTitle')
          }}</ShadcnAlertDialogTitle>
          <ShadcnAlertDialogDescription>{{
            t('files.syncRules.deleteText', { name: deleting?.rule.name ?? '' })
          }}</ShadcnAlertDialogDescription>
        </ShadcnAlertDialogHeader>
        <ShadcnAlertDialogFooter>
          <ShadcnAlertDialogCancel>{{
            t('files.cancel')
          }}</ShadcnAlertDialogCancel>
          <UiButton
            variant="destructive"
            data-testid="files-sync-rule-delete-confirm"
            @click="confirmDeleteAsync"
          >
            {{ t('files.syncRules.delete') }}
          </UiButton>
        </ShadcnAlertDialogFooter>
      </ShadcnAlertDialogContent>
    </ShadcnAlertDialog>
  </div>
</template>
