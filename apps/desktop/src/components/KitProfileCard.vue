<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { ShieldAlert, TriangleAlert } from '@lucide/vue'
import { defaultSelection, effectiveTokens, listingTokens, selectedSkillCount } from '../lib/kits'
import type { ClaudeProfile, KitProfileRow, KitSelection, KitView, PolicySummary } from '../lib/types'

// One profile × one kit: whether its NEXUS sessions get the kit, and exactly which parts (ADR-0024).
const props = defineProps<{ kit: KitView; profile: ClaudeProfile; row: KitProfileRow | null; policy: PolicySummary | null; busy?: boolean }>()
const emit = defineEmits<{ save: [selection: KitSelection]; disable: [] }>()

const inv = computed(() => props.kit.inventory)
const sel = ref<KitSelection>(props.row ? { ...props.row } : defaultSelection(props.kit.inventory))
watch(
  () => props.row,
  (r) => (sel.value = r ? { skills: r.skills, agents: r.agents, commands: r.commands, hooks: r.hooks, mcp: r.mcp } : defaultSelection(inv.value)),
)
const allSkills = computed(() => sel.value.skills === null)
const isChecked = (name: string) => sel.value.skills === null || sel.value.skills.includes(name)
function toggleSkill(name: string, on: boolean) {
  const current = sel.value.skills === null ? inv.value.skills.map((s) => s.name) : [...sel.value.skills]
  const next = on ? [...new Set([...current, name])] : current.filter((n) => n !== name)
  sel.value = { ...sel.value, skills: next.length === inv.value.skills.length ? null : next }
}
const tokens = computed(() => effectiveTokens(inv.value, sel.value))
const fullTokens = computed(() => listingTokens([...inv.value.skills, ...inv.value.agents, ...inv.value.commands]))
const blocked = computed(() =>
  props.policy?.disableSideloadFlags
    ? 'Organisation policy disables --plugin-dir for this profile (disableSideloadFlags).'
    : props.policy?.strictPluginOnlyCustomization
      ? 'Organisation policy allows only approved plugins (strictPluginOnlyCustomization).'
      : null,
)
</script>

<template>
  <article class="kit-profile" :data-profile="profile.id" :class="{ on: !!row }">
    <header>
      <span class="pdot" :style="{ background: profile.color }" />
      <strong>{{ profile.name }}</strong>
      <span v-if="row" class="badge badge-accent">enabled</span>
      <span class="spacer" />
      <span class="faint small">~{{ tokens.toLocaleString() }} tokens per session<span v-if="fullTokens > tokens"> (of {{ fullTokens.toLocaleString() }})</span></span>
    </header>
    <div v-if="blocked" class="notice notice-warn"><ShieldAlert /> {{ blocked }}</div>

    <div v-if="inv.skills.length" class="skills">
      <div class="label">
        Skills — {{ selectedSkillCount(inv, sel) }} of {{ inv.skills.length }}
        <button class="link" @click="sel = { ...sel, skills: null }">all</button> ·
        <button class="link" @click="sel = { ...sel, skills: [] }">none</button>
      </div>
      <label v-for="s in inv.skills" :key="s.name" class="checkbox skill">
        <input type="checkbox" :checked="isChecked(s.name)" :data-skill="s.name" @change="toggleSkill(s.name, ($event.target as HTMLInputElement).checked)" />
        <span><code>{{ inv.name }}:{{ s.name }}</code> <span class="faint">{{ s.description }}</span></span>
      </label>
      <p v-if="!allSkills && selectedSkillCount(inv, sel) === 0" class="hint">No skills selected — Claude's context stays free of this kit's skill listings.</p>
    </div>
    <div class="toggles">
      <label v-if="inv.agents.length" class="checkbox"><input v-model="sel.agents" type="checkbox" /><span>{{ inv.agents.length }} agent(s)</span></label>
      <label v-if="inv.commands.length" class="checkbox"><input v-model="sel.commands" type="checkbox" /><span>{{ inv.commands.length }} command(s)</span></label>
      <label v-if="inv.hooks.length" class="checkbox danger"><input v-model="sel.hooks" type="checkbox" data-hooks /><span>Hooks — run {{ inv.hooks.flatMap((h) => h.programs).join(', ') || 'commands' }}</span></label>
      <label v-if="inv.mcpServers.length" class="checkbox danger"><input v-model="sel.mcp" type="checkbox" data-mcp /><span>MCP servers — start {{ inv.mcpServers.map((m) => m.target || m.name).join(', ') }}</span></label>
    </div>
    <p v-if="sel.hooks || sel.mcp" class="notice notice-warn small"><TriangleAlert /> These run programs from the kit with your permissions in every session of {{ profile.name }}.</p>
    <footer>
      <button v-if="row" class="btn btn-sm btn-ghost" :disabled="busy" data-disable @click="emit('disable')">Disable</button>
      <span class="spacer" />
      <button class="btn btn-sm btn-primary" :disabled="busy" data-save @click="emit('save', sel)">{{ row ? 'Save selection' : `Enable for ${profile.name}` }}</button>
    </footer>
  </article>
</template>

<style scoped>
.kit-profile {
  border: 1px solid var(--border);
  border-radius: 10px;
  padding: 12px;
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.kit-profile.on {
  border-color: var(--accent-ring);
}
header,
footer {
  display: flex;
  align-items: center;
  gap: 8px;
}
.pdot {
  width: 10px;
  height: 10px;
  border-radius: 50%;
}
.skills {
  max-height: 240px;
  overflow: auto;
}
.skill {
  display: flex;
  margin: 3px 0;
  font-size: 13px;
}
.toggles {
  display: flex;
  flex-wrap: wrap;
  gap: 14px;
  font-size: 13px;
}
.danger span {
  color: var(--warn, #d9a441);
}
.small {
  font-size: 12px;
}
.link {
  background: none;
  border: none;
  color: var(--accent-strong);
  cursor: pointer;
  padding: 0;
}
</style>
