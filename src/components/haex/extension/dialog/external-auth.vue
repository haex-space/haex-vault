<template>
  <UiDrawerModal
    v-model:open="open"
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
          @click="onDeny"
        />
      </div>
    </template>

    <template #content>
      <div
        v-if="pendingAuth"
        class="flex flex-col gap-4"
      >
        <!-- Client Info -->
        <div class="flex items-center gap-3 p-3 bg-muted rounded-lg">
          <UIcon
            name="i-heroicons-computer-desktop"
            class="w-10 h-10 text-primary shrink-0"
          />
          <div class="flex-1 min-w-0">
            <h4 class="font-semibold truncate">
              {{ pendingAuth.clientName }}
            </h4>
            <p class="text-sm text-muted">
              {{ t('wantsToConnect') }}
            </p>
          </div>
        </div>

        <!-- Client ID (fingerprint) -->
        <div class="p-3 border border-default rounded-lg">
          <div class="flex items-center gap-2 mb-2">
            <UIcon
              name="i-heroicons-finger-print"
              class="w-5 h-5 text-muted"
            />
            <span class="text-sm text-muted">{{ t('clientId') }}</span>
          </div>
          <code class="text-xs font-mono break-all">{{
            pendingAuth.clientId
          }}</code>
        </div>

        <!-- Extension Selection -->
        <div class="space-y-2">
          <label class="text-sm font-medium block w-full">{{
            t('selectExtension')
          }}</label>
          <USelectMenu
            v-model="selectedExtensionIds"
            :items="extensionOptions"
            :placeholder="t('selectExtensionPlaceholder')"
            value-key="value"
            label-key="label"
            multiple
            class="w-full"
          />
          <p class="text-xs text-muted">
            {{ t('extensionHint') }}
          </p>
        </div>

        <!-- Warning -->
        <UAlert
          color="warning"
          variant="soft"
          :title="t('warning.title')"
          :description="t('warning.description')"
          icon="i-heroicons-shield-exclamation"
        />

        <!-- Remember checkbox -->
        <UCheckbox
          v-model="rememberDecision"
          :label="t('rememberDecision')"
        />
      </div>
    </template>

    <template #footer>
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
          :disabled="selectedExtensionIds.length === 0"
          size="lg"
          @click="onAllow"
        />
      </div>
    </template>
  </UiDrawerModal>
</template>

<script setup lang="ts">
import type { ExternalAuthDecision } from '@haex-space/vault-sdk'

// Accept both mutable and readonly versions of PendingAuthorization
interface RequestedExtensionProp {
  name: string
  extensionPublicKey: string
}

interface PendingAuthProp {
  clientId: string
  clientName: string
  publicKey: string
  requestedExtensions:
    | readonly RequestedExtensionProp[]
    | RequestedExtensionProp[]
}

const { t } = useI18n()

const open = defineModel<boolean>('open', { required: true })

const props = defineProps<{
  pendingAuth: PendingAuthProp | null
}>()

const emit = defineEmits<{
  decision: [
    decision: ExternalAuthDecision,
    extensionIds?: string[],
    remember?: boolean,
  ]
}>()

const selectedExtensionIds = ref<string[]>([])
const rememberDecision = ref(false)

// Get installed extensions
const extensionsStore = useExtensionsStore()
const extensionOptions = computed(() => {
  return extensionsStore.availableExtensions.map((ext) => ({
    value: ext.id,
    label: ext.name || ext.id,
  }))
})

// Reset selection when dialog opens and pre-select requested extensions
watch(open, (isOpen) => {
  if (isOpen) {
    rememberDecision.value = false

    // Pre-select extensions that match the client's requestedExtensions
    const requested = props.pendingAuth?.requestedExtensions ?? []
    if (requested.length > 0) {
      const matchedIds = extensionsStore.availableExtensions
        .filter((ext) =>
          requested.some(
            (req) =>
              ext.name === req.name && ext.publicKey === req.extensionPublicKey,
          ),
        )
        .map((ext) => ext.id)

      selectedExtensionIds.value = matchedIds
    } else {
      selectedExtensionIds.value = []
    }
  }
})

function onAllow() {
  if (selectedExtensionIds.value.length > 0) {
    emit(
      'decision',
      'allow',
      selectedExtensionIds.value,
      rememberDecision.value,
    )
  }
}

function onDeny() {
  emit('decision', 'deny', undefined, rememberDecision.value)
}
</script>

<i18n lang="yaml">
de:
  title: Externe Verbindung
  wantsToConnect: möchte sich verbinden
  clientId: Client-ID (Fingerprint)
  selectExtension: Erweiterungen auswählen
  selectExtensionPlaceholder: Erweiterungen wählen...
  extensionHint: Der Client erhält Zugriff auf die ausgewählten Erweiterungen.
  warning:
    title: Sicherheitshinweis
    description: Genehmige nur Verbindungen von Anwendungen, die du selbst gestartet hast.
  rememberDecision: Dauerhaft merken
  allow: Erlauben
  deny: Ablehnen
en:
  title: External Connection
  wantsToConnect: wants to connect
  clientId: Client ID (Fingerprint)
  selectExtension: Select Extensions
  selectExtensionPlaceholder: Choose extensions...
  extensionHint: The client will have access to the selected extensions.
  warning:
    title: Security Notice
    description: Only approve connections from applications you started yourself.
  rememberDecision: Remember permanently
  allow: Allow
  deny: Deny
</i18n>
