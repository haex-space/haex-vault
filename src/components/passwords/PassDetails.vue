<script setup lang="ts">
/**
 * The tab Details of a pass in view mode (spec 092, FR-002, FR-008, FR-012): the kind, the start in
 * the time of the place (and in the device's time when the zones differ, both labelled), the end,
 * the address, the codes, the note and the tags. Empty fields are not shown. Every value copies
 * (`usePasswordsCopy`), a stored field by its field, a code by its value on the one screen that
 * loaded it.
 */
import { openUrl } from '@tauri-apps/plugin-opener'
import { toast } from 'vue-sonner'
import type { ItemDetail } from '@bindings/ItemDetail'
import { openableUrl } from '~/lib/passwords/format'
import { formatStart } from '~/lib/passwords/passDate'
import { PASS_KIND_ICONS, passKindOf } from '~/lib/passwords/passType'

const props = defineProps<{
  itemId: string
  detail: ItemDetail
}>()

const { t, locale } = useI18n()
const { copyField, copyText } = usePasswordsCopy()

const kind = computed(() => passKindOf(props.detail))
const start = computed(() =>
  formatStart(
    props.detail,
    Intl.DateTimeFormat().resolvedOptions().timeZone,
    locale.value,
  ),
)

async function openUrlAsync() {
  try {
    const url = openableUrl(props.detail.url)
    if (!url) throw new Error('not a web address')
    await openUrl(url)
  } catch {
    toast.error(t('passwords.openUrlFailed'))
  }
}
</script>

<template>
  <div class="flex flex-col gap-4" data-testid="entry-view-details">
    <SettingsGroup>
      <SettingsRow :title="t('passwords.pass.kind')">
        <span
          class="flex min-w-0 items-center gap-2"
          data-no-swipe
          data-testid="passwords-pass-kind"
        >
          <Icon :name="PASS_KIND_ICONS[kind]" class="size-4 shrink-0" />
          <span class="truncate">{{ t(`passwords.kinds.${kind}`) }}</span>
        </span>
      </SettingsRow>
      <SettingsRow v-if="start" :title="t('passwords.pass.start')">
        <span class="flex min-w-0 flex-col items-end text-right" data-no-swipe>
          <span data-testid="passwords-pass-start">{{ start.place }}</span>
          <span
            v-if="start.device"
            class="text-sm text-muted-foreground"
            data-testid="passwords-pass-start-device"
            >{{ start.device }}</span
          >
        </span>
        <PasswordsCopyButton
          :label="t('passwords.pass.start')"
          @copy="
            copyText(
              start.device ? `${start.place} (${start.device})` : start.place,
              t('passwords.pass.start'),
            )
          "
        />
      </SettingsRow>
      <SettingsRow v-if="detail.expiresAt" :title="t('passwords.pass.expires')">
        <span data-no-swipe data-testid="passwords-pass-expires">{{
          detail.expiresAt
        }}</span>
        <PasswordsCopyButton
          :label="t('passwords.pass.expires')"
          @copy="copyText(detail.expiresAt, t('passwords.pass.expires'))"
        />
      </SettingsRow>
      <SettingsRow v-if="detail.url" :title="t('passwords.fields.url')">
        <span
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
          @copy="copyField(itemId, { kind: 'url' }, t('passwords.fields.url'))"
        />
      </SettingsRow>
    </SettingsGroup>

    <PasswordsPassCodes :item-id="itemId" :updated-at="detail.updatedAt" />

    <SettingsGroup v-if="detail.note" :label="t('passwords.fields.note')">
      <li
        class="relative px-4 py-3 pr-14 text-sm whitespace-pre-wrap"
        data-no-swipe
        data-testid="passwords-value-note"
      >
        <PasswordsCopyButton
          class="absolute top-1.5 right-2"
          :label="t('passwords.fields.note')"
          @copy="
            copyField(itemId, { kind: 'note' }, t('passwords.fields.note'))
          "
        />
        {{ detail.note }}
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
