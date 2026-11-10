// "Needs you" (ADR-0048): one click from an inbox item to the place where the person acts on it.
import type { Router } from 'vue-router'
import type { NeedsYouItem } from './types'
import { useAppStore } from '../stores/app'
import { useSessionStore } from '../stores/sessions'

export const NEEDS_YOU_LABELS: Record<NeedsYouItem['kind'], string> = {
  session: 'Session waiting',
  'push-failed': 'Push failed',
  push: 'Push request',
  'workflow-review': 'Sign-off',
  'task-review': 'Task to review',
  draft: 'Draft',
  approval: 'Approval',
  kit: 'AI Kit',
}

/** Most urgent first: Claude is blocked on the person, then failures, then reviews. */
export function isUrgent(i: NeedsYouItem): boolean {
  return i.kind === 'session' || i.kind === 'push-failed'
}

export async function openNeedsYou(i: NeedsYouItem, router: Router): Promise<void> {
  const app = useAppStore()
  const sessions = useSessionStore()
  switch (i.kind) {
    case 'session': {
      const s = sessions.byId[i.id]
      if (s?.running) sessions.openTab(i.id)
      else if (i.projectId) await router.push({ name: 'project', params: { id: i.projectId }, query: { tab: 'sessions' } })
      return
    }
    case 'push':
    case 'push-failed':
      app.pushRequestId = i.id
      return
    case 'workflow-review':
      await router.push({ name: 'workflows', query: { run: i.id } })
      return
    case 'task-review':
      if (i.projectId) await router.push({ name: 'project', params: { id: i.projectId }, query: { tab: 'tasks' } })
      else await router.push({ name: 'knowledge', query: { task: i.id } })
      return
    case 'draft':
      if (i.projectId) await router.push({ name: 'project', params: { id: i.projectId }, query: { tab: 'learn' } })
      return
    case 'approval':
      if (i.docKind === 'workflow') await router.push({ name: 'workflows' })
      else if (i.docKind === 'constitution') await router.push({ name: 'constitutions', query: { pending: '1' } })
      else await router.push({ name: 'knowledge', query: { tab: 'rule' } })
      return
    case 'kit':
      await router.push({ name: 'kits' })
  }
}
