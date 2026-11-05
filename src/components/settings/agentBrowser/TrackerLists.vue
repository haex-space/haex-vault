<script setup lang="ts">
/**
 * "Tracker blockieren" in "KI → Browser" (spec 065 FR-045, research R16): blocking as a whole, each
 * filter list with its state (downloaded when, or why not), a switch per list, and the user's own
 * lists by their https address. A change applies on selection (023 FR-021) and runs the guardrail
 * action `settings.agentBrowser.trackers.set`, which no agent can call; Rust checks the choice
 * again and answers the state of the lists. Errors are toasts.
 */
import { invoke } from '@tauri-apps/api/core'
import { toast } from 'vue-sonner'
import type { TrackerOverview } from '~/types/bindings/TrackerOverview'
import type { TrackerSettings } from '~/types/bindings/TrackerSettings'

const { t, locale } = useI18n()
const { errString } = useErrorString()
const fieldLabels = useFieldLabels()
const setTrackers = useActionOrThrow('settings.agentBrowser.trackers.set')

const overview = ref<TrackerOverview | null>(null)
const draftUrl = ref('')
const draftName = ref('')
const adding = ref(false)
/** Reads the lists again while the first download runs, so the notice goes away by itself. */
let refresh: ReturnType<typeof setInterval> | null = null

async function reloadAsync() {
  try {
    overview.value = await invoke<TrackerOverview>('agent_browser_trackers')
  } catch (error) {
    toast.error(errString(error))
  }
}

function watchLoading(loading: boolean) {
  if (loading && refresh === null) {
    refresh = setInterval(() => void reloadAsync(), 3000)
  } else if (!loading && refresh !== null) {
    clearInterval(refresh)
    refresh = null
  }
}

watch(() => overview.value?.loading === true, watchLoading)
onMounted(reloadAsync)
onBeforeUnmount(() => watchLoading(false))

/** The choice the page shows, as Rust keeps it. */
function current(): TrackerSettings {
  const lists = overview.value?.lists ?? []
  return {
    enabled: overview.value?.enabled ?? true,
    off: lists.filter((l) => !l.custom && !l.enabled).map((l) => l.id),
    custom: lists
      .filter((l) => l.custom)
      .map((l) => ({
        id: l.id,
        name: l.name,
        url: l.url,
        enabled: l.enabled,
      })),
  }
}

/** Changes wait for each other: each one starts from the state the one before it left. */
let queue: Promise<unknown> = Promise.resolve()

/**
 * Sends the choice `change` makes of the one shown; `false` when it failed (shown as a toast).
 * `change` runs when it is its turn, so two quick switches do not undo each other.
 */
function changeAsync(
  change: (choice: TrackerSettings) => TrackerSettings,
): Promise<boolean> {
  const run = async (): Promise<boolean> => {
    try {
      overview.value = (await setTrackers({
        settings: change(current()),
      })) as TrackerOverview
      return true
    } catch (error) {
      toast.error(errString(error))
      await reloadAsync()
      return false
    }
  }
  const result = queue.then(run)
  queue = result
  return result
}

function setEnabled(enabled: boolean) {
  void changeAsync((choice) => ({ ...choice, enabled }))
}

function setList(id: string, custom: boolean, enabled: boolean) {
  void changeAsync((next) => {
    if (custom) {
      return {
        ...next,
        custom: next.custom.map((l) => (l.id === id ? { ...l, enabled } : l)),
      }
    }
    const off = new Set(next.off)
    if (enabled) off.delete(id)
    else off.add(id)
    return { ...next, off: [...off] }
  })
}

function removeList(id: string) {
  void changeAsync((next) => ({
    ...next,
    custom: next.custom.filter((l) => l.id !== id),
  }))
}

async function addAsync() {
  const url = draftUrl.value.trim()
  if (!url || adding.value) return
  const name = draftName.value.trim()
  adding.value = true
  try {
    const added = await changeAsync((next) => ({
      ...next,
      custom: [...next.custom, { id: '', name, url, enabled: true }],
    }))
    if (added) {
      draftUrl.value = ''
      draftName.value = ''
    }
  } finally {
    adding.value = false
  }
}

