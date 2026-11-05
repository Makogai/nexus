<script setup lang="ts">
// Stacked daily (or weekly) token bars by token type — measured values only (ADR-0030).
// Marks: 2px surface gap between segments and bars, 4px rounded data end, recessive grid; legend above; one tooltip
// per column (hover and keyboard focus) listing every series; a table view carries the same numbers.
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'
import { SERIES, compact, grouped, niceMax, totalTokens, type Bucket } from '../lib/usage'

const props = defineProps<{ buckets: Bucket[]; dimmed?: boolean }>()

const wrap = ref<HTMLElement | null>(null)
const width = ref(720)
let observer: ResizeObserver | null = null
onMounted(() => {
  if (wrap.value && typeof ResizeObserver !== 'undefined') {
    observer = new ResizeObserver((entries) => {
      const w = entries[0]?.contentRect.width
      if (w) width.value = w
    })
    observer.observe(wrap.value)
  }
})
onBeforeUnmount(() => observer?.disconnect())

const showTable = ref(false)
const hovered = ref<number | null>(null)

const H = 230
const PAD = { l: 48, r: 10, t: 12, b: 28 }
const plotW = computed(() => Math.max(120, width.value - PAD.l - PAD.r))
const plotH = H - PAD.t - PAD.b
const max = computed(() => niceMax(Math.max(0, ...props.buckets.map((b) => totalTokens(b.tokens)))))
const step = computed(() => plotW.value / Math.max(1, props.buckets.length))
const barW = computed(() => Math.max(2, Math.min(32, step.value - 2)))
const y = (v: number) => PAD.t + plotH - (v / max.value) * plotH
const empty = computed(() => props.buckets.every((b) => totalTokens(b.tokens) === 0))

interface Seg {
  key: string
  color: string
  x: number
  y: number
  w: number
  h: number
  top: boolean
}
const columns = computed(() =>
  props.buckets.map((b, i) => {
    const x = PAD.l + i * step.value + (step.value - barW.value) / 2
    const raw: Seg[] = []
    let acc = 0
    for (const s of SERIES) {
      const v = b.tokens[s.key]
      if (v <= 0) continue
      const y0 = y(acc)
      acc += v
      const y1 = y(acc)
      raw.push({ key: s.key, color: s.color, x, y: y1, w: barW.value, h: y0 - y1, top: false })
    }
    if (raw.length) raw[raw.length - 1]!.top = true
    // 2px surface gap above every segment that has another on top of it.
    const segs = raw.map((s) => (s.top || s.h <= 3 ? s : { ...s, y: s.y + 2, h: s.h - 2 }))
    return { b, i, x, segs, total: acc }
  }),
)

/** Bar with a 4px rounded data end (top) and a square baseline end. */
function topPath(s: Seg): string {
  const r = Math.min(4, s.w / 2, s.h)
  return `M${s.x},${s.y + s.h}V${s.y + r}Q${s.x},${s.y} ${s.x + r},${s.y}H${s.x + s.w - r}Q${s.x + s.w},${s.y} ${s.x + s.w},${s.y + r}V${s.y + s.h}Z`
}

const ticks = computed(() => [0, max.value / 2, max.value].map((v) => ({ v, y: y(v), label: compact(v) })))
const xLabels = computed(() => {
  const n = props.buckets.length
  if (!n) return []
  const every = Math.max(1, Math.ceil(n / Math.max(2, Math.floor(plotW.value / 70))))
  return columns.value.filter((c) => c.i % every === 0 || c.i === n - 1).map((c) => ({ x: c.x + barW.value / 2, label: c.b.label, i: c.i }))
})

const tip = computed(() => {
  if (hovered.value == null) return null
  const c = columns.value[hovered.value]
  if (!c) return null
  const left = Math.min(Math.max(c.x + barW.value / 2, 90), width.value - 90)
  return { c, left, rows: SERIES.map((s) => ({ ...s, value: c.b.tokens[s.key] })).reverse() }
})

function aria(c: (typeof columns.value)[number]): string {
  return `${c.b.label}: ${grouped(c.total)} tokens — ${SERIES.map((s) => `${s.label} ${grouped(c.b.tokens[s.key])}`).join(', ')}; ${c.b.tokens.requests} requests`
}
</script>

