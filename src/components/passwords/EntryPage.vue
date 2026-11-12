<script setup lang="ts">
/**
 * The place `/entry/:id` (spec 034, research R13): the entry's view, or its editor for
 * `/entry/:id?edit` and for `/entry/new`. The path carries the id only, never a title or a value.
 */
import { entryTypeOf } from '~/lib/passwords/passType'

const router = useTabRouter()
const store = usePasswordsStore()

const itemId = computed(() => router.route.params.id ?? '')
const isNew = computed(() => itemId.value === 'new')
/** An entry of a type a newer holzi wrote is shown as such, never opened in an editor (spec 092, FR-042). */
const unknownType = computed(
  () => entryTypeOf(store.headersById.get(itemId.value)) === 'unknown',
)
const editing = computed(() => router.route.query.edit !== undefined)
</script>

<template>
  <PasswordsUnknownType v-if="!isNew && unknownType" />
  <PasswordsEntryEditor
    v-else-if="isNew || editing"
    :key="isNew ? 'new' : `edit-${itemId}`"
    :item-id="isNew ? null : itemId"
  />
  <PasswordsEntryView v-else :key="itemId" :item-id="itemId" />
</template>
