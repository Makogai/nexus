// Typed wrappers over Tauri IPC. The only module that talks to the backend.
import { Channel, invoke } from '@tauri-apps/api/core'
import type {
  AppInfo,
  AuthStatus,
  Checkpoint,
  ClaudeProfile,
  DetectedProfile,
  GitStatus,
  Installation,
  KnowledgeCreate,
  KnowledgeDoc,
  KnowledgeEntry,
  KnowledgeHit,
  KnowledgeOverview,
  LaunchOptions,
  PathInspection,
  ProfileDetails,
  ProfileInput,
  Project,
  SessionFilter,
  SessionView,
  ShellInfo,
  StartClaude,
  Workspace,
} from './types'

export class IpcError extends Error {
  constructor(
    public code: string,
    message: string,
  ) {
    super(message)
    this.name = 'IpcError'
  }
}

export function toIpcError(e: unknown): IpcError {
  if (e instanceof IpcError) return e
  if (e && typeof e === 'object' && 'message' in e) {
    const o = e as { code?: string; message: string }
    return new IpcError(o.code ?? 'internal', String(o.message))
  }
  return new IpcError('internal', String(e))
}

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(cmd, args)
  } catch (e) {
    throw toIpcError(e)
  }
}

export const SESSION_EVENT = 'nexus://session'

export const api = {
  appInfo: () => call<AppInfo>('app_info'),
  uiPreferencesGet: () => call<Record<string, unknown> | null>('ui_preferences_get'),
  uiPreferencesSet: (value: Record<string, unknown>) => call<void>('ui_preferences_set', { value }),

  workspaceList: () => call<Workspace[]>('workspace_list'),
  workspaceActive: () => call<Workspace | null>('workspace_active'),
  workspaceCreate: (path: string, name: string, description?: string) =>
    call<Workspace>('workspace_create', { path, name, description }),
  workspaceImport: (path: string) => call<Workspace>('workspace_import', { path }),
  workspaceActivate: (id: string) => call<Workspace>('workspace_activate', { id }),
  workspaceForget: (id: string) => call<void>('workspace_forget', { id }),

  projectList: (workspaceId: string) => call<Project[]>('project_list', { workspaceId }),
  projectGet: (id: string) => call<Project>('project_get', { id }),
  projectInspectPath: (workspaceId: string, path: string) =>
    call<PathInspection>('project_inspect_path', { workspaceId, path }),
  projectRegister: (workspaceId: string, path: string, name?: string) =>
    call<Project>('project_register', { workspaceId, path, name }),
  projectUnregister: (id: string) => call<void>('project_unregister', { id }),
  projectLocate: (id: string, path: string) => call<Project>('project_locate', { id, path }),
  projectRefresh: (id: string) => call<Project>('project_refresh', { id }),
  projectGitStatus: (id: string) => call<GitStatus>('project_git_status', { id }),
  projectCheckpoints: (id: string, limit?: number) => call<Checkpoint[]>('project_checkpoints', { id, limit }),
  dbDataVersion: () => call<number>('db_data_version'),

  claudeInstallations: () => call<Installation[]>('claude_installations'),
  profileList: (includeArchived = false) => call<ClaudeProfile[]>('profile_list', { includeArchived }),
  profileDetect: () => call<DetectedProfile[]>('profile_detect'),
  profileRegister: (input: ProfileInput) => call<ClaudeProfile>('profile_register', { input }),
  profileCreate: (input: ProfileInput) => call<ClaudeProfile>('profile_create', { input }),
  profileUpdate: (id: string, input: ProfileInput) => call<ClaudeProfile>('profile_update', { id, input }),
  profileSetArchived: (id: string, archived: boolean) =>
    call<ClaudeProfile>('profile_set_archived', { id, archived }),
  profileForget: (id: string) => call<void>('profile_forget', { id }),
  profileDetails: (id: string) => call<ProfileDetails>('profile_details', { id }),
  profileAuthStatus: (id: string) => call<AuthStatus>('profile_auth_status', { id }),
  launchOptions: () => call<LaunchOptions>('launch_options'),

  sessionList: (filter?: SessionFilter) => call<SessionView[]>('session_list', { filter }),
  sessionGet: (id: string) => call<SessionView>('session_get', { id }),
  sessionStartClaude: (args: StartClaude) => call<SessionView>('session_start_claude', { args }),
  sessionResume: (id: string, cols?: number, rows?: number) =>
    call<SessionView>('session_resume', { id, cols, rows }),
  shellList: () => call<ShellInfo[]>('shell_list'),
  sessionOpenShell: (args: { shellId?: string | null; projectId?: string | null; cwd?: string | null; cols?: number; rows?: number }) =>
    call<SessionView>('session_open_shell', { args }),
  sessionAttach: (id: string, onData: (bytes: Uint8Array) => void) => {
    const output = new Channel<ArrayBuffer>()
    output.onmessage = (buf) => onData(new Uint8Array(buf))
    return call<boolean>('session_attach', { id, output })
  },
  sessionWrite: (id: string, data: string, binary = false) => call<void>('session_write', { id, data, binary }),
  sessionResize: (id: string, cols: number, rows: number) => call<void>('session_resize', { id, cols, rows }),
  sessionInterrupt: (id: string) => call<void>('session_interrupt', { id }),
  sessionTerminate: (id: string) => call<void>('session_terminate', { id }),
  sessionRelease: (id: string) => call<boolean>('session_release', { id }),
  sessionRename: (id: string, title: string) => call<SessionView>('session_rename', { id, title }),
  sessionSetArchived: (id: string, archived: boolean) =>
    call<SessionView>('session_set_archived', { id, archived }),

  knowledgeOverview: (workspaceId: string, rebuild = false) =>
    call<KnowledgeOverview>('knowledge_overview', { workspaceId, rebuild }),
  knowledgeRead: (workspaceId: string, id: string) => call<KnowledgeDoc>('knowledge_read', { workspaceId, id }),
  knowledgeSearch: (workspaceId: string, query: string, kinds?: string[]) =>
    call<KnowledgeHit[]>('knowledge_search', { workspaceId, query, kinds }),
  knowledgeCreate: (workspaceId: string, input: KnowledgeCreate) =>
    call<KnowledgeEntry>('knowledge_create', { workspaceId, input }),
  knowledgeUpdateTask: (workspaceId: string, id: string, status: string | null, log?: string | null, expectedHash?: string | null) =>
    call<KnowledgeEntry>('knowledge_update_task', { workspaceId, id, status, log, expectedHash }),
  knowledgeApprove: (workspaceId: string, id: string) => call<KnowledgeEntry>('knowledge_approve', { workspaceId, id }),
}

export type Api = typeof api
