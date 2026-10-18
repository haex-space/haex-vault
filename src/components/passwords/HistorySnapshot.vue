<script setup lang="ts">
/**
 * One state of an entry, read only (spec 036, US2, FR-007; spec 034 FR-017): its time, the fields
 * with the secrets masked until asked (and hidden again when the state or the tab changes: the
 * parent changes `resetKey`), a copy button per value (a secret is copied by Rust without passing
 * the webview), and "Restore". The restore button only asks; the parent confirms and writes.
 */
import type { SnapshotView } from '@bindings/SnapshotView'

defineProps<{
  snapshot: SnapshotView
  restoring: boolean
  readonly?: boolean
  /** Changes when the secrets must hide again (the Verlauf tab was left). */
  resetKey: number
}>()
const emit = defineEmits<{ restore: [] }>()

const { t, d } = useI18n()
const { historyRevealAsync } = usePasswords()
const { copyText, copyHistory } = usePasswordsCopy()

function when(stamp: string | null): string {
  if (!stamp) return '–'
  const date = new Date(stamp)
  return Number.isNaN(date.getTime())
    ? stamp
    : d(date, { dateStyle: 'medium', timeStyle: 'medium' })
}
</script>

<template>
  <div
    class="flex min-w-0 flex-col gap-4"
    data-testid="passwords-history-snapshot"
  >
    <div class="flex flex-col items-start gap-2">
      <p
        class="text-xs font-normal text-muted-foreground"
        data-testid="passwords-history-saved-at"
      >
        {{
          t('passwords.history.savedAt', { when: when(snapshot.modifiedAt) })
        }}
      </p>
      <UiButton
        v-if="!readonly"
        :loading="restoring"
        data-testid="passwords-history-restore"
        @click="emit('restore')"
      >
        <Icon name="lucide:rotate-ccw" class="size-4" />
        {{ t('passwords.history.restore') }}
      </UiButton>
    </div>
    <SettingsGroup>
      <SettingsRow
        :title="t('passwords.fields.title')"
        :description="snapshot.title ?? t('passwords.noValue')"
      >
        <PasswordsCopyButton
          v-if="snapshot.title"
          :label="t('passwords.fields.title')"
          @copy="copyText(snapshot.title, t('passwords.fields.title'))"
        />
      </SettingsRow>
      <SettingsRow
        :title="t('passwords.fields.username')"
        :description="snapshot.username ?? t('passwords.noValue')"
      >
        <PasswordsCopyButton
          v-if="snapshot.username"
          :label="t('passwords.fields.username')"
          @copy="
            copyText(snapshot.username, t('passwords.fields.username'), {
              itemId: snapshot.itemId,
              field: { kind: 'username' },
            })
          "
        />
      </SettingsRow>
      <SettingsRow :title="t('passwords.fields.password')">
        <PasswordsMaskedValue
          :fetch="
            async () =>
              (await historyRevealAsync(snapshot.id, { kind: 'password' }))
                .value
          "
          :identity="`${snapshot.id}:password:${resetKey}`"
          kind="history-password"
          :present="snapshot.hasPassword"
          :label="t('passwords.fields.password')"
        />
        <PasswordsCopyButton
          v-if="snapshot.hasPassword"
          :label="t('passwords.fields.password')"
          @copy="
            copyHistory(
              snapshot.id,
              { kind: 'password' },
              t('passwords.fields.password'),
            )
          "
        />
      </SettingsRow>
      <SettingsRow
        :title="t('passwords.fields.url')"
        :description="snapshot.url ?? t('passwords.noValue')"
      >
        <PasswordsCopyButton
          v-if="snapshot.url"
          :label="t('passwords.fields.url')"
          @copy="
            copyText(snapshot.url, t('passwords.fields.url'), {
              itemId: snapshot.itemId,
              field: { kind: 'url' },
            })
          "
        />
      </SettingsRow>
      <SettingsRow
        v-if="snapshot.hasOtpSecret"
        :title="t('passwords.fields.totp')"
      >
        <PasswordsMaskedValue
          :fetch="
            async () =>
              (await historyRevealAsync(snapshot.id, { kind: 'otpSecret' }))
                .value
          "
          :identity="`${snapshot.id}:otp:${resetKey}`"
          kind="history-otp"
          :present="true"
          :label="t('passwords.fields.totp')"
        />
        <PasswordsCopyButton
          :label="t('passwords.history.fields.otpSecret')"
          @copy="
            copyHistory(
              snapshot.id,
              { kind: 'otpSecret' },
              t('passwords.history.fields.otpSecret'),
            )
          "
        />
      </SettingsRow>
      <SettingsRow
        v-if="snapshot.expiresAt"
        :title="t('passwords.fields.expires')"
        :description="snapshot.expiresAt"
      >
        <PasswordsCopyButton
          :label="t('passwords.fields.expires')"
          @copy="copyText(snapshot.expiresAt, t('passwords.fields.expires'))"
        />
      </SettingsRow>
      <SettingsRow
        v-if="snapshot.tags.length"
        :title="t('passwords.fields.tags')"
        :description="snapshot.tags.join(', ')"
      >
        <PasswordsCopyButton
          :label="t('passwords.fields.tags')"
          @copy="copyText(snapshot.tags.join(', '), t('passwords.fields.tags'))"
        />
      </SettingsRow>
      <SettingsRow
        v-for="(field, index) in snapshot.keyValues"
        :key="index"
        :title="field.key ?? ''"
      >
        <PasswordsMaskedValue
          :fetch="
            async () =>
              (
                await historyRevealAsync(snapshot.id, {
                  kind: 'keyValue',
                  key: field.key ?? '',
                })
              ).value
          "
          :identity="`${snapshot.id}:kv:${field.key}:${resetKey}`"
          kind="history-keyvalue"
          :present="field.hasValue"
          :label="field.key ?? ''"
        />
        <PasswordsCopyButton
          v-if="field.hasValue"
          :label="field.key ?? ''"
          @copy="
            copyHistory(
              snapshot.id,
              { kind: 'keyValue', key: field.key ?? '' },
              field.key ?? '',
            )
          "
        />
      </SettingsRow>
      <SettingsRow
        v-for="attachment in snapshot.attachments"
        :key="attachment.binaryHash + attachment.fileName"
        :title="attachment.fileName"
        :description="
          attachment.available
            ? undefined
            : t('passwords.history.attachmentGone')
        "
        icon="lucide:paperclip"
      />
    </SettingsGroup>
    <div
      v-if="snapshot.note"
      class="relative rounded-xl bg-muted px-4 py-3 pr-14 text-sm whitespace-pre-wrap"
    >
      <PasswordsCopyButton
        class="absolute top-1.5 right-2"
        :label="t('passwords.fields.note')"
        @copy="
          copyText(snapshot.note, t('passwords.fields.note'), {
            itemId: snapshot.itemId,
            field: { kind: 'note' },
          })
        "
      />
      {{ snapshot.note }}
    </div>
  </div>
</template>
