// Every action shows it is working and how it ended (ADR-0048, rule NX-7): while a backend call runs its button is
// disabled, `aria-busy` and spinning (long ones say what they do), it cannot run twice, and it is usable again after.
// Kits, Workspace, Restore, Settings, project discovery, Add project and Sync; only the IPC boundary is mocked.
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount, type VueWrapper } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import type { ClaudeProfile, DoctorReport, KitView, ProjectScan, Restoration, SyncPushReview, SyncStatus, Workspace } from '../src/lib/types'

const api = vi.hoisted(() => ({
  kitList: vi.fn(),
  kitApprove: vi.fn(),
  kitUpdate: vi.fn(),
  kitValidate: vi.fn(),
  kitRemove: vi.fn(),
  kitEnable: vi.fn(),
  kitDisable: vi.fn(),
  profileDetails: vi.fn(),
  workspaceDoctor: vi.fn(),
  workspaceRepairPlan: vi.fn(),
  workspaceRepair: vi.fn(),
  templateList: vi.fn(),
  templatePlan: vi.fn(),
  templateApply: vi.fn(),
  workspaceRestoration: vi.fn(),
  workspaceList: vi.fn(),
  workspaceActive: vi.fn(),
  workspaceActivate: vi.fn(),
  workspaceForget: vi.fn(),
  workspaceImport: vi.fn(),
  shellList: vi.fn(),
  backupList: vi.fn(),
  backupCreate: vi.fn(),
  backupDelete: vi.fn(),
  backupScheduleRestore: vi.fn(),
  backupCancelRestore: vi.fn(),
  appRestart: vi.fn(),
  uiPreferencesSet: vi.fn(),
  projectList: vi.fn(),
  projectScan: vi.fn(),
  projectClone: vi.fn(),
  projectCloneNew: vi.fn(),
  projectRegister: vi.fn(),
  projectLocate: vi.fn(),
  projectInspectPath: vi.fn(),
  syncStatus: vi.fn(),
  syncFetch: vi.fn(),
  syncPull: vi.fn(),
  syncDiff: vi.fn(),
  syncReviewCommit: vi.fn(),
  syncCommit: vi.fn(),
  syncReviewPush: vi.fn(),
  syncPush: vi.fn(),
  syncRestore: vi.fn(),
  syncPolicyGet: vi.fn(),
}))
const updaterCheck = vi.hoisted(() => vi.fn())
const push = vi.hoisted(() => vi.fn())
vi.mock('../src/lib/ipc', () => ({
  api,
  SYNC_EVENT: 'nexus://sync',
  SESSION_EVENT: 'nexus://session',
  toIpcError: (e: unknown) => {
    const o = e as { code?: string; message?: string }
    return { code: o?.code ?? 'internal', message: String(o?.message ?? e) }
  },
}))
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(async () => () => undefined) }))
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: vi.fn(async () => 'D:\\code\\app') }))
vi.mock('@tauri-apps/plugin-opener', () => ({ revealItemInDir: vi.fn() }))
vi.mock('@tauri-apps/plugin-updater', () => ({ check: updaterCheck }))
vi.mock('@tauri-apps/plugin-process', () => ({ relaunch: vi.fn() }))
vi.mock('vue-router', () => ({ useRoute: () => ({ name: 'x', query: {} }), useRouter: () => ({ push, replace: vi.fn() }), RouterLink: { template: '<a><slot /></a>' } }))
vi.mock('../src/router', () => ({ router: { push: vi.fn() } }))

import AddProjectDialog from '../src/components/AddProjectDialog.vue'
import ProjectDiscovery from '../src/components/ProjectDiscovery.vue'
import KitsView from '../src/views/KitsView.vue'
import RestoreView from '../src/views/RestoreView.vue'
import SettingsView from '../src/views/SettingsView.vue'
import SyncView from '../src/views/SyncView.vue'
import WorkspaceView from '../src/views/WorkspaceView.vue'
import { useAppStore } from '../src/stores/app'
import { useProfileStore } from '../src/stores/profiles'
import { useProjectStore } from '../src/stores/projects'
import { useWorkspaceStore } from '../src/stores/workspaces'

/** A backend call that answers when the test says so. */
function slow<T = unknown>() {
  let resolve!: (v: T) => void
  let reject!: (e: unknown) => void
  const promise = new Promise<T>((res, rej) => {
    resolve = res
    reject = rej
  })
  return { promise, resolve, reject }
}
const never = () => new Promise<never>(() => undefined)

