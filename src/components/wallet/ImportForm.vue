<script setup lang="ts">
/**
 * One pass read from a file, before it is saved (spec 092, FR-031): what the file holds (kind,
 * start, codes by label, fields, what is not kept), and what the person may change (title, kind,
 * place, tag, the entry it belongs to, the zone of a moment). The proposal of FR-019 fills the
 * place and the tag; nothing is linked by itself. A code's value is never here, only its format and
 * label (FR-025).
 */
import { toast } from 'vue-sonner'
import type { PassImportPreview } from '@bindings/PassImportPreview'
import type { SignatureResult } from '@bindings/SignatureResult'
import { displayTitle } from '~/lib/passwords/format'
import { whenText } from '~/lib/passwords/passDate'
import { PASS_KINDS, passKindOf } from '~/lib/passwords/passType'
import { startFields } from '~/lib/wallet/importPlace'

export type ImportDraft = {
  title: string
  passKind: string
  /** The folders below Wallet as one line. */
  folder: string
  tag: string
  parentId: string | null
  /** The zone of a moment, as `+HH:MM` or an IANA name. */
  zone: string
}

const props = defineProps<{ preview: PassImportPreview }>()
const draft = defineModel<ImportDraft>('draft', { required: true })

const { t, locale } = useI18n()
const { errString } = useErrorString()
const { signatureAsync } = usePassImport()
const fieldLabels = useFieldLabels()
const store = usePasswordsStore()

const deviceZone = Intl.DateTimeFormat().resolvedOptions().timeZone
const pickingParent = ref(false)

const KIND_OPTIONS = PASS_KINDS.map((kind) => ({
  value: kind,
  label: t(`passwords.kinds.${kind}`),
}))

const isMoment = computed(() => {
  const start = props.preview.pass.start
  return start !== undefined && start !== null && 'at' in start
})

const when = computed(() =>
  whenText(
    {
      ...startFields(props.preview.pass.start),
      expiresAt: props.preview.pass.expiresAt,
    },
    deviceZone,
    locale.value,
  ),
)

const parentTitle = computed(() => {
  if (!draft.value.parentId) return null
  const header = store.headersById.get(draft.value.parentId)
  return header
    ? (displayTitle(header.title) ?? t('passwords.untitled'))
    : t('passwords.pass.links.notAvailable')
})

/** The offline check of the signature is off until the person turns it on; its result is shown
 * before saving and never stops the import (FR-030). */
const checkSignature = ref(false)
const signature = ref<SignatureResult | null>(null)
const checking = ref(false)

watch(checkSignature, async (on) => {
  signature.value = null
  if (!on) return
  checking.value = true
  try {
    const result = await signatureAsync(props.preview.previewId)
    // The switch may have been turned off while the check ran.
    if (checkSignature.value) signature.value = result
  } catch (cause) {
    checkSignature.value = false
    toast.error(errString(cause))
  } finally {
    checking.value = false
  }
})

function codeNote(flag: string | undefined | null): string | null {
  return flag ? t(`wallet.import.codeFlag.${flag}`) : null
}
</script>

