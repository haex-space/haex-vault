<script setup lang="ts">
/**
 * The autofill aliases of an entry in the tab Extra (spec 036, FR-001a): per fillable value the
 * names of the input fields the browser extension fills with it. Editing offers user name,
 * password, TOTP code, every custom field of the draft and then stored keys that have no field
 * any more; the view lists only rows with names and nothing when there are none (FR-005).
 */
const props = defineProps<{
  /** Names of the custom fields of the entry. */
  fieldKeys: string[]
  readonly?: boolean
}>()

const aliases = defineModel<Record<string, string[]>>({ required: true })

const { t } = useI18n()

const FIXED: Record<string, string> = {
  username: 'passwords.fields.username',
  password: 'passwords.fields.password',
  otp: 'passwords.fields.totp',
}

const inputs = ref<Record<string, string>>({})

const rows = computed(() => {
  const keys = [...Object.keys(FIXED)]
  for (const key of props.fieldKeys.map((k) => k.trim())) {
    if (key && !keys.includes(key)) keys.push(key)
  }
  const withField = new Set(keys)
  for (const key of Object.keys(aliases.value)) {
    if (!keys.includes(key)) keys.push(key)
  }
  return keys
    .map((key) => ({
      key,
      label: Object.hasOwn(FIXED, key) ? t(FIXED[key]!) : key,
      names: Object.hasOwn(aliases.value, key) ? aliases.value[key]! : [],
      hasField: withField.has(key),
    }))
    .filter((row) => !props.readonly || row.names.length)
})

function add(key: string) {
  const name = (
    Object.hasOwn(inputs.value, key) ? (inputs.value[key] ?? '') : ''
  ).trim()
  inputs.value = { ...inputs.value, [key]: '' }
  if (!name) return
  const names = Object.hasOwn(aliases.value, key) ? aliases.value[key]! : []
  if (names.some((known) => known.toLowerCase() === name.toLowerCase())) return
  aliases.value = { ...aliases.value, [key]: [...names, name] }
}

function remove(key: string, name: string) {
  const names = Object.hasOwn(aliases.value, key) ? aliases.value[key]! : []
  const rest = Object.fromEntries(
    Object.entries(aliases.value).filter(([aliasKey]) => aliasKey !== key),
  )
  const kept = names.filter((known) => known !== name)
  aliases.value = kept.length ? { ...rest, [key]: kept } : rest
}
</script>

<template>
  <div
    v-if="rows.length"
    class="flex flex-col gap-2"
    data-testid="passwords-aliases"
  >
    <SettingsGroup :label="t('passwords.aliases.title')">
      <SettingsRow
        v-for="(row, index) in rows"
        :key="row.key"
        :title="row.label"
        :description="row.hasField ? undefined : t('passwords.aliases.noField')"
      >
        <template v-if="row.names.length" #below>
          <div class="flex flex-wrap gap-1.5" data-no-swipe>
            <ShadcnBadge
              v-for="name in row.names"
              :key="name"
              variant="secondary"
              class="gap-1 font-mono select-text"
              :data-testid="`passwords-alias-${index}`"
            >
              {{ name }}
              <button
                v-if="!readonly"
                type="button"
                class="rounded-full hover:text-destructive"
                :aria-label="t('passwords.aliases.remove', { name })"
                :data-testid="`passwords-alias-remove-${index}`"
                @click="remove(row.key, name)"
              >
                <Icon name="lucide:x" class="size-3" />
              </button>
            </ShadcnBadge>
          </div>
        </template>
        <div v-if="!readonly" class="w-full md:w-56" data-no-swipe>
          <UiInput
            :model-value="inputs[row.key] ?? ''"
            autocomplete="off"
            :placeholder="t('passwords.aliases.placeholder')"
            :aria-label="t('passwords.aliases.add', { field: row.label })"
            :data-testid="`passwords-alias-input-${index}`"
            @update:model-value="
              inputs = { ...inputs, [row.key]: String($event ?? '') }
            "
            @keydown.enter.prevent="add(row.key)"
          />
        </div>
      </SettingsRow>
    </SettingsGroup>
    <p v-if="!readonly" class="px-1 text-sm text-muted-foreground">
      {{ t('passwords.aliases.hint') }}
    </p>
  </div>
</template>
