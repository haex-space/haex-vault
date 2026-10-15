<script setup lang="ts">
/**
 * What can be done with the contacts selected in the list (spec 051 FR-018c, FR-018d): send them
 * to a phone of the vault with the sync on, or take them off a phone (which asks whether the
 * phone deletes them or keeps them unsynced).
 */
import { toast } from 'vue-sonner'
import type { ContactsPhoneSummary } from '@bindings/ContactsPhoneSummary'

const props = defineProps<{
  ids: string[]
}>()

const emit = defineEmits<{
  done: []
}>()

const { t } = useI18n()
const { errString } = useErrorString()
const store = useContactsStore()
const { targetsSetAsync } = useContactTargets()

const removing = ref<ContactsPhoneSummary | null>(null)

function nameOf(phone: ContactsPhoneSummary): string {
  return phone.name ?? t('contacts.targets.unnamedPhone')
}

async function sendAsync(phone: ContactsPhoneSummary) {
  try {
    // action-exempt: the contact actions join the catalog with spec 051 US6 (T083); until then the
    // choice of the window calls the command.
    await targetsSetAsync(props.ids, phone.device)
    await store.quietReloadAsync()
    toast.success(
      t(
        'contacts.targets.sent',
        { phone: nameOf(phone), count: props.ids.length },
        props.ids.length,
      ),
    )
    emit('done')
  } catch (cause) {
    toast.error(errString(cause))
  }
}
</script>

<template>
  <div
    class="flex flex-wrap items-center gap-2 rounded-xl bg-muted px-3 py-2"
    data-testid="contacts-selection"
  >
    <span class="min-w-0 flex-1 text-sm" data-testid="contacts-selection-count">
      {{ t('contacts.selection.count', { count: ids.length }, ids.length) }}
    </span>
    <ShadcnDropdownMenu>
      <ShadcnDropdownMenuTrigger as-child>
        <UiButton
          variant="outline"
          size="sm"
          :disabled="ids.length === 0"
          data-testid="contacts-selection-send"
        >
          <Icon name="lucide:smartphone" class="size-4" />
          {{ t('contacts.targets.send') }}
        </UiButton>
      </ShadcnDropdownMenuTrigger>
      <ShadcnDropdownMenuContent align="end">
        <ShadcnDropdownMenuItem
          v-for="phone in store.phones"
          :key="phone.device"
          class="gap-2"
          :data-testid="`contacts-selection-send-${phone.device}`"
          @select="sendAsync(phone)"
        >
          {{ nameOf(phone) }}
        </ShadcnDropdownMenuItem>
      </ShadcnDropdownMenuContent>
    </ShadcnDropdownMenu>
    <ShadcnDropdownMenu>
      <ShadcnDropdownMenuTrigger as-child>
        <UiButton
          variant="outline"
          size="sm"
          :disabled="ids.length === 0"
          data-testid="contacts-selection-take-off"
        >
          <Icon name="lucide:smartphone-off" class="size-4" />
          {{ t('contacts.targets.takeOff') }}
        </UiButton>
      </ShadcnDropdownMenuTrigger>
      <ShadcnDropdownMenuContent align="end">
        <ShadcnDropdownMenuItem
          v-for="phone in store.phones"
          :key="phone.device"
          class="gap-2"
          :data-testid="`contacts-selection-take-off-${phone.device}`"
          @select="removing = phone"
        >
          {{ nameOf(phone) }}
        </ShadcnDropdownMenuItem>
      </ShadcnDropdownMenuContent>
    </ShadcnDropdownMenu>
    <UiButton
      variant="ghost"
      size="sm"
      data-testid="contacts-selection-done"
      @click="emit('done')"
    >
      {{ t('contacts.selection.done') }}
    </UiButton>
    <ContactsTargetRemoveDialog
      v-if="removing"
      :open="removing !== null"
      :ids="ids"
      :device="removing.device"
      :phone="nameOf(removing)"
      @update:open="(value: boolean) => (removing = value ? removing : null)"
      @done="emit('done')"
    />
  </div>
</template>