const btn = (sel: string) => document.querySelector<HTMLButtonElement>(sel)
const byText = (text: string) => [...document.querySelectorAll<HTMLButtonElement>('button')].find((b) => b.textContent?.trim() === text) ?? null
async function click(target: string | HTMLButtonElement | null) {
  const b = typeof target === 'string' ? btn(target) : target
  expect(b, `${String(target)} is there to click`).not.toBeNull()
  b!.click()
  await flushPromises()
}
/** Disabled, aria-busy and spinning; `text`: the progress it shows. */
function expectRunning(target: string | HTMLButtonElement | null, text?: string) {
  const b = typeof target === 'string' ? btn(target) : target
  expect(b, `${String(target)} exists`).not.toBeNull()
  expect(b!.disabled, 'disabled while the call runs').toBe(true)
  expect(b!.getAttribute('aria-busy'), 'aria-busy while the call runs').toBe('true')
  expect(b!.querySelector('.spin'), 'a spinner while the call runs').not.toBeNull()
  if (text) expect(b!.textContent).toContain(text)
}
function expectIdle(target: string | HTMLButtonElement | null) {
  const b = typeof target === 'string' ? btn(target) : target
  expect(b, `${String(target)} exists`).not.toBeNull()
  expect(b!.disabled, 'enabled after the call').toBe(false)
  expect(b!.getAttribute('aria-busy')).toBeNull()
  expect(b!.querySelector('.spin')).toBeNull()
}
/** Waiting for another action: disabled, but not the one that runs. */
function expectWaiting(sel: string) {
  expect(btn(sel)?.disabled, `${sel} waits`).toBe(true)
  expect(btn(sel)?.getAttribute('aria-busy')).toBeNull()
}
const toasts = (tone: string) => useAppStore().toasts.filter((t) => t.tone === tone).map((t) => t.message)

let wrappers: VueWrapper[] = []
function mountHere(c: Parameters<typeof mount>[0], props: Record<string, unknown> = {}) {
  const w = mount(c, { props, attachTo: document.body })
  wrappers.push(w as VueWrapper)
  return w
}

beforeEach(() => {
  setActivePinia(createPinia())
  for (const fn of Object.values(api)) fn.mockReset()
  updaterCheck.mockReset()
  push.mockReset()
  api.syncPolicyGet.mockImplementation(never)
  api.profileDetails.mockResolvedValue(null)
  api.workspaceList.mockResolvedValue([])
  api.workspaceActive.mockResolvedValue({ id: 'w1' })
  api.projectList.mockResolvedValue([])
})
afterEach(() => {
  for (const w of wrappers) w.unmount()
  wrappers = []
  document.body.innerHTML = ''
})

describe('KitsView', () => {
  const entry = (name: string) => ({ name, description: `${name} skill`, listingChars: 20 })
  const kit: KitView = {
    id: 'k1', name: 'probe-kit', sourceKind: 'git', location: 'https://example.test/kit.git', gitRef: null, subdir: '', kind: 'plugin', version: '1.0.0',
    contentHash: 'h', approvedHash: null, commitSha: null, registeredAt: 0, updatedAt: 0, workspaceId: null, status: 'needs-review', contentDir: '/kits/probe-kit',
    inventory: { name: 'probe-kit', version: '1.0.0', description: 'Probe', kind: 'plugin', skills: [entry('s0')], agents: [], commands: [], outputStyles: [], hooks: [], mcpServers: [], files: 2, bytes: 10, problems: [] },
    profiles: [],
  }
  beforeEach(() => {
    useProfileStore().list = [{ id: 'w', name: 'Work', color: '#7c6cf2', configDir: '/cfg/w' } as ClaudeProfile]
    api.kitList.mockResolvedValue([kit])
  })

  it('approve, validate and fetch run once, show it, and are usable again; a failure stays as an error toast', async () => {
    mountHere(KitsView)
    await flushPromises()

    const approving = slow()
    api.kitApprove.mockReturnValue(approving.promise)
    await click('[data-approve]')
    expectRunning('[data-approve]', 'Approving…')
    expectWaiting('[data-validate]')
    expectWaiting('[data-profile="w"] [data-save]')
    btn('[data-approve]')!.click()
    expect(api.kitApprove).toHaveBeenCalledTimes(1)
    approving.resolve(kit)
    await flushPromises()
    expectIdle('[data-approve]')
    expectIdle('[data-validate]')
    expect(toasts('success')).toContain('Approved probe-kit — this exact content')

    const validating = slow()
    api.kitValidate.mockReturnValue(validating.promise)
    await click('[data-validate]')
    expectRunning('[data-validate]', 'Validating…')
    validating.reject({ code: 'internal', message: 'claude is not installed' })
    await flushPromises()
    expectIdle('[data-validate]')
    expect(toasts('error')).toContain('Could not run the validator: claude is not installed')

    const updating = slow()
    api.kitUpdate.mockReturnValue(updating.promise)
    await click('[data-kit-update]')
    expectRunning('[data-kit-update]', 'Fetching the update…')
    updating.resolve(kit)
    await flushPromises()
    expectIdle('[data-kit-update]')
  })

  it('Refresh shows that it reads the kits', async () => {
    mountHere(KitsView)
    await flushPromises()
    const refresh = byText('Refresh')
    const listing = slow<KitView[]>()
    api.kitList.mockReturnValueOnce(listing.promise)
    await click(refresh)
    expectRunning(refresh)
    listing.resolve([kit])
    await flushPromises()
    expectIdle(refresh)
  })
})

