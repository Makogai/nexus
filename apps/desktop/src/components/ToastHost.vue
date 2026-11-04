<script setup lang="ts">
import { CircleAlert, CircleCheck, Info, X } from '@lucide/vue'
import { useAppStore } from '../stores/app'

const app = useAppStore()
</script>

<template>
  <div class="toasts" role="status" aria-live="polite">
    <TransitionGroup name="toast">
      <div v-for="t in app.toasts" :key="t.id" class="toast" :class="t.tone">
        <CircleAlert v-if="t.tone === 'error'" />
        <CircleCheck v-else-if="t.tone === 'success'" />
        <Info v-else />
        <span class="selectable">{{ t.message }}</span>
        <button class="btn btn-ghost btn-icon btn-sm" aria-label="Dismiss" @click="app.dismiss(t.id)"><X /></button>
      </div>
    </TransitionGroup>
  </div>
</template>

<style scoped>
.toasts {
  position: fixed;
  right: 18px;
  bottom: 18px;
  z-index: 60;
  display: flex;
  flex-direction: column;
  gap: 8px;
  width: 380px;
}
.toast {
  display: flex;
  align-items: flex-start;
  gap: 10px;
  padding: 10px 8px 10px 14px;
  border-radius: 12px;
  background: var(--bg-raised);
  border: 1px solid var(--border-strong);
  box-shadow: var(--shadow);
  font-size: 13px;
}
.toast > svg {
  width: 16px;
  height: 16px;
  margin-top: 2px;
  flex: none;
}
.toast span {
  flex: 1;
  word-break: break-word;
}
.toast.error > svg {
  color: var(--red);
}
.toast.success > svg {
  color: var(--green);
}
.toast.info > svg {
  color: var(--accent-strong);
}
.toast-enter-active,
.toast-leave-active {
  transition: all 0.18s;
}
.toast-enter-from,
.toast-leave-to {
  opacity: 0;
  transform: translateY(6px);
}
</style>
