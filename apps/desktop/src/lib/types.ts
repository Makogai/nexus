// Mirrors the serde output of nexus-core / nexus-desktop DTOs (camelCase). Keep in sync with Rust.

export interface AppInfo {
  version: string
  platform: string
  dataDir: string
  dbPath: string
  schemaVersion: number
  gitVersion: string | null
  homeDir: string
  recovery: { quarantinedTo: string; reason: string; latestBackup: string | null } | null
  /** Schema version this build understands; a newer database is refused, never changed. */
  supportedSchema: number
  /** The `nexus` CLI next to the desktop program (also the MCP server and hook handler). */
  cliPath: string | null
  /** A restore scheduled for the next start. */
  pendingRestore: string | null
  /** A restore applied at this start (backup id) or why it failed. */
  restored: string | null
  restoreError: string | null
}

export interface BackupInfo {
  id: string
  path: string
  kind: 'manual' | 'automatic' | 'pre-restore' | 'pre-migration' | 'replaced'
  createdAt: number
  schemaVersion: number | null
  appVersion: string | null
  bytes: number
  restorable: boolean
  problem: string | null
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
  /** Schema, writability and migration state (ADR-0036); null when the manifest cannot be read. */
  compat: WorkspaceCompat | null
  template: { id: string; version: string } | null
}

export interface WorkspaceCompat {
  schemaVersion: number
  writable: boolean
  reason: string | null
  needsMigration: boolean
}

export interface MigrationChange {
  path: string
  action: 'update' | 'append' | 'create'
  description: string
}

export interface MigrationPlan {
  workspaceId: string
  from: number
  to: number
  changes: MigrationChange[]
  notes: string[]
}

export interface TemplateSummary {
  id: string
  version: string
  title: string
  summary: string
  extends: string | null
  features: string[]
  highlights: string[]
  constitutions: number
  stackRuleSets: number
  skills: number
  workflows: number
  integrations: number
  alwaysOnChars: number
}

export type TemplateAction = 'add' | 'update' | 'unchanged' | 'keep-customized' | 'keep-removed' | 'skip-existing' | 'retire'

export interface TemplatePlan {
  template: { id: string; version: string }
  installed: { id: string; version: string } | null
  items: { path: string; kind: 'constitution' | 'skill' | 'workflow' | 'integration' | 'file'; template: string; action: TemplateAction }[]
  notes: string[]
}

export interface TemplateApplied {
  plan: TemplatePlan
  written: string[]
  savedForReview: string[]
  kitId: string | null
}

export interface ProjectScan {
  projectsDir: string
  found: { path: string; rel: string; name: string; remoteUrl: string | null; stacks: string[]; status: 'registered' | 'known' | 'new'; projectId: string | null }[]
  missing: { id: string; name: string; remoteUrl: string | null; cloneTo: string | null; canClone: boolean; reason: string | null }[]
  external: string[]
}

export interface ContextInitialized {
  dir: string
  created: string[]
  updated: string[]
  kept: string[]
}

export interface EnvRequirement {
  tool: string
  constraint: string
  source: string
}

export interface EnvCheckItem extends EnvRequirement {
  name: string
  installed: string | null
  foundAt: string | null
  status: 'ok' | 'missing' | 'mismatch' | 'unknown'
  installUrl: string | null
  installCommand: string[] | null
}

export interface EnvironmentReport {
  environment: {
    nexus: number
    requirements: EnvRequirement[]
    containers: { dockerfile: string | null; composeFile: string | null; composeServices: string[]; devcontainer: string | null; devcontainerImage: string | null }
    envVars: { name: string; source: string }[]
    setupSection: string | null
    commands: { kind: string; command: string; source: string }[]
  }
  from: 'repository' | 'workspace record'
  checks: EnvCheckItem[]
}

export interface ClaudeSetupMethod {
  id: string
  title: string
  command: string
  shell: 'powershell' | 'cmd' | 'bash' | 'any'
  recommended: boolean
  autoUpdates: boolean
  notes: string
}

export interface ClaudeSetupStatus {
  installations: Installation[]
  ready: boolean
  version: string | null
  profiles: number
  gitForWindows: boolean | null
  node: string | null
  methods: ClaudeSetupMethod[]
  docsUrl: string
  verified: string
  next: string[]
}

