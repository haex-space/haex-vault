<script setup lang="ts">
/**
 * The pass part of a history state (spec 092, US5, FR-023): the kind, the start and the codes. A
 * code is a secret like a password: the state shows its format and label, and the value only
 * after "Show" (read by Rust from the stored state, hidden again when `resetKey` changes). A
 * state of a login has no such block and this renders nothing.
 */
import type { SnapshotPassView } from '@bindings/SnapshotPassView'
import { formatStart } from '~/lib/passwords/passDate'
import { PASS_KINDS } from '~/lib/passwords/passType'

const props = defineProps<{
  snapshotId: string
  pass: SnapshotPassView
  /** Changes when the codes must hide again (the history tab was left). */
  resetKey: number
}>()

const { t, locale } = useI18n()
const { historyRevealAsync } = usePasswords()

const deviceZone = Intl.DateTimeFormat().resolvedOptions().timeZone

const kindText = computed(() => {
  const kind = props.pass.passKind
  return kind !== null && (PASS_KINDS as readonly string[]).includes(kind)
    ? t(`passwords.kinds.${kind}`)
    : null
})

const start = computed(() => {
  const text = formatStart(props.pass, deviceZone, locale.value)
  if (text === null) return null
  return text.device ? `${text.place} (${text.device})` : text.place
})

function formatName(format: string): string {
  const known = ['qr', 'code128', 'ean13', 'pdf417', 'aztec'].includes(format)
  return known ? t(`passwords.pass.codeFormats.${format}`) : format
}
</script>

<template>
  <SettingsRow
    :title="t('passwords.history.fields.kindOfPass')"
    :description="kindText ?? t('passwords.noValue')"
    data-testid="passwords-history-pass-kind"
  />
  <SettingsRow
    :title="t('passwords.history.fields.start')"
    :description="start ?? t('passwords.noValue')"
    data-testid="passwords-history-pass-start"
  />
  <SettingsRow
    v-for="(code, index) in pass.codes"
    :key="`${index}:${code.format}:${code.label ?? ''}`"
    :title="code.label || formatName(code.format)"
    :description="code.label ? formatName(code.format) : undefined"
    :data-testid="`passwords-history-pass-code-${index}`"
  >
    <PasswordsMaskedValue
      :fetch="
        async () =>
          (await historyRevealAsync(snapshotId, { kind: 'code', index })).value
      "
      :identity="`${snapshotId}:code:${index}:${resetKey}`"
      :kind="`history-code-${index}`"
      :present="true"
      :label="code.label || formatName(code.format)"
    />
  </SettingsRow>
</template>
