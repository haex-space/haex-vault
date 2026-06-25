<template>
  <div class="border border-default rounded-lg overflow-hidden">
    <div class="px-4 py-3 border-b border-default bg-elevated/30">
      <div class="flex items-center gap-2">
        <UIcon
          name="i-lucide-sliders-horizontal"
          class="size-4 text-primary"
        />
        <p class="text-sm font-medium">
          {{ t('extra.customFields') }}
        </p>
      </div>
      <p class="text-xs text-muted mt-0.5">
        {{ t('extra.description') }}
      </p>
    </div>
    <div class="p-4">
      <!-- Master-detail: key list left, value textarea right -->
      <div
        v-if="visibleKeyValues.length > 0"
        class="flex flex-col @2xl:flex-row gap-4"
      >
        <!-- Key list -->
        <div class="flex-1 flex flex-col gap-3">
          <div class="border border-default rounded-lg divide-y divide-default">
            <div
              v-for="(kv, index) in visibleKeyValues"
              :key="kv.id"
              :class="[
                'flex items-center gap-1 px-2 transition-colors cursor-pointer',
                currentSelectedKv === kv ? 'bg-elevated' : 'hover:bg-elevated/50',
                index === 0 ? 'rounded-t-lg' : '',
                index === visibleKeyValues.length - 1 ? 'rounded-b-lg' : '',
              ]"
              @click="emit('selectKv', kv)"
            >
              <UInput
                :ref="(el) => { if (index === visibleKeyValues.length - 1) lastKvKeyInputEl = el as { $el?: HTMLElement } }"
                v-model="kv.key"
                :readonly="!isEditing"
                :placeholder="t('extra.keyPlaceholder')"
                variant="none"
                class="flex-1 text-sm"
                @click.stop="emit('selectKv', kv)"
              >
                <template #trailing>
                  <UiButton
                    :icon="kvCopiedItem === kv ? 'i-lucide-check' : 'i-lucide-copy'"
                    :color="kvCopiedItem === kv ? 'success' : 'neutral'"
                    variant="ghost"
                    type="button"
                    @click.stop="emit('copyKv', kv)"
                  />
                  <UiButton
                    v-if="isEditing"
                    icon="i-lucide-trash-2"
                    color="error"
                    variant="ghost"
                    type="button"
                    @click.stop="emit('removeKv', index)"
                  />
                </template>
              </UInput>
            </div>
          </div>

          <UiButton
            v-if="isEditing"
            :label="t('extra.add')"
            icon="i-lucide-plus"
            color="neutral"
            variant="outline"
            type="button"
            @click="emit('addKv', lastKvKeyInputEl)"
          />
        </div>

        <!-- Value textarea -->
        <div class="flex-1 @2xl:min-w-52">
          <UiTextarea
            :model-value="currentKvValue"
            :read-only="!isEditing || !currentSelectedKv"
            :placeholder="t('extra.valuePlaceholder')"
            :with-copy-button="!!currentSelectedKv"
            :rows="8"
            @update:model-value="(v: string | undefined) => emit('update:currentKvValue', v ?? '')"
          />
        </div>
      </div>

      <!-- Empty state -->
      <div
        v-else
        class="flex flex-col items-center justify-center gap-3 py-6 text-muted"
      >
        <UIcon
          name="i-lucide-list-plus"
          class="size-8 opacity-40"
        />
        <p class="text-sm">
          {{ t('extra.empty') }}
        </p>
        <UiButton
          v-if="isEditing"
          :label="t('extra.add')"
          icon="i-lucide-plus"
          color="neutral"
          variant="outline"
          type="button"
          @click="emit('addKv', lastKvKeyInputEl)"
        />
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import type { EditableKeyValue } from '~/composables/passwords/usePasswordEditorForm'

const { t } = useI18n()

defineProps<{
  visibleKeyValues: EditableKeyValue[]
  currentSelectedKv: EditableKeyValue | null
  currentKvValue: string
  kvCopiedItem: EditableKeyValue | null
  isEditing: boolean
}>()

const emit = defineEmits<{
  selectKv: [kv: EditableKeyValue]
  copyKv: [kv: EditableKeyValue]
  removeKv: [index: number]
  addKv: [focusEl: { $el?: HTMLElement } | null]
  'update:currentKvValue': [value: string]
}>()

const lastKvKeyInputEl = ref<{ $el?: HTMLElement } | null>(null)
</script>
