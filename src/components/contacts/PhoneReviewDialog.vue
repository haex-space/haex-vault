<script setup lang="ts">
/**
 * The overview before the first run of the sync with the phone (spec 051 FR-019): how many contacts
 * come into the address book, go to the phone and are merged, and the uncertain pairs, each with
 * "zusammenführen" or "getrennt lassen" (the default). Nothing is written before the confirmation.
 */
import type { ContactsPhonePairSide } from '@bindings/ContactsPhonePairSide'
import type { ContactsPhoneReview } from '@bindings/ContactsPhoneReview'
import { toast } from 'vue-sonner'

const open = defineModel<boolean>('open', { required: true })
const emit = defineEmits<{ done: [] }>()

const { t } = useI18n()
const { errString } = useErrorString()
const store = useContactsStore()
const phone = useContactsPhone()

const review = shallowRef<ContactsPhoneReview | null>(null)
const merge = ref<Record<string, boolean>>({})
const loading = ref(false)
const busy = ref(false)
const error = ref<string | null>(null)

watch(
  open,
  async (now) => {
    if (!now) return
    loading.value = true
    error.value = null
    merge.value = {}
    try {
      review.value = await phone.reviewAsync()
    } catch (cause) {
      error.value = errString(cause)
    } finally {
      loading.value = false
    }
  },
  { immediate: true },
)

function describe(side: ContactsPhonePairSide): string {
  return [side.name, ...side.phones, ...side.emails]
    .filter((part) => part.length > 0)
    .join(' · ')
}

async function confirmAsync() {
  busy.value = true
  try {
    const decisions = Object.entries(merge.value).map(([pairId, merged]) => ({
      pairId,
      merge: merged,
    }))
    // action-exempt: the first run is the person's own decision; no agent action covers it.
    const summary = await phone.confirmAsync(decisions)
    await store.quietReloadAsync()
    toast.success(
      t('contacts.review.done', {
        imported: summary.counts.imported,
        merged: summary.counts.merged,
        written: summary.counts.written,
      }),
    )
    open.value = false
    emit('done')
  } catch (cause) {
    error.value = errString(cause)
  } finally {
    busy.value = false
  }
}
</script>

<template>
  <ShadcnAlertDialog
    :open="open"
    @update:open="(value: boolean) => (open = value)"
  >
    <ShadcnAlertDialogContent
      class="max-h-[85vh] overflow-y-auto"
      data-testid="contacts-review"
    >
      <ShadcnAlertDialogHeader>
        <ShadcnAlertDialogTitle>{{
          t('contacts.review.title')
        }}</ShadcnAlertDialogTitle>
        <ShadcnAlertDialogDescription>
          {{ t('contacts.review.description') }}
        </ShadcnAlertDialogDescription>
      </ShadcnAlertDialogHeader>

      <p v-if="loading" class="text-sm text-muted-foreground" role="status">
        {{ t('contacts.review.loading') }}
      </p>
      <ul
        v-else-if="review"
        class="flex flex-col gap-1 text-sm"
        data-testid="contacts-review-counts"
      >
        <li data-testid="contacts-review-to-address-book">
          {{
            t(
              'contacts.review.toAddressBook',
              { count: review.toAddressBook },
              review.toAddressBook,
            )
          }}
        </li>
        <li data-testid="contacts-review-to-phone">
          {{
            t(
              'contacts.review.toPhone',
              { count: review.toPhone },
              review.toPhone,
            )
          }}
        </li>
        <li data-testid="contacts-review-merged">
          {{
            t('contacts.review.merged', { count: review.merged }, review.merged)
          }}
        </li>
      </ul>

      <section
        v-if="review && review.uncertain.length > 0"
        class="flex flex-col gap-3"
        data-testid="contacts-review-pairs"
      >
        <h3 class="text-sm font-semibold">
          {{ t('contacts.review.uncertain') }}
        </h3>
        <div
          v-for="pair in review.uncertain"
          :key="pair.id"
          class="flex flex-col gap-2 rounded-lg bg-muted p-3 text-sm"
          data-testid="contacts-review-pair"
        >
          <p>
            <span class="text-muted-foreground">{{
              t('contacts.review.onPhone')
            }}</span>
            {{ describe(pair.phone) }}
          </p>
          <p>
            <span class="text-muted-foreground">{{
              t('contacts.review.inHolzi')
            }}</span>
            {{ describe(pair.contact) }}
          </p>
          <ShadcnRadioGroup
            :model-value="merge[pair.id] ? 'merge' : 'apart'"
            class="flex gap-4"
            @update:model-value="merge[pair.id] = $event === 'merge'"
          >
            <label class="flex items-center gap-2">
              <ShadcnRadioGroupItem
                value="apart"
                data-testid="contacts-review-apart"
              />
              {{ t('contacts.review.apart') }}
            </label>
            <label class="flex items-center gap-2">
              <ShadcnRadioGroupItem
                value="merge"
                data-testid="contacts-review-merge"
              />
              {{ t('contacts.review.merge') }}
            </label>
          </ShadcnRadioGroup>
        </div>
      </section>

      <p v-if="error" class="text-sm text-destructive" role="alert">
        {{ error }}
      </p>

      <ShadcnAlertDialogFooter>
        <ShadcnAlertDialogCancel>{{
          t('contacts.review.later')
        }}</ShadcnAlertDialogCancel>
        <UiButton
          :loading="busy"
          :disabled="loading || !review"
          data-testid="contacts-review-confirm"
          @click="confirmAsync"
        >
          {{ t('contacts.review.confirm') }}
        </UiButton>
      </ShadcnAlertDialogFooter>
    </ShadcnAlertDialogContent>
  </ShadcnAlertDialog>
</template>
