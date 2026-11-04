// Mirrors the serde output of nexus-core / nexus-desktop DTOs (camelCase). Keep in sync with Rust.

export interface AppInfo {
  version: string
  platform: string
  dataDir: string
  dbPath: string
  schemaVersion: number
  gitVersion: string | null
  homeDir: string
  recovery: { quarantinedTo: string; reason: string } | null
}

export interface Workspace {
  id: string
  name: string
  description: string
  rootPath: string
  createdAt: number
  lastOpenedAt: number | null
  projectCount: number
  available: boolean
}

export interface ScriptHint {
  kind: string
  command: string
}

export interface StackInfo {
  stacks: string[]
  packageManagers: string[]
  scripts: ScriptHint[]
  hasReadme: boolean
  hasClaudeMd: boolean
  hasAgentsMd: boolean
  hasClaudeDir: boolean
  isGit: boolean
}

export interface Project {
  id: string
  workspaceId: string
  name: string
  localPath: string | null
  remoteUrl: string | null
  stack: StackInfo
  createdAt: number
  lastOpenedAt: number | null
  available: boolean
}

export interface PathInspection {
  path: string
  suggestedName: string
  gitRoot: string | null
  remoteUrl: string | null
  stack: StackInfo
  registeredAs: string | null
  insideWorkspace: boolean
}

export interface CommitInfo {
  oid: string
  summary: string
  timestampMs: number
}

export interface GitStatus {
  isRepo: boolean
  branch: string | null
  detached: boolean
  headOid: string | null
  upstream: string | null
  ahead: number
  behind: number
  staged: number
  unstaged: number
  untracked: number
  conflicted: number
  lastCommit: CommitInfo | null
}

export type InstallSource = 'path' | 'native-installer' | 'npm' | 'homebrew' | 'custom'

export interface Installation {
  path: string
  version: string | null
  source: InstallSource
  launchable: boolean
  issue: string | null
}

export interface ClaudeProfile {
  id: string
  name: string
  description: string
  color: string
  configDir: string | null
  executable: string | null
  extraArgs: string[]
  managedByNexus: boolean
  inheritProviderEnv: boolean
  defaultWorkProfile: string | null
  defaultEffort: string | null
  defaultModel: string | null
  createdAt: number
  updatedAt: number
  lastUsedAt: number | null
  archivedAt: number | null
}

export interface ProfileInput {
  name: string
  description?: string
  color?: string | null
  configDir?: string | null
  executable?: string | null
  extraArgs?: string[]
  inheritProviderEnv?: boolean
  defaultWorkProfile?: string | null
  defaultEffort?: string | null
  defaultModel?: string | null
}

export interface DetectedProfile {
  configDir: string | null
  path: string
  suggestedName: string
  markers: string[]
  source: string
  registeredProfileId: string | null
}

export type PolicySourceKind = 'machine-file' | 'registry' | 'server-managed'

export interface PolicySummary {
  sources: { kind: PolicySourceKind; location: string }[]
  organizationManaged: boolean
  disableSideloadFlags: boolean
  allowManagedHooksOnly: boolean
  disableAllHooks: boolean
  allowManagedMcpServersOnly: boolean
  strictPluginOnlyCustomization: boolean
  allowManagedPermissionRulesOnly: boolean
  bypassPermissionsDisabled: boolean
  managedTelemetry: boolean
  unreadable: string[]
}

export interface ProfileDetails {
  effectiveConfigDir: string
  configDirExists: boolean
  policy: PolicySummary
  executable: Installation | null
  executableError: string | null
}

export interface AuthStatus {
  loggedIn: boolean
  authMethod: string | null
  apiProvider: string | null
  subscriptionType: string | null
  orgName: string | null
  email: string | null
}

export interface WorkProfile {
  id: string
  name: string
  summary: string
  characteristics: string[]
  prompt: string
}

export interface LaunchOptions {
  workProfiles: WorkProfile[]
  defaultWorkProfile: string
  effortLevels: string[]
}

export type SessionKind = 'claude' | 'shell'
export type SessionState = 'starting' | 'running' | 'exited' | 'interrupted' | 'failed'

export interface SessionView {
  id: string
  kind: SessionKind
  host: 'desktop' | 'cli' | 'service'
  hostId: string | null
  workspaceId: string | null
  projectId: string | null
  profileId: string | null
  claudeSessionId: string | null
  title: string
  cwd: string
  branch: string | null
  workProfile: string | null
  effort: string | null
  model: string | null
  program: string
  pid: number | null
  state: SessionState
  exitCode: number | null
  error: string | null
  resumedFrom: string | null
  startedAt: number
  endedAt: number | null
  archivedAt: number | null
  attached: boolean
  running: boolean
  liveElsewhere: boolean
  integrations: {
    mcp: IntegrationStatus
    hooks?: IntegrationStatus
    task?: { id: string; title: string; status: string; briefChars: number }
  } | null
  turns: number
  prompts: number
  subagents: number
  lastActivityAt: number | null
  /** Set while Claude waits for the user (Notification hook). */
  attention: string | null
  endReason: string | null
  transcriptPath: string | null
  taskId: string | null
  conversationRecorded: boolean | null
  resumable: boolean
}

export interface SessionFilter {
  workspaceId?: string
  projectId?: string
  includeArchived?: boolean
  kind?: SessionKind
  limit?: number
}

