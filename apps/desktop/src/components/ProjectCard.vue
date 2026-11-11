<script setup lang="ts">
import { computed, onMounted } from 'vue'
import { useRouter } from 'vue-router'
import { FolderSearch, Sparkles } from '@lucide/vue'
import type { Project } from '../lib/types'
import { relativeTime, shortPath } from '../lib/format'
import { useAppStore } from '../stores/app'
import { useProjectStore } from '../stores/projects'
import GitSummary from './GitSummary.vue'

const props = defineProps<{ project: Project }>()
const emit = defineEmits<{ locate: [project: Project] }>()
const app = useAppStore()
const projects = useProjectStore()
const router = useRouter()

const git = computed(() => projects.git[props.project.id])
onMounted(() => {
  if (props.project.available) projects.gitStatus(props.project.id)
})
</script>

<template>
  <article class="card project" :class="{ missing: !project.available }" :data-project-card="project.id" tabindex="0" @click="router.push(`/projects/${project.id}`)" @keydown.enter="router.push(`/projects/${project.id}`)">
    <div class="head">
      <div class="avatar" aria-hidden="true">{{ project.name.slice(0, 1).toUpperCase() }}</div>
      <div class="titles">
        <h3 class="truncate">{{ project.name }}</h3>
        <p class="truncate faint" :title="project.localPath ?? ''">
          {{ project.localPath ? shortPath(project.localPath, app.info?.homeDir, 44) : 'Not on this machine' }}
        </p>
      </div>
      <button
        v-if="project.available"
        class="btn btn-sm btn-ghost btn-icon launch"
        title="Launch a session here"
        @click.stop="app.openLaunch(project.id)"
      >
        <Sparkles />
      </button>
    </div>
    <div class="chips">
      <span v-for="s in project.stack.stacks.slice(0, 4)" :key="s" class="chip">{{ s }}</span>
      <span v-if="project.stack.stacks.length === 0" class="chip faint">no stack detected</span>
    </div>
    <div class="foot">
      <GitSummary v-if="project.available" :status="git?.status ?? null" :loading="git?.loading" :error="git?.error" compact />
      <button v-else class="btn btn-sm" @click.stop="emit('locate', project)"><FolderSearch /> Locate…</button>
      <span class="spacer" />
      <span class="faint opened">{{ relativeTime(project.lastOpenedAt ?? project.createdAt) }}</span>
    </div>
  </article>
</template>

<style scoped>
.project {
  display: flex;
  flex-direction: column;
  gap: 14px;
  cursor: pointer;
  transition: border-color 0.12s, transform 0.12s;
}
.project:hover {
  border-color: var(--border-strong);
  transform: translateY(-1px);
}
.project.missing {
  opacity: 0.75;
}
.head {
  display: flex;
  align-items: center;
  gap: 12px;
  min-width: 0;
}
.avatar {
  width: 38px;
  height: 38px;
  border-radius: 10px;
  display: flex;
  align-items: center;
  justify-content: center;
  font-weight: 800;
  color: #fff;
  background: linear-gradient(135deg, var(--accent), var(--cyan));
  flex: none;
}
.titles {
  flex: 1;
  min-width: 0;
}
h3 {
  margin: 0;
  font-size: 15px;
}
.titles p {
  margin: 2px 0 0;
  font-size: 12px;
}
.launch {
  color: var(--accent-strong);
}
.foot {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
}
.opened {
  font-size: 12px;
  flex: none;
}
</style>