export interface DevHistory {
  records: { id: string | null; title: string; path: string; created: string | null; branch: string | null; commit: string | null; sync: 'local' | 'committed' | 'synced' | 'no-repository' }[]
  sessions: { id: string; title: string; startedAt: number; endedAt: number | null; state: string; record: string | null }[]
  /** [id, title, status] */
  tasks: [string, string, string][]
  /** [title, status] */
  decisions: [string, string][]
  /** [oid, summary, timestamp] */
  commits: [string, string, number][]
  /** [gate, status, startedAt] */
  gates: [string, string, number][]
  /** [summary, createdAt] */
  blockers: [string, number][]
}

export type SyncPolicyMode = 'manual' | 'review' | 'trusted'

export interface SyncPolicyView {
  policy: { mode: SyncPolicyMode; trust: { remote: string; branch: string; authorizedAt: number } | null }
  managed: { trustedAllowed: boolean; reviewAllowed: boolean; source: string | null }
  allowlist: string[]
  trustedUnavailable: string | null
  remote: string | null
  branch: string | null
}

export interface RestorationStep {
  id: string
  title: string
  status: 'done' | 'todo' | 'optional' | 'blocked'
  summary: string
  detail: string[]
  repair: string | null
  navigate: string | null
}

export interface Restoration {
  workspaceId: string
  name: string
  steps: RestorationStep[]
  restored: {
    constitutions: number
    rules: number
    workflows: number
    skills: number
    projects: number
    tasks: number
    decisions: number
    notes: number
    developmentRecords: number
    template: string | null
  }
  notRecoverable: string[]
  done: number
  total: number
}

export type DoctorStatus = 'ok' | 'info' | 'warn' | 'error'

export interface DoctorFinding {
  check: string
  area: 'workspace' | 'knowledge' | 'skills' | 'integrations' | 'projects' | 'tools' | 'sync'
  status: DoctorStatus
  summary: string
  detail: string[]
  recommendation: string | null
  repair: { id: string; title: string } | null
  navigate: string | null
}

export interface DoctorReport {
  workspaceId: string
  findings: DoctorFinding[]
  counts: { ok: number; info: number; warn: number; error: number }
  checkedAt: number
}

export interface RepairPlan {
  id: string
  title: string
  changes: string[]
  notes: string[]
}

export interface Migrated {
  plan: MigrationPlan
  backupDir: string | null
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
  /** Keep the default profile's ~/.claude instructions out of this profile's NEXUS sessions (ADR-0020). */
  isolateInstructions: boolean
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
  isolateInstructions?: boolean
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
  managedClaudeMd?: boolean
  claudeMdExcludes?: string[]
  instructionFiles?: string | null
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
    task?: { id: string; title: string; status: string; delivery: 'session-start' | 'tools' | 'none' }
    isolation?: { claudeMdExcludes: string[] }
  } | null
  turns: number
  prompts: number
  subagents: number
  lastActivityAt: number | null
  /** Set while Claude waits for the user (Notification hook). */
  attention: string | null
  /** Structured attention reported by a supported hook (ADR-0025). */
  attentionKind: 'permission' | 'input' | 'idle' | 'error' | null
  attentionAt: number | null
  endReason: string | null
  transcriptPath: string | null
  taskId: string | null
  conversationRecorded: boolean | null
  resumable: boolean
  status: SessionStatus
}

export type AttentionStatus =
  | 'running'
  | 'waiting-for-input'
  | 'permission-required'
  | 'idle'
  | 'error'
  | 'completed'
  | 'failed'
  | 'interrupted'
  | 'unknown'

export interface SessionStatus {
  status: AttentionStatus
  /** `reported` by Claude/the process, or `assumed` (no signal — NEXUS never guesses prompts). */
  certainty: 'reported' | 'assumed'
  since: number | null
  detail: string | null
}

export type NotificationCategory =
  | 'input-required'
  | 'permission-requested'
  | 'agent-finished'
  | 'task-completed'
  | 'task-blocked'
  | 'session-finished'
  | 'session-interrupted'
  | 'workflow-failed'
  | 'workflow-completed'
  | 'process-issue'
  | 'review-required'
  | 'info'

