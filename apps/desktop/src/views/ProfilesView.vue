<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { revealItemInDir } from '@tauri-apps/plugin-opener'
import {
  Archive,
  Building2,
  CircleAlert,
  CircleCheck,
  FolderOpen,
  KeyRound,
  Pencil,
  Plus,
  RefreshCw,
  ScanSearch,
  ShieldAlert,
  Sparkles,
  Stethoscope,
} from '@lucide/vue'
import { useRouter } from 'vue-router'
import type { ClaudeProfile, DetectedProfile, PolicySummary, ProfileDialogMode } from '../lib/types'
import { relativeTime, shortPath } from '../lib/format'
import { useAppStore } from '../stores/app'
import { useProfileStore } from '../stores/profiles'
import ProfileDialog from '../components/ProfileDialog.vue'

const app = useAppStore()
const profiles = useProfileStore()
const router = useRouter()

const dialog = ref<{ open: boolean; mode: ProfileDialogMode; profile: ClaudeProfile | null; detected: DetectedProfile | null }>({
  open: false,
  mode: 'register',
  profile: null,
  detected: null,
})

function openDialog(mode: ProfileDialogMode, profile: ClaudeProfile | null = null, detected: DetectedProfile | null = null) {
  dialog.value = { open: true, mode, profile, detected }
}

async function refreshAll() {
  await Promise.all([profiles.load(), profiles.detect(), profiles.loadInstallations()]).catch(() => undefined)
  for (const p of profiles.list) {
    profiles.loadDetails(p.id)
    profiles.loadAuth(p.id)
  }
}
onMounted(refreshAll)

function restrictions(p: PolicySummary): string[] {
  const r: string[] = []
  if (p.disableSideloadFlags) r.push('Session plugins and MCP flags are disabled')
  if (p.allowManagedHooksOnly) r.push('Only managed hooks run')
  if (p.disableAllHooks) r.push('All hooks disabled')
  if (p.allowManagedMcpServersOnly) r.push('Only managed MCP servers')
  if (p.strictPluginOnlyCustomization) r.push('Customizations only via plugins')
  if (p.allowManagedPermissionRulesOnly) r.push('Only managed permission rules')
  if (p.bypassPermissionsDisabled) r.push('Bypass-permissions mode disabled')
  if (p.managedTelemetry) r.push('Organization telemetry configured')
  return r
}

async function reveal(path: string) {
  await app.attempt(() => revealItemInDir(path), 'Could not open the folder')
}

async function archive(p: ClaudeProfile) {
  const ok = await app.confirm({
    title: `Archive “${p.name}”?`,
    message: 'The profile is hidden from launch choices. Its configuration folder, credentials and history are not touched.',
    confirmLabel: 'Archive',
  })
  if (ok) await app.attempt(() => profiles.archive(p.id), 'Could not archive the profile')
}
</script>

