<template>
  <UiDrawerModal
    v-model:open="open"
    :ui="{
      content: 'sm:max-w-2xl sm:mx-auto',
    }"
  >
    <template #header>
      <div class="flex items-center justify-between w-full">
        <h3 class="text-lg font-semibold">
          {{ t('title') }}
        </h3>
        <UButton
          icon="i-heroicons-x-mark"
          color="neutral"
          variant="ghost"
          @click="onDeny"
        />
      </div>
    </template>

    <template #content>
      <div class="flex flex-col gap-6">
        <!-- Extension Info -->
        <UCard>
          <div class="flex items-start gap-4">
            <div
              v-if="preview?.manifest.icon"
              class="w-16 h-16 shrink-0"
            >
              <UIcon
                :name="preview.manifest.icon"
                class="w-full h-full"
              />
            </div>
            <div class="flex-1">
              <h3 class="text-xl font-bold">
                {{ preview?.manifest.name }}
              </h3>
              <p class="text-sm text-gray-500 dark:text-gray-400">
                {{ t('version') }}: {{ preview?.manifest.version }}
              </p>
              <p
                v-if="preview?.manifest.author"
                class="text-sm text-gray-500 dark:text-gray-400"
              >
                {{ t('author') }}: {{ preview.manifest.author }}
              </p>
              <p
                v-if="preview?.manifest.description"
                class="text-sm mt-2"
              >
                {{ preview.manifest.description }}
              </p>

              <!-- Signature Verification -->
              <UBadge
                :color="preview?.isValidSignature ? 'success' : 'error'"
                variant="subtle"
                class="mt-2"
              >
                <template #leading>
                  <UIcon
                    :name="
                      preview?.isValidSignature
                        ? 'i-heroicons-shield-check'
                        : 'i-heroicons-shield-exclamation'
                    "
                  />
                </template>
                {{
                  preview?.isValidSignature
                    ? t('signature.valid')
                    : t('signature.invalid')
                }}
              </UBadge>
            </div>
          </div>
        </UCard>

        <!-- Version Selection (only shown when versions are provided) -->
        <UCard v-if="showVersionSelection">
          <template #header>
            <div class="flex items-center gap-2">
              <UIcon
                name="i-heroicons-tag"
                class="w-5 h-5"
              />
              <h4 class="font-semibold">
                {{ t('versionSelection.title') }}
              </h4>
            </div>
          </template>

          <!-- Installed Version Info -->
          <div
            v-if="installedVersion"
            class="mb-4 p-3 bg-gray-50 dark:bg-gray-800 rounded-lg"
          >
            <div class="flex items-center gap-2 text-sm">
              <UIcon
                name="i-heroicons-check-circle"
                class="w-4 h-4 text-success"
              />
              <span>{{ t('versionSelection.installedVersion', { version: installedVersion }) }}</span>
            </div>
          </div>

          <!-- Loading Versions -->
          <div
            v-if="isLoadingVersions"
            class="flex justify-center py-4"
          >
            <UIcon
              name="i-heroicons-arrow-path"
              class="w-6 h-6 animate-spin text-muted"
            />
          </div>

          <!-- Version Radio Group -->
          <URadioGroup
            v-else
            v-model="internalSelectedVersion"
            :items="versionRadioItems"
          />
        </UCard>

        <!-- Create Native Desktop Shortcut (Desktop only) -->
        <div
          v-if="showDesktopShortcutOption"
          class="flex flex-col gap-1"
        >
          <UCheckbox
            v-model="createDesktopShortcut"
            :label="t('createDesktopShortcut.label')"
          />
          <p class="text-sm text-gray-500 dark:text-gray-400 ml-6">
            {{ t('createDesktopShortcut.description') }}
          </p>
        </div>

        <!-- Permissions Section -->
        <div class="flex flex-col gap-4">
          <h4 class="text-lg font-semibold">
            {{ t('permissions.title') }}
          </h4>

          <UAccordion
            :items="permissionAccordionItems"
            :ui="{ root: 'flex flex-col gap-2' }"
          >
            <template #database>
              <div
                v-if="databasePermissions"
                class="pb-4"
              >
                <HaexExtensionPermissionList
                  v-model="databasePermissions"
                  :title="t('permissions.database')"
                />
              </div>
            </template>

            <template #filesystem>
              <div
                v-if="filesystemPermissions"
                class="pb-4"
              >
                <HaexExtensionPermissionList
                  v-model="filesystemPermissions"
                  :title="t('permissions.filesystem')"
                />
              </div>
            </template>

            <template #http>
              <div
                v-if="httpPermissions"
                class="pb-4"
              >
                <HaexExtensionPermissionList
                  v-model="httpPermissions"
                  :title="t('permissions.http')"
                />
              </div>
            </template>

            <template #shell>
              <div
                v-if="shellPermissions"
                class="pb-4"
              >
                <HaexExtensionPermissionList
                  v-model="shellPermissions"
                  :title="t('permissions.shell')"
                />
              </div>
            </template>
          </UAccordion>
        </div>
      </div>
    </template>

    <template #footer>
      <div class="flex flex-col sm:flex-row gap-4 justify-end w-full">
        <UButton
          icon="i-heroicons-x-mark"
          :label="t('abort')"
          color="error"
          variant="outline"
          class="w-full sm:w-auto"
          @click="onDeny"
        />
        <UButton
          icon="i-heroicons-check"
          :label="t('confirm')"
          color="primary"
          class="w-full sm:w-auto"
          @click="onConfirm"
        />
      </div>
    </template>
  </UiDrawerModal>