export type NotificationPriority = 'critical' | 'action' | 'success' | 'warning' | 'info'

export interface AppNotification {
  id: string
  category: NotificationCategory
  priority: NotificationPriority
  source: string
  title: string
  body: string
  workspaceId: string | null
  projectId: string | null
  profileId: string | null
  sessionId: string | null
  taskId: string | null
  workflowId: string | null
  targetKind: 'session' | 'task' | 'proposals' | 'rules' | 'kit' | 'project' | 'profile' | 'workflow' | 'sync' | null
  targetId: string | null
  dedupeKey: string | null
  occurrences: number
  createdAt: number
  updatedAt: number
  readAt: number | null
  dismissedAt: number | null
  snoozedUntil: number | null
  resolvedAt: number | null
  desktopState: 'pending' | 'sent' | 'suppressed'
  desktopReason: string | null
}

export interface NotificationFilter {
  workspaceId?: string | null
  projectId?: string | null
  profileId?: string | null
  sessionId?: string | null
  priority?: NotificationPriority | null
  unreadOnly?: boolean
  includeDismissed?: boolean
  limit?: number
}

export interface NotificationCounts {
  unread: number
  actionRequired: number
}

export interface CategoryPref {
  inApp: boolean
  desktop: boolean
}

export interface NotificationPrefs {
  desktopEnabled: boolean
  sound: boolean
  notifyFocusedSession: boolean
  quietHours: [string, string] | null
  categories: Record<NotificationCategory, CategoryPref>
  mutedWorkspaces: string[]
  mutedProfiles: string[]
  /** What desktop notifications may say: details except while locked (default), always generic, always details. */
  lockScreen: 'hide-details' | 'always-generic' | 'show-details'
}

export interface NotifierAvailability {
  backend: string
  available: boolean
  reason: string | null
  clickToOpen: boolean
  sender: string | null
  clickAfterRestart: boolean
}

export interface NotificationTarget {
  kind: AppNotification['targetKind']
  id: string | null
  exists: boolean
  workspaceId: string | null
  projectId: string | null
  attached: boolean
  liveElsewhere: boolean
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
  load: 'at-start' | 'on-demand' | 'excluded' | 'not-loaded' | 'fallback' | 'needs-approval'
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
  constitution?: ConstitutionDelivery | null
}

// --- Constitutions (ADR-0027) ------------------------------------------------------------------------------------

export type Enforcement = 'mandatory' | 'rule' | 'guideline'
export type ScopeLevel = 'global' | 'workspace' | 'stack' | 'project' | 'task'

export interface CRule {
  id: string
  title: string
  enforcement: Enforcement
  enabled: boolean
  overrides: string | null
  /** Quality gate that verifies compliance; without one the rule is instructed only. */
  check?: string | null
  text: string
}

export interface CSection {
  title: string
  text: string
  rules: CRule[]
}

export interface CArticle {
  title: string
  text: string
  sections: CSection[]
}

export interface ConstitutionStructure {
  preamble: string
  articles: CArticle[]
}

export interface ConstitutionProblem {
  severity: 'error' | 'warning'
  message: string
  ruleId: string | null
}

export interface ConstitutionSummary {
  id: string
  title: string
  scope: ScopeLevel
  stacks: string[]
  projectId: string | null
  taskId: string | null
  workflowId: string | null
  applies: 'always' | 'on-demand'
  status: string
  prefix: string
  approved: boolean
  rules: number
  mandatory: number
  errors: number
  warnings: number
  relativePath: string
  hash: string
  updated: string | null
}

export interface RuleRef {
  id: string
  title: string
  enforcement: Enforcement
  docId: string
  docTitle: string
  scope: ScopeLevel
}

export interface ConstitutionView extends ConstitutionSummary {
  structure: ConstitutionStructure
  problems: ConstitutionProblem[]
  body: string
  appliesTo: { id: string; name: string }[]
  otherRules: RuleRef[]
}

export interface EffectiveRule {
  id: string
  title: string
  enforcement: Enforcement
  text: string
  article: string
  section: string
  docId: string
  docTitle: string
  scope: ScopeLevel
  replaces: RuleRef | null
  check?: string | null
}