<template>
  <div class="page">
    <div class="page-header">
      <div>
        <h1>Claude profiles</h1>
        <p>Each profile is a separate Claude Code configuration — its own account, settings, plugins and history.</p>
      </div>
      <div class="page-actions">
        <RouterLink :to="{ name: 'claude-setup' }" class="btn btn-ghost" data-open-setup>Claude Code setup</RouterLink>
        <button class="btn btn-ghost btn-icon" title="Refresh" @click="refreshAll"><RefreshCw /></button>
        <button class="btn" @click="openDialog('register')"><FolderOpen /> Register existing</button>
        <button class="btn btn-primary" @click="openDialog('create')"><Plus /> New profile</button>
      </div>
    </div>

    <div v-if="profiles.unregistered.length" class="card found">
      <h2 class="card-title"><ScanSearch /> Found on this machine</h2>
      <div v-for="d in profiles.unregistered" :key="d.path" class="found-row">
        <div class="found-main">
          <strong>{{ d.suggestedName }}</strong>
          <span class="mono faint">{{ shortPath(d.path, app.info?.homeDir, 60) }}</span>
          <div class="chips">
            <span class="chip">{{ d.source }}</span>
            <span v-for="m in d.markers" :key="m" class="chip">{{ m }}</span>
          </div>
        </div>
        <button class="btn btn-sm btn-primary" @click="openDialog('register', null, d)">Register</button>
      </div>
    </div>

    <div v-if="profiles.list.length" class="grid grid-2">
      <article v-for="p in profiles.list" :key="p.id" class="card profile">
        <header class="row">
          <span class="pdot" :style="{ background: p.color }" />
          <h3 class="truncate">{{ p.name }}</h3>
          <span v-if="!p.configDir" class="badge">default location</span>
          <span v-if="p.managedByNexus" class="badge badge-accent">created by NEXUS</span>
          <span class="spacer" />
          <button class="btn btn-ghost btn-icon btn-sm" title="Edit" @click="openDialog('edit', p)"><Pencil /></button>
          <button class="btn btn-ghost btn-icon btn-sm" title="Archive" @click="archive(p)"><Archive /></button>
        </header>
        <p v-if="p.description" class="muted desc">{{ p.description }}</p>

        <dl class="facts">
          <dt>Config</dt>
          <dd class="row">
            <code class="truncate selectable" :title="profiles.details[p.id]?.value?.effectiveConfigDir">
              {{ shortPath(profiles.details[p.id]?.value?.effectiveConfigDir ?? p.configDir ?? '~/.claude', app.info?.homeDir, 46) }}
            </code>
            <button
              v-if="profiles.details[p.id]?.value?.configDirExists"
              class="btn btn-ghost btn-icon btn-sm"
              title="Open folder"
              @click="reveal(profiles.details[p.id]!.value!.effectiveConfigDir)"
            >
              <FolderOpen />
            </button>
          </dd>

          <dt>Sign-in</dt>
          <dd>
            <span v-if="profiles.auth[p.id]?.loading" class="skeleton" style="width: 160px; display: inline-block" />
            <span v-else-if="profiles.auth[p.id]?.error" class="row warn" :title="profiles.auth[p.id]?.error ?? ''">
              <CircleAlert class="ic" /> Unknown — {{ profiles.auth[p.id]?.error }}
            </span>
            <template v-else-if="profiles.auth[p.id]?.value">
              <span v-if="profiles.auth[p.id]!.value!.loggedIn" class="row ok">
                <CircleCheck class="ic" />
                <span class="truncate">
                  {{ profiles.auth[p.id]!.value!.email ?? 'Signed in' }}
                  <span class="faint">
                    · {{ profiles.auth[p.id]!.value!.authMethod }}
                    <template v-if="profiles.auth[p.id]!.value!.subscriptionType">· {{ profiles.auth[p.id]!.value!.subscriptionType }}</template>
                  </span>
                </span>
              </span>
              <span v-else class="row warn"><KeyRound class="ic" /> Not signed in — launch a session to log in</span>
            </template>
          </dd>

          <dt>Claude</dt>
          <dd>
            <template v-if="profiles.details[p.id]?.value?.executable">
              <span class="mono">{{ profiles.details[p.id]!.value!.executable!.version ?? '?' }}</span>
              <span class="faint"> · {{ p.executable ? 'custom executable' : 'auto-detected' }}</span>
            </template>
            <span v-else-if="profiles.details[p.id]?.value?.executableError" class="warn">{{ profiles.details[p.id]!.value!.executableError }}</span>
            <span v-else class="skeleton" style="width: 90px; display: inline-block" />
          </dd>

          <dt>Last used</dt>
          <dd class="faint">{{ relativeTime(p.lastUsedAt) }}</dd>
        </dl>

        <div v-if="profiles.details[p.id]?.value?.policy.organizationManaged" class="policy">
          <div class="row policy-title"><Building2 class="ic" /> Organization-managed</div>
          <ul>
            <li v-for="r in restrictions(profiles.details[p.id]!.value!.policy)" :key="r"><ShieldAlert class="ic" /> {{ r }}</li>
            <li v-if="restrictions(profiles.details[p.id]!.value!.policy).length === 0" class="faint">Managed settings apply; no restriction affects NEXUS.</li>
          </ul>
          <p class="faint small">
            Sources:
            {{ profiles.details[p.id]!.value!.policy.sources.map((s) => (s.kind === 'server-managed' ? 'server-managed (this account)' : 'machine-wide (all profiles)')).join(', ') }}.
            NEXUS never overrides managed settings.
          </p>
        </div>

        <footer class="row">
          <span class="faint small">{{ p.defaultWorkProfile ? `Default rigor: ${p.defaultWorkProfile}` : '' }}</span>
          <span class="spacer" />
          <button class="btn btn-sm" :data-diagnose="p.id" @click="router.push(`/profiles/${p.id}`)"><Stethoscope /> Diagnostics</button>
          <button class="btn btn-sm btn-primary" @click="app.openLaunch(null, p.id)">
            <Sparkles /> Launch with this profile
          </button>
        </footer>
      </article>
    </div>
    <div v-else-if="!profiles.unregistered.length" class="empty">
      <KeyRound class="empty-icon" />
      <h3>No Claude profiles yet</h3>
      <p>Register your existing Claude configuration (for example <code>~/.claude</code>) or create a separate one for another account or kit.</p>
      <div class="row">
        <button class="btn" @click="openDialog('register')">Register existing</button>
        <button class="btn btn-primary" @click="openDialog('create')">New profile</button>
      </div>
    </div>

    <h2 class="section-title">Claude Code installations</h2>
    <div class="list">
      <div v-for="i in profiles.installations ?? []" :key="i.path" class="list-row">
        <span class="mono">{{ i.version ?? 'unknown' }}</span>
        <code class="truncate faint selectable" :title="i.path">{{ i.path }}</code>
        <span class="spacer" />
        <span class="badge">{{ i.source }}</span>
        <span v-if="i.launchable" class="badge badge-ok">launchable</span>
        <span v-else class="badge badge-warn" :title="i.issue ?? ''">not launchable</span>
      </div>
      <div v-if="profiles.installationsLoading" class="list-row"><span class="skeleton" style="width: 40%" /></div>
      <div v-else-if="(profiles.installations ?? []).length === 0" class="list-row muted">No Claude Code executable found.</div>
    </div>

    <ProfileDialog
      :open="dialog.open"
      :mode="dialog.mode"
      :profile="dialog.profile"
      :detected="dialog.detected"
      @close="(dialog.open = false), refreshAll()"
    />
  </div>
