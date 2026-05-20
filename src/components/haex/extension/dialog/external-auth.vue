<template>
  <UiDrawerModal
    v-model:open="open"
    :title="t('title')"
    :dismissible="false"
    :ui="{
      content: 'sm:max-w-md sm:mx-auto',
    }"
  >
    <template #header>
      <UiDialogHeader
        :title="t('title')"
        @close="onDeny"
      />
    </template>

    <template #body>
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
          @click="onDeny"
        />
        <UiButton
          icon="i-heroicons-check"
          :label="t('allow')"
          color="success"
          class="w-full sm:flex-1"
          :disabled="selectedExtensionIds.length === 0"
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

const CORE_EXTENSION_ID = '__core__'
const CORE_EXTENSION_NAME = 'core'

// Names that should map to the HaexVault core target instead of a separate
// extension. haex-pass used to ship as its own extension but is now built into
// haex-vault, so clients still requesting "haex-pass" land on core.
const CORE_ALIAS_NAMES = new Set(['haex-pass'])

// Get installed extensions
const extensionsStore = useExtensionsStore()

const requestsCoreAccess = computed(() =>
  (props.pendingAuth?.requestedExtensions ?? []).some(
    (req) =>
      (req.extensionPublicKey === CORE_EXTENSION_ID && req.name === CORE_EXTENSION_NAME)
      || CORE_ALIAS_NAMES.has(req.name),
  ),
)

const extensionOptions = computed(() => {
  const options = extensionsStore.availableExtensions.map((ext) => ({
    value: ext.id,
    label: ext.name || ext.id,
  }))
  // HaexVault core is always offered so users can grant access to built-in
  // features (passwords etc.) regardless of what the client requested.
  options.unshift({
    value: CORE_EXTENSION_ID,
    label: t('coreOption'),
  })
  return options
})

// Reset selection when dialog opens and pre-select requested extensions
watch(open, (isOpen) => {
  if (isOpen) {
    rememberDecision.value = false

    const requested = props.pendingAuth?.requestedExtensions ?? []
    if (requested.length > 0) {
      // Pre-select installed extensions that match the client's request
      const matchedIds = extensionsStore.availableExtensions
        .filter((ext) =>
          requested.some((req) => ext.name === req.name && ext.publicKey === req.extensionPublicKey),
        )
        .map((ext) => ext.id)

      // Also pre-select core access if requested (explicitly or via alias)
      if (requestsCoreAccess.value) matchedIds.unshift(CORE_EXTENSION_ID)

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
  selectExtension: Zugriff auswählen
  selectExtensionPlaceholder: Zugriff wählen...
  extensionHint: Der Client erhält Zugriff auf die ausgewählten Bereiche.
  coreOption: HaexVault (Kernfeatures)
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
  selectExtension: Select Access
  selectExtensionPlaceholder: Choose access...
  extensionHint: The client will have access to the selected areas.
  coreOption: HaexVault (Core features)
  warning:
    title: Security Notice
    description: Only approve connections from applications you started yourself.
  rememberDecision: Remember permanently
  allow: Allow
  deny: Deny
</i18n>
