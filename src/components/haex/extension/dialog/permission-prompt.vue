<template>
  <UiDrawerModal
    v-model:open="modelOpen"
    :ui="{
      content: 'sm:max-w-md sm:mx-auto',
    }"
  >
    <template #header>
      <div class="flex items-center justify-between w-full">
        <div class="flex items-center gap-2">
          <h3 class="text-lg font-semibold">
            {{ t('title') }}
          </h3>
          <span
            v-if="props.pendingCount && props.pendingCount > 0"
            class="text-xs text-muted bg-muted px-2 py-0.5 rounded-full"
          >
            +{{ props.pendingCount }} {{ t('pending') }}
          </span>
        </div>
        <UButton
          icon="i-heroicons-x-mark"
          color="neutral"
          variant="ghost"
          @click="onCancel"
        />
      </div>
    </template>

    <template #content>
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
          <div class="text-sm space-y-1 pl-7">
            <div class="flex gap-2">
              <span class="text-muted">{{ t('action') }}:</span>
              <span class="font-mono">{{ promptData.action }}</span>
            </div>
            <div class="flex gap-2">
              <span class="text-muted">{{ t('target') }}:</span>
              <span class="font-mono break-all">{{ promptData.target }}</span>
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

        <!-- Action buttons -->
        <div class="flex flex-col sm:flex-row gap-2 w-full">
          <UiButton
            icon="i-heroicons-x-mark"
            :label="t('deny')"
            color="error"
            class="w-full sm:flex-1"
            size="lg"
            @click="onDeny"
          />
          <UiButton
            icon="i-heroicons-check"
            :label="t('allow')"
            color="success"
            class="w-full sm:flex-1"
            size="lg"
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
  decision: [value: PermissionDecision, remember: boolean]
}>()

const rememberDecision = ref(false)

// Reset checkbox when dialog opens
watch(
  () => props.open,
  (isOpen) => {
    if (isOpen) {
      rememberDecision.value = false
    }
  },
)

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
    case 'filesync':
      return 'i-heroicons-cloud-arrow-up'
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
    case 'filesync':
      return t('resourceType.filesync')
    default:
      return t('resourceType.unknown')
  }
})

function onAllow() {
  emit('decision', 'granted', rememberDecision.value)
}

function onDeny() {
  emit('decision', 'denied', rememberDecision.value)
}

function onCancel() {
  emit('decision', 'denied', false)
}
</script>

<i18n lang="yaml">
de:
  title: Berechtigungsanfrage
  pending: weitere
  requestsPermission: möchte eine Berechtigung
  action: Aktion
  target: Ziel
  resourceType:
    db: Datenbankzugriff
    web: Netzwerkzugriff
    fs: Dateisystemzugriff
    shell: Shell-Befehl
    filesync: Dateisynchronisation
    unknown: Unbekannt
  warning:
    title: Vorsicht
    description: Erteile nur Berechtigungen für Erweiterungen, denen du vertraust.
  rememberDecision: Entscheidung merken
  allow: Erlauben
  deny: Ablehnen
en:
  title: Permission Request
  pending: more
  requestsPermission: is requesting a permission
  action: Action
  target: Target
  resourceType:
    db: Database Access
    web: Network Access
    fs: Filesystem Access
    shell: Shell Command
    filesync: File Sync
    unknown: Unknown
  warning:
    title: Caution
    description: Only grant permissions to extensions you trust.
  rememberDecision: Remember decision
  allow: Allow
  deny: Deny
</i18n>
