<script setup lang="ts">
import { computed, onActivated, ref, watch } from 'vue'
import { useRouter } from 'vue-router'
import { ArrowLeft, Bot, Building2, Gauge, Package, Puzzle, RefreshCw, Server, Settings2, Stethoscope, TriangleAlert, Webhook } from '@lucide/vue'
import { api, toIpcError } from '../lib/ipc'
import { shortPath } from '../lib/format'
import type { Diagnostics } from '../lib/types'
import { useAppStore } from '../stores/app'

// Profile diagnostics (ADR-0023): read-only, allowlisted view of what a Claude profile's configuration contains.
const props = defineProps<{ id: string }>()
const app = useAppStore()
const router = useRouter()
const d = ref<Diagnostics | null>(null)
const error = ref<string | null>(null)
const loading = ref(false)

async function load() {
  loading.value = true
  try {
    d.value = await api.profileDiagnostics(props.id)
    error.value = null
  } catch (e) {
    error.value = toIpcError(e).message
  } finally {
    loading.value = false
  }
}
watch(() => props.id, load, { immediate: true })
onActivated(load)

const bySource = computed(() => {
  const groups: Record<string, Diagnostics['skills']> = { user: [], plugin: [], kit: [] }
  for (const s of d.value?.skills ?? []) groups[s.source]!.push(s)
  return groups
})
const SOURCE_LABEL: Record<string, string> = { user: 'Your skills', plugin: 'From installed plugins', kit: 'From NEXUS kits (this profile only)' }
const contextBars = computed(() => {
  const c = d.value?.context
  if (!c || !c.totalTokens) return []
  return [
    ['Skills', c.skillsTokens],
    ['Agents', c.agentsTokens],
    ['Commands', c.commandsTokens],
    ['User memory', c.memoryTokens],
  ].map(([label, n]) => ({ label: label as string, n: n as number, pct: Math.round(((n as number) / c.totalTokens) * 100) }))
})
</script>

