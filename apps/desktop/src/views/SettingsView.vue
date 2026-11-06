<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { open as openDialog } from '@tauri-apps/plugin-dialog'
import { revealItemInDir } from '@tauri-apps/plugin-opener'
import { Archive, Check, FolderInput, FolderOpen, FolderPlus, Monitor, Moon, RotateCcw, Sun, Trash2 } from '@lucide/vue'
import { api, toIpcError } from '../lib/ipc'
import type { BackupInfo, ShellInfo } from '../lib/types'
import { relativeTime } from '../lib/format'
import NewWorkspaceForm, { type WorkspaceSource } from '../components/NewWorkspaceForm.vue'
import { useAppStore } from '../stores/app'
import { useUpdateStore } from '../stores/updates'
import { useWorkspaceStore } from '../stores/workspaces'

const app = useAppStore()
const updates = useUpdateStore()
const workspaces = useWorkspaceStore()
const shells = ref<ShellInfo[]>([])
/** The inline "new workspace" form, when open. */
const newForm = ref<{ source: WorkspaceSource; folder?: string; key: number } | null>(null)
/** A folder picked to open that is not a workspace yet. */
const notYet = ref<string | null>(null)

const backups = ref<BackupInfo[]>([])
const backingUp = ref(false)
const KIND: Record<BackupInfo['kind'], string> = { manual: 'Manual', automatic: 'Automatic (daily)', 'pre-restore': 'Before a restore', 'pre-migration': 'Before an upgrade', replaced: 'Replaced by a restore' }
const visibleBackups = computed(() => backups.value.filter((b) => b.kind !== 'replaced'))

async function loadBackups() {
  backups.value = await api.backupList().catch(() => [])
}

onMounted(async () => {
  shells.value = await api.shellList().catch(() => [])
  await loadBackups()
})

async function backupNow() {
  backingUp.value = true
  const b = await app.attempt(() => api.backupCreate(), 'Could not create the backup')
  backingUp.value = false
  if (b) app.toast('success', `Backup ${b.id} created`)
  await loadBackups()
}

async function restore(b: BackupInfo) {
  const ok = await app.confirm({
    title: `Restore the backup from ${new Date(b.createdAt).toLocaleString()}?`,
    message: 'NEXUS restarts and replaces its database and global knowledge with this backup. The current state is backed up first and the replaced files are kept, so you can go back. Workspace folders and Git repositories are not touched. Running sessions are stopped.',
    confirmLabel: 'Restore and restart',
    danger: true,
  })
  if (!ok) return
  if (await app.attempt(() => api.backupScheduleRestore(b.id), 'Could not schedule the restore')) {
    await app.attempt(() => api.appRestart(), 'Could not restart — quit and start NEXUS to finish the restore')
  }
}

async function cancelRestore() {
  await api.backupCancelRestore()
  await app.load()
}

async function removeBackup(b: BackupInfo) {
  const ok = await app.confirm({ title: 'Delete this backup?', message: `${KIND[b.kind]} backup from ${new Date(b.createdAt).toLocaleString()}. This cannot be undone.`, confirmLabel: 'Delete', danger: true })
  if (ok) {
    await app.attempt(() => api.backupDelete(b.id), 'Could not delete the backup')
    await loadBackups()
  }
}

const cliFolder = computed(() => app.info?.cliPath?.replace(/[\/][^\/]+$/, '') ?? '')
const pathCommand = computed(() => `[Environment]::SetEnvironmentVariable('Path', [Environment]::GetEnvironmentVariable('Path', 'User') + ';${cliFolder.value}', 'User')`)
async function copyPathCommand() {
  await navigator.clipboard.writeText(pathCommand.value).catch(() => undefined)
  app.toast('success', 'Copied — run it in PowerShell, then open a new terminal')
}

const size = (n: number) => (n >= 1 << 20 ? `${(n / (1 << 20)).toFixed(1)} MB` : `${Math.max(1, Math.round(n / 1024))} KB`)

function openNew(source: WorkspaceSource, folder?: string) {
  newForm.value = { source, folder, key: (newForm.value?.key ?? 0) + 1 }
  notYet.value = null
}

