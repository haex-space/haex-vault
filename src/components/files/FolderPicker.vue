<script setup lang="ts">
/**
 * Picks a folder of the file browser as a place of a sync rule (spec 061 FR-033) in two steps:
 * first the device or storage (this device, another own device, a storage), then down through its
 * folders. On this device the folders start with its places and drives. Browsing does not pick:
 * the open folder is taken with "Choose this folder", so a folder with subfolders can be chosen
 * as well. A contact's share is never offered (FR-002). It tells whether the chosen folder lies in an encrypted folder, for the
 * hints of the rule dialog (FR-030a, FR-031).
 */
import type { Entry } from '@bindings/Entry'
import type { SourceRef } from '@bindings/SourceRef'
import type { Sources } from '@bindings/Sources'
import { asFilesError } from '~/composables/useFiles'
import { childPath, lastName } from '~/lib/files/syncRules'

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

type Origin = { key: string; label: string; source: SourceRef }
/** A folder to open: a place of this device, or a subfolder of the open one. */
type Row = {
  key: string
  name: string
  label: string
  encrypted: boolean
  path: string
}

const DEVICE_KEY = 'device'

const origins = computed<Origin[]>(() => {
  const sources = props.sources
  if (!sources) return []
  return [
    {
      key: DEVICE_KEY,
      label: t('files.syncRules.picker.thisDevice'),
      source: { kind: 'device' },
    },
    ...sources.devices
      .filter((peer) => peer.state === 'reachable')
      .map((peer) => ({
        key: `peer:${peer.id}`,
        label: peer.name ?? t('files.peer.unnamed'),
        source: { kind: 'storage', storageId: peer.id } as SourceRef,
      })),
    ...sources.storages.map((storage) => ({
      key: `storage:${storage.id}`,
      label: storage.name,
      source: { kind: 'storage', storageId: storage.id } as SourceRef,
    })),
  ]
})

/** The places and drives this device starts with. */
const devicePlaces = computed<Row[]>(() => {
  const sources = props.sources
  if (!sources) return []
  return [
    ...sources.known.map((place) => ({
      key: `known:${place.path}`,
      name: place.name,
      label: t(`files.places.${place.name}`),
      encrypted: false,
      path: place.path,
    })),
    // Only mounted drives have a folder to pick (spec 064).
    ...sources.drives.flatMap((drive) =>
      drive.path === null
        ? []
        : [
            {
              key: `drive:${drive.id}`,
              name: drive.label ?? drive.path,
              label: drive.label ?? drive.path,
              encrypted: false,
              path: drive.path,
            },
          ],
    ),
  ]
})

const originOptions = computed(() =>
  origins.value.map((origin) => ({ value: origin.key, label: origin.label })),
)

const originKey = ref<string | undefined>(undefined)
/** The folder list is open until a folder is taken; then only the choice is shown. */
const browsing = ref(true)
/** The folders from the start down to the open one, each with whether it is encrypted. */
const trail = ref<{ path: string; encrypted: boolean }[]>([])
const folders = ref<Entry[]>([])
const error = ref<string | null>(null)
const loading = ref(false)
let generation = 0

const origin = computed(() =>
  origins.value.find((o) => o.key === originKey.value),
)
const here = computed(() => trail.value.at(-1))
/** This device before a place is opened: the places are listed, none is picked yet. */
const atDeviceTop = computed(
  () => originKey.value === DEVICE_KEY && trail.value.length === 0,
)

const rows = computed<Row[]>(() =>
  atDeviceTop.value
    ? devicePlaces.value
    : folders.value.map((entry) => ({
        key: entry.path,
        name: entry.name,
        label: entry.name,
        encrypted: entry.encrypted !== null,
        path: childPath(here.value?.path ?? '', entry.name),
      })),
)

