// Presentation helpers for workspace knowledge (pure; unit-tested).
import type { KnowledgeItem, KnowledgeKind, TaskStatus } from './types'

export const TASK_STATUSES: TaskStatus[] = ['backlog', 'ready', 'in-progress', 'review', 'blocked', 'done']

export const STATUS_LABELS: Record<TaskStatus, string> = {
  backlog: 'Backlog',
  ready: 'Ready',
  'in-progress': 'In progress',
  review: 'Review',
  blocked: 'Blocked',
  done: 'Done',
}

export const KIND_LABELS: Record<KnowledgeKind, string> = {
  rule: 'Rules',
  task: 'Tasks',
  decision: 'Decisions',
  note: 'Notes',
  spec: 'Specs',
  workflow: 'Workflows',
}

const PRIORITY_RANK: Record<string, number> = { urgent: 0, high: 1, medium: 2, low: 3 }

/** Tasks grouped into board columns, most urgent first, then most recently updated. */
export function boardColumns(items: KnowledgeItem[]): Record<TaskStatus, KnowledgeItem[]> {
  const cols = Object.fromEntries(TASK_STATUSES.map((s) => [s, [] as KnowledgeItem[]])) as Record<TaskStatus, KnowledgeItem[]>
  for (const t of items) {
    if (t.kind !== 'task' || !t.valid || t.conflict) continue
    const status = (TASK_STATUSES as string[]).includes(t.status ?? '') ? (t.status as TaskStatus) : 'backlog'
    cols[status].push(t)
  }
  for (const s of TASK_STATUSES) {
    cols[s].sort(
      (a, b) =>
        (PRIORITY_RANK[a.priority ?? 'medium'] ?? 2) - (PRIORITY_RANK[b.priority ?? 'medium'] ?? 2) ||
        (b.updated ?? '').localeCompare(a.updated ?? ''),
    )
  }
  return cols
}

/** Files the index could not use: invalid frontmatter or a duplicated id. */
export function problems(items: KnowledgeItem[]): KnowledgeItem[] {
  return items.filter((i) => !i.valid || i.conflict)
}

/** Rules in hierarchy order (global → workspace → stack → project), like the launch. */
export function rulesInOrder(items: KnowledgeItem[]): KnowledgeItem[] {
  const rank: Record<string, number> = { global: 0, workspace: 1, stack: 2, project: 3 }
  return items
    .filter((i) => i.kind === 'rule' && i.valid && !i.conflict)
    .sort((a, b) => (rank[a.scope] ?? 9) - (rank[b.scope] ?? 9) || a.title.localeCompare(b.title))
}

export function fileName(path: string): string {
  return path.split(/[\/]/).pop() ?? path
}