export interface ConstitutionConflict {
  kind: 'mandatory-override' | 'same-scope-duplicate' | 'unknown-override' | 'invalid-document'
  message: string
  ruleId: string | null
  docs: string[]
}

export interface RuleDoc {
  id: string
  title: string
  scope: ScopeLevel
  applies: string
}

export interface RuleCompliance {
  ruleId: string
  enforcement: Enforcement
  status: 'instructed' | 'passed' | 'failed' | 'not-run' | 'not-approved' | 'gate-missing' | 'no-project'
  check: string | null
  at: number | null
}

export interface EffectiveConstitution {
  compliance?: RuleCompliance[]
  rules: EffectiveRule[]
  overridden: { rule: RuleRef; by: RuleRef; disabled: boolean }[]
  conflicts: ConstitutionConflict[]
  pending: RuleDoc[]
  documents: RuleDoc[]
}

export interface ConstitutionDelivery {
  text: string | null
  included: string[]
  onDemand: string[]
  pendingApproval: string[]
  conflicts: number
}

export interface RuleHit {
  rule: RuleRef
  article: string
  section: string
  snippet: string
}

export interface ConstitutionHistory {
  available: boolean
  reason: string | null
  commits: { oid: string; summary: string; timestampMs: number }[]
}

export interface NewConstitution {
  title: string
  scope: ScopeLevel
  stacks?: string[]
  projectId?: string | null
  taskId?: string | null
  applies?: 'always' | 'on-demand' | null
  prefix?: string | null
  starter?: boolean
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
    isolationExcludes: string[]
  }
}

// --- Resume project (ADR-0021) -----------------------------------------------------------------------------------

export interface CommitRef {
  oid: string
  summary: string
  at: number
}

export interface ResumeSummary {
  projectId: string
  projectName: string
  verified: {
    checkedAt: number
    available: boolean
    repo: boolean
    branch: string | null
    detached: boolean
    head: CommitRef | null
    upstream: string | null
    ahead: number
    behind: number
    staged: number
    unstaged: number
    untracked: number
    conflicted: number
    recentCommits: CommitRef[]
  }
  saved: {
    tasks: { id: string; title: string; status: string; priority: string | null; branch: string | null; checklistDone: number; checklistTotal: number; lastProgress: string | null; updated: string | null }[]
    notes: { id: string; kind: string; title: string; status: string | null; relevance: Relevance; branch: string | null; updated: string | null }[]
    decisions: { id: string; kind: string; title: string; status: string | null; relevance: Relevance; branch: string | null; updated: string | null }[]
    checkpoints: { kind: string; summary: string; sessionTitle: string; at: number; branch: string | null; otherBranch: boolean }[]
    sessions: { id: string; title: string; state: string; startedAt: number; endedAt: number | null; resumable: boolean; taskId: string | null; branch: string | null }[]
    hiddenOtherBranch: number
    pendingProposals: number
  }
  next: {
    kind: 'resume-session' | 'launch-task' | 'resolve-conflicts' | 'review-changes' | 'pull' | 'launch'
    label: string
    evidence: string[]
    sessionId: string | null
    taskId: string | null
  }
  warnings: string[]
}

// --- Memory proposals (ADR-0022) ---------------------------------------------------------------------------------

export interface MemoryProposal {
  id: string
  workspaceId: string
  projectId: string | null
  sessionId: string
  sessionTitle: string
  kind: 'note' | 'decision'
  title: string
  body: string
  source: string
  branch: string | null
  status: 'pending' | 'approved' | 'rejected'
  documentId: string | null
  createdAt: number
  decidedAt: number | null
}

export interface MemoryApproval {
  title?: string
  body?: string
  scope?: 'workspace' | 'project' | 'branch'
  localOnly?: boolean
}

// --- Profile diagnostics (ADR-0023) and AI Kits (ADR-0024) --------------------------------------------------------

export interface InvEntry {
  name: string
  description: string
  listingChars: number
}

export interface HookGroupInfo {
  event: string
  handlers: number
  types: string[]
  programs: string[]
}

export interface McpServerInfo {
  name: string
  transport: string
  target: string
  envVars: number
  headers: number
}

