<template>
  <div class="space-y-4">
    <!-- Storage backend dropdown -->
    <div class="space-y-2">
      <label class="text-sm font-medium">{{ t('backendLabel') }}</label>
      <USelectMenu
        v-model="selectedBackend"
        :items="backendOptions"
        :placeholder="t('backendPlaceholder')"
        :loading="loading"
        by="value"
        class="w-full"
      />
    </div>

    <!-- Scope selection -->
    <div
      v-if="selectedBackend"
      class="space-y-3"
    >
      <label class="text-sm font-medium">{{ t('scopeLabel') }}</label>

      <URadioGroup
        v-model="scopeMode"
        :items="scopeOptions"
      />

      <!-- Prefix summary line (only when in prefix mode) -->
      <div
        v-if="scopeMode === 'prefix'"
        class="text-xs text-muted"
      >
        <span v-if="selectedPrefix">
          {{ t('folderLabel', { path: selectedPrefix }) }}
        </span>
        <span v-else>{{ t('noFolderSelected') }}</span>
      </div>

      <!-- Folder tree browser -->
      <div class="rounded-md border border-muted">
        <!-- Breadcrumb -->
        <div
          class="flex flex-wrap items-center gap-1 px-3 py-2 border-b border-muted text-xs"
        >
          <button
            type="button"
            class="hover:underline"
            :class="currentPrefix === '' ? 'font-medium text-primary' : 'text-muted'"
            @click="navigateToPrefix('')"
          >
            {{ t('breadcrumbRoot') }}
          </button>
          <template
            v-for="(seg, i) in breadcrumbSegments"
            :key="i"
          >
            <span class="text-muted">/</span>
            <button
              type="button"
              class="hover:underline"
              :class="i === breadcrumbSegments.length - 1
                ? 'font-medium text-primary'
                : 'text-muted'"
              @click="navigateToSegment(i)"
            >
              {{ seg }}
            </button>
          </template>
        </div>

        <!-- Body -->
        <div class="min-h-[10rem] p-2">
          <!-- Loading -->
          <div
            v-if="isTreeLoading"
            class="flex items-center justify-center py-6 text-xs text-muted"
          >
            <UIcon
              name="i-lucide-loader-2"
              class="animate-spin mr-2"
            />
            {{ t('treeLoading') }}
          </div>

          <!-- Error -->
          <UAlert
            v-else-if="treeError"
            color="error"
            variant="soft"
            :icon="'i-lucide-alert-triangle'"
            :title="t('treeErrorTitle')"
            :description="treeError"
            :actions="[{
              label: t('treeRetry'),
              onClick: () => void loadTreeAsync(),
              color: 'error',
              variant: 'outline',
            }]"
          />

          <!-- Empty -->
          <div
            v-else-if="folders.length === 0 && files.length === 0"
            class="py-6 text-center text-xs text-muted"
          >
            {{ t('treeEmpty') }}
          </div>

          <!-- List -->
          <ul
            v-else
            class="space-y-1"
          >
            <li
              v-for="folder in folders"
              :key="'d:' + folder.path"
              class="flex items-center gap-2 px-2 py-1 rounded hover:bg-muted/40 group"
              :class="selectedPrefix === folder.path
                ? 'bg-primary/10 border border-primary/40'
                : ''"
            >
              <button
                type="button"
                class="flex items-center gap-2 flex-1 text-left text-sm"
                @click="navigateToPrefix(folder.path)"
              >
                <UIcon
                  name="i-lucide-folder"
                  class="text-primary shrink-0"
                />
                <span class="truncate">{{ folder.name }}</span>
              </button>
              <UButton
                size="xs"
                :color="selectedPrefix === folder.path ? 'primary' : 'neutral'"
                :variant="selectedPrefix === folder.path ? 'solid' : 'outline'"
                icon="i-lucide-share-2"
                @click="selectFolderForShare(folder.path)"
              >
                {{ t('shareThisFolder') }}
              </UButton>
            </li>

            <li
              v-for="file in files"
              :key="'f:' + file.key"
              class="flex items-center gap-2 px-2 py-1 rounded text-sm text-muted cursor-not-allowed"
              :title="t('fileDisabledTooltip')"
            >
              <UIcon
                name="i-lucide-lock"
                class="shrink-0"
              />
              <span class="truncate">{{ file.name }}</span>
            </li>
          </ul>
        </div>
      </div>
    </div>

    <!-- ARN preview -->
    <div
      v-if="selectedBackend"
      class="space-y-1"
    >
      <label class="text-xs uppercase tracking-wide text-muted">
        {{ t('arnPreviewLabel') }}
      </label>
      <code class="block text-xs break-all bg-gray-50 dark:bg-gray-800/50 px-2 py-1 rounded">
        {{ arnPreview }}
      </code>
    </div>
  </div>
