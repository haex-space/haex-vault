<script setup lang="ts">
/**
 * Reading a contact in (spec 060 US4, FR-021 to FR-025a): paste the text of a share or an old
 * Visitenkarte, or scan its QR code on a phone. holzi checks it and shows a preview with the target
 * (a new contact, or the one that has the vault already, or one the person picks) and one row per
 * item with its choices. Nothing is stored before "Übernehmen"; the defaults add what is new and
 * replace nothing, and a vault identity of a contact is never replaced.
 */
import { toast } from 'vue-sonner'
import type { ContactSharePreview } from '@bindings/ContactSharePreview'
import type { ContactShareTarget } from '@bindings/ContactShareTarget'
import {
  choose,
  defaultDecisions,
  replacing,
  summary,
  toArgs,
  validate,
  type Decisions,
  type ImportChoice,
} from '~/lib/contacts/shareImport'

const { t } = useI18n()
const { errString } = useErrorString()
const router = useTabRouter()
const store = useContactsStore()
const instances = useInstancesStore()
const { isAndroid } = useDeviceCapabilities()
const { sharePreviewAsync, shareAcceptAsync } = useContactShare()
const { blockAsync } = useContactCards()

const text = ref('')
const preview = ref<ContactSharePreview | null>(null)
const decisions = ref<Decisions>(new Map())
const previewedText = ref('')
const error = ref<string | null>(null)
const busy = ref(false)
let previewSeq = 0

watch(text, () => {
  previewSeq += 1
  if (preview.value && text.value.trim() !== previewedText.value) {
    preview.value = null
    decisions.value = new Map()
    previewedText.value = ''
  }
  if (busy.value) busy.value = false
})

const inbox = useShareInboxStore()

// Spec 060 FR-018a: the text of a link that was opened outside holzi is read in as if pasted;
// only the preview is shown, nothing is stored before "Übernehmen". Taken out of the inbox at
// once, so a later link can come in (also while this page is open).
watch(
  () => inbox.text,
  async (waiting) => {
    if (waiting === null) return
    const linked = inbox.take()
    if (linked === null) return
    text.value = linked
    await nextTick()
    await checkAsync()
  },
  { immediate: true },
)

const problem = computed(() =>
  preview.value ? validate(preview.value, decisions.value) : null,
)
const counts = computed(() =>
  preview.value ? summary(preview.value, decisions.value) : null,
)
const vaultLocked = computed(
  () =>
    preview.value?.target.kind === 'contact' &&
    preview.value.candidates.some((candidate) => candidate.reason === 'vault'),
)

async function previewAsync(target?: ContactShareTarget) {
  error.value = null
  busy.value = true
  const seq = ++previewSeq
  const source = text.value.trim()
  try {
    const shown = await sharePreviewAsync(source, target)
    if (seq !== previewSeq) return
    previewedText.value = source
    preview.value = shown
    decisions.value = defaultDecisions(shown)
  } catch (cause) {
    if (seq !== previewSeq) return
    preview.value = null
    error.value = errString(cause)
  } finally {
    if (seq === previewSeq) busy.value = false
  }
}

async function checkAsync() {
  preview.value = null
  previewedText.value = ''
  decisions.value = new Map()
  if (!text.value.trim()) return
  await previewAsync()
}

async function scanAsync() {
  try {
    const { scan, Format } = await import('@tauri-apps/plugin-barcode-scanner')
    const scanned = await scan({ formats: [Format.QRCode] })
    text.value = scanned.content
    await nextTick()
    await checkAsync()
  } catch (cause) {
    toast.error(errString(cause))
  }
}

function pickTarget(value: string) {
  void previewAsync(
    value === 'new' ? { kind: 'new' } : { kind: 'contact', id: value },
  )
}

