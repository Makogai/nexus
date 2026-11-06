// Sync Center (ADR-0031, M29): real components + Pinia; IPC, dialogs and router mocked.
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import type { SyncCommitReview, SyncPushReview, SyncStatus } from '../src/lib/types'

const api = vi.hoisted(() => ({
  syncStatus: vi.fn(),
  syncFetch: vi.fn(),
  syncPull: vi.fn(),
  syncDiff: vi.fn(),
  syncReviewCommit: vi.fn(),
  syncCommit: vi.fn(),
  syncReviewPush: vi.fn(),
  syncPush: vi.fn(),
  syncClone: vi.fn(),
  syncRestore: vi.fn(),
  workspaceList: vi.fn(),
  workspaceActive: vi.fn(),
  projectList: vi.fn(),
  uiPreferencesSet: vi.fn(),
}))
vi.mock('../src/lib/ipc', () => ({ api, SYNC_EVENT: 'nexus://sync', SESSION_EVENT: 'nexus://session', toIpcError: (e: unknown) => ({ code: 'internal', message: String((e as { message?: string })?.message ?? e) }) }))
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(async () => () => undefined) }))
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: vi.fn(async () => 'D:\\work') }))
vi.mock('vue-router', () => ({ useRoute: () => ({ name: 'sync', query: {} }), useRouter: () => ({ push: vi.fn(), replace: vi.fn() }), RouterLink: { template: '<a><slot /></a>' } }))
vi.mock('../src/router', () => ({ router: { push: vi.fn() } }))

import SyncView from '../src/views/SyncView.vue'
import CloneWorkspaceForm from '../src/components/CloneWorkspaceForm.vue'
import ConfirmDialog from '../src/components/ConfirmDialog.vue'
import { useWorkspaceStore } from '../src/stores/workspaces'

function status(o: Partial<SyncStatus> = {}): SyncStatus {
  return {
    readiness: { workspaceId: 'w1', repo: true, branch: 'main', upstream: 'origin/main', remote: 'https://git.example.invalid/team/ws.git', ahead: 1, behind: 0, changed: [], conflicted: 0, sensitive: [], dataDirInside: false, pendingApprovals: 0, ready: true, issues: [] },
    state: 'ahead',
    remoteName: 'origin',
    lastFetchAt: Date.now() - 60_000,
    incoming: [],
    outgoing: [{ oid: 'abcdef1234567', timestampMs: Date.now(), author: 'T', summary: 'Rule' }],
    incomingFiles: [],
    outgoingFiles: [{ status: 'M', mode: '100644', path: 'rules/r.md' }],
    working: [
      { path: 'notes/a.md', status: 'modified', findings: [] },
      { path: 'deploy.pem', status: 'added', findings: [{ path: 'deploy.pem', line: null, kind: 'sensitive-file', severity: 'block', message: 'private key or keystore — never synced' }] },
    ],
    inProgress: null,
    identity: true,
    history: [{ id: 'h1', workspaceId: 'w1', op: 'fetch', status: 'failed', branch: 'main', headBefore: null, headAfter: null, commits: 0, files: 0, errorKind: 'offline', message: 'the remote cannot be reached', startedAt: Date.now(), endedAt: Date.now() }],
    ...o,
  }
}
const inspection = (blocked: boolean) => ({
  findings: blocked ? [{ path: 'notes/a.md', line: 3, kind: 'secret', severity: 'block' as const, message: 'looks like a GitHub token — credentials never belong in a workspace' }] : [{ path: 'notes/a.md', line: 1, kind: 'machine-path', severity: 'warn' as const, message: 'an absolute path of this machine' }],
  blocked,
  files: 1,
  scannedBytes: 10,
  truncated: false,
})

beforeEach(() => {
  setActivePinia(createPinia())
  for (const fn of Object.values(api)) fn.mockReset()
  useWorkspaceStore().active = { id: 'w1', name: 'Team' } as never
})