export interface KitInventory {
  name: string
  version: string | null
  description: string | null
  kind: 'plugin' | 'skills'
  skills: InvEntry[]
  agents: InvEntry[]
  commands: InvEntry[]
  outputStyles: string[]
  hooks: HookGroupInfo[]
  mcpServers: McpServerInfo[]
  files: number
  bytes: number
  problems: string[]
}

export interface DiagItem extends InvEntry {
  source: 'user' | 'plugin' | 'kit'
  origin: string | null
}

export interface Diagnostics {
  profileId: string
  profileName: string
  configDir: string
  defaultLocation: boolean
  configExists: boolean
  managedByNexus: boolean
  isolateInstructions: boolean
  executable: string | null
  version: string | null
  executableError: string | null
  policy: PolicySummary
  policyEffects: string[]
  settings: {
    path: string
    exists: boolean
    parseError: string | null
    model: string | null
    defaultPermissionMode: string | null
    outputStyle: string | null
    permissionRules: [number, number, number]
    additionalDirectories: number
    envNames: string[]
    sensitiveEnvNames: string[]
    apiKeyHelper: boolean
    cloudCredentialHelper: boolean
    statusLine: boolean
    disableAllHooks: boolean
    claudeMdExcludes: number
    enabledPlugins: string[]
    disabledPlugins: string[]
    extraMarketplaces: string[]
    otherKeys: string[]
  }
  memory: { claudeMdLines: number; claudeMdChars: number; rules: number; rulesChars: number }
  skills: DiagItem[]
  agents: DiagItem[]
  commands: DiagItem[]
  outputStyles: string[]
  mcpServers: (McpServerInfo & { qualifiedName: string; scope: 'user' | 'local' | 'plugin' | 'managed' | 'kit'; origin: string | null })[]
  hooks: (HookGroupInfo & { source: 'user' | 'plugin' | 'kit'; origin: string | null })[]
  plugins: {
    id: string
    name: string
    marketplace: string | null
    version: string | null
    scope: string | null
    enabled: boolean
    installed: boolean
    installPath: string | null
    skills: number
    agents: number
    commands: number
    hooks: number
    mcpServers: number
    listingTokens: number
    runsCode: boolean
  }[]
  marketplaces: { name: string; kind: string; location: string }[]
  kits: { kitId: string; inventory: KitInventory; blocked: string | null }[]
  context: { skillsTokens: number; agentsTokens: number; commandsTokens: number; memoryTokens: number; totalTokens: number }
  warnings: string[]
}

export interface KitSelection {
  skills: string[] | null
  agents: boolean
  commands: boolean
  hooks: boolean
  mcp: boolean
}

export interface KitProfileRow extends KitSelection {
  kitId: string
  profileId: string
  enabledAt: number
}

export interface KitView {
  id: string
  name: string
  sourceKind: 'local' | 'git'
  location: string
  gitRef: string | null
  subdir: string
  kind: 'plugin' | 'skills'
  version: string | null
  contentHash: string
  approvedHash: string | null
  commitSha: string | null
  registeredAt: number
  updatedAt: number
  /** Set for a workspace's own skills kit: only that workspace's sessions get it (ADR-0037). */
  workspaceId: string | null
  status: 'approved' | 'needs-review' | 'changed' | 'missing'
  contentDir: string
  inventory: KitInventory
  profiles: KitProfileRow[]
}

export interface KitValidation {
  ok: boolean
  exitCode: number | null
  output: string
}


// --- Context Manager (ADR-0028) ----------------------------------------------------------------------------------

export type ContextStrategy = 'minimal' | 'balanced' | 'deep' | 'custom'

export interface ContextSettings {
  alwaysRules: boolean
  constitution: 'mandatory' | 'rules' | 'on-demand'
  rulesBudget: number
  startupSummary: boolean
  startupIndex: boolean
  checkpointsAtStart: boolean
  startupChars: number
  projectDocs: boolean
  observeInstructions: boolean
  retrievalLimit: number
  retrievalChars: number
}

export interface ContextConfig {
  projectId: string
  strategy: ContextStrategy
  settings: ContextSettings
  configured: boolean
  updatedAt: number | null
}

