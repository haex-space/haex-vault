<template>
  <UiDrawerModal
    v-model:open="modelOpen"
    :ui="{
      content: 'sm:max-w-md sm:mx-auto',
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
      <div class="flex flex-col gap-2 w-full">
        <!-- Primary actions -->
        <div class="flex flex-col sm:flex-row gap-2 w-full">
          <UButton
            icon="i-heroicons-check"
            :label="t('allow')"
            color="success"
            class="w-full sm:flex-1"
            @click="onAllow"
          />
          <UButton
            icon="i-heroicons-x-mark"
            :label="t('deny')"
            color="error"
            class="w-full sm:flex-1"
            @click="onDeny"
          />
        </div>
        <!-- Secondary action -->
        <UButton
          icon="i-heroicons-clock"
          :label="t('allowOnce')"
          color="neutral"
          variant="outline"
          class="w-full"
          @click="onAllowOnce"
        />
      </div>
    </template>
  </UiDrawerModal>
</template>

<script setup lang="ts">
import type { PermissionPromptData, PermissionDecision } from '~/composables/usePermissionPrompt'

const { t } = useI18n()

const props = defineProps<{
  open: boolean
  promptData: PermissionPromptData | null
}>()

const emit = defineEmits<{
  'update:open': [value: boolean]
  decision: [value: PermissionDecision]
}>()

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
    default:
      return t('resourceType.unknown')
  }
})

function onAllow() {
  emit('decision', 'granted')
}

function onDeny() {
  emit('decision', 'denied')
}

function onAllowOnce() {
  emit('decision', 'ask')
}

function onCancel() {
  emit('decision', 'denied')
}
</script>

<i18n lang="yaml">
de:
  title: Berechtigungsanfrage
  requestsPermission: möchte eine Berechtigung
  action: Aktion
  target: Ziel
  resourceType:
    db: Datenbankzugriff
    web: Netzwerkzugriff
    fs: Dateisystemzugriff
    shell: Shell-Befehl
    unknown: Unbekannt
  warning:
    title: Vorsicht
    description: Erteile nur Berechtigungen für Erweiterungen, denen du vertraust.
  allow: Erlauben
  deny: Ablehnen
  allowOnce: Einmal erlauben
en:
  title: Permission Request
  requestsPermission: is requesting a permission
  action: Action
  target: Target
  resourceType:
    db: Database Access
    web: Network Access
    fs: Filesystem Access
    shell: Shell Command
    unknown: Unknown
  warning:
    title: Caution
    description: Only grant permissions to extensions you trust.
  allow: Allow
  deny: Deny
  allowOnce: Allow Once
</i18n>
