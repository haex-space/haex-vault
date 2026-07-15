<template>
  <UiDrawerModal
    v-model:open="modelOpen"
    :title="t('title')"
    :dismissible="false"
    :ui="{
      content: 'sm:max-w-md sm:mx-auto',
    }"
  >
    <template #header>
      <UiDialogHeader
        :title="t('title')"
        @close="onCancel"
      >
        <template
          v-if="props.pendingCount && props.pendingCount > 0"
          #suffix
        >
          <span class="text-xs text-muted bg-muted px-2 py-0.5 rounded-full">
            +{{ props.pendingCount }} {{ t('pending') }}
          </span>
        </template>
      </UiDialogHeader>
    </template>

    <template #body>
      <div
        v-if="promptData"
        class="flex flex-col gap-4"
      >
        <!-- Extension Info -->
        <div class="flex items-center gap-3 p-3 bg-muted rounded-lg">
          <UIcon
            name="i-heroicons-puzzle-piece"
            class="w-10 h-10 text-primary shrink-0"
          />
          <div class="flex-1 min-w-0">
            <h4 class="font-semibold truncate">
              {{ promptData.extensionName }}
            </h4>
            <p class="text-sm text-muted">
              {{ t('requestsPermission') }}
            </p>
          </div>
        </div>

        <!-- Permission Details -->
        <div class="p-3 border border-default rounded-lg space-y-2">
          <div class="flex items-center gap-2">
            <UIcon
              :name="resourceTypeIcon"
              class="w-5 h-5 text-warning"
            />
            <span class="font-medium">{{ resourceTypeLabel }}</span>
          </div>
          <div class="text-sm space-y-2 pl-7">
            <div class="flex gap-2 items-center">
              <span class="text-muted">{{ t('action') }}:</span>
              <span class="font-mono">{{ promptData.action }}</span>
            </div>
            <!-- Passwords: tag-based scope editor -->
            <div
              v-if="promptData.resourceType === 'passwords'"
              class="flex flex-col gap-2"
            >
              <span class="text-muted">{{ t('target') }}:</span>
              <UCheckbox
                v-model="passwordsWildcard"
                :label="t('passwordsAllTags')"
              />
              <HaexSystemPasswordsEditorTagPicker
                v-if="!passwordsWildcard"
                v-model="passwordsTags"
              />
              <div
                v-if="showPasswordsDefaultTag"
                class="flex flex-col gap-1"
              >
                <span class="text-xs text-muted">{{ t('passwordsDefaultTag') }}</span>
                <USelectMenu
                  v-model="passwordsDefaultTag"
                  :items="passwordsTags"
                  :placeholder="t('passwordsDefaultTagPlaceholder')"
                />
              </div>
            </div>

            <!-- Every other resource type: plain editable target string -->
            <div
              v-else
              class="flex flex-col gap-1"
            >
              <span class="text-muted">{{ t('target') }}:</span>
              <div class="flex gap-2 items-start">
                <UInput
                  v-model="editableTarget"
                  class="flex-1 font-mono"
                  size="sm"
                />
                <UiButton
                  v-if="domainOnlyTarget"
                  :label="t('domainOnly')"
                  variant="soft"
                  color="neutral"
                  size="sm"
                  @click="applyDomainOnlyTarget"
                />
              </div>
            </div>
          </div>
        </div>

        <!-- Warning -->
        <UAlert
          color="warning"
          variant="soft"
          :title="t('warning.title')"
          :description="t('warning.description')"
          icon="i-heroicons-shield-exclamation"
        />
      </div>
    </template>

    <template #footer>
      <div class="flex flex-col gap-3 w-full">
        <!-- Remember checkbox -->
        <UCheckbox
          v-model="rememberDecision"
          :label="t('rememberDecision')"
        />
        <!-- Apply to all checkbox (only shown when there are pending prompts) -->
        <UCheckbox
          v-if="props.pendingCount && props.pendingCount > 0"
          v-model="applyToAll"
          :label="t('applyToAll', { count: props.pendingCount })"
        />

        <!-- Action buttons -->
        <div class="flex flex-col sm:flex-row gap-2 w-full">
          <UiButton
            icon="i-heroicons-x-mark"
            :label="t('deny')"
            color="error"
            class="w-full sm:flex-1"
            @click="onDeny"
          />
          <UiButton
            icon="i-heroicons-check"
            :label="t('allow')"
            color="success"
            class="w-full sm:flex-1"
            :disabled="isAllowDisabled"
            @click="onAllow"
          />
        </div>
      </div>
    </template>
  </UiDrawerModal>
