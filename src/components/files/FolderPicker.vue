<script setup lang="ts">
/**
 * Picks a folder of the file browser as a place of a sync rule (spec 061 FR-033): a root (a place
 * or drive of this device, another own device, a storage), then down through its folders. A
 * contact's share is never offered (FR-002). It tells whether the chosen folder lies in an
 * encrypted folder, for the hints of the rule dialog (FR-030a, FR-031).
 */
import type { Entry } from '@bindings/Entry'
import type { SourceRef } from '@bindings/SourceRef'
import type { Sources } from '@bindings/Sources'
import { asFilesError } from '~/composables/useFiles'
import { childPath, lastName, parentPath } from '~/lib/files/syncRules'

export type PickedFolder = {
  source: SourceRef
  path: string
  encrypted: boolean
}

const props = defineProps<{
  sources: Sources | null
  /** Names the picker in `data-testid`s, for two pickers in one dialog. */
  name: string
}>()

const picked = defineModel<PickedFolder | null>({ required: true })

const { t } = useI18n()
const { listAsync } = useFiles()

type Root = { key: string; label: string; source: SourceRef; path: string }

const roots = computed<Root[]>(() => {
  const sources = props.sources
  if (!sources) return []
  const device: SourceRef = { kind: 'device' }
  return [
    ...sources.known.map((place) => ({
      key: `known:${place.path}`,
      label: t(`files.places.${place.name}`),
      source: device,
      path: place.path,
    })),
    ...sources.drives.map((drive) => ({
      key: `drive:${drive.path}`,
      label: drive.name,
      source: device,
      path: drive.path,
    })),
    ...sources.devices
      .filter((peer) => peer.state === 'reachable')
      .map((peer) => ({
        key: peer.id,
        label: peer.name ?? t('files.peer.unnamed'),
        source: { kind: 'storage', storageId: peer.id } as SourceRef,
        path: '/',
      })),
    ...sources.storages.map((storage) => ({
      key: storage.id,
      label: storage.name,
      source: { kind: 'storage', storageId: storage.id } as SourceRef,
      path: '/',
    })),
  ]
})

const rootOptions = computed(() =>
  roots.value.map((root) => ({ value: root.key, label: root.label })),
)

const rootKey = ref<string | undefined>(undefined)
/** The folders from the root down to the open one, each with whether it is encrypted. */
const trail = ref<{ path: string; encrypted: boolean }[]>([])
const folders = ref<Entry[]>([])
const error = ref<string | null>(null)
const loading = ref(false)
let generation = 0

const root = computed(() => roots.value.find((r) => r.key === rootKey.value))
const here = computed(() => trail.value.at(-1))

async function loadAsync() {
  const current = root.value
  const at = here.value
  if (!current || !at) return
  const mine = ++generation
  loading.value = true
  try {
    const entries = await listAsync(current.source, at.path)
    if (mine !== generation) return
    folders.value = entries.filter(
      (entry) => entry.kind === 'dir' && !entry.symlink && !entry.holziOwned,
    )
    error.value = null
  } catch (cause) {
    if (mine !== generation) return
    folders.value = []
    const code = asFilesError(cause)?.code
    error.value = code ? t(`files.error.${code}`) : t('files.syncRules.failed')
  } finally {
    if (mine === generation) loading.value = false
  }
}

function emitPicked() {
  const current = root.value
  const at = here.value
  picked.value =
    current && at
      ? { source: current.source, path: at.path, encrypted: at.encrypted }
      : null
}

watch(rootKey, () => {
  const current = root.value
  trail.value = current ? [{ path: current.path, encrypted: false }] : []
  emitPicked()
  void loadAsync()
})

function enter(entry: Entry) {
  const at = here.value
  if (!at) return
  trail.value = [
    ...trail.value,
    {
      path: childPath(at.path, entry.name),
      encrypted: at.encrypted || entry.encrypted !== null,
    },
  ]
  emitPicked()
  void loadAsync()
}

function up() {
  if (trail.value.length <= 1) return
  trail.value = trail.value.slice(0, -1)
  emitPicked()
  void loadAsync()
}

const canGoUp = computed(
  () => trail.value.length > 1 && parentPath(here.value?.path ?? '') !== null,
)
</script>

<template>
  <div class="flex flex-col gap-2" :data-testid="`files-picker-${name}`">
    <UiSelect
      v-model="rootKey"
      :options="rootOptions"
      :placeholder="t('files.syncRules.picker.root')"
      :aria-label="t('files.syncRules.picker.root')"
      :data-testid="`files-picker-${name}-root`"
    />
    <div v-if="here" class="flex flex-col gap-1 rounded-md border p-2">
      <div class="flex items-center gap-2 text-sm">
        <UiButton
          type="button"
          variant="ghost"
          size="icon-sm"
          :disabled="!canGoUp"
          :aria-label="t('files.syncRules.picker.up')"
          :tooltip="t('files.syncRules.picker.up')"
          :data-testid="`files-picker-${name}-up`"
          @click="up"
        >
          <Icon name="lucide:arrow-up" class="size-4" />
        </UiButton>
        <Icon
          v-if="here.encrypted"
          name="lucide:lock"
          class="size-4 shrink-0 text-muted-foreground"
        />
        <span
          class="min-w-0 flex-1 truncate font-medium"
          :data-testid="`files-picker-${name}-path`"
          >{{ here.path }}</span
        >
      </div>
      <p v-if="error" class="text-sm text-destructive" role="alert">
        {{ error }}
      </p>
      <p
        v-else-if="!loading && folders.length === 0"
        class="px-2 text-xs text-muted-foreground"
      >
        {{ t('files.syncRules.picker.empty') }}
      </p>
      <ul class="max-h-48 overflow-y-auto">
        <li v-for="folder in folders" :key="folder.path">
          <button
            type="button"
            class="flex w-full items-center gap-2 rounded px-2 py-1 text-left text-sm hover:bg-accent"
            :data-testid="`files-picker-${name}-folder-${lastName(folder.name)}`"
            @click="enter(folder)"
          >
            <Icon
              :name="folder.encrypted ? 'lucide:folder-lock' : 'lucide:folder'"
              class="size-4 shrink-0"
            />
            <span class="truncate">{{ folder.name }}</span>
          </button>
        </li>
      </ul>
    </div>
  </div>
</template>
