import { fileURLToPath } from 'node:url'

// https://nuxt.com/docs/api/configuration/nuxt-config
export default defineNuxtConfig({
  compatibilityDate: '2025-07-15',
  devtools: { enabled: true },

  srcDir: './src',

  alias: {
    '@bindings': fileURLToPath(
      new URL('./src-tauri/bindings', import.meta.url),
    ),
  },

  app: {
    head: {
      viewport: 'width=device-width, initial-scale=1.0, viewport-fit=cover',
    },
    pageTransition: {
      name: 'fade',
    },
  },

  modules: [
    'nuxt-zod-i18n',
    '@nuxtjs/i18n',
    '@pinia/nuxt',
    '@vueuse/nuxt',
    '@nuxt/icon',
    '@nuxt/eslint',
    '@nuxt/fonts',
    '@nuxt/ui',
  ],

  imports: {
    dirs: [
      'composables/**',
      'stores/**',
      'components/**',
      'pages/**',
      'types/**',
    ],
    presets: [
      {
        from: '@vueuse/gesture',
        imports: [
          'useDrag',
          'useGesture',
          'useHover',
          'useMove',
          'usePinch',
          'useScroll',
          'useWheel',
        ],
      },
    ],
  },

  css: ['./assets/css/main.css'],

  icon: {
    // Use local bundles only - no runtime fetching from external servers
    provider: 'iconify',
    mode: 'svg',
    clientBundle: {
      // Bundle ALL icons from these collections for offline use
      scan: true,
      sizeLimitKb: 0, // 0 = no limit, bundle everything that's scanned
      includeCustomCollections: true,
      icons: [
        // Explicitly bundled icons (used dynamically, not detected by scan)
        'solar:global-outline',
        'gg:extension',
        'hugeicons:corporate',
        // System window icons (from windowManager.ts)
        'hugeicons:developer',
        'mdi:cog',
        'mdi:store',
        'heroicons:bug-ant',
        // Theme icons (from stores/ui/index.ts)
        'line-md:moon-rising-alt-loop',
        'line-md:moon-to-sunny-outline-loop-transition',
        // UCheckbox default icon
        'lucide:check',
        // UButton loading icon
        'lucide:loader-circle',
      ],
    },
    serverBundle: {
      collections: [
        'heroicons',
        'mdi',
        'line-md',
        'solar',
        'gg',
        'emojione',
        'lucide',
        'hugeicons',
      ],
    },

    customCollections: [
      {
        prefix: 'my-icon',
        dir: './src/assets/icons/',
      },
    ],

    // Disable fetching from external API - all icons must be bundled
    fetchTimeout: 0,
  },

  i18n: {
    strategy: 'prefix_and_default',
    defaultLocale: 'de',

    locales: [
      { code: 'de', language: 'de-DE', isCatchallLocale: true },
      { code: 'en', language: 'en-EN' },
    ],

    detectBrowserLanguage: {
      useCookie: true,
      cookieKey: 'i18n_redirected',
      redirectOn: 'root', // recommended
    },
    types: 'composition',

    vueI18n: './i18n.config.ts',
  },

  zodI18n: {
    localeCodesMapping: {
      'en-GB': 'en',
      'de-DE': 'de',
    },
  },

  runtimeConfig: {
    public: {
      haexVault: {
        deviceFileName: 'device.json',
        defaultVaultName: 'HaexVault',
      },
    },
  },

  ssr: false,
  // Enables the development server to be discoverable by other devices when running on iOS physical devices
  devServer: {
    host: '0',
    port: 3003,
  },

  vite: {
    // Better support for Tauri CLI output
    clearScreen: false,
    // Enable environment variables
    // Additional environment variables can be found at
    // https://v2.tauri.app/reference/environment-variables/
    envPrefix: ['VITE_', 'TAURI_'],
    server: {
      // Tauri requires a consistent port
      strictPort: true,
    },
  },
  ignore: ['**/src-tauri/**'],
})