<template>
  <div class="flex flex-col gap-4" data-testid="wallet-import-form">
    <SettingsGroup>
      <li class="px-4 py-3">
        <UiInput
          id="wallet-import-title"
          v-model="draft.title"
          :label="t('passwords.fields.title')"
          :labels="fieldLabels.input.value"
          label-bg="var(--muted)"
          data-testid="wallet-import-title"
        />
      </li>
      <li class="px-4 py-3">
        <UiSelect
          id="wallet-import-kind"
          :model-value="passKindOf({ passKind: draft.passKind })"
          :options="KIND_OPTIONS"
          :label="t('passwords.pass.kind')"
          label-bg="var(--muted)"
          data-testid="wallet-import-kind"
          @update:model-value="draft.passKind = $event ?? 'other'"
        />
      </li>
      <li
        v-if="when"
        class="px-4 py-3 text-sm"
        data-testid="wallet-import-when"
      >
        <span class="text-muted-foreground"
          >{{ t('passwords.pass.when') }}:</span
        >
        {{ when }}
      </li>
      <li v-if="isMoment" class="px-4 py-3">
        <UiInput
          id="wallet-import-zone"
          v-model="draft.zone"
          :label="t('wallet.import.zone')"
          :labels="fieldLabels.input.value"
          label-bg="var(--muted)"
          data-testid="wallet-import-zone"
        />
        <p class="px-1 pt-1 text-xs text-muted-foreground">
          {{ t('wallet.import.zoneHint') }}
        </p>
      </li>
    </SettingsGroup>

    <SettingsGroup :label="t('wallet.import.place')">
      <li class="px-4 py-3">
        <UiInput
          id="wallet-import-folder"
          v-model="draft.folder"
          :label="t('wallet.import.folder')"
          :labels="fieldLabels.input.value"
          label-bg="var(--muted)"
          data-testid="wallet-import-folder"
        />
        <p class="px-1 pt-1 text-xs text-muted-foreground">
          {{ t('wallet.import.folderHint') }}
        </p>
      </li>
      <li class="px-4 py-3">
        <UiInput
          id="wallet-import-tag"
          v-model="draft.tag"
          :label="t('wallet.import.tag')"
          :labels="fieldLabels.input.value"
          label-bg="var(--muted)"
          data-testid="wallet-import-tag"
        />
      </li>
    </SettingsGroup>

    <SettingsGroup :label="t('passwords.pass.links.belongsToTitle')">
      <li class="flex flex-wrap items-center gap-2 px-4 py-3">
        <span
          class="min-w-0 flex-1 truncate"
          :class="draft.parentId ? '' : 'text-muted-foreground'"
          data-testid="wallet-import-parent-title"
        >
          {{ parentTitle ?? t('passwords.pass.links.none') }}
        </span>
        <UiButton
          type="button"
          variant="outline"
          size="sm"
          data-testid="wallet-import-parent-choose"
          @click="pickingParent = true"
        >
          {{ t('passwords.pass.links.choose') }}
        </UiButton>
        <UiButton
          v-if="draft.parentId"
          type="button"
          variant="outline"
          size="sm"
          @click="draft.parentId = null"
        >
          {{ t('passwords.pass.links.clear') }}
        </UiButton>
      </li>
    </SettingsGroup>

    <SettingsGroup :label="t('wallet.import.signature.title')">
      <SettingsRow
        :title="t('wallet.import.signature.check')"
        :description="t('wallet.import.signature.hint')"
        label-for="wallet-import-signature-check"
      >
        <ShadcnSwitch
          id="wallet-import-signature-check"
          :model-value="checkSignature"
          data-testid="wallet-import-signature-check"
          @update:model-value="checkSignature = $event === true"
        />
      </SettingsRow>
      <li
        v-if="checking"
        class="px-4 py-3 text-sm text-muted-foreground"
        data-testid="wallet-import-signature-checking"
      >
        {{ t('wallet.import.signature.checking') }}
      </li>
      <li
        v-else-if="signature"
        class="flex flex-col px-4 py-3 text-sm"
        :data-state="signature.state"
        role="status"
        data-testid="wallet-import-signature"
      >
        <span>{{ t(`wallet.import.signature.state.${signature.state}`) }}</span>
        <span v-if="signature.issuer" class="text-xs text-muted-foreground">
          {{
            t('wallet.import.signature.issuer', { issuer: signature.issuer })
          }}
        </span>
        <span v-if="signature.reason" class="text-xs text-muted-foreground">
          {{ t(`wallet.import.signature.reason.${signature.reason}`) }}
        </span>
      </li>
    </SettingsGroup>

    <SettingsGroup :label="t('wallet.import.codes')">
      <li
        v-if="preview.pass.codes.length === 0"
        class="px-4 py-3 text-sm text-muted-foreground"
      >
        {{ t('wallet.import.noCodes') }}
      </li>
      <li
        v-for="(code, index) in preview.pass.codes"
        :key="index"
        class="flex flex-col px-4 py-3 text-sm"
        :data-testid="`wallet-import-code-${index}`"
      >
        <span>{{ code.label ?? t('wallet.import.codeNoLabel') }}</span>
        <span class="text-xs text-muted-foreground">
          {{ code.format }}
          <template v-if="codeNote(code.flag)">
            · {{ codeNote(code.flag) }}</template
          >
        </span>
      </li>
    </SettingsGroup>

    <SettingsGroup
      v-if="preview.pass.fields.length"
      :label="t('wallet.import.fields')"
    >
      <li
        v-for="field in preview.pass.fields"
        :key="field.name"
        class="flex items-baseline justify-between gap-4 px-4 py-2 text-sm"
      >
        <span class="text-muted-foreground">{{ field.name }}</span>
        <span class="min-w-0 truncate">{{ field.value }}</span>
      </li>
    </SettingsGroup>

    <SettingsGroup
      v-if="preview.notKept.length"
      :label="t('wallet.import.notKept')"
    >
      <li
        v-for="item in preview.notKept"
        :key="item.what"
        class="flex flex-col px-4 py-2 text-sm"
        data-testid="wallet-import-not-kept"
      >
        <span>{{ item.what }}</span>
        <span class="text-xs text-muted-foreground">{{
          t(`wallet.import.why.${item.why}`)
        }}</span>
      </li>
    </SettingsGroup>

    <WalletParentPicker
      v-model:open="pickingParent"
      :item-id="null"
      :title="draft.title"
      url=""
      @pick="draft.parentId = $event"
    />
  </div>
</template>
