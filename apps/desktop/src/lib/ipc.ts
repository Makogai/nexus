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
  InstructionsPreview,
  ProjectKnowledge,
  ResumeSummary,
  Diagnostics,
  KitProfileRow,
  KitSelection,
  KitValidation,
  KitView,
  MemoryApproval,
  MemoryProposal,
  ApprovalImpact,
  SessionUsage,
  SyncCloned,
  SyncCommitReview,
  SyncFetched,
  SyncFileDiff,
  SyncPulled,
  SyncPushReview,
  SyncRestore,
  SyncSide,
  SyncStatus,
  UsageDimension,
  UsageFilter,
  UsageView,
  Gate,
  GateInput,
  GateResult,
  RunView,
  StartRunInput,
  WorkflowDefinition,
  WorkflowRun,
  WorkflowSummary,
  WorkflowTemplate,
  WorkflowView,
  ContextAnalysis,
  ContextConfig,
  ContextReport,
  ContextSettings,
  ContextStrategy,
  ProjectDocEntry,
  ConstitutionHistory,
  ConstitutionStructure,
  ConstitutionSummary,
  ConstitutionView,
  EffectiveConstitution,
  NewConstitution,
  RuleHit,
  AppNotification,
  NotificationCounts,
  NotificationFilter,
  NotificationPrefs,
  NotificationTarget,
  NotifierAvailability,
  TaskUpdateInput,
  TaskView,
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
/** The notification set changed (any process). */
export const NOTIFICATIONS_EVENT = 'nexus://notifications'
/** A desktop notification was clicked: payload = notification id. */
export const NOTIFICATION_ACTIVATED_EVENT = 'nexus://notification-activated'
/** A workflow run changed (gates ran, a review was given …). */
export const WORKFLOW_EVENT = 'nexus://workflows'
/** New usage data was read from transcripts (ADR-0030). */
export const USAGE_EVENT = 'nexus://usage'
/** A sync operation finished (or was refused) — the Sync Center refreshes (ADR-0031). */
export const SYNC_EVENT = 'nexus://sync'

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
  knowledgeUpdateTask: (workspaceId: string, id: string, u: TaskUpdateInput) =>
    call<KnowledgeEntry>('knowledge_update_task', { workspaceId, id, ...u }),
  knowledgeTask: (workspaceId: string, id: string) => call<TaskView>('knowledge_task', { workspaceId, id }),
  projectKnowledge: (projectId: string) => call<ProjectKnowledge>('project_knowledge', { projectId }),
  projectResume: (projectId: string) => call<ResumeSummary>('project_resume', { projectId }),
  knowledgeProposals: (workspaceId: string, extract = true) => call<MemoryProposal[]>('knowledge_proposals', { workspaceId, extract }),
  knowledgeProposalApprove: (workspaceId: string, id: string, approval: MemoryApproval) =>
    call<KnowledgeEntry>('knowledge_proposal_approve', { workspaceId, id, approval }),
  knowledgeProposalReject: (id: string) => call<void>('knowledge_proposal_reject', { id }),

  profileDiagnostics: (profileId: string) => call<Diagnostics>('profile_diagnostics', { profileId }),
  kitList: () => call<KitView[]>('kit_list'),
  kitRegister: (args: { source: string; git: boolean; gitRef?: string | null; subdir?: string | null; plugin?: string | null }) =>
    call<KitView>('kit_register', args),
  kitApprove: (id: string) => call<KitView>('kit_approve', { id }),
  kitUpdate: (id: string) => call<KitView>('kit_update', { id }),
  kitRemove: (id: string) => call<void>('kit_remove', { id }),
  kitEnable: (kitId: string, profileId: string, selection: KitSelection | null) =>
    call<KitProfileRow>('kit_enable', { kitId, profileId, selection }),
  kitDisable: (kitId: string, profileId: string) => call<void>('kit_disable', { kitId, profileId }),
  kitValidate: (id: string, profileId?: string | null) => call<KitValidation>('kit_validate', { id, profileId }),
  instructionsInspect: (args: { profileId: string; projectId?: string | null; cwd?: string | null; taskId?: string | null; workProfile?: string | null }) =>
    call<InstructionsPreview>('instructions_inspect', args),
  knowledgeApprove: (workspaceId: string, id: string) => call<KnowledgeEntry>('knowledge_approve', { workspaceId, id }),

  workflowTemplates: () => call<WorkflowTemplate[]>('workflow_templates'),
  workflowList: (workspaceId: string) => call<WorkflowSummary[]>('workflow_list', { workspaceId }),
  workflowGet: (workspaceId: string, id: string) => call<WorkflowView>('workflow_get', { workspaceId, id }),
  workflowCreate: (workspaceId: string, input: { title?: string; category?: string; rigor?: string | null; skills?: string[]; template?: string | null; projectId?: string | null }) =>
    call<WorkflowView>('workflow_create', { workspaceId, input }),
  workflowSave: (workspaceId: string, id: string, definition: WorkflowDefinition, rigor: string, skills: string[], expectedHash: string) =>
    call<WorkflowView>('workflow_save', { workspaceId, id, definition, rigor, skills, expectedHash }),
  workflowSaveRaw: (workspaceId: string, id: string, body: string, expectedHash: string) =>
    call<WorkflowView>('workflow_save_raw', { workspaceId, id, body, expectedHash }),
  workflowStart: (workspaceId: string, input: StartRunInput) => call<RunView>('workflow_start', { workspaceId, input }),
  workflowRuns: (workspaceId: string, filter?: { projectId?: string | null; activeOnly?: boolean; limit?: number }) =>
    call<WorkflowRun[]>('workflow_runs', { workspaceId, filter }),
  workflowRun: (id: string) => call<RunView>('workflow_run', { id }),
  workflowReview: (id: string, step: string, approve: boolean, note?: string) => call<RunView>('workflow_review', { id, step, approve, note }),
  workflowCancel: (id: string) => call<RunView>('workflow_cancel', { id }),
  workflowRetry: (id: string, step: string) => call<RunView>('workflow_retry', { id, step }),
  gatesList: (projectId: string) => call<[Gate, GateResult | null][]>('gates_list', { projectId }),
  gateUpsert: (projectId: string, input: GateInput) => call<Gate>('gate_upsert', { projectId, input }),
  gateApprove: (projectId: string, name: string, expectedHash: string) => call<Gate>('gate_approve', { projectId, name, expectedHash }),
  gateRevoke: (projectId: string, name: string) => call<void>('gate_revoke', { projectId, name }),
  gateApprovalImpact: (projectId: string, name: string) => call<ApprovalImpact>('gate_approval_impact', { projectId, name }),
  usageReport: (filter: UsageFilter, by: UsageDimension) => call<UsageView>('usage_report', { filter, by }),
  usageSessions: (filter: UsageFilter, limit?: number) => call<SessionUsage[]>('usage_sessions', { filter, limit: limit ?? null }),
  usageScan: () => call<number>('usage_scan'),
  syncStatus: (workspaceId: string) => call<SyncStatus>('sync_status', { workspaceId }),
  syncFetch: (workspaceId: string) => call<SyncFetched>('sync_fetch', { workspaceId }),
  syncPull: (workspaceId: string) => call<SyncPulled>('sync_pull', { workspaceId }),
  syncDiff: (workspaceId: string, side: SyncSide, path: string) => call<SyncFileDiff>('sync_diff', { workspaceId, side, path }),
  syncReviewCommit: (workspaceId: string, paths: string[] | null) => call<SyncCommitReview>('sync_review_commit', { workspaceId, paths }),
  syncCommit: (workspaceId: string, message: string, paths: string[] | null, token: string) =>
    call<{ oid: string; files: number }>('sync_commit', { workspaceId, message, paths, token }),
  syncReviewPush: (workspaceId: string) => call<SyncPushReview>('sync_review_push', { workspaceId }),
  syncPush: (workspaceId: string, token: string) => call<{ remote: string; branch: string; commits: number }>('sync_push', { workspaceId, token }),
  syncClone: (url: string, folder: string) => call<SyncCloned>('sync_clone', { url, folder }),
  syncRestore: (workspaceId: string) => call<SyncRestore>('sync_restore', { workspaceId }),
  gateRemove: (projectId: string, name: string) => call<boolean>('gate_remove', { projectId, name }),
  gateSuggest: (projectId: string) => call<GateInput[]>('gate_suggest', { projectId }),
  gateRun: (projectId: string, name: string) => call<GateResult>('gate_run', { projectId, name }),

  contextGet: (projectId: string) => call<ContextConfig>('context_get', { projectId }),
  contextSet: (projectId: string, strategy: ContextStrategy, settings?: ContextSettings | null) =>
    call<ContextConfig>('context_set', { projectId, strategy, settings: settings ?? null }),
  contextPresets: () => call<[ContextStrategy, ContextSettings][]>('context_presets'),
  contextAnalyze: (projectId: string) => call<ContextAnalysis>('context_analyze', { projectId }),
  contextDocs: (projectId: string) => call<ProjectDocEntry[]>('context_docs', { projectId }),
  contextInspect: (projectId: string, profileId: string | null) => call<ContextReport>('context_inspect', { projectId, profileId }),

  constitutionList: (workspaceId: string) => call<ConstitutionSummary[]>('constitution_list', { workspaceId }),
  constitutionGet: (workspaceId: string, id: string) => call<ConstitutionView>('constitution_get', { workspaceId, id }),
  constitutionCreate: (workspaceId: string, input: NewConstitution) => call<ConstitutionView>('constitution_create', { workspaceId, input }),
  constitutionSave: (workspaceId: string, id: string, structure: ConstitutionStructure, expectedHash: string) =>
    call<ConstitutionView>('constitution_save', { workspaceId, id, structure, expectedHash }),
  constitutionSaveRaw: (workspaceId: string, id: string, body: string, expectedHash: string) =>
    call<ConstitutionView>('constitution_save_raw', { workspaceId, id, body, expectedHash }),
  constitutionEffective: (workspaceId: string, projectId: string | null, taskId: string | null, includePending = false) =>
    call<EffectiveConstitution>('constitution_effective', { workspaceId, projectId, taskId, includePending }),
  constitutionSearch: (workspaceId: string, query: string) => call<RuleHit[]>('constitution_search', { workspaceId, query }),
  constitutionHistory: (workspaceId: string, id: string) => call<ConstitutionHistory>('constitution_history', { workspaceId, id }),
  constitutionVersion: (workspaceId: string, id: string, commit: string) => call<string>('constitution_version', { workspaceId, id, commit }),

  notificationsList: (filter?: NotificationFilter) => call<AppNotification[]>('notifications_list', { filter }),
  notificationsCounts: () => call<NotificationCounts>('notifications_counts'),
  notificationsMarkRead: (ids: string[], all = false) => call<number>('notifications_mark_read', { ids, all }),
  notificationsDismiss: (ids: string[]) => call<number>('notifications_dismiss', { ids }),
  notificationsSnooze: (id: string, minutes: number) => call<void>('notifications_snooze', { id, minutes }),
  notificationsPrefs: () => call<NotificationPrefs>('notifications_prefs'),
  notificationsSetPrefs: (prefs: NotificationPrefs) => call<NotificationPrefs>('notifications_set_prefs', { prefs }),
  notificationsSetView: (visibleSession: string | null, utcOffsetMinutes: number) =>
    call<void>('notifications_set_view', { visibleSession, utcOffsetMinutes }),
  notificationsAvailability: () => call<NotifierAvailability>('notifications_availability'),
  notificationsTest: () => call<void>('notifications_test'),
  notificationTarget: (id: string) => call<NotificationTarget>('notification_target', { id }),
  notificationsTakeActivation: () => call<string | null>('notifications_take_activation'),
}

export type Api = typeof api
