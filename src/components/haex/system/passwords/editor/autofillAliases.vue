<template>
  <div class="space-y-4">
    <div
      v-for="field in allFields"
      :key="field.key"
      class="space-y-1.5"
    >
      <div class="flex items-center gap-1.5">
        <UIcon
          :name="field.icon"
          class="size-3.5 text-muted"
        />
        <span class="text-sm font-medium">{{ field.label }}</span>
        <UBadge
          v-if="field.isCustom"
          :label="t('customField')"
          color="neutral"
          variant="soft"
          size="sm"
        />
      </div>

      <div class="flex flex-wrap gap-1.5 min-h-7">
        <div
          v-for="alias in (aliases[field.key] ?? [])"
          :key="alias"
          class="inline-flex items-center gap-1 px-2 py-0.5 rounded-full bg-elevated border border-default text-sm"
        >
          <span>{{ alias }}</span>
          <button
            v-if="!readOnly"
            type="button"
            class="text-muted hover:text-default leading-none"
            @click="removeAlias(field.key, alias)"
          >
            ×
          </button>
        </div>

        <p
          v-if="!aliases[field.key]?.length && readOnly"
          class="text-sm text-muted italic"
        >
          {{ t('noAliases') }}
        </p>
      </div>

      <div
        v-if="!readOnly"
        class="flex gap-1"
      >
        <UInput
          v-model="newAlias[field.key]"
          :placeholder="t('placeholder')"
          size="sm"
          class="flex-1"
          @keydown.enter.prevent="addAlias(field.key)"
        />
        <UiButton
          icon="i-lucide-plus"
          size="sm"
          color="neutral"
          variant="outline"
          type="button"
          @click="addAlias(field.key)"
        />
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
type EditableKeyValue = { id: string; key: string; value: string }

const props = defineProps<{
  keyValues?: EditableKeyValue[]
  readOnly?: boolean
}>()

const aliases = defineModel<Record<string, string[]>>({ default: () => ({}) })

const { t } = useI18n()

const DEFAULT_ALIASES: Record<string, string[]> = {
  username: ['email', 'login', 'user', 'e-mail', 'mail'],
  password: ['pass', 'pwd', 'secret'],
  otpSecret: ['otp', 'totp', '2fa', 'code', 'token'],
}

const standardFields = computed(() => [
  { key: 'username', label: t('fields.username'), icon: 'i-lucide-user', isCustom: false },
  { key: 'password', label: t('fields.password'), icon: 'i-lucide-lock', isCustom: false },
  { key: 'otpSecret', label: t('fields.otp'), icon: 'i-lucide-timer', isCustom: false },
])

const customFields = computed(() =>
  (props.keyValues ?? [])
    .filter((kv) => kv.key.trim())
    .map((kv) => ({
      key: kv.key,
      label: kv.key,
      icon: 'i-lucide-key-round',
      isCustom: true,
    })),
)

const allFields = computed(() => [...standardFields.value, ...customFields.value])

// Seed default aliases for standard fields if not yet customised
watch(
  allFields,
  (fields) => {
    const current = { ...aliases.value }
    let changed = false
    for (const field of fields) {
      if (!field.isCustom && !current[field.key]) {
        const defaults = DEFAULT_ALIASES[field.key]
        if (defaults?.length) {
          current[field.key] = [...defaults]
          changed = true
        }
      }
    }
    if (changed) aliases.value = current
  },
  { immediate: true },
)

const newAlias = ref<Record<string, string>>({})

const addAlias = (fieldKey: string) => {
  const raw = (newAlias.value[fieldKey] ?? '').trim()
  if (!raw) return
  const current = aliases.value[fieldKey] ?? []
  if (current.includes(raw)) { newAlias.value[fieldKey] = ''; return }
  aliases.value = { ...aliases.value, [fieldKey]: [...current, raw] }
  newAlias.value[fieldKey] = ''
}

const removeAlias = (fieldKey: string, alias: string) => {
  const current = aliases.value[fieldKey] ?? []
  aliases.value = { ...aliases.value, [fieldKey]: current.filter((a) => a !== alias) }
}
</script>

<i18n lang="yaml">
de:
  placeholder: Alias hinzufügen...
  customField: Benutzerdefiniert
  noAliases: Keine Aliases
  fields:
    username: Nutzername
    password: Passwort
    otp: OTP/2FA

en:
  placeholder: Add alias...
  customField: Custom
  noAliases: No aliases
  fields:
    username: Username
    password: Password
    otp: OTP/2FA
</i18n>
