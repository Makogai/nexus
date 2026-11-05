<script setup lang="ts">
// A constitution as people read it (W2): preamble, articles, sections and rule cards — id, enforcement, the gate that
// verifies it — instead of raw Markdown with rule markers.
import { ShieldCheck } from '@lucide/vue'
import type { ConstitutionStructure, Enforcement } from '../lib/types'
import MarkdownView from './MarkdownView.vue'

defineProps<{ structure: ConstitutionStructure }>()
const LABEL: Record<Enforcement, string> = { mandatory: 'Mandatory', rule: 'Rule', guideline: 'Guideline' }
</script>

<template>
  <div class="c-rules" data-constitution-rules>
    <MarkdownView v-if="structure.preamble.trim()" class="preamble" :source="structure.preamble" />
    <section v-for="(a, ai) in structure.articles" :key="ai" class="article">
      <h3 v-if="a.title" class="article-title">{{ a.title }}</h3>
      <MarkdownView v-if="a.text.trim()" :source="a.text" />
      <div v-for="(s, si) in a.sections" :key="si" class="section">
        <h4 v-if="s.title" class="section-title">{{ s.title }}</h4>
        <MarkdownView v-if="s.text.trim()" :source="s.text" />
        <article v-for="r in s.rules" :key="r.id" class="rule" :class="[r.enforcement, { disabled: !r.enabled }]" :data-rule="r.id">
          <header>
            <span class="rid mono">{{ r.id }}</span>
            <span class="enf" :class="r.enforcement"><ShieldCheck v-if="r.enforcement === 'mandatory'" />{{ LABEL[r.enforcement] }}</span>
            <span v-if="r.check" class="gate" :title="`Verified by the ${r.check} gate`">checked by {{ r.check }}</span>
            <span v-if="r.overrides" class="gate">overrides {{ r.overrides }}</span>
            <span v-if="!r.enabled" class="gate">disabled</span>
          </header>
          <div class="rule-title">{{ r.title }}</div>
          <MarkdownView class="rule-text" :source="r.text" />
        </article>
      </div>
    </section>
  </div>
</template>

<style scoped>
.c-rules {
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.preamble {
  color: var(--text-muted);
  margin-bottom: 6px;
}
.article + .article {
  margin-top: 12px;
}
.article-title {
  margin: 4px 0 8px;
  font-size: 15px;
  letter-spacing: -0.01em;
}
.section {
  margin: 6px 0 4px;
}
.section-title {
  margin: 10px 0 8px;
  font-size: 12px;
  font-weight: 600;
  color: var(--text-faint);
  letter-spacing: 0.02em;
}
.rule {
  padding: 10px 12px 8px;
  margin: 0 0 8px;
  border: 1px solid var(--border);
  border-left: 3px solid var(--border-strong);
  border-radius: var(--radius-sm);
  background: var(--bg-input);
}
.rule.mandatory {
  border-left-color: var(--red);
}
.rule.rule {
  border-left-color: var(--accent);
}
.rule.guideline {
  border-left-color: var(--border-strong);
}
.rule.disabled {
  opacity: 0.55;
}
.rule header {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  align-items: center;
  margin-bottom: 4px;
}
.rid {
  font-size: 11.5px;
  color: var(--text-muted);
}
.enf,
.gate {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  font-size: 11px;
  padding: 1px 7px;
  border-radius: 999px;
  border: 1px solid var(--border);
  color: var(--text-muted);
}
.enf svg {
  width: 11px;
  height: 11px;
}
.enf.mandatory {
  color: var(--red);
  border-color: color-mix(in srgb, var(--red) 35%, transparent);
  background: var(--red-soft);
}
.enf.rule {
  color: var(--accent-strong);
  border-color: color-mix(in srgb, var(--accent) 35%, transparent);
  background: var(--accent-soft);
}
.rule-title {
  font-weight: 600;
  font-size: 13.5px;
  margin-bottom: 2px;
}
.rule-text {
  font-size: 13px;
  color: var(--text-muted);
}
.rule-text :deep(p:last-child) {
  margin-bottom: 0;
}
</style>
