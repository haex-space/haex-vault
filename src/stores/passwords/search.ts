import { useDebounceFn } from '@vueuse/core'
import Fuse from 'fuse.js'
import { usePasswordsStore } from '~/stores/passwords'

export const usePasswordsSearchStore = defineStore(
  'passwordsSearchStore',
  () => {
    const searchInput = ref('')
    const search = ref('')

    const updateSearch = useDebounceFn((value: string) => {
      search.value = value
    }, 300)

    watch(searchInput, (value) => {
      updateSearch(value)
    })

    const { items } = storeToRefs(usePasswordsStore())

    const searchableFuse = computed(() => {
      return new Fuse(items.value, {
        keys: ['title', 'username', 'url', 'note'],
        threshold: 0.2,
        ignoreLocation: true,
        shouldSort: true,
        minMatchCharLength: 2,
      })
    })

    const searchResults = computed(() => {
      if (!search.value) return null
      if (search.value.length < 2) return []
      return searchableFuse.value
        .search(search.value, { limit: 50 })
        .map((match) => match.item)
    })

    return {
      search,
      searchInput,
      searchResults,
    }
  },
)
