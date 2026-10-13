import { fileURLToPath } from 'node:url'

import tailwindcss from '@tailwindcss/vite'

/** pdf.js data the PDF viewer loads at run time (spec 044 T038); served under `/pdfjs/`. */
const pdfjs = (folder: string) => ({
  dir: fileURLToPath(
    new URL(`./node_modules/pdfjs-dist/${folder}`, import.meta.url),
  ),
  baseURL: `/pdfjs/${folder}`,
})

// https://nuxt.com/docs/api/configuration/nuxt-config
export default defineNuxtConfig({
  compatibilityDate: '2026-09-09',
  devtools: { enabled: true },
  ssr: false,
  srcDir: 'src/',
  // haex-ui Nuxt layer — provides Shadcn* + Ui* components and the theme
  // tokens. Pinned by commit SHA so upstream changes never surprise our
  // build. Layer declares its peers; we list them explicitly under
  // `dependencies` below. It ships no i18n: pass translated labels to
  // its components (e.g. `UiInputPassword` `labels`).
  // The commit of haex-space/haextension#96 (a closed context menu trigger no
  // longer catches clicks); move it to the merge commit once #96 is merged.
  extends: [
    'github:haex-space/haextension/packages/haex-ui#cbea6d5b6e445fb55b4be70c7ef0753f5643219d',
  ],
  build: {
    transpile: ['reka-ui'],
  },
  // Nuxt/Nitro liest den Dev-Port aus `devServer`, NICHT aus `vite.server.port`
  // (letzteres greift nur beim Vite-Preview nach `nuxt build`). Muss mit
  // tauri.conf.json → build.devUrl und security.devCsp synchron bleiben.
  // `tauri android dev` auf einem echten Telefon setzt TAURI_DEV_HOST auf die
  // Adresse dieses Rechners im lokalen Netz (Spec 043 FR-034); im Emulator und
  // am Desktop bleibt es bei localhost (der Emulator erreicht ihn per
  // `adb reverse`).
  devServer: {
    host: process.env.TAURI_DEV_HOST || 'localhost',
    port: 3030,
  },
  alias: {
    '@bindings': '../src/types/bindings',
  },
  modules: [
    '@nuxtjs/i18n',
    '@nuxt/icon',
    '@nuxt/eslint',
    '@pinia/nuxt',
    '@vueuse/nuxt',
  ],
  components: [{ path: '~/components', pathPrefix: true }],
  css: ['~/assets/css/tailwind.css'],
  nitro: {
    publicAssets: [pdfjs('cmaps'), pdfjs('standard_fonts'), pdfjs('wasm')],
  },
  vite: {
    plugins: [tailwindcss()],
    clearScreen: false,
    server: {
      strictPort: true,
      watch: {
        ignored: ['**/src-tauri/**'],
      },
    },
  },
  i18n: {
    restructureDir: 'src/i18n',
    defaultLocale: 'en',
    locales: [
      { code: 'de', file: 'de.json' },
      { code: 'en', file: 'en.json' },
    ],
    strategy: 'no_prefix',
    detectBrowserLanguage: { useCookie: false, fallbackLocale: 'en' },
  },
  icon: {
    mode: 'svg',
    // Every icon ships with the app, so none waits for the network (holzi runs offline). The scan
    // also reads `.ts`: the app list, the settings registry and other lookup tables name their icons
    // there, and an icon only named in TypeScript would otherwise be fetched from the iconify API.
    clientBundle: {
      scan: { globInclude: ['**/*.{vue,ts}'] },
      includeCustomCollections: true,
    },
    serverBundle: false,
  },
  typescript: {
    strict: true,
  },
})
