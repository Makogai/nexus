<script setup lang="ts">
import { computed, ref } from 'vue'
import { FolderGit2, Plus, Search } from '@lucide/vue'
import { open as openDialog } from '@tauri-apps/plugin-dialog'
import type { Project } from '../lib/types'
import { fuzzyFilter } from '../lib/fuzzy'
import { useAppStore } from '../stores/app'
import { useProjectStore } from '../stores/projects'
import { useWorkspaceStore } from '../stores/workspaces'
import AddProjectDialog from '../components/AddProjectDialog.vue'
import ProjectCard from '../components/ProjectCard.vue'

const app = useAppStore()
const projects = useProjectStore()
const workspaces = useWorkspaceStore()
const query = ref('')
const adding = ref(false)

const filtered = computed(() =>
  fuzzyFilter(projects.list, query.value, (p) => `${p.name} ${p.stack.stacks.join(' ')} ${p.localPath ?? ''}`, 500),
)

async function locate(p: Project) {
  const dir = await openDialog({ directory: true, title: `Where is “${p.name}” on this machine?` })
  if (typeof dir === 'string') await app.attempt(() => projects.locate(p.id, dir), 'Could not locate the project')
}
</script>

<template>
  <div class="page">
    <div class="page-header">
      <div>
        <h1>Projects</h1>
        <p>Independent repositories registered in {{ workspaces.active?.name }}.</p>
      </div>
      <div class="page-actions">
        <div class="search">
          <Search />
          <input v-model="query" class="input" placeholder="Filter by name, stack or path" />
        </div>
        <button class="btn btn-primary" @click="adding = true"><Plus /> Add project</button>
      </div>
    </div>

    <div v-if="filtered.length" class="grid grid-3">
      <ProjectCard v-for="p in filtered" :key="p.id" :project="p" @locate="locate" />
    </div>
    <div v-else-if="projects.list.length" class="empty"><p>No project matches “{{ query }}”.</p></div>
    <div v-else class="empty">
      <FolderGit2 class="empty-icon" />
      <h3>Register your first project</h3>
      <p>Choose a local folder — usually a Git repository. NEXUS inspects it deterministically and never runs its scripts.</p>
      <button class="btn btn-primary" @click="adding = true"><Plus /> Add project</button>
    </div>

    <AddProjectDialog :open="adding" @close="adding = false" />
  </div>
</template>

<style scoped>
.search {
  position: relative;
  width: 300px;
}
.search svg {
  position: absolute;
  left: 10px;
  top: 10px;
  width: 15px;
  height: 15px;
  color: var(--text-faint);
}
.search .input {
  padding-left: 32px;
}
</style>