const targetOptions = computed(() => [
  ...(vaultLocked.value
    ? []
    : [{ value: 'new', label: t('contacts.import.targetNew') }]),
  ...(preview.value?.candidates ?? []).map((candidate) => ({
    value: candidate.id,
    label: `${candidate.name || t('contacts.noName')} · ${t(`contacts.import.reason.${candidate.reason}`)}`,
  })),
])

const targetValue = computed(() =>
  preview.value?.target.kind === 'contact' ? preview.value.target.id : 'new',
)

async function acceptAsync() {
  const shown = preview.value
  if (!shown || problem.value) return
  busy.value = true
  try {
    // action-exempt: the contact actions join the catalog with spec 051 US6 (T083); until then the
    // choice after the preview calls the command.
    const contact = await shareAcceptAsync(
      previewedText.value,
      shown.target,
      toArgs(shown, decisions.value),
      shown.targetRevision,
    )
    await store.quietReloadAsync()
    toast.success(t('contacts.import.accepted', { name: contact.name }))
    await router.replace(`/entry/${contact.id}`)
  } catch (cause) {
    if (
      typeof cause === 'object' &&
      cause !== null &&
      (cause as { kind?: string }).kind === 'SharePreviewStale'
    ) {
      // The contact changed since the preview: show it as it is now, choose again.
      toast.warning(t('contacts.import.stale'))
      await previewAsync(shown.target)
    } else {
      error.value = errString(cause)
    }
  } finally {
    busy.value = false
  }
}

async function blockVaultAsync() {
  const key = preview.value?.vaultPubkey
  if (!key) return
  try {
    // action-exempt: the contact actions join the catalog with spec 051 US6 (T083); until then the
    // button of the preview calls the command.
    await blockAsync(key)
    await previewAsync(preview.value?.target)
  } catch (cause) {
    toast.error(errString(cause))
  }
}

function onChoose(itemId: number, choice: ImportChoice) {
  if (preview.value)
    decisions.value = choose(preview.value, decisions.value, itemId, choice)
}

function onReplace(itemId: number, valueId: string) {
  decisions.value = replacing(decisions.value, itemId, valueId)
}
</script>