async function importWorkspace() {
  const dir = await openDialog({ directory: true, title: 'Choose a folder containing nexus-workspace.json' })
  if (typeof dir !== 'string') return
  notYet.value = null
  try {
    await workspaces.importExisting(dir)
  } catch (e) {
    const err = toIpcError(e)
    // Not a workspace yet: offer to set one up there (the core refuses folders with other content).
    if (err.code === 'not_found') notYet.value = dir
    else app.toast('error', `Could not open workspace: ${err.message}`)
  }
}

async function forget(id: string, name: string) {
  const ok = await app.confirm({
    title: `Forget “${name}” on this machine?`,
    message: 'NEXUS stops listing this workspace. The folder, its manifest and all project repositories stay on disk; you can open it again later.',
    confirmLabel: 'Forget',
    danger: true,
  })
  if (ok) await app.attempt(() => workspaces.forget(id), 'Could not forget workspace')
}

async function reveal(path: string) {
  await app.attempt(() => revealItemInDir(path), 'Could not open the folder')
}
</script>

<template>
  <div class="page narrow">
    <div class="page-header">
      <div>
        <h1>Settings</h1>
        <p>Preferences are stored on this machine.</p>
      </div>
    </div>

    <section class="card">
      <h2 class="card-title">Appearance</h2>
      <div class="theme-choices">
        <button v-for="t in (['dark', 'light', 'system'] as const)" :key="t" class="theme" :class="{ selected: app.prefs.theme === t }" @click="app.prefs.theme = t">
          <Moon v-if="t === 'dark'" /><Sun v-else-if="t === 'light'" /><Monitor v-else />
          <span>{{ t === 'system' ? 'System' : t === 'dark' ? 'Dark' : 'Light' }}</span>
          <Check v-if="app.prefs.theme === t" class="check" />
        </button>
      </div>
    </section>

    <section class="card">
      <h2 class="card-title">Terminal</h2>
      <div class="two">
        <div class="field">
          <label for="font-size">Font size</label>
          <input id="font-size" v-model.number="app.prefs.terminalFontSize" class="input" type="number" min="9" max="24" />
        </div>
        <div class="field">
          <label for="default-shell">Default shell</label>
          <select id="default-shell" v-model="app.prefs.defaultShell" class="select">
            <option :value="null">Automatic</option>
            <option v-for="s in shells" :key="s.id" :value="s.id">{{ s.name }}</option>
          </select>
        </div>
      </div>
      <p class="hint">Copy: Ctrl+Shift+C (or Ctrl+C with a selection) · Paste: Ctrl+V · Find: Ctrl+Shift+F · Toggle panel: Ctrl+`</p>
    </section>

    <section class="card">
      <h2 class="card-title">Workspaces</h2>
      <div class="list ws-list">
        <div v-for="w in workspaces.list" :key="w.id" class="list-row">
          <div class="ws-main">
            <div class="row">
              <strong>{{ w.name }}</strong>
              <span v-if="w.id === workspaces.active?.id" class="badge badge-accent">active</span>
              <span v-if="!w.available" class="badge badge-warn">folder missing</span>
            </div>
            <code class="faint truncate selectable">{{ w.rootPath }}</code>
            <span class="faint small">{{ w.projectCount }} projects · opened {{ relativeTime(w.lastOpenedAt) }}</span>
          </div>
          <button class="btn btn-sm btn-ghost btn-icon" title="Open folder" :disabled="!w.available" @click="reveal(w.rootPath)"><FolderOpen /></button>
          <button v-if="w.id !== workspaces.active?.id" class="btn btn-sm" :disabled="!w.available" @click="app.attempt(() => workspaces.activate(w.id))">Switch</button>
          <button class="btn btn-sm btn-ghost" @click="forget(w.id, w.name)">Forget</button>
        </div>
      </div>
      <div v-if="newForm" class="ws-new" data-settings-new-ws>
        <NewWorkspaceForm :key="newForm.key" :source="newForm.source" :folder="newForm.folder" back-label="Cancel" @created="newForm = null" @cancel="newForm = null" />
      </div>
      <div v-else class="ws-actions">
        <button class="btn" data-settings-create-ws @click="openNew('new')"><FolderPlus /> New workspace…</button>
        <button class="btn" @click="importWorkspace"><FolderInput /> Open existing…</button>
      </div>
      <p v-if="notYet && !newForm" class="notice selectable" data-settings-not-yet>
        {{ notYet }} is not a NEXUS workspace yet.
        <button class="btn btn-sm" @click="openNew('existing', notYet)">Set up a new workspace in this folder</button>
      </p>
    </section>

    <section class="card">
      <h2 class="card-title">Data and privacy</h2>
      <dl class="facts">
        <dt>Data folder</dt>
        <dd class="row"><code class="selectable truncate">{{ app.info?.dataDir }}</code>
          <button class="btn btn-ghost btn-icon btn-sm" title="Open" @click="app.info && reveal(app.info.dbPath)"><FolderOpen /></button>
        </dd>
        <dt>Database</dt>
        <dd>schema v{{ app.info?.schemaVersion }} <span class="faint">(this version understands up to v{{ app.info?.supportedSchema }})</span></dd>
        <dt>Version</dt>
        <dd>NEXUS {{ app.info?.version }} · {{ app.info?.platform }}</dd>
      </dl>
      <p class="hint">
        NEXUS stores no credentials. It never modifies Claude configurations it did not create, and it removes inherited
        Claude session variables, telemetry settings and API keys before launching Claude.
      </p>
    </section>

    <section class="card" data-updates>
      <h2 class="card-title">Version and updates</h2>
      <p>NEXUS {{ app.info?.version }}<template v-if="updates.available"> · <strong>v{{ updates.available.version }} is available</strong></template><template v-else-if="updates.checkedAt"> · up to date <span class="faint">(checked {{ relativeTime(updates.checkedAt) }})</span></template></p>
      <template v-if="app.info?.updates">
        <label class="checkbox">
          <input :checked="app.prefs.autoUpdate !== false" type="checkbox" data-auto-update @change="app.prefs.autoUpdate = ($event.target as HTMLInputElement).checked" />
          <span>Install updates by themselves when NEXUS starts</span>
        </label>
        <div class="row actions">
          <button class="btn" :disabled="updates.phase === 'checking' || updates.phase === 'downloading'" @click="updates.checkNow(false)">Check now</button>
          <button v-if="updates.available" class="btn btn-primary" :disabled="updates.phase === 'downloading'" @click="updates.install()">Update to v{{ updates.available.version }}</button>
        </div>
        <p v-if="updates.error" class="hint">Last check: {{ updates.error }}</p>
        <p class="hint">NEXUS checks the latest published release on start and every few hours. Every download is verified against NEXUS's update-signing key before it is installed; an update never starts while Claude sessions run without asking you.</p>
      </template>
      <p v-else class="hint">This is a development build — it does not update itself.</p>
    </section>

    <section class="card" data-cli>
      <h2 class="card-title">Command-line tool</h2>
      <p v-if="app.info?.cliPath" class="hint">
        <code class="selectable">{{ app.info.cliPath }}</code> — the <code>nexus</code> CLI, also the MCP server and hook handler NEXUS gives Claude
        sessions (always by this full path, so it works without any PATH change).
      </p>
      <p v-else class="notice notice-warn">The <code>nexus</code> CLI was not found next to NEXUS — reinstall NEXUS. Claude sessions start without the NEXUS tools until then.</p>
      <p v-if="app.info?.cliPath && app.info.platform === 'windows'" class="hint">
        To use <code>nexus</code> in your own terminals, add its folder to your user PATH (NEXUS does not change PATH itself):
        <button class="btn btn-sm" data-cli-copy @click="copyPathCommand">Copy PowerShell command</button>
      </p>
    </section>

    <section class="card" data-backups>
      <h2 class="card-title"><Archive /> Backups</h2>
      <p class="hint">
        A backup holds NEXUS's machine-local state on this machine: the database (workspaces registered here, profiles, sessions, approvals,
        gate commands, usage, notifications) and global knowledge. Workspace folders are not included — they live in their own folders and
        Git repositories. NEXUS backs up automatically once a day and before every upgrade and restore.
      </p>
      <p v-if="app.info?.pendingRestore" class="notice notice-warn" data-backup-pending>
        A restore of {{ app.info.pendingRestore }} happens when NEXUS starts next.
        <button class="btn btn-sm" @click="api.appRestart()">Restart now</button>
        <button class="btn btn-sm btn-ghost" @click="cancelRestore">Cancel</button>
      </p>
      <div class="row">
        <button class="btn" :disabled="backingUp" data-backup-create @click="backupNow"><Archive /> {{ backingUp ? 'Backing up…' : 'Back up now' }}</button>
        <button class="btn btn-ghost" @click="app.info && reveal(`${app.info.dataDir}${app.info.platform === 'windows' ? '\\' : '/'}backups`)"><FolderOpen /> Open the backups folder</button>
      </div>
      <table v-if="visibleBackups.length" class="table">
        <thead><tr><th>When</th><th>Kind</th><th>Schema</th><th>Size</th><th /></tr></thead>
        <tbody>
          <tr v-for="b in visibleBackups" :key="b.id" :data-backup="b.id" :data-backup-restorable="b.restorable ? 'yes' : 'no'">
            <td>{{ relativeTime(b.createdAt) }}</td>
            <td>{{ KIND[b.kind] }}</td>
            <td>{{ b.schemaVersion ? `v${b.schemaVersion}` : '–' }}</td>
            <td>{{ size(b.bytes) }}</td>
            <td class="tools">
              <button v-if="b.restorable" class="btn btn-sm" :data-backup-restore="b.id" @click="restore(b)"><RotateCcw /> Restore…</button>
              <span v-else class="faint small" :title="b.problem ?? ''">not restorable: {{ b.problem }}</span>
              <button class="btn btn-sm btn-ghost btn-icon" title="Delete" @click="removeBackup(b)"><Trash2 /></button>
            </td>
          </tr>
        </tbody>
      </table>
      <p v-else class="faint small">No backups yet.</p>
    </section>
  </div>
</template>

<style scoped>
.table {
  width: 100%;
  border-collapse: collapse;
  font-size: 13px;
  margin-top: 10px;
}
.table th {
  text-align: left;
  font-size: 12px;
  font-weight: 500;
  color: var(--text-faint);
  padding: 6px;
}
.table td {
  padding: 6px;
  border-top: 1px solid var(--border);
}
.tools {
  display: flex;
  gap: 6px;
  justify-content: flex-end;
  align-items: center;
}
.small {
  font-size: 12px;
}
.narrow {
  max-width: 880px;
}
.card {
  margin-bottom: 16px;
}
.theme-choices {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: 10px;
}
.theme {
  display: flex;
  align-items: center;
  gap: 10px;
  height: 46px;
  padding: 0 14px;
  border-radius: 10px;
  border: 1px solid var(--border-strong);
  background: var(--bg-input);
  cursor: pointer;
  font-weight: 600;
}
.theme svg {
  width: 17px;
  height: 17px;
}
.theme.selected {
  border-color: var(--accent);
  background: var(--accent-soft);
}
.theme .check {
  margin-left: auto;
  color: var(--accent-strong);
}
.two {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 12px;
}
.ws-list {
  margin-bottom: 14px;
}
.ws-main {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 2px;
}
.small {
  font-size: 12px;
}
.ws-actions {
  display: flex;
  gap: 8px;
}
.ws-new {
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  padding: 14px;
}
.notice .btn {
  margin-left: 8px;
}
.facts {
  display: grid;
  grid-template-columns: 110px 1fr;
  gap: 8px 14px;
  margin: 0 0 12px;
  font-size: 13px;
  align-items: center;
}
.facts dt {
  color: var(--text-faint);
}
.facts dd {
  margin: 0;
  min-width: 0;
}
</style>
