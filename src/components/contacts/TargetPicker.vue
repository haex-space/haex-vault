<script setup lang="ts">
/**
 * Where a new contact goes (spec 051 FR-018c): the phones of the vault with the sync on, each
 * with its target account suggested. Nothing chosen keeps the contact only in holzi, which is
 * the preselection. Without a phone with the sync on the question is not asked.
 */
import type { ContactsPhoneSummary } from '@bindings/ContactsPhoneSummary'
import type { TargetInput } from '@bindings/TargetInput'

const model = defineModel<TargetInput[]>({ required: true })

const { t } = useI18n()
const store = useContactsStore()
const nameAccount = useAccountLabel()

function writable(phone: ContactsPhoneSummary) {
  return phone.accounts.filter((account) => account.writable)
}

function chosen(device: string) {
  return model.value.find((target) => target.device === device)
}

function toggle(phone: ContactsPhoneSummary, on: boolean) {
  if (!on) {
    model.value = model.value.filter((target) => target.device !== phone.device)
    return
  }
  const account = phone.target ?? writable(phone)[0]
  model.value = [
    ...model.value,
    account ? { device: phone.device, account } : { device: phone.device },
  ]
}

function options(phone: ContactsPhoneSummary) {
  return writable(phone).map((account) => ({
    value: accountKey(account),
    label: nameAccount(account.accountType, account.name),
  }))
}

function choose(phone: ContactsPhoneSummary, key: string | number | null) {
  const account = writable(phone).find((each) => accountKey(each) === key)
  if (!account) return
  model.value = model.value.map((target) =>
    target.device === phone.device ? { device: phone.device, account } : target,
  )
}

function chosenLabel(device: string): string {
  const account = chosen(device)?.account
  return account
    ? nameAccount(account.accountType, account.name)
    : t('contacts.targets.targetAccount')
}
</script>

<template>
  <SettingsGroup
    v-if="store.phones.length"
    :label="t('contacts.targets.question')"
    data-testid="contacts-editor-targets"
  >
    <li
      v-for="phone in store.phones"
      :key="phone.device"
      class="flex flex-wrap items-center gap-3 px-4 py-3"
    >
      <label class="flex min-w-0 flex-1 items-center gap-3">
        <ShadcnCheckbox
          :model-value="Boolean(chosen(phone.device))"
          :data-testid="`contacts-editor-target-${phone.device}`"
          @update:model-value="toggle(phone, $event === true)"
        />
        <span class="truncate">{{
          phone.name ?? t('contacts.targets.unnamedPhone')
        }}</span>
      </label>
      <template v-if="chosen(phone.device)">
        <SettingsSelect
          v-if="options(phone).length > 1"
          :id="`contacts-editor-target-account-${phone.device}`"
          :model-value="
            chosen(phone.device)?.account
              ? accountKey(chosen(phone.device)!.account!)
              : ''
          "
          :options="options(phone)"
          :data-testid="`contacts-editor-target-account-${phone.device}`"
          @update:model-value="choose(phone, $event)"
        />
        <span v-else class="text-sm text-muted-foreground">{{
          chosenLabel(phone.device)
        }}</span>
      </template>
    </li>
    <li
      v-if="model.length === 0"
      class="px-4 py-3 text-sm text-muted-foreground"
      data-testid="contacts-editor-only-holzi"
    >
      {{ t('contacts.targets.onlyHolzi') }}
    </li>
  </SettingsGroup>
</template>
