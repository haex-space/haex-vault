<script setup lang="ts">
/**
 * Overview of a category with several areas (spec 023-settings-app, FR-003): one card per area
 * with icon, title, one-line description and chevron, like the COSMIC settings; a card opens its
 * sub-view as a new tab location. Rows that name a group share one boxed list under its heading
 *; the others get a card each.
 */
import {
  categoryOf,
  locationPath,
  overviewGroups,
} from '~/lib/settings/registry'

const { t } = useI18n()
const router = useTabRouter()

const groups = computed(() => {
  const category = categoryOf(router.route.path)
  return category ? overviewGroups(category) : []
})
</script>

<template>
  <div class="flex flex-col gap-3">
    <SettingsGroup
      v-for="group in groups"
      :key="group.group ?? group.rows[0]!.id"
      :label="group.group ? t(`settings.groups.${group.group}`) : undefined"
    >
      <SettingsRow
        v-for="row in group.rows"
        :key="row.id"
        :to="locationPath(row)"
        :icon="row.icon"
        :title="t(row.titleKey)"
        :description="row.descriptionKey ? t(row.descriptionKey) : undefined"
        :data-testid="`settings-row-${row.id}`"
      />
    </SettingsGroup>
  </div>
</template>