describe('SyncView', () => {
  it('shows status, changes with blocking findings, outgoing commits and history', async () => {
    api.syncStatus.mockResolvedValue(status())
    const w = mount(SyncView, { attachTo: document.body })
    await flushPromises()
    expect(w.find('[data-sync-badge]').text()).toBe('Outgoing changes')
    expect(w.find('[data-sync-file="deploy.pem"]').attributes('data-sync-blocked')).toBe('yes')
    expect((w.find('[data-sync-file="deploy.pem"] input').element as HTMLInputElement).checked, 'blocked files are not preselected').toBe(false)
    expect((w.find('[data-sync-file="notes/a.md"] input').element as HTMLInputElement).checked).toBe(true)
    expect(w.find('[data-sync-outgoing]').text()).toContain('Rule')
    expect(w.find('[data-sync-history-row="fetch:failed"]').text()).toContain('offline')
    expect(w.find('[data-sync-pull]').attributes('disabled'), 'nothing incoming').toBeDefined()
    w.unmount()
  })

  it('commits only after a non-blocking review and a confirmation, with the review token', async () => {
    api.syncStatus.mockResolvedValue(status())
    const blocked: SyncCommitReview = { branch: 'main', files: [{ status: 'M', path: 'notes/a.md' }], inspection: inspection(true), identity: true, token: 't-blocked', suggestedMessage: 'Plan the checkout' }
    api.syncReviewCommit.mockResolvedValueOnce(blocked)
    const w = mount(SyncView, { attachTo: document.body })
    const dialog = mount(ConfirmDialog, { attachTo: document.body })
    await flushPromises()
    await w.find('[data-sync-review]').trigger('click')
    await flushPromises()
    expect(api.syncReviewCommit).toHaveBeenCalledWith('w1', ['notes/a.md'])
    expect(document.querySelector('[data-sync-finding="secret:block"]')?.textContent).toContain('notes/a.md:3')
    expect((document.querySelector('[data-sync-message]') as HTMLTextAreaElement).value, 'Claude’s message is pre-filled').toBe('Plan the checkout')
    ;(document.querySelector('[data-sync-message]') as HTMLTextAreaElement).value = 'Notes'
    document.querySelector('[data-sync-message]')!.dispatchEvent(new Event('input'))
    await flushPromises()
    expect((document.querySelector('[data-sync-commit-confirm]') as HTMLButtonElement).disabled, 'blocked: no commit').toBe(true)

    api.syncReviewCommit.mockResolvedValueOnce({ ...blocked, inspection: inspection(false), token: 't-ok' })
    ;(document.querySelector('[data-sync-commit-dialog]')!.closest('.modal, [role=dialog]')?.querySelector('footer .btn') as HTMLButtonElement | null)?.click()
    await flushPromises()
    await w.find('[data-sync-review]').trigger('click')
    await flushPromises()
    ;(document.querySelector('[data-sync-message]') as HTMLTextAreaElement).value = 'Notes'
    document.querySelector('[data-sync-message]')!.dispatchEvent(new Event('input'))
    await flushPromises()
    api.syncCommit.mockResolvedValue({ oid: 'x', files: 1 })
    ;(document.querySelector('[data-sync-commit-confirm]') as HTMLButtonElement).click()
    await flushPromises()
    expect(document.querySelector('[data-confirm-message]')?.textContent).toContain('Nothing is pushed')
    expect(api.syncCommit, 'not before the confirmation').not.toHaveBeenCalled()
    ;(document.querySelector('[data-confirm-ok]') as HTMLButtonElement).click()
    await flushPromises()
    expect(api.syncCommit).toHaveBeenCalledWith('w1', 'Notes', ['notes/a.md'], 't-ok')
    dialog.unmount()
    w.unmount()
  })

  it('never offers a push the remote would reject or the inspection blocks', async () => {
    api.syncStatus.mockResolvedValue(status())
    const review: SyncPushReview = { remote: 'origin', branch: 'main', firstPush: true, commits: [{ oid: 'abcdef1', timestampMs: 0, author: 'T', summary: 'Rule' }], files: [], behind: 2, inspection: inspection(false), token: 'p' }
    api.syncReviewPush.mockResolvedValue(review)
    const w = mount(SyncView, { attachTo: document.body })
    await flushPromises()
    await w.find('[data-sync-push]').trigger('click')
    await flushPromises()
    const d = document.querySelector('[data-sync-push-dialog]')!.textContent!
    expect(d).toContain('whole history travels')
    expect(d).toContain('never force-pushes')
    expect((document.querySelector('[data-sync-push-confirm]') as HTMLButtonElement).disabled).toBe(true)
    w.unmount()
  })
})

describe('CloneWorkspaceForm', () => {
  it('clones into a new folder named after the repository', async () => {
    api.syncClone.mockResolvedValue({ workspace: { id: 'w2', name: 'Team' }, restore: { steps: [] } })
    api.workspaceList.mockResolvedValue([])
    api.workspaceActive.mockResolvedValue({ id: 'w2', name: 'Team' })
    api.projectList.mockResolvedValue([])
    const w = mount(CloneWorkspaceForm)
    await w.find('[data-sync-clone-url]').setValue('ext::sh -c evil')
    await w.find('[data-sync-clone-parent]').setValue('D:\\work')
    expect(w.find('[data-sync-clone-start]').attributes('disabled'), 'command transports are not offered').toBeDefined()
    await w.find('[data-sync-clone-url]').setValue('https://git.example.invalid/team/team-ws.git')
    expect(w.text()).toContain('D:\\work\\team-ws')
    await w.find('[data-sync-clone-start]').trigger('click')
    await flushPromises()
    expect(api.syncClone).toHaveBeenCalledWith('https://git.example.invalid/team/team-ws.git', 'D:\\work\\team-ws')
    expect(w.emitted('cloned')).toBeTruthy()
  })
})