function stateOf(list: TrackerOverview['lists'][number]): string {
  if (list.error)
    return t('agentBrowser.settings.trackers.error', { error: list.error })
  if (list.fetchedAt == null)
    return t('agentBrowser.settings.trackers.notFetched')
  return t('agentBrowser.settings.trackers.fetched', {
    date: new Date(list.fetchedAt).toLocaleDateString(locale.value),
  })
}
</script>

<template>
  <template v-if="overview">
    <p class="px-1 text-sm font-medium">
      {{ t('agentBrowser.settings.trackers.title') }}
    </p>
    <SettingsGroup data-testid="agent-browser-trackers">
      <SettingsRow
        :title="t('agentBrowser.settings.trackers.enabled')"
        :description="t('agentBrowser.settings.trackers.enabledHint')"
        label-for="agent-browser-trackers-switch"
      >
        <ShadcnSwitch
          id="agent-browser-trackers-switch"
          :model-value="overview.enabled"
          data-testid="agent-browser-trackers-switch"
          @update:model-value="setEnabled($event === true)"
        />
      </SettingsRow>
    </SettingsGroup>
    <template v-if="overview.enabled">
      <p
        v-if="overview.loading"
        class="rounded-xl bg-muted px-3 py-2 text-sm"
        role="status"
        data-testid="agent-browser-trackers-loading"
      >
        {{ t('agentBrowser.settings.trackers.loading') }}
      </p>
      <SettingsGroup :label="t('agentBrowser.settings.trackers.lists')">
        <SettingsRow
          v-for="list in overview.lists"
          :key="list.id"
          :title="list.name"
          :description="stateOf(list)"
          data-testid="agent-browser-tracker-list"
          :data-list="list.id"
        >
          <div class="flex flex-wrap items-center gap-2">
            <UiButton
              v-if="list.custom"
              type="button"
              size="sm"
              variant="outline"
              :data-testid="`agent-browser-tracker-remove-${list.id}`"
              @click="removeList(list.id)"
            >
              {{ t('agentBrowser.settings.trackers.remove') }}
            </UiButton>
            <ShadcnSwitch
              :model-value="list.enabled"
              :aria-label="list.name"
              :data-testid="`agent-browser-tracker-switch-${list.id}`"
              @update:model-value="
                setList(list.id, list.custom, $event === true)
              "
            />
          </div>
        </SettingsRow>
        <div class="flex flex-col gap-2 p-4">
          <p class="text-sm font-medium">
            {{ t('agentBrowser.settings.trackers.addTitle') }}
          </p>
          <div class="flex flex-wrap items-end gap-2">
            <UiInput
              id="agent-browser-tracker-url"
              v-model="draftUrl"
              class="min-w-0 flex-1"
              :label="t('agentBrowser.settings.trackers.addUrl')"
              :labels="fieldLabels.input.value"
              label-bg="var(--muted)"
              type="url"
              autocomplete="off"
              placeholder="https://lists.example/filters.txt"
              data-testid="agent-browser-tracker-url"
              @keydown.enter.prevent="addAsync"
            />
            <UiInput
              id="agent-browser-tracker-name"
              v-model="draftName"
              class="min-w-0 flex-1"
              :label="t('agentBrowser.settings.trackers.addName')"
              :labels="fieldLabels.input.value"
              label-bg="var(--muted)"
              autocomplete="off"
              data-testid="agent-browser-tracker-name"
              @keydown.enter.prevent="addAsync"
            />
            <UiButton
              type="button"
              :disabled="!draftUrl.trim() || adding"
              data-testid="agent-browser-tracker-add"
              @click="addAsync"
            >
              {{ t('agentBrowser.settings.trackers.add') }}
            </UiButton>
          </div>
          <p class="text-xs text-muted-foreground">
            {{ t('agentBrowser.settings.trackers.defaultHint') }}
            {{ t('agentBrowser.settings.trackers.addHint') }}
          </p>
        </div>
      </SettingsGroup>
    </template>
  </template>
</template>