describe('WorkspaceView', () => {
  const ws: Workspace = {
    id: 'w1', name: 'Personal', description: '', rootPath: 'C:\\ws', createdAt: 0, lastOpenedAt: null, projectCount: 1, available: true,
    compat: { schemaVersion: 2, writable: true, reason: null, needsMigration: false }, template: null,
  }
  const report = (): DoctorReport => ({
    workspaceId: 'w1', checkedAt: Date.now(), counts: { ok: 0, info: 0, warn: 1, error: 0 },
    findings: [{ check: 'workspace-skills', area: 'skills', status: 'warn', summary: 'skills do not reach Claude yet', detail: [], recommendation: null, repair: { id: 'register-workspace-skills', title: 'Register the workspace skills' }, navigate: 'kits' }],
  })
  beforeEach(() => {
    useWorkspaceStore().active = ws
    api.workspaceDoctor.mockResolvedValue(report())
    api.templateList.mockResolvedValue([{ id: 'ai-engineering-pro', version: '1.0.0', title: 'AI Engineering Pro', summary: 'library', extends: 'developer', features: [], highlights: [], constitutions: 14, stackRuleSets: 13, skills: 11, workflows: 7, integrations: 4, alwaysOnChars: 950 }])
  })

  it('Check again, a repair plan and its failed apply each show their state; the error stays', async () => {
    mountHere(WorkspaceView)
    await flushPromises()

    const checking = slow<DoctorReport>()
    api.workspaceDoctor.mockReturnValueOnce(checking.promise)
    await click('[data-doctor-refresh]')
    expectRunning('[data-doctor-refresh]', 'Checking…')
    expectWaiting('[data-template-review="ai-engineering-pro"]')
    checking.resolve(report())
    await flushPromises()
    expectIdle('[data-doctor-refresh]')
    expectIdle('[data-template-review="ai-engineering-pro"]')

    const planning = slow()
    api.workspaceRepairPlan.mockReturnValue(planning.promise)
    await click('[data-repair="register-workspace-skills"]')
    expectRunning('[data-repair="register-workspace-skills"]')
    planning.resolve({ id: 'register-workspace-skills', title: 'Register', changes: ['register skills'], notes: [] })
    await flushPromises()
    expectIdle('[data-repair="register-workspace-skills"]')

    const applying = slow()
    api.workspaceRepair.mockReturnValue(applying.promise)
    await click('[data-repair-apply]')
    expectRunning('[data-repair-apply]', 'Applying…')
    expectWaiting('[data-doctor-refresh]')
    applying.reject({ code: 'io', message: 'the folder is read-only' })
    await flushPromises()
    expectIdle('[data-repair-apply]')
    expect(toasts('error')).toContain('Repair failed: the folder is read-only')
  })

  it('reviewing a template shows that the plan is being prepared', async () => {
    mountHere(WorkspaceView)
    await flushPromises()
    const planning = slow()
    api.templatePlan.mockReturnValue(planning.promise)
    await click('[data-template-review="ai-engineering-pro"]')
    expectRunning('[data-template-review="ai-engineering-pro"]', 'Preparing…')
    planning.resolve({ template: { id: 'ai-engineering-pro', version: '1.0.0' }, installed: null, items: [{ path: 'a.md', kind: 'constitution', template: 'minimal', action: 'add' }], notes: [] })
    await flushPromises()
    expectIdle('[data-template-review="ai-engineering-pro"]')
    const applying = slow()
    api.templateApply.mockReturnValue(applying.promise)
    await click('[data-template-apply]')
    expectRunning('[data-template-apply]', 'Applying the template…')
    applying.reject({ code: 'conflict', message: 'changed meanwhile' })
    await flushPromises()
    expectIdle('[data-template-apply]')
  })
})