<template>
  <div class="page">
    <button class="btn btn-ghost btn-sm back" @click="router.push('/profiles')"><ArrowLeft /> Claude profiles</button>
    <div class="page-header">
      <div>
        <h1><Stethoscope class="title-ic" /> {{ d?.profileName ?? 'Profile' }} — diagnostics</h1>
        <p>What this profile's configuration really contains. Read-only; secret values are never read out.</p>
      </div>
      <div class="page-actions">
        <button class="btn btn-ghost" :disabled="loading" @click="load"><RefreshCw :class="{ spin: loading }" /> Refresh</button>
      </div>
    </div>
    <div v-if="error" class="notice notice-error selectable">{{ error }}</div>

    <template v-if="d">
      <div v-for="w in d.warnings" :key="w" class="notice notice-warn" data-warning><TriangleAlert /> {{ w }}</div>

      <div class="grid grid-2 top">
        <section class="card" data-section="overview">
          <h2 class="card-title"><Settings2 /> Profile</h2>
          <dl class="facts">
            <dt>Configuration</dt>
            <dd class="mono selectable">{{ shortPath(d.configDir, app.info?.homeDir, 60) }}<span v-if="d.defaultLocation" class="faint"> · default location</span><span v-if="!d.configExists" class="faint"> · not created yet</span></dd>
            <dt>Claude Code</dt>
            <dd>{{ d.version ?? 'unknown version' }} <span class="faint mono">{{ d.executable ? shortPath(d.executable, app.info?.homeDir, 50) : 'not found' }}</span></dd>
            <dt>Model</dt>
            <dd>{{ d.settings.model ?? 'Claude default' }}</dd>
            <dt>Permissions</dt>
            <dd>mode {{ d.settings.defaultPermissionMode ?? 'default' }} · {{ d.settings.permissionRules[0] }} allow · {{ d.settings.permissionRules[1] }} ask · {{ d.settings.permissionRules[2] }} deny</dd>
            <dt>Isolation</dt>
            <dd>{{ d.isolateInstructions ? 'default profile instructions excluded from NEXUS sessions' : 'off' }}</dd>
            <template v-if="d.settings.envNames.length">
              <dt>Environment</dt>
              <dd>
                <span v-for="n in d.settings.envNames" :key="n" class="chip mono" :class="{ sensitive: d.settings.sensitiveEnvNames.includes(n) }">{{ n }}</span>
                <span class="faint small block">names only — values are never shown</span>
              </dd>
            </template>
            <template v-if="d.settings.apiKeyHelper || d.settings.cloudCredentialHelper">
              <dt>Credentials</dt>
              <dd>{{ d.settings.apiKeyHelper ? 'apiKeyHelper configured' : '' }} {{ d.settings.cloudCredentialHelper ? 'cloud credential helper configured' : '' }} <span class="faint">(commands not shown)</span></dd>
            </template>
          </dl>
        </section>

        <section class="card" data-section="policy">
          <h2 class="card-title"><Building2 /> Organisation policy</h2>
          <p v-if="!d.policy.organizationManaged" class="faint">No managed settings apply to this profile.</p>
          <template v-else>
            <ul class="effects">
              <li v-for="e in d.policyEffects" :key="e">{{ e }}</li>
              <li v-if="!d.policyEffects.length" class="faint">Managed settings apply; none restricts what NEXUS adds.</li>
            </ul>
            <p class="faint small">Managed settings take precedence over everything NEXUS passes; NEXUS never overrides them.</p>
          </template>
        </section>
      </div>

      <section class="card context" data-section="context">
        <h2 class="card-title"><Gauge /> Context at every session start <span class="faint">≈ {{ d.context.totalTokens.toLocaleString() }} tokens (estimate)</span></h2>
        <div v-if="contextBars.length" class="bars">
          <div v-for="b in contextBars" :key="b.label" class="bar-row">
            <span class="bar-label">{{ b.label }}</span>
            <span class="bar"><span class="fill" :style="{ width: `${b.pct}%` }" /></span>
            <span class="faint bar-n">{{ b.n.toLocaleString() }}</span>
          </div>
        </div>
        <p class="faint small">Skill, agent and command listings plus your user memory. Built-in tools and project files come on top.</p>
      </section>

      <section class="card" data-section="skills">
        <h2 class="card-title"><Puzzle /> Skills <span class="faint">{{ d.skills.length }}</span></h2>
        <template v-for="(list, src) in bySource" :key="src">
          <div v-if="list.length" class="group">
            <div class="label">{{ SOURCE_LABEL[src] }}</div>
            <div v-for="s in list" :key="s.name" class="row-item">
              <code>{{ s.name }}</code><span class="faint grow">{{ s.description }}</span><span class="faint small">~{{ Math.ceil(s.listingChars / 4) }} tok</span>
            </div>
          </div>
        </template>
        <p v-if="!d.skills.length" class="faint">No skills besides Claude's built-ins.</p>
      </section>

      <div class="grid grid-2">
        <section class="card" data-section="agents">
          <h2 class="card-title"><Bot /> Agents <span class="faint">{{ d.agents.length }}</span> · commands <span class="faint">{{ d.commands.length }}</span></h2>
          <div v-for="a in d.agents" :key="a.name" class="row-item"><code>{{ a.name }}</code><span class="faint grow">{{ a.description }}</span></div>
          <div v-for="c in d.commands" :key="c.name" class="row-item"><code>/{{ c.name }}</code><span class="faint grow">{{ c.description }}</span></div>
          <p v-if="!d.agents.length && !d.commands.length" class="faint">None besides Claude's built-ins.</p>
        </section>

        <section class="card" data-section="mcp">
          <h2 class="card-title"><Server /> MCP servers <span class="faint">{{ d.mcpServers.length }}</span></h2>
          <div v-for="m in d.mcpServers" :key="m.scope + m.qualifiedName" class="row-item">
            <span class="badge">{{ m.scope }}</span><code>{{ m.qualifiedName }}</code>
            <span class="faint grow">{{ m.transport }} · {{ m.target }}<template v-if="m.origin"> · {{ m.origin }}</template></span>
          </div>
          <p v-if="!d.mcpServers.length" class="faint">None configured.</p>
          <p class="faint small">Arguments, environment values and headers are never shown.</p>
        </section>
      </div>

      <div class="grid grid-2">
        <section class="card" data-section="hooks">
          <h2 class="card-title"><Webhook /> Hooks <span class="faint">{{ d.hooks.reduce((n, h) => n + h.handlers, 0) }}</span></h2>
          <div v-for="h in d.hooks" :key="h.source + h.event + (h.origin ?? '')" class="row-item">
            <span class="badge">{{ h.source }}</span><code>{{ h.event }}</code>
            <span class="faint grow">{{ h.handlers }} × {{ h.types.join(', ') }}<template v-if="h.programs.length"> · runs {{ h.programs.join(', ') }}</template><template v-if="h.origin"> · {{ h.origin }}</template></span>
          </div>
          <p v-if="!d.hooks.length" class="faint">No hooks in this profile's settings or plugins. NEXUS adds its own lifecycle hooks per session.</p>
        </section>

        <section class="card" data-section="plugins">
          <h2 class="card-title"><Package /> Plugins and kits</h2>
          <div v-for="p in d.plugins" :key="p.id" class="row-item">
            <span class="badge" :class="p.enabled && p.installed ? 'badge-ok' : ''">{{ !p.installed ? 'not installed' : p.enabled ? 'enabled' : 'disabled' }}</span>
            <code>{{ p.id }}</code>
            <span class="faint grow">{{ p.version }} · {{ p.skills }} skills · ~{{ p.listingTokens }} tok<template v-if="p.runsCode"> · runs code</template></span>
          </div>
          <div v-for="k in d.kits" :key="k.kitId" class="row-item" data-kit>
            <span class="badge" :class="k.blocked ? 'badge-warn' : 'badge-accent'">{{ k.blocked ? 'not used' : 'NEXUS kit' }}</span>
            <code>{{ k.inventory.name }}</code>
            <span class="faint grow">{{ k.blocked ?? `${k.inventory.skills.length} skill(s) in NEXUS sessions of this profile` }}</span>
          </div>
          <p v-if="!d.plugins.length && !d.kits.length" class="faint">No plugins installed and no NEXUS kits enabled.</p>
          <div v-for="m in d.marketplaces" :key="m.name" class="faint small">marketplace {{ m.name }} ({{ m.kind }}) {{ m.location }}</div>
          <RouterLink to="/kits" class="small">Manage AI Kits →</RouterLink>
        </section>
      </div>
    </template>
    <p v-else-if="loading" class="faint">Inspecting the profile…</p>
  </div>
