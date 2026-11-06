<script setup lang="ts">
import { nextTick, onBeforeUnmount, ref, watch } from 'vue'
import { X } from '@lucide/vue'
import { closeModal, isTopModal, openModal } from '../lib/modalStack'

const props = withDefaults(defineProps<{ open: boolean; title: string; subtitle?: string; width?: number }>(), {
  width: 560,
  subtitle: undefined,
})
const emit = defineEmits<{ close: [] }>()
const panel = ref<HTMLElement>()
/** Layer and place in the open-dialog stack: a dialog opened later (a confirmation) is always on top. */
const z = ref(50)
let modalId = 0

function onKey(e: KeyboardEvent) {
  // Only the topmost dialog closes on Escape — not the dialog under a confirmation.
  if (e.key === 'Escape' && isTopModal(modalId)) {
    e.stopPropagation()
    emit('close')
  }
}

function release() {
  window.removeEventListener('keydown', onKey, true)
  closeModal(modalId)
  modalId = 0
}

watch(
  () => props.open,
  async (open) => {
    if (open) {
      if (!modalId) {
        const m = openModal()
        modalId = m.id
        z.value = m.z
      }
      window.addEventListener('keydown', onKey, true)
      await nextTick()
      const first = panel.value?.querySelector<HTMLElement>('[autofocus], input, select, textarea, button.btn-primary')
      first?.focus()
    } else {
      release()
    }
  },
  { immediate: true },
)
onBeforeUnmount(release)
</script>

<template>
  <Teleport to="body">
    <Transition name="modal">
      <div v-if="open" class="backdrop" :style="{ zIndex: z }" @mousedown.self="emit('close')">
        <div ref="panel" class="panel" role="dialog" aria-modal="true" :aria-label="title" :style="{ width: `${width}px` }">
          <header>
            <div>
              <h2>{{ title }}</h2>
              <p v-if="subtitle" class="muted">{{ subtitle }}</p>
            </div>
            <button class="btn btn-ghost btn-icon btn-sm" aria-label="Close" @click="emit('close')"><X /></button>
          </header>
          <div class="body"><slot /></div>
          <footer v-if="$slots.footer"><slot name="footer" /></footer>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<style scoped>
.backdrop {
  position: fixed;
  inset: 0;
  z-index: 50;
  background: rgba(5, 6, 10, 0.55);
  backdrop-filter: blur(3px);
  display: flex;
  align-items: flex-start;
  justify-content: center;
  padding-top: 9vh;
}
.panel {
  max-width: calc(100vw - 48px);
  max-height: 82vh;
  display: flex;
  flex-direction: column;
  background: var(--bg-raised);
  border: 1px solid var(--border-strong);
  border-radius: 16px;
  box-shadow: var(--shadow);
}
header {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 12px;
  padding: 18px 20px 6px;
}
h2 {
  margin: 0;
  font-size: 17px;
}
header p {
  margin: 4px 0 0;
  font-size: 13px;
}
.body {
  padding: 14px 20px;
  overflow: auto;
}
footer {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  padding: 12px 20px 18px;
  border-top: 1px solid var(--border);
}
.modal-enter-active,
.modal-leave-active {
  transition: opacity 0.14s;
}
.modal-enter-active .panel,
.modal-leave-active .panel {
  transition: transform 0.14s;
}
.modal-enter-from,
.modal-leave-to {
  opacity: 0;
}
.modal-enter-from .panel {
  transform: translateY(8px) scale(0.98);
}
</style>