describe('RestoreView', () => {
  const restoration: Restoration = {
    workspaceId: 'w1', name: 'Portable', done: 1, total: 2,
    steps: [{ id: 'projects', title: 'Projects', status: 'todo', summary: '1 project missing here', detail: [], repair: 'clone-missing', navigate: 'projects' }],
    restored: { constitutions: 1, rules: 0, workflows: 0, skills: 0, projects: 1, tasks: 0, decisions: 0, notes: 0, developmentRecords: 0, template: null },
    notRecoverable: [],
  }
  beforeEach(() => {
    useWorkspaceStore().active = { id: 'w1' } as Workspace
    api.workspaceRestoration.mockResolvedValue(restoration)
  })

  it('Check again, Do it… and a cloning Apply show what runs and are usable again', async () => {
    mountHere(RestoreView)
    await flushPromises()

    const checking = slow<Restoration>()
    api.workspaceRestoration.mockReturnValueOnce(checking.promise)
    await click('[data-restore-refresh]')
    expectRunning('[data-restore-refresh]', 'Checking…')
    checking.resolve(restoration)
    await flushPromises()
    expectIdle('[data-restore-refresh]')

    const planning = slow()
    api.workspaceRepairPlan.mockReturnValue(planning.promise)
    await click('[data-restore-repair="clone-missing"]')
    expectRunning('[data-restore-repair="clone-missing"]')
    planning.resolve({ id: 'clone-missing', title: 'Clone missing projects', changes: ['git clone → projects/shop'], notes: [] })
    await flushPromises()
    expectIdle('[data-restore-repair="clone-missing"]')

    const cloning = slow()
    api.workspaceRepair.mockReturnValue(cloning.promise)
    await click('[data-restore-apply]')
    expectRunning('[data-restore-apply]', 'Cloning the projects… this can take a few minutes')
    expectWaiting('[data-restore-refresh]')
    btn('[data-restore-apply]')!.click()
    expect(api.workspaceRepair).toHaveBeenCalledTimes(1)
    cloning.resolve({ id: 'clone-missing', message: 'cloned 1 project(s)' })
    await flushPromises()
    expect(btn('[data-restore-apply]'), 'the plan closes after applying').toBeNull()
    expectIdle('[data-restore-refresh]')
  })
})

describe('SettingsView', () => {
  const backup = (id: string) => ({ id, path: 'x', kind: 'manual' as const, createdAt: Date.now(), schemaVersion: 15, appVersion: '0.1.0', bytes: 4096, restorable: true, problem: null })
  const ws = (id: string) => ({ id, name: id.toUpperCase(), description: '', rootPath: `C:\\${id}`, createdAt: 0, lastOpenedAt: null, projectCount: 0, available: true, compat: null, template: null }) as Workspace
  beforeEach(() => {
    api.shellList.mockResolvedValue([])
    api.backupList.mockResolvedValue([backup('b1'), backup('b2')])
    const app = useAppStore()
    app.info = { version: '0.1.0', platform: 'windows', dataDir: 'C:\\d', dbPath: 'C:\\d\\nexus.db', schemaVersion: 15, supportedSchema: 15, cliPath: null, pendingRestore: null, updates: true } as never
    const workspaces = useWorkspaceStore()
    workspaces.list = [ws('w1'), ws('w2')]
    workspaces.active = ws('w1')
  })

  it('Back up now, Delete and Switch show that they run and are usable again', async () => {
    vi.spyOn(useAppStore(), 'confirm').mockResolvedValue(true)
    mountHere(SettingsView)
    await flushPromises()

    const creating = slow()
    api.backupCreate.mockReturnValue(creating.promise)
    await click('[data-backup-create]')
    expectRunning('[data-backup-create]', 'Backing up…')
    btn('[data-backup-create]')!.click()
    expect(api.backupCreate).toHaveBeenCalledTimes(1)
    creating.resolve(backup('b3'))
    await flushPromises()
    expectIdle('[data-backup-create]')
    expect(btn('[data-backup-create]')!.textContent).toContain('Back up now')

    const deleting = slow()
    api.backupDelete.mockReturnValue(deleting.promise)
    await click('[data-backup-delete="b1"]')
    expectRunning('[data-backup-delete="b1"]')
    expectWaiting('[data-backup-delete="b2"]')
    deleting.resolve(undefined)
    await flushPromises()
    expectIdle('[data-backup-delete="b1"]')

    const switching = slow()
    api.workspaceActivate.mockReturnValue(switching.promise)
    await click('[data-settings-switch-ws="w2"]')
    expectRunning('[data-settings-switch-ws="w2"]', 'Switching…')
    switching.reject({ code: 'not_found', message: 'the folder is missing' })
    await flushPromises()
    expectIdle('[data-settings-switch-ws="w2"]')
    expect(toasts('error')).toContain('Could not switch the workspace: the folder is missing')
  })

  it('Check now shows the update check while it runs', async () => {
    mountHere(SettingsView)
    await flushPromises()
    const checking = slow()
    updaterCheck.mockReturnValue(checking.promise)
    await click('[data-update-check]')
    expectRunning('[data-update-check]', 'Checking…')
    checking.resolve(null)
    await flushPromises()
    expectIdle('[data-update-check]')
  })
})

