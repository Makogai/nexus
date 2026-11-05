// Pure helpers for the Workflow Manager (ADR-0029). The backend owns parsing, validation and the run state machine.
import type { Rigor, RunStep, StepKind, WorkflowDefinition } from './types'

export const RIGOR_LEVELS: { id: Rigor; name: string }[] = [
  { id: 'prototype', name: 'Prototype' },
  { id: 'balanced', name: 'Balanced' },
  { id: 'professional', name: 'Professional' },
  { id: 'critical', name: 'Critical' },
]

export const KIND_LABELS: Record<StepKind, string> = {
  claude: 'Claude',
  check: 'NEXUS check',
  review: 'Your review',
}

export const STATUS_LABELS: Record<RunStep['status'], string> = {
  pending: 'Waiting for earlier steps',
  ready: 'Ready for Claude',
  blocked: 'Blocked',
  verifying: 'Verifying with gates',
  'awaiting-review': 'Waiting for your review',
  passed: 'Done',
  failed: 'Failed',
  skipped: 'Not in this rigor',
  cancelled: 'Cancelled',
}

/** How a step's completion is known — the central distinction of the engine. */
export function verificationLabel(s: Pick<RunStep, 'status' | 'verification'>): { text: string; tone: 'ok' | 'accent' | 'warn' | 'muted' } | null {
  if (s.status === 'skipped') return { text: 'Skipped (rigor)', tone: 'muted' }
  if (s.status !== 'passed') return null
  switch (s.verification) {
    case 'verified':
      return { text: 'Verified by gates', tone: 'ok' }
    case 'human':
      return { text: 'Signed off by you', tone: 'accent' }
    default:
      return { text: 'AI-reported — not verified', tone: 'warn' }
  }
}

export function rigorRank(r: string | null | undefined): number {
  const i = RIGOR_LEVELS.findIndex((x) => x.id === r)
  return i === -1 ? 1 : i
}

/** Steps of a definition that are part of a run at `rigor`. */
export function includedSteps(d: WorkflowDefinition, rigor: Rigor): string[] {
  return d.steps.filter((s) => !s.rigor || rigorRank(s.rigor) <= rigorRank(rigor)).map((s) => s.id)
}

/**
 * Split a command line typed in the gate editor into an argument vector (quotes group, no shell expansion).
 * `npm run "test:unit" --silent` → ['npm', 'run', 'test:unit', '--silent'].
 */
export function splitArgs(line: string): string[] {
  const out: string[] = []
  let cur = ''
  let quote: string | null = null
  let any = false
  for (const ch of line) {
    if (quote) {
      if (ch === quote) quote = null
      else cur += ch
    } else if (ch === '"' || ch === "'") {
      quote = ch
      any = true
    } else if (/\s/.test(ch)) {
      if (cur || any) out.push(cur)
      cur = ''
      any = false
    } else {
      cur += ch
    }
  }
  if (cur || any) out.push(cur)
  return out
}

export function joinArgs(args: string[]): string {
  return args.map((a) => (a === '' || /\s|"/.test(a) ? `"${a.replace(/"/g, "'")}"` : a)).join(' ')
}

/** Plain-language effect of approving a gate on waiting workflow work (shown before the approval is confirmed). */
export function approvalImpactText(i: { steps: number; runs: number; stillWaiting: number }): string {
  const plural = (n: number, one: string, many: string) => `${n} ${n === 1 ? one : many}`
  let text = i.steps > 0
    ? `Approving resumes ${plural(i.steps, 'waiting workflow step', 'waiting workflow steps')} in ${plural(i.runs, 'run', 'runs')} automatically.`
    : 'No waiting workflow step resumes with this approval.'
  if (i.stillWaiting > 0) text += ` ${plural(i.stillWaiting, 'step keeps', 'steps keep')} waiting for other gates.`
  return text
}
