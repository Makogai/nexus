<script setup lang="ts">
// Visual tree editor for one constitution: articles → sections → rules. Edits the structure in place; the parent saves.
import { computed } from 'vue'
import { ArrowDown, ArrowUp, Plus, Trash2 } from '@lucide/vue'
import type { ConstitutionProblem, ConstitutionStructure, Enforcement, RuleRef } from '../lib/types'
import { ENFORCEMENT_HINTS, ENFORCEMENT_LABELS, matches, move, newArticle, newRule, newSection } from '../lib/constitution'

const props = defineProps<{
  structure: ConstitutionStructure
  problems: ConstitutionProblem[]
  otherRules: RuleRef[]
  filter: string
  readonly?: boolean
}>()
const emit = defineEmits<{ change: [] }>()

const enforcements: Enforcement[] = ['mandatory', 'rule', 'guideline']
const problemsByRule = computed(() => {
  const m: Record<string, string[]> = {}
  for (const p of props.problems) if (p.ruleId) (m[p.ruleId] ??= []).push(p.message)
  return m
})
const overrideChoices = computed(() => props.otherRules.filter((r) => r.enforcement !== 'mandatory'))

function changed() {
  emit('change')
}
function visibleSection(rules: { id: string; title: string; text: string }[]) {
  return !props.filter.trim() || rules.some((r) => matches(r as never, props.filter))
}
</script>

<template>
  <div class="tree" :class="{ readonly }">
    <div class="field">
      <label>Preamble <span class="faint">(purpose of this constitution)</span></label>
      <textarea v-model="structure.preamble" class="textarea" rows="2" :readonly="readonly" @input="changed" />
    </div>

    <section v-for="(a, ai) in structure.articles" :key="ai" class="article" :data-article="ai">
      <header class="row-head">
        <span class="level">Article</span>
        <input v-model="a.title" class="input title-input" aria-label="Article title" :readonly="readonly" @input="changed" />
        <div v-if="!readonly" class="tools">
          <button class="btn btn-icon btn-sm btn-ghost" title="Move up" @click="move(structure.articles, ai, -1) && changed()"><ArrowUp /></button>
          <button class="btn btn-icon btn-sm btn-ghost" title="Move down" @click="move(structure.articles, ai, 1) && changed()"><ArrowDown /></button>
          <button class="btn btn-icon btn-sm btn-ghost" title="Delete article" @click="structure.articles.splice(ai, 1), changed()"><Trash2 /></button>
        </div>
      </header>
      <textarea v-model="a.text" class="textarea" rows="1" placeholder="Why this article exists (optional)" :readonly="readonly" @input="changed" />

      <div v-for="(s, si) in a.sections" v-show="visibleSection(s.rules)" :key="si" class="section">
        <header class="row-head">
          <span class="level">Section</span>
          <input v-model="s.title" class="input title-input" aria-label="Section title" placeholder="(rules directly under the article)" :readonly="readonly" @input="changed" />
          <div v-if="!readonly" class="tools">
            <button class="btn btn-icon btn-sm btn-ghost" title="Move up" @click="move(a.sections, si, -1) && changed()"><ArrowUp /></button>
            <button class="btn btn-icon btn-sm btn-ghost" title="Move down" @click="move(a.sections, si, 1) && changed()"><ArrowDown /></button>
            <button class="btn btn-icon btn-sm btn-ghost" title="Delete section" @click="a.sections.splice(si, 1), changed()"><Trash2 /></button>
          </div>
        </header>

        <div
          v-for="(r, ri) in s.rules"
          v-show="matches(r, filter)"
          :key="ri"
          class="rule"
          :class="[`e-${r.enforcement}`, { disabled: !r.enabled }]"
          :data-rule="r.id || 'new'"
        >
          <header class="row-head">
            <span class="rid mono" :title="r.id ? 'Stable rule id' : 'Assigned when saved'">{{ r.id || 'new' }}</span>
            <input v-model="r.title" class="input title-input" aria-label="Rule title" :readonly="readonly" @input="changed" />
            <select v-model="r.enforcement" class="select enf" :title="ENFORCEMENT_HINTS[r.enforcement]" aria-label="Enforcement" :disabled="readonly" @change="changed">
              <option v-for="e in enforcements" :key="e" :value="e">{{ ENFORCEMENT_LABELS[e] }}</option>
            </select>
            <div v-if="!readonly" class="tools">
              <button class="btn btn-icon btn-sm btn-ghost" title="Move up" @click="move(s.rules, ri, -1) && changed()"><ArrowUp /></button>
              <button class="btn btn-icon btn-sm btn-ghost" title="Move down" @click="move(s.rules, ri, 1) && changed()"><ArrowDown /></button>
              <button class="btn btn-icon btn-sm btn-ghost" title="Delete rule" @click="s.rules.splice(ri, 1), changed()"><Trash2 /></button>
            </div>
          </header>
          <textarea v-model="r.text" class="textarea" rows="2" placeholder="What Claude must / should do" aria-label="Rule text" :readonly="readonly" @input="changed" />
          <div class="rule-meta">
            <label class="checkbox"><input v-model="r.enabled" type="checkbox" :disabled="readonly" @change="changed" /><span>Enabled</span></label>
            <label class="check" title="A quality gate (Workflows → Gates) that verifies this rule. Without one the rule is an instruction only.">
              <span class="faint">Verified by gate</span>
              <input v-model="r.check" class="input mono" placeholder="none — instructed only" aria-label="Verified by gate" :readonly="readonly" @input="changed" />
            </label>
            <label class="override">
              <span class="faint">Overrides</span>
              <select v-model="r.overrides" class="select" :disabled="readonly" aria-label="Overrides" @change="changed">
                <option :value="null">—</option>
                <option v-for="o in overrideChoices" :key="o.docId + o.id" :value="o.id">{{ o.id }} · {{ o.title }} ({{ o.docTitle }})</option>
              </select>
            </label>
          </div>
          <p v-for="m in problemsByRule[r.id] ?? []" :key="m" class="problem">{{ m }}</p>
        </div>
        <button v-if="!readonly" class="btn btn-sm btn-ghost add" @click="s.rules.push(newRule()), changed()"><Plus /> Rule</button>
      </div>
      <button v-if="!readonly" class="btn btn-sm btn-ghost add" @click="a.sections.push(newSection()), changed()"><Plus /> Section</button>
    </section>
    <button v-if="!readonly" class="btn btn-sm add" data-add-article @click="structure.articles.push(newArticle()), changed()"><Plus /> Article</button>
  </div>