describe('ProjectDiscovery', () => {
  const scan = (): ProjectScan => ({
    projectsDir: 'projects',
    found: [
      { path: 'C:\\ws\\projects\\billing', rel: 'projects/billing', name: 'billing', remoteUrl: 'https://example.test/billing.git', stacks: [], status: 'known', projectId: 'p2' },
      { path: 'C:\\ws\\projects\\scratch', rel: 'projects/scratch', name: 'scratch', remoteUrl: null, stacks: [], status: 'new', projectId: null },
    ],
    missing: [{ id: 'p1', name: 'Invoices', remoteUrl: 'https://example.test/invoices.git', cloneTo: 'projects/invoices', canClone: true, reason: null }],
    external: [],
  })
  beforeEach(async () => {
    useWorkspaceStore().active = { id: 'w1', name: 'W', rootPath: 'C:\\ws' } as Workspace
    api.projectScan.mockResolvedValue(scan())
    await useProjectStore().load('w1')
  })

  it('a clone says what it clones and that it takes a while; the other actions wait', async () => {
    mountHere(ProjectDiscovery)
    await flushPromises()
    const cloning = slow()
    api.projectClone.mockReturnValue(cloning.promise)
    await click('[data-discovery-clone]')
    expectRunning('[data-discovery-clone]', 'Cloning Invoices… this can take a few minutes')
    expectWaiting('[data-register="projects/scratch"]')
    expectWaiting('[data-link="projects/billing"]')
    btn('[data-discovery-clone]')!.click()
    expect(api.projectClone).toHaveBeenCalledTimes(1)
    cloning.resolve({ id: 'p1' })
    await flushPromises()
    expectIdle('[data-discovery-clone]')
    expectIdle('[data-register="projects/scratch"]')
  })

  it('register, link and rescan show their state', async () => {
    vi.spyOn(useAppStore(), 'confirm').mockResolvedValue(true)
    mountHere(ProjectDiscovery)
    await flushPromises()

    const registering = slow()
    api.projectRegister.mockReturnValue(registering.promise)
    await click('[data-register="projects/scratch"]')
    expectRunning('[data-register="projects/scratch"]', 'Registering…')
    expectWaiting('[data-discovery-clone]')
    registering.reject({ code: 'invalid', message: 'already registered' })
    await flushPromises()
    expectIdle('[data-register="projects/scratch"]')
    expect(toasts('error')).toContain('Could not register the repository: already registered')

    const linking = slow()
    api.projectLocate.mockReturnValue(linking.promise)
    await click('[data-link="projects/billing"]')
    expectRunning('[data-link="projects/billing"]', 'Linking…')
    linking.resolve({ id: 'p2' })
    await flushPromises()
    expectIdle('[data-link="projects/billing"]')

    const scanning = slow<ProjectScan>()
    api.projectScan.mockReturnValueOnce(scanning.promise)
    await click('[data-discovery-rescan]')
    expectRunning('[data-discovery-rescan]')
    scanning.resolve(scan())
    await flushPromises()
    expectIdle('[data-discovery-rescan]')
  })
})

