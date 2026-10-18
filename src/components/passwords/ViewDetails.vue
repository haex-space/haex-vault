<script setup lang="ts">
/**
 * The tab Details of an entry in view mode (spec 036, FR-001, FR-005; fields from spec 034 FR-002):
 * username, password (masked until asked), address, expiry, the live TOTP block, the note and the
 * tags. Empty fields are not shown, the password neither, and a tab without any says so. Every
 * field copies (`usePasswordsCopy`), a stored value by its field, so no value passes through here
 * except what the masked value asks for.
 */
import { openUrl } from '@tauri-apps/plugin-opener'
import { toast } from 'vue-sonner'
import type { ItemDetail } from '@bindings/ItemDetail'
import { openableUrl } from '~/lib/passwords/format'

const props = defineProps<{
  itemId: string
  detail: ItemDetail
}>()
const emit = defineEmits<{
  edit: []
  removeOtp: []
}>()

const { t } = useI18n()
const { revealAsync } = usePasswords()
const { copyField, copyText } = usePasswordsCopy()

/** Opens the address in the browser; an address with references is resolved first. */
async function openUrlAsync() {
  try {
    const value = props.detail.references.url.length
      ? (await revealAsync(props.itemId, { kind: 'url' })).value
      : props.detail.url
    const url = openableUrl(value)
    if (!url) throw new Error('not a web address')
    await openUrl(url)
  } catch {
    toast.error(t('passwords.openUrlFailed'))
  }
}

const isEmpty = computed(
  () =>
    !props.detail.username &&
    !props.detail.hasPassword &&
    !props.detail.url &&
    !props.detail.expiresAt &&
    props.detail.otpState === 'none' &&
    !props.detail.note &&
    props.detail.tags.length === 0,
)
</script>

<template>
  <div class="flex flex-col gap-4" data-testid="entry-view-details">
    <p
      v-if="isEmpty"
      class="py-6 text-center text-sm text-muted-foreground"
      data-testid="passwords-details-empty"
    >
      {{ t('passwords.tabs.detailsEmpty') }}
    </p>
    <SettingsGroup
      v-if="
        detail.username || detail.hasPassword || detail.url || detail.expiresAt
      "
    >
      <SettingsRow
        v-if="detail.username"
        :title="t('passwords.fields.username')"
      >
        <template v-if="detail.references.username.length" #below>
          <PasswordsReferenceValue
            :marks="detail.references.username"
            :text="detail.username"
            kind="username"
          />
        </template>
        <PasswordsMaskedValue
          v-if="detail.references.username.length"
          :fetch="
            async () => (await revealAsync(itemId, { kind: 'username' })).value
          "
          :identity="`${itemId}:username:${detail.username}`"
          kind="username-resolved"
          present
          :label="t('passwords.fields.username')"
        />
        <span
          v-else
          class="min-w-0 truncate"
          data-no-swipe
          data-testid="passwords-value-username"
          >{{ detail.username }}</span
        >
        <PasswordsCopyButton
          :label="t('passwords.fields.username')"
          data-testid="passwords-copy-username"
          @copy="
            copyField(
              itemId,
              { kind: 'username' },
              t('passwords.fields.username'),
            )
          "
        />
      </SettingsRow>
      <SettingsRow
        v-if="detail.hasPassword"
        :title="t('passwords.fields.password')"
      >
        <template v-if="detail.references.password.length" #below>
          <PasswordsReferenceValue
            :marks="detail.references.password"
            :text="null"
            kind="password"
          />
        </template>
        <PasswordsMaskedValue
          :fetch="
            async () => (await revealAsync(itemId, { kind: 'password' })).value
          "
          :identity="`${itemId}:password`"
          kind="password"
          present
          :label="t('passwords.fields.password')"
        />
        <PasswordsCopyButton
          :label="t('passwords.fields.password')"
          data-testid="passwords-copy-password"
          @copy="
            copyField(
              itemId,
              { kind: 'password' },
              t('passwords.fields.password'),
            )
          "
        />
      </SettingsRow>
      <SettingsRow v-if="detail.url" :title="t('passwords.fields.url')">
        <template v-if="detail.references.url.length" #below>
          <PasswordsReferenceValue
            :marks="detail.references.url"
            :text="detail.url"
            kind="url"
          />
        </template>
        <PasswordsMaskedValue
          v-if="detail.references.url.length"
          :fetch="
            async () => (await revealAsync(itemId, { kind: 'url' })).value
          "
          :identity="`${itemId}:url:${detail.url}`"
          kind="url-resolved"
          present
          :label="t('passwords.fields.url')"
        />
        <span
          v-else
          class="min-w-0 truncate"
          data-no-swipe
          data-testid="passwords-value-url"
          >{{ detail.url }}</span
        >
        <UiButton
          variant="ghost"
          size="icon"
          :aria-label="t('passwords.openUrl')"
          :tooltip="t('passwords.openUrl')"
          data-testid="passwords-open-url"
          @click="openUrlAsync"
        >
          <Icon name="lucide:external-link" class="size-4" />
        </UiButton>
        <PasswordsCopyButton
          :label="t('passwords.fields.url')"
          data-testid="passwords-copy-url"
          @copy="copyField(itemId, { kind: 'url' }, t('passwords.fields.url'))"
        />
      </SettingsRow>
      <SettingsRow
        v-if="detail.expiresAt"
        :title="t('passwords.fields.expires')"
      >
        <span data-no-swipe>{{ detail.expiresAt }}</span>
        <PasswordsCopyButton
          :label="t('passwords.fields.expires')"
          @copy="copyText(detail.expiresAt, t('passwords.fields.expires'))"
        />
      </SettingsRow>
    </SettingsGroup>

    <SettingsGroup
      v-if="detail.otpState !== 'none'"
      :label="t('passwords.fields.totp')"
    >
      <li class="px-4 py-3" data-no-swipe>
        <PasswordsTotpCode
          :item-id="itemId"
          :state="detail.otpState"
          @copy="
            copyField(itemId, { kind: 'totp' }, t('passwords.fields.totp'))
          "
          @replace="emit('edit')"
          @remove="emit('removeOtp')"
        />
      </li>
    </SettingsGroup>

    <SettingsGroup v-if="detail.note" :label="t('passwords.fields.note')">
      <li
        class="relative px-4 py-3 pr-14 text-sm whitespace-pre-wrap"
        data-no-swipe
        data-testid="passwords-value-note"
      >
        <PasswordsCopyButton
          class="absolute top-1.5 right-2"
          :label="t('passwords.fields.note')"
          data-testid="passwords-copy-note"
          @copy="
            copyField(itemId, { kind: 'note' }, t('passwords.fields.note'))
          "
        />
        <template v-if="detail.references.note.length">
          <PasswordsReferenceValue
            :marks="detail.references.note"
            :text="detail.note"
            kind="note"
          />
          <PasswordsMaskedValue
            class="mt-2"
            :fetch="
              async () => (await revealAsync(itemId, { kind: 'note' })).value
            "
            :identity="`${itemId}:note:${detail.note}`"
            kind="note-resolved"
            present
            :label="t('passwords.fields.note')"
          />
        </template>
        <template v-else>{{ detail.note }}</template>
      </li>
    </SettingsGroup>

    <div
      v-if="detail.tags.length"
      class="flex flex-wrap gap-1.5"
      data-testid="passwords-entry-tags"
    >
      <ShadcnBadge v-for="tag in detail.tags" :key="tag.id" variant="secondary">
        {{ tag.name }}
      </ShadcnBadge>
    </div>
  </div>
</template>