export interface ContextEstimate {
  strategy: ContextStrategy
  rulesChars: number
  startupCharsMax: number
  rulesInPrompt: number
  onDemand: number
}

export interface ContextAnalysis {
  projectId: string
  project: string
  docs: { files: number; bytes: number; sample: string[]; skipped: string[] }
  instructions: { claudeMd: boolean; agentsMd: boolean; claudeDir: boolean }
  knowledge: { rules: number; alwaysRules: number; openTasks: number; decisions: number; notes: number; specs: number; workflows: number; invalid: number; conflicts: number }
  constitutionRules: number
  constitutionMandatory: number
  estimates: ContextEstimate[]
  recommendation: ContextStrategy
  reasons: string[]
  configured: ContextStrategy | null
}

export interface ProjectDocEntry {
  path: string
  title: string
  size: number
  modifiedMs: number
}

export interface ContextReport {
  configured: ContextConfig
  predicted: InstructionsPreview | null
  observed: {
    session: { id: string; title: string; startedAt: number; profileId: string | null } | null
    instructionsObserved: boolean
    instructions: { path: string; memoryType: string; loadReason: string; predicted: boolean }[]
    missing: string[]
    nexus: { rules?: LaunchRules | null; task?: unknown; kits?: unknown; context?: { strategy: ContextStrategy; observeInstructions: boolean } | null } | null
  }
  onDemand: { knowledge: ContextAnalysis['knowledge']; projectDocs: number; constitutionRules: string[]; tools: string[] }
  retrieved: { tool: string; results: number; chars: number; ids: string[]; at: number }[]
  metrics: {
    sessions: number
    sessionsWithRetrieval: number
    retrievals: number
    chars: number
    empty: number
    byTool: [string, number][]
    topDocuments: [string, number][]
  }
}

// --- Workflows (ADR-0029) ----------------------------------------------------------------------------------------

export type StepKind = 'claude' | 'check' | 'review'
export type Rigor = 'prototype' | 'balanced' | 'professional' | 'critical'

export interface WorkflowStep {
  id: string
  title: string
  kind: StepKind
  needs: string[]
  gates: string[]
  rigor: Rigor | null
  text: string
  criteria: string[]
}

export interface WorkflowDefinition {
  preamble: string
  steps: WorkflowStep[]
}

export interface WorkflowSummary {
  id: string
  title: string
  category: string
  rigor: Rigor
  skills: string[]
  steps: number
  gates: string[]
  approved: boolean
  errors: number
  status: string
  relativePath: string
  hash: string
  projectId: string | null
}

export interface WorkflowView extends WorkflowSummary {
  definition: WorkflowDefinition
  problems: { severity: 'error' | 'warning'; message: string; step: string | null }[]
  body: string
}

export interface WorkflowTemplate {
  id: string
  title: string
  category: string
  summary: string
}

export interface WorkflowRun {
  id: string
  workspaceId: string
  projectId: string | null
  workflowId: string
  workflowTitle: string
  workflowHash: string
  taskId: string
  goal: string
  rigor: Rigor
  profileId: string | null
  skills: string[]
  status: 'running' | 'waiting' | 'completed' | 'failed' | 'cancelled'
  error: string | null
  createdAt: number
  updatedAt: number
  endedAt: number | null
}

export interface GateResult {
  id: string
  projectId: string | null
  runId: string | null
  stepId: string | null
  gateName: string
  command: string
  status: 'running' | 'passed' | 'failed' | 'timeout' | 'error' | 'interrupted' | 'cancelled'
  exitCode: number | null
  durationMs: number | null
  outputTail: string
  startedAt: number
  endedAt: number | null
  /** The gate version (approved command) that produced this result. */
  gateVersion: number | null
}

export interface RunStep {
  stepId: string
  position: number
  kind: StepKind
  status: 'pending' | 'ready' | 'blocked' | 'verifying' | 'awaiting-review' | 'passed' | 'failed' | 'skipped' | 'cancelled'
  verification: 'ai-reported' | 'verified' | 'human' | 'skipped' | null
  attempts: number
  maxAttempts: number
  report: string | null
  criteriaMet: number[]
  lastFailure: string | null
  updatedAt: number
  /** 'gates' while reported work waits for its gates to be defined or approved. */
  blockedOn: string | null
  title: string
  text: string
  needs: string[]
  gates: string[]
  criteria: string[]
  results: GateResult[]
}