</template>

<style scoped>
.tree {
  display: flex;
  flex-direction: column;
  gap: 12px;
}
.article {
  border: 1px solid var(--border);
  border-radius: var(--radius);
  padding: 12px;
  background: var(--bg-panel);
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.section {
  margin-left: 14px;
  padding-left: 12px;
  border-left: 2px solid var(--border);
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.rule {
  border: 1px solid var(--border);
  border-left: 3px solid var(--accent);
  border-radius: var(--radius-sm);
  padding: 8px;
  display: flex;
  flex-direction: column;
  gap: 6px;
  background: var(--bg);
}
.rule.e-mandatory {
  border-left-color: var(--red);
}
.rule.e-guideline {
  border-left-color: var(--cyan);
}
.rule.disabled {
  opacity: 0.6;
}
.row-head {
  display: flex;
  align-items: center;
  gap: 8px;
}
.level {
  font-size: 11px;
  text-transform: uppercase;
  letter-spacing: 0.06em;
  color: var(--text-faint);
  width: 56px;
  flex: none;
}
.title-input {
  flex: 1;
  font-weight: 600;
}
.rid {
  flex: none;
  min-width: 56px;
  font-size: 12px;
  color: var(--accent-strong);
}
.enf {
  width: 170px;
  flex: none;
}
.tools {
  display: flex;
  gap: 2px;
  flex: none;
}
.rule-meta {
  display: flex;
  align-items: center;
  gap: 16px;
}
.override {
  display: flex;
  align-items: center;
  gap: 6px;
  flex: 1;
}
.override .select {
  max-width: 420px;
}
.check {
  display: flex;
  align-items: center;
  gap: 6px;
}
.check .input {
  width: 170px;
}
.problem {
  color: var(--red);
  font-size: 12px;
  margin: 0;
}
.add {
  align-self: flex-start;
}
.readonly .textarea,
.readonly .input {
  background: transparent;
}
</style>
