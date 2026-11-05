<script setup lang="ts">
// Rendered Markdown (W2): safe by construction (lib/markdown.ts), calm typography from the design tokens.
import { computed } from 'vue'
import { openUrl } from '@tauri-apps/plugin-opener'
import { externalLink, renderInline, renderMarkdown } from '../lib/markdown'

const props = withDefaults(defineProps<{ source: string; inline?: boolean }>(), { inline: false })
const html = computed(() => (props.inline ? renderInline(props.source) : renderMarkdown(props.source)))

function onClick(e: MouseEvent) {
  const a = (e.target as HTMLElement | null)?.closest('a[data-md-link]') as HTMLAnchorElement | null
  if (!a) return
  e.preventDefault()
  const href = externalLink(a.getAttribute('href'))
  if (href) openUrl(href).catch(() => undefined)
}
</script>

<template>
  <!-- eslint-disable-next-line vue/no-v-html -- markdown-it with html:false escapes all raw HTML -->
  <span v-if="inline" class="md md-inline selectable" @click="onClick" v-html="html" />
  <!-- eslint-disable-next-line vue/no-v-html -- markdown-it with html:false escapes all raw HTML -->
  <div v-else class="md selectable" data-markdown @click="onClick" v-html="html" />
</template>

<style scoped>
.md {
  font-size: 14px;
  line-height: 1.65;
  color: var(--text);
  overflow-wrap: anywhere;
}
.md :deep(h1),
.md :deep(h2),
.md :deep(h3),
.md :deep(h4) {
  margin: 1.3em 0 0.5em;
  line-height: 1.3;
  letter-spacing: -0.01em;
  text-transform: none;
}
.md :deep(h1) {
  font-size: 20px;
}
.md :deep(h2) {
  font-size: 16px;
  padding-bottom: 6px;
  border-bottom: 1px solid var(--border);
}
.md :deep(h3) {
  font-size: 14.5px;
}
.md :deep(h4) {
  font-size: 13.5px;
  color: var(--text-muted);
}
.md :deep(> :first-child) {
  margin-top: 0;
}
.md :deep(p),
.md :deep(ul),
.md :deep(ol),
.md :deep(blockquote),
.md :deep(pre),
.md :deep(table) {
  margin: 0 0 0.85em;
}
.md :deep(ul),
.md :deep(ol) {
  padding-left: 1.4em;
}
.md :deep(li + li) {
  margin-top: 0.25em;
}
.md :deep(a) {
  color: var(--accent-strong);
  text-decoration: underline;
  text-underline-offset: 2px;
  cursor: pointer;
}
.md :deep(code) {
  font-family: var(--font-mono, ui-monospace, monospace);
  font-size: 0.88em;
  padding: 1px 5px;
  border-radius: 4px;
  background: var(--bg-input);
  border: 1px solid var(--border);
}
.md :deep(pre) {
  padding: 10px 12px;
  border-radius: var(--radius-sm);
  background: var(--bg-input);
  border: 1px solid var(--border);
  overflow: auto;
}
.md :deep(pre code) {
  padding: 0;
  border: 0;
  background: none;
  font-size: 12.5px;
}
.md :deep(blockquote) {
  padding: 2px 12px;
  border-left: 3px solid var(--border-strong);
  color: var(--text-muted);
}
.md :deep(table) {
  border-collapse: collapse;
  font-size: 13px;
}
.md :deep(th),
.md :deep(td) {
  border: 1px solid var(--border);
  padding: 5px 9px;
  text-align: left;
}
.md :deep(th) {
  background: var(--bg-input);
}
.md :deep(hr) {
  border: 0;
  border-top: 1px solid var(--border);
  margin: 1.2em 0;
}
.md :deep(.md-image) {
  color: var(--text-faint);
  font-style: italic;
}
.md-inline :deep(p) {
  margin: 0;
}
</style>
