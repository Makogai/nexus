// Every action shows it is working and how it ended (ADR-0048, rule NX-7). `useAction` wraps one backend call:
// `pending` while it runs (re-entrant calls are ignored, so a double click cannot run it twice), `error` until the next
// attempt or `clearError()` — never cleared by a reload — and optional toasts for success and failure.
import { ref, type Ref } from 'vue'
import { toIpcError } from '../lib/ipc'
import { useAppStore } from '../stores/app'

export interface ActionOptions<A extends unknown[], R> {
  /** Failure toast prefix ("Could not save"); errors also stay in `error` for inline display. */
  failure?: string
  /** Success toast: fixed text, or built from the result. */
  success?: string | ((result: R) => string | null)
  /** For actions on rows: which row is running (`pendingKey`), from the call's arguments. */
  key?: (...args: A) => string
}

export interface Action<A extends unknown[], R> {
  run: (...args: A) => Promise<R | undefined>
  pending: Ref<boolean>
  /** The row the action runs for (with `key`). */
  pendingKey: Ref<string | null>
  isPending: (key: string) => boolean
  error: Ref<string | null>
  clearError: () => void
}

export function useAction<A extends unknown[], R>(fn: (...args: A) => Promise<R>, opts: ActionOptions<A, R> = {}): Action<A, R> {
  const pending = ref(false)
  const pendingKey = ref<string | null>(null)
  const error = ref<string | null>(null)

  async function run(...args: A): Promise<R | undefined> {
    if (pending.value) return undefined
    pending.value = true
    pendingKey.value = opts.key ? opts.key(...args) : null
    error.value = null
    const app = useAppStore()
    try {
      const result = await fn(...args)
      const text = typeof opts.success === 'function' ? opts.success(result) : opts.success
      if (text) app.toast('success', text)
      return result
    } catch (e) {
      const message = toIpcError(e).message
      error.value = message
      if (opts.failure) app.toast('error', `${opts.failure}: ${message}`)
      return undefined
    } finally {
      pending.value = false
      pendingKey.value = null
    }
  }

  return {
    run,
    pending,
    pendingKey,
    isPending: (key: string) => pending.value && pendingKey.value === key,
    error,
    clearError: () => {
      error.value = null
    },
  }
}