describe('AddProjectDialog', () => {
  beforeEach(async () => {
    useWorkspaceStore().active = { id: 'w1', name: 'W', rootPath: 'C:\\ws' } as Workspace
    await useProjectStore().load('w1')
  })

  it('Clone from Git says that it clones and how long it can take; a failure stays until the next attempt', async () => {
    mountHere(AddProjectDialog, { open: true })
    await flushPromises()
    await click('[data-add-project-mode="git"]')
    const url = document.querySelector<HTMLInputElement>('[data-add-project-url]')!
    url.value = 'https://example.test/team/app.git'
    url.dispatchEvent(new Event('input'))
    await flushPromises()

    const failing = slow()
    api.projectCloneNew.mockReturnValueOnce(failing.promise)
    await click('[data-add-project-clone]')
    expectRunning('[data-add-project-clone]', 'Cloning app… this can take a few minutes')
    btn('[data-add-project-clone]')!.click()
    expect(api.projectCloneNew).toHaveBeenCalledTimes(1)
    failing.reject({ code: 'git', message: 'repository not found' })
    await flushPromises()
    expectIdle('[data-add-project-clone]')
    expect(document.querySelector('.notice-error')?.textContent).toContain('repository not found')

    const cloning = slow()
    api.projectCloneNew.mockReturnValueOnce(cloning.promise)
    await click('[data-add-project-clone]')
    expectRunning('[data-add-project-clone]')
    expect(document.querySelector('.notice-error'), 'the next attempt replaces the old error').toBeNull()
    cloning.resolve({ id: 'p9', name: 'app' })
    await flushPromises()
    expectIdle('[data-add-project-clone]')
    expect(push).toHaveBeenCalledWith({ path: '/projects/p9', query: { tab: 'learn' } })
  })

  it('choosing a folder and adding it show their state', async () => {
    mountHere(AddProjectDialog, { open: true })
    await flushPromises()
    const reading = slow()
    api.projectInspectPath.mockReturnValue(reading.promise)
    await click('[data-add-project-choose]')
    expectRunning('[data-add-project-choose]', 'Reading the folder…')
    reading.resolve({ path: 'D:\\code\\app', suggestedName: 'app', gitRoot: 'D:\\code\\app', remoteUrl: null, stack: { stacks: [], packageManagers: [], hasClaudeMd: false, hasAgentsMd: false }, registeredAs: null, insideWorkspace: false })
    await flushPromises()
    expectIdle('[data-add-project-choose]')

    const adding = slow()
    api.projectRegister.mockReturnValue(adding.promise)
    await click('[data-add-project-add]')
    expectRunning('[data-add-project-add]', 'Adding…')
    expectWaiting('[data-add-project-choose]')
    adding.resolve({ id: 'p8', name: 'app' })
    await flushPromises()
    expectIdle('[data-add-project-add]')
  })
})

