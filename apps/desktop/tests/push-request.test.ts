// Push request outcomes (ADR-0047): the dialog never loses an error to its own reload, shows the result of a decision,
// keeps Push disabled while the remote is ahead and offers "Tell Claude" for a rejected push.
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import type { PushRequest, PushReview, SessionView } from '../src/lib/types'

const api = vi.hoisted(() => ({
  pushRequestReview: vi.fn(),
  pushRequestDecide: vi.fn(),
  pushRequestDismiss: vi.fn(),
  sessionWrite: vi.fn(),
  uiPreferencesSet: vi.fn(),
}))
const push = vi.hoisted(() => vi.fn())
vi.mock('../src/lib/ipc', () => ({ api, SESSION_EVENT: 'nexus://session', toIpcError: (e: unknown) => ({ code: 'external', message: String((e as { message?: string })?.message ?? e) }) }))
vi.mock('vue-router', () => ({ useRouter: () => ({ push }), useRoute: () => ({ query: {} }) }))

import PushRequestDialog from '../src/components/PushRequestDialog.vue'
import { useAppStore } from '../src/stores/app'
import { useSessionStore } from '../src/stores/sessions'
import { behindExplanation, failureGuidance, tellClaudeText } from '../src/lib/push'

const ModalStub = { props: ['open'], template: '<div v-if="open"><slot /><slot name="footer" /></div>' }

function request(o: Partial<PushRequest> = {}): PushRequest {
  return { id: 'r1', sessionId: 's1', projectId: 'p1', branch: 'main', head: 'abc1234', reason: 'Ship the fix', status: 'pending', message: null, failureKind: null, createdAt: 1, decidedAt: null, dismissedAt: null, ...o }
}
function review(o: Partial<PushReview> = {}): PushReview {
  return {
    request: request(),
    projectName: 'anilist',
    remote: 'origin',
    remoteUrl: 'https://git.example.invalid/anilist.git',
    remoteBranch: 'main',
    firstPush: false,
    head: 'abc1234',
    moved: false,
    commits: [{ oid: 'abc1234567', timestampMs: Date.now(), author: 'Ann', summary: 'Our change' }],
    inspection: { findings: [], blocked: false, files: 1, scannedBytes: 10, truncated: false },
    token: 'abc1234',
    fetched: true,
    fetchError: null,
    ahead: 1,
    behind: 0,
    diverged: false,
    incoming: [],
    ...o,
  }
}
const mountDialog = () => mount(PushRequestDialog, { global: { stubs: { BaseModal: ModalStub } } })
const approve = (w: ReturnType<typeof mountDialog>) => w.find('[data-push-approve]')

beforeEach(() => {
  setActivePinia(createPinia())
  Object.values(api).forEach((f) => f.mockReset())
  push.mockReset()
})