<template>
  <div class="flex min-h-0 flex-col gap-4 p-3" data-testid="contacts-import">
    <div class="flex items-center gap-1">
      <UiButton
        variant="ghost"
        size="icon"
        :aria-label="t('contacts.back')"
        :tooltip="t('contacts.back')"
        data-testid="contacts-back"
        @click="router.back()"
      >
        <Icon name="lucide:arrow-left" class="size-4" />
      </UiButton>
      <h1 class="min-w-0 flex-1 truncate px-1 text-2xl font-semibold">
        {{ t('contacts.import.title') }}
      </h1>
    </div>

    <form class="flex flex-col gap-2" @submit.prevent="checkAsync">
      <UiTextarea
        id="contacts-import-text"
        v-model="text"
        :label="t('contacts.import.paste')"
        label-bg="var(--background)"
        rows="3"
        data-testid="contacts-import-text"
      />
      <div class="flex flex-wrap gap-2">
        <UiButton
          type="submit"
          :loading="busy && !preview"
          data-testid="contacts-import-check"
        >
          {{ t('contacts.import.check') }}
        </UiButton>
        <UiButton
          v-if="isAndroid"
          type="button"
          variant="outline"
          data-testid="contacts-import-scan"
          @click="scanAsync"
        >
          <Icon name="lucide:scan-qr-code" class="size-4" />
          {{ t('contacts.import.scan') }}
        </UiButton>
      </div>
    </form>

    <p
      v-if="error"
      class="rounded-lg border border-destructive/40 p-3 text-sm text-destructive"
      role="alert"
      data-testid="contacts-import-error"
    >
      {{ error }}
    </p>

    <template v-if="preview">
      <SettingsGroup
        :label="t('contacts.import.preview')"
        data-testid="contacts-import-preview"
      >
        <SettingsRow
          :title="preview.name || t('contacts.noName')"
          :description="
            preview.npub && !preview.nameIsNpub ? preview.npub : undefined
          "
          data-testid="contacts-import-name"
        />
        <li class="px-4 py-3 text-sm text-muted-foreground">
          {{
            t('contacts.import.vault', {
              vault: instances.activeInstance ?? '',
            })
          }}
        </li>
        <li
          v-if="preview.identity.kind === 'new' && preview.npub"
          class="px-4 py-3 text-sm text-muted-foreground"
          data-testid="contacts-import-identity-new"
        >
          {{ t('contacts.import.identityNew', { npub: preview.npub }) }}
        </li>
        <li
          v-else-if="preview.identity.kind === 'matches'"
          class="px-4 py-3 text-sm text-muted-foreground"
          data-testid="contacts-import-identity-matches"
        >
          {{ t('contacts.import.identityMatches') }}
        </li>
        <li
          v-else-if="preview.identity.kind === 'mismatch'"
          class="px-4 py-3 text-sm text-destructive"
          data-testid="contacts-import-identity-mismatch"
        >
          {{
            t('contacts.import.identityMismatch', {
              npub: preview.identity.existingNpub,
            })
          }}
        </li>
        <li
          v-if="!preview.reachable"
          class="px-4 py-3 text-sm text-muted-foreground"
          data-testid="contacts-import-unreachable"
        >
          {{ t('contacts.import.unreachable') }}
        </li>
        <li
          v-if="preview.skipped > 0"
          class="px-4 py-3 text-sm text-muted-foreground"
          data-testid="contacts-import-skipped"
        >
          {{
            t(
              'contacts.import.skipped',
              { count: preview.skipped },
              preview.skipped,
            )
          }}
        </li>
        <li
          v-if="preview.legacyCard"
          class="px-4 py-3 text-sm text-muted-foreground"
        >
          {{ t('contacts.import.legacy') }}
        </li>
      </SettingsGroup>

      <SettingsGroup :label="t('contacts.import.target')">
        <li class="px-4 py-3">
          <UiSelect
            :model-value="targetValue"
            :options="targetOptions"
            :label="t('contacts.import.targetLabel')"
            :disabled="targetOptions.length <= 1"
            data-testid="contacts-import-target"
            @update:model-value="pickTarget(String($event))"
          />
        </li>
      </SettingsGroup>

      <SettingsGroup
        v-if="preview.items.length"
        :label="t('contacts.import.items')"
      >
        <ContactsShareImportItem
          v-for="item in preview.items"
          :key="item.id"
          :item="item"
          :decision="decisions.get(item.id)"
          @choose="onChoose(item.id, $event)"
          @replace="onReplace(item.id, $event)"
        />
      </SettingsGroup>

      <p
        v-if="preview.blocked"
        class="text-sm text-destructive"
        data-testid="contacts-import-blocked"
      >
        {{ t('contacts.import.blocked') }}
      </p>
      <p
        v-else-if="problem"
        class="text-sm text-destructive"
        data-testid="contacts-import-problem"
      >
        {{ t(`contacts.import.problem.${problem}`) }}
      </p>
      <p
        v-else-if="counts"
        class="text-sm text-muted-foreground"
        data-testid="contacts-import-summary"
      >
        {{ t('contacts.import.summary', counts) }}
      </p>

      <div class="flex flex-wrap items-center gap-2">
        <UiButton
          :loading="busy"
          :disabled="problem !== null"
          data-testid="contacts-import-accept"
          @click="acceptAsync"
        >
          {{ t('contacts.import.accept') }}
        </UiButton>
        <UiButton
          v-if="!preview.blocked && preview.vaultPubkey"
          variant="ghost"
          data-testid="contacts-import-block"
          @click="blockVaultAsync"
        >
          {{ t('contacts.import.block') }}
        </UiButton>
      </div>
    </template>
  </div>
</template>