</template>

<script setup lang="ts">
import type { ExtensionPreview } from '~~/src-tauri/bindings/ExtensionPreview'
import type { ExtensionVersion } from '@haex-space/marketplace-sdk/vue'
import { isDesktop } from '~/utils/platform'

const { t } = useI18n()

const open = defineModel<boolean>('open', { default: false })
const preview = defineModel<ExtensionPreview | null>('preview', {
  default: null,
})
const selectedVersion = defineModel<string | null>('selectedVersion', {
  default: null,
})

// Props for version selection
const props = defineProps<{
  /** Available versions from marketplace (optional - shows version selection when provided) */
  availableVersions?: ExtensionVersion[]
  /** Currently installed version (optional - shows installed badge) */
  installedVersion?: string | null
  /** Whether versions are being loaded */
  isLoadingVersions?: boolean
}>()

// Show version selection only when versions are provided
const showVersionSelection = computed(() =>
  (props.availableVersions && props.availableVersions.length > 0) || props.isLoadingVersions,
)

// Internal selected version state, synced with model
const internalSelectedVersion = computed({
  get: () => selectedVersion.value || props.availableVersions?.[0]?.version || null,
  set: (value) => {
    selectedVersion.value = value
  },
})

// Build radio items from available versions
const versionRadioItems = computed(() => {
  if (!props.availableVersions) return []

  return props.availableVersions.map((v) => {
    const isInstalled = v.version === props.installedVersion
    const isLatest = v.version === props.availableVersions?.[0]?.version

    let label = `v${v.version}`
    if (isLatest) label += ` (${t('versionSelection.latest')})`
    if (isInstalled) label += ` (${t('versionSelection.installed')})`

    return {
      value: v.version,
      label,
      description: v.changelog || undefined,
    }
  })
})

// Desktop shortcut option (only shown on desktop platforms)
const showDesktopShortcutOption = computed(() => isDesktop())
const createDesktopShortcut = ref(false)

const databasePermissions = computed({
  get: () => preview.value?.editablePermissions?.database || [],
  set: (value) => {
    if (preview.value?.editablePermissions) {
      preview.value.editablePermissions.database = value
    }
  },
})

const filesystemPermissions = computed({
  get: () => preview.value?.editablePermissions?.filesystem || [],
  set: (value) => {
    if (preview.value?.editablePermissions) {
      preview.value.editablePermissions.filesystem = value
    }
  },
})

const httpPermissions = computed({
  get: () => preview.value?.editablePermissions?.http || [],
  set: (value) => {
    if (preview.value?.editablePermissions) {
      preview.value.editablePermissions.http = value
    }
  },
})

const shellPermissions = computed({
  get: () => preview.value?.editablePermissions?.shell || [],
  set: (value) => {
    if (preview.value?.editablePermissions) {
      preview.value.editablePermissions.shell = value
    }
  },
})

const permissionAccordionItems = computed(() => {
  const items = []

  if (databasePermissions.value?.length) {
    items.push({
      label: t('permissions.database'),
      icon: 'i-heroicons-circle-stack',
      slot: 'database',
      defaultOpen: true,
    })
  }

  if (filesystemPermissions.value?.length) {
    items.push({
      label: t('permissions.filesystem'),
      icon: 'i-heroicons-folder',
      slot: 'filesystem',
    })
  }

  if (httpPermissions.value?.length) {
    items.push({
      label: t('permissions.http'),
      icon: 'i-heroicons-globe-alt',
      slot: 'http',
    })
  }

  if (shellPermissions.value?.length) {
    items.push({
      label: t('permissions.shell'),
      icon: 'i-heroicons-command-line',
      slot: 'shell',
    })
  }

  return items
})

const emit = defineEmits<{
  deny: []
  confirm: [createDesktopShortcut: boolean]
}>()

const onDeny = () => {
  open.value = false
  emit('deny')
}

const onConfirm = () => {
  open.value = false
  emit('confirm', createDesktopShortcut.value)
}
</script>

<i18n lang="yaml">
de:
  title: Erweiterung installieren
  version: Version
  author: Autor
  createDesktopShortcut:
    label: Desktop-Verknüpfung erstellen
    description: Erstellt eine Verknüpfung auf deinem System-Desktop, um diese Erweiterung direkt zu starten.
  signature:
    valid: Signatur verifiziert
    invalid: Signatur ungültig
  versionSelection:
    title: Versionsauswahl
    installedVersion: "Aktuell installiert: v{version}"
    latest: Neueste
    installed: Installiert
  permissions:
    title: Berechtigungen
    database: Datenbank
    filesystem: Dateisystem
    http: Internet
    shell: Terminal
  abort: Abbrechen
  confirm: Installieren

en:
  title: Install Extension
  version: Version
  author: Author
  createDesktopShortcut:
    label: Create desktop shortcut
    description: Creates a shortcut on your system desktop to launch this extension directly.
  signature:
    valid: Signature verified
    invalid: Invalid signature
  versionSelection:
    title: Version Selection
    installedVersion: "Currently installed: v{version}"
    latest: Latest
    installed: Installed
  permissions:
    title: Permissions
    database: Database
    filesystem: Filesystem
    http: Internet
    shell: Terminal
  abort: Cancel
  confirm: Install
</i18n>