describe('PushRequestDialog', () => {
  it('keeps a decision error visible after it reloads the review', async () => {
    api.pushRequestReview.mockResolvedValue(review())
    api.pushRequestDecide.mockRejectedValue(new Error('the branch moved since you reviewed it — review it again'))
    useAppStore().pushRequestId = 'r1'
    const w = mountDialog()
    await flushPromises()
    await approve(w).trigger('click')
    await flushPromises()
    expect(api.pushRequestReview).toHaveBeenCalledTimes(2) // reloaded after the error …
    expect(w.find('[data-push-error]').text()).toContain('the branch moved') // … and the error is still there
  })

  it('shows the pushed result with a way on to the Commits tab', async () => {
    api.pushRequestReview.mockResolvedValue(review())
    api.pushRequestDecide.mockResolvedValue(request({ status: 'pushed', message: 'pushed 1 commit(s) to origin/main' }))
    useAppStore().pushRequestId = 'r1'
    const w = mountDialog()
    await flushPromises()
    await approve(w).trigger('click')
    await flushPromises()
    expect(w.find('[data-push-result="pushed"]').text()).toContain('pushed 1 commit(s) to origin/main')
    await w.find('[data-push-open-commits]').trigger('click')
    expect(push).toHaveBeenCalledWith({ name: 'project', params: { id: 'p1' }, query: { tab: 'commits' } })
  })

  it('disables Push while the remote is ahead and says why, with the incoming commits', async () => {
    api.pushRequestReview.mockResolvedValue(review({ behind: 3, diverged: true, incoming: [{ oid: 'fff0000111', timestampMs: Date.now(), author: 'Bo', summary: 'Teammate change' }] }))
    useAppStore().pushRequestId = 'r1'
    const w = mountDialog()
    await flushPromises()
    expect(approve(w).attributes('disabled')).toBeDefined()
    const behind = w.find('[data-push-behind]').text()
    expect(behind).toContain("origin/main has 3 commits you don't have")
    expect(behind).toContain('NEXUS never force-pushes')
    expect(behind).toContain('Teammate change')
  })

  it('says when the remote could not be checked and still allows a reviewed attempt', async () => {
    api.pushRequestReview.mockResolvedValue(review({ fetched: false, fetchError: { kind: 'offline', message: 'Could not resolve host' } }))
    useAppStore().pushRequestId = 'r1'
    const w = mountDialog()
    await flushPromises()
    expect(w.find('[data-push-fetch-error]').text()).toContain('cannot be reached')
    expect(approve(w).attributes('disabled')).toBeUndefined()
  })

  it('shows a rejected push with guidance and types the fixed message into the running session without Enter', async () => {
    const failed = request({ status: 'failed', failureKind: 'rejected', message: 'origin/main has 2 commit(s) this branch does not have' })
    api.pushRequestReview.mockResolvedValueOnce(review()).mockResolvedValue(review({ request: failed, behind: 2, diverged: true }))
    api.pushRequestDecide.mockResolvedValue(failed)
    const sessions = useSessionStore()
    sessions.upsert({ id: 's1', title: 'Fix login', running: true, liveElsewhere: false } as SessionView)
    useAppStore().pushRequestId = 'r1'
    const w = mountDialog()
    await flushPromises()
    await approve(w).trigger('click')
    await flushPromises()
    const result = w.find('[data-push-result="failed"]')
    expect(result.attributes('data-push-failure')).toBe('rejected')
    expect(result.text()).toContain('Rejected — the remote has new commits')
    expect(result.text()).toContain('pull --rebase or merge')
    await w.find('[data-push-tell-claude]').trigger('click')
    await flushPromises()
    const [sid, text] = api.sessionWrite.mock.calls[0] ?? []
    expect(sid).toBe('s1')
    expect(text).toBe('NEXUS: the push of main was rejected — origin/main has 2 new commits. Integrate them (git pull --rebase or merge), resolve conflicts, run the tests, then call request_push again.')
    expect(text).not.toMatch(/[\r\n]/)
    expect(sessions.activeTab).toBe('s1')
  })

  it('opened for a failed request, it explains the failure; Dismiss sets it aside', async () => {
    api.pushRequestReview.mockResolvedValue(review({ request: request({ status: 'failed', failureKind: 'auth', message: 'Authentication failed' }) }))
    api.pushRequestDismiss.mockResolvedValue(request({ status: 'failed', dismissedAt: 2 }))
    const app = useAppStore()
    app.pushRequestId = 'r1'
    const w = mountDialog()
    await flushPromises()
    expect(w.find('[data-push-result="failed"]').text()).toContain('refused the credentials')
    expect(w.find('[data-push-tell-claude]').exists()).toBe(false) // only for a rejected push
    await w.find('[data-push-dismiss]').trigger('click')
    await flushPromises()
    expect(api.pushRequestDismiss).toHaveBeenCalledWith('r1')
    expect(app.pushRequestId).toBeNull()
  })

  it('declines a request it cannot review, such as one for a branch that is no longer checked out', async () => {
    api.pushRequestReview.mockRejectedValue(new Error('fix/old is no longer checked out — switch back to it to push it, or decline the request'))
    api.pushRequestDecide.mockResolvedValue(request({ branch: 'fix/old', status: 'declined' }))
    const app = useAppStore()
    app.pushRequestId = 'r1'
    const w = mountDialog()
    await flushPromises()
    expect(w.find('[data-push-load-error]').text()).toContain('no longer checked out')
    expect(approve(w).attributes('disabled')).toBeDefined()
    await w.find('[data-push-decline]').trigger('click')
    await flushPromises()
    expect(api.pushRequestDecide).toHaveBeenCalledWith('r1', false, '') // declining needs no reviewed commit
    expect(app.pushRequestId).toBeNull()
  })

  it('lets the person select and copy the findings that block a push', async () => {
    const findings = [
      { path: 'src/inspect.rs', line: 227, kind: 'secret', severity: 'block' as const, message: 'looks like a private key — credentials never belong in a workspace' },
      { path: '', line: null, kind: 'not-fully-inspected', severity: 'warn' as const, message: 'the history is larger than NEXUS inspects' },
    ]
    api.pushRequestReview.mockResolvedValue(review({ inspection: { findings, blocked: true, files: 2, scannedBytes: 10, truncated: true } }))
    const writeText = vi.fn().mockResolvedValue(undefined)
    Object.defineProperty(navigator, 'clipboard', { value: { writeText }, configurable: true })
    useAppStore().pushRequestId = 'r1'
    const w = mountDialog()
    await flushPromises()
    expect(w.find('[data-push-findings]').classes()).toContain('selectable')
    await w.find('[data-copy-findings]').trigger('click')
    await flushPromises()
    expect(writeText).toHaveBeenCalledWith('blocks   src/inspect.rs:227 — looks like a private key — credentials never belong in a workspace\nwarning  (the change) — the history is larger than NEXUS inspects')
    expect(w.find('[data-copy-findings]').text()).toBe('Copied')
  })

  it('offers no copy button when nothing was found', async () => {
    api.pushRequestReview.mockResolvedValue(review())
    useAppStore().pushRequestId = 'r1'
    const w = mountDialog()
    await flushPromises()
    expect(w.find('[data-copy-findings]').exists()).toBe(false)
  })
})

describe('push wording', () => {
  it('names the remote, the count and the next step; strips control characters from Git names', () => {
    expect(behindExplanation({ remote: 'origin', remoteBranch: 'main', behind: 1 })).toBe("origin/main has 1 commit you don't have. Integrate it first (pull --rebase or merge), resolve any conflicts, run the tests, then push. NEXUS never force-pushes.")
    expect(tellClaudeText({ branch: 'feat\u001b[31m/x' }, null)).toContain('the push of feat[31m/x was rejected — the remote branch has new commits.')
    expect(failureGuidance('offline')).toContain('nothing was sent')
  })
})
