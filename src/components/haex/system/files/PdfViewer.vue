<template>
  <div
    ref="containerRef"
    class="flex flex-col items-center gap-4 overflow-y-auto max-h-[85vh] w-full px-4"
  >
    <!-- Toolbar -->
    <div class="sticky top-0 z-10 flex items-center gap-3 bg-black/70 backdrop-blur rounded-full px-4 py-2">
      <UButton
        icon="i-lucide-minus"
        variant="ghost"
        color="neutral"
        class="text-white"
        :disabled="scale <= 0.5"
        @click="scale = Math.max(0.5, scale - 0.25)"
      />
      <span class="text-white text-sm min-w-12 text-center">{{ Math.round(scale * 100) }}%</span>
      <UButton
        icon="i-lucide-plus"
        variant="ghost"
        color="neutral"
        class="text-white"
        :disabled="scale >= 3"
        @click="scale = Math.min(3, scale + 0.25)"
      />
      <span class="text-white/60 text-sm">{{ currentPage }} / {{ totalPages }}</span>
    </div>

    <!-- Pages -->
    <div
      v-if="isLoading"
      class="flex items-center justify-center py-16"
    >
      <UIcon
        name="i-lucide-loader-2"
        class="w-8 h-8 animate-spin text-white"
      />
    </div>

    <div
      v-else-if="error"
      class="text-red-400 text-sm py-8"
    >
      {{ error }}
    </div>

    <canvas
      v-for="page in totalPages"
      :key="page"
      :ref="(el) => setCanvasRef(el as HTMLCanvasElement, page)"
      class="shadow-lg rounded"
    />
  </div>
</template>

<script setup lang="ts">
import * as pdfjsLib from 'pdfjs-dist'

pdfjsLib.GlobalWorkerOptions.workerSrc = new URL(
  'pdfjs-dist/build/pdf.worker.mjs',
  import.meta.url,
).toString()

const props = defineProps<{
  src: string
}>()

const containerRef = ref<HTMLElement | null>(null)
const isLoading = ref(true)
const error = ref<string | null>(null)
const totalPages = ref(0)
const currentPage = ref(1)
const scale = ref(1)

const canvasRefs = new Map<number, HTMLCanvasElement>()
let pdfDoc: pdfjsLib.PDFDocumentProxy | null = null

const setCanvasRef = (el: HTMLCanvasElement | null, page: number) => {
  if (el) canvasRefs.set(page, el)
}

const renderPage = async (pageNum: number) => {
  if (!pdfDoc) return
  const canvas = canvasRefs.get(pageNum)
  if (!canvas) return

  const page = await pdfDoc.getPage(pageNum)
  const viewport = page.getViewport({ scale: scale.value * window.devicePixelRatio })

  canvas.width = viewport.width
  canvas.height = viewport.height
  canvas.style.width = `${viewport.width / window.devicePixelRatio}px`
  canvas.style.height = `${viewport.height / window.devicePixelRatio}px`

  const ctx = canvas.getContext('2d')
  if (!ctx) return

  await page.render({ canvasContext: ctx, viewport }).promise
}

const renderAll = async () => {
  for (let i = 1; i <= totalPages.value; i++) {
    await renderPage(i)
  }
}

const loadPdf = async () => {
  isLoading.value = true
  error.value = null

  try {
    pdfDoc = await pdfjsLib.getDocument(props.src).promise
    totalPages.value = pdfDoc.numPages

    await nextTick()
    await renderAll()
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e)
  } finally {
    isLoading.value = false
  }
}

// Track scroll position for current page indicator
const onScroll = () => {
  if (!containerRef.value) return
  const scrollTop = containerRef.value.scrollTop
  let accumulated = 0

  for (let i = 1; i <= totalPages.value; i++) {
    const canvas = canvasRefs.get(i)
    if (!canvas) continue
    accumulated += canvas.offsetHeight + 16 // gap
    if (accumulated > scrollTop + 100) {
      currentPage.value = i
      break
    }
  }
}

watch(scale, async () => {
  await nextTick()
  await renderAll()
})

watch(() => props.src, () => loadPdf())

onMounted(async () => {
  await loadPdf()
  containerRef.value?.addEventListener('scroll', onScroll, { passive: true })
})

onUnmounted(() => {
  containerRef.value?.removeEventListener('scroll', onScroll)
  pdfDoc?.destroy()
})
</script>