</template>

<style scoped>
.back {
  margin: -8px 0 10px -8px;
}
.title-ic {
  width: 22px;
  height: 22px;
  vertical-align: -3px;
}
.top,
.grid {
  margin-top: 14px;
}
.context,
[data-section='skills'] {
  margin-top: 14px;
}
.facts {
  display: grid;
  grid-template-columns: 120px 1fr;
  gap: 6px 14px;
  margin: 0;
  font-size: 13px;
}
dt {
  color: var(--text-muted);
}
dd {
  margin: 0;
}
.chip.sensitive {
  border-color: var(--warn, #d9a441);
}
.block {
  display: block;
}
.small {
  font-size: 12px;
}
.effects {
  margin: 0;
  padding-left: 18px;
  font-size: 13px;
}
.bars {
  display: flex;
  flex-direction: column;
  gap: 6px;
  margin: 6px 0;
}
.bar-row {
  display: grid;
  grid-template-columns: 110px 1fr 60px;
  gap: 10px;
  align-items: center;
  font-size: 13px;
}
.bar {
  height: 8px;
  border-radius: 4px;
  background: var(--bg-input);
  overflow: hidden;
}
.fill {
  display: block;
  height: 100%;
  background: var(--accent);
}
.bar-n {
  text-align: right;
}
.group {
  margin-bottom: 10px;
}
.row-item {
  display: flex;
  gap: 8px;
  align-items: baseline;
  padding: 4px 0;
  font-size: 13px;
  border-bottom: 1px solid var(--border);
}
.grow {
  flex: 1;
  min-width: 0;
}
</style>
