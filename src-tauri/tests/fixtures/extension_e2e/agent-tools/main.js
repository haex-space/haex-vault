// The test extension of spec 018 (research R13): offers holzi's agent the tools of manifest.json
// through the released vault-sdk. `build.sh` bundles this file with the SDK into app.js.
import { createHaexVaultSdk } from '@haex-space/vault-sdk'
import manifest from './manifest.json'

const sdk = createHaexVaultSdk({ manifest })

function log(text) {
  const item = document.createElement('li')
  item.textContent = `${new Date().toLocaleTimeString()} ${text}`
  document.getElementById('log').prepend(item)
  console.info('[agent-tools]', text)
}

async function table() {
  await sdk.ready()
  return sdk.getTableName('entries')
}

async function countAsync() {
  const count = await sdk.database.count(await table())
  document.getElementById('count').textContent = String(count)
  return count
}

sdk.tools.register('count_entries', async () => {
  const count = await countAsync()
  log(`count_entries: ${count}`)
  return { entries: count }
})

sdk.tools.register('add_entry', async (input) => {
  const text = String(input.text)
  await sdk.database.insert(await table(), { id: crypto.randomUUID(), text })
  const count = await countAsync()
  log(`add_entry: ${text}`)
  // Needs the notifications permission; unticked at install, holzi asks (quickstart step 9).
  await sdk.notifications.show({ title: 'Eintrag hinzugefügt', body: text })
  return { added: text, entries: count }
})

sdk.tools.register('slow_echo', (input, { signal }) => {
  const seconds = typeof input.seconds === 'number' ? input.seconds : 30
  log(`slow_echo started: ${input.text}`)
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => resolve(String(input.text)), seconds * 1000)
    signal.addEventListener('abort', () => {
      clearTimeout(timer)
      log('slow_echo cancelled')
      reject(new Error('cancelled'))
    })
  })
})

countAsync().catch((error) => log(`count failed: ${error}`))
