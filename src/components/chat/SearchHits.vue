<script setup lang="ts">
/**
 * The "Nachrichten" section of the chat sidebar's search (spec 076 FR-011, FR-012): one entry per
 * message hit with the conversation's picture and name, the extract with the searched text
 * emphasised, and the time; a hint when there are more hits than listed.
 */
import type { ChatSearchHit } from '@bindings/ChatSearchHit'
import { listTime } from '~/lib/chat/listTime'

const props = defineProps<{
  hits: ChatSearchHit[]
  more: boolean
  /** The searched text, to emphasise it in the extracts. */
  text: string
  /** Name and picture of a hit's conversation. */
  resolve: (hit: ChatSearchHit) => {
    name: string
    photo: string | null
    isAgent: boolean
  }
}>()

const emit = defineEmits<{ select: [hit: ChatSearchHit] }>()

const { t, d } = useI18n()

/** The extract split around the first match, compared per character as the core does. */
function parts(snippet: string): [string, string, string] {
  const needle = [...props.text.trim().split(/\s+/).join(' ')].flatMap((c) => [
    ...c.toLowerCase(),
  ])
  const chars = [...snippet]
  const folded = chars.flatMap((c, at) =>
    [...c.toLowerCase()].map((l) => ({ l, at })),
  )
  for (let i = 0; i + needle.length <= folded.length; i++) {
    if (needle.every((c, k) => folded[i + k]?.l === c)) {
      const start = folded[i]!.at
      const end = folded[i + needle.length - 1]!.at + 1
      return [
        chars.slice(0, start).join(''),
        chars.slice(start, end).join(''),
        chars.slice(end).join(''),
      ]
    }
  }
  return [snippet, '', '']
}
</script>

<template>
  <section class="space-y-1" :aria-label="t('chat.search.messages')">
    <h3 class="px-2 pt-2 text-xs font-medium text-muted-foreground">
      {{ t('chat.search.messages') }}
    </h3>
    <button
      v-for="hit in hits"
      :key="hit.messageId"
      type="button"
      data-testid="chat-search-hit"
      class="flex w-full min-w-0 items-center gap-2 rounded-lg px-2 py-1.5 text-left text-sm transition-colors hover:bg-accent focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
      @click="emit('select', hit)"
    >
      <span
        v-if="resolve(hit).isAgent"
        class="flex size-9 shrink-0 items-center justify-center rounded-full bg-foreground text-background"
        aria-hidden="true"
      >
        <Icon name="lucide:bot" class="h-4 w-4" />
      </span>
      <ContactsAvatar
        v-else
        :name="resolve(hit).name"
        :photo="resolve(hit).photo"
      />
      <span class="min-w-0 flex-1">
        <span class="flex items-baseline gap-2">
          <span class="min-w-0 flex-1 truncate">{{ resolve(hit).name }}</span>
          <span class="shrink-0 text-xs tabular-nums text-muted-foreground">
            {{ listTime(hit.at, d) }}
          </span>
        </span>
        <span class="block truncate text-xs text-muted-foreground">
          {{ parts(hit.snippet)[0]
          }}<mark class="bg-transparent font-semibold text-foreground">{{
            parts(hit.snippet)[1]
          }}</mark
          >{{ parts(hit.snippet)[2] }}
        </span>
      </span>
    </button>
    <p
      v-if="more"
      class="px-2 py-1 text-xs text-muted-foreground"
      data-testid="chat-search-more"
    >
      {{ t('chat.search.more') }}
    </p>
  </section>
</template>