export interface ShellInfo {
  id: string
  name: string
  program: string
  args: string[]
}

export interface StartClaude {
  profileId: string
  projectId?: string | null
  cwd?: string | null
  workProfile?: string | null
  effort?: string | null
  model?: string | null
  title?: string | null
  /** Work on this knowledge task (its project is used when none is given). */
  taskId?: string | null
  /** Move a backlog/ready task to in-progress (default true). */
  startTask?: boolean
  cols?: number
  rows?: number
}

export interface UiPreferences {
  theme: 'system' | 'dark' | 'light'
  terminalFontSize: number
  terminalHeight: number
  terminalOpen: boolean
  sidebarCollapsed: boolean
  defaultShell: string | null
  lastLaunch: { profileId?: string; workProfile?: string; effort?: string | null } | null
}

export type ProfileDialogMode = 'register' | 'create' | 'edit'

export interface Checkpoint {
  id: string
  sessionId: string
  sessionTitle: string
  kind: 'progress' | 'decision' | 'blocker' | 'done' | 'note'
  summary: string
  source: 'claude' | 'human' | 'nexus'
  createdAt: number
}

export interface IntegrationStatus {
  state: 'enabled' | 'blocked' | 'unavailable'
  reason?: string
  config?: string
}

// --- Workspace knowledge (ADR-0013/0017) -------------------------------------------------------------------------

export type KnowledgeKind = 'rule' | 'task' | 'decision' | 'note' | 'spec' | 'workflow'
export type TaskStatus = 'backlog' | 'ready' | 'in-progress' | 'review' | 'blocked' | 'done'
export type KnowledgeScope = 'global' | 'workspace' | 'stack' | 'project'

/** One indexed file (`knowledge::index::Item`). Invalid files have `valid: false` and an `error`. */
export interface KnowledgeEntry {
  id: string | null
  workspaceId: string | null
  path: string
  kind: KnowledgeKind | null
  title: string
  status: string | null
  projectId: string | null
  priority: 'low' | 'medium' | 'high' | 'urgent' | null
  stacks: string[]
  applies: 'always' | 'on-demand' | null
  /** Git branch the document is about, and the commit it was recorded at (ADR-0018). */
  branch: string | null
  commit: string | null
  privacy: 'shared' | 'local-only'
  updated: string | null
  hash: string
  valid: boolean
  error: string | null
  conflict: string | null
}

export interface KnowledgeItem extends KnowledgeEntry {
  scope: KnowledgeScope
  /** Always-on rule whose current version is not approved on this machine (not given to Claude). */
  needsApproval: boolean
}

export interface KnowledgeReport {
  scanned: number
  changed: number
  removed: number
  invalid: number
  conflicts: number
}

export interface KnowledgeOverview {
  report: KnowledgeReport
  items: KnowledgeItem[]
}

export interface KnowledgeDoc extends KnowledgeEntry {
  fields: Record<string, unknown>
  body: string
  truncated: boolean
  approved: boolean
  relativePath: string
}

export interface KnowledgeHit extends KnowledgeEntry {
  snippet: string
}

export interface KnowledgeCreate {
  kind: KnowledgeKind
  title: string
  body: string
  projectId?: string | null
  localOnly?: boolean
  priority?: string | null
  status?: string | null
  applies?: 'always' | 'on-demand' | null
  stacks?: string[]
  labels?: string[]
  global?: boolean
}

// --- Tasks, branch-aware knowledge, effective instructions (ADR-0018/0019) -------------------------------------

export interface ChecklistItem {
  index: number
  text: string
  done: boolean
}

export interface TaskView {
  id: string
  title: string
  status: TaskStatus
  priority: string | null
  projectId: string | null
  branch: string | null
  commit: string | null
  labels: string[]
  acceptance: string[]
  description: string
  truncated: boolean
  checklist: ChecklistItem[]
  recentLog: string[]
  logEntries: number
  hash: string
}

export interface TaskUpdateInput {
  status?: string | null
  log?: string | null
  expectedHash?: string | null
  check?: number[]
  uncheck?: number[]
}

export type Relevance = 'unbound' | 'this-branch' | 'inherited' | 'other-branch'

export interface ProjectKnowledge {
  git: { branch: string | null; head: string | null }
  tasks: KnowledgeItem[]
  notes: (KnowledgeItem & { relevance: Relevance })[]
  hiddenOtherBranch: number
}

export interface InstructionLayer {
  scope: 'managed' | 'user' | 'project' | 'local'
  kind: 'claude-md' | 'rule' | 'import' | 'agents-md' | 'managed-setting'
  path: string | null
  load: 'at-start' | 'on-demand' | 'excluded' | 'not-loaded' | 'fallback'
  reason: string | null
  chars: number
  lines: number
  importedFrom: string | null
  paths: string[]
}

export interface LaunchRules {
  text?: string | null
  included: string[]
  pendingApproval: string[]
  overBudget: string[]
  alreadyLoaded?: string[]
}

export interface InstructionsPreview {
  claude: {
    layers: InstructionLayer[]
    filesAtStart: number
    charsAtStart: number
    excludes: string[]
    agentsMode: string
    disabled: boolean
    warnings: string[]
  }
  nexus: {
    workProfile: string | null
    workProfileChars: number
    rules: LaunchRules | null
    task: string | null
    taskBriefChars: number
    appendedChars: number
    mcp: string | null
    hooks: string | null
  }
}