</template>

<style scoped>
.found {
  margin-bottom: 18px;
  border-color: var(--accent-ring);
}
.found-row {
  display: flex;
  align-items: center;
  gap: 16px;
  padding: 10px 0;
  border-top: 1px solid var(--border);
}
.found-row:first-of-type {
  border-top: none;
}
.found-main {
  flex: 1;
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 6px 12px;
  min-width: 0;
}
.profile {
  display: flex;
  flex-direction: column;
  gap: 12px;
}
.profile header h3 {
  margin: 0;
  font-size: 16px;
}
.pdot {
  width: 11px;
  height: 11px;
  border-radius: 50%;
  flex: none;
}
.desc {
  margin: -4px 0 0;
  font-size: 13px;
}
.facts {
  display: grid;
  grid-template-columns: 78px 1fr;
  gap: 8px 12px;
  margin: 0;
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
.ic {
  width: 14px;
  height: 14px;
  flex: none;
}
.ok {
  color: var(--green);
  min-width: 0;
}
.ok .truncate {
  color: var(--text);
}
.warn {
  color: var(--amber);
}
.policy {
  padding: 12px;
  border-radius: 10px;
  background: var(--amber-soft);
  font-size: 13px;
}
.policy-title {
  font-weight: 650;
  color: var(--amber);
  margin-bottom: 6px;
}
.policy ul {
  list-style: none;
  margin: 0;
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.policy li {
  display: flex;
  gap: 8px;
  align-items: center;
}
.small {
  font-size: 12px;
  margin: 8px 0 0;
}
.profile footer {
  margin-top: auto;
}
</style>
