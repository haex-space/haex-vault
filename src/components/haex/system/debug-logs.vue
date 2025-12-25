<template>
  <HaexSystem :is-dragging="isDragging">
    <template #header>
      <div class="flex flex-wrap items-center justify-between gap-2">
        <div class="flex items-center gap-2">
          <UIcon
            name="i-heroicons-bug-ant"
            class="w-5 h-5 shrink-0"
          />
          <h2 class="text-2xl font-bold">
            Debug Logs
          </h2>
          <span class="text-xs text-gray-500">
            {{ logs.length }} logs
          </span>
        </div>
        <div class="flex flex-wrap gap-2">
          <UButton
            label="Clear Logs"
            color="error"
            @click="clearLogs"
          />
          <UButton
            :label="allCopied ? 'Copied!' : 'Copy All'"
            :color="allCopied ? 'success' : 'primary'"
            @click="copyAllLogs"
          />
        </div>
      </div>
    </template>

    <div class="w-full h-full flex flex-col">

    <!-- Filter Buttons -->
    <div class="flex gap-2 p-4 border-b border-gray-200 dark:border-gray-700 overflow-x-auto">
      <UButton
        v-for="level in ['all', 'log', 'info', 'warn', 'error', 'debug']"
        :key="level"
        :label="level"
        :color="filter === level ? 'primary' : 'neutral'"
        @click="filter = level as any"
      />
    </div>

    <!-- Logs Container -->
    <div
      ref="logsContainer"
      class="flex-1 overflow-y-auto p-4 space-y-2 font-mono text-xs"
    >
      <!-- Loading State -->
      <div
        v-if="isLoading"
        class="flex flex-col items-center justify-center py-16 gap-3"
      >
        <UIcon
          name="i-heroicons-arrow-path"
          class="w-8 h-8 animate-spin text-gray-400"
        />
        <p class="text-gray-500">Loading logs...</p>
      </div>

      <template v-else>
        <div
          v-for="(log, index) in displayedLogs"
          :key="index"
          :class="[
            'p-3 rounded-lg border-l-4 relative group',
            log.level === 'error'
              ? 'bg-red-50 dark:bg-red-950/30 border-red-500'
              : log.level === 'warn'
                ? 'bg-yellow-50 dark:bg-yellow-950/30 border-yellow-500'
                : log.level === 'info'
                  ? 'bg-blue-50 dark:bg-blue-950/30 border-blue-500'
                  : log.level === 'debug'
                    ? 'bg-purple-50 dark:bg-purple-950/30 border-purple-500'
                    : 'bg-gray-50 dark:bg-gray-800 border-gray-400',
          ]"
        >
          <!-- Copy Button -->
          <button
            class="absolute top-2 right-2 p-1.5 rounded bg-white dark:bg-gray-700 shadow-sm hover:bg-gray-100 dark:hover:bg-gray-600 active:scale-95 transition-all"
            @click="copyLogToClipboard(log)"
          >
            <UIcon
              :name="copiedIndex === index ? 'i-heroicons-check' : 'i-heroicons-clipboard-document'"
              :class="[
                'w-4 h-4',
                copiedIndex === index ? 'text-green-500' : ''
              ]"
            />
          </button>

          <div class="flex items-start gap-2 mb-1">
            <span class="text-gray-500 dark:text-gray-400 text-[10px] shrink-0">
              {{ log.timestamp }}
            </span>
            <span
              :class="[
                'font-semibold text-[10px] uppercase shrink-0',
                log.level === 'error'
                  ? 'text-red-600 dark:text-red-400'
                  : log.level === 'warn'
                    ? 'text-yellow-600 dark:text-yellow-400'
                    : log.level === 'info'
                      ? 'text-blue-600 dark:text-blue-400'
                      : log.level === 'debug'
                        ? 'text-purple-600 dark:text-purple-400'
                        : 'text-gray-600 dark:text-gray-400',
              ]"
            >
              {{ log.level }}
            </span>
          </div>
          <pre class="whitespace-pre-wrap break-words text-gray-900 dark:text-gray-100 pr-8">{{ log.message }}</pre>
        </div>

        <div
          v-if="displayedLogs.length === 0"
          class="text-center text-gray-500 py-8"
        >
          <UIcon
            name="i-heroicons-document-text"
            class="w-12 h-12 mx-auto mb-2 opacity-50"
          />
          <p>No logs to display</p>
        </div>
      </template>
    </div>
    </div>
  </HaexSystem>
</template>

<script setup lang="ts">
import { globalConsoleLogs } from '~/plugins/console-interceptor'
import type { ConsoleLog } from '~/plugins/console-interceptor'

defineProps<{
  isDragging?: boolean
}>()

const filter = ref<'all' | 'log' | 'info' | 'warn' | 'error' | 'debug'>('all')
const logsContainer = ref<HTMLDivElement>()
const copiedIndex = ref<number | null>(null)
const allCopied = ref(false)
const isLoading = ref(true)
const displayedLogs = ref<ConsoleLog[]>([])

const { $clearConsoleLogs } = useNuxtApp()
const { copy } = useClipboard()

const logs = computed(() => globalConsoleLogs.value)

const filteredLogs = computed(() => {
  if (filter.value === 'all') {
    return logs.value
  }
  return logs.value.filter((log) => log.level === filter.value)
})

// Load logs asynchronously to show loading spinner
const loadLogsAsync = async () => {
  isLoading.value = true
  // Use setTimeout to allow the UI to render the loading spinner first
  await new Promise((resolve) => setTimeout(resolve, 50))
  displayedLogs.value = filteredLogs.value
  isLoading.value = false
}

// Initial load
onMounted(() => {
  loadLogsAsync()
})

// Reload when filter changes
watch(filter, () => {
  loadLogsAsync()
})

const clearLogs = () => {
  if ($clearConsoleLogs) {
    $clearConsoleLogs()
  }
}

const copyLogToClipboard = async (log: ConsoleLog) => {
  const text = `[${log.timestamp}] [${log.level.toUpperCase()}] ${log.message}`
  await copy(text)

  // Find the index in filteredLogs for visual feedback
  const index = filteredLogs.value.indexOf(log)
  copiedIndex.value = index

  // Reset after 2 seconds
  setTimeout(() => {
    copiedIndex.value = null
  }, 2000)
}

const copyAllLogs = async () => {
  const allLogsText = filteredLogs.value
    .map((log) => `[${log.timestamp}] [${log.level.toUpperCase()}] ${log.message}`)
    .join('\n')

  await copy(allLogsText)
  allCopied.value = true

  // Reset after 2 seconds
  setTimeout(() => {
    allCopied.value = false
  }, 2000)
}

// Update displayed logs when new logs arrive (without loading spinner)
watch(
  () => logs.value.length,
  () => {
    // Only update if not currently loading (initial load)
    if (!isLoading.value) {
      displayedLogs.value = filteredLogs.value
    }
    // Auto-scroll to bottom
    nextTick(() => {
      if (logsContainer.value) {
        logsContainer.value.scrollTop = logsContainer.value.scrollHeight
      }
    })
  },
  { immediate: true }
)
</script>
