<script setup lang="ts">
/**
 * The place `/entry/:id` (spec 034, research R13): the entry's view, or its editor for
 * `/entry/:id?edit` and for `/entry/new`. The path carries the id only, never a title or a value.
 * The editor is the one of the entry's type (spec 092): a pass for `/entry/new?type=pass` and for
 * an existing pass, the login editor otherwise.
 */
import { entryTypeOf } from '~/lib/passwords/passType'

const router = useTabRouter()
const store = usePasswordsStore()

const itemId = computed(() => router.route.params.id ?? '')
const isNew = computed(() => itemId.value === 'new')
const type = computed(() => entryTypeOf(store.headersById.get(itemId.value)))
/** An entry of a type a newer holzi wrote is shown as such, never opened in an editor (spec 092, FR-042). */
const unknownType = computed(() => type.value === 'unknown')
/** A new pass is named by the place (`?type=pass`); no other value opens another editor. */
const newPass = computed(
  () => isNew.value && router.route.query.type === 'pass',
)
// Until the overview is loaded the type of an existing entry is not known, so its editor waits
// for it; the view shows meanwhile.
const editing = computed(() => router.route.query.edit !== undefined)
</script>

<template>
  <PasswordsUnknownType v-if="!isNew && unknownType" />
  <PasswordsPassEditor
    v-else-if="
      newPass || (!isNew && editing && store.hasLoadedOnce && type === 'pass')
    "
    :key="newPass ? 'new-pass' : `edit-pass-${itemId}`"
    :item-id="newPass ? null : itemId"
  />
  <PasswordsEntryEditor
    v-else-if="isNew || (editing && store.hasLoadedOnce)"
    :key="isNew ? 'new' : `edit-${itemId}`"
    :item-id="isNew ? null : itemId"
  />
  <PasswordsEntryView v-else :key="itemId" :item-id="itemId" />
</template>
