// Pure helpers for the Constitution Manager (ADR-0027). The backend owns parsing, validation and ids; these only
// edit the structure in the UI and summarise it.
import type { CArticle, CRule, CSection, ConstitutionStructure, Enforcement, ScopeLevel } from './types'

export const ENFORCEMENT_LABELS: Record<Enforcement, string> = {
  mandatory: 'Mandatory policy',
  rule: 'Rule',
  guideline: 'Guideline',
}

export const ENFORCEMENT_HINTS: Record<Enforcement, string> = {
  mandatory: 'Always applies; narrower scopes cannot override or disable it.',
  rule: 'Applies unless a narrower scope overrides it.',
  guideline: 'A preference; Claude reads it on demand.',
}

export const SCOPE_LABELS: Record<ScopeLevel, string> = {
  global: 'Global (this machine)',
  workspace: 'Workspace',
  stack: 'Stack',
  project: 'Project',
  task: 'Task / workflow',
}

export const SCOPE_ORDER: ScopeLevel[] = ['global', 'workspace', 'stack', 'project', 'task']

export function newRule(enforcement: Enforcement = 'rule'): CRule {
  return { id: '', title: 'New rule', enforcement, enabled: true, overrides: null, text: '' }
}

export function newSection(): CSection {
  return { title: 'New section', text: '', rules: [newRule()] }
}

export function newArticle(): CArticle {
  return { title: 'New article', text: '', sections: [{ title: '', text: '', rules: [newRule()] }] }
}

/** Move `list[index]` by `delta` (−1 up, +1 down) in place; returns whether it moved. */
export function move<T>(list: T[], index: number, delta: -1 | 1): boolean {
  const to = index + delta
  if (index < 0 || index >= list.length || to < 0 || to >= list.length) return false
  const [item] = list.splice(index, 1)
  list.splice(to, 0, item as T)
  return true
}

export function allRules(s: ConstitutionStructure): CRule[] {
  return s.articles.flatMap((a) => a.sections.flatMap((x) => x.rules))
}

export function counts(s: ConstitutionStructure): Record<Enforcement, number> & { total: number; disabled: number } {
  const rules = allRules(s)
  return {
    total: rules.length,
    mandatory: rules.filter((r) => r.enforcement === 'mandatory').length,
    rule: rules.filter((r) => r.enforcement === 'rule').length,
    guideline: rules.filter((r) => r.enforcement === 'guideline').length,
    disabled: rules.filter((r) => !r.enabled).length,
  }
}

/** Case-insensitive filter of the tree: rules whose id/title/text match; articles/sections kept if they hold a match. */
export function matches(r: CRule, q: string): boolean {
  const needle = q.trim().toLowerCase()
  return !needle || `${r.id} ${r.title} ${r.text}`.toLowerCase().includes(needle)
}

export function clone<T>(v: T): T {
  return JSON.parse(JSON.stringify(v)) as T
}
