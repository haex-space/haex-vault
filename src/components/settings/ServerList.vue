<script setup lang="ts">
/**
 * One kind of server (spec 024, FR-008) as a list with an input to add one: in the settings, where
 * each change is saved at once, and in the link form of the landing page, where it only applies to
 * the link. The built-in servers are always listed and can be switched off, not removed; added
 * ones can be switched off or removed. Switched off ones stay listed.
 */
import type { ServerEntry } from '~/lib/sync/serverEntries'

interface Props {
  label: string
  description: string
  entries: ServerEntry[]
  placeholder: string
  /** Said while every server is switched off. */
  noneNote: string
  /** Prefix of the test ids, e.g. `settings-servers-nostrRelays`. */
  testId: string
  busy?: boolean
  /** Takes an entered URL; the input is cleared when it returns `true`. */
  addAsync: (url: string) => Promise<boolean>
}

const props = defineProps<Props>()
const emit = defineEmits<{
  toggle: [url: string, enabled: boolean]
  remove: [url: string]
}>()

const { t } = useI18n()
const fieldLabels = useFieldLabels()
const draft = ref('')
const noneInUse = computed(() => props.entries.every((e) => !e.enabled))

async function onAdd() {
  const url = draft.value.trim()
  if (!url || props.busy || props.entries.some((e) => e.url === url)) return
  if (await props.addAsync(url)) draft.value = ''
}
</script>

<template>
  <SettingsGroup :label="props.label">
    <li class="px-4 py-3 text-sm text-muted-foreground">
      {{ props.description }}
    </li>
    <li
      v-for="entry in props.entries"
      :key="entry.url"
      class="flex items-center gap-2 pr-4 hover:bg-foreground/5"
      :data-testid="`${props.testId}-${entry.isDefault ? 'default' : 'added'}`"
      :data-enabled="entry.enabled"
    >
      <label
        class="flex min-h-14 min-w-0 flex-1 cursor-pointer items-center gap-4 px-4 py-3 has-disabled:cursor-default"
      >
        <ShadcnCheckbox
          :model-value="entry.enabled"
          :disabled="props.busy"
          :aria-label="t('settings.federation.servers.use', { url: entry.url })"
          :data-testid="`${props.testId}-toggle`"
          @update:model-value="emit('toggle', entry.url, $event === true)"
        />
        <span
          class="flex min-w-0 flex-1 flex-col"
          :class="{ 'opacity-60': !entry.enabled }"
        >
          <span class="break-all">{{ entry.url }}</span>
          <span v-if="entry.isDefault" class="text-sm text-muted-foreground">
            {{ t('settings.federation.servers.default') }}
          </span>
        </span>
      </label>
      <DeleteButton
        :label="t('settings.federation.servers.remove', { url: entry.url })"
        :placeholder="entry.isDefault"
        :disabled="props.busy"
        :data-testid="entry.isDefault ? undefined : `${props.testId}-remove`"
        @click="emit('remove', entry.url)"
      />
    </li>
    <li
      v-if="noneInUse"
      class="px-4 py-3 text-sm text-muted-foreground"
      :data-testid="`${props.testId}-none`"
    >
      {{ props.noneNote }}
    </li>
    <li class="flex items-center gap-2 px-4 py-3">
      <UiInput
        v-model="draft"
        :placeholder="props.placeholder"
        :aria-label="props.label"
        :labels="fieldLabels.input.value"
        autocomplete="off"
        spellcheck="false"
        :data-testid="`${props.testId}-input`"
        @keydown.enter.prevent="onAdd"
      />
      <UiButton
        type="button"
        :disabled="props.busy || draft.trim() === ''"
        :data-testid="`${props.testId}-add`"
        @click="onAdd"
      >
        {{ t('settings.federation.servers.add') }}
      </UiButton>
    </li>
  </SettingsGroup>
</template>
