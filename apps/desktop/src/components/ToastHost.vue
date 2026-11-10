<script setup lang="ts">
import { ref } from 'vue'
import { CircleAlert, CircleCheck, Copy, Info, X } from '@lucide/vue'
import { useAppStore } from '../stores/app'

const app = useAppStore()
/** Error toasts stay until dismissed (ADR-0048); their text can be copied for a bug report or a message to Claude. */
const copied = ref<number | null>(null)
async function copy(id: number, message: string) {
  try {
    await navigator.clipboard.writeText(message)
    copied.value = id
    setTimeout(() => (copied.value = copied.value === id ? null : copied.value), 1500)
  } catch {
    // The clipboard can be unavailable (permissions); the text stays selectable.
  }
}
</script>

<template>
  <div class="toasts" role="status" aria-live="polite">
    <TransitionGroup name="toast">
      <div v-for="t in app.toasts" :key="t.id" class="toast" :class="t.tone">
        <CircleAlert v-if="t.tone === 'error'" />
        <CircleCheck v-else-if="t.tone === 'success'" />
        <Info v-else />
        <span class="selectable msg">{{ t.message }}<span v-if="t.count && t.count > 1" class="times"> ×{{ t.count }}</span></span>
        <button v-if="t.tone === 'error'" class="btn btn-ghost btn-icon btn-sm" :aria-label="copied === t.id ? 'Copied' : 'Copy details'" :title="copied === t.id ? 'Copied' : 'Copy details'" data-toast-copy @click="copy(t.id, t.message)"><Copy /></button>
        <button class="btn btn-ghost btn-icon btn-sm" aria-label="Dismiss" data-toast-dismiss @click="app.dismiss(t.id)"><X /></button>
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
.toast .msg {
  flex: 1;
  word-break: break-word;
  max-height: 160px;
  overflow: auto;
}
.toast .times {
  color: var(--text-faint);
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
