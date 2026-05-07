#!/usr/bin/env node
import net from 'node:net'

// Keep in sync with nuxt.config.ts (devServer.port) and src-tauri/tauri.conf.json (build.devUrl).
const PORT = 31303

const server = net.createServer()

server.once('error', (err) => {
  if (err.code === 'EADDRINUSE') {
    console.error(`\nDev port ${PORT} is already in use.`)
    console.error(
      `Another process is bound to this port. Tauri would load that process's content instead of haex-vault.`,
    )
    console.error(`Find the process with: lsof -i :${PORT}\n`)
    process.exit(1)
  }
  throw err
})

server.once('listening', () => {
  server.close(() => process.exit(0))
})

server.listen(PORT, '0.0.0.0')