export interface RunView extends WorkflowRun {
  steps: RunStep[]
  progress: { total: number; done: number; verified: number; aiReported: number; human: number; skipped: number }
}

export interface Gate {
  id: string
  projectId: string
  name: string
  title: string
  program: string
  args: string[]
  timeoutSecs: number
  updatedAt: number
  /** Increments whenever program or arguments change. */
  version: number
  commandHash: string
  /** The current command (this version) was authorised; only then does the gate run. */
  approved: boolean
  approvedAt: number | null
}

/** What approving a gate resumes: steps waiting only for it (in `runs` runs) and steps that keep waiting. */
export interface ApprovalImpact {
  steps: number
  runs: number
  stillWaiting: number
}

export interface GateInput {
  name: string
  title?: string
  program: string
  args: string[]
  timeoutSecs?: number | null
}

export interface StartRunInput {
  workflowId: string
  projectId?: string | null
  taskId?: string | null
  goal: string
  rigor?: Rigor | null
  profileId?: string | null
}

// ---- Usage analytics (ADR-0030) -------------------------------------------------------------------------------

export type UsageDimension = 'project' | 'profile' | 'model' | 'strategy' | 'work-profile' | 'task' | 'workflow'
export type UsageStatus = 'measured' | 'no-requests' | 'pending' | 'missing' | 'unreadable'

export interface UsageFilter {
  from?: number | null
  to?: number | null
  workspaceId?: string | null
  projectId?: string | null
  profileId?: string | null
  taskId?: string | null
  workflowRunId?: string | null
  strategy?: string | null
  workProfile?: string | null
  model?: string | null
}

/** Token counts reported by the Claude API, parsed from Claude Code's session transcripts (provenance `parsed`). */
export interface UsageTokens {
  requests: number
  input: number
  output: number
  cacheWrite: number
  cacheRead: number
  webSearches: number
  subagentRequests: number
}

export interface UsageEfficiency {
  tokensPerSession: number | null
  contextPerRequest: number | null
  outputPerRequest: number | null
  cacheHit: number | null
}

/** Claude Code's own cost estimate (API prices) — an estimate, never a bill; API-billed profiles only (ADR-0010). */
export interface UsageCost {
  usd: number | null
  incomplete: boolean
  /** Sessions left out: subscription usage is never converted into spend. */
  withheldSubscription: number
  /** Sessions left out because the profile's billing is not known yet. */
  withheldUnknown: number
}

/** NEXUS's estimate (characters ÷ 4) of the context it added itself. */
export interface UsageContextEstimate {
  sessions: number
  tokensPerSession: number | null
  shareOfContext: number | null
}

export interface UsageGroup {
  key: string
  label: string
  sessions: number
  tokens: UsageTokens
  efficiency: UsageEfficiency
  cost: UsageCost
  contextEstimate: UsageContextEstimate
}

export interface UsageDay {
  day: string
  tokens: UsageTokens
}

export interface UsageCoverage {
  sessions: number
  measured: number
  noRequests: number
  pending: number
  missing: number
  unreadable: number
  outsideNexusRequests: number
}

export interface UsageReport {
  totals: UsageTokens
  sessions: number
  efficiency: UsageEfficiency
  cost: UsageCost
  contextEstimate: UsageContextEstimate
  coverage: UsageCoverage
  daily: UsageDay[]
  by: UsageDimension
  groups: UsageGroup[]
  claudeVersions: string[]
  /** Every transcript format read is one NEXUS has verified; otherwise parsed values may be inaccurate. */
  formatVerified: boolean
  /** ADR-0010 provenance of the token counts: API-reported, parsed from Claude Code's transcripts. */
  tokenProvenance: 'parsed'
}

export interface TelemetryNote {
  profileId: string
  profile: string
  organizationManaged: boolean
  managedTelemetry: boolean
}

export interface UsageView {
  report: UsageReport
  telemetry: TelemetryNote[]
}

