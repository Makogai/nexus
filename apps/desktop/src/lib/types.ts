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
  } | null
  turns: number
  prompts: number
  subagents: number
  lastActivityAt: number | null
  /** Set while Claude waits for the user (Notification hook). */
  attention: string | null
  endReason: string | null
  transcriptPath: string | null
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
