<script setup lang="ts">
/**
 * "Mit Kontakt schreiben" (spec 072 US1, FR-004): picks a contact to write to. Only contacts with a
 * confirmed Visitenkarte of another vault are offered, never the own contact. Groups (several
 * contacts plus a name) join with spec 072 US4.
 */
import type { ContactHeader } from '@bindings/ContactHeader'

const props = defineProps<{
  open: boolean
  contacts: ContactHeader[]
}>()

const emit = defineEmits<{
  'update:open': [open: boolean]
  select: [contactId: string]
}>()

const { t } = useI18n()
const search = ref('')

const writable = computed(() =>
  props.contacts
    .filter((contact) => contact.hasCard && !contact.isSelf)
    .sort((a, b) => a.name.localeCompare(b.name)),
)
const shown = computed(() => {
  const needle = search.value.trim().toLocaleLowerCase()
  if (!needle) return writable.value
  return writable.value.filter((contact) =>
    contact.name.toLocaleLowerCase().includes(needle),
  )
})

watch(
  () => props.open,
  (open) => {
    if (open) search.value = ''
  },
)
</script>

<template>
  <UiDrawerModal
    :open="open"
    :title="t('chat.newContact.title')"
    @update:open="emit('update:open', $event)"
  >
    <template #content>
      <div class="space-y-3 px-6 py-2" data-testid="chat-new-contact">
        <p v-if="writable.length === 0" class="text-sm text-muted-foreground">
          {{ t('chat.newContact.empty') }}
        </p>
        <template v-else>
          <UiInput
            v-model="search"
            :placeholder="t('chat.newContact.search')"
            :aria-label="t('chat.newContact.search')"
          />
          <ul class="max-h-80 space-y-1 overflow-y-auto">
            <li v-for="contact in shown" :key="contact.id">
              <button
                type="button"
                class="flex w-full items-center gap-3 rounded-lg px-2 py-2 text-left text-sm hover:bg-accent focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                data-testid="chat-new-contact-entry"
                @click="emit('select', contact.id)"
              >
                <ContactsAvatar :name="contact.name" :photo="contact.photo" />
                <span class="truncate">{{ contact.name }}</span>
              </button>
            </li>
          </ul>
          <p v-if="shown.length === 0" class="text-sm text-muted-foreground">
            {{ t('chat.newContact.noMatch') }}
          </p>
        </template>
      </div>
    </template>
  </UiDrawerModal>
</template>