describe('SyncView', () => {
  function status(o: Partial<SyncStatus> = {}): SyncStatus {
    return {
      readiness: { workspaceId: 'w1', repo: true, branch: 'main', upstream: 'origin/main', remote: 'https://git.example.invalid/team/ws.git', ahead: 1, behind: 0, changed: [], conflicted: 0, sensitive: [], dataDirInside: false, pendingApprovals: 0, ready: true, issues: [] },
      state: 'ahead',
      remoteName: 'origin',
      lastFetchAt: null,
      incoming: [],
      outgoing: [{ oid: 'abcdef1234567', timestampMs: Date.now(), author: 'T', summary: 'Rule' }],
      incomingFiles: [],
      outgoingFiles: [{ status: 'M', mode: '100644', path: 'rules/r.md' }],
      working: [{ path: 'notes/a.md', status: 'modified', findings: [] }],
      inProgress: null,
      identity: true,
      history: [],
      ...o,
    }
  }
  const clean = { findings: [], blocked: false, files: 1, scannedBytes: 10, truncated: false }
  beforeEach(() => {
    useWorkspaceStore().active = { id: 'w1', name: 'Team' } as Workspace
    api.syncStatus.mockResolvedValue(status())
    vi.spyOn(useAppStore(), 'confirm').mockResolvedValue(true)
  })

  it('Fetch says where it fetches from while the others wait', async () => {
    mountHere(SyncView)
    await flushPromises()
    const fetching = slow()
    api.syncFetch.mockReturnValue(fetching.promise)
    await click('[data-sync-fetch]')
    expectRunning('[data-sync-fetch]', 'Fetching from origin…')
    for (const sel of ['[data-sync-refresh]', '[data-sync-review]', '[data-sync-push]', '[data-sync-restore-open]']) expectWaiting(sel)
    btn('[data-sync-fetch]')!.click()
    expect(api.syncFetch).toHaveBeenCalledTimes(1)
    fetching.resolve({ incoming: 0, remote: 'origin' })
    await flushPromises()
    expectIdle('[data-sync-fetch]')
    expectIdle('[data-sync-push]')
  })

  it('pull says how many commits it brings', async () => {
    api.syncStatus.mockResolvedValue(status({ state: 'behind', incoming: [{ oid: 'a1', timestampMs: 0, author: 'M', summary: 'One' }, { oid: 'a2', timestampMs: 0, author: 'M', summary: 'Two' }] }))
    mountHere(SyncView)
    await flushPromises()
    const pulling = slow()
    api.syncPull.mockReturnValue(pulling.promise)
    await click('[data-sync-pull]')
    expectRunning('[data-sync-pull]', 'Pulling 2 commits…')
    pulling.resolve({ commits: 2, files: [], headBefore: null, headAfter: null, needsApproval: [], kitsChanged: [], warnings: [] })
    await flushPromises()
    expectIdle('[data-sync-pull]')
  })

  it('reviewing and committing show their state; the commit says how many files', async () => {
    mountHere(SyncView)
    await flushPromises()
    const reviewing = slow()
    api.syncReviewCommit.mockReturnValue(reviewing.promise)
    await click('[data-sync-review]')
    expectRunning('[data-sync-review]', 'Inspecting the changes…')
    reviewing.resolve({ branch: 'main', files: [{ status: 'M', path: 'notes/a.md' }], inspection: clean, identity: true, token: 't', suggestedMessage: 'Notes' })
    await flushPromises()
    expectIdle('[data-sync-review]')

    const committing = slow()
    api.syncCommit.mockReturnValue(committing.promise)
    await click('[data-sync-commit-confirm]')
    expectRunning('[data-sync-commit-confirm]', 'Committing 1 file…')
    committing.reject({ code: 'git', message: 'index.lock exists' })
    await flushPromises()
    expectIdle('[data-sync-commit-confirm]')
    expect(toasts('error')).toContain('index.lock exists')
  })

  it('the push review and the push show their state; the push says how many commits go where', async () => {
    mountHere(SyncView)
    await flushPromises()
    const reviewing = slow<SyncPushReview>()
    api.syncReviewPush.mockReturnValue(reviewing.promise)
    await click('[data-sync-push]')
    expectRunning('[data-sync-push]', 'Inspecting the commits…')
    reviewing.resolve({ remote: 'origin', branch: 'main', firstPush: false, commits: [{ oid: 'abcdef1', timestampMs: 0, author: 'T', summary: 'Rule' }], files: [], behind: 0, inspection: clean, token: 'p' })
    await flushPromises()
    expectIdle('[data-sync-push]')

    const pushing = slow()
    api.syncPush.mockReturnValue(pushing.promise)
    await click('[data-sync-push-confirm]')
    expectRunning('[data-sync-push-confirm]', 'Pushing 1 commit to origin…')
    expectWaiting('[data-sync-fetch]')
    pushing.resolve({ remote: 'origin', branch: 'main', commits: 1 })
    await flushPromises()
    expect(btn('[data-sync-push-confirm]'), 'the dialog closes after the push').toBeNull()
    expectIdle('[data-sync-push]')
  })

  it('a diff link is busy while its diff is read', async () => {
    mountHere(SyncView)
    await flushPromises()
    const reading = slow()
    api.syncDiff.mockReturnValue(reading.promise)
    const link = document.querySelector<HTMLButtonElement>('[data-sync-file="notes/a.md"] .link')!
    link.click()
    await flushPromises()
    expect(link.disabled).toBe(true)
    expect(link.getAttribute('aria-busy')).toBe('true')
    reading.resolve({ path: 'notes/a.md', text: '+x', binary: false, truncated: false })
    await flushPromises()
    expect(link.disabled).toBe(false)
    expect(link.getAttribute('aria-busy')).toBeNull()
    expect(document.querySelector('[data-sync-diff]')?.textContent).toContain('+x')
  })
})