</template>

<script setup lang="ts">
import { invoke } from '@tauri-apps/api/core'
import type { SelectHaexS3Backends } from '~/database/schemas'
import type { StorageListDirResponse } from '~/../src-tauri/bindings/StorageListDirResponse'

type BackendOption = {
  label: string
  value: string
  bucket: string
}

type FolderEntry = {
  /** Basename displayed to the user (no trailing slash). */
  name: string
  /** Full prefix from bucket root, ending with "/". Matches Rust ARN format. */
  path: string
}

type FileEntry = {
  /** Basename shown to the user. */
  name: string
  /** Full object key (for :key uniqueness only — files are not selectable). */
  key: string
}

type ScopeMode = 'whole' | 'prefix'

const props = defineProps<{
  backends: SelectHaexS3Backends[]
  loading: boolean
}>()

const selectedBackendId = defineModel<string | null>('selectedBackendId', {
  required: true,
})
const selectedPrefix = defineModel<string | undefined>('selectedPrefix', {
  required: true,
})

const { t } = useI18n()

// --- Backend selection -----------------------------------------------------

const backendOptions = computed<BackendOption[]>(() =>
  props.backends.map((b) => ({
    label: b.name,
    value: b.id,
    bucket: String((b.config as Record<string, unknown>)?.bucket ?? ''),
  })),
)

const selectedBackend = computed<BackendOption | undefined>({
  get: () =>
    selectedBackendId.value
      ? backendOptions.value.find((o) => o.value === selectedBackendId.value)
      : undefined,
  set: (opt) => {
    selectedBackendId.value = opt?.value ?? null
  },
})

// --- Scope mode ------------------------------------------------------------

// Derived from selectedPrefix so external resets stay in sync. `undefined`
// prefix always means "whole bucket"; picking a folder auto-flips this.
const scopeMode = computed<ScopeMode>({
  get: () => (selectedPrefix.value === undefined ? 'whole' : 'prefix'),
  set: (mode) => {
    if (mode === 'whole') {
      selectedPrefix.value = undefined
    } else if (mode === 'prefix' && selectedPrefix.value === undefined) {
      // Entering prefix mode without a chosen folder yet: keep undefined but
      // let the tree remain visible (user has to click "Share this folder").
      // We surface an empty-prefix placeholder in the UI meanwhile.
      selectedPrefix.value = ''
    }
  },
})

const scopeOptions = computed(() => [
  { label: t('scopeWholeBucket'), value: 'whole' as ScopeMode },
  { label: t('scopeFolder'), value: 'prefix' as ScopeMode },
])

// --- Tree browsing state ---------------------------------------------------

// Empty string = bucket root. Non-empty always ends with "/".
const currentPrefix = ref('')
const folders = ref<FolderEntry[]>([])
const files = ref<FileEntry[]>([])
const isTreeLoading = ref(false)
const treeError = ref<string | null>(null)

const breadcrumbSegments = computed(() =>
  currentPrefix.value === ''
    ? []
    : currentPrefix.value.replace(/\/$/, '').split('/'),
)

// Concurrent-navigation guard: only the newest load result may mutate state.
let loadGeneration = 0