async function loadAsync() {
  const current = origin.value
  const at = here.value
  const mine = ++generation
  if (!current || !at) {
    folders.value = []
    error.value = null
    loading.value = false
    return
  }
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
  const current = origin.value
  const at = here.value
  picked.value =
    current && at
      ? { source: current.source, path: at.path, encrypted: at.encrypted }
      : null
}

watch(originKey, () => {
  trail.value =
    originKey.value && originKey.value !== DEVICE_KEY
      ? [{ path: '/', encrypted: false }]
      : []
  picked.value = null
  browsing.value = true
  void loadAsync()
})

function enter(row: Row) {
  const at = here.value
  trail.value = [
    ...trail.value,
    { path: row.path, encrypted: (at?.encrypted ?? false) || row.encrypted },
  ]
  void loadAsync()
}

function up() {
  if (!canGoUp.value) return
  trail.value = trail.value.slice(0, -1)
  void loadAsync()
}

/** Takes the open folder as the place and closes the folder list. */
function choose() {
  emitPicked()
  if (picked.value) browsing.value = false
}

/** A storage starts at its root; this device goes back up to its list of places. */
const canGoUp = computed(
  () =>
    trail.value.length > 1 ||
    (originKey.value === DEVICE_KEY && trail.value.length === 1),
)
</script>

<template>
  <div class="flex flex-col gap-2" :data-testid="`files-picker-${name}`">
    <UiSelect
      v-model="originKey"
      :options="originOptions"
      :placeholder="t('files.syncRules.picker.origin')"
      :aria-label="t('files.syncRules.picker.origin')"
      :data-testid="`files-picker-${name}-root`"
    />
    <div
      v-if="origin && !browsing && picked"
      class="flex items-center gap-2 rounded-md border p-2 text-sm"
      :data-testid="`files-picker-${name}-chosen`"
    >
      <Icon
        :name="picked.encrypted ? 'lucide:folder-lock' : 'lucide:folder-check'"
        class="size-4 shrink-0"
      />
      <span
        class="min-w-0 flex-1 truncate font-medium"
        :data-testid="`files-picker-${name}-path`"
        >{{ picked.path }}</span
      >
      <UiButton
        type="button"
        variant="ghost"
        size="sm"
        :data-testid="`files-picker-${name}-change`"
        @click="browsing = true"
        >{{ t('files.syncRules.picker.change') }}</UiButton
      >
    </div>
    <div v-else-if="origin" class="flex flex-col gap-1 rounded-md border p-2">
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
          v-if="here?.encrypted"
          name="lucide:lock"
          class="size-4 shrink-0 text-muted-foreground"
        />
        <span
          class="min-w-0 flex-1 truncate font-medium"
          :class="{ 'text-muted-foreground': !here }"
          :data-testid="`files-picker-${name}-path`"
          >{{ here?.path ?? t('files.syncRules.picker.chooseFolder') }}</span
        >
      </div>
      <p v-if="error" class="text-sm text-destructive" role="alert">
        {{ error }}
      </p>
      <p
        v-else-if="!loading && rows.length === 0"
        class="px-2 text-xs text-muted-foreground"
      >
        {{ t('files.syncRules.picker.empty') }}
      </p>
      <ul class="max-h-48 overflow-y-auto">
        <li v-for="row in rows" :key="row.key">
          <button
            type="button"
            class="flex w-full items-center gap-2 rounded px-2 py-1 text-left text-sm hover:bg-accent"
            :data-testid="`files-picker-${name}-folder-${lastName(row.name)}`"
            @click="enter(row)"
          >
            <Icon
              :name="row.encrypted ? 'lucide:folder-lock' : 'lucide:folder'"
              class="size-4 shrink-0"
            />
            <span class="truncate">{{ row.label }}</span>
          </button>
        </li>
      </ul>
      <UiButton
        type="button"
        size="sm"
        class="self-end"
        :disabled="!here"
        :data-testid="`files-picker-${name}-choose`"
        @click="choose"
        >{{ t('files.syncRules.picker.choose') }}</UiButton
      >
    </div>
  </div>
</template>
