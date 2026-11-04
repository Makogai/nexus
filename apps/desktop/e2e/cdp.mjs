// CLI over cdp-lib.mjs for ad-hoc inspection of the running NEXUS webview (development only).
//
//   node e2e/cdp.mjs eval "<expression>"        print the JSON result
//   node e2e/cdp.mjs wait "<expression>" [ms]   poll until truthy
//   node e2e/cdp.mjs shot <file.png>            screenshot
//   node e2e/cdp.mjs send <Method> '<json>'     raw CDP call (e.g. Input.dispatchKeyEvent)
//   node e2e/cdp.mjs console [ms]               reload and print console output / exceptions
import { writeFileSync } from 'node:fs'
import { connect } from './cdp-lib.mjs'

const [cmd, arg, extra] = process.argv.slice(2)
const cdp = await connect()
try {
  if (cmd === 'eval') {
    console.log(JSON.stringify(await cdp.evaluate(arg), null, 2))
  } else if (cmd === 'wait') {
    console.log(JSON.stringify(await cdp.waitFor(arg, Number(extra ?? 15000))))
  } else if (cmd === 'shot') {
    const { data } = await cdp.send('Page.captureScreenshot', { format: 'png' })
    writeFileSync(arg, Buffer.from(data, 'base64'))
    console.log(`saved ${arg}`)
  } else if (cmd === 'send') {
    console.log(JSON.stringify(await cdp.send(arg, extra ? JSON.parse(extra) : {})))
  } else if (cmd === 'console') {
    const lines = []
    cdp.onEvent = (msg) => {
      if (msg.method === 'Runtime.exceptionThrown') {
        const d = msg.params.exceptionDetails
        lines.push(`EXCEPTION ${d.exception?.description ?? d.text}`)
      } else if (msg.method === 'Runtime.consoleAPICalled') {
        lines.push(`${msg.params.type}: ${msg.params.args.map((a) => a.value ?? a.description ?? '').join(' ')}`)
      } else if (msg.method === 'Log.entryAdded') {
        lines.push(`log.${msg.params.entry.level}: ${msg.params.entry.text} ${msg.params.entry.url ?? ''}`)
      }
    }
    await cdp.send('Runtime.enable')
    await cdp.send('Log.enable')
    await cdp.send('Page.reload', { ignoreCache: true })
    await new Promise((r) => setTimeout(r, Number(arg ?? 4000)))
    console.log(lines.join('\n') || '(no console output)')
  } else {
    console.error('usage: cdp.mjs eval|wait|shot|send|console …')
    process.exitCode = 2
  }
} finally {
  cdp.close()
}
