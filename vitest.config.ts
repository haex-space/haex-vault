import { defineConfig } from 'vitest/config'
import { resolve } from 'path'
import AutoImport from 'unplugin-auto-import/vite'

export default defineConfig({
  plugins: [
    AutoImport({
      imports: ['vue', 'pinia'],
      dts: false, // Don't generate .d.ts for tests
    }),
  ],
  test: {
    globals: true,
    environment: 'jsdom',
    include: ['src/tests/**/*.test.ts', 'src/tests/**/*.spec.ts'],
    exclude: ['node_modules', 'dist', '.nuxt', 'src-tauri'],
    testTimeout: 30000,
    hookTimeout: 30000,
  },
  resolve: {
    alias: {
      '~': resolve(__dirname, 'src'),
      '@': resolve(__dirname, 'src'),
    },
  },
})
