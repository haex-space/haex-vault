<script setup lang="ts">
/**
 * The rights group "Browser" of an agent (spec 065 FR-005, data-model.md): off by default; when on,
 * the websites the agent may open, each with "Lesen" or "Bedienen". A website over plain http can
 * only be read (research R9). Rust checks every change again.
 */
import { siteOf } from '~/lib/agents/rights'
import type { BrowserRights } from '~/types/bindings/BrowserRights'
import type { SiteLevel } from '~/types/bindings/SiteLevel'

const props = defineProps<{
  modelValue: BrowserRights | null
  disabled?: boolean
}>()
const emit = defineEmits<{ 'update:modelValue': [BrowserRights | null] }>()

const { t } = useI18n()
const fieldLabels = useFieldLabels()

const draft = ref('')
const problem = ref(false)

const sites = computed(() =>
  Object.entries(props.modelValue?.sites ?? {}).sort(([a], [b]) =>
    a.localeCompare(b),
  ),
)

const levelOptions = computed(() =>
  (['read', 'operate'] as const).map((value) => ({
    value,
    label: t(`mcpAgents.browser.level.${value}`),
  })),
)

function toggle(on: boolean) {
  emit('update:modelValue', on ? { userBrowser: false, sites: {} } : null)
}

function change(next: Partial<BrowserRights>) {
  if (!props.modelValue) return
  emit('update:modelValue', { ...props.modelValue, ...next })
}

function setLevel(site: string, level: string) {
  const current = props.modelValue?.sites[site]
  if (!current) return
  change({
    sites: {
      ...props.modelValue?.sites,
      [site]: { ...current, level: level as SiteLevel },
    },
  })
}

function remove(site: string) {
  const { [site]: _removed, ...rest } = props.modelValue?.sites ?? {}
  change({ sites: rest })
}

function add() {
  const site = siteOf(draft.value)
  problem.value = site === null
  if (!site) return
  const level: SiteLevel = 'read'
  change({
    sites: {
      ...props.modelValue?.sites,
      [site]: props.modelValue?.sites[site] ?? { level, fillRemembered: false },
    },
  })
  draft.value = ''
}

function plain(site: string): boolean {
  return site.startsWith('http://')
}
</script>

<template>
  <SettingsGroup :label="t('mcpAgents.browser.title')">
    <SettingsRow
      :title="t('mcpAgents.browser.enabled')"
      :description="t('mcpAgents.browser.enabledHint')"
      label-for="mcp-browser-switch"
    >
      <ShadcnSwitch
        id="mcp-browser-switch"
        :model-value="modelValue !== null"
        :disabled="disabled"
        data-testid="mcp-browser-switch"
        @update:model-value="toggle($event === true)"
      />
    </SettingsRow>
    <template v-if="modelValue">
      <SettingsRow
        v-for="[site, grant] in sites"
        :key="site"
        :title="site"
        data-testid="mcp-browser-site"
        :data-site="site"
      >
        <div class="flex items-center gap-2">
          <SettingsSelect
            :model-value="plain(site) ? 'read' : grant.level"
            class="w-40"
            :options="levelOptions"
            :disabled="disabled || plain(site)"
            :data-testid="`mcp-browser-level-${site}`"
            @update:model-value="setLevel(site, $event)"
          />
          <UiButton
            type="button"
            size="sm"
            variant="outline"
            :disabled="disabled"
            :aria-label="t('mcpAgents.browser.remove', { site })"
            @click="remove(site)"
          >
            {{ t('mcpAgents.browser.removeShort') }}
          </UiButton>
        </div>
      </SettingsRow>
      <SettingsRow
        v-if="sites.length === 0"
        :title="t('mcpAgents.browser.none')"
      />
      <div class="flex flex-col gap-1 p-4">
        <div class="flex items-end gap-2">
          <UiInput
            id="mcp-browser-site"
            v-model="draft"
            class="flex-1"
            :label="t('mcpAgents.browser.add')"
            :labels="fieldLabels.input.value"
            label-bg="var(--muted)"
            type="url"
            autocomplete="off"
            placeholder="https://shop.example.de"
            :disabled="disabled"
            data-testid="mcp-browser-new-site"
            @keydown.enter.prevent="add"
          />
          <UiButton
            type="button"
            :disabled="disabled || !draft.trim()"
            data-testid="mcp-browser-add"
            @click="add"
          >
            {{ t('mcpAgents.browser.addShort') }}
          </UiButton>
        </div>
        <p v-if="problem" class="text-xs text-destructive" role="alert">
          {{ t('mcpAgents.browser.invalid') }}
        </p>
        <p v-else class="text-xs text-muted-foreground">
          {{ t('mcpAgents.browser.hint') }}
        </p>
      </div>
    </template>
  </SettingsGroup>
</template>
