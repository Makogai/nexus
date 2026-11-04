<script setup lang="ts">
import { computed } from 'vue'
import { useAppStore } from '../stores/app'
import BaseModal from './BaseModal.vue'

const app = useAppStore()
const req = computed(() => app.confirmRequest)
</script>

<template>
  <BaseModal :open="!!req" :title="req?.title ?? ''" :width="460" @close="app.settleConfirm(false)">
    <p class="message" data-confirm-message>{{ req?.message }}</p>
    <template #footer>
      <button class="btn" @click="app.settleConfirm(false)">Cancel</button>
      <button class="btn" :class="req?.danger ? 'btn-danger' : 'btn-primary'" autofocus data-confirm-ok @click="app.settleConfirm(true)">
        {{ req?.confirmLabel ?? 'Confirm' }}
      </button>
    </template>
  </BaseModal>
</template>

<style scoped>
.message {
  margin: 0;
  color: var(--text-muted);
  white-space: pre-line;
  overflow-wrap: anywhere;
}
</style>