</template>

<script setup lang="ts">
import type {
  PermissionPromptData,
  PermissionPromptEdit,
  PermissionDecision,
} from '~/composables/usePermissionPrompt'

const { t } = useI18n()

const props = defineProps<{
  open: boolean
  promptData: PermissionPromptData | null
  pendingCount?: number
}>()

const emit = defineEmits<{
  'update:open': [value: boolean]
  decision: [value: PermissionDecision, remember: boolean, applyToAll: boolean, edit: PermissionPromptEdit]
}>()

const rememberDecision = ref(false)
const applyToAll = ref(false)
const editableTarget = ref('')

// Passwords-specific scope editor state.
const passwordsWildcard = ref(false)
const passwordsTags = ref<string[]>([])
const passwordsDefaultTag = ref<string | undefined>(undefined)

// A passwords prompt target is either the wildcard "*" or a comma-joined
// list of tags (see the backend's check_passwords_permission, which now
// surfaces the extension's actually-declared tags instead of a bare "*").
function parseTagsFromTarget(target: string): string[] {
  if (target === '*' || !target) {
    return []
  }
  return target.split(',').map((t) => t.trim()).filter(Boolean)
}

const isPasswordsWriteAction = computed(() => props.promptData?.action === 'readWrite')

const showPasswordsDefaultTag = computed(
  () => !passwordsWildcard.value && passwordsTags.value.length > 1 && isPasswordsWriteAction.value,
)

const isAllowDisabled = computed(() => {
  if (props.promptData?.resourceType === 'passwords') {
    if (passwordsWildcard.value) {
      return false
    }
    if (passwordsTags.value.length === 0) {
      return true
    }
    return (
      showPasswordsDefaultTag.value
      && !passwordsTags.value.includes(passwordsDefaultTag.value ?? '')
    )
  }
  return !editableTarget.value.trim()
})

// Keep the chosen default tag valid whenever the tag selection changes: if the
// tag currently marked as default gets deselected, fall back to the lone
// remaining tag (implicit default) or clear it so Allow stays gated.
watch(passwordsTags, (tags) => {
  if (passwordsDefaultTag.value && !tags.includes(passwordsDefaultTag.value)) {
    passwordsDefaultTag.value = tags.length === 1 ? tags[0] : undefined
  }
})

// Reset checkboxes and editable target/tags when dialog opens
watch(
  () => props.open,
  (isOpen) => {
    if (isOpen) {
      rememberDecision.value = false
      applyToAll.value = false
      editableTarget.value = props.promptData?.target ?? ''

      const isWildcardTarget = props.promptData?.target === '*'
      passwordsWildcard.value = isWildcardTarget
      passwordsTags.value = isWildcardTarget ? [] : parseTagsFromTarget(props.promptData?.target ?? '')
      passwordsDefaultTag.value = passwordsTags.value.length === 1 ? passwordsTags.value[0] : undefined
    }
  },
)

// Offers a one-click narrowing of a full URL target down to its bare host,
// matching the backend's domain-suffix matching (see web_matching_status).
const domainOnlyTarget = computed(() => {
  try {
    const host = new URL(editableTarget.value).hostname
    return host && host !== editableTarget.value ? host : null
  } catch {
    return null
  }
})

function applyDomainOnlyTarget() {
  if (domainOnlyTarget.value) {
    editableTarget.value = domainOnlyTarget.value
  }
}

const modelOpen = computed({
  get: () => props.open,
  set: (value) => emit('update:open', value),
})

