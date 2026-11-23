<script setup lang="ts">
/**
 * The protection state of a conversation in its header (spec 095 US4, FR-013): the state in plain
 * words with an icon. A press opens `ProtectionExplanation.vue` below the header (not floating, so
 * it stays in place when the keyboard opens on a phone).
 */
import type { ChatPeerProtection } from '@bindings/ChatPeerProtection'
import { PROTECTION_VIEWS } from '~/lib/chat/protection'

const props = defineProps<{ protection: ChatPeerProtection }>()
const open = defineModel<boolean>('open', { default: false })

const { t } = useI18n()

const view = computed(() => PROTECTION_VIEWS[props.protection.state])
</script>

<template>
  <UiButton
    type="button"
    variant="ghost"
    size="sm"
    class="gap-1 px-2"
    :class="view.warning ? 'text-warning' : 'text-muted-foreground'"
    :aria-expanded="open"
    :aria-label="
      open ? t('chat.peer.protection.hide') : t('chat.peer.protection.show')
    "
    :tooltip="
      open ? t('chat.peer.protection.hide') : t('chat.peer.protection.show')
    "
    data-testid="chat-peer-protection"
    :data-state="protection.state"
    @click="open = !open"
  >
    <Icon :name="view.icon" class="h-4 w-4 shrink-0" />
    <span class="hidden text-xs sm:inline">{{ t(view.label) }}</span>
  </UiButton>
</template>
