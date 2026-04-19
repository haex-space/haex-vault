<template>
  <UPopover v-model:open="isOpen">
    <UButton
      color="neutral"
      variant="outline"
      class="w-full justify-between"
      :style="
        color
          ? { backgroundColor: color, borderColor: color, color: textColor }
          : undefined
      "
      type="button"
      :disabled="readOnly"
    >
      <UIcon
        :name="modelValue || 'mdi:key'"
        class="size-5"
      />
      <UIcon
        name="i-lucide-chevron-down"
        class="size-3 opacity-50"
      />
    </UButton>

    <template #content>
      <div class="p-3 w-96 max-h-112 overflow-y-auto space-y-3">
        <div class="grid grid-cols-6 gap-1.5">
          <button
            v-for="icon in standardIcons"
            :key="icon"
            type="button"
            :class="[
              'p-3 rounded-md transition-colors flex items-center justify-center',
              modelValue === icon
                ? 'bg-primary/15 ring-2 ring-primary'
                : 'hover:bg-elevated',
            ]"
            @click="select(icon)"
          >
            <UIcon
              :name="icon"
              class="size-7"
            />
          </button>
        </div>

        <div class="flex gap-2 pt-2 border-t border-default">
          <UiButton
            v-if="modelValue"
            :label="t('clear')"
            color="neutral"
            variant="outline"
            class="flex-1"
            type="button"
            @click="clear"
          />
          <UiButton
            :label="t('close')"
            color="primary"
            class="flex-1"
            type="button"
            @click="isOpen = false"
          />
        </div>
      </div>
    </template>
  </UPopover>
</template>

<script setup lang="ts">
const modelValue = defineModel<string>({ default: '' })

const props = defineProps<{
  color?: string
  readOnly?: boolean
}>()

const { t } = useI18n()
const isOpen = ref(false)

// Picks a legible text color (black/white) against a given hex background.
const textColor = computed(() => {
  if (!props.color || !props.color.startsWith('#')) return undefined
  const hex = props.color.slice(1)
  if (hex.length !== 6) return undefined
  const r = parseInt(hex.slice(0, 2), 16)
  const g = parseInt(hex.slice(2, 4), 16)
  const b = parseInt(hex.slice(4, 6), 16)
  const brightness = (r * 299 + g * 587 + b * 114) / 1000
  return brightness > 128 ? '#000' : '#fff'
})

const select = (icon: string) => {
  modelValue.value = icon
  isOpen.value = false
}

const clear = () => {
  modelValue.value = ''
}

const standardIcons = [
  'mdi:key',
  'mdi:key-variant',
  'mdi:shield',
  'mdi:shield-check',
  'mdi:lock',
  'mdi:lock-open',
  'mdi:security',
  'mdi:fingerprint',

  'mdi:folder',
  'mdi:folder-lock',
  'mdi:folder-key',
  'mdi:file-document',

  'mdi:bank',
  'mdi:credit-card',
  'mdi:bitcoin',
  'mdi:cash',
  'mdi:piggy-bank',
  'mdi:currency-usd',

  'mdi:email',
  'mdi:email-outline',
  'mdi:web',
  'mdi:shopping',
  'mdi:cart',
  'mdi:store',

  'mdi:message',
  'mdi:chat',
  'mdi:phone',
  'mdi:account',
  'mdi:account-circle',
  'mdi:account-group',

  'mdi:laptop',
  'mdi:cellphone',
  'mdi:desktop-tower',
  'mdi:server',
  'mdi:database',
  'mdi:cloud',
  'mdi:wifi',
  'mdi:harddisk',

  'mdi:controller',
  'mdi:headphones',
  'mdi:music',
  'mdi:video',
  'mdi:camera',
  'mdi:television',

  'mdi:briefcase',
  'mdi:note',
  'mdi:notebook',
  'mdi:calendar',
  'mdi:clock',
  'mdi:bookmark',

  'mdi:github',
  'mdi:gitlab',
  'mdi:wrench',
  'mdi:code-tags',
  'mdi:console',
  'mdi:application',

  'mdi:airplane',
  'mdi:car',
  'mdi:bus',
  'mdi:train',
  'mdi:bike',
  'mdi:ticket',
  'mdi:map-marker',

  'mdi:home',
  'mdi:star',
  'mdi:heart',
  'mdi:tag',
  'mdi:lightbulb',
  'mdi:gift',
] as const
</script>

<i18n lang="yaml">
de:
  clear: Zurücksetzen
  close: Schließen
en:
  clear: Clear
  close: Close
</i18n>
