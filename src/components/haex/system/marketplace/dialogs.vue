<template>
  <div class="contents">
    <HaexExtensionDialogReinstall
      v-model:open="openOverwriteDialogModel"
      :preview="installPreview ?? null"
      :mode="reinstallMode"
      :icon-url="currentMarketplaceExtension?.iconUrl"
      @update:preview="(value: ExtensionPreview | null) => emit('update:installPreview', value ?? undefined)"
      @confirm="emit('confirm-reinstall')"
    />

    <HaexExtensionDialogInstall
      v-model:open="showConfirmationModel"
      :preview="installPreview"
      :available-versions="currentMarketplaceExtension?.versions"
      :installed-version="currentMarketplaceExtension?.installedVersion"
      :icon-url="currentMarketplaceExtension?.iconUrl"
      @confirm="(...args: ConfirmInstallArgs) => emit('confirm-install', ...args)"
    />

    <HaexExtensionDialogRemove
      v-model:open="showRemoveDialogModel"
      :extension="extensionToBeRemoved"
      @confirm="(deleteMode: 'device' | 'complete') => emit('confirm-remove', deleteMode)"
    />

    <HaexExtensionDialogDetails
      v-model:open="showDetailsDialogModel"
      :extension="selectedExtensionForDetails"
      @install="(ext: MarketplaceExtensionViewModel) => emit('details-install', ext)"
      @update="(ext: MarketplaceExtensionViewModel) => emit('details-update', ext)"
      @remove="(ext: MarketplaceExtensionViewModel) => emit('details-remove', ext)"
    />
  </div>
</template>

<script setup lang="ts">
import type {
  IHaexSpaceExtension,
  MarketplaceExtensionViewModel,
} from '~/types/haexspace'
import type { ExtensionPreview } from '~~/src-tauri/bindings/ExtensionPreview'

type ConfirmInstallArgs = [
  createDesktopShortcut?: boolean,
  selectedVersion?: string | null,
]

const props = defineProps<{
  openOverwriteDialog: boolean
  showConfirmation: boolean
  showRemoveDialog: boolean
  showDetailsDialog: boolean
  installPreview: ExtensionPreview | undefined
  reinstallMode: 'update' | 'reinstall'
  currentMarketplaceExtension: MarketplaceExtensionViewModel | null
  extensionToBeRemoved: IHaexSpaceExtension | undefined
  selectedExtensionForDetails: MarketplaceExtensionViewModel | null
}>()

const emit = defineEmits<{
  'update:openOverwriteDialog': [value: boolean]
  'update:showConfirmation': [value: boolean]
  'update:showRemoveDialog': [value: boolean]
  'update:showDetailsDialog': [value: boolean]
  'update:installPreview': [value: ExtensionPreview | undefined]
  'confirm-reinstall': []
  'confirm-install': [
    createDesktopShortcut?: boolean,
    selectedVersion?: string | null,
  ]
  'confirm-remove': [deleteMode: 'device' | 'complete']
  'details-install': [ext: MarketplaceExtensionViewModel]
  'details-update': [ext: MarketplaceExtensionViewModel]
  'details-remove': [ext: MarketplaceExtensionViewModel]
}>()

const openOverwriteDialogModel = computed({
  get: () => props.openOverwriteDialog,
  set: (value: boolean) => emit('update:openOverwriteDialog', value),
})

const showConfirmationModel = computed({
  get: () => props.showConfirmation,
  set: (value: boolean) => emit('update:showConfirmation', value),
})

const showRemoveDialogModel = computed({
  get: () => props.showRemoveDialog,
  set: (value: boolean) => emit('update:showRemoveDialog', value),
})

const showDetailsDialogModel = computed({
  get: () => props.showDetailsDialog,
  set: (value: boolean) => emit('update:showDetailsDialog', value),
})

</script>
