<script setup lang="ts">
// A button for a backend call (ADR-0048, rule NX-7): disabled and `aria-busy` while pending, a spinner in place of its
// icon, and an optional pending label ("Applying…"). Classes and data-* attributes pass through to the <button>.
import { LoaderCircle } from '@lucide/vue'

withDefaults(defineProps<{ pending?: boolean; disabled?: boolean; pendingLabel?: string; type?: 'button' | 'submit' }>(), {
  pending: false,
  disabled: false,
  pendingLabel: undefined,
  type: 'button',
})
defineEmits<{ click: [e: MouseEvent] }>()
</script>

<template>
  <button class="btn" :type="type" :disabled="disabled || pending" :aria-busy="pending ? 'true' : undefined" @click="$emit('click', $event)">
    <LoaderCircle v-if="pending" class="spin" aria-hidden="true" />
    <slot v-else name="icon" />
    <template v-if="pending && pendingLabel">{{ pendingLabel }}</template>
    <slot v-else />
  </button>
</template>
