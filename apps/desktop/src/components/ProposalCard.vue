<script setup lang="ts">
import { ref } from 'vue'
import { Check, X } from '@lucide/vue'
import { relativeTime } from '../lib/format'
import type { MemoryApproval, MemoryProposal } from '../lib/types'

// A memory proposal (ADR-0022): Claude's recorded checkpoint turned into a candidate note/decision. It is not
// knowledge until a person approves it — edits are applied on approval.
const props = defineProps<{ proposal: MemoryProposal; busy?: boolean }>()
const emit = defineEmits<{ approve: [approval: MemoryApproval]; reject: [] }>()

const title = ref(props.proposal.title)
const body = ref(props.proposal.body)
const scope = ref<'workspace' | 'project' | 'branch'>(props.proposal.projectId ? 'project' : 'workspace')
const localOnly = ref(false)

function approve() {
  emit('approve', { title: title.value.trim(), body: body.value, scope: scope.value, localOnly: localOnly.value })
}
</script>

<template>
  <article class="proposal" :data-proposal="proposal.id">
    <header>
      <span class="badge" :class="proposal.kind === 'decision' ? 'badge-accent' : ''">{{ proposal.kind }}</span>
      <span class="faint">from “{{ proposal.sessionTitle }}”<template v-if="proposal.branch"> on {{ proposal.branch }}</template> · {{ relativeTime(proposal.createdAt) }} · recorded by Claude as a {{ proposal.source }}</span>
    </header>
    <input v-model="title" class="input title" aria-label="Title" maxlength="200" />
    <textarea v-model="body" class="textarea" rows="3" aria-label="Content" />
    <footer>
      <select v-model="scope" class="select scope" aria-label="Scope">
        <option value="workspace">Whole workspace</option>
        <option v-if="proposal.projectId" value="project">This project</option>
        <option v-if="proposal.projectId && proposal.branch" value="branch">Branch {{ proposal.branch }}</option>
      </select>
      <label class="checkbox"><input v-model="localOnly" type="checkbox" /><span>This machine only</span></label>
      <span class="spacer" />
      <button class="btn btn-sm btn-ghost" :disabled="busy" data-reject @click="emit('reject')"><X /> Reject</button>
      <button class="btn btn-sm btn-primary" :disabled="busy || !title.trim()" data-approve @click="approve"><Check /> Approve as {{ proposal.kind }}</button>
    </footer>
  </article>
</template>

<style scoped>
.proposal {
  border: 1px solid var(--border);
  border-radius: 10px;
  padding: 12px;
  display: flex;
  flex-direction: column;
  gap: 8px;
}
header {
  display: flex;
  gap: 8px;
  align-items: center;
  font-size: 12.5px;
}
.title {
  font-weight: 600;
}
footer {
  display: flex;
  gap: 10px;
  align-items: center;
}
.scope {
  width: auto;
  height: 30px;
}
</style>
