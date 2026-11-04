import type { AttentionStatus, NotificationCategory, NotificationPriority, SessionState, SessionStatus, SessionView } from './types'

export function relativeTime(ms: number | null | undefined, now = Date.now()): string {
  if (!ms) return 'never'
  const s = Math.round((now - ms) / 1000)
  if (s < 0) return 'just now'
  if (s < 45) return 'just now'
  const m = Math.round(s / 60)
  if (m < 60) return `${m} min ago`
  const h = Math.round(m / 60)
  if (h < 24) return `${h} h ago`
  const d = Math.round(h / 24)
  if (d < 14) return d === 1 ? 'yesterday' : `${d} days ago`
  const w = Math.round(d / 7)
  if (d < 60) return `${w} weeks ago`
  return new Date(ms).toLocaleDateString()
}

export function duration(fromMs: number, toMs: number | null | undefined, now = Date.now()): string {
  const total = Math.max(0, Math.round(((toMs ?? now) - fromMs) / 1000))
  const h = Math.floor(total / 3600)
  const m = Math.floor((total % 3600) / 60)
  const s = total % 60
  if (h > 0) return `${h}h ${m}m`
  if (m > 0) return `${m}m ${s}s`
  return `${s}s`
}

/** Shorten long paths for display: keep the drive/root and the last segments. */
export function shortPath(path: string | null | undefined, home?: string, max = 48): string {
  if (!path) return ''
  let p = path
  if (home && p.toLowerCase().startsWith(home.toLowerCase())) p = '~' + p.slice(home.length)
  if (p.length <= max) return p
  const sep = p.includes('\\') ? '\\' : '/'
  const parts = p.split(sep)
  const head = parts[0] ?? ''
  let tail = parts[parts.length - 1] ?? ''
  for (let i = parts.length - 2; i > 0; i--) {
    const next = `${parts[i]}${sep}${tail}`
    if (head.length + next.length + 4 > max) break
    tail = next
  }
  return `${head}${sep}…${sep}${tail}`
}

export const stateLabel: Record<SessionState, string> = {
  starting: 'Starting',
  running: 'Running',
  exited: 'Exited',
  interrupted: 'Interrupted',
  failed: 'Failed',
}

/** One-line human description of where a session stands. */
export function sessionStatus(s: SessionView): string {
  if (s.running) return 'Running'
  if (s.liveElsewhere) return s.host === 'cli' ? 'Running in a terminal (nexus launch)' : `Running (${s.host})`
  switch (s.state) {
    case 'exited':
      return s.exitCode === 0 || s.exitCode === null ? 'Exited' : `Exited (code ${s.exitCode})`
    case 'failed':
      return s.error ? `Failed: ${s.error}` : 'Failed to start'
    case 'interrupted':
      return s.host === 'desktop' ? 'Interrupted (NEXUS closed)' : 'Interrupted (its terminal closed)'
    default:
      return stateLabel[s.state]
  }
}

export function sessionTone(s: SessionView): 'live' | 'ok' | 'warn' | 'error' | 'muted' {
  if (s.running || s.liveElsewhere) return 'live'
  if (s.state === 'failed') return 'error'
  if (s.state === 'interrupted') return 'warn'
  if (s.state === 'exited' && s.exitCode !== 0 && s.exitCode !== null) return 'warn'
  return 'muted'
}

export function plural(n: number, word: string, pluralWord = `${word}s`): string {
  return `${n} ${n === 1 ? word : pluralWord}`
}

export const attentionLabels: Record<AttentionStatus, string> = {
  running: 'Running',
  'waiting-for-input': 'Waiting for input',
  'permission-required': 'Permission required',
  idle: 'Your turn',
  error: 'Stopped on an error',
  completed: 'Completed',
  failed: 'Failed',
  interrupted: 'Interrupted',
  unknown: 'Unknown',
}

/** Live statuses that need the user, most urgent first (for sorting the attention list). */
export const attentionRank: Partial<Record<AttentionStatus, number>> = {
  'permission-required': 0,
  'waiting-for-input': 1,
  error: 2,
  idle: 3,
  running: 4,
  unknown: 5,
}

export function needsUser(st: SessionStatus): boolean {
  return st.status === 'permission-required' || st.status === 'waiting-for-input' || st.status === 'error'
}

export const categoryLabels: Record<NotificationCategory, string> = {
  'input-required': 'Input required',
  'permission-requested': 'Permission requested',
  'agent-finished': 'Claude finished',
  'task-completed': 'Task completed',
  'task-blocked': 'Task blocked',
  'session-finished': 'Session finished',
  'session-interrupted': 'Session interrupted',
  'workflow-failed': 'Workflow failed',
  'process-issue': 'Claude stopped on an error',
  'review-required': 'Review required',
  info: 'Information',
}

export const priorityLabels: Record<NotificationPriority, string> = {
  critical: 'Critical',
  action: 'Action required',
  success: 'Success',
  warning: 'Warning',
  info: 'Information',
}