export interface SessionUsage {
  sessionId: string
  title: string
  startedAt: number
  endedAt: number | null
  live: boolean
  projectId: string | null
  project: string | null
  profileId: string | null
  profile: string | null
  taskId: string | null
  task: string | null
  workflowRunId: string | null
  workflow: string | null
  strategy: string
  workProfile: string | null
  status: UsageStatus
  detail: string | null
  tokens: UsageTokens
  cost: UsageCost
  contextEstimate: number | null
}

// ---- Workspace sync (ADR-0031, M29) ---------------------------------------------------------------------------

export type SyncState = 'not-repo' | 'conflicts' | 'in-progress' | 'no-remote' | 'no-upstream' | 'diverged' | 'behind' | 'ahead' | 'up-to-date'

export interface SyncIssue {
  severity: 'error' | 'warning' | 'info'
  message: string
}

export interface SensitiveFile {
  path: string
  tracked: boolean
  reason: string
}

export interface SyncReadiness {
  workspaceId: string
  repo: boolean
  branch: string | null
  upstream: string | null
  remote: string | null
  ahead: number
  behind: number
  changed: string[]
  conflicted: number
  sensitive: SensitiveFile[]
  dataDirInside: boolean
  pendingApprovals: number
  ready: boolean
  issues: SyncIssue[]
}

/** A finding of the security inspection — names the file, line and kind, never the matched value. */
export interface SyncFinding {
  path: string
  line: number | null
  kind: string
  severity: 'block' | 'warn'
  message: string
}

export interface SyncInspection {
  findings: SyncFinding[]
  blocked: boolean
  files: number
  scannedBytes: number
  truncated: boolean
}

export interface GitLogEntry {
  oid: string
  timestampMs: number
  author: string
  summary: string
}

export interface GitDiffEntry {
  status: string
  mode: string
  path: string
}

export interface SyncWorkingFile {
  path: string
  status: 'added' | 'modified' | 'deleted' | 'conflicted'
  findings: SyncFinding[]
}

export interface SyncHistoryEntry {
  id: string
  workspaceId: string
  op: 'fetch' | 'pull' | 'commit' | 'push' | 'clone'
  status: 'running' | 'ok' | 'refused' | 'failed' | 'interrupted'
  branch: string | null
  headBefore: string | null
  headAfter: string | null
  commits: number
  files: number
  errorKind: string | null
  message: string | null
  startedAt: number
  endedAt: number | null
}

export interface SyncStatus {
  readiness: SyncReadiness
  state: SyncState
  remoteName: string | null
  lastFetchAt: number | null
  incoming: GitLogEntry[]
  outgoing: GitLogEntry[]
  incomingFiles: GitDiffEntry[]
  outgoingFiles: GitDiffEntry[]
  working: SyncWorkingFile[]
  inProgress: string | null
  identity: boolean
  history: SyncHistoryEntry[]
}

export interface SyncFetched {
  remote: string
  incoming: number
  outgoing: number
}

export interface SyncNeedsApproval {
  id: string
  title: string
  kind: string
}

export interface SyncPulled {
  commits: number
  files: string[]
  headBefore: string | null
  headAfter: string | null
  needsApproval: SyncNeedsApproval[]
  kitsChanged: string[]
  warnings: SyncFinding[]
}

export interface SyncCommitReview {
  branch: string | null
  files: { status: 'A' | 'M' | 'D' | 'U'; path: string }[]
  inspection: SyncInspection
  identity: boolean
  token: string
}

export interface SyncPushReview {
  remote: string
  branch: string
  firstPush: boolean
  commits: GitLogEntry[]
  files: GitDiffEntry[]
  behind: number
  inspection: SyncInspection
  token: string
}

export type SyncSide = 'working' | 'incoming' | 'outgoing'

export interface SyncFileDiff {
  path: string
  text: string
  truncated: boolean
  binary: boolean
}

export interface SyncRestore {
  workspaceId: string
  name: string
  projects: { id: string; name: string; remoteUrl: string | null; available: boolean }[]
  pendingApprovals: SyncNeedsApproval[]
  kitFolders: [string, boolean][]
  gatesNeeded: string[]
  steps: string[]
}

export interface SyncCloned {
  workspace: Workspace
  restore: SyncRestore
}
