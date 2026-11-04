// Chrome DevTools Protocol helpers for driving the real NEXUS webview (development only).
// The app must run with WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=<port> (Windows).

export async function connect(port = process.env.CDP_PORT ?? '9333') {
  const targets = await (await fetch(`http://127.0.0.1:${port}/json`)).json()
  const page = targets.find((t) => t.type === 'page' && !t.url.startsWith('about:')) ?? targets.find((t) => t.type === 'page')
  if (!page) throw new Error('no page target')
  const ws = new WebSocket(page.webSocketDebuggerUrl)
  await new Promise((res, rej) => {
    ws.onopen = res
    ws.onerror = rej
  })
  let seq = 0
  const pending = new Map()
  const client = { onEvent: null }
  ws.onmessage = (ev) => {
    const msg = JSON.parse(ev.data)
    if (msg.id && pending.has(msg.id)) {
      pending.get(msg.id)(msg)
      pending.delete(msg.id)
    } else if (msg.method && client.onEvent) {
      client.onEvent(msg)
    }
  }
  const send = (method, params = {}) =>
    new Promise((res, rej) => {
      const id = ++seq
      pending.set(id, (m) => (m.error ? rej(new Error(m.error.message)) : res(m.result)))
      ws.send(JSON.stringify({ id, method, params }))
    })
  const evaluate = async (expression) => {
    const r = await send('Runtime.evaluate', { expression, awaitPromise: true, returnByValue: true })
    if (r.exceptionDetails) throw new Error(r.exceptionDetails.exception?.description ?? r.exceptionDetails.text)
    return r.result.value
  }
  const waitFor = async (expression, timeoutMs = 15000) => {
    const deadline = Date.now() + timeoutMs
    for (;;) {
      const v = await evaluate(expression).catch(() => false)
      if (v) return v
      if (Date.now() > deadline) throw new Error(`timed out waiting for: ${expression}`)
      await new Promise((r) => setTimeout(r, 250))
    }
  }
  return Object.assign(client, { send, evaluate, waitFor, close: () => ws.close() })
}

export async function waitForPort(port = process.env.CDP_PORT ?? '9333', timeoutMs = 240000) {
  const deadline = Date.now() + timeoutMs
  for (;;) {
    try {
      const targets = await (await fetch(`http://127.0.0.1:${port}/json`)).json()
      if (targets.some((t) => t.type === 'page' && t.url.startsWith('http'))) return
    } catch {
      /* not up yet */
    }
    if (Date.now() > deadline) throw new Error(`webview debugging port ${port} not reachable`)
    await new Promise((r) => setTimeout(r, 1000))
  }
}
