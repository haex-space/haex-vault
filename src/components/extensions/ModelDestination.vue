<script setup lang="ts">
/**
 * The consent in the question of a job (spec 055 US4, T048): the provider the job would go to,
 * with its host and place, and what the step would send, kinds and sizes but never the content.
 * Nothing leaves this device before the user allows it.
 */
import type { ExtensionModelDestination } from '@bindings/ExtensionModelDestination'
import type { ExtensionModelTransfer } from '@bindings/ExtensionModelTransfer'
import { transferLines } from '~/lib/extensions/modelConsent'
import { placeLabelKey } from '~/lib/extensions/modelGrants'

const props = defineProps<{
  destination: ExtensionModelDestination
  transfer?: ExtensionModelTransfer
}>()
const { t } = useI18n()

const lines = computed(() =>
  props.transfer ? transferLines(props.transfer) : [],
)
</script>

<template>
  <div class="space-y-3" data-testid="extension-model-destination">
    <p class="text-sm">
      {{ t('extensions.permissionRequest.modelDestination.intro') }}
    </p>
    <div class="rounded-xl bg-muted px-3 py-2">
      <p class="text-sm font-medium">{{ destination.provider }}</p>
      <p class="text-xs break-all text-muted-foreground">
        <template v-if="destination.host">{{ destination.host }} · </template>
        {{ t(placeLabelKey(destination.place)) }}
      </p>
    </div>
    <div v-if="lines.length > 0" class="space-y-1">
      <p class="text-sm">
        {{ t('extensions.permissionRequest.modelDestination.data') }}
      </p>
      <ul class="list-disc ps-5 text-sm">
        <li v-for="line in lines" :key="line.key">
          {{
            line.plural === undefined
              ? t(line.key, line.params)
              : t(line.key, line.params, line.plural)
          }}
        </li>
      </ul>
    </div>
    <p class="text-xs text-muted-foreground">
      {{ t('extensions.permissionRequest.modelDestination.hint') }}
    </p>
  </div>
</template>