<template>
  <div class="trend" :class="{ dimmed }" data-usage-trend>
    <div class="head">
      <ul class="legend" aria-label="Token types">
        <li v-for="s in SERIES" :key="s.key"><span class="swatch" :style="{ background: s.color }" />{{ s.label }}</li>
      </ul>
      <button class="btn btn-sm btn-ghost" :aria-pressed="showTable" data-usage-table-toggle @click="showTable = !showTable">{{ showTable ? 'Chart' : 'Table' }}</button>
    </div>

    <div v-show="!showTable" ref="wrap" class="plot">
      <p v-if="empty" class="empty-note">No measured usage in this period.</p>
      <svg :width="width" :height="H" role="list" aria-label="Measured tokens per day" @pointerleave="hovered = null">
        <g class="grid">
          <line v-for="t in ticks" :key="t.v" :x1="PAD.l" :x2="PAD.l + plotW" :y1="t.y" :y2="t.y" />
          <text v-for="t in ticks" :key="`l${t.v}`" :x="PAD.l - 8" :y="t.y + 4" text-anchor="end">{{ t.label }}</text>
        </g>
        <g
          v-for="c in columns"
          :key="c.b.day"
          role="listitem"
          tabindex="0"
          class="col"
          :class="{ faded: hovered != null && hovered !== c.i }"
          :aria-label="aria(c)"
          :data-bucket="c.b.day"
          :data-total="c.total"
          @pointerenter="hovered = c.i"
          @focus="hovered = c.i"
          @blur="hovered = null"
        >
          <rect class="hit" :x="PAD.l + c.i * step" :y="PAD.t" :width="step" :height="plotH" />
          <template v-for="s in c.segs" :key="s.key">
            <path v-if="s.top" :d="topPath(s)" :fill="s.color" />
            <rect v-else :x="s.x" :y="s.y" :width="s.w" :height="s.h" :fill="s.color" />
          </template>
        </g>
        <g class="axis">
          <line :x1="PAD.l" :x2="PAD.l + plotW" :y1="PAD.t + plotH" :y2="PAD.t + plotH" />
          <text v-for="l in xLabels" :key="l.i" :x="l.x" :y="H - 8" text-anchor="middle">{{ l.label }}</text>
        </g>
      </svg>
      <div v-if="tip" class="tooltip" role="status" :style="{ left: `${tip.left}px` }" data-usage-tooltip>
        <div class="tip-title">{{ tip.c.b.label }}</div>
        <div class="tip-total"><strong>{{ grouped(tip.c.total) }}</strong> tokens · {{ grouped(tip.c.b.tokens.requests) }} requests</div>
        <div v-for="r in tip.rows" :key="r.key" class="tip-row">
          <span class="key" :style="{ background: r.color }" /><strong>{{ grouped(r.value) }}</strong><span class="muted">{{ r.label }}</span>
        </div>
      </div>
    </div>

    <table v-if="showTable" class="table" data-usage-trend-table>
      <thead>
        <tr><th>{{ buckets[0]?.days === 7 ? 'Week' : 'Day' }}</th><th>Requests</th><th v-for="s in SERIES" :key="s.key">{{ s.label }}</th><th>Total</th></tr>
      </thead>
      <tbody>
        <tr v-for="b in buckets" :key="b.day">
          <td>{{ b.label }}</td>
          <td class="num">{{ grouped(b.tokens.requests) }}</td>
          <td v-for="s in SERIES" :key="s.key" class="num">{{ grouped(b.tokens[s.key]) }}</td>
          <td class="num"><strong>{{ grouped(totalTokens(b.tokens)) }}</strong></td>
        </tr>
      </tbody>
    </table>
  </div>
</template>

<style scoped>
.trend {
  transition: opacity 0.15s;
}
.trend.dimmed {
  opacity: 0.55;
}
.head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  margin-bottom: 6px;
}
.legend {
  display: flex;
  flex-wrap: wrap;
  gap: 14px;
  margin: 0;
  padding: 0;
  list-style: none;
  font-size: 12px;
  color: var(--text-muted);
}
.legend li {
  display: flex;
  align-items: center;
  gap: 6px;
}
.swatch {
  width: 10px;
  height: 10px;
  border-radius: 2px;
}
.plot {
  position: relative;
  width: 100%;
}
svg {
  display: block;
  overflow: visible;
}
.grid line {
  stroke: var(--border);
  stroke-width: 1;
}
.grid text,
.axis text {
  fill: var(--text-faint);
  font-size: 11px;
  font-variant-numeric: tabular-nums;
}
.axis line {
  stroke: var(--border-strong);
}
.col {
  outline: none;
  transition: opacity 0.12s;
}
.col.faded {
  opacity: 0.45;
}
.col:focus-visible .hit {
  stroke: var(--accent-ring);
  stroke-width: 2;
}
.hit {
  fill: transparent;
}
.empty-note {
  position: absolute;
  inset: 40% 0 auto;
  margin: 0;
  text-align: center;
  color: var(--text-faint);
  font-size: 13px;
  pointer-events: none;
}
.tooltip {
  position: absolute;
  top: 0;
  transform: translateX(-50%);
  min-width: 170px;
  padding: 8px 10px;
  border-radius: var(--radius-sm);
  border: 1px solid var(--border-strong);
  background: var(--bg-raised);
  box-shadow: var(--shadow-sm);
  font-size: 12px;
  pointer-events: none;
  z-index: 2;
}
.tip-title {
  color: var(--text-muted);
  margin-bottom: 2px;
}
.tip-total {
  margin-bottom: 4px;
  color: var(--text-muted);
}
.tip-total strong,
.tip-row strong {
  color: var(--text);
  font-variant-numeric: tabular-nums;
}
.tip-row {
  display: flex;
  align-items: center;
  gap: 6px;
}
.key {
  width: 12px;
  height: 2px;
  border-radius: 1px;
}
.muted {
  color: var(--text-muted);
}
.table {
  width: 100%;
  border-collapse: collapse;
  font-size: 12px;
}
.table th {
  text-align: left;
  color: var(--text-faint);
  font-weight: 500;
  padding: 4px 6px;
}
.table td {
  padding: 4px 6px;
  border-top: 1px solid var(--border);
}
.num {
  text-align: right;
  font-variant-numeric: tabular-nums;
}
</style>
