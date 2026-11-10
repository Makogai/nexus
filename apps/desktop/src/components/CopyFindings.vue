<script setup lang="ts">
// The security inspection's findings as plain text on the clipboard — for a bug report or a message to Claude (owner
// 2026-11-10: the push review's findings could not be copied). The lists that show them are selectable too.
import { ref } from 'vue'
import { Check, Copy } from '@lucide/vue'
import { findingsText } from '../lib/push'
import type { SyncFinding } from '../lib/types'

const props = defineProps<{ findings: SyncFinding[] }>()
const copied = ref(false)
async function copy() {
  try {
    await navigator.clipboard.writeText(findingsText(props.findings))
    copied.value = true
    setTimeout(() => (copied.value = false), 1500)
  } catch {
    // The clipboard can be unavailable (permissions); the list stays selectable.
  }
}
</script>

<template>
  <button v-if="findings.length" type="button" class="btn btn-ghost btn-sm" data-copy-findings @click="copy">
    <Check v-if="copied" /><Copy v-else /> {{ copied ? 'Copied' : 'Copy findings' }}
  </button>
</template>
