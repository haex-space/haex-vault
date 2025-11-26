import { defineConfig } from 'drizzle-kit'

export default defineConfig({
  schema: './src/database/schemas/**.ts',
  out: './src-tauri/database/migrations',
  dialect: 'sqlite',
  // IMPORTANT: We don't use Drizzle for actual table creation!
  // Drizzle is ONLY used to generate SQL migration files from TypeScript schemas.
  // All tables are created via Rust migration runner (migrations.rs)
  // The vault.db file is NEVER touched by drizzle-kit - it's just a reference for schema introspection
  dbCredentials: {
    url: ':memory:', // Use in-memory DB - we never actually apply migrations via Drizzle
  },
})