const loadTreeAsync = async () => {
  if (!selectedBackendId.value) {
    folders.value = []
    files.value = []
    return
  }

  const generation = ++loadGeneration
  isTreeLoading.value = true
  treeError.value = null
  folders.value = []
  files.value = []

  try {
    const response = await invoke<StorageListDirResponse>(
      'remote_storage_list_dir',
      {
        request: {
          backendId: selectedBackendId.value,
          // Rust `Option<String>`: `None` at root, `Some(prefix)` otherwise.
          prefix: currentPrefix.value === '' ? undefined : currentPrefix.value,
        },
      },
    )
    if (generation !== loadGeneration) return

    folders.value = response.folders.map((full) => {
      const trimmed = full.replace(/\/$/, '')
      const name = trimmed.slice(currentPrefix.value.length)
      return { name, path: full }
    })
    files.value = response.objects.map((obj) => {
      const name = obj.key.slice(currentPrefix.value.length)
      return { name, key: obj.key }
    })
  } catch (error) {
    if (generation !== loadGeneration) return
    treeError.value = error instanceof Error
      ? error.message
      : typeof error === 'string'
        ? error
        : t('treeErrorGeneric')
  } finally {
    if (generation === loadGeneration) {
      isTreeLoading.value = false
    }
  }
}

const navigateToPrefix = (prefix: string) => {
  currentPrefix.value = prefix
  void loadTreeAsync()
}

const navigateToSegment = (index: number) => {
  // Segments are the "/"-stripped path components. Segment `i` becomes the
  // new leaf prefix, so we rejoin everything up to and including `i` and
  // re-append the trailing "/".
  const segments = breadcrumbSegments.value.slice(0, index + 1)
  navigateToPrefix(segments.join('/') + '/')
}

const selectFolderForShare = (prefix: string) => {
  selectedPrefix.value = prefix
}

// Reload tree when the user picks a different backend. Reset the browser
// back to root so we don't try to list a prefix that lives in a former bucket.
watch(
  selectedBackendId,
  () => {
    currentPrefix.value = ''
    // The chosen share scope belongs to whichever backend it was picked
    // under — a backend switch invalidates it entirely, not just the tree
    // browsing position. Without this reset the share could be submitted
    // with backend B but a prefix from backend A's bucket.
    selectedPrefix.value = undefined
    void loadTreeAsync()
  },
  { immediate: true },
)

// --- ARN preview -----------------------------------------------------------

const arnPreview = computed(() => {
  const bucket = selectedBackend.value?.bucket
  if (!bucket) return ''
  if (scopeMode.value === 'prefix' && selectedPrefix.value) {
    // Prefix already carries its trailing "/", so the wildcard glues directly
    // onto it to match every object underneath (matches Rust `prefix_condition`).
    return `arn:aws:s3:::${bucket}/${selectedPrefix.value}*`
  }
  return `arn:aws:s3:::${bucket}`
})
</script>

<i18n lang="yaml">
de:
  backendLabel: Storage-Backend auswählen
  backendPlaceholder: Backend wählen
  scopeLabel: Bereich
  scopeWholeBucket: Gesamter Bucket
  scopeFolder: Ordner
  folderLabel: 'Ordner: {path}'
  noFolderSelected: Kein Ordner ausgewählt
  breadcrumbRoot: Bucket-Root
  shareThisFolder: Diesen Ordner teilen
  treeLoading: Lade Ordner…
  treeEmpty: Ordner ist leer
  treeErrorTitle: Ordner konnten nicht geladen werden
  treeErrorGeneric: Unbekannter Fehler beim Auflisten des Buckets
  treeRetry: Erneut versuchen
  fileDisabledTooltip: Einzelne Dateien können in dieser Version nicht geteilt werden
  arnPreviewLabel: ARN Vorschau
en:
  backendLabel: Select storage backend
  backendPlaceholder: Choose backend
  scopeLabel: Scope
  scopeWholeBucket: Whole bucket
  scopeFolder: Folder
  folderLabel: 'Folder: {path}'
  noFolderSelected: No folder selected
  breadcrumbRoot: Bucket root
  shareThisFolder: Share this folder
  treeLoading: Loading folders…
  treeEmpty: This folder is empty
  treeErrorTitle: Failed to load folder
  treeErrorGeneric: Unknown error while listing bucket
  treeRetry: Retry
  fileDisabledTooltip: Sharing single files is not supported in this version
  arnPreviewLabel: ARN preview
</i18n>
