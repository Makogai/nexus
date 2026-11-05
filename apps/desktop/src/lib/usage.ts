// Usage analytics helpers (ADR-0030): formatting, periods, day buckets and the series of the trend chart.
import type { UsageCost, UsageDay, UsageDimension, UsageStatus, UsageTokens } from './types'

export const PERIODS = [
  { id: '7d', label: 'Last 7 days', days: 7 },
  { id: '30d', label: 'Last 30 days', days: 30 },
  { id: '90d', label: 'Last 90 days', days: 90 },
  { id: 'all', label: 'All time', days: 0 },
] as const
export type PeriodId = (typeof PERIODS)[number]['id']

export const DIMENSIONS: { id: UsageDimension; label: string }[] = [
  { id: 'project', label: 'Project' },
  { id: 'profile', label: 'Profile' },
  { id: 'task', label: 'Task' },
  { id: 'workflow', label: 'Workflow run' },
  { id: 'strategy', label: 'Context strategy' },
  { id: 'work-profile', label: 'Work profile' },
  { id: 'model', label: 'Model' },
]

/** The stacked series of the trend chart, in fixed categorical order (colour follows the series, never its rank). */
export const SERIES = [
  { key: 'input', label: 'Input', color: 'var(--series-1)' },
  { key: 'cacheWrite', label: 'Cache write', color: 'var(--series-2)' },
  { key: 'cacheRead', label: 'Cache read', color: 'var(--series-3)' },
  { key: 'output', label: 'Output', color: 'var(--series-4)' },
] as const
export type SeriesKey = (typeof SERIES)[number]['key']

export const STATUS: Record<UsageStatus, { label: string; tone: 'ok' | 'muted' | 'warn' | 'error'; kind: 'measured' | 'none' | 'unavailable' }> = {
  measured: { label: 'Parsed', tone: 'ok', kind: 'measured' },
  'no-requests': { label: 'No API requests', tone: 'muted', kind: 'none' },
  pending: { label: 'Pending', tone: 'muted', kind: 'none' },
  missing: { label: 'Unavailable', tone: 'warn', kind: 'unavailable' },
  unreadable: { label: 'Unavailable', tone: 'error', kind: 'unavailable' },
}

export const totalTokens = (t: UsageTokens) => t.input + t.output + t.cacheWrite + t.cacheRead

/** 1,234 → "1.2k", 3_400_000 → "3.4M". */
export function compact(n: number | null | undefined): string {
  if (n == null || !Number.isFinite(n)) return '–'
  const a = Math.abs(n)
  const fmt = (v: number, s: string) => `${v >= 100 ? Math.round(v) : v >= 10 ? v.toFixed(1).replace(/\.0$/, '') : v.toFixed(2).replace(/\.?0+$/, '')}${s}`
  if (a >= 1e9) return fmt(n / 1e9, 'B')
  if (a >= 1e6) return fmt(n / 1e6, 'M')
  if (a >= 1e3) return fmt(n / 1e3, 'k')
  return String(Math.round(n))
}

export const grouped = (n: number) => Math.round(n).toLocaleString('en-US')

export function percent(v: number | null | undefined): string {
  return v == null ? '–' : `${Math.round(v * 100)}%`
}

/** Claude Code's own cost estimate, labelled; `null` → why it is not shown. */
export function costText(c: UsageCost): string {
  if (c.usd == null) return c.withheldSubscription > 0 ? 'not shown (subscription)' : c.withheldUnknown > 0 ? 'billing unknown' : 'unavailable'
  const v = c.usd < 0.01 && c.usd > 0 ? '< $0.01' : `$${c.usd.toFixed(2)}`
  return `≈ ${v}${c.incomplete ? ' (incomplete)' : ''}`
}

function localDay(ms: number): string {
  const d = new Date(ms)
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`
}

function addDays(day: string, n: number): string {
  const [y, m, d] = day.split('-').map(Number)
  return localDay(new Date(y!, m! - 1, d! + n, 12).getTime())
}

const EMPTY: UsageTokens = { requests: 0, input: 0, output: 0, cacheWrite: 0, cacheRead: 0, webSearches: 0, subagentRequests: 0 }

export interface Bucket {
  /** First day of the bucket (`YYYY-MM-DD`). */
  day: string
  label: string
  /** Days in the bucket (1, or 7 for weekly buckets of long periods). */
  days: number
  tokens: UsageTokens
}

function add(a: UsageTokens, b: UsageTokens): UsageTokens {
  return {
    requests: a.requests + b.requests,
    input: a.input + b.input,
    output: a.output + b.output,
    cacheWrite: a.cacheWrite + b.cacheWrite,
    cacheRead: a.cacheRead + b.cacheRead,
    webSearches: a.webSearches + b.webSearches,
    subagentRequests: a.subagentRequests + b.subagentRequests,
  }
}

const SHORT = new Intl.DateTimeFormat('en-GB', { day: 'numeric', month: 'short' })
const label = (day: string) => {
  const [y, m, d] = day.split('-').map(Number)
  return SHORT.format(new Date(y!, m! - 1, d!, 12))
}

/** Contiguous day buckets for the period (missing days are zero); periods over 120 days use weekly buckets. */
export function buckets(daily: UsageDay[], days: number, now = Date.now()): Bucket[] {
  const today = localDay(now)
  const first = days > 0 ? addDays(today, -(days - 1)) : (daily[0]?.day ?? today)
  const byDay = new Map(daily.map((d) => [d.day, d.tokens]))
  const all: Bucket[] = []
  for (let day = first, i = 0; day <= today && i < 3700; day = addDays(day, 1), i++) {
    all.push({ day, label: label(day), days: 1, tokens: byDay.get(day) ?? EMPTY })
  }
  if (all.length <= 120) return all
  const weeks: Bucket[] = []
  for (let i = 0; i < all.length; i += 7) {
    const slice = all.slice(i, i + 7)
    weeks.push({ day: slice[0]!.day, label: `week of ${slice[0]!.label}`, days: slice.length, tokens: slice.reduce((acc, b) => add(acc, b.tokens), EMPTY) })
  }
  return weeks
}

/** A round axis maximum ≥ v (1, 2, 2.5, 5 × 10ⁿ). */
export function niceMax(v: number): number {
  if (v <= 0) return 1
  const p = 10 ** Math.floor(Math.log10(v))
  for (const m of [1, 2, 2.5, 5, 10]) if (m * p >= v) return m * p
  return 10 * p
}