const resourceTypeIcon = computed(() => {
  switch (props.promptData?.resourceType) {
    case 'db':
      return 'i-heroicons-circle-stack'
    case 'web':
      return 'i-heroicons-globe-alt'
    case 'fs':
      return 'i-heroicons-folder'
    case 'shell':
      return 'i-heroicons-command-line'
    case 'syncServers':
      return 'i-heroicons-server'
    case 'cloudStorage':
      return 'i-heroicons-cloud-arrow-up'
    case 'syncRules':
      return 'i-heroicons-arrow-path'
    case 'spaces':
      return 'i-heroicons-user-group'
    case 'passwords':
      return 'i-heroicons-key'
    case 'extensionApi':
      return 'i-heroicons-puzzle-piece'
    default:
      return 'i-heroicons-question-mark-circle'
  }
})

const resourceTypeLabel = computed(() => {
  switch (props.promptData?.resourceType) {
    case 'db':
      return t('resourceType.db')
    case 'web':
      return t('resourceType.web')
    case 'fs':
      return t('resourceType.fs')
    case 'shell':
      return t('resourceType.shell')
    case 'syncServers':
      return t('resourceType.syncServers')
    case 'cloudStorage':
      return t('resourceType.cloudStorage')
    case 'syncRules':
      return t('resourceType.syncRules')
    case 'spaces':
      return t('resourceType.spaces')
    case 'passwords':
      return t('resourceType.passwords')
    case 'extensionApi':
      return t('resourceType.extensionApi')
    default:
      return t('resourceType.unknown')
  }
})

function buildEdit(): PermissionPromptEdit {
  if (props.promptData?.resourceType === 'passwords') {
    const tags = passwordsWildcard.value ? ['*'] : passwordsTags.value
    return {
      target: tags.join(','),
      tags,
      defaultTag: passwordsWildcard.value ? undefined : passwordsDefaultTag.value,
    }
  }
  return { target: editableTarget.value.trim() }
}

function onAllow() {
  emit('decision', 'granted', rememberDecision.value, applyToAll.value, buildEdit())
}

function onDeny() {
  emit('decision', 'denied', rememberDecision.value, applyToAll.value, buildEdit())
}

function onCancel() {
  emit('decision', 'denied', false, false, { target: props.promptData?.target ?? '' })
}
</script>

<i18n lang="yaml">
de:
  title: Berechtigungsanfrage
  pending: weitere
  requestsPermission: möchte eine Berechtigung
  action: Aktion
  target: Ziel
  domainOnly: Nur Domain
  passwordsAllTags: Zugriff auf alle Tags erlauben
  passwordsDefaultTag: Standard-Tag für neue Einträge
  passwordsDefaultTagPlaceholder: Tag auswählen
  resourceType:
    db: Datenbankzugriff
    web: Netzwerkzugriff
    fs: Dateisystemzugriff
    shell: Shell-Befehl
    syncServers: Sync-Server
    cloudStorage: Cloud-Speicher
    syncRules: Sync-Regeln
    spaces: Shared Spaces
    passwords: Passwortzugriff
    extensionApi: Zugriff auf externe Anwendung
    unknown: Unbekannt
  warning:
    title: Vorsicht
    description: Erteile nur Berechtigungen für Erweiterungen, denen du vertraust.
  rememberDecision: Entscheidung merken
  applyToAll: Für alle {count} wartenden Anfragen anwenden
  allow: Erlauben
  deny: Ablehnen
en:
  title: Permission Request
  pending: more
  requestsPermission: is requesting a permission
  action: Action
  target: Target
  domainOnly: Domain only
  passwordsAllTags: Allow access to all tags
  passwordsDefaultTag: Default tag for new entries
  passwordsDefaultTagPlaceholder: Select a tag
  resourceType:
    db: Database Access
    web: Network Access
    fs: Filesystem Access
    shell: Shell Command
    syncServers: Sync Servers
    cloudStorage: Cloud Storage
    syncRules: Sync Rules
    spaces: Shared Spaces
    passwords: Password Access
    extensionApi: External Application Access
    unknown: Unknown
  warning:
    title: Caution
    description: Only grant permissions to extensions you trust.
  rememberDecision: Remember decision
  applyToAll: Apply to all {count} pending requests
  allow: Allow
  deny: Deny
</i18n>
