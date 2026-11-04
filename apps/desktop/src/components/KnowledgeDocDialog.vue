<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { FolderOpen, ShieldCheck, TriangleAlert } from '@lucide/vue'
import { revealItemInDir } from '@tauri-apps/plugin-opener'
import { api, toIpcError } from '../lib/ipc'
import type { KnowledgeDoc, KnowledgeItem } from '../lib/types'
import BaseModal from './BaseModal.vue'

const props = defineProps<{ item: KnowledgeItem | null; workspaceId: string }>()
const emit = defineEmits<{ close: []; approved: [] }>()

const doc = ref<KnowledgeDoc | null>(null)
const error = ref<string | null>(null)
const busy = ref(false)

watch(
  () => props.item,
  async (item) => {
    doc.value = null
    error.value = null
    if (!item?.id) return
    try {
      doc.value = await api.knowledgeRead(props.workspaceId, item.id)
    } catch (e) {
      error.value = toIpcError(e).message
    }
  },
  { immediate: true },
)

const hidden = new Set(['nexus', 'id', 'title', 'kind'])
const fields = computed(() =>
  Object.entries(doc.value?.fields ?? {})
    .filter(([k]) => !hidden.has(k))
    .map(([k, v]) => [k, Array.isArray(v) ? v.join(', ') : String(v ?? '')] as const),
)

async function approve() {
  if (!props.item?.id) return
  busy.value = true
  try {
    await api.knowledgeApprove(props.workspaceId, props.item.id)
    emit('approved')
    emit('close')
  } catch (e) {
    error.value = toIpcError(e).message
  } finally {
    busy.value = false
  }
}
</script>

<template>
  <BaseModal :open="!!item" :title="item?.title ?? ''" :subtitle="doc?.relativePath" :width="720" @close="emit('close')">
    <div v-if="item?.needsApproval" class="notice notice-warn">
      <TriangleAlert />
      <span>This always-on rule is not given to Claude until you approve this exact version on this machine. Read it first — workspace files can change through sync.</span>
    </div>
    <dl v-if="fields.length" class="fields">
      <template v-for="[k, v] in fields" :key="k">
        <dt>{{ k }}</dt>
        <dd class="selectable">{{ v }}</dd>
      </template>
    </dl>
    <pre v-if="doc" class="body selectable">{{ doc.body.trim() || '(empty)' }}</pre>
    <p v-if="doc?.truncated" class="hint">Showing the first 200,000 characters.</p>
    <div v-if="error" class="notice notice-error selectable"><TriangleAlert /> {{ error }}</div>
    <template #footer>
      <button v-if="item" class="btn btn-ghost" @click="revealItemInDir(item.path)"><FolderOpen /> Show file</button>
      <span class="spacer" />
      <button v-if="item?.needsApproval" class="btn btn-primary" :disabled="busy || !doc" @click="approve"><ShieldCheck /> Approve this version</button>
      <button class="btn" @click="emit('close')">Close</button>
    </template>
  </BaseModal>
</template>

<style scoped>
.fields {
  display: grid;
  grid-template-columns: 110px 1fr;
  gap: 6px 14px;
  margin: 0 0 14px;
  font-size: 13px;
}
dt {
  color: var(--text-muted);
}
dd {
  margin: 0;
}
.body {
  white-space: pre-wrap;
  word-break: break-word;
  font-family: var(--font-mono, ui-monospace, monospace);
  font-size: 13px;
  line-height: 1.55;
  background: var(--bg-raised);
  border: 1px solid var(--border);
  border-radius: 8px;
  padding: 12px 14px;
  max-height: 50vh;
  overflow: auto;
  margin: 0;
}
</style>
