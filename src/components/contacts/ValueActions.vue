<script setup lang="ts">
/**
 * What a value of a contact offers (spec 051, FR-006): copy it everywhere; on a phone call, write
 * a message or open the map with the apps of the system; e-mail addresses and web pages open on
 * every device.
 */
import { openUrl } from '@tauri-apps/plugin-opener'
import { toast } from 'vue-sonner'
import { displayValue, openTargets } from '~/lib/contacts/kinds'

const props = defineProps<{
  kind: string
  value: string
  /** What the value is, for the labels of the buttons. */
  label: string
}>()

const { t, locale } = useI18n()
const { isAndroid } = useDeviceCapabilities()

const ICONS = {
  call: 'lucide:phone',
  message: 'lucide:message-square',
  mail: 'lucide:mail',
  map: 'lucide:map',
  web: 'lucide:external-link',
} as const

/** Calls, messages and maps only where a phone has the apps for them. */
const targets = computed(() =>
  openTargets(props.kind, props.value).filter(
    (target) =>
      isAndroid.value || target.action === 'mail' || target.action === 'web',
  ),
)

async function copyAsync() {
  try {
    await navigator.clipboard.writeText(
      displayValue(props.kind, props.value, locale.value),
    )
    toast.success(t('contacts.copied', { what: props.label }))
  } catch {
    toast.error(t('contacts.copyFailed'))
  }
}

async function openAsync(url: string) {
  try {
    await openUrl(url)
  } catch {
    toast.error(t('contacts.openFailed'))
  }
}
</script>

<template>
  <div class="flex shrink-0 items-center gap-1">
    <UiButton
      v-for="target in targets"
      :key="target.action"
      variant="ghost"
      size="icon"
      :aria-label="t(`contacts.open.${target.action}`, { what: label })"
      :tooltip="t(`contacts.open.${target.action}`, { what: label })"
      :data-testid="`contacts-open-${target.action}`"
      @click="openAsync(target.url)"
    >
      <Icon :name="ICONS[target.action]" class="size-4" />
    </UiButton>
    <UiButton
      variant="ghost"
      size="icon"
      :aria-label="t('contacts.copy', { what: label })"
      :tooltip="t('contacts.copy', { what: label })"
      data-testid="contacts-copy"
      @click="copyAsync"
    >
      <Icon name="lucide:copy" class="size-4" />
    </UiButton>
  </div>
</template>
